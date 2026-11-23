//! The machine that runs the filter's tree. It walks the tree with a stack of its own on the
//! heap, never the machine's: what is to be done with an output is a chain of frames (a
//! continuation), and each place that has more outputs to give is a choice on a stack of
//! choices. An output is passed down the chain of frames until it leaves the filter; then the
//! machine goes back to the last choice for the next one (backtracking). A call of a function
//! keeps the chain it was given, so a call in tail position takes no room; a call nested a
//! million deep is a chain of a million frames on the heap.
//!
//! An error goes back through the choices to the last `?` that is waiting for one; a `?` stops
//! waiting while its output is used outside it, and waits again when the machine comes back
//! into it, as jq's `try` does.
//!
//! Every frame, choice, environment and state is shared by a count of its holders, as values
//! are. The registers of the loop hold one count of each thing they name.

const std = @import("std");
const value = @import("value.zig");
const parse = @import("parse.zig");
const builtin = @import("builtin.zig");
const Value = value.Value;
const Node = parse.Node;
const gpa = value.gpa;
const oom = value.oom;
const retain = value.retain;
const release = value.release;
const Raised = builtin.Raised;

/// A frame of the environment: a variable's value, or the filter arguments of a call.
pub const Env = struct {
    rc: u32 = 1,
    parent: ?*Env,
    value: Value = .null,
    closures: []Closure = &.{},
};

/// A filter argument: its node and the environment it was written in.
pub const Closure = struct { node: *Node, env: ?*Env };

pub fn newEnv(parent: ?*Env, v: Value) *Env {
    const e = gpa.create(Env) catch oom();
    e.* = .{ .parent = parent, .value = v };
    return e;
}

fn envRetain(e: ?*Env) ?*Env {
    if (e) |x| x.rc += 1;
    return e;
}

/// Takes one holder from `e`; the frames left without one are freed through a list, not by
/// calling this again, as a chain of them may be long.
pub fn envRelease(e: ?*Env) void {
    var pending: std.ArrayList(*Env) = .empty;
    defer pending.deinit(gpa);
    var current = e;
    while (true) {
        if (current) |x| {
            x.rc -= 1;
            if (x.rc == 0) {
                release(x.value);
                for (x.closures) |cl| if (cl.env) |ce| pending.append(gpa, ce) catch oom();
                gpa.free(x.closures);
                const parent = x.parent;
                gpa.destroy(x);
                current = parent;
                continue;
            }
        }
        current = pending.pop() orelse return;
    }
}

fn lookup(e: ?*Env, up: u32) ?*Env {
    var x = e;
    for (0..up) |_| x = x.?.parent;
    return x;
}

/// Where an output stands in its input, while `path(f)` and the assignments follow it: `off`
/// when no one follows, `bad` when the output is not a path of the input.
const Path = union(enum) {
    off,
    bad,
    at: Value,
};

fn pathRetain(p: Path) Path {
    if (p == .at) _ = retain(p.at);
    return p;
}

fn pathRelease(p: Path) void {
    if (p == .at) release(p.at);
}

/// What a construct keeps between its outputs: an accumulator, a count, a flag.
const State = struct {
    rc: u32 = 1,
    /// The accumulator of `reduce` and `foreach`, the items of `[...]`, the value an
    /// assignment changes.
    acc: Value = .null,
    /// The right side of `=` and `op=`.
    operand: Value = .null,
    /// The paths an update or `del` deletes at its end.
    dels: Value = .null,
    /// Where its choice stands on the stack, for cutting back to it.
    mark: usize = 0,
    count: f64 = 0,
    limit: f64 = 0,
    /// `?`: waiting for an error; `//`: an output was given; `all`: is `all`.
    flag: bool = false,
    how: parse.Assign = .update,
};

fn newState(s: State) *State {
    const x = gpa.create(State) catch oom();
    x.* = s;
    return x;
}

fn stateRetain(s: *State) *State {
    s.rc += 1;
    return s;
}

fn stateRelease(s: *State) void {
    s.rc -= 1;
    if (s.rc > 0) return;
    release(s.acc);
    release(s.operand);
    release(s.dels);
    gpa.destroy(s);
}

/// A step of `reduce` or `foreach`.
const Step = struct { node: *Node, state: *State, env: ?*Env };

/// What is done with an output.
const Frame = union(enum) {
    /// `a | b`: `b` with it.
    pipe: struct { node: *Node, env: ?*Env },
    /// An argument of `node` given: the next, or the node itself when all are there.
    args: struct { node: *Node, env: ?*Env, input: Value, path: Path, count: u8, vals: [3]Value },
    /// `t[key]` with the target given.
    index_apply: struct { key: Value, opt: bool },
    /// `t[from:to]` with the target given.
    slice_apply: struct { bounds: [2]Value, opt: bool },
    /// `t[]` with the target given.
    iterate: bool,
    try_out: *State,
    negate,
    /// `a op b` with `b` given: `a` next.
    arith_rhs: struct { node: *Node, env: ?*Env, input: Value },
    /// `a op b` with `a` given.
    arith_lhs: struct { op: parse.Op, rhs: Value },
    /// `a and b`, `a or b` with `a` given.
    and_or: struct { node: *Node, env: ?*Env, input: Value },
    to_bool,
    if_: struct { node: *Node, env: ?*Env, input: Value, path: Path },
    /// `a as $x | b` with `$x` given.
    bind: struct { node: *Node, env: ?*Env, input: Value, path: Path },
    collect: *State,
    /// `reduce` or `foreach` with its first value given.
    reduce_init: struct { node: *Node, env: ?*Env, input: Value },
    /// An item of `reduce`'s source given.
    reduce_step: Step,
    reduce_store: *State,
    foreach_step: Step,
    /// `foreach`'s update given; `env` holds the item.
    foreach_store: Step,
    alt: *State,
    /// An object being built: the key or the value of entry `index` given.
    object: struct { node: *Node, env: ?*Env, input: Value, index: u32, partial: Value, key: Value, value_stage: bool },
    /// A text being built: the part `index` given, the parts after it in `vals`.
    text: struct { node: *Node, env: ?*Env, input: Value, index: u32, vals: Value },
    path_out,
    paths_out,
    select: struct { input: Value, path: Path },
    recurse: struct { f: ?*Node, env: ?*Env },
    limit: *State,
    any_all: *State,
    modify_path: struct { node: *Node, state: *State, env: ?*Env },
    modify_store: struct { state: *State, path: Value, mark: usize },
    assign_rhs: struct { node: *Node, env: ?*Env, input: Value },
    /// An output of something that is not a path, inside `path(f)`: a path still, `path`, when
    /// it is `at`, the value the input was there.
    mark_bad: struct { path: Path, at: Value },
};

const Cont = struct {
    rc: u32 = 1,
    next: ?*Cont,
    frame: Frame,
};

fn newCont(frame: Frame, next: ?*Cont) *Cont {
    const c = gpa.create(Cont) catch oom();
    c.* = .{ .next = next, .frame = frame };
    return c;
}

fn contRetain(c: ?*Cont) ?*Cont {
    if (c) |x| x.rc += 1;
    return c;
}

fn contRelease(c: ?*Cont) void {
    var current = c;
    while (current) |x| {
        x.rc -= 1;
        if (x.rc > 0) return;
        frameRelease(x.frame);
        current = x.next;
        gpa.destroy(x);
    }
}

/// Adds a holder to everything a frame holds.
fn frameRetain(f: Frame) Frame {
    switch (f) {
        .pipe => |x| _ = envRetain(x.env),
        .args => |x| {
            _ = envRetain(x.env);
            _ = retain(x.input);
            _ = pathRetain(x.path);
            for (x.vals[0..x.count]) |v| _ = retain(v);
        },
        .index_apply => |x| _ = retain(x.key),
        .slice_apply => |x| {
            _ = retain(x.bounds[0]);
            _ = retain(x.bounds[1]);
        },
        .try_out, .collect, .reduce_store, .alt, .limit, .any_all => |s| _ = stateRetain(s),
        .arith_rhs => |x| {
            _ = envRetain(x.env);
            _ = retain(x.input);
        },
        .arith_lhs => |x| _ = retain(x.rhs),
        .and_or => |x| {
            _ = envRetain(x.env);
            _ = retain(x.input);
        },
        .if_ => |x| {
            _ = envRetain(x.env);
            _ = retain(x.input);
            _ = pathRetain(x.path);
        },
        .bind => |x| {
            _ = envRetain(x.env);
            _ = retain(x.input);
            _ = pathRetain(x.path);
        },
        .reduce_init => |x| {
            _ = envRetain(x.env);
            _ = retain(x.input);
        },
        .reduce_step, .foreach_step, .foreach_store => |x| {
            _ = stateRetain(x.state);
            _ = envRetain(x.env);
        },
        .object => |x| {
            _ = envRetain(x.env);
            _ = retain(x.input);
            _ = retain(x.partial);
            _ = retain(x.key);
        },
        .text => |x| {
            _ = envRetain(x.env);
            _ = retain(x.input);
            _ = retain(x.vals);
        },
        .select => |x| {
            _ = retain(x.input);
            _ = pathRetain(x.path);
        },
        .recurse => |x| _ = envRetain(x.env),
        .modify_path => |x| {
            _ = stateRetain(x.state);
            _ = envRetain(x.env);
        },
        .modify_store => |x| {
            _ = stateRetain(x.state);
            _ = retain(x.path);
        },
        .assign_rhs => |x| {
            _ = envRetain(x.env);
            _ = retain(x.input);
        },
        .mark_bad => |x| {
            _ = pathRetain(x.path);
            _ = retain(x.at);
        },
        .iterate, .negate, .to_bool, .path_out, .paths_out => {},
    }
    return f;
}

