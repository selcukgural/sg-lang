//! The filter read into a tree: the tokens, then the operators by jq's precedence, loosest first:
//! `|`, `,`, `//` (right), the assignments, `or`, `and`, the comparisons, `+ -`, `* / %`, unary `-`,
//! and the postfix forms. `f as $x | g` and `def` take the rest of the pipe as their body.

use crate::number::Decimal;
use crate::value::Value;

#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    Dot,
    DotDot,
    Field(String),
    Var(String),
    Ident(String),
    Number(String),
    Str(Vec<Part>),
    Format(String),
    Op(&'static str),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Part {
    Text(String),
    Interpolation(Vec<Token>),
}

/// The tree of the filter.
pub enum Ast {
    Identity,
    RecurseAll,
    Literal(Value),
    /// A text with `\(...)` inside, under a format (`@base64 "..."`) or none.
    Interpolation(Option<String>, Vec<Piece>),
    Format(String),
    Field(Box<Ast>, String),
    Index(Box<Ast>, Box<Ast>),
    Slice(Box<Ast>, Option<Box<Ast>>, Option<Box<Ast>>),
    Iterate(Box<Ast>),
    /// An access (`.a`, `.[e]`, `.[e:f]`, `.[]`) followed by `?`: only the access's own failure
    /// gives no output, as jq's `INDEX_OPT` and `EACH_OPT`.
    Optional(Box<Ast>),
    Try(Box<Ast>),
    Array(Option<Box<Ast>>),
    Object(Vec<(Ast, Ast)>),
    Neg(Box<Ast>),
    Pipe(Box<Ast>, Box<Ast>),
    Comma(Box<Ast>, Box<Ast>),
    Alternative(Box<Ast>, Box<Ast>),
    And(Box<Ast>, Box<Ast>),
    Or(Box<Ast>, Box<Ast>),
    Binary(Op, Box<Ast>, Box<Ast>),
    Assign(Assign, Box<Ast>, Box<Ast>),
    If(Box<Ast>, Box<Ast>, Box<Ast>),
    Reduce(Box<Ast>, String, Box<Ast>, Box<Ast>),
    Foreach(Box<Ast>, String, Box<Ast>, Box<Ast>, Option<Box<Ast>>),
    As(Box<Ast>, String, Box<Ast>),
    Var(String),
    Call(String, Vec<Ast>),
    Def(Box<Def>, Box<Ast>),
}

pub enum Piece {
    Text(String),
    Value(Ast),
}

pub struct Def {
    pub name: String,
    /// Each parameter's name and whether it is a value (`$name`) rather than a filter.
    pub params: Vec<(String, bool)>,
    pub body: Ast,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Assign {
    /// `=`
    Set,
    /// `|=`
    Update,
    /// `+=` and the others
    Arithmetic(Op),
    /// `//=`
    Alternative,
}

pub type Result<T> = std::result::Result<T, String>;

const OPERATORS: [&str; 30] = [
    "?//", "//=", "|=", "+=", "-=", "*=", "/=", "%=", "==", "!=", "<=", ">=", "//", "|", ",", "=", "<", ">",
    "+", "-", "*", "/", "%", "(", ")", "[", "]", "{", "}", ":",
];
const MORE_OPERATORS: [&str; 2] = [";", "?"];

struct Lexer<'a> {
    text: &'a [u8],
    at: usize,
}

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}

fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

