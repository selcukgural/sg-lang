//! Splits source text into tokens and applies the line-end rule.
//!
//! Line-end rule: a line end terminates the statement. Three exceptions: inside `(` or `[`,
//! when the line ends with an operator, a comma, a dot or an opening bracket, and when the
//! next line starts with a dot, the statement continues on the next line.

use crate::diag::{Diag, Span};

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Int(u64),
    Str(String),
    Interp(Vec<Part>),
    Ident(String),

    Var,
    If,
    Else,
    While,
    For,
    In,
    Return,
    Break,
    Continue,
    True,
    False,
    Null,
    Is,
    New,
    Read,
    Ref,
    Public,
    Struct,
    Enum,
    Match,
    Try,

    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Dot,
    DotDot,

    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Assign,
    FatArrow,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    AndAnd,
    OrOr,
    Bang,
    Question,
    QuestionQuestion,

    Newline,
    Eof,
}

/// A piece of `$"..."`: plain text, or the source range of an expression inside `{ }`.
#[derive(Clone, Debug, PartialEq)]
pub enum Part {
    Text(String),
    Code(Span),
}

#[derive(Clone, Debug)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
}

impl Tok {
    /// A line ending with this token continues on the next line.
    fn continues_line(&self) -> bool {
        matches!(
            self,
            Tok::Comma
                | Tok::Dot
                | Tok::DotDot
                | Tok::Plus
                | Tok::Minus
                | Tok::Star
                | Tok::Slash
                | Tok::Percent
                | Tok::Assign
                | Tok::FatArrow
                | Tok::PlusEq
                | Tok::MinusEq
                | Tok::StarEq
                | Tok::SlashEq
                | Tok::PercentEq
                | Tok::Eq
                | Tok::Ne
                | Tok::Lt
                | Tok::Le
                | Tok::Gt
                | Tok::Ge
                | Tok::AndAnd
                | Tok::OrOr
                | Tok::Bang
                | Tok::QuestionQuestion
                | Tok::LParen
                | Tok::LBracket
                | Tok::LBrace
        )
    }

    pub fn describe(&self) -> String {
        match self {
            Tok::Int(v) => format!("`{v}`"),
            Tok::Str(_) | Tok::Interp(_) => "a string".to_string(),
            Tok::Ident(name) => format!("`{name}`"),
            Tok::Newline => "end of line".to_string(),
            Tok::Eof => "end of file".to_string(),
            other => format!("`{}`", other.text()),
        }
    }

    fn text(&self) -> &'static str {
        match self {
            Tok::Var => "var",
            Tok::If => "if",
            Tok::Else => "else",
            Tok::While => "while",
            Tok::For => "for",
            Tok::In => "in",
            Tok::Return => "return",
            Tok::Break => "break",
            Tok::Continue => "continue",
            Tok::True => "true",
            Tok::False => "false",
            Tok::Null => "null",
            Tok::Is => "is",
            Tok::New => "new",
            Tok::Read => "read",
            Tok::Ref => "ref",
            Tok::Public => "public",
            Tok::Struct => "struct",
            Tok::Enum => "enum",
            Tok::Match => "match",
            Tok::Try => "try",
            Tok::FatArrow => "=>",
            Tok::LParen => "(",
            Tok::RParen => ")",
            Tok::LBrace => "{",
            Tok::RBrace => "}",
            Tok::LBracket => "[",
            Tok::RBracket => "]",
            Tok::Comma => ",",
            Tok::Dot => ".",
            Tok::DotDot => "..",
            Tok::Plus => "+",
            Tok::Minus => "-",
            Tok::Star => "*",
            Tok::Slash => "/",
            Tok::Percent => "%",
            Tok::Assign => "=",
            Tok::PlusEq => "+=",
            Tok::MinusEq => "-=",
            Tok::StarEq => "*=",
            Tok::SlashEq => "/=",
            Tok::PercentEq => "%=",
            Tok::Eq => "==",
            Tok::Ne => "!=",
            Tok::Lt => "<",
            Tok::Le => "<=",
            Tok::Gt => ">",
            Tok::Ge => ">=",
            Tok::AndAnd => "&&",
            Tok::OrOr => "||",
            Tok::Bang => "!",
            Tok::Question => "?",
            Tok::QuestionQuestion => "??",
            _ => "",
        }
    }
}