fn frameRelease(f: Frame) void {
    switch (f) {
        .pipe => |x| envRelease(x.env),
        .args => |x| {
            envRelease(x.env);
            release(x.input);
            pathRelease(x.path);
            for (x.vals[0..x.count]) |v| release(v);
        },
        .index_apply => |x| release(x.key),
        .slice_apply => |x| {
            release(x.bounds[0]);
            release(x.bounds[1]);
        },
        .try_out, .collect, .reduce_store, .alt, .limit, .any_all => |s| stateRelease(s),
        .arith_rhs => |x| {
            envRelease(x.env);
            release(x.input);
        },
        .arith_lhs => |x| release(x.rhs),
        .and_or => |x| {
            envRelease(x.env);
            release(x.input);
        },
        .if_ => |x| {
            envRelease(x.env);
            release(x.input);
            pathRelease(x.path);
        },
        .bind => |x| {
            envRelease(x.env);
            release(x.input);
            pathRelease(x.path);
        },
        .reduce_init => |x| {
            envRelease(x.env);
            release(x.input);
        },
        .reduce_step, .foreach_step, .foreach_store => |x| {
            stateRelease(x.state);
            envRelease(x.env);
        },
        .object => |x| {
            envRelease(x.env);
            release(x.input);
            release(x.partial);
            release(x.key);
        },
        .text => |x| {
            envRelease(x.env);
            release(x.input);
            release(x.vals);
        },
        .select => |x| {
            release(x.input);
            pathRelease(x.path);
        },
        .recurse => |x| envRelease(x.env),
        .modify_path => |x| {
            stateRelease(x.state);
            envRelease(x.env);
        },
        .modify_store => |x| {
            stateRelease(x.state);
            release(x.path);
        },
        .assign_rhs => |x| {
            envRelease(x.env);
            release(x.input);
        },
        .mark_bad => |x| {
            pathRelease(x.path);
            release(x.at);
        },
        .iterate, .negate, .to_bool, .path_out, .paths_out => {},
    }
}

/// A place with more outputs to give, and the chain its outputs go to.
const Choice = struct {
    cont: ?*Cont,
    kind: union(enum) {
        comma: struct { node: *Node, input: Value, path: Path, env: ?*Env },
        iterate: struct { container: Value, index: usize, path: Path },
        range: struct { current: f64, end: f64, step: f64 },
        collect_end: *State,
        reduce_end: *State,
        foreach_end: *State,
        try_mark: *State,
        try_rearm: *State,
        alt_end: struct { state: *State, node: *Node, input: Value, path: Path, env: ?*Env },
        limit_mark: *State,
        recurse_more: struct { f: ?*Node, env: ?*Env, value: Value, path: Path },
        inputs,
        modify_end: *State,
        modify_first: struct { state: *State, path: Value },
        any_all_end: *State,
    },
};

fn choiceRelease(ch: Choice) void {
    contRelease(ch.cont);
    switch (ch.kind) {
        .comma => |x| {
            release(x.input);
            pathRelease(x.path);
            envRelease(x.env);
        },
        .iterate => |x| {
            release(x.container);
            pathRelease(x.path);
        },
        .range, .inputs => {},
        .collect_end, .reduce_end, .foreach_end, .try_mark, .try_rearm, .limit_mark, .modify_end, .any_all_end => |s| stateRelease(s),
        .alt_end => |x| {
            stateRelease(x.state);
            release(x.input);
            pathRelease(x.path);
            envRelease(x.env);
        },
        .recurse_more => |x| {
            envRelease(x.env);
            release(x.value);
            pathRelease(x.path);
        },
        .modify_first => |x| {
            stateRelease(x.state);
            release(x.path);
        },
    }
}

/// The nodes that follow paths; any other gives an output that is not a path.
fn pathCapable(n: *Node) bool {
    return switch (n.kind) {
        .identity, .recurse_all, .empty, .index, .slice, .iterate, .try_, .pipe, .comma, .alternative, .if_, .bind, .call, .param, .getpath, .select, .type_select, .recurse, .limit, .first => true,
        .native => |x| x.id == .error_,
        else => false,
    };
}

/// The arguments of a node that takes values, in the order they are worked out.
fn argNodes(n: *Node, i: usize) *Node {
    return switch (n.kind) {
        .native => |x| x.args[x.args.len - 1 - i],
        .range => |args| args[i],
        .slice => |s| if (i == 0 and s.from != null) s.from.? else s.to.?,
        .index => |x| x.b,
        .getpath => |p| p,
        .limit => |x| x.a,
        else => unreachable,
    };
}

fn argCount(n: *Node) u8 {
    return switch (n.kind) {
        .native => |x| @intCast(x.args.len),
        .range => |args| @intCast(args.len),
        .slice => |s| @as(u8, @intFromBool(s.from != null)) + @intFromBool(s.to != null),
        .index, .getpath, .limit => 1,
        else => unreachable,
    };
}

/// The recursion of `..`, `recurse(.[]?)`, shared by `paths`.
var recurse_all_node: Node = .{ .kind = .recurse_all };