impl Lexer<'_> {
    /// The tokens up to the end, or up to the `)` that closes an interpolation.
    fn tokens(&mut self, in_interpolation: bool) -> Result<Vec<Token>> {
        let mut tokens = Vec::new();
        let mut depth = 0;
        loop {
            self.skip_blanks();
            let Some(&c) = self.text.get(self.at) else {
                if in_interpolation {
                    return Err("unfinished interpolation".into());
                }
                return Ok(tokens);
            };
            if in_interpolation {
                if c == b'(' {
                    depth += 1;
                } else if c == b')' {
                    if depth == 0 {
                        self.at += 1;
                        return Ok(tokens);
                    }
                    depth -= 1;
                }
            }
            tokens.push(self.token()?);
        }
    }

    fn skip_blanks(&mut self) {
        while let Some(&c) = self.text.get(self.at) {
            if c == b'#' {
                while self.at < self.text.len() && self.text[self.at] != b'\n' {
                    self.at += 1;
                }
            } else if c.is_ascii_whitespace() {
                self.at += 1;
            } else {
                break;
            }
        }
    }

    fn name(&mut self) -> String {
        let start = self.at;
        while self.at < self.text.len() && is_ident(self.text[self.at]) {
            self.at += 1;
        }
        String::from_utf8_lossy(&self.text[start..self.at]).into_owned()
    }

    fn token(&mut self) -> Result<Token> {
        let text = self.text;
        let c = text[self.at];
        let next = text.get(self.at + 1).copied().unwrap_or(0);
        if c == b'"' {
            self.at += 1;
            return self.string();
        }
        if c == b'.' {
            if next == b'.' {
                self.at += 2;
                return Ok(Token::DotDot);
            }
            if is_ident_start(next) {
                self.at += 1;
                return Ok(Token::Field(self.name()));
            }
            if !next.is_ascii_digit() {
                self.at += 1;
                return Ok(Token::Dot);
            }
        }
        if c.is_ascii_digit() || c == b'.' {
            let start = self.at;
            while self.at < text.len() && text[self.at].is_ascii_digit() {
                self.at += 1;
            }
            if self.at < text.len() && text[self.at] == b'.' {
                self.at += 1;
                while self.at < text.len() && text[self.at].is_ascii_digit() {
                    self.at += 1;
                }
            }
            if self.at < text.len() && (text[self.at] == b'e' || text[self.at] == b'E') {
                let mut end = self.at + 1;
                if end < text.len() && (text[end] == b'+' || text[end] == b'-') {
                    end += 1;
                }
                if end < text.len() && text[end].is_ascii_digit() {
                    while end < text.len() && text[end].is_ascii_digit() {
                        end += 1;
                    }
                    self.at = end;
                }
            }
            return Ok(Token::Number(String::from_utf8_lossy(&text[start..self.at]).into_owned()));
        }
        if c == b'$' && is_ident_start(next) {
            self.at += 1;
            return Ok(Token::Var(self.name()));
        }
        if c == b'@' && is_ident_start(next) {
            self.at += 1;
            return Ok(Token::Format(self.name()));
        }
        if is_ident_start(c) {
            let mut name = self.name();
            // Names of modules, `a::b`.
            while text.get(self.at) == Some(&b':') && text.get(self.at + 1) == Some(&b':') {
                self.at += 2;
                name.push_str("::");
                name.push_str(&self.name());
            }
            return Ok(Token::Ident(name));
        }
        for op in OPERATORS.iter().chain(MORE_OPERATORS.iter()) {
            if text[self.at..].starts_with(op.as_bytes()) {
                self.at += op.len();
                return Ok(Token::Op(op));
            }
        }
        Err(format!("unexpected character '{}'", c as char))
    }

    /// A text after its `"`: its pieces and interpolations.
    fn string(&mut self) -> Result<Token> {
        let mut parts = Vec::new();
        let mut current: Vec<u8> = Vec::new();
        loop {
            let Some(&c) = self.text.get(self.at) else {
                return Err("unfinished text".into());
            };
            self.at += 1;
            match c {
                b'"' => break,
                b'\\' => {
                    let Some(&e) = self.text.get(self.at) else {
                        return Err("unfinished text".into());
                    };
                    self.at += 1;
                    match e {
                        b'"' | b'\\' | b'/' => current.push(e),
                        b'b' => current.push(8),
                        b'f' => current.push(12),
                        b'n' => current.push(b'\n'),
                        b'r' => current.push(b'\r'),
                        b't' => current.push(b'\t'),
                        b'u' => {
                            let mut code = self.hex4()?;
                            if (0xD800..0xDC00).contains(&code)
                                && self.text.get(self.at) == Some(&b'\\')
                                && self.text.get(self.at + 1) == Some(&b'u')
                            {
                                let save = self.at;
                                self.at += 2;
                                let low = self.hex4()?;
                                if (0xDC00..0xE000).contains(&low) {
                                    code = 0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00);
                                } else {
                                    self.at = save;
                                }
                            }
                            let ch = char::from_u32(code).unwrap_or('\u{FFFD}');
                            let mut buf = [0; 4];
                            current.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                        }
                        b'(' => {
                            if !current.is_empty() {
                                parts.push(Part::Text(String::from_utf8_lossy(&current).into_owned()));
                                current.clear();
                            }
                            parts.push(Part::Interpolation(self.tokens(true)?));
                        }
                        _ => return Err(format!("invalid escape '\\{}'", e as char)),
                    }
                }
                _ => current.push(c),
            }
        }
        if !current.is_empty() || parts.is_empty() {
            parts.push(Part::Text(String::from_utf8_lossy(&current).into_owned()));
        }
        Ok(Token::Str(parts))
    }

    fn hex4(&mut self) -> Result<u32> {
        let digits = self.text.get(self.at..self.at + 4).ok_or("invalid \\u escape")?;
        let code = std::str::from_utf8(digits)
            .ok()
            .and_then(|d| u32::from_str_radix(d, 16).ok())
            .ok_or("invalid \\u escape")?;
        self.at += 4;
        Ok(code)
    }
}