/// Tokenizes `range`. `nested` is for an expression inside `$"..."`, where line ends
/// carry no meaning.
pub fn lex(src: &str, range: Span, nested: bool) -> Result<Vec<Token>, Diag> {
    let mut lexer = Lexer {
        src,
        bytes: src.as_bytes(),
        pos: range.start,
        end: range.end,
        toks: Vec::new(),
        nest: Vec::new(),
        nested,
    };
    lexer.run()?;
    Ok(lexer.toks)
}

struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    end: usize,
    toks: Vec<Token>,
    /// Open brackets; the innermost one decides whether a line end is significant.
    nest: Vec<Tok>,
    nested: bool,
}

impl<'a> Lexer<'a> {
    fn at(&self, offset: usize) -> Option<u8> {
        let i = self.pos + offset;
        if i < self.end {
            Some(self.bytes[i])
        } else {
            None
        }
    }

    fn push(&mut self, tok: Tok, start: usize) {
        self.toks.push(Token { tok, span: Span::new(start, self.pos) });
    }

    fn run(&mut self) -> Result<(), Diag> {
        while let Some(c) = self.at(0) {
            let start = self.pos;
            match c {
                b' ' | b'\t' | b'\r' => self.pos += 1,
                b'\n' => {
                    self.pos += 1;
                    self.newline(start);
                }
                b'/' if self.at(1) == Some(b'/') => {
                    while self.at(0).is_some_and(|c| c != b'\n') {
                        self.pos += 1;
                    }
                }
                b'0'..=b'9' => self.number()?,
                b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.word(),
                b'"' => {
                    self.pos += 1;
                    let text = self.string_body(start)?;
                    self.push(Tok::Str(text), start);
                }
                b'$' if self.at(1) == Some(b'"') => self.interpolated()?,
                _ => self.punct()?,
            }
        }
        let end = self.end;
        self.toks.push(Token { tok: Tok::Eof, span: Span::new(end, end) });
        Ok(())
    }

    fn newline(&mut self, start: usize) {
        let significant = !self.nested && matches!(self.nest.last(), None | Some(Tok::LBrace));
        if !significant || self.next_line_starts_with_dot() {
            return;
        }
        if let Some(last) = self.toks.last() {
            if last.tok != Tok::Newline && !last.tok.continues_line() {
                self.toks.push(Token { tok: Tok::Newline, span: Span::new(start, start + 1) });
            }
        }
    }

    /// A line starting with `.` continues the previous one, so method chains can be
    /// written one call per line.
    fn next_line_starts_with_dot(&self) -> bool {
        let mut i = self.pos;
        while i < self.end {
            match self.bytes[i] {
                b' ' | b'\t' | b'\r' | b'\n' => i += 1,
                b'/' if self.bytes.get(i + 1) == Some(&b'/') => {
                    while i < self.end && self.bytes[i] != b'\n' {
                        i += 1;
                    }
                }
                b'.' => return self.bytes.get(i + 1) != Some(&b'.'),
                _ => return false,
            }
        }
        false
    }

