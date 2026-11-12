//! Hand-written recursive descent parser.

use crate::ast::*;
use crate::diag::{Diag, Span};
use crate::lexer::{lex, Part, Tok, Token};
use crate::types::IntTy;

/// Words that are in the design but not in the interpreter yet. They get an explicit
/// message instead of "unknown name".
const NOT_YET: &[&str] = &[
    "enum", "interface", "match", "use", "try", "shared", "weak", "checked", "locked", "raw", "unsafe", "lock",
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

    /// `[read | ref] Name { * | ? }`
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
        let mut suffixes = Vec::new();
        loop {
            match self.peek() {
                Tok::Star => suffixes.push(Suffix::Pointer),
                Tok::Question => suffixes.push(Suffix::Nullable),
                _ => break,
            }
            self.bump();
        }
        Ok(TypeExpr { mode, name, suffixes, span: Span::new(start.start, self.prev_end()) })
    }

    fn program(&mut self) -> Res<Program> {
        let mut program = Program { structs: Vec::new(), funcs: Vec::new() };
        self.skip_newlines();
        while *self.peek() != Tok::Eof {
            // One module for now, so `public` changes nothing yet.
            self.eat(&Tok::Public);
            if *self.peek() == Tok::Struct {
                program.structs.push(self.struct_decl()?);
            } else {
                let ret = self.type_expr().map_err(|d| {
                    d.hint("only structs and functions are written at the top level: `void main() { ... }`")
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
        Ok(StructDecl { name, name_span, fields, methods })
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

    /// The type comes first: `int x = 1`, `Node*? list = null`. A name, any `*` and `?`,
    /// then another name is a declaration.
    fn at_declaration(&self) -> bool {
        if !matches!(self.peek(), Tok::Ident(_)) {
            return false;
        }
        let mut n = 1;
        while matches!(self.peek_at(n), Tok::Star | Tok::Question) {
            n += 1;
        }
        matches!(self.peek_at(n), Tok::Ident(_))
    }

    fn stmt(&mut self) -> Res<Stmt> {
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
                if *self.peek() != Tok::DotDot {
                    return Err(self.unexpected("`..`").note("for now `for` only iterates over a range `a..b`"));
                }
                self.bump();
                let to = self.head_expr(true)?;
                let body = self.block()?;
                StmtKind::For { var, var_span, from, to, body }
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
            _ if self.at_declaration() => {
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
                        return Err(Diag::error(expr.span, "the left side of an assignment must be a variable or a field"));
                    }
                    self.bump();
                    let value = self.expr()?;
                    StmtKind::Assign { target: expr, op, value, through: false }
                } else if matches!(expr.kind, ExprKind::Call { .. } | ExprKind::Method { .. }) {
                    StmtKind::Expr(expr)
                } else {
                    return Err(Diag::error(expr.span, "the value of this expression is not used")
                        .hint("only a call, an assignment or a declaration can be a statement"));
                }
            }
        };
        Ok(Stmt { kind, span: Span::new(start.start, self.prev_end()) })
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
        let right = self.expr()?;
        let span = left.span.to(right.span);
        Ok(Expr { kind: ExprKind::Coalesce(Box::new(left), Box::new(right)), span })
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
                left = self.is_null(left)?;
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

    /// `x is null`, `x is not null`
    fn is_null(&mut self, operand: Expr) -> Res<Expr> {
        self.bump();
        let negated = matches!(self.peek(), Tok::Ident(word) if word == "not");
        if negated {
            self.bump();
        }
        if *self.peek() != Tok::Null {
            return Err(self.unexpected("`null`").note("for now `is` only tests for `null`"));
        }
        self.bump();
        let span = Span::new(operand.span.start, self.prev_end());
        Ok(Expr { kind: ExprKind::IsNull { expr: Box::new(operand), negated }, span })
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
        while *self.peek() == Tok::Dot {
            self.bump();
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
            Tok::Interp(parts) => {
                self.bump();
                ExprKind::Interp(self.interp_parts(parts)?)
            }
            Tok::LParen => {
                self.bump();
                let inner = self.bracketed(|p| p.expr())?;
                self.expect(Tok::RParen, "`)`")?;
                return Ok(Expr { kind: inner.kind, span: Span::new(start.start, self.prev_end()) });
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