pub const Machine = struct {
    ctx: builtin.Context,
    choices: std.ArrayList(Choice) = .empty,
    /// The run to start on the next call of `next`.
    pending: ?struct { node: *Node, input: Value, env: ?*Env } = null,
    /// The node to evaluate next, set by `applyArgs`.
    pending_node: *Node = undefined,

    pub fn start(m: *Machine, node: *Node, input: Value, env: ?*Env) void {
        m.pending = .{ .node = node, .input = input, .env = envRetain(env) };
    }

    fn push(m: *Machine, cont: ?*Cont, kind: @FieldType(Choice, "kind")) void {
        m.choices.append(gpa, .{ .cont = cont, .kind = kind }) catch oom();
    }

    /// Drops the choices from `mark` up: the outputs they would give are not wanted.
    fn cut(m: *Machine, mark: usize) void {
        while (m.choices.items.len > mark) choiceRelease(m.choices.pop().?);
    }

    fn invalidPath(m: *Machine, v: Value) Raised {
        return m.ctx.fail("Invalid path expression with result {f}", .{builtin.shown(v)});
    }

    /// `p` with `key` added; raises when `p` is not a path, `v` being what was indexed.
    fn extend(m: *Machine, p: Path, key: Value, v: Value) Raised!Path {
        switch (p) {
            .off => return .off,
            .bad => return m.invalidPath(v),
            .at => |a| {
                var arr = a;
                value.uniqueArray(&arr).items.append(gpa, retain(key)) catch oom();
                return .{ .at = arr };
            },
        }
    }

    /// The next output of the run, null when there is none; `error.Raised` for an error no `?`
    /// stopped, its value in `ctx.err`.
    pub fn next(m: *Machine) Raised!?Value {
        const ctx = &m.ctx;
        var node: *Node = undefined;
        var input: Value = .null;
        var path: Path = .off;
        var env: ?*Env = null;
        var cont: ?*Cont = null;
        var out: Value = .null;
        var mode: enum { eval, emit, backtrack, raise } = .backtrack;
        if (m.pending) |p| {
            node = p.node;
            input = p.input;
            env = p.env;
            m.pending = null;
            mode = .eval;
        }

        loop: switch (mode) {
            // `node` with `input` (and `path`) in `env`, its outputs to `cont`.
            .eval => {
                if (node.simple and (path == .off or !pathCapable(node))) {
                    // In a path, an output that is not a path still stands where the input
                    // does when it is the input itself, as jq's `path_intact` has it.
                    const at = if (path != .off) retain(input) else .null;
                    defer release(at);
                    const v = m.evalSimple(node, input, env) catch {
                        pathRelease(path);
                        envRelease(env);
                        contRelease(cont);
                        continue :loop .raise;
                    };
                    envRelease(env);
                    if (path != .off and !identical(v, at)) {
                        pathRelease(path);
                        path = .bad;
                    }
                    out = v;
                    continue :loop .emit;
                }
                if (path != .off and !pathCapable(node)) {
                    // Worked out as usual; its outputs are not paths, unless one is the input.
                    cont = newCont(.{ .mark_bad = .{ .path = path, .at = retain(input) } }, cont);
                    path = .off;
                }
                switch (node.kind) {
                    .identity => {
                        out = input;
                        envRelease(env);
                        continue :loop .emit;
                    },
                    .recurse_all => {
                        m.push(contRetain(cont), .{ .recurse_more = .{ .f = null, .env = null, .value = retain(input), .path = pathRetain(path) } });
                        out = input;
                        envRelease(env);
                        continue :loop .emit;
                    },
                    .empty => {
                        release(input);
                        pathRelease(path);
                        envRelease(env);
                        contRelease(cont);
                        continue :loop .backtrack;
                    },
                    .index, .slice, .native, .range, .getpath, .limit => {
                        // The arguments first, each a value.
                        var vals: [3]Value = undefined;
                        var count: u8 = 0;
                        const total = argCount(node);
                        while (count < total) {
                            const arg = argNodes(node, count);
                            if (!arg.simple) break;
                            vals[count] = m.evalSimple(arg, retain(input), env) catch {
                                for (vals[0..count]) |v| release(v);
                                release(input);
                                pathRelease(path);
                                envRelease(env);
                                contRelease(cont);
                                continue :loop .raise;
                            };
                            count += 1;
                        }
                        if (count < total) {
                            const arg = argNodes(node, count);
                            cont = newCont(.{ .args = .{ .node = node, .env = envRetain(env), .input = retain(input), .path = path, .count = count, .vals = vals } }, cont);
                            path = .off;
                            node = arg;
                            continue :loop .eval;
                        }
                        // All there: the node itself.
                        if (m.applyArgs(node, vals[0..count], &input, &path, &env, &cont)) |next_mode| switch (next_mode) {
                            .eval => {
                                node = m.pending_node;
                                continue :loop .eval;
                            },
                            .emit => {
                                out = input;
                                continue :loop .emit;
                            },
                            .backtrack => continue :loop .backtrack,
                        } else |_| continue :loop .raise;
                    },
                    .iterate => |x| {
                        if (x.target.kind == .identity) {
                            envRelease(env);
                            m.startIterate(input, path, cont, x.opt) catch continue :loop .raise;
                            continue :loop .backtrack;
                        }
                        cont = newCont(.{ .iterate = x.opt }, cont);
                        node = x.target;
                        continue :loop .eval;
                    },
                    .try_ => |body| {
                        const s = newState(.{ .flag = true });
                        m.push(null, .{ .try_mark = s });
                        cont = newCont(.{ .try_out = stateRetain(s) }, cont);
                        node = body;
                        continue :loop .eval;
                    },
                    .negate => |a| {
                        cont = newCont(.negate, cont);
                        node = a;
                        continue :loop .eval;
                    },
                    .pipe => |x| {
                        if (x.a.simple and path == .off) {
                            const v = m.evalSimple(x.a, input, env) catch {
                                envRelease(env);
                                contRelease(cont);
                                continue :loop .raise;
                            };
                            input = v;
                            node = x.b;
                            continue :loop .eval;
                        }
                        cont = newCont(.{ .pipe = .{ .node = x.b, .env = envRetain(env) } }, cont);
                        node = x.a;
                        continue :loop .eval;
                    },
                    .comma => |x| {
                        m.push(contRetain(cont), .{ .comma = .{ .node = x.b, .input = retain(input), .path = pathRetain(path), .env = envRetain(env) } });
                        node = x.a;
                        continue :loop .eval;
                    },
                    .alternative => |x| {
                        const s = newState(.{});
                        m.push(contRetain(cont), .{ .alt_end = .{ .state = s, .node = x.b, .input = retain(input), .path = pathRetain(path), .env = envRetain(env) } });
                        cont = newCont(.{ .alt = stateRetain(s) }, cont);
                        node = x.a;
                        continue :loop .eval;
                    },
                    .and_, .or_ => |x| {
                        cont = newCont(.{ .and_or = .{ .node = node, .env = envRetain(env), .input = retain(input) } }, cont);
                        node = x.a;
                        continue :loop .eval;
                    },
                    .arith => |x| {
                        if (x.b.simple) {
                            const r = m.evalSimple(x.b, retain(input), env) catch {
                                release(input);
                                envRelease(env);
                                contRelease(cont);
                                continue :loop .raise;
                            };
                            cont = newCont(.{ .arith_lhs = .{ .op = x.op, .rhs = r } }, cont);
                            node = x.a;
                            continue :loop .eval;
                        }
                        cont = newCont(.{ .arith_rhs = .{ .node = node, .env = envRetain(env), .input = retain(input) } }, cont);
                        node = x.b;
                        continue :loop .eval;
                    },
                    .if_ => |x| {
                        if (x.cond.simple) {
                            const c = m.evalSimple(x.cond, retain(input), env) catch {
                                release(input);
                                pathRelease(path);
                                envRelease(env);
                                contRelease(cont);
                                continue :loop .raise;
                            };
                            node = if (value.truthy(c)) x.then else x.otherwise;
                            release(c);
                            continue :loop .eval;
                        }
                        cont = newCont(.{ .if_ = .{ .node = node, .env = envRetain(env), .input = retain(input), .path = path } }, cont);
                        path = .off;
                        node = x.cond;
                        continue :loop .eval;
                    },
                    .bind => |x| {
                        if (x.a.simple) {
                            const v = m.evalSimple(x.a, retain(input), env) catch {
                                release(input);
                                pathRelease(path);
                                envRelease(env);
                                contRelease(cont);
                                continue :loop .raise;
                            };
                            env = newEnv(env, v);
                            node = x.b;
                            continue :loop .eval;
                        }
                        cont = newCont(.{ .bind = .{ .node = node, .env = envRetain(env), .input = retain(input), .path = path } }, cont);
                        path = .off;
                        node = x.a;
                        continue :loop .eval;
                    },
                    .reduce, .foreach => {
                        const init = if (node.kind == .reduce) node.kind.reduce.init else node.kind.foreach.init;
                        cont = newCont(.{ .reduce_init = .{ .node = node, .env = envRetain(env), .input = retain(input) } }, cont);
                        node = init;
                        continue :loop .eval;
                    },
                    .call => |x| {
                        const target = envRetain(lookup(env, x.up));
                        if (x.args.len > 0) {
                            const closures = gpa.alloc(Closure, x.args.len) catch oom();
                            for (x.args, closures) |arg, *cl| cl.* = .{ .node = arg, .env = envRetain(env) };
                            const frame = gpa.create(Env) catch oom();
                            frame.* = .{ .parent = target, .closures = closures };
                            envRelease(env);
                            env = frame;
                        } else {
                            envRelease(env);
                            env = target;
                        }
                        node = x.def.body;
                        continue :loop .eval;
                    },
                    .param => |x| {
                        const cl = lookup(env, x.up).?.closures[x.index];
                        const closure_env = envRetain(cl.env);
                        envRelease(env);
                        env = closure_env;
                        node = cl.node;
                        continue :loop .eval;
                    },
                    .array => |f| {
                        const s = newState(.{ .acc = value.emptyArray() });
                        m.push(cont, .{ .collect_end = s });
                        cont = newCont(.{ .collect = stateRetain(s) }, null);
                        node = f.?;
                        continue :loop .eval;
                    },
                    .object => {
                        const step = m.buildObject(node, env, input, 0, value.emptyObject(), .null, false, &cont) catch continue :loop .raise;
                        switch (step) {
                            .emit => |v| {
                                out = v;
                                continue :loop .emit;
                            },
                            .eval => |e| {
                                node = e.node;
                                input = e.input;
                                env = e.env;
                                continue :loop .eval;
                            },
                        }
                    },
                    .text => |t| {
                        var vals = value.emptyArray();
                        vals.array.items.appendNTimes(gpa, .null, t.parts.len) catch oom();
                        const step = m.buildText(node, env, input, @intCast(t.parts.len), vals, &cont) catch continue :loop .raise;
                        switch (step) {
                            .emit => |v| {
                                out = v;
                                continue :loop .emit;
                            },
                            .eval => |e| {
                                node = e.node;
                                input = e.input;
                                env = e.env;
                                continue :loop .eval;
                            },
                        }
                    },
                    .path_of => |f| {
                        cont = newCont(.path_out, cont);
                        path = .{ .at = value.emptyArray() };
                        node = f;
                        continue :loop .eval;
                    },
                    .paths => {
                        cont = newCont(.paths_out, cont);
                        path = .{ .at = value.emptyArray() };
                        node = &recurse_all_node;
                        continue :loop .eval;
                    },
                    .select => |f| {
                        if (f.simple) {
                            const c = m.evalSimple(f, retain(input), env) catch {
                                release(input);
                                pathRelease(path);
                                envRelease(env);
                                contRelease(cont);
                                continue :loop .raise;
                            };
                            envRelease(env);
                            if (value.truthy(c)) {
                                release(c);
                                out = input;
                                continue :loop .emit;
                            }
                            release(c);
                            release(input);
                            pathRelease(path);
                            contRelease(cont);
                            continue :loop .backtrack;
                        }
                        cont = newCont(.{ .select = .{ .input = retain(input), .path = path } }, cont);
                        path = .off;
                        node = f;
                        continue :loop .eval;
                    },
                    .type_select => |mask| {
                        envRelease(env);
                        if (mask & (@as(u8, 1) << @as(u3, @intCast(@backingInt(value.kind(input))))) != 0) {
                            out = input;
                            continue :loop .emit;
                        }
                        release(input);
                        pathRelease(path);
                        contRelease(cont);
                        continue :loop .backtrack;
                    },
                    .recurse => |f| {
                        m.push(contRetain(cont), .{ .recurse_more = .{ .f = f, .env = envRetain(env), .value = retain(input), .path = pathRetain(path) } });
                        envRelease(env);
                        out = input;
                        continue :loop .emit;
                    },
                    .first => |f| {
                        const s = newState(.{ .limit = 1, .mark = m.choices.items.len });
                        m.push(null, .{ .limit_mark = s });
                        cont = newCont(.{ .limit = stateRetain(s) }, cont);
                        node = f;
                        continue :loop .eval;
                    },
                    .any_all => |x| {
                        const s = newState(.{ .flag = x.all, .mark = m.choices.items.len });
                        m.push(contRetain(cont), .{ .any_all_end = s });
                        cont = newCont(.{ .any_all = stateRetain(s) }, cont);
                        node = x.gen;
                        continue :loop .eval;
                    },
                    .delete => |f| {
                        const s = newState(.{ .acc = retain(input), .dels = value.emptyArray(), .how = .delete });
                        m.push(cont, .{ .modify_end = s });
                        cont = newCont(.{ .modify_path = .{ .node = node, .state = stateRetain(s), .env = envRetain(env) } }, null);
                        path = .{ .at = value.emptyArray() };
                        node = f;
                        continue :loop .eval;
                    },
                    .assign => |x| {
                        if (x.how == .update) {
                            m.startModify(node, input, env, .null, &cont);
                            path = .{ .at = value.emptyArray() };
                            node = x.lhs;
                            continue :loop .eval;
                        }
                        if (x.rhs.simple) {
                            const operand = m.evalSimple(x.rhs, retain(input), env) catch {
                                release(input);
                                envRelease(env);
                                contRelease(cont);
                                continue :loop .raise;
                            };
                            m.startModify(node, input, env, operand, &cont);
                            path = .{ .at = value.emptyArray() };
                            node = x.lhs;
                            continue :loop .eval;
                        }
                        cont = newCont(.{ .assign_rhs = .{ .node = node, .env = envRetain(env), .input = retain(input) } }, cont);
                        node = x.rhs;
                        continue :loop .eval;
                    },
                    .inputs => {
                        release(input);
                        envRelease(env);
                        const v = ctx.input.next() catch {
                            contRelease(cont);
                            _ = ctx.fail("{s}", .{ctx.input.message}) catch {};
                            continue :loop .raise;
                        };
                        if (v) |x| {
                            m.push(contRetain(cont), .inputs);
                            out = x;
                            continue :loop .emit;
                        }
                        contRelease(cont);
                        continue :loop .backtrack;
                    },
                    .literal, .variable, .map_simple => unreachable,
                }
            },

            // `out` (at `path`) given to `cont`.
            .emit => {
                const c = cont orelse {
                    pathRelease(path);
                    path = .off;
                    return out;
                };
                var frame: Frame = undefined;
                if (c.rc == 1) {
                    frame = c.frame;
                    cont = c.next;
                    gpa.destroy(c);
                } else {
                    frame = frameRetain(c.frame);
                    cont = contRetain(c.next);
                    c.rc -= 1;
                }
                switch (frame) {
                    .pipe => |x| {
                        input = out;
                        env = x.env;
                        node = x.node;
                        continue :loop .eval;
                    },
                    .args => |x| {
                        var a = x;
                        a.vals[a.count] = out;
                        a.count += 1;
                        // A path given for an argument is not wanted.
                        pathRelease(path);
                        path = .off;
                        const total = argCount(a.node);
                        while (a.count < total) {
                            const arg = argNodes(a.node, a.count);
                            if (!arg.simple) break;
                            a.vals[a.count] = m.evalSimple(arg, retain(a.input), a.env) catch {
                                frameRelease(.{ .args = a });
                                contRelease(cont);
                                continue :loop .raise;
                            };
                            a.count += 1;
                        }
                        if (a.count < total) {
                            const arg = argNodes(a.node, a.count);
                            input = retain(a.input);
                            env = envRetain(a.env);
                            cont = newCont(.{ .args = a }, cont);
                            node = arg;
                            continue :loop .eval;
                        }
                        input = a.input;
                        path = a.path;
                        env = a.env;
                        if (m.applyArgs(a.node, a.vals[0..a.count], &input, &path, &env, &cont)) |next_mode| switch (next_mode) {
                            .eval => {
                                node = m.pending_node;
                                continue :loop .eval;
                            },
                            .emit => {
                                out = input;
                                continue :loop .emit;
                            },
                            .backtrack => continue :loop .backtrack,
                        } else |_| continue :loop .raise;
                    },
                    .index_apply => |x| {
                        const key = x.key;
                        defer release(key);
                        if (path == .bad) {
                            _ = m.invalidPath(out) catch {};
                            release(out);
                            contRelease(cont);
                            continue :loop .raise;
                        }
                        const v = builtin.index(ctx, out, key) catch {
                            release(out);
                            pathRelease(path);
                            contRelease(cont);
                            if (x.opt) {
                                value.replace(&ctx.err, .null);
                                continue :loop .backtrack;
                            }
                            continue :loop .raise;
                        };
                        path = m.extend(path, key, out) catch {
                            release(v);
                            release(out);
                            contRelease(cont);
                            continue :loop .raise;
                        };
                        release(out);
                        out = v;
                        continue :loop .emit;
                    },
                    .slice_apply => |x| {
                        const b = x.bounds;
                        defer release(b[0]);
                        defer release(b[1]);
                        if (path == .bad) {
                            _ = m.invalidPath(out) catch {};
                            release(out);
                            contRelease(cont);
                            continue :loop .raise;
                        }
                        const v = builtin.slice(ctx, out, b[0], b[1]) catch {
                            release(out);
                            pathRelease(path);
                            contRelease(cont);
                            if (x.opt) {
                                value.replace(&ctx.err, .null);
                                continue :loop .backtrack;
                            }
                            continue :loop .raise;
                        };
                        const key = sliceKey(b[0], b[1]);
                        defer release(key);
                        path = m.extend(path, key, out) catch {
                            release(v);
                            release(out);
                            contRelease(cont);
                            continue :loop .raise;
                        };
                        release(out);
                        out = v;
                        continue :loop .emit;
                    },
                    .iterate => |opt| {
                        m.startIterate(out, path, cont, opt) catch continue :loop .raise;
                        continue :loop .backtrack;
                    },
                    .try_out => |s| {
                        // The output is used outside: an error there is not this `?`'s.
                        s.flag = false;
                        m.push(null, .{ .try_rearm = s });
                        continue :loop .emit;
                    },
                    .negate => {
                        const v = builtin.negate(ctx, out) catch {
                            release(out);
                            contRelease(cont);
                            continue :loop .raise;
                        };
                        release(out);
                        out = v;
                        continue :loop .emit;
                    },
                    .arith_rhs => |x| {
                        const a = x.node.kind.arith;
                        if (a.a.simple) {
                            defer envRelease(x.env);
                            const l = m.evalSimple(a.a, x.input, x.env) catch {
                                release(out);
                                contRelease(cont);
                                continue :loop .raise;
                            };
                            out = builtin.arith(ctx, a.op, l, out) catch {
                                contRelease(cont);
                                continue :loop .raise;
                            };
                            continue :loop .emit;
                        }
                        cont = newCont(.{ .arith_lhs = .{ .op = a.op, .rhs = out } }, cont);
                        input = x.input;
                        env = x.env;
                        node = a.a;
                        continue :loop .eval;
                    },
                    .arith_lhs => |x| {
                        out = builtin.arith(ctx, x.op, out, x.rhs) catch {
                            contRelease(cont);
                            continue :loop .raise;
                        };
                        continue :loop .emit;
                    },
                    .and_or => |x| {
                        const is_and = x.node.kind == .and_;
                        const b = if (is_and) x.node.kind.and_.b else x.node.kind.or_.b;
                        const t = value.truthy(out);
                        release(out);
                        if (t != is_and) {
                            envRelease(x.env);
                            release(x.input);
                            out = value.boolean(t);
                            continue :loop .emit;
                        }
                        cont = newCont(.to_bool, cont);
                        input = x.input;
                        env = x.env;
                        node = b;
                        continue :loop .eval;
                    },
                    .to_bool => {
                        const t = value.truthy(out);
                        release(out);
                        out = value.boolean(t);
                        continue :loop .emit;
                    },
                    .if_ => |x| {
                        const i = x.node.kind.if_;
                        node = if (value.truthy(out)) i.then else i.otherwise;
                        release(out);
                        input = x.input;
                        env = x.env;
                        path = x.path;
                        continue :loop .eval;
                    },
                    .bind => |x| {
                        pathRelease(path);
                        env = newEnv(x.env, out);
                        input = x.input;
                        path = x.path;
                        node = x.node.kind.bind.b;
                        continue :loop .eval;
                    },
                    .collect => |s| {
                        pathRelease(path);
                        path = .off;
                        s.acc.array.items.append(gpa, out) catch oom();
                        stateRelease(s);
                        continue :loop .backtrack;
                    },
                    .reduce_init => |x| {
                        pathRelease(path);
                        path = .off;
                        const is_reduce = x.node.kind == .reduce;
                        const source = if (is_reduce) x.node.kind.reduce.source else x.node.kind.foreach.source;
                        const s = newState(.{ .acc = out });
                        if (is_reduce) {
                            m.push(cont, .{ .reduce_end = s });
                            cont = newCont(.{ .reduce_step = .{ .node = x.node, .state = stateRetain(s), .env = envRetain(x.env) } }, null);
                        } else {
                            m.push(null, .{ .foreach_end = s });
                            cont = newCont(.{ .foreach_step = .{ .node = x.node, .state = stateRetain(s), .env = envRetain(x.env) } }, cont);
                        }
                        input = x.input;
                        env = x.env;
                        node = source;
                        continue :loop .eval;
                    },
                    .reduce_step => |x| {
                        pathRelease(path);
                        path = .off;
                        const update = x.node.kind.reduce.update;
                        const item_env = newEnv(x.env, out);
                        const acc = value.take(&x.state.acc);
                        if (update.simple) {
                            // The accumulator is given away, so that the update holds it once
                            // and can change it in place.
                            const v = m.evalSimple(update, acc, item_env) catch {
                                envRelease(item_env);
                                stateRelease(x.state);
                                continue :loop .raise;
                            };
                            envRelease(item_env);
                            value.replace(&x.state.acc, v);
                            stateRelease(x.state);
                            continue :loop .backtrack;
                        }
                        cont = newCont(.{ .reduce_store = x.state }, null);
                        input = acc;
                        env = item_env;
                        node = update;
                        continue :loop .eval;
                    },
                    .reduce_store => |s| {
                        pathRelease(path);
                        path = .off;
                        value.replace(&s.acc, out);
                        stateRelease(s);
                        continue :loop .backtrack;
                    },
                    .foreach_step => |x| {
                        pathRelease(path);
                        path = .off;
                        const update = x.node.kind.foreach.update;
                        const item_env = newEnv(x.env, out);
                        const acc = value.take(&x.state.acc);
                        if (!update.simple) {
                            cont = newCont(.{ .foreach_store = .{ .node = x.node, .state = x.state, .env = envRetain(item_env) } }, cont);
                            input = acc;
                            env = item_env;
                            node = update;
                            continue :loop .eval;
                        }
                        const v = m.evalSimple(update, acc, item_env) catch {
                            envRelease(item_env);
                            stateRelease(x.state);
                            contRelease(cont);
                            continue :loop .raise;
                        };
                        value.replace(&x.state.acc, retain(v));
                        stateRelease(x.state);
                        if (x.node.kind.foreach.extract) |extract| {
                            input = v;
                            env = item_env;
                            node = extract;
                            continue :loop .eval;
                        }
                        envRelease(item_env);
                        out = v;
                        continue :loop .emit;
                    },
                    .foreach_store => |x| {
                        pathRelease(path);
                        path = .off;
                        value.replace(&x.state.acc, retain(out));
                        stateRelease(x.state);
                        if (x.node.kind.foreach.extract) |extract| {
                            input = out;
                            env = x.env;
                            node = extract;
                            continue :loop .eval;
                        }
                        envRelease(x.env);
                        continue :loop .emit;
                    },
                    .alt => |s| {
                        if (value.truthy(out)) {
                            s.flag = true;
                            stateRelease(s);
                            continue :loop .emit;
                        }
                        stateRelease(s);
                        release(out);
                        pathRelease(path);
                        contRelease(cont);
                        continue :loop .backtrack;
                    },
                    .object => |x| {
                        pathRelease(path);
                        path = .off;
                        var partial = x.partial;
                        var key = x.key;
                        var index = x.index;
                        var value_stage = x.value_stage;
                        if (!value_stage) {
                            release(key);
                            key = out;
                            value_stage = true;
                        } else {
                            m.objectPut(&partial, key, out) catch {
                                release(partial);
                                release(key);
                                envRelease(x.env);
                                release(x.input);
                                contRelease(cont);
                                continue :loop .raise;
                            };
                            release(key);
                            key = .null;
                            index += 1;
                            value_stage = false;
                        }
                        const step = m.buildObject(x.node, x.env, x.input, index, partial, key, value_stage, &cont) catch continue :loop .raise;
                        switch (step) {
                            .emit => |v| {
                                out = v;
                                continue :loop .emit;
                            },
                            .eval => |e| {
                                node = e.node;
                                input = e.input;
                                env = e.env;
                                continue :loop .eval;
                            },
                        }
                    },
                    .text => |x| {
                        pathRelease(path);
                        path = .off;
                        var vals = x.vals;
                        value.replace(&value.uniqueArray(&vals).items.items[x.index], out);
                        const step = m.buildText(x.node, x.env, x.input, x.index, vals, &cont) catch continue :loop .raise;
                        switch (step) {
                            .emit => |v| {
                                out = v;
                                continue :loop .emit;
                            },
                            .eval => |e| {
                                node = e.node;
                                input = e.input;
                                env = e.env;
                                continue :loop .eval;
                            },
                        }
                    },
                    .path_out => {
                        switch (path) {
                            .at => |p| {
                                release(out);
                                out = p;
                                path = .off;
                                continue :loop .emit;
                            },
                            else => {
                                _ = m.invalidPath(out) catch {};
                                release(out);
                                contRelease(cont);
                                continue :loop .raise;
                            },
                        }
                    },
                    .paths_out => {
                        release(out);
                        const p = path.at;
                        path = .off;
                        if (p.array.items.items.len == 0) {
                            release(p);
                            contRelease(cont);
                            continue :loop .backtrack;
                        }
                        out = p;
                        continue :loop .emit;
                    },
                    .select => |x| {
                        const t = value.truthy(out);
                        release(out);
                        pathRelease(path);
                        if (t) {
                            out = x.input;
                            path = x.path;
                            continue :loop .emit;
                        }
                        release(x.input);
                        pathRelease(x.path);
                        path = .off;
                        contRelease(cont);
                        continue :loop .backtrack;
                    },
                    .recurse => |x| {
                        m.push(contRetain(cont), .{ .recurse_more = .{ .f = x.f, .env = x.env, .value = retain(out), .path = pathRetain(path) } });
                        continue :loop .emit;
                    },
                    .limit => |s| {
                        s.count += 1;
                        if (s.count >= s.limit) m.cut(s.mark);
                        stateRelease(s);
                        continue :loop .emit;
                    },
                    .any_all => |s| {
                        const t = value.truthy(out);
                        release(out);
                        pathRelease(path);
                        path = .off;
                        const all = s.flag;
                        if (t != all) {
                            m.cut(s.mark);
                            stateRelease(s);
                            out = value.boolean(!all);
                            continue :loop .emit;
                        }
                        stateRelease(s);
                        contRelease(cont);
                        continue :loop .backtrack;
                    },
                    .modify_path => |x| {
                        release(out);
                        const p = switch (path) {
                            .at => |p| p,
                            else => {
                                _ = ctx.fail("Invalid path expression", .{}) catch {};
                                stateRelease(x.state);
                                envRelease(x.env);
                                contRelease(cont);
                                continue :loop .raise;
                            },
                        };
                        path = .off;
                        const update = if (x.node.kind == .assign) x.node.kind.assign.rhs else undefined;
                        const done = m.modifyAt(x.state, p, update, x.env) catch {
                            release(p);
                            stateRelease(x.state);
                            envRelease(x.env);
                            contRelease(cont);
                            continue :loop .raise;
                        };
                        if (done) {
                            release(p);
                            stateRelease(x.state);
                            envRelease(x.env);
                            contRelease(cont);
                            continue :loop .backtrack;
                        }
                        // The update is a filter: its first output is set, none deletes.
                        const old = builtin.takePath(ctx, &x.state.acc, p.array.items.items) catch {
                            release(p);
                            stateRelease(x.state);
                            envRelease(x.env);
                            contRelease(cont);
                            continue :loop .raise;
                        };
                        const mark = m.choices.items.len;
                        m.push(null, .{ .modify_first = .{ .state = stateRetain(x.state), .path = retain(p) } });
                        contRelease(cont);
                        cont = newCont(.{ .modify_store = .{ .state = x.state, .path = p, .mark = mark } }, null);
                        input = old;
                        env = x.env;
                        node = update;
                        continue :loop .eval;
                    },
                    .modify_store => |x| {
                        pathRelease(path);
                        path = .off;
                        defer stateRelease(x.state);
                        defer release(x.path);
                        builtin.setPath(ctx, &x.state.acc, x.path.array.items.items, out) catch {
                            contRelease(cont);
                            continue :loop .raise;
                        };
                        m.cut(x.mark);
                        contRelease(cont);
                        continue :loop .backtrack;
                    },
                    .assign_rhs => |x| {
                        pathRelease(path);
                        path = .off;
                        m.startModify(x.node, x.input, x.env, out, &cont);
                        input = x.input;
                        env = x.env;
                        path = .{ .at = value.emptyArray() };
                        node = x.node.kind.assign.lhs;
                        continue :loop .eval;
                    },
                    .mark_bad => |x| {
                        pathRelease(path);
                        defer release(x.at);
                        if (identical(out, x.at)) {
                            path = x.path;
                        } else {
                            pathRelease(x.path);
                            path = .bad;
                        }
                        continue :loop .emit;
                    },
                }
            },

            // Back to the last choice for its next output.
            .backtrack => {
                const ch = m.choices.pop() orelse return null;
                cont = ch.cont;
                switch (ch.kind) {
                    .comma => |x| {
                        node = x.node;
                        input = x.input;
                        path = x.path;
                        env = x.env;
                        continue :loop .eval;
                    },
                    .iterate => |x| {
                        const items: []const Value = switch (x.container) {
                            .array => |a| a.items.items,
                            .object => |o| o.map.values(),
                            else => unreachable,
                        };
                        const key: Value = switch (x.container) {
                            .array => value.number(@floatFromInt(x.index)),
                            .object => |o| if (x.path != .off) value.string(o.map.keys()[x.index]) else .null,
                            else => unreachable,
                        };
                        defer release(key);
                        out = retain(items[x.index]);
                        path = m.extend(pathRetain(x.path), key, x.container) catch unreachable;
                        if (x.index + 1 < items.len) {
                            m.push(contRetain(cont), .{ .iterate = .{ .container = x.container, .index = x.index + 1, .path = x.path } });
                        } else {
                            release(x.container);
                            pathRelease(x.path);
                        }
                        continue :loop .emit;
                    },
                    .range => |x| {
                        const current = x.current + x.step;
                        if ((x.step > 0 and current < x.end) or (x.step < 0 and current > x.end)) {
                            m.push(contRetain(cont), .{ .range = .{ .current = current, .end = x.end, .step = x.step } });
                            out = value.number(current);
                            path = .off;
                            continue :loop .emit;
                        }
                        contRelease(cont);
                        continue :loop .backtrack;
                    },
                    .collect_end => |s| {
                        out = value.take(&s.acc);
                        stateRelease(s);
                        path = .off;
                        continue :loop .emit;
                    },
                    .reduce_end => |s| {
                        out = value.take(&s.acc);
                        stateRelease(s);
                        path = .off;
                        continue :loop .emit;
                    },
                    .foreach_end, .limit_mark => |s| {
                        stateRelease(s);
                        contRelease(cont);
                        continue :loop .backtrack;
                    },
                    .try_mark => |s| {
                        stateRelease(s);
                        contRelease(cont);
                        continue :loop .backtrack;
                    },
                    .try_rearm => |s| {
                        s.flag = true;
                        stateRelease(s);
                        contRelease(cont);
                        continue :loop .backtrack;
                    },
                    .alt_end => |x| {
                        const found = x.state.flag;
                        stateRelease(x.state);
                        if (!found) {
                            node = x.node;
                            input = x.input;
                            path = x.path;
                            env = x.env;
                            continue :loop .eval;
                        }
                        release(x.input);
                        pathRelease(x.path);
                        envRelease(x.env);
                        contRelease(cont);
                        continue :loop .backtrack;
                    },
                    .recurse_more => |x| {
                        if (x.f) |f| {
                            cont = newCont(.{ .recurse = .{ .f = f, .env = envRetain(x.env) } }, cont);
                            node = f;
                            input = x.value;
                            path = x.path;
                            env = x.env;
                            continue :loop .eval;
                        }
                        // `..`: the items of the value, each recursed into.
                        const has_items = switch (x.value) {
                            .array => |a| a.items.items.len > 0,
                            .object => |o| o.map.count() > 0,
                            else => false,
                        };
                        if (!has_items) {
                            release(x.value);
                            pathRelease(x.path);
                            contRelease(cont);
                            continue :loop .backtrack;
                        }
                        const rc = newCont(.{ .recurse = .{ .f = null, .env = null } }, cont);
                        m.startIterate(x.value, x.path, rc, true) catch continue :loop .raise;
                        continue :loop .backtrack;
                    },
                    .inputs => {
                        const v = ctx.input.next() catch {
                            contRelease(cont);
                            _ = ctx.fail("{s}", .{ctx.input.message}) catch {};
                            continue :loop .raise;
                        };
                        if (v) |x| {
                            m.push(contRetain(cont), .inputs);
                            out = x;
                            path = .off;
                            continue :loop .emit;
                        }
                        contRelease(cont);
                        continue :loop .backtrack;
                    },
                    .modify_end => |s| {
                        defer stateRelease(s);
                        var acc = value.take(&s.acc);
                        if (s.dels == .array and s.dels.array.items.items.len > 0) {
                            builtin.delPaths(ctx, &acc, s.dels) catch {
                                release(acc);
                                contRelease(cont);
                                continue :loop .raise;
                            };
                        }
                        out = acc;
                        path = .off;
                        continue :loop .emit;
                    },
                    .modify_first => |x| {
                        // The update gave nothing: the path is deleted at the end.
                        value.uniqueArray(&x.state.dels).items.append(gpa, x.path) catch oom();
                        stateRelease(x.state);
                        contRelease(cont);
                        continue :loop .backtrack;
                    },
                    .any_all_end => |s| {
                        out = value.boolean(s.flag);
                        stateRelease(s);
                        path = .off;
                        continue :loop .emit;
                    },
                }
            },

            // An error, its value in `ctx.err`: back to the last `?` waiting for one.
            .raise => {
                while (m.choices.pop()) |ch| {
                    if (ch.kind == .try_mark and ch.kind.try_mark.flag) {
                        choiceRelease(ch);
                        value.replace(&ctx.err, .null);
                        continue :loop .backtrack;
                    }
                    choiceRelease(ch);
                }
                return error.Raised;
            },
        }
    }

    /// Starts giving the items of `container` (taken, with `path`) to `cont` (taken), through a
    /// choice; the caller goes back to it.
    /// `opt`: what cannot be iterated gives nothing.
    fn startIterate(m: *Machine, container: Value, path: Path, cont: ?*Cont, opt: bool) Raised!void {
        if (path == .bad) {
            defer release(container);
            contRelease(cont);
            return m.invalidPath(container);
        }
        const len = switch (container) {
            .array => |a| a.items.items.len,
            .object => |o| o.map.count(),
            else => {
                defer release(container);
                pathRelease(path);
                contRelease(cont);
                if (opt) return;
                return m.ctx.fail("Cannot iterate over {f}", .{builtin.shown(container)});
            },
        };
        if (len == 0) {
            release(container);
            pathRelease(path);
            contRelease(cont);
            return;
        }
        m.push(cont, .{ .iterate = .{ .container = container, .index = 0, .path = path } });
    }

    const Next = enum { eval, emit, backtrack };

    /// A node whose value arguments are all there (taken): sets the registers for what follows.
    /// For `.emit`, the output is left in `input`.
    fn applyArgs(m: *Machine, node: *Node, vals: []Value, input: *Value, path: *Path, env: *?*Env, cont: *?*Cont) Raised!Next {
        const ctx = &m.ctx;
        errdefer {
            release(input.*);
            pathRelease(path.*);
            envRelease(env.*);
            contRelease(cont.*);
            input.* = .null;
            path.* = .off;
        }
        switch (node.kind) {
            .native => |x| {
                // The arguments were worked out from the last; the function takes them in order.
                std.mem.reverse(Value, vals);
                defer for (vals) |v| release(v);
                const v = try builtin.call(ctx, x.id, x.format, input.*, vals);
                release(input.*);
                input.* = v;
                envRelease(env.*);
                env.* = null;
                if (path.* != .off) {
                    pathRelease(path.*);
                    path.* = .bad;
                }
                return .emit;
            },
            .range => {
                defer for (vals) |v| release(v);
                for (vals) |v| if (!value.isNumber(v)) return ctx.fail("Range bounds must be numeric", .{});
                const from = if (vals.len == 1) value.number(0) else vals[0];
                const end = value.toDouble(if (vals.len == 1) vals[0] else vals[1]);
                const step: f64 = if (vals.len == 3) value.toDouble(vals[2]) else 1;
                release(input.*);
                envRelease(env.*);
                env.* = null;
                const first = value.toDouble(from);
                if (!((step > 0 and first < end) or (step < 0 and first > end))) {
                    pathRelease(path.*);
                    path.* = .off;
                    contRelease(cont.*);
                    input.* = .null;
                    return .backtrack;
                }
                m.push(contRetain(cont.*), .{ .range = .{ .current = first, .end = end, .step = step } });
                input.* = retain(from);
                if (path.* != .off) {
                    pathRelease(path.*);
                    path.* = .bad;
                }
                return .emit;
            },
            .index => |x| {
                const key = vals[0];
                if (x.a.kind == .identity) {
                    defer release(key);
                    if (path.* == .bad) return m.invalidPath(input.*);
                    const v = builtin.index(ctx, input.*, key) catch |err| {
                        if (!x.opt) return err;
                        value.replace(&ctx.err, .null);
                        release(input.*);
                        input.* = .null;
                        pathRelease(path.*);
                        path.* = .off;
                        envRelease(env.*);
                        env.* = null;
                        contRelease(cont.*);
                        return .backtrack;
                    };
                    const p = m.extend(path.*, key, input.*) catch |err| {
                        release(v);
                        path.* = .off;
                        return err;
                    };
                    path.* = p;
                    release(input.*);
                    input.* = v;
                    envRelease(env.*);
                    env.* = null;
                    return .emit;
                }
                cont.* = newCont(.{ .index_apply = .{ .key = key, .opt = x.opt } }, cont.*);
                m.pending_node = x.a;
                return .eval;
            },
            .slice => |s| {
                var bounds: [2]Value = .{ .null, .null };
                var at: usize = 0;
                if (s.from != null) {
                    bounds[0] = vals[at];
                    at += 1;
                }
                if (s.to != null) bounds[1] = vals[at];
                cont.* = newCont(.{ .slice_apply = .{ .bounds = bounds, .opt = s.opt } }, cont.*);
                m.pending_node = s.target;
                return .eval;
            },
            .getpath => {
                const p = vals[0];
                defer release(p);
                const v = try builtin.getPath(ctx, input.*, p);
                if (path.* != .off) {
                    var current = path.*;
                    path.* = .off;
                    for (p.array.items.items) |key| {
                        current = m.extend(current, key, input.*) catch |err| {
                            release(v);
                            return err;
                        };
                    }
                    path.* = current;
                }
                release(input.*);
                input.* = v;
                envRelease(env.*);
                env.* = null;
                return .emit;
            },
            .limit => |x| {
                const n = vals[0];
                defer release(n);
                if (value.isNumber(n)) {
                    const d = value.toDouble(n);
                    if (d > 0) {
                        const s = newState(.{ .limit = d, .mark = m.choices.items.len });
                        m.push(null, .{ .limit_mark = s });
                        cont.* = newCont(.{ .limit = stateRetain(s) }, cont.*);
                        m.pending_node = x.b;
                        return .eval;
                    }
                    if (d == 0) {
                        release(input.*);
                        input.* = .null;
                        pathRelease(path.*);
                        path.* = .off;
                        envRelease(env.*);
                        env.* = null;
                        contRelease(cont.*);
                        return .backtrack;
                    }
                    return ctx.fail("limit doesn't support negative count", .{});
                }
                return ctx.fail("Invalid limit: {f}", .{builtin.shown(n)});
            },
            else => unreachable,
        }
    }

    /// What follows a builder: its finished value, or a node to evaluate for its next part
    /// (the frame waiting for it already on `cont`).
    const Built = union(enum) {
        emit: Value,
        eval: struct { node: *Node, input: Value, env: ?*Env },
    };

    /// Builds an object from entry `index` on, its key given when `value_stage`; what is
    /// simple is worked out at once. Takes `env`, `input`, `partial`, `key` and, on an error,
    /// `cont`.
    fn buildObject(m: *Machine, node: *Node, env: ?*Env, input: Value, index: u32, partial_: Value, key_: Value, value_stage_: bool, cont: *?*Cont) Raised!Built {
        const entries = node.kind.object;
        var i = index;
        var partial = partial_;
        var key = key_;
        var value_stage = value_stage_;
        errdefer {
            release(partial);
            release(key);
            release(input);
            envRelease(env);
            contRelease(cont.*);
        }
        while (i < entries.len) {
            const e = entries[i];
            if (!value_stage) {
                if (!e.key.simple) {
                    cont.* = newCont(.{ .object = .{ .node = node, .env = env, .input = input, .index = i, .partial = partial, .key = .null, .value_stage = false } }, cont.*);
                    return .{ .eval = .{ .node = e.key, .input = retain(input), .env = envRetain(env) } };
                }
                key = try m.evalSimple(e.key, retain(input), env);
                value_stage = true;
            }
            if (!e.value.simple) {
                cont.* = newCont(.{ .object = .{ .node = node, .env = env, .input = input, .index = i, .partial = partial, .key = key, .value_stage = true } }, cont.*);
                return .{ .eval = .{ .node = e.value, .input = retain(input), .env = envRetain(env) } };
            }
            const v = try m.evalSimple(e.value, retain(input), env);
            try m.objectPut(&partial, key, v);
            release(key);
            key = .null;
            value_stage = false;
            i += 1;
        }
        release(input);
        envRelease(env);
        return .{ .emit = partial };
    }

    /// Builds a text from its parts before `index`, the last first (the outer loop), their
    /// values in `vals`. Takes `env`, `input`, `vals` and, on an error, `cont`.
    fn buildText(m: *Machine, node: *Node, env: ?*Env, input: Value, index: u32, vals_: Value, cont: *?*Cont) Raised!Built {
        const t = node.kind.text;
        var vals = vals_;
        errdefer {
            release(vals);
            release(input);
            envRelease(env);
            contRelease(cont.*);
        }
        var i = index;
        while (i > 0) {
            i -= 1;
            const e = switch (t.parts[i]) {
                .literal => continue,
                .expr => |e| e,
            };
            if (!e.simple) {
                cont.* = newCont(.{ .text = .{ .node = node, .env = env, .input = input, .index = i, .vals = vals } }, cont.*);
                return .{ .eval = .{ .node = e, .input = retain(input), .env = envRetain(env) } };
            }
            const v = try m.evalSimple(e, retain(input), env);
            value.replace(&value.uniqueArray(&vals).items.items[i], v);
        }
        release(input);
        envRelease(env);
        const items = value.uniqueArray(&vals).items.items;
        const result = try m.joinText(t.parts, t.format, items);
        release(vals);
        return .{ .emit = result };
    }

    fn objectPut(m: *Machine, partial: *Value, key: Value, v: Value) Raised!void {
        if (key != .string) {
            release(v);
            return m.ctx.fail("Object keys must be strings", .{});
        }
        value.objectPut(value.uniqueObject(partial), key.string.bytes.items, v);
    }

    /// Starts an assignment: its state, the choice that gives the result at the end, and the
    /// frame that takes each path of the left side.
    fn startModify(m: *Machine, node: *Node, input: Value, env: ?*Env, operand: Value, cont: *?*Cont) void {
        const how = node.kind.assign.how;
        const s = newState(.{ .acc = retain(input), .operand = operand, .dels = value.emptyArray(), .how = how });
        m.push(cont.*, .{ .modify_end = s });
        cont.* = newCont(.{ .modify_path = .{ .node = node, .state = stateRetain(s), .env = envRetain(env) } }, null);
    }

    /// One path of an assignment's left side; false when the update is a filter that must
    /// still run.
    fn modifyAt(m: *Machine, s: *State, p: Value, update: *Node, env: ?*Env) Raised!bool {
        const ctx = &m.ctx;
        const keys = p.array.items.items;
        switch (s.how) {
            .delete => {
                value.uniqueArray(&s.dels).items.append(gpa, retain(p)) catch oom();
                return true;
            },
            .set => {
                try builtin.setPath(ctx, &s.acc, keys, retain(s.operand));
                return true;
            },
            .arith => |op| {
                const old = try builtin.takePath(ctx, &s.acc, keys);
                const new = try builtin.arith(ctx, op, old, retain(s.operand));
                try builtin.setPath(ctx, &s.acc, keys, new);
                return true;
            },
            .alternative => {
                const old = try builtin.takePath(ctx, &s.acc, keys);
                const new = if (value.truthy(old)) old else blk: {
                    release(old);
                    break :blk retain(s.operand);
                };
                try builtin.setPath(ctx, &s.acc, keys, new);
                return true;
            },
            .update => {
                if (!update.simple) return false;
                const old = try builtin.takePath(ctx, &s.acc, keys);
                const new = try m.evalSimple(update, old, env);
                try builtin.setPath(ctx, &s.acc, keys, new);
                return true;
            },
        }
    }

    /// A simple node worked out at once: its one output, or an error. It takes `input`, so
    /// that the last use of a value held once (`. + [$x]` in `reduce`) holds it once still.
    pub fn evalSimple(m: *Machine, node: *Node, input: Value, env: ?*Env) Raised!Value {
        const ctx = &m.ctx;
        switch (node.kind) {
            .identity => return input,
            .literal => |v| {
                release(input);
                return retain(v);
            },
            .variable => |up| {
                release(input);
                return retain(lookup(env, up).?.value);
            },
            .index => |x| {
                const key = m.evalSimple(x.b, retain(input), env) catch |err| {
                    release(input);
                    return err;
                };
                defer release(key);
                const t = try m.evalSimple(x.a, input, env);
                defer release(t);
                return builtin.index(ctx, t, key);
            },
            .slice => |s| {
                var to: Value = .null;
                var from: Value = .null;
                defer release(to);
                defer release(from);
                {
                    errdefer release(input);
                    if (s.to) |n| to = try m.evalSimple(n, retain(input), env);
                    if (s.from) |n| from = try m.evalSimple(n, retain(input), env);
                }
                const t = try m.evalSimple(s.target, input, env);
                defer release(t);
                return builtin.slice(ctx, t, from, to);
            },
            .negate => |a| {
                const v = try m.evalSimple(a, input, env);
                defer release(v);
                return builtin.negate(ctx, v);
            },
            .pipe => |x| {
                const v = try m.evalSimple(x.a, input, env);
                return m.evalSimple(x.b, v, env);
            },
            .alternative => |x| {
                const v = m.evalSimple(x.a, retain(input), env) catch |err| {
                    release(input);
                    return err;
                };
                if (value.truthy(v)) {
                    release(input);
                    return v;
                }
                release(v);
                return m.evalSimple(x.b, input, env);
            },
            .and_, .or_ => |x| {
                const is_and = node.kind == .and_;
                const a = m.evalSimple(x.a, retain(input), env) catch |err| {
                    release(input);
                    return err;
                };
                const t = value.truthy(a);
                release(a);
                if (t != is_and) {
                    release(input);
                    return value.boolean(t);
                }
                const b = try m.evalSimple(x.b, input, env);
                defer release(b);
                return value.boolean(value.truthy(b));
            },
            .arith => |x| {
                const r = m.evalSimple(x.b, retain(input), env) catch |err| {
                    release(input);
                    return err;
                };
                const l = m.evalSimple(x.a, input, env) catch |err| {
                    release(r);
                    return err;
                };
                return builtin.arith(ctx, x.op, l, r);
            },
            .if_ => |x| {
                const c = m.evalSimple(x.cond, retain(input), env) catch |err| {
                    release(input);
                    return err;
                };
                defer release(c);
                return m.evalSimple(if (value.truthy(c)) x.then else x.otherwise, input, env);
            },
            .bind => |x| {
                const v = m.evalSimple(x.a, retain(input), env) catch |err| {
                    release(input);
                    return err;
                };
                const e = newEnv(envRetain(env), v);
                defer envRelease(e);
                return m.evalSimple(x.b, input, e);
            },
            .array => |f| {
                const result = value.emptyArray();
                if (f) |body| {
                    const v = m.evalSimple(body, input, env) catch |err| {
                        release(result);
                        return err;
                    };
                    result.array.items.append(gpa, v) catch oom();
                } else release(input);
                return result;
            },
            .map_simple => |f| {
                defer release(input);
                const items: []const Value = switch (input) {
                    .array => |a| a.items.items,
                    .object => |o| o.map.values(),
                    else => return ctx.fail("Cannot iterate over {f}", .{builtin.shown(input)}),
                };
                var list: std.ArrayList(Value) = .empty;
                list.ensureTotalCapacityPrecise(gpa, items.len) catch oom();
                for (items) |item| {
                    const v = m.evalSimple(f, retain(item), env) catch |err| {
                        for (list.items) |done| release(done);
                        list.deinit(gpa);
                        return err;
                    };
                    list.appendAssumeCapacity(v);
                }
                return value.arrayFromList(list);
            },
            .object => |entries| {
                defer release(input);
                var result = value.emptyObject();
                errdefer release(result);
                value.reserve(&result.object.map, entries.len);
                for (entries) |e| {
                    const k = try m.evalSimple(e.key, retain(input), env);
                    defer release(k);
                    if (k != .string) return ctx.fail("Object keys must be strings", .{});
                    const v = try m.evalSimple(e.value, retain(input), env);
                    try m.objectPut(&result, k, v);
                }
                return result;
            },
            .text => |t| {
                defer release(input);
                const vals = gpa.alloc(Value, t.parts.len) catch oom();
                defer gpa.free(vals);
                @memset(vals, .null);
                var i = t.parts.len;
                while (i > 0) {
                    i -= 1;
                    switch (t.parts[i]) {
                        .literal => {},
                        .expr => |e| vals[i] = m.evalSimple(e, retain(input), env) catch |err| {
                            for (vals) |v| release(v);
                            return err;
                        },
                    }
                }
                return m.joinText(t.parts, t.format, vals);
            },
            .native => |x| {
                defer release(input);
                // The arguments from the last, the outer loop, as jq's C functions take them.
                var vals: [3]Value = undefined;
                var first = x.args.len;
                defer for (vals[first..x.args.len]) |v| release(v);
                while (first > 0) {
                    vals[first - 1] = try m.evalSimple(x.args[first - 1], retain(input), env);
                    first -= 1;
                }
                return builtin.call(ctx, x.id, x.format, input, vals[0..x.args.len]);
            },
            else => unreachable,
        }
    }

    /// The text of a `"...\(f)..."` from its parts and the values of its expressions (taken).
    fn joinText(m: *Machine, parts: []const parse.Part, format: ?parse.Format, vals: []Value) Raised!Value {
        var list: std.ArrayList(u8) = .empty;
        var i: usize = 0;
        errdefer {
            list.deinit(gpa);
            for (vals[i..]) |v| release(v);
        }
        while (i < parts.len) : (i += 1) {
            switch (parts[i]) {
                .literal => |s| list.appendSlice(gpa, s) catch oom(),
                .expr => {
                    const v = vals[i];
                    vals[i] = .null;
                    defer release(v);
                    const s = if (format) |f| try builtin.format(&m.ctx, f, v) else value.toText(retain(v));
                    defer release(s);
                    list.appendSlice(gpa, s.string.bytes.items) catch oom();
                },
            }
        }
        return value.stringFromList(list);
    }
};

/// Whether two values are the same one, as jq's `jv_identical` tells: the same scalar, or the
/// same text, array or object in memory.
fn identical(a: Value, b: Value) bool {
    return switch (a) {
        .null => b == .null,
        .false => b == .false,
        .true => b == .true,
        .number => |d| b == .number and @as(u64, @bitCast(d)) == @as(u64, @bitCast(b.number)),
        .literal => |l| b == .literal and l == b.literal,
        .string => |x| b == .string and x == b.string,
        .array => |x| b == .array and x == b.array,
        .object => |x| b == .object and x == b.object,
    };
}

/// The key a slice stands for in a path: `{"start": from, "end": to}`.
fn sliceKey(from: Value, to: Value) Value {
    const k = value.emptyObject();
    value.objectPut(k.object, "start", retain(from));
    value.objectPut(k.object, "end", retain(to));
    return k;
}
