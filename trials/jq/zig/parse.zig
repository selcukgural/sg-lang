//! The filter read into a tree. Names are resolved as they are read: a variable becomes how
//! many frames of the environment lie above the one holding it, a call the function's
//! definition and the frames above the environment it was written in, a filter argument the
//! frame of its call and its place there. The built-in functions that jq writes in its own
//! language are built here from the nodes of the tree (`map(f)` is `[.[] | f]`); the others are
//! done by the machine or by `builtin.zig`.
//!
//! A node is marked `simple` when it gives exactly one output (or an error) and calls nothing:
//! the machine works it out at once, without frames.

const std = @import("std");
const value = @import("value.zig");
const Value = value.Value;
const gpa = value.gpa;
const oom = value.oom;

pub const Op = enum { add, sub, mul, div, mod, eq, ne, lt, le, gt, ge };

pub const Format = enum { text, json, csv, tsv, base64, base64d, uri };

/// The built-in functions done by `builtin.zig` on values.
pub const Native = enum {
    not_,
    length,
    utf8bytelength,
    keys,
    keys_unsorted,
    has,
    in_,
    to_entries,
    from_entries,
    add,
    any,
    all,
    sort,
    sort_by,
    group_by,
    unique,
    unique_by,
    min,
    max,
    min_by,
    max_by,
    reverse,
    tostring,
    tonumber,
    tojson,
    fromjson,
    type_,
    split,
    join,
    test_,
    ascii_downcase,
    ascii_upcase,
    startswith,
    endswith,
    ltrimstr,
    rtrimstr,
    floor,
    sqrt,
    setpath,
    delpaths,
    error_,
    input,
    format,
};

pub const Binary = struct { a: *Node, b: *Node };

pub const Def = struct {
    name: []const u8,
    /// Its filter arguments; a `$name` argument is one too, bound to `$name` in the body.
    params: usize,
    body: *Node = undefined,
};

pub const Kind = union(enum) {
    identity,
    /// `..`
    recurse_all,
    empty,
    literal: Value,
    /// The frames above the one holding the variable.
    variable: u32,
    /// `a[b]`; `b` is worked out first, with the same input as `a`. `opt` (`a[b]?`): an error of
    /// the indexing itself gives no output, as jq's `INDEX_OPT`; one of `a` or `b` still raises.
    index: struct { a: *Node, b: *Node, opt: bool = false },
    /// `t[from:to]`
    slice: struct { target: *Node, from: ?*Node, to: ?*Node, opt: bool = false },
    /// `t[]`; `opt` (`t[]?`): what cannot be iterated gives no output.
    iterate: struct { target: *Node, opt: bool = false },
    /// `t?`
    try_: *Node,
    negate: *Node,
    pipe: Binary,
    comma: Binary,
    alternative: Binary,
    and_: Binary,
    or_: Binary,
    /// The right side is worked out first, the outer loop.
    arith: struct { op: Op, a: *Node, b: *Node },
    assign: struct { how: Assign, lhs: *Node, rhs: *Node },
    if_: struct { cond: *Node, then: *Node, otherwise: *Node },
    reduce: struct { source: *Node, init: *Node, update: *Node },
    foreach: struct { source: *Node, init: *Node, update: *Node, extract: ?*Node },
    /// `a as $x | b`
    bind: Binary,
    call: struct { def: *Def, up: u32, args: []*Node },
    param: struct { up: u32, index: u32 },
    array: ?*Node,
    /// `[.[] | f]` with `f` simple, worked out at once.
    map_simple: *Node,
    object: []Entry,
    text: struct { parts: []Part, format: ?Format },
    /// A built-in function on values; its arguments, each a value, are worked out from the
    /// last (the outer loop) to the first, as jq does for the functions it writes in C.
    native: struct { id: Native, args: []*Node, format: Format = .text },
    /// `range`; its arguments from the first (the outer loop) to the last, as jq's `$`
    /// arguments are.
    range: []*Node,
    getpath: *Node,
    path_of: *Node,
    paths,
    select: *Node,
    /// `values`, `nulls`, `numbers` and the like: the input when its kind is in the set.
    type_select: u8,
    recurse: *Node,
    limit: Binary,
    first: *Node,
    any_all: struct { gen: *Node, all: bool },
    delete: *Node,
    inputs,
};

pub const Assign = union(enum) { set, update, arith: Op, alternative, delete };

pub const Entry = struct { key: *Node, value: *Node };

pub const Part = union(enum) { literal: []const u8, expr: *Node };

pub const Node = struct {
    simple: bool = false,
    kind: Kind,
};

/// A name the filter can see; variables and filter arguments are frames of the environment
/// at run time, definitions are not.
const Scope = union(enum) {
    variable: []const u8,
    params: [][]const u8,
    def: *Def,
    /// The names given with `--arg` and `--argjson`: frames like variables.
    global: []const u8,
};

pub const Error = error{Compile};

const Tag = enum {
    end,
    dot,
    dotdot,
    field,
    ident,
    variable,
    number,
    string,
    format,
    lparen,
    rparen,
    lbracket,
    rbracket,
    lbrace,
    rbrace,
    pipe,
    comma,
    colon,
    semicolon,
    question,
    alt,
    assign,
    update,
    arith_assign,
    alt_assign,
    op,
    minus,
};

const Token = struct {
    tag: Tag,
    text: []const u8 = "",
    op: Op = .add,
};

