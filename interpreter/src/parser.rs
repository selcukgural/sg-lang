//! Hand-written recursive descent parser.

use crate::ast::*;
use crate::diag::{Diag, Span};
use crate::lexer::{lex, Part, Tok, Token};
use crate::types::IntTy;

/// Words that are in the design but not in the interpreter yet. They get an explicit
/// message instead of "unknown name".
const NOT_YET: &[&str] = &[
    "interface", "use", "shared", "weak", "checked", "locked", "raw", "unsafe", "lock",
    "spawn", "share", "using", "readonly",
];

type Res<T> = Result<T, Diag>;

pub fn parse(src: &str) -> Res<Program> {
    let toks = lex(src, Span::new(0, src.len()), false)?;
    let mut parser = Parser { src, toks, i: 0, struct_literals: true };
    parser.program()
}

struct Parser<'a> {
    src: &'a str,
    toks: Vec<Token>,
    i: usize,
    /// Off while parsing a condition: there `x {` starts the block, not a struct literal.
    struct_literals: bool,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> &Tok {
        &self.toks[self.i].tok
    }

    fn peek_at(&self, n: usize) -> &Tok {
        &self.toks[(self.i + n).min(self.toks.len() - 1)].tok
    }

    fn span(&self) -> Span {
        self.toks[self.i].span
    }

    fn prev_end(&self) -> usize {
        if self.i == 0 {
            0
        } else {
            self.toks[self.i - 1].span.end
        }
    }

    fn bump(&mut self) -> Token {
        let token = self.toks[self.i].clone();
        if self.i < self.toks.len() - 1 {
            self.i += 1;
        }
        token
    }

    fn eat(&mut self, tok: &Tok) -> bool {
        if self.peek() == tok {
            self.bump();
            true
        } else {
            false
        }
    }

    fn skip_newlines(&mut self) {
        while *self.peek() == Tok::Newline {
            self.bump();
        }
    }

    fn unexpected(&self, wanted: &str) -> Diag {
        let diag = Diag::error(self.span(), format!("expected {wanted}, found {}", self.peek().describe()));
        match self.peek() {
            Tok::Ident(name) if NOT_YET.contains(&name.as_str()) => {
                diag.note(format!("`{name}` is not in the interpreter yet"))
            }
            _ => diag,
        }
    }

    fn expect(&mut self, tok: Tok, wanted: &str) -> Res<Span> {
        if *self.peek() == tok {
            Ok(self.bump().span)
        } else {
            Err(self.unexpected(wanted))
        }
    }

    fn ident(&mut self, wanted: &str) -> Res<(String, Span)> {
        match self.peek() {
            Tok::Ident(name) if !NOT_YET.contains(&name.as_str()) => {
                let name = name.clone();
                Ok((name, self.bump().span))
            }
            _ => Err(self.unexpected(wanted)),
        }
    }

    /// `[read | ref] Name [<Type, ...>] { * | ? }`
    fn type_expr(&mut self) -> Res<TypeExpr> {
        let start = self.span();
        let mode = match self.peek() {
            Tok::Read => Some(Mode::Read),
            Tok::Ref => Some(Mode::Ref),
            _ => None,
        };
        if mode.is_some() {
            self.bump();
        }
        let (name, _) = self.ident("a type name")?;
        let mut args = Vec::new();
        if self.eat(&Tok::Lt) {
            loop {
                args.push(self.type_expr()?);
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
            self.expect(Tok::Gt, "`>`")?;
        }
        let mut suffixes = Vec::new();
        loop {
            match self.peek() {
                Tok::Star => suffixes.push(Suffix::Pointer),
                Tok::Question => suffixes.push(Suffix::Nullable),
                _ => break,
            }
            self.bump();
        }
        Ok(TypeExpr { mode, name, args, suffixes, span: Span::new(start.start, self.prev_end()) })
    }

    fn program(&mut self) -> Res<Program> {
        let mut program = Program { structs: Vec::new(), enums: Vec::new(), funcs: Vec::new() };
        self.skip_newlines();
        while *self.peek() != Tok::Eof {
            // One module for now, so `public` changes nothing yet.
            self.eat(&Tok::Public);
            if *self.peek() == Tok::Struct {
                program.structs.push(self.struct_decl()?);
            } else if *self.peek() == Tok::Enum {
                program.enums.push(self.enum_decl()?);
            } else {
                let ret = self.type_expr().map_err(|d| {
                    d.hint("only structs, enums and functions are written at the top level: `void main() { ... }`")
                })?;
                let (name, name_span) = self.ident("a function name")?;
                program.funcs.push(self.func(ret, name, name_span, false)?);
            }
            if *self.peek() != Tok::Eof {
                self.expect(Tok::Newline, "end of line")?;
            }
            self.skip_newlines();
        }
        Ok(program)
    }

    fn struct_decl(&mut self) -> Res<StructDecl> {
        self.bump();
        let (name, name_span) = self.ident("a struct name")?;
        let interfaces = self.interfaces()?;
        self.open_brace()?;
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        self.skip_newlines();
        while *self.peek() != Tok::RBrace {
            if *self.peek() == Tok::Eof {
                return Err(self.unexpected("`}`"));
            }
            self.eat(&Tok::Public);
            let ty = self.type_expr()?;
            let (member, span) = self.ident("a field or method name")?;
            if *self.peek() == Tok::LParen {
                methods.push(self.func(ty, member, span, true)?);
            } else {
                fields.push(FieldDecl { ty, name: member, span });
            }
            if *self.peek() != Tok::RBrace {
                self.expect(Tok::Newline, "end of line")?;
            }
            self.skip_newlines();
        }
        self.bump();
        Ok(StructDecl { name, name_span, interfaces, fields, methods })
    }

    /// `: Copyable, Formattable` after the name of a struct or an enum.
    fn interfaces(&mut self) -> Res<Vec<(String, Span)>> {
        let mut list = Vec::new();
        if self.eat(&Tok::Colon) {
            loop {
                list.push(self.ident("an interface name")?);
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
        }
        Ok(list)
    }

    /// `enum Shape { Circle(int radius)` / `Empty }`: one variant per line.
    fn enum_decl(&mut self) -> Res<EnumDecl> {
        self.bump();
        let (name, name_span) = self.ident("an enum name")?;
        let interfaces = self.interfaces()?;
        self.open_brace()?;
        let mut variants = Vec::new();
        self.skip_newlines();
        while *self.peek() != Tok::RBrace {
            if *self.peek() == Tok::Eof {
                return Err(self.unexpected("`}`"));
            }
            let (variant, span) = self.ident("a variant name")?;
            let mut fields = Vec::new();
            if *self.peek() == Tok::LParen {
                let open = self.bump().span;
                while *self.peek() != Tok::RParen {
                    let ty = self.type_expr()?;
                    let (field, span) = self.ident("a field name")?;
                    fields.push(FieldDecl { ty, name: field, span });
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                let close = self.expect(Tok::RParen, "`)`")?;
                if fields.is_empty() {
                    return Err(Diag::error(open.to(close), "a variant that carries nothing is written without parentheses")
                        .hint(format!("`{variant}`")));
                }
            }
            variants.push(VariantDecl { name: variant, span, fields });
            if *self.peek() == Tok::Comma {
                return Err(Diag::error(self.span(), "variants are separated by line ends, not commas")
                    .hint("write each variant on its own line"));
            }
            if *self.peek() != Tok::RBrace {
                self.expect(Tok::Newline, "end of line")?;
            }
            self.skip_newlines();
        }
        self.bump();
        Ok(EnumDecl { name, name_span, interfaces, variants })
    }

    /// The rest of a function after its return type and name. In a struct body the first
    /// parameter may be `this`, `read this` or `ref this`.
    fn func(&mut self, ret: TypeExpr, name: String, name_span: Span, in_struct: bool) -> Res<Func> {
        self.expect(Tok::LParen, "`(`")?;
        let mut this = None;
        let mut params: Vec<Param> = Vec::new();
        while *self.peek() != Tok::RParen {
            let start = self.span();
            let is_this = |tok: &Tok| matches!(tok, Tok::Ident(word) if word == "this");
            let mode = match self.peek() {
                tok if is_this(tok) => Some(This::Value),
                Tok::Read if is_this(self.peek_at(1)) => Some(This::Read),
                Tok::Ref if is_this(self.peek_at(1)) => Some(This::Ref),
                _ => None,
            };
            if let Some(mode) = mode {
                if mode != This::Value {
                    self.bump();
                }
                let span = start.to(self.bump().span);
                if !in_struct {
                    return Err(Diag::error(span, "`this` can only be a parameter of a method")
                        .hint("a method is written inside the body of its struct"));
                }
                if this.is_some() || !params.is_empty() {
                    return Err(Diag::error(span, "`this` must be the first parameter"));
                }
                this = Some(mode);
            } else {
                let ty = self.type_expr()?;
                let (name, span) = self.ident("a parameter name")?;
                if name == "this" {
                    return Err(Diag::error(start.to(span), "`this` with a type is not in the interpreter yet")
                        .hint("write `this`, `read this` or `ref this`"));
                }
                params.push(Param { ty, name, span });
            }
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(Tok::RParen, "`)`")?;
        let body = self.block()?;
        Ok(Func { ret, name, name_span, this, params, body })
    }

    fn open_brace(&mut self) -> Res<()> {
        if *self.peek() == Tok::Newline && *self.peek_at(1) == Tok::LBrace {
            return Err(Diag::error(self.toks[self.i + 1].span, "`{` cannot start a new line")
                .hint("put `{` at the end of the previous line"));
        }
        self.expect(Tok::LBrace, "`{`")?;
        Ok(())
    }

    fn block(&mut self) -> Res<Block> {
        self.open_brace()?;
        let outer = std::mem::replace(&mut self.struct_literals, true);
        let mut stmts = Vec::new();
        self.skip_newlines();
        while *self.peek() != Tok::RBrace {
            if *self.peek() == Tok::Eof {
                return Err(self.unexpected("`}`"));
            }
            stmts.push(self.stmt()?);
            if *self.peek() != Tok::RBrace {
                self.expect(Tok::Newline, "end of line")?;
            }
            self.skip_newlines();
        }
        self.bump();
        self.struct_literals = outer;
        Ok(stmts)
    }

    /// A condition or range bound: a struct literal must be parenthesized here.
    fn head_expr(&mut self, range_bound: bool) -> Res<Expr> {
        let outer = std::mem::replace(&mut self.struct_literals, false);
        let result = if range_bound { self.binary(ADDITIVE) } else { self.expr() };
        self.struct_literals = outer;
        result
    }

    /// How many tokens the type written `n` tokens ahead takes: a name, `<...>`, any `*` and
    /// `?`. `None` if no type is written there.
    fn type_len(&self, start: usize) -> Option<usize> {
        if !matches!(self.peek_at(start), Tok::Ident(_)) {
            return None;
        }
        let mut n = start + 1;
        if *self.peek_at(n) == Tok::Lt {
            loop {
                n += 1;
                if matches!(self.peek_at(n), Tok::Read | Tok::Ref) {
                    n += 1;
                }
                n += self.type_len(n)?;
                match self.peek_at(n) {
                    Tok::Comma => {}
                    Tok::Gt => break,
                    _ => return None,
                }
            }
            n += 1;
        }
        while matches!(self.peek_at(n), Tok::Star | Tok::Question) {
            n += 1;
        }
        Some(n - start)
    }

    /// The type comes first: `int x = 1`, `Node*? list = null`, `Result<int, E> r = f()`. A
    /// type and then a name is a declaration.
    fn at_declaration(&self) -> bool {
        self.type_len(0).is_some_and(|n| matches!(self.peek_at(n), Tok::Ident(_)))
    }

    /// Whether a declaration starts here and is followed by `=`.
    fn at_initialized_declaration(&self) -> bool {
        self.at_declaration() && self.type_len(0).is_some_and(|n| *self.peek_at(n + 1) == Tok::Assign)
    }

    /// Whether only line ends stand between here and the `}` that closes the block.
    fn at_block_end(&self) -> bool {
        let mut n = 0;
        while *self.peek_at(n) == Tok::Newline {
            n += 1;
        }
        *self.peek_at(n) == Tok::RBrace
    }

    fn stmt(&mut self) -> Res<Stmt> {
        match self.stmt_or_tail(Tail::Never)? {
            Ok(stmt) => Ok(stmt),
            Err(_) => unreachable!("a tail expression is only read when asked for"),
        }
    }

    /// A statement, or an expression that is the value of a `match` arm (see `Tail`).
    fn stmt_or_tail(&mut self, tail: Tail) -> Res<Result<Stmt, Expr>> {
        let tail_ok = |parser: &Self| match tail {
            Tail::Never => false,
            Tail::LastLine => parser.at_block_end(),
            Tail::Always => true,
        };
        let start = self.span();
        let kind = match self.peek().clone() {
            Tok::Var => {
                self.bump();
                let (name, name_span) = self.ident("a variable name")?;
                self.expect(Tok::Assign, "`=`")?;
                let init = self.expr()?;
                StmtKind::Let { ty: None, name, name_span, init }
            }
            Tok::If => self.if_stmt()?,
            Tok::While => {
                self.bump();
                let cond = self.head_expr(false)?;
                let body = self.block()?;
                StmtKind::While { cond, body }
            }
            Tok::For => {
                self.bump();
                let (var, var_span) = self.ident("a loop variable")?;
                self.expect(Tok::In, "`in`")?;
                let from = self.head_expr(true)?;
                if self.eat(&Tok::DotDot) {
                    let to = self.head_expr(true)?;
                    let body = self.block()?;
                    StmtKind::For { var, var_span, from, to, body }
                } else {
                    let body = self.block()?;
                    StmtKind::ForEach { var, var_span, iter: from, body, by: None }
                }
            }
            Tok::Return => {
                self.bump();
                if matches!(self.peek(), Tok::Newline | Tok::RBrace | Tok::Eof) {
                    StmtKind::Return(None)
                } else {
                    StmtKind::Return(Some(self.expr()?))
                }
            }
            Tok::Break => {
                self.bump();
                StmtKind::Break
            }
            Tok::Continue => {
                self.bump();
                StmtKind::Continue
            }
            Tok::Match => {
                let expr = self.match_expr()?;
                if tail_ok(self) {
                    return Ok(Err(expr));
                }
                StmtKind::Expr(expr)
            }
            Tok::Else => {
                return Err(Diag::error(start, "`else` cannot start a new line")
                    .hint("put `else` on the same line as `}`: `} else {`"));
            }
            Tok::AndAnd | Tok::OrOr | Tok::Plus | Tok::Star | Tok::Slash | Tok::Percent | Tok::Eq | Tok::Ne
            | Tok::Lt | Tok::Le | Tok::Gt | Tok::Ge | Tok::QuestionQuestion => {
                let text = self.peek().describe();
                return Err(Diag::error(start, format!("a line cannot start with {text}"))
                    .hint(format!("the statement ended on the previous line; to continue it, put {text} at the end of that line")));
            }
            _ if self.at_declaration() && (tail == Tail::Never || self.at_initialized_declaration()) => {
                let ty = self.type_expr()?;
                let (name, name_span) = self.ident("a variable name")?;
                if *self.peek() != Tok::Assign {
                    let written = &self.src[ty.span.start..ty.span.end];
                    return Err(self
                        .unexpected("`=`")
                        .hint(format!("a variable is declared with an initial value: `{written} {name} = ...`")));
                }
                self.bump();
                let init = self.expr()?;
                StmtKind::Let { ty: Some(ty), name, name_span, init }
            }
            _ => {
                let expr = self.expr()?;
                if let Some(op) = assign_op(self.peek()) {
                    if !expr.is_place() {
                        return Err(Diag::error(expr.span, "the left side of an assignment must be a variable, a field or an item"));
                    }
                    self.bump();
                    let value = self.expr()?;
                    match (&expr.kind, op) {
                        (ExprKind::Var(name), None) if name == "_" => StmtKind::Discard(value),
                        _ => StmtKind::Assign { target: expr, op, value, through: false },
                    }
                } else if tail_ok(self) {
                    return Ok(Err(expr));
                } else if matches!(
                    expr.kind,
                    ExprKind::Call { .. } | ExprKind::Method { .. } | ExprKind::Match { .. } | ExprKind::Try { .. }
                ) {
                    StmtKind::Expr(expr)
                } else {
                    return Err(Diag::error(expr.span, "the value of this expression is not used")
                        .hint("only a call, an assignment or a declaration can be a statement"));
                }
            }
        };
        Ok(Ok(Stmt { kind, span: Span::new(start.start, self.prev_end()) }))
    }

    /// `match subject { pattern [when condition] => body ... }`: one arm per line.
    fn match_expr(&mut self) -> Res<Expr> {
        let start = self.bump().span;
        let subject = self.head_expr(false)?;
        self.open_brace()?;
        let outer = std::mem::replace(&mut self.struct_literals, true);
        let mut arms = Vec::new();
        self.skip_newlines();
        while *self.peek() != Tok::RBrace {
            if *self.peek() == Tok::Eof {
                return Err(self.unexpected("`}`"));
            }
            let arm_start = self.span();
            let pattern = self.pattern()?;
            let guard = if matches!(self.peek(), Tok::Ident(word) if word == "when") {
                self.bump();
                Some(self.expr()?)
            } else {
                None
            };
            self.expect(Tok::FatArrow, "`=>`")?;
            let (body, value) = match self.peek() {
                Tok::LBrace => self.arm_block()?,
                _ => match self.stmt_or_tail(Tail::Always)? {
                    Ok(stmt) => (vec![stmt], None),
                    Err(value) => (Vec::new(), Some(value)),
                },
            };
            arms.push(Arm { pattern, guard, body, value, span: Span::new(arm_start.start, self.prev_end()) });
            if *self.peek() == Tok::Comma {
                return Err(Diag::error(self.span(), "arms are separated by line ends, not commas")
                    .hint("write each arm on its own line"));
            }
            if *self.peek() != Tok::RBrace {
                self.expect(Tok::Newline, "end of line")?;
            }
            self.skip_newlines();
        }
        self.bump();
        self.struct_literals = outer;
        let span = Span::new(start.start, self.prev_end());
        Ok(Expr { kind: ExprKind::Match { subject: Box::new(subject), arms, borrowed: false }, span })
    }

    /// The block of a `match` arm: statements, and the last line may be the value.
    fn arm_block(&mut self) -> Res<(Block, Option<Expr>)> {
        self.open_brace()?;
        let outer = std::mem::replace(&mut self.struct_literals, true);
        let mut stmts = Vec::new();
        let mut value = None;
        self.skip_newlines();
        while *self.peek() != Tok::RBrace {
            if *self.peek() == Tok::Eof {
                return Err(self.unexpected("`}`"));
            }
            match self.stmt_or_tail(Tail::LastLine)? {
                Ok(stmt) => stmts.push(stmt),
                Err(expr) => value = Some(expr),
            }
            if *self.peek() != Tok::RBrace {
                self.expect(Tok::Newline, "end of line")?;
            }
            self.skip_newlines();
        }
        self.bump();
        self.struct_literals = outer;
        Ok((stmts, value))
    }

    /// `Circle(r)`, `Rect(w, _)`, `Empty`, `_`, `var other`
    fn pattern(&mut self) -> Res<Pattern> {
        let start = self.span();
        let kind = match self.peek().clone() {
            Tok::Var => {
                self.bump();
                PatternKind::Bind(self.ident("a variable name")?.0)
            }
            Tok::Ident(word) if word == "_" => {
                self.bump();
                PatternKind::Wildcard
            }
            Tok::Char(value) => {
                self.bump();
                PatternKind::Char(value)
            }
            Tok::Int(_) | Tok::Minus => {
                let negative = self.eat(&Tok::Minus);
                let Tok::Int(value) = *self.peek() else { return Err(self.unexpected("a number")) };
                let span = start.to(self.bump().span);
                let value = if negative { -(value as i128) } else { value as i128 };
                match i64::try_from(value) {
                    Ok(value) => PatternKind::Int(value),
                    Err(_) => return Err(Diag::error(span, format!("{value} does not fit in any integer type"))),
                }
            }
            Tok::Str(_) | Tok::Interp(_) => {
                return Err(self.unexpected("a pattern").note("patterns for text are not in the interpreter yet; compare with `==`"))
            }
            Tok::Ident(_) => {
                let (name, _) = self.ident("a variant name")?;
                if *self.peek() == Tok::Dot {
                    let diag = Diag::error(start.to(self.span()), "a pattern names the variant alone, without its enum");
                    return Err(match self.peek_at(1) {
                        Tok::Ident(variant) => diag.hint(format!("`{variant}`")),
                        _ => diag,
                    });
                }
                let mut args = None;
                if self.eat(&Tok::LParen) {
                    let mut names = Vec::new();
                    while *self.peek() != Tok::RParen {
                        match self.peek().clone() {
                            Tok::Ident(_) if !matches!(self.peek_at(1), Tok::Comma | Tok::RParen) => {
                                self.bump();
                                return Err(self
                                    .unexpected("`,` or `)`")
                                    .note("a pattern inside a pattern is not in the interpreter yet; name the value and test it in the arm"));
                            }
                            Tok::Ident(word) => {
                                let span = self.bump().span;
                                names.push(((word != "_").then_some(word), span));
                            }
                            _ => {
                                return Err(self.unexpected("a name or `_`").note(
                                    "a pattern inside a pattern is not in the interpreter yet; name the value and test it in the arm",
                                ))
                            }
                        }
                        if !self.eat(&Tok::Comma) {
                            break;
                        }
                    }
                    self.expect(Tok::RParen, "`)`")?;
                    args = Some(names);
                }
                PatternKind::Variant { name, args, index: 0 }
            }
            _ => {
                return Err(self
                    .unexpected("a pattern")
                    .note("patterns for now: a variant such as `Circle(r)`, a number, a character, `_`, or `var name`"))
            }
        };
        Ok(Pattern { kind, span: Span::new(start.start, self.prev_end()) })
    }

    fn if_stmt(&mut self) -> Res<StmtKind> {
        self.bump();
        let cond = self.head_expr(false)?;
        if *self.peek() == Tok::Assign {
            return Err(Diag::error(self.span(), "an assignment cannot be a condition").hint("comparison is written `==`"));
        }
        let then = self.block()?;
        let els = if self.eat(&Tok::Else) {
            if *self.peek() == Tok::If {
                let start = self.span();
                let kind = self.if_stmt()?;
                Some(vec![Stmt { kind, span: Span::new(start.start, self.prev_end()) }])
            } else {
                Some(self.block()?)
            }
        } else {
            None
        };
        Ok(StmtKind::If { cond, then, els })
    }

    /// `a ?? b` binds loosest and groups to the right, as in C#.
    fn expr(&mut self) -> Res<Expr> {
        let left = self.binary(0)?;
        if !self.eat(&Tok::QuestionQuestion) {
            return Ok(left);
        }
        // The right side can leave the function: `x ?? return Error(...)`.
        let right = if *self.peek() == Tok::Return {
            let start = self.bump().span;
            let value = match self.peek() {
                Tok::Newline | Tok::RBrace | Tok::RParen | Tok::Comma | Tok::Eof => None,
                _ => Some(Box::new(self.expr()?)),
            };
            Expr { kind: ExprKind::Return(value), span: Span::new(start.start, self.prev_end()) }
        } else {
            self.expr()?
        };
        let span = left.span.to(right.span);
        Ok(Expr { kind: ExprKind::Coalesce { left: Box::new(left), right: Box::new(right), on_result: false }, span })
    }

    /// Same precedence as C#: `||`, `&&`, equality, comparison and `is`, additive, multiplicative.
    fn binary(&mut self, level: usize) -> Res<Expr> {
        const LEVELS: &[&[(Tok, BinOp)]] = &[
            &[(Tok::OrOr, BinOp::Or)],
            &[(Tok::AndAnd, BinOp::And)],
            &[(Tok::Eq, BinOp::Eq), (Tok::Ne, BinOp::Ne)],
            &[(Tok::Lt, BinOp::Lt), (Tok::Le, BinOp::Le), (Tok::Gt, BinOp::Gt), (Tok::Ge, BinOp::Ge)],
            &[(Tok::Plus, BinOp::Add), (Tok::Minus, BinOp::Sub)],
            &[(Tok::Star, BinOp::Mul), (Tok::Slash, BinOp::Div), (Tok::Percent, BinOp::Rem)],
        ];
        if level == LEVELS.len() {
            return self.unary();
        }
        let mut left = self.binary(level + 1)?;
        loop {
            if level == COMPARISON && *self.peek() == Tok::Is {
                left = self.is_test(left)?;
                continue;
            }
            let Some(&(_, op)) = LEVELS[level].iter().find(|(tok, _)| tok == self.peek()) else {
                return Ok(left);
            };
            self.bump();
            let right = self.binary(level + 1)?;
            let span = left.span.to(right.span);
            left = Expr { kind: ExprKind::Binary(op, Box::new(left), Box::new(right)), span };
        }
    }

    /// `x is null`, `x is not null`, `x is Circle(r)`, `x is not Circle`
    fn is_test(&mut self, operand: Expr) -> Res<Expr> {
        self.bump();
        let negated = matches!(self.peek(), Tok::Ident(word) if word == "not");
        if negated {
            self.bump();
        }
        let start = operand.span.start;
        let expr = Box::new(operand);
        let kind = match self.peek() {
            Tok::Null => {
                self.bump();
                ExprKind::IsNull { expr, negated }
            }
            Tok::Ident(word) if word != "_" => {
                let pattern = self.pattern()?;
                ExprKind::IsVariant { expr, negated, pattern, by_ref: false }
            }
            _ => return Err(self.unexpected("`null` or a variant name").note("`is` tests for `null` or for one variant of an enum")),
        };
        Ok(Expr { kind, span: Span::new(start, self.prev_end()) })
    }

    fn unary(&mut self) -> Res<Expr> {
        let start = self.span();
        let wrap = |kind: fn(Box<Expr>) -> ExprKind, operand: Expr| {
            let span = start.to(operand.span);
            Expr { kind: kind(Box::new(operand)), span }
        };
        match self.peek() {
            Tok::Minus => {
                self.bump();
                // `-2147483648` is one literal; read separately, 2147483648 would not fit in int.
                if let Tok::Int(value) = *self.peek() {
                    if !matches!(self.peek_at(1), Tok::Dot) {
                        let span = start.to(self.bump().span);
                        return int_literal(-(value as i128), span);
                    }
                }
                let operand = self.unary()?;
                Ok(wrap(|e| ExprKind::Unary(UnOp::Neg, e), operand))
            }
            Tok::Bang => {
                self.bump();
                let operand = self.unary()?;
                Ok(wrap(|e| ExprKind::Unary(UnOp::Not, e), operand))
            }
            Tok::New => {
                self.bump();
                let operand = self.unary()?;
                Ok(wrap(ExprKind::New, operand))
            }
            // A prefix, like `await`: `try a().b()` tries the whole chain.
            Tok::Try => {
                self.bump();
                let operand = self.unary()?;
                Ok(wrap(|e| ExprKind::Try { expr: e, kind: TryKind::Null }, operand))
            }
            Tok::Read => {
                self.bump();
                let operand = self.unary()?;
                Ok(wrap(|e| ExprKind::Borrow { mutable: false, expr: e }, operand))
            }
            Tok::Ref => {
                self.bump();
                let operand = self.unary()?;
                Ok(wrap(|e| ExprKind::Borrow { mutable: true, expr: e }, operand))
            }
            // `(int)x`: a number type name alone in parentheses is a cast.
            Tok::LParen => match self.cast_type() {
                Some(ty) => {
                    self.bump();
                    self.bump();
                    self.bump();
                    let operand = self.unary()?;
                    let span = start.to(operand.span);
                    Ok(Expr { kind: ExprKind::Cast { ty, expr: Box::new(operand) }, span })
                }
                None => self.postfix(),
            },
            _ => self.postfix(),
        }
    }

    fn cast_type(&self) -> Option<IntTy> {
        let Tok::Ident(name) = self.peek_at(1) else { return None };
        if *self.peek_at(2) != Tok::RParen {
            return None;
        }
        IntTy::from_name(name)
    }

    fn postfix(&mut self) -> Res<Expr> {
        let mut expr = self.primary()?;
        loop {
            if *self.peek() == Tok::LBracket {
                expr = self.index_or_slice(expr)?;
                continue;
            }
            if !self.eat(&Tok::Dot) {
                break;
            }
            let (name, name_span) = self.ident("a field or method name")?;
            let recv = Box::new(expr);
            let kind = if *self.peek() == Tok::LParen {
                let args = self.args()?;
                ExprKind::Method { recv, name, name_span, args, callee: Callee::Plain }
            } else {
                ExprKind::Field { recv, name, name_span, index: 0 }
            };
            let start = match &kind {
                ExprKind::Method { recv, .. } | ExprKind::Field { recv, .. } => recv.span.start,
                _ => unreachable!(),
            };
            expr = Expr { kind, span: Span::new(start, self.prev_end()) };
        }
        Ok(expr)
    }

    /// `text[i]`, or a range of it: `text[a..b]`, `text[..b]`, `text[a..]`, `text[..]`.
    fn index_or_slice(&mut self, recv: Expr) -> Res<Expr> {
        self.bump();
        let start = recv.span.start;
        let recv = Box::new(recv);
        let kind = self.bracketed(|p| {
            let from = match p.peek() {
                Tok::DotDot => None,
                _ => Some(Box::new(p.expr()?)),
            };
            if !p.eat(&Tok::DotDot) {
                let index = from.expect("read above when there is no `..`");
                return Ok(ExprKind::Index { recv, index, kind: IndexKind::Text });
            }
            let to = match p.peek() {
                Tok::RBracket => None,
                _ => Some(Box::new(p.expr()?)),
            };
            Ok(ExprKind::Slice { recv, from, to })
        })?;
        self.expect(Tok::RBracket, "`]`")?;
        Ok(Expr { kind, span: Span::new(start, self.prev_end()) })
    }

    /// Runs `inner` between brackets, where a struct literal is always unambiguous.
    fn bracketed<T>(&mut self, inner: impl FnOnce(&mut Self) -> Res<T>) -> Res<T> {
        let outer = std::mem::replace(&mut self.struct_literals, true);
        let result = inner(self);
        self.struct_literals = outer;
        result
    }

    fn args(&mut self) -> Res<Vec<Expr>> {
        self.expect(Tok::LParen, "`(`")?;
        let args = self.bracketed(|p| {
            let mut args = Vec::new();
            while *p.peek() != Tok::RParen {
                args.push(p.expr()?);
                if !p.eat(&Tok::Comma) {
                    break;
                }
            }
            Ok(args)
        })?;
        self.expect(Tok::RParen, "`)`")?;
        Ok(args)
    }

    fn primary(&mut self) -> Res<Expr> {
        let start = self.span();
        let kind = match self.peek().clone() {
            Tok::Int(value) => {
                self.bump();
                return int_literal(value as i128, start);
            }
            Tok::True => {
                self.bump();
                ExprKind::Bool(true)
            }
            Tok::False => {
                self.bump();
                ExprKind::Bool(false)
            }
            Tok::Null => {
                self.bump();
                ExprKind::Null
            }
            Tok::Str(text) => {
                self.bump();
                ExprKind::Str(text)
            }
            Tok::Char(value) => {
                self.bump();
                ExprKind::Char(value)
            }
            Tok::Interp(parts) => {
                self.bump();
                ExprKind::Interp(self.interp_parts(parts)?)
            }
            Tok::Match => return self.match_expr(),
            Tok::LParen => {
                self.bump();
                let inner = self.bracketed(|p| p.expr())?;
                self.expect(Tok::RParen, "`)`")?;
                return Ok(Expr { kind: inner.kind, span: Span::new(start.start, self.prev_end()) });
            }
            Tok::Ident(_) if self.at_collection() => {
                let ty = self.type_expr()?;
                let args = if *self.peek() == Tok::LParen { self.args()? } else { Vec::new() };
                let items = if *self.peek() == Tok::LBrace && self.struct_literals { Some(self.items()?) } else { None };
                ExprKind::Collection { ty, args, items }
            }
            Tok::Ident(_) => {
                let (name, name_span) = self.ident("an expression")?;
                if *self.peek() == Tok::LParen {
                    let args = self.args()?;
                    ExprKind::Call { name, name_span, args, callee: Callee::Plain }
                } else if *self.peek() == Tok::LBrace && self.struct_literals {
                    let fields = self.struct_fields()?;
                    ExprKind::StructLit { name, name_span, fields }
                } else {
                    ExprKind::Var(name)
                }
            }
            Tok::Return => {
                return Err(self.unexpected("an expression").note("inside an expression `return` is written only on the right side of `??`"))
            }
            _ => return Err(self.unexpected("an expression")),
        };
        Ok(Expr { kind, span: Span::new(start.start, self.prev_end()) })
    }

    /// `{ value = 1, next = null }`; fields are separated by commas, line ends do not matter.
    fn struct_fields(&mut self) -> Res<Vec<FieldInit>> {
        self.bump();
        let fields = self.bracketed(|p| {
            let mut fields = Vec::new();
            p.skip_newlines();
            while *p.peek() != Tok::RBrace {
                let (name, span) = p.ident("a field name")?;
                p.expect(Tok::Assign, "`=`")?;
                let value = p.expr()?;
                fields.push(FieldInit { name, span, value, index: 0 });
                p.skip_newlines();
                if !p.eat(&Tok::Comma) {
                    break;
                }
                p.skip_newlines();
            }
            Ok(fields)
        })?;
        self.expect(Tok::RBrace, "`}`")?;
        Ok(fields)
    }

    /// Whether a type with `<...>` is written here and then built: `List<int>()`,
    /// `List<int> { 1, 2 }`. As in C#, `a < b > (c)` is read this way too.
    fn at_collection(&self) -> bool {
        if *self.peek_at(1) != Tok::Lt {
            return false;
        }
        let Some(n) = self.type_len(0) else { return false };
        *self.peek_at(n - 1) == Tok::Gt
            && (*self.peek_at(n) == Tok::LParen || (*self.peek_at(n) == Tok::LBrace && self.struct_literals))
    }

    /// `{ 1, 2 }` or `{ {"x", 10}, {"y", 20} }`; line ends do not matter.
    fn items(&mut self) -> Res<Vec<Item>> {
        self.bump();
        let items = self.bracketed(|p| {
            let mut items = Vec::new();
            p.skip_newlines();
            while *p.peek() != Tok::RBrace {
                let start = p.span();
                let braced = p.eat(&Tok::LBrace);
                let mut values = Vec::new();
                if braced {
                    while *p.peek() != Tok::RBrace {
                        values.push(p.expr()?);
                        if !p.eat(&Tok::Comma) {
                            break;
                        }
                    }
                    p.expect(Tok::RBrace, "`}`")?;
                } else {
                    values.push(p.expr()?);
                }
                items.push(Item { values, braced, span: Span::new(start.start, p.prev_end()) });
                p.skip_newlines();
                if !p.eat(&Tok::Comma) {
                    break;
                }
                p.skip_newlines();
            }
            Ok(items)
        })?;
        self.expect(Tok::RBrace, "`}`")?;
        Ok(items)
    }

    fn interp_parts(&mut self, parts: Vec<Part>) -> Res<Vec<InterpPart>> {
        let mut result = Vec::new();
        for part in parts {
            match part {
                Part::Text(text) => result.push(InterpPart::Text(text)),
                Part::Code(range) => {
                    let toks = lex(self.src, range, true)?;
                    let mut inner = Parser { src: self.src, toks, i: 0, struct_literals: true };
                    let expr = inner.expr()?;
                    if *inner.peek() != Tok::Eof {
                        return Err(inner.unexpected("`}`"));
                    }
                    result.push(InterpPart::Expr(expr));
                }
            }
        }
        Ok(result)
    }
}

/// Where an expression may stand as the value of a `match` arm instead of a statement.
#[derive(Clone, Copy, PartialEq)]
enum Tail {
    Never,
    /// On the last line of an arm's block.
    LastLine,
    /// In an arm written without a block: `Circle(r) => 3 * r * r`.
    Always,
}

const COMPARISON: usize = 3;
const ADDITIVE: usize = 4;

fn int_literal(value: i128, span: Span) -> Res<Expr> {
    match i64::try_from(value) {
        Ok(value) => Ok(Expr { kind: ExprKind::Int(value, IntTy::Int), span }),
        Err(_) => Err(Diag::error(span, format!("{value} does not fit in any integer type"))),
    }
}

/// `Some` for an assignment operator; for compound assignment (`+=`) with the inner operator.
fn assign_op(tok: &Tok) -> Option<Option<BinOp>> {
    match tok {
        Tok::Assign => Some(None),
        Tok::PlusEq => Some(Some(BinOp::Add)),
        Tok::MinusEq => Some(Some(BinOp::Sub)),
        Tok::StarEq => Some(Some(BinOp::Mul)),
        Tok::SlashEq => Some(Some(BinOp::Div)),
        Tok::PercentEq => Some(Some(BinOp::Rem)),
        _ => None,
    }
}