/// Reads a whole filter.
pub fn parse(text: &str) -> Result<Ast> {
    let tokens = Lexer { text: text.as_bytes(), at: 0 }.tokens(false)?;
    let mut parser = Parser { tokens, at: 0, no_binding: false };
    let ast = parser.program()?;
    if parser.at != parser.tokens.len() {
        return Err(format!("syntax error: unexpected {:?}", parser.tokens[parser.at]));
    }
    Ok(ast)
}

struct Parser {
    tokens: Vec<Token>,
    at: usize,
    /// In the value of an object's member, where jq takes no `as` and no `def` outside brackets.
    no_binding: bool,
}

fn boxed(ast: Ast) -> Box<Ast> {
    Box::new(ast)
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn peek_op(&self, op: &str) -> bool {
        matches!(self.peek(), Some(Token::Op(o)) if *o == op)
    }

    fn peek_ident(&self, name: &str) -> bool {
        matches!(self.peek(), Some(Token::Ident(n)) if n == name)
    }

    fn eat_op(&mut self, op: &str) -> bool {
        if self.peek_op(op) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn eat_ident(&mut self, name: &str) -> bool {
        if self.peek_ident(name) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn expect_op(&mut self, op: &str) -> Result<()> {
        if self.eat_op(op) { Ok(()) } else { Err(format!("syntax error: expected '{op}', found {:?}", self.peek())) }
    }

    fn expect_ident(&mut self, name: &str) -> Result<()> {
        if self.eat_ident(name) { Ok(()) } else { Err(format!("syntax error: expected '{name}', found {:?}", self.peek())) }
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.at).cloned();
        self.at += 1;
        token
    }

    /// A program: definitions, then a filter (`.` when there is none).
    fn program(&mut self) -> Result<Ast> {
        if self.peek_ident("def") {
            self.at += 1;
            let def = self.def()?;
            let rest = self.program()?;
            return Ok(Ast::Def(Box::new(def), boxed(rest)));
        }
        self.pipe()
    }

    /// Parses with `as` and `def` allowed again (inside brackets).
    fn nested<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        let saved = std::mem::replace(&mut self.no_binding, false);
        let result = f(self);
        self.no_binding = saved;
        result
    }

    fn pipe(&mut self) -> Result<Ast> {
        let left = self.comma()?;
        if self.eat_op("|") {
            let right = self.pipe()?;
            return Ok(Ast::Pipe(boxed(left), boxed(right)));
        }
        Ok(left)
    }

    fn comma(&mut self) -> Result<Ast> {
        let mut left = self.bound()?;
        while self.eat_op(",") {
            let right = self.bound()?;
            left = Ast::Comma(boxed(left), boxed(right));
        }
        Ok(left)
    }

    /// An expression, or one of jq's two forms of a query that take the rest of it: `def f: g; body`
    /// and `Expr as $x | body`. The bound expression is a whole one, so `1 + .a as $x | $x` binds
    /// `1 + .a`; either may stand after `,` (`1, .a as $x | $x, 2` binds over `$x, 2`).
    fn bound(&mut self) -> Result<Ast> {
        if !self.no_binding && self.peek_ident("def") {
            self.at += 1;
            let def = self.def()?;
            let rest = self.pipe()?;
            return Ok(Ast::Def(Box::new(def), boxed(rest)));
        }
        let source = self.alternative()?;
        if !self.no_binding && self.eat_ident("as") {
            let name = self.variable()?;
            self.expect_op("|")?;
            let body = self.pipe()?;
            return Ok(Ast::As(boxed(source), name, boxed(body)));
        }
        Ok(source)
    }

    fn alternative(&mut self) -> Result<Ast> {
        let left = self.assignment()?;
        if self.eat_op("//") {
            let right = self.alternative()?;
            return Ok(Ast::Alternative(boxed(left), boxed(right)));
        }
        Ok(left)
    }

    fn assignment(&mut self) -> Result<Ast> {
        let left = self.or()?;
        let kind = match self.peek() {
            Some(Token::Op("=")) => Assign::Set,
            Some(Token::Op("|=")) => Assign::Update,
            Some(Token::Op("+=")) => Assign::Arithmetic(Op::Add),
            Some(Token::Op("-=")) => Assign::Arithmetic(Op::Sub),
            Some(Token::Op("*=")) => Assign::Arithmetic(Op::Mul),
            Some(Token::Op("/=")) => Assign::Arithmetic(Op::Div),
            Some(Token::Op("%=")) => Assign::Arithmetic(Op::Mod),
            Some(Token::Op("//=")) => Assign::Alternative,
            _ => return Ok(left),
        };
        self.at += 1;
        let right = self.or()?;
        Ok(Ast::Assign(kind, boxed(left), boxed(right)))
    }

    fn or(&mut self) -> Result<Ast> {
        let mut left = self.and()?;
        while self.eat_ident("or") {
            let right = self.and()?;
            left = Ast::Or(boxed(left), boxed(right));
        }
        Ok(left)
    }

    fn and(&mut self) -> Result<Ast> {
        let mut left = self.comparison()?;
        while self.eat_ident("and") {
            let right = self.comparison()?;
            left = Ast::And(boxed(left), boxed(right));
        }
        Ok(left)
    }

    fn comparison(&mut self) -> Result<Ast> {
        let left = self.additive()?;
        let op = match self.peek() {
            Some(Token::Op("==")) => Op::Eq,
            Some(Token::Op("!=")) => Op::Ne,
            Some(Token::Op("<")) => Op::Lt,
            Some(Token::Op("<=")) => Op::Le,
            Some(Token::Op(">")) => Op::Gt,
            Some(Token::Op(">=")) => Op::Ge,
            _ => return Ok(left),
        };
        self.at += 1;
        let right = self.additive()?;
        if matches!(self.peek(), Some(Token::Op("==" | "!=" | "<" | "<=" | ">" | ">="))) {
            return Err("syntax error: comparisons do not chain".into());
        }
        Ok(Ast::Binary(op, boxed(left), boxed(right)))
    }

    fn additive(&mut self) -> Result<Ast> {
        let mut left = self.multiplicative()?;
        loop {
            let op = match self.peek() {
                Some(Token::Op("+")) => Op::Add,
                Some(Token::Op("-")) => Op::Sub,
                _ => return Ok(left),
            };
            self.at += 1;
            let right = self.multiplicative()?;
            left = Ast::Binary(op, boxed(left), boxed(right));
        }
    }

    fn multiplicative(&mut self) -> Result<Ast> {
        let mut left = self.unary()?;
        loop {
            let op = match self.peek() {
                Some(Token::Op("*")) => Op::Mul,
                Some(Token::Op("/")) => Op::Div,
                Some(Token::Op("%")) => Op::Mod,
                _ => return Ok(left),
            };
            self.at += 1;
            let right = self.unary()?;
            left = Ast::Binary(op, boxed(left), boxed(right));
        }
    }

    fn unary(&mut self) -> Result<Ast> {
        if self.eat_op("-") {
            let operand = self.unary()?;
            return Ok(Ast::Neg(boxed(operand)));
        }
        self.postfix()
    }

    /// A term and what follows it: `.name`, `[...]`, `?`.
    fn postfix(&mut self) -> Result<Ast> {
        let mut term = self.primary()?;
        loop {
            match self.peek() {
                Some(Token::Field(_)) => {
                    let Some(Token::Field(name)) = self.next() else { unreachable!() };
                    term = self.optional(Ast::Field(boxed(term), name));
                }
                Some(Token::Dot) if matches!(self.tokens.get(self.at + 1), Some(Token::Str(_))) => {
                    self.at += 1;
                    term = self.string_index(term)?;
                }
                Some(Token::Dot) if matches!(self.tokens.get(self.at + 1), Some(Token::Op("["))) => {
                    self.at += 2;
                    term = self.bracket(term)?;
                }
                Some(Token::Op("[")) => {
                    self.at += 1;
                    term = self.bracket(term)?;
                }
                Some(Token::Op("?")) => {
                    self.at += 1;
                    term = Ast::Try(boxed(term));
                }
                _ => break,
            }
        }
        Ok(term)
    }

    fn variable(&mut self) -> Result<String> {
        match self.next() {
            Some(Token::Var(name)) => Ok(name),
            other => Err(format!("syntax error: expected a $name, found {other:?}")),
        }
    }

    /// An access, made optional by a `?` straight after it.
    fn optional(&mut self, access: Ast) -> Ast {
        if self.eat_op("?") { Ast::Optional(boxed(access)) } else { access }
    }

    /// `."name"`: the text after the dot.
    fn string_index(&mut self, term: Ast) -> Result<Ast> {
        let Some(Token::Str(parts)) = self.next() else { unreachable!() };
        let access = match self.string(None, parts)? {
            Ast::Literal(Value::String(s)) => Ast::Field(boxed(term), (*s).clone()),
            key => Ast::Index(boxed(term), boxed(key)),
        };
        Ok(self.optional(access))
    }

    /// What follows a `[` after a term: `]`, `e]`, `e:f]`, `:f]`, `e:]`.
    fn bracket(&mut self, term: Ast) -> Result<Ast> {
        let access = self.bracket_access(term)?;
        Ok(self.optional(access))
    }

    fn bracket_access(&mut self, term: Ast) -> Result<Ast> {
        let saved = std::mem::replace(&mut self.no_binding, false);
        let access = self.bracket_inner(term);
        self.no_binding = saved;
        access
    }

    fn bracket_inner(&mut self, term: Ast) -> Result<Ast> {
        if self.eat_op("]") {
            return Ok(Ast::Iterate(boxed(term)));
        }
        if self.eat_op(":") {
            let to = self.pipe()?;
            self.expect_op("]")?;
            return Ok(Ast::Slice(boxed(term), None, Some(boxed(to))));
        }
        let index = self.pipe()?;
        if self.eat_op(":") {
            if self.eat_op("]") {
                return Ok(Ast::Slice(boxed(term), Some(boxed(index)), None));
            }
            let to = self.pipe()?;
            self.expect_op("]")?;
            return Ok(Ast::Slice(boxed(term), Some(boxed(index)), Some(boxed(to))));
        }
        self.expect_op("]")?;
        Ok(Ast::Index(boxed(term), boxed(index)))
    }

    fn primary(&mut self) -> Result<Ast> {
        if matches!(self.peek(), Some(Token::Op("(" | "[" | "{")))
            || self.peek_ident("if")
            || self.peek_ident("reduce")
            || self.peek_ident("foreach")
        {
            return self.nested(|p| p.primary_inner());
        }
        if self.no_binding && self.peek_ident("def") {
            return Err("syntax error: unexpected def".into());
        }
        self.primary_inner()
    }

    fn primary_inner(&mut self) -> Result<Ast> {
        let Some(token) = self.next() else {
            return Err("syntax error: unexpected end of the filter".into());
        };
        Ok(match token {
            Token::Number(text) => {
                let decimal = Decimal::parse(text.as_bytes()).ok_or("invalid number")?;
                Ast::Literal(Value::literal(decimal))
            }
            Token::Str(parts) => self.string(None, parts)?,
            Token::Format(name) => {
                if let Some(Token::Str(_)) = self.peek() {
                    let Some(Token::Str(parts)) = self.next() else { unreachable!() };
                    self.string(Some(name), parts)?
                } else {
                    Ast::Format(name)
                }
            }
            Token::Dot => match self.peek() {
                Some(Token::Str(_)) => self.string_index(Ast::Identity)?,
                Some(Token::Op("[")) => {
                    self.at += 1;
                    self.bracket(Ast::Identity)?
                }
                _ => Ast::Identity,
            },
            Token::Field(name) => self.optional(Ast::Field(boxed(Ast::Identity), name)),
            Token::DotDot => Ast::RecurseAll,
            Token::Var(name) => {
                if name == "__loc__" {
                    return Err("$__loc__ is not supported".into());
                }
                Ast::Var(name)
            }
            Token::Op("(") => {
                let inner = self.pipe()?;
                self.expect_op(")")?;
                inner
            }
            Token::Op("[") => {
                if self.eat_op("]") {
                    Ast::Array(None)
                } else {
                    let inner = self.pipe()?;
                    self.expect_op("]")?;
                    Ast::Array(Some(boxed(inner)))
                }
            }
            Token::Op("{") => self.object()?,
            Token::Ident(name) => match name.as_str() {
                "null" => Ast::Literal(Value::Null),
                "true" => Ast::Literal(Value::True),
                "false" => Ast::Literal(Value::False),
                "if" => self.conditional()?,
                "reduce" => {
                    let source = self.alternative()?;
                    self.expect_ident("as")?;
                    let name = self.variable()?;
                    self.expect_op("(")?;
                    let init = self.pipe()?;
                    self.expect_op(";")?;
                    let update = self.pipe()?;
                    self.expect_op(")")?;
                    Ast::Reduce(boxed(source), name, boxed(init), boxed(update))
                }
                "foreach" => {
                    let source = self.alternative()?;
                    self.expect_ident("as")?;
                    let name = self.variable()?;
                    self.expect_op("(")?;
                    let init = self.pipe()?;
                    self.expect_op(";")?;
                    let update = self.pipe()?;
                    let extract = if self.eat_op(";") { Some(boxed(self.pipe()?)) } else { None };
                    self.expect_op(")")?;
                    Ast::Foreach(boxed(source), name, boxed(init), boxed(update), extract)
                }
                "def" => {
                    let def = self.def()?;
                    let rest = self.pipe()?;
                    Ast::Def(Box::new(def), boxed(rest))
                }
                "try" => {
                    let body = self.unary()?;
                    if self.peek_ident("catch") {
                        return Err("try ... catch is not supported".into());
                    }
                    Ast::Try(boxed(body))
                }
                "then" | "elif" | "else" | "end" | "as" | "and" | "or" | "catch" | "label" | "import"
                | "include" | "__loc__" => return Err(format!("syntax error: unexpected '{name}'")),
                _ => {
                    let mut args = Vec::new();
                    if self.eat_op("(") {
                        loop {
                            args.push(self.nested(|p| p.pipe())?);
                            if self.eat_op(")") {
                                break;
                            }
                            self.expect_op(";")?;
                        }
                    }
                    Ast::Call(name, args)
                }
            },
            other => return Err(format!("syntax error: unexpected {other:?}")),
        })
    }

    fn conditional(&mut self) -> Result<Ast> {
        let condition = self.pipe()?;
        self.expect_ident("then")?;
        let then = self.pipe()?;
        let otherwise = if self.eat_ident("elif") {
            self.conditional()?
        } else if self.eat_ident("else") {
            let otherwise = self.pipe()?;
            self.expect_ident("end")?;
            otherwise
        } else {
            self.expect_ident("end")?;
            Ast::Identity
        };
        Ok(Ast::If(boxed(condition), boxed(then), boxed(otherwise)))
    }

    /// `name(params): body;` after `def`.
    fn def(&mut self) -> Result<Def> {
        let name = match self.next() {
            Some(Token::Ident(name)) => name,
            other => return Err(format!("syntax error: expected a name after def, found {other:?}")),
        };
        let mut params = Vec::new();
        if self.eat_op("(") {
            loop {
                match self.next() {
                    Some(Token::Ident(p)) => params.push((p, false)),
                    Some(Token::Var(p)) => params.push((p, true)),
                    other => return Err(format!("syntax error: bad parameter {other:?}")),
                }
                if self.eat_op(")") {
                    break;
                }
                self.expect_op(";")?;
            }
        }
        self.expect_op(":")?;
        let body = self.pipe()?;
        self.expect_op(";")?;
        Ok(Def { name, params, body })
    }

    fn object(&mut self) -> Result<Ast> {
        let mut entries = Vec::new();
        if self.eat_op("}") {
            return Ok(Ast::Object(entries));
        }
        loop {
            let entry = match self.next() {
                Some(Token::Var(name)) => {
                    if self.eat_op(":") {
                        (Ast::Var(name), self.object_value()?)
                    } else {
                        (Ast::Literal(Value::string(name.clone())), Ast::Var(name))
                    }
                }
                Some(Token::Ident(name)) => {
                    let key = Ast::Literal(Value::string(name.clone()));
                    if self.eat_op(":") {
                        (key, self.object_value()?)
                    } else {
                        (key, Ast::Field(boxed(Ast::Identity), name))
                    }
                }
                Some(Token::Number(text)) if self.peek_op(":") => {
                    return Err(format!("syntax error: a number ({text}) cannot be an object key"));
                }
                Some(Token::Str(parts)) => {
                    let key = self.string(None, parts.clone())?;
                    if self.eat_op(":") {
                        (key, self.object_value()?)
                    } else {
                        let again = self.string(None, parts)?;
                        (key, Ast::Index(boxed(Ast::Identity), boxed(again)))
                    }
                }
                Some(Token::Format(name)) if matches!(self.peek(), Some(Token::Str(_))) => {
                    let Some(Token::Str(parts)) = self.next() else { unreachable!() };
                    let key = self.string(Some(name), parts)?;
                    self.expect_op(":")?;
                    (key, self.object_value()?)
                }
                Some(Token::Op("(")) => {
                    let key = self.pipe()?;
                    self.expect_op(")")?;
                    self.expect_op(":")?;
                    (key, self.object_value()?)
                }
                other => return Err(format!("syntax error: unexpected {other:?} in an object")),
            };
            entries.push(entry);
            if self.eat_op("}") {
                return Ok(Ast::Object(entries));
            }
            self.expect_op(",")?;
        }
    }

    /// The value of a member: a pipe without `,`, and without `as` or `def` outside brackets.
    fn object_value(&mut self) -> Result<Ast> {
        let saved = std::mem::replace(&mut self.no_binding, true);
        let value = self.object_value_pipe();
        self.no_binding = saved;
        value
    }

    fn object_value_pipe(&mut self) -> Result<Ast> {
        let left = self.alternative()?;
        if self.eat_op("|") {
            let right = self.object_value_pipe()?;
            return Ok(Ast::Pipe(boxed(left), boxed(right)));
        }
        Ok(left)
    }

    /// A text token as a tree: a literal, or its interpolations under a format.
    fn string(&mut self, format: Option<String>, parts: Vec<Part>) -> Result<Ast> {
        if parts.len() == 1 && format.is_none()
            && let Part::Text(text) = &parts[0] {
                return Ok(Ast::Literal(Value::string(text.clone())));
            }
        let mut pieces = Vec::new();
        for part in parts {
            match part {
                Part::Text(text) => pieces.push(Piece::Text(text)),
                Part::Interpolation(tokens) => {
                    let mut inner = Parser { tokens, at: 0, no_binding: false };
                    let ast = inner.pipe()?;
                    if inner.at != inner.tokens.len() {
                        return Err("syntax error in an interpolation".into());
                    }
                    pieces.push(Piece::Value(ast));
                }
            }
        }
        Ok(Ast::Interpolation(format, pieces))
    }
}