    fn number(&mut self) -> Result<(), Diag> {
        let start = self.pos;
        while self.at(0).is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
        }
        if self.at(0).is_some_and(|c| c.is_ascii_alphabetic() || c == b'_') {
            while self.at(0).is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_') {
                self.pos += 1;
            }
            return Err(Diag::error(Span::new(start, self.pos), "a number cannot be followed by a letter"));
        }
        if self.at(0) == Some(b'.') && self.at(1).is_some_and(|c| c.is_ascii_digit()) {
            return Err(Diag::error(
                Span::new(start, self.pos),
                "fractional numbers are not in the interpreter yet",
            ));
        }
        let text = &self.src[start..self.pos];
        match text.parse::<u64>() {
            Ok(value) => {
                self.push(Tok::Int(value), start);
                Ok(())
            }
            Err(_) => Err(Diag::error(Span::new(start, self.pos), "number does not fit in any integer type")),
        }
    }

    fn word(&mut self) {
        let start = self.pos;
        while self.at(0).is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_') {
            self.pos += 1;
        }
        let tok = match &self.src[start..self.pos] {
            "var" => Tok::Var,
            "if" => Tok::If,
            "else" => Tok::Else,
            "while" => Tok::While,
            "for" => Tok::For,
            "in" => Tok::In,
            "return" => Tok::Return,
            "break" => Tok::Break,
            "continue" => Tok::Continue,
            "true" => Tok::True,
            "false" => Tok::False,
            "null" => Tok::Null,
            "is" => Tok::Is,
            "new" => Tok::New,
            "read" => Tok::Read,
            "ref" => Tok::Ref,
            "public" => Tok::Public,
            "struct" => Tok::Struct,
            "enum" => Tok::Enum,
            "match" => Tok::Match,
            "try" => Tok::Try,
            name => Tok::Ident(name.to_string()),
        };
        self.push(tok, start);
    }

    /// Reads what follows the opening quote and consumes the closing quote.
    fn string_body(&mut self, start: usize) -> Result<String, Diag> {
        let mut text = Vec::new();
        loop {
            match self.at(0) {
                None | Some(b'\n') => return Err(self.unterminated(start)),
                Some(b'"') => {
                    self.pos += 1;
                    break;
                }
                Some(b'\\') => text.push(self.escape()?),
                Some(c) => {
                    text.push(c);
                    self.pos += 1;
                }
            }
        }
        Ok(String::from_utf8(text).expect("source is valid UTF-8"))
    }

    fn unterminated(&self, start: usize) -> Diag {
        Diag::error(Span::new(start, self.pos), "unterminated string").hint("close it with `\"` before the line ends")
    }

    fn escape(&mut self) -> Result<u8, Diag> {
        let start = self.pos;
        self.pos += 1;
        let value = match self.at(0) {
            Some(b'n') => b'\n',
            Some(b't') => b'\t',
            Some(b'r') => b'\r',
            Some(b'0') => 0,
            Some(b'\\') => b'\\',
            Some(b'"') => b'"',
            _ => {
                return Err(Diag::error(Span::new(start, self.pos + 1), "unknown escape sequence")
                    .hint("valid ones: \\n \\t \\r \\0 \\\\ \\\""))
            }
        };
        self.pos += 1;
        Ok(value)
    }

    fn interpolated(&mut self) -> Result<(), Diag> {
        let start = self.pos;
        self.pos += 2;
        let mut parts = Vec::new();
        let mut text = Vec::new();
        loop {
            match self.at(0) {
                None | Some(b'\n') => return Err(self.unterminated(start)),
                Some(b'"') => {
                    self.pos += 1;
                    break;
                }
                Some(b'\\') => text.push(self.escape()?),
                Some(b'{') if self.at(1) == Some(b'{') => {
                    text.push(b'{');
                    self.pos += 2;
                }
                Some(b'}') if self.at(1) == Some(b'}') => {
                    text.push(b'}');
                    self.pos += 2;
                }
                Some(b'}') => {
                    return Err(Diag::error(Span::new(self.pos, self.pos + 1), "unmatched `}`")
                        .hint("write `}}` for a literal brace"))
                }
                Some(b'{') => {
                    if !text.is_empty() {
                        let done = std::mem::take(&mut text);
                        parts.push(Part::Text(String::from_utf8(done).expect("source is valid UTF-8")));
                    }
                    parts.push(Part::Code(self.interpolation_code(start)?));
                }
                Some(c) => {
                    text.push(c);
                    self.pos += 1;
                }
            }
        }
        if !text.is_empty() {
            parts.push(Part::Text(String::from_utf8(text).expect("source is valid UTF-8")));
        }
        self.push(Tok::Interp(parts), start);
        Ok(())
    }

    /// Called on `{`; moves past the matching `}` and returns the range in between.
    fn interpolation_code(&mut self, string_start: usize) -> Result<Span, Diag> {
        let open = self.pos;
        self.pos += 1;
        let code_start = self.pos;
        let mut depth = 0;
        loop {
            match self.at(0) {
                None | Some(b'\n') => return Err(self.unterminated(string_start)),
                Some(b'"') => {
                    let inner = self.pos;
                    self.pos += 1;
                    self.string_body(inner)?;
                }
                Some(b'{') => {
                    depth += 1;
                    self.pos += 1;
                }
                Some(b'}') if depth == 0 => break,
                Some(b'}') => {
                    depth -= 1;
                    self.pos += 1;
                }
                Some(_) => self.pos += 1,
            }
        }
        let code = Span::new(code_start, self.pos);
        self.pos += 1;
        if self.src[code.start..code.end].trim().is_empty() {
            return Err(Diag::error(Span::new(open, self.pos), "no expression inside `{ }`")
                .hint("write `{{` for a literal brace"));
        }
        Ok(code)
    }

    fn punct(&mut self) -> Result<(), Diag> {
        let start = self.pos;
        let next = self.at(1);
        let (tok, len) = match (self.bytes[start], next) {
            (b'=', Some(b'=')) => (Tok::Eq, 2),
            (b'=', Some(b'>')) => (Tok::FatArrow, 2),
            (b'!', Some(b'=')) => (Tok::Ne, 2),
            (b'<', Some(b'=')) => (Tok::Le, 2),
            (b'>', Some(b'=')) => (Tok::Ge, 2),
            (b'&', Some(b'&')) => (Tok::AndAnd, 2),
            (b'|', Some(b'|')) => (Tok::OrOr, 2),
            (b'+', Some(b'=')) => (Tok::PlusEq, 2),
            (b'-', Some(b'=')) => (Tok::MinusEq, 2),
            (b'*', Some(b'=')) => (Tok::StarEq, 2),
            (b'/', Some(b'=')) => (Tok::SlashEq, 2),
            (b'%', Some(b'=')) => (Tok::PercentEq, 2),
            (b'.', Some(b'.')) => (Tok::DotDot, 2),
            (b'?', Some(b'?')) => (Tok::QuestionQuestion, 2),
            (b'?', _) => (Tok::Question, 1),
            (b'=', _) => (Tok::Assign, 1),
            (b'!', _) => (Tok::Bang, 1),
            (b'<', _) => (Tok::Lt, 1),
            (b'>', _) => (Tok::Gt, 1),
            (b'+', _) => (Tok::Plus, 1),
            (b'-', _) => (Tok::Minus, 1),
            (b'*', _) => (Tok::Star, 1),
            (b'/', _) => (Tok::Slash, 1),
            (b'%', _) => (Tok::Percent, 1),
            (b'.', _) => (Tok::Dot, 1),
            (b',', _) => (Tok::Comma, 1),
            (b'(', _) => (Tok::LParen, 1),
            (b')', _) => (Tok::RParen, 1),
            (b'{', _) => (Tok::LBrace, 1),
            (b'}', _) => (Tok::RBrace, 1),
            (b'[', _) => (Tok::LBracket, 1),
            (b']', _) => (Tok::RBracket, 1),
            _ => {
                let ch = self.src[start..].chars().next().unwrap_or('?');
                let span = Span::new(start, start + ch.len_utf8());
                let diag = Diag::error(span, format!("unexpected character `{ch}`"));
                return Err(match ch {
                    ';' => diag.hint("there are no semicolons; a statement ends at the end of the line"),
                    '\'' => diag.note("character literals are not in the interpreter yet"),
                    _ => diag,
                });
            }
        };
        self.pos += len;
        match tok {
            Tok::LParen | Tok::LBracket | Tok::LBrace => self.nest.push(tok.clone()),
            Tok::RParen | Tok::RBracket | Tok::RBrace => {
                self.nest.pop();
            }
            _ => {}
        }
        self.push(tok, start);
        Ok(())
    }
}