pub const Parser = struct {
    src: []const u8,
    pos: usize = 0,
    peeked: ?Token = null,
    scope: std.ArrayList(Scope) = .empty,
    message: []const u8 = "",
    /// `as` and `def` may begin here: not in the value of an object's member.
    allow_bind: bool = true,

    pub fn init(src: []const u8, globals: []const []const u8) Parser {
        var p: Parser = .{ .src = src };
        for (globals) |name| p.scope.append(gpa, .{ .global = name }) catch oom();
        return p;
    }

    fn fail(p: *Parser, comptime format: []const u8, args: anytype) Error {
        p.message = std.fmt.allocPrint(gpa, format, args) catch oom();
        return error.Compile;
    }

    // -----------------------------------------------------------------------------------------
    // Tokens

    fn isIdentStart(c: u8) bool {
        return std.ascii.isAlphabetic(c) or c == '_';
    }

    fn isIdentChar(c: u8) bool {
        return std.ascii.isAlphanumeric(c) or c == '_';
    }

    fn skipSpace(p: *Parser) void {
        while (p.pos < p.src.len) {
            const c = p.src[p.pos];
            if (c == ' ' or c == '\t' or c == '\n' or c == '\r') {
                p.pos += 1;
            } else if (c == '#') {
                while (p.pos < p.src.len and p.src[p.pos] != '\n') p.pos += 1;
            } else break;
        }
    }

    fn ident(p: *Parser) []const u8 {
        const start = p.pos;
        while (p.pos < p.src.len and (isIdentChar(p.src[p.pos]) or
            (p.src[p.pos] == ':' and p.pos + 2 < p.src.len and p.src[p.pos + 1] == ':' and isIdentStart(p.src[p.pos + 2]))))
        {
            p.pos += if (p.src[p.pos] == ':') 2 else 1;
        }
        return p.src[start..p.pos];
    }

    fn lex(p: *Parser) Error!Token {
        p.skipSpace();
        if (p.pos >= p.src.len) return .{ .tag = .end };
        const c = p.src[p.pos];
        const next: u8 = if (p.pos + 1 < p.src.len) p.src[p.pos + 1] else 0;
        const after: u8 = if (p.pos + 2 < p.src.len) p.src[p.pos + 2] else 0;
        if (c == '.') {
            if (next == '.') {
                p.pos += 2;
                return .{ .tag = .dotdot };
            }
            if (isIdentStart(next)) {
                p.pos += 1;
                return .{ .tag = .field, .text = p.ident() };
            }
            if (std.ascii.isDigit(next)) return p.lexNumber();
            p.pos += 1;
            return .{ .tag = .dot };
        }
        if (std.ascii.isDigit(c)) return p.lexNumber();
        if (isIdentStart(c)) return .{ .tag = .ident, .text = p.ident() };
        if (c == '$') {
            p.pos += 1;
            p.skipSpace();
            if (p.pos >= p.src.len or !isIdentStart(p.src[p.pos])) return p.fail("syntax error: a name must follow $", .{});
            return .{ .tag = .variable, .text = p.ident() };
        }
        if (c == '@') {
            p.pos += 1;
            return .{ .tag = .format, .text = p.ident() };
        }
        if (c == '"') {
            p.pos += 1;
            return .{ .tag = .string };
        }
        p.pos += 1;
        switch (c) {
            '(' => return .{ .tag = .lparen },
            ')' => return .{ .tag = .rparen },
            '[' => return .{ .tag = .lbracket },
            ']' => return .{ .tag = .rbracket },
            '{' => return .{ .tag = .lbrace },
            '}' => return .{ .tag = .rbrace },
            ',' => return .{ .tag = .comma },
            ':' => return .{ .tag = .colon },
            ';' => return .{ .tag = .semicolon },
            '?' => {
                if (next == '/' and after == '/') return p.fail("syntax error: ?// is not supported", .{});
                return .{ .tag = .question };
            },
            '|' => {
                if (next == '=') {
                    p.pos += 1;
                    return .{ .tag = .update };
                }
                return .{ .tag = .pipe };
            },
            '/' => {
                if (next == '/') {
                    p.pos += 1;
                    if (after == '=') {
                        p.pos += 1;
                        return .{ .tag = .alt_assign };
                    }
                    return .{ .tag = .alt };
                }
                return p.arithToken(.div, next);
            },
            '=' => {
                if (next == '=') {
                    p.pos += 1;
                    return .{ .tag = .op, .op = .eq };
                }
                return .{ .tag = .assign };
            },
            '!' => {
                if (next == '=') {
                    p.pos += 1;
                    return .{ .tag = .op, .op = .ne };
                }
            },
            '<' => {
                if (next == '=') {
                    p.pos += 1;
                    return .{ .tag = .op, .op = .le };
                }
                return .{ .tag = .op, .op = .lt };
            },
            '>' => {
                if (next == '=') {
                    p.pos += 1;
                    return .{ .tag = .op, .op = .ge };
                }
                return .{ .tag = .op, .op = .gt };
            },
            '+' => return p.arithToken(.add, next),
            '-' => {
                if (next == '=') {
                    p.pos += 1;
                    return .{ .tag = .arith_assign, .op = .sub };
                }
                return .{ .tag = .minus, .op = .sub };
            },
            '*' => return p.arithToken(.mul, next),
            '%' => return p.arithToken(.mod, next),
            else => {},
        }
        return p.fail("syntax error: unexpected character '{c}'", .{c});
    }

    fn arithToken(p: *Parser, op: Op, next: u8) Token {
        if (next == '=') {
            p.pos += 1;
            return .{ .tag = .arith_assign, .op = op };
        }
        return .{ .tag = .op, .op = op };
    }

    fn lexNumber(p: *Parser) Token {
        const start = p.pos;
        while (p.pos < p.src.len and std.ascii.isDigit(p.src[p.pos])) p.pos += 1;
        if (p.pos < p.src.len and p.src[p.pos] == '.') {
            p.pos += 1;
            while (p.pos < p.src.len and std.ascii.isDigit(p.src[p.pos])) p.pos += 1;
        }
        if (p.pos < p.src.len and (p.src[p.pos] == 'e' or p.src[p.pos] == 'E')) {
            var at = p.pos + 1;
            if (at < p.src.len and (p.src[at] == '+' or p.src[at] == '-')) at += 1;
            if (at < p.src.len and std.ascii.isDigit(p.src[at])) {
                while (at < p.src.len and std.ascii.isDigit(p.src[at])) at += 1;
                p.pos = at;
            }
        }
        return .{ .tag = .number, .text = p.src[start..p.pos] };
    }

    fn peek(p: *Parser) Error!Token {
        if (p.peeked == null) p.peeked = try p.lex();
        return p.peeked.?;
    }

    fn take(p: *Parser) Error!Token {
        const t = try p.peek();
        p.peeked = null;
        return t;
    }

    fn expect(p: *Parser, tag: Tag, what: []const u8) Error!void {
        const t = try p.take();
        if (t.tag != tag) return p.fail("syntax error: expected {s}", .{what});
    }

    fn isWord(t: Token, word: []const u8) bool {
        return t.tag == .ident and std.mem.eql(u8, t.text, word);
    }

    fn expectWord(p: *Parser, word: []const u8) Error!void {
        const t = try p.take();
        if (!isWord(t, word)) return p.fail("syntax error: expected {s}", .{word});
    }

    // -----------------------------------------------------------------------------------------
    // Grammar, loosest first: `|`, `,`, `//`, the assignments, `or`, `and`, the comparisons,
    // `+ -`, `* / %`, unary `-`, postfix.

    pub fn parseProgram(p: *Parser) Error!*Node {
        const n = try p.parsePipe();
        if ((try p.peek()).tag != .end) return p.fail("syntax error: unexpected token", .{});
        return n;
    }

    fn parsePipe(p: *Parser) Error!*Node {
        const allow = p.allow_bind;
        p.allow_bind = true;
        defer p.allow_bind = allow;
        const lhs = try p.parseComma();
        if ((try p.peek()).tag == .pipe) {
            _ = try p.take();
            return pipe(lhs, try p.parsePipe());
        }
        return lhs;
    }

    fn parseComma(p: *Parser) Error!*Node {
        var lhs = try p.parseAlt();
        while ((try p.peek()).tag == .comma) {
            _ = try p.take();
            lhs = new(.{ .comma = .{ .a = lhs, .b = try p.parseAlt() } });
        }
        return lhs;
    }

    fn parseAlt(p: *Parser) Error!*Node {
        const lhs = try p.parseAssign();
        if ((try p.peek()).tag == .alt) {
            _ = try p.take();
            return new(.{ .alternative = .{ .a = lhs, .b = try p.parseAlt() } });
        }
        return lhs;
    }

    fn parseAssign(p: *Parser) Error!*Node {
        const lhs = try p.parseOr();
        const t = try p.peek();
        const how: Assign = switch (t.tag) {
            .assign => .set,
            .update => .update,
            .arith_assign => .{ .arith = t.op },
            .alt_assign => .alternative,
            else => return lhs,
        };
        _ = try p.take();
        const rhs = try p.parseAlt();
        return new(.{ .assign = .{ .how = how, .lhs = lhs, .rhs = rhs } });
    }

    fn parseOr(p: *Parser) Error!*Node {
        var lhs = try p.parseAnd();
        while (isWord(try p.peek(), "or")) {
            _ = try p.take();
            lhs = new(.{ .or_ = .{ .a = lhs, .b = try p.parseAnd() } });
        }
        return lhs;
    }

    fn parseAnd(p: *Parser) Error!*Node {
        var lhs = try p.parseCompare();
        while (isWord(try p.peek(), "and")) {
            _ = try p.take();
            lhs = new(.{ .and_ = .{ .a = lhs, .b = try p.parseCompare() } });
        }
        return lhs;
    }

    fn isComparison(t: Token) bool {
        return t.tag == .op and switch (t.op) {
            .eq, .ne, .lt, .le, .gt, .ge => true,
            else => false,
        };
    }

    fn parseCompare(p: *Parser) Error!*Node {
        const lhs = try p.parseAdditive();
        const t = try p.peek();
        if (!isComparison(t)) return lhs;
        _ = try p.take();
        const rhs = try p.parseAdditive();
        if (isComparison(try p.peek())) return p.fail("syntax error: comparisons do not chain", .{});
        return new(.{ .arith = .{ .op = t.op, .a = lhs, .b = rhs } });
    }

    fn parseAdditive(p: *Parser) Error!*Node {
        var lhs = try p.parseMultiplicative();
        while (true) {
            const t = try p.peek();
            const op: Op = if (t.tag == .minus) .sub else if (t.tag == .op and t.op == .add) .add else break;
            _ = try p.take();
            lhs = new(.{ .arith = .{ .op = op, .a = lhs, .b = try p.parseMultiplicative() } });
        }
        return lhs;
    }

    fn parseMultiplicative(p: *Parser) Error!*Node {
        var lhs = try p.parseUnary();
        while (true) {
            const t = try p.peek();
            if (!(t.tag == .op and (t.op == .mul or t.op == .div or t.op == .mod))) break;
            _ = try p.take();
            lhs = new(.{ .arith = .{ .op = t.op, .a = lhs, .b = try p.parseUnary() } });
        }
        return lhs;
    }

    fn parseUnary(p: *Parser) Error!*Node {
        if ((try p.peek()).tag == .minus) {
            _ = try p.take();
            // `-` takes what binds tighter than `+`: `-a * b` is `-(a * b)`.
            return new(.{ .negate = try p.parseMultiplicative() });
        }
        return p.parsePostfix(true);
    }

    /// A term and what follows it: `.name`, `[...]`, `?`, and `as $x | body` when `bind` is set.
    fn parsePostfix(p: *Parser, bind: bool) Error!*Node {
        const first = try p.peek();
        var t = try p.parsePrimary();
        // Whether `t` was made by an indexing just now (`.a`, `."a"`, `t[...]`): a `?` after it
        // makes only that indexing optional, as jq's grammar does; after anything else it is a
        // `try`.
        var indexed = first.tag == .field or (first.tag == .dot and t.kind == .index);
        while (true) {
            const tok = try p.peek();
            switch (tok.tag) {
                .field => {
                    _ = try p.take();
                    t = new(.{ .index = .{ .a = t, .b = textLiteral(tok.text) } });
                    indexed = true;
                },
                .dot => {
                    _ = try p.take();
                    const after = try p.peek();
                    if (after.tag == .string) {
                        _ = try p.take();
                        t = new(.{ .index = .{ .a = t, .b = try p.parseString(null) } });
                    } else if (after.tag == .lbracket) {
                        _ = try p.take();
                        t = try p.parseBracket(t);
                    } else return p.fail("syntax error: unexpected '.'", .{});
                    indexed = true;
                },
                .lbracket => {
                    _ = try p.take();
                    t = try p.parseBracket(t);
                    indexed = true;
                },
                .question => {
                    _ = try p.take();
                    if (indexed) {
                        switch (t.kind) {
                            .index => |*x| x.opt = true,
                            .slice => |*x| x.opt = true,
                            .iterate => |*x| x.opt = true,
                            else => unreachable,
                        }
                        t.simple = isSimple(t);
                    } else {
                        t = new(.{ .try_ = t });
                    }
                    indexed = false;
                },
                .ident => {
                    if (!bind or !p.allow_bind or !isWord(tok, "as")) return t;
                    _ = try p.take();
                    const v = try p.take();
                    if (v.tag != .variable) return p.fail("syntax error: only `as $name` is supported", .{});
                    try p.expect(.pipe, "|");
                    p.scope.append(gpa, .{ .variable = v.text }) catch oom();
                    const body = try p.parsePipe();
                    _ = p.scope.pop();
                    return new(.{ .bind = .{ .a = t, .b = body } });
                },
                else => return t,
            }
        }
    }

    /// After `t[`: `t[]`, `t[e]`, `t[e:]`, `t[:e]`, `t[e:e]`.
    fn parseBracket(p: *Parser, t: *Node) Error!*Node {
        if ((try p.peek()).tag == .rbracket) {
            _ = try p.take();
            return new(.{ .iterate = .{ .target = t } });
        }
        if ((try p.peek()).tag == .colon) {
            _ = try p.take();
            const to = try p.parsePipe();
            try p.expect(.rbracket, "]");
            return new(.{ .slice = .{ .target = t, .from = null, .to = to } });
        }
        const e = try p.parsePipe();
        if ((try p.peek()).tag == .colon) {
            _ = try p.take();
            var to: ?*Node = null;
            if ((try p.peek()).tag != .rbracket) to = try p.parsePipe();
            try p.expect(.rbracket, "]");
            return new(.{ .slice = .{ .target = t, .from = e, .to = to } });
        }
        try p.expect(.rbracket, "]");
        return new(.{ .index = .{ .a = t, .b = e } });
    }

    fn parsePrimary(p: *Parser) Error!*Node {
        const t = try p.take();
        switch (t.tag) {
            .dot => {
                if ((try p.peek()).tag == .string) {
                    _ = try p.take();
                    return new(.{ .index = .{ .a = identity(), .b = try p.parseString(null) } });
                }
                return identity();
            },
            .dotdot => return new(.recurse_all),
            .field => return new(.{ .index = .{ .a = identity(), .b = textLiteral(t.text) } }),
            .number => return literal(value.numberFromText(t.text, false) orelse return p.fail("bad number {s}", .{t.text})),
            .string => return p.parseString(null),
            .format => {
                const format = try p.formatNamed(t.text);
                if ((try p.peek()).tag == .string) {
                    _ = try p.take();
                    return p.parseString(format);
                }
                const n = native(.format, &.{});
                n.kind.native.format = format;
                return n;
            },
            .lparen => {
                const e = try p.parsePipe();
                try p.expect(.rparen, ")");
                return e;
            },
            .lbracket => {
                if ((try p.peek()).tag == .rbracket) {
                    _ = try p.take();
                    return array(null);
                }
                const e = try p.parsePipe();
                try p.expect(.rbracket, "]");
                return array(e);
            },
            .lbrace => return p.parseObject(),
            .variable => {
                if (std.mem.eql(u8, t.text, "__loc__")) return p.fail("$__loc__ is not supported", .{});
                return new(.{ .variable = try p.resolveVariable(t.text) });
            },
            .ident => {
                if (std.mem.eql(u8, t.text, "def") and p.allow_bind) {
                    const def = try p.parseDef();
                    const body = try p.parsePipe();
                    p.popDef(def);
                    return body;
                }
                if (std.mem.eql(u8, t.text, "null")) return literal(.null);
                if (std.mem.eql(u8, t.text, "true")) return literal(.true);
                if (std.mem.eql(u8, t.text, "false")) return literal(.false);
                if (std.mem.eql(u8, t.text, "if")) return p.parseIf();
                if (std.mem.eql(u8, t.text, "reduce")) return p.parseReduce(false);
                if (std.mem.eql(u8, t.text, "foreach")) return p.parseReduce(true);
                if (std.mem.eql(u8, t.text, "try")) {
                    const body = try p.parsePostfix(false);
                    if (isWord(try p.peek(), "catch")) return p.fail("try ... catch is not supported", .{});
                    return new(.{ .try_ = body });
                }
                for ([_][]const u8{ "then", "elif", "else", "end", "as", "and", "or", "catch", "label", "import", "include", "__loc__" }) |word| {
                    if (std.mem.eql(u8, t.text, word)) return p.fail("syntax error: unexpected {s}", .{word});
                }
                var args: std.ArrayList(*Node) = .empty;
                if ((try p.peek()).tag == .lparen) {
                    _ = try p.take();
                    while (true) {
                        args.append(gpa, try p.parsePipe()) catch oom();
                        const sep = try p.take();
                        if (sep.tag == .rparen) break;
                        if (sep.tag != .semicolon) return p.fail("syntax error: expected ; or )", .{});
                    }
                }
                return p.resolveCall(t.text, args.items);
            },
            .minus => return new(.{ .negate = try p.parsePostfix(false) }),
            else => return p.fail("syntax error: unexpected token", .{}),
        }
    }

    fn formatNamed(p: *Parser, name: []const u8) Error!Format {
        return std.meta.stringToEnum(Format, name) orelse p.fail("{s} is not a valid format", .{name});
    }

    fn parseIf(p: *Parser) Error!*Node {
        const cond = try p.parsePipe();
        try p.expectWord("then");
        const then = try p.parsePipe();
        const t = try p.take();
        const otherwise: *Node = if (isWord(t, "elif"))
            try p.parseIf()
        else if (isWord(t, "else")) blk: {
            const e = try p.parsePipe();
            try p.expectWord("end");
            break :blk e;
        } else if (isWord(t, "end")) identity() else return p.fail("syntax error: expected elif, else or end", .{});
        return new(.{ .if_ = .{ .cond = cond, .then = then, .otherwise = otherwise } });
    }

    /// `reduce SOURCE as $x (INIT; UPDATE)` and `foreach SOURCE as $x (INIT; UPDATE[; EXTRACT])`.
    fn parseReduce(p: *Parser, foreach: bool) Error!*Node {
        const source = try p.parsePostfix(false);
        try p.expectWord("as");
        const v = try p.take();
        if (v.tag != .variable) return p.fail("syntax error: only `as $name` is supported", .{});
        try p.expect(.lparen, "(");
        const first = try p.parsePipe();
        try p.expect(.semicolon, ";");
        p.scope.append(gpa, .{ .variable = v.text }) catch oom();
        defer _ = p.scope.pop();
        const update = try p.parsePipe();
        var extract: ?*Node = null;
        if (foreach and (try p.peek()).tag == .semicolon) {
            _ = try p.take();
            extract = try p.parsePipe();
        }
        try p.expect(.rparen, ")");
        if (foreach) return new(.{ .foreach = .{ .source = source, .init = first, .update = update, .extract = extract } });
        return new(.{ .reduce = .{ .source = source, .init = first, .update = update } });
    }

    /// `def name: body;` or `def name(f; $x): body;`, seen from here on.
    fn parseDef(p: *Parser) Error!*Def {
        const name = try p.take();
        if (name.tag != .ident) return p.fail("syntax error: a name must follow def", .{});
        var params: std.ArrayList([]const u8) = .empty;
        var dollar: std.ArrayList(bool) = .empty;
        if ((try p.peek()).tag == .lparen) {
            _ = try p.take();
            while (true) {
                const t = try p.take();
                switch (t.tag) {
                    .ident => {
                        params.append(gpa, t.text) catch oom();
                        dollar.append(gpa, false) catch oom();
                    },
                    .variable => {
                        params.append(gpa, t.text) catch oom();
                        dollar.append(gpa, true) catch oom();
                    },
                    else => return p.fail("syntax error: bad parameter", .{}),
                }
                const sep = try p.take();
                if (sep.tag == .rparen) break;
                if (sep.tag != .semicolon) return p.fail("syntax error: expected ; or )", .{});
            }
        }
        try p.expect(.colon, ":");
        const def = gpa.create(Def) catch oom();
        def.* = .{ .name = name.text, .params = params.items.len };
        p.scope.append(gpa, .{ .def = def }) catch oom();
        const mark = p.scope.items.len;
        if (params.items.len > 0) p.scope.append(gpa, .{ .params = params.items }) catch oom();
        // `$x` is `x as $x | ...`, each in turn.
        var binds: std.ArrayList(u32) = .empty;
        for (params.items, dollar.items, 0..) |param, is_dollar, i| {
            if (!is_dollar) continue;
            binds.append(gpa, @intCast(i)) catch oom();
            p.scope.append(gpa, .{ .variable = param }) catch oom();
        }
        var body = try p.parsePipe();
        try p.expect(.semicolon, ";");
        var k = binds.items.len;
        while (k > 0) {
            k -= 1;
            // The parameters' frame lies under the `k` variables bound before this one.
            const source = new(.{ .param = .{ .up = @intCast(k), .index = binds.items[k] } });
            body = new(.{ .bind = .{ .a = source, .b = body } });
        }
        def.body = body;
        p.scope.shrinkRetainingCapacity(mark);
        return def;
    }

    fn popDef(p: *Parser, def: *Def) void {
        const last = p.scope.pop().?;
        std.debug.assert(last == .def and last.def == def);
    }

    /// An object: `{a: f, $x, "text": f, "\(f)": g, (f): g, a, "a"}`. A member's value is an
    /// expression without `,` (which ends the member) and without `as` or `def`, as jq 1.8.2
    /// reads it.
    fn parseObject(p: *Parser) Error!*Node {
        var entries: std.ArrayList(Entry) = .empty;
        if ((try p.peek()).tag == .rbrace) {
            _ = try p.take();
            return new(.{ .object = entries.items });
        }
        while (true) {
            const t = try p.take();
            var key: *Node = undefined;
            var shorthand: ?*Node = null;
            switch (t.tag) {
                .ident => {
                    key = textLiteral(t.text);
                    shorthand = new(.{ .index = .{ .a = identity(), .b = key } });
                },
                .variable => {
                    const v = new(.{ .variable = try p.resolveVariable(t.text) });
                    if ((try p.peek()).tag == .colon) {
                        key = v;
                    } else {
                        key = textLiteral(t.text);
                        shorthand = v;
                    }
                },
                .string => {
                    key = try p.parseString(null);
                    shorthand = new(.{ .index = .{ .a = identity(), .b = key } });
                },
                .format => {
                    const format = try p.formatNamed(t.text);
                    try p.expect(.string, "a string");
                    key = try p.parseString(format);
                    shorthand = new(.{ .index = .{ .a = identity(), .b = key } });
                },
                .number => {
                    key = literal(value.numberFromText(t.text, false) orelse return p.fail("bad number", .{}));
                },
                .lparen => {
                    key = try p.parsePipe();
                    try p.expect(.rparen, ")");
                },
                else => return p.fail("syntax error: bad object key", .{}),
            }
            var val: *Node = undefined;
            if ((try p.peek()).tag == .colon) {
                _ = try p.take();
                val = try p.parseObjectValue();
            } else {
                val = shorthand orelse return p.fail("syntax error: expected :", .{});
            }
            entries.append(gpa, .{ .key = key, .value = val }) catch oom();
            const sep = try p.take();
            if (sep.tag == .rbrace) break;
            if (sep.tag != .comma) return p.fail("syntax error: expected , or }}", .{});
        }
        return new(.{ .object = entries.items });
    }

    fn parseObjectValue(p: *Parser) Error!*Node {
        const allow = p.allow_bind;
        p.allow_bind = false;
        defer p.allow_bind = allow;
        const lhs = try p.parseAlt();
        if ((try p.peek()).tag == .pipe) {
            _ = try p.take();
            return pipe(lhs, try p.parseObjectValue());
        }
        return lhs;
    }

    /// The rest of a text after its `"`, with `\(...)` parts; under a format, each part is
    /// written in it.
    fn parseString(p: *Parser, format: ?Format) Error!*Node {
        std.debug.assert(p.peeked == null);
        var parts: std.ArrayList(Part) = .empty;
        var current: std.ArrayList(u8) = .empty;
        while (true) {
            if (p.pos >= p.src.len) return p.fail("syntax error: unfinished string", .{});
            const c = p.src[p.pos];
            p.pos += 1;
            if (c == '"') break;
            if (c != '\\') {
                current.append(gpa, c) catch oom();
                continue;
            }
            if (p.pos >= p.src.len) return p.fail("syntax error: unfinished string", .{});
            const e = p.src[p.pos];
            p.pos += 1;
            switch (e) {
                '"', '\\', '/' => current.append(gpa, e) catch oom(),
                'b' => current.append(gpa, 0x08) catch oom(),
                'f' => current.append(gpa, 0x0c) catch oom(),
                'n' => current.append(gpa, '\n') catch oom(),
                'r' => current.append(gpa, '\r') catch oom(),
                't' => current.append(gpa, '\t') catch oom(),
                'u' => {
                    var cp = try p.hex4();
                    if (cp >= 0xD800 and cp < 0xDC00 and p.pos + 1 < p.src.len and p.src[p.pos] == '\\' and p.src[p.pos + 1] == 'u') {
                        const save = p.pos;
                        p.pos += 2;
                        const low = try p.hex4();
                        if (low >= 0xDC00 and low < 0xE000) {
                            cp = 0x10000 + ((cp - 0xD800) << 10) + (low - 0xDC00);
                        } else p.pos = save;
                    }
                    if (cp >= 0xD800 and cp < 0xE000) cp = 0xFFFD;
                    var buf: [4]u8 = undefined;
                    const n = std.unicode.utf8Encode(@intCast(cp), &buf) catch unreachable;
                    current.appendSlice(gpa, buf[0..n]) catch oom();
                },
                '(' => {
                    if (current.items.len > 0) {
                        parts.append(gpa, .{ .literal = current.toOwnedSlice(gpa) catch oom() }) catch oom();
                    }
                    const e2 = try p.parsePipe();
                    try p.expect(.rparen, ")");
                    parts.append(gpa, .{ .expr = e2 }) catch oom();
                },
                else => return p.fail("syntax error: invalid escape", .{}),
            }
        }
        const has_expr = for (parts.items) |part| {
            if (part == .expr) break true;
        } else false;
        if (!has_expr) return literal(value.stringFromList(current));
        if (current.items.len > 0) parts.append(gpa, .{ .literal = current.toOwnedSlice(gpa) catch oom() }) catch oom();
        return new(.{ .text = .{ .parts = parts.items, .format = format } });
    }

    fn hex4(p: *Parser) Error!u32 {
        if (p.pos + 4 > p.src.len) return p.fail("syntax error: invalid \\u escape", .{});
        const cp = std.fmt.parseInt(u32, p.src[p.pos..][0..4], 16) catch return p.fail("syntax error: invalid \\u escape", .{});
        p.pos += 4;
        return cp;
    }

    // -----------------------------------------------------------------------------------------
    // Names

    fn resolveVariable(p: *Parser, name: []const u8) Error!u32 {
        var frames: u32 = 0;
        var i = p.scope.items.len;
        while (i > 0) {
            i -= 1;
            switch (p.scope.items[i]) {
                .variable, .global => |n| {
                    if (std.mem.eql(u8, n, name)) return frames;
                    frames += 1;
                },
                .params => frames += 1,
                .def => {},
            }
        }
        return p.fail("${s} is not defined", .{name});
    }

    fn resolveCall(p: *Parser, name: []const u8, args: []*Node) Error!*Node {
        var frames: u32 = 0;
        var i = p.scope.items.len;
        while (i > 0) {
            i -= 1;
            switch (p.scope.items[i]) {
                .variable, .global => frames += 1,
                .params => |names| {
                    if (args.len == 0) {
                        for (names, 0..) |n, k| {
                            if (std.mem.eql(u8, n, name)) return new(.{ .param = .{ .up = frames, .index = @intCast(k) } });
                        }
                    }
                    frames += 1;
                },
                .def => |d| {
                    if (d.params == args.len and std.mem.eql(u8, d.name, name)) {
                        return new(.{ .call = .{ .def = d, .up = frames, .args = args } });
                    }
                },
            }
        }
        return builtin(name, args) orelse p.fail("{s}/{d} is not defined", .{ name, args.len });
    }
};

// ---------------------------------------------------------------------------------------------
// Nodes

fn new(kind: Kind) *Node {
    const n = gpa.create(Node) catch oom();
    n.* = .{ .kind = kind };
    n.simple = isSimple(n);
    return n;
}

fn literal(v: Value) *Node {
    return new(.{ .literal = v });
}

fn textLiteral(s: []const u8) *Node {
    return literal(value.string(s));
}

fn identity() *Node {
    return new(.identity);
}

fn pipe(a: *Node, b: *Node) *Node {
    return new(.{ .pipe = .{ .a = a, .b = b } });
}

fn native(id: Native, args: []const *Node) *Node {
    return new(.{ .native = .{ .id = id, .args = gpa.dupe(*Node, args) catch oom() } });
}

fn array(f: ?*Node) *Node {
    if (f) |body| {
        // `[.[] | f]` with `f` simple: one pass over the input.
        if (body.kind == .pipe and body.kind.pipe.a.kind == .iterate and
            body.kind.pipe.a.kind.iterate.target.kind == .identity and !body.kind.pipe.a.kind.iterate.opt and body.kind.pipe.b.simple)
        {
            return new(.{ .map_simple = body.kind.pipe.b });
        }
    }
    return new(.{ .array = f });
}

/// Whether a node gives exactly one output, or an error, and calls nothing.
fn isSimple(n: *Node) bool {
    return switch (n.kind) {
        .identity, .literal, .variable, .map_simple => true,
        .index => |x| !x.opt and x.a.simple and x.b.simple,
        .slice => |s| !s.opt and s.target.simple and (s.from == null or s.from.?.simple) and (s.to == null or s.to.?.simple),
        .negate => |a| a.simple,
        .pipe, .alternative, .and_, .or_, .bind => |x| x.a.simple and x.b.simple,
        .arith => |x| x.a.simple and x.b.simple,
        .if_ => |x| x.cond.simple and x.then.simple and x.otherwise.simple,
        .array => |f| f == null or f.?.simple,
        .object => |entries| for (entries) |e| {
            if (!e.key.simple or !e.value.simple) break false;
        } else true,
        .text => |t| for (t.parts) |part| switch (part) {
            .literal => {},
            .expr => |e| if (!e.simple) break false,
        } else true,
        .native => |x| x.id != .input and for (x.args) |a| {
            if (!a.simple) break false;
        } else true,
        else => false,
    };
}

fn typeMask(kinds: []const value.Kind) u8 {
    var mask: u8 = 0;
    for (kinds) |k| mask |= @as(u8, 1) << @as(u3, @intCast(@backingInt(k)));
    return mask;
}

fn iterate() *Node {
    return new(.{ .iterate = .{ .target = identity() } });
}

fn index(key: Value) *Node {
    return new(.{ .index = .{ .a = identity(), .b = literal(key) } });
}

/// `map([f])`: the keys of `sort_by(f)` and its kin.
fn keysOf(f: *Node) *Node {
    return array(pipe(iterate(), array(f)));
}

/// The functions jq has built in, from the nodes of the tree.
fn builtin(name: []const u8, args: []*Node) ?*Node {
    const eql = std.mem.eql;
    const a0: *Node = if (args.len > 0) args[0] else undefined;
    switch (args.len) {
        0 => {
            const natives = .{
                .{ "not", Native.not_ },                  .{ "length", Native.length },               .{ "utf8bytelength", Native.utf8bytelength },
                .{ "keys", Native.keys },                 .{ "keys_unsorted", Native.keys_unsorted }, .{ "to_entries", Native.to_entries },
                .{ "from_entries", Native.from_entries }, .{ "add", Native.add },                     .{ "any", Native.any },
                .{ "all", Native.all },                   .{ "sort", Native.sort },                   .{ "unique", Native.unique },
                .{ "min", Native.min },                   .{ "max", Native.max },                     .{ "reverse", Native.reverse },
                .{ "tostring", Native.tostring },         .{ "tonumber", Native.tonumber },           .{ "tojson", Native.tojson },
                .{ "fromjson", Native.fromjson },         .{ "type", Native.type_ },                  .{ "ascii_downcase", Native.ascii_downcase },
                .{ "ascii_upcase", Native.ascii_upcase }, .{ "floor", Native.floor },                 .{ "sqrt", Native.sqrt },
                .{ "error", Native.error_ },              .{ "input", Native.input },
            };
            inline for (natives) |entry| {
                if (eql(u8, name, entry[0])) return native(entry[1], &.{});
            }
            const selects = .{
                .{ "values", typeMask(&.{ .false, .true, .number, .string, .array, .object }) },
                .{ "nulls", typeMask(&.{.null}) },
                .{ "booleans", typeMask(&.{ .false, .true }) },
                .{ "numbers", typeMask(&.{.number}) },
                .{ "strings", typeMask(&.{.string}) },
                .{ "arrays", typeMask(&.{.array}) },
                .{ "objects", typeMask(&.{.object}) },
                .{ "iterables", typeMask(&.{ .array, .object }) },
                .{ "scalars", typeMask(&.{ .null, .false, .true, .number, .string }) },
            };
            inline for (selects) |entry| {
                if (eql(u8, name, entry[0])) return new(.{ .type_select = entry[1] });
            }
            if (eql(u8, name, "empty")) return new(.empty);
            if (eql(u8, name, "infinite")) return literal(value.number(std.math.inf(f64)));
            if (eql(u8, name, "nan")) return literal(value.number(std.math.nan(f64)));
            if (eql(u8, name, "recurse")) return new(.recurse_all);
            if (eql(u8, name, "paths")) return new(.paths);
            if (eql(u8, name, "inputs")) return new(.inputs);
            if (eql(u8, name, "first")) return index(value.number(0));
            if (eql(u8, name, "last")) return index(value.number(-1));
            return null;
        },
        1 => {
            const natives = .{
                .{ "has", Native.has },           .{ "in", Native.in_ },            .{ "split", Native.split },
                .{ "join", Native.join },         .{ "test", Native.test_ },        .{ "startswith", Native.startswith },
                .{ "endswith", Native.endswith }, .{ "ltrimstr", Native.ltrimstr }, .{ "rtrimstr", Native.rtrimstr },
                .{ "error", Native.error_ },      .{ "delpaths", Native.delpaths },
            };
            inline for (natives) |entry| {
                if (eql(u8, name, entry[0])) return native(entry[1], args);
            }
            const by = .{
                .{ "sort_by", Native.sort_by }, .{ "group_by", Native.group_by }, .{ "unique_by", Native.unique_by },
                .{ "min_by", Native.min_by },   .{ "max_by", Native.max_by },
            };
            inline for (by) |entry| {
                if (eql(u8, name, entry[0])) return native(entry[1], &.{keysOf(a0)});
            }
            if (eql(u8, name, "map")) return array(pipe(iterate(), a0));
            if (eql(u8, name, "map_values")) return new(.{ .assign = .{ .how = .update, .lhs = iterate(), .rhs = a0 } });
            if (eql(u8, name, "select")) return new(.{ .select = a0 });
            if (eql(u8, name, "recurse")) return new(.{ .recurse = a0 });
            if (eql(u8, name, "with_entries")) {
                return pipe(native(.to_entries, &.{}), pipe(array(pipe(iterate(), a0)), native(.from_entries, &.{})));
            }
            if (eql(u8, name, "add")) {
                const update = new(.{ .arith = .{ .op = .add, .a = identity(), .b = new(.{ .variable = 0 }) } });
                return new(.{ .reduce = .{ .source = a0, .init = literal(.null), .update = update } });
            }
            if (eql(u8, name, "any")) return new(.{ .any_all = .{ .gen = pipe(iterate(), a0), .all = false } });
            if (eql(u8, name, "all")) return new(.{ .any_all = .{ .gen = pipe(iterate(), a0), .all = true } });
            if (eql(u8, name, "range")) return new(.{ .range = gpa.dupe(*Node, &.{ literal(value.number(0)), a0 }) catch oom() });
            if (eql(u8, name, "first")) return new(.{ .first = a0 });
            if (eql(u8, name, "last")) {
                return new(.{ .reduce = .{ .source = a0, .init = literal(.null), .update = new(.{ .variable = 0 }) } });
            }
            if (eql(u8, name, "path")) return new(.{ .path_of = a0 });
            if (eql(u8, name, "getpath")) return new(.{ .getpath = a0 });
            if (eql(u8, name, "del")) return new(.{ .delete = a0 });
            return null;
        },
        2 => {
            if (eql(u8, name, "test")) return native(.test_, args);
            if (eql(u8, name, "setpath")) return native(.setpath, args);
            if (eql(u8, name, "range")) return new(.{ .range = gpa.dupe(*Node, args) catch oom() });
            if (eql(u8, name, "limit")) return new(.{ .limit = .{ .a = args[0], .b = args[1] } });
            if (eql(u8, name, "any")) return new(.{ .any_all = .{ .gen = pipe(args[0], args[1]), .all = false } });
            if (eql(u8, name, "all")) return new(.{ .any_all = .{ .gen = pipe(args[0], args[1]), .all = true } });
            return null;
        },
        3 => {
            if (eql(u8, name, "range")) return new(.{ .range = gpa.dupe(*Node, args) catch oom() });
            return null;
        },
        else => return null,
    }
}
