//! Splits source text into tokens and applies the line-end rule.
//!
//! Line-end rule: a line end terminates the statement. Three exceptions: inside `(` or `[`,
//! when the line ends with an operator, a comma, a dot or an opening bracket, and when the
//! next line starts with a dot, the statement continues on the next line.

use crate::ast::{Letter, Spec};
use crate::diag::{Diag, Span};

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Int(u64),
    /// A fractional number as written, without the `_`: `1.5`, `2e-3`.
    Float(String),
    Str(String),
    Char(char),
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
    /// `or`: joins the patterns of an arm.
    Or,
    New,
    Read,
    Ref,
    Public,
    Readonly,
    Use,
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
    Colon,
    Dot,
    DotDot,
    /// `..=`: a range that takes its end too, in a pattern.
    DotDotEq,

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
    Amp,
    Pipe,
    Caret,
    Tilde,
    /// `<<`. There is no token for `>>`: inside a type, `List<List<int>>` ends with two `>`,
    /// so the parser reads two `>` with nothing between them as the shift.
    Shl,
    AmpEq,
    PipeEq,
    CaretEq,
    ShlEq,
    ShrEq,

    Newline,
    Eof,
}

/// A piece of `$"..."`: plain text, or the source range of an expression inside `{ }`.
#[derive(Clone, Debug, PartialEq)]
pub enum Part {
    Text(String),
    /// The code of a value, and what is written after it (`,8`, `:X2`).
    Code(Span, Option<Spec>),
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
                | Tok::DotDotEq
                | Tok::Or
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
                | Tok::Amp
                | Tok::Pipe
                | Tok::Caret
                | Tok::Tilde
                | Tok::Shl
                | Tok::AmpEq
                | Tok::PipeEq
                | Tok::CaretEq
                | Tok::ShlEq
                | Tok::ShrEq
                | Tok::LParen
                | Tok::LBracket
                | Tok::LBrace
        )
    }

    pub fn describe(&self) -> String {
        match self {
            Tok::Int(v) => format!("`{v}`"),
            Tok::Float(text) => format!("`{text}`"),
            Tok::Str(_) | Tok::Interp(_) => "a string".to_string(),
            Tok::Char(_) => "a character".to_string(),
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
            Tok::Or => "or",
            Tok::New => "new",
            Tok::Read => "read",
            Tok::Ref => "ref",
            Tok::Public => "public",
            Tok::Readonly => "readonly",
            Tok::Use => "use",
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
            Tok::Colon => ":",
            Tok::Dot => ".",
            Tok::DotDot => "..",
            Tok::DotDotEq => "..=",
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
            Tok::Amp => "&",
            Tok::Pipe => "|",
            Tok::Caret => "^",
            Tok::Tilde => "~",
            Tok::Shl => "<<",
            Tok::AmpEq => "&=",
            Tok::PipeEq => "|=",
            Tok::CaretEq => "^=",
            Tok::ShlEq => "<<=",
            Tok::ShrEq => ">>=",
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
    /// Bytes of the character at the current position (1 at the end of the source).
    fn char_len(&self) -> usize {
        self.at(0).map_or(1, utf8_len)
    }

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
                b'"' if self.quotes_at(self.pos) >= 3 => self.raw(start, false)?,
                b'$' if self.quotes_at(self.pos + 1) >= 3 => {
                    self.pos += 1;
                    self.raw(start, true)?
                }
                b'"' => {
                    self.pos += 1;
                    let text = self.string_body(start)?;
                    self.push(Tok::Str(text), start);
                }
                b'$' if self.at(1) == Some(b'"') => self.interpolated()?,
                b'\'' => self.character()?,
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

    /// `42`, `0xde`, `0b1010`, and `_` between digits: `1_000_000`, `0xdead_beef`. A
    /// fractional number has digits on both sides of the point, an exponent, or both:
    /// `1.5`, `1e9`, `6.674_30e-11`.
    fn number(&mut self) -> Result<(), Diag> {
        let start = self.pos;
        let (radix, name) = match (self.at(0), self.at(1)) {
            (Some(b'0'), Some(b'x')) => (16, "hex "),
            (Some(b'0'), Some(b'b')) => (2, "binary "),
            _ => (10, ""),
        };
        if radix == 10 {
            return self.decimal();
        }
        self.pos += 2;
        let digits = self.pos;
        while self.at(0).is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_') {
            self.pos += 1;
        }
        let span = Span::new(start, self.pos);
        let text = &self.src[digits..self.pos];
        if text.is_empty() {
            let prefix = &self.src[start..digits];
            return Err(Diag::error(span, format!("`{prefix}` needs digits after it")).hint(match radix {
                16 => "a hex number: `0xff`",
                _ => "a binary number: `0b1010`",
            }));
        }
        if let Some(bad) = text.bytes().find(|c| *c != b'_' && !(*c as char).is_digit(radix)) {
            return Err(Diag::error(span, format!("`{}` is not a {name}digit", bad as char)));
        }
        if text.starts_with('_') || text.ends_with('_') || text.contains("__") {
            return Err(Diag::error(span, "`_` in a number stands between two digits").hint("`1_000_000`"));
        }
        if self.at(0) == Some(b'.') && self.at(1).is_some_and(|c| c.is_ascii_digit()) {
            return Err(Diag::error(span, format!("a {name}number has no fractional part")));
        }
        let plain: String = text.chars().filter(|c| *c != '_').collect();
        match u64::from_str_radix(&plain, radix) {
            Ok(value) => {
                self.push(Tok::Int(value), start);
                Ok(())
            }
            Err(_) => Err(Diag::error(span, "number does not fit in any integer type")),
        }
    }

    /// Digits and `_`; the caller checks where the `_` stand.
    fn digits(&mut self) {
        while self.at(0).is_some_and(|c| c.is_ascii_digit() || c == b'_') {
            self.pos += 1;
        }
    }

    fn decimal(&mut self) -> Result<(), Diag> {
        let start = self.pos;
        self.digits();
        let mut fractional = false;
        if self.at(0) == Some(b'.') && self.at(1).is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
            self.digits();
            fractional = true;
        }
        let exponent = |c: Option<u8>| c.is_some_and(|c| c.is_ascii_digit());
        if matches!(self.at(0), Some(b'e' | b'E'))
            && (exponent(self.at(1)) || (matches!(self.at(1), Some(b'+' | b'-')) && exponent(self.at(2))))
        {
            if self.at(0) == Some(b'E') {
                let at = Span::new(self.pos, self.pos + 1);
                return Err(Diag::error(at, "the exponent is written with a small `e`").hint(format!(
                    "`{}e{}`",
                    &self.src[start..self.pos],
                    &self.src[self.pos + 1..self.pos + 1 + self.src[self.pos + 1..].bytes().take_while(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'-' | b'_')).count()]
                )));
            }
            self.pos += 1;
            if matches!(self.at(0), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            self.digits();
            fractional = true;
        }
        let end = self.pos;
        while self.at(0).is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_') {
            self.pos += 1;
        }
        let span = Span::new(start, self.pos);
        let text = &self.src[start..end];
        if end != self.pos {
            if !fractional && (text == "0" && matches!(self.bytes.get(end), Some(b'X' | b'B'))) {
                let written = &self.src[start..self.pos];
                let lower = written[..2].to_ascii_lowercase();
                return Err(Diag::error(span, format!("`{}` is written with a small letter", &written[..2])).hint(format!("`{lower}{}`", &written[2..])));
            }
            let suffix = &self.src[end..self.pos];
            if fractional && matches!(suffix, "f" | "d" | "F" | "D" | "m" | "M") {
                return Err(Diag::error(span, format!("a number has no `{suffix}` at its end; it takes the type it is used as"))
                    .hint(format!("`float x = {text}`")));
            }
            return Err(Diag::error(span, "a number cannot be followed by a letter"));
        }
        // Each run of digits: `_` only between two digits.
        let runs = text.split(['.', 'e', '+', '-']);
        if runs.into_iter().any(|run| run.starts_with('_') || run.ends_with('_') || run.contains("__")) {
            return Err(Diag::error(span, "`_` in a number stands between two digits").hint("`1_000_000`"));
        }
        let plain: String = text.chars().filter(|c| *c != '_').collect();
        if fractional {
            self.push(Tok::Float(plain), start);
            return Ok(());
        }
        match plain.parse::<u64>() {
            Ok(value) => {
                self.push(Tok::Int(value), start);
                Ok(())
            }
            Err(_) => Err(Diag::error(span, "number does not fit in any integer type")),
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
            "or" => Tok::Or,
            "new" => Tok::New,
            "read" => Tok::Read,
            "ref" => Tok::Ref,
            "public" => Tok::Public,
            "readonly" => Tok::Readonly,
            "use" => Tok::Use,
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
                Some(b'\\') => {
                    let value = self.escape()?;
                    text.extend_from_slice(value.encode_utf8(&mut [0; 4]).as_bytes());
                }
                Some(c) => {
                    text.push(c);
                    self.pos += 1;
                }
            }
        }
        Ok(String::from_utf8(text).expect("source is valid UTF-8"))
    }

    /// How many `"` stand one after the other from `at`, within the range.
    fn quotes_at(&self, at: usize) -> usize {
        self.bytes[at.min(self.end)..self.end].iter().take_while(|&&byte| byte == b'"').count()
    }

    /// A raw text, C#'s: `"""a "quoted" word"""` on one line, or `"""` at the end of a line,
    /// lines, and `"""` on a line of its own, whose indentation is taken off every line. Nothing
    /// is an escape; more quotes open and close a text that holds `"""`. With `$` in front
    /// (`interpolated`), `{value}` and `{{` are read as in `$"..."`.
    fn raw(&mut self, start: usize, interpolated: bool) -> Result<(), Diag> {
        let quotes = self.quotes_at(self.pos);
        self.pos += quotes;
        let line_end = self.bytes[self.pos..self.end].iter().position(|&byte| byte == b'\n').map_or(self.end, |at| self.pos + at);
        let (mut parts, mut text) = (Vec::new(), Vec::new());
        let blank = |bytes: &[u8]| bytes.iter().all(|&byte| matches!(byte, b' ' | b'\t' | b'\r'));
        if !blank(&self.bytes[self.pos..line_end]) {
            // One line: the text ends at the first `"""`.
            if !self.raw_piece(line_end, quotes, true, interpolated, start, &mut parts, &mut text)? {
                return Err(Diag::error(Span::new(start, line_end), "unterminated raw text")
                    .hint(format!("close it with `{}` on the same line, or start its lines on the next one", "\"".repeat(quotes))));
            }
        } else {
            // Lines: find the one that closes the text first, for the indentation.
            let mut lines = Vec::new();
            let mut at = line_end;
            let close = loop {
                if at >= self.end {
                    return Err(Diag::error(Span::new(start, self.pos), "unterminated raw text")
                        .hint(format!("close it with `{}` on a line of its own", "\"".repeat(quotes))));
                }
                let begin = at + 1;
                let end = self.bytes[begin..self.end].iter().position(|&byte| byte == b'\n').map_or(self.end, |n| begin + n);
                let indent = self.bytes[begin..end].iter().take_while(|&&byte| byte == b' ' || byte == b'\t').count();
                let run = self.quotes_at(begin + indent);
                if run >= quotes {
                    if run > quotes {
                        return Err(Diag::error(Span::new(begin + indent, begin + indent + run), format!("{run} quotes close a raw text opened with {quotes}"))
                            .hint("open and close it with as many quotes"));
                    }
                    break (begin, indent);
                }
                lines.push((begin, end));
                at = end;
            };
            let (close_line, indent) = close;
            let prefix = &self.bytes[close_line..close_line + indent];
            for (number, &(begin, end)) in lines.iter().enumerate() {
                if number > 0 {
                    text.push(b'\n');
                }
                let end = if end > begin && self.bytes[end - 1] == b'\r' { end - 1 } else { end };
                if blank(&self.bytes[begin..end]) && end - begin <= indent {
                    continue;
                }
                if !self.bytes[begin..end].starts_with(prefix) {
                    let written = self.bytes[begin..end].iter().take_while(|&&byte| byte == b' ' || byte == b'\t').count();
                    return Err(Diag::error(Span::new(begin, begin + written.max(utf8_len(self.bytes[begin]))), "this line is indented less than the closing quotes of its raw text")
                        .hint("indent every line at least as far as the closing quotes, with the same spaces or tabs"));
                }
                self.pos = begin + indent;
                self.raw_piece(end, quotes, false, interpolated, start, &mut parts, &mut text)?;
            }
            self.pos = close_line + indent + quotes;
        }
        if !interpolated {
            self.push(Tok::Str(String::from_utf8(text).expect("source is valid UTF-8")), start);
            return Ok(());
        }
        if !text.is_empty() {
            parts.push(Part::Text(String::from_utf8(text).expect("source is valid UTF-8")));
        }
        self.push(Tok::Interp(parts), start);
        Ok(())
    }

    /// Reads a raw text up to `end`, the end of a line, into `text` and `parts`. `one_line`: the
    /// text is on one line and ends at its closing quotes, after which the position stands and
    /// `true` is given.
    #[allow(clippy::too_many_arguments)]
    fn raw_piece(&mut self, end: usize, quotes: usize, one_line: bool, interpolated: bool, start: usize, parts: &mut Vec<Part>, text: &mut Vec<u8>) -> Result<bool, Diag> {
        while self.pos < end {
            match self.bytes[self.pos] {
                b'"' => {
                    let run = self.quotes_at(self.pos).min(end - self.pos);
                    if run == quotes && one_line {
                        self.pos += run;
                        return Ok(true);
                    }
                    if run >= quotes {
                        let span = Span::new(self.pos, self.pos + run);
                        return Err(if one_line {
                            Diag::error(span, format!("{run} quotes close a raw text opened with {quotes}"))
                                .hint("a raw text on one line neither starts nor ends with `\"`; open and close it with more quotes than it holds")
                        } else {
                            Diag::error(span, format!("{run} quotes inside a raw text opened with {quotes}"))
                                .hint(format!("open and close it with {} quotes, and the closing ones stand on a line of their own", run + 1))
                        });
                    }
                    text.extend(std::iter::repeat_n(b'"', run));
                    self.pos += run;
                }
                b'{' if interpolated && self.at(1) == Some(b'{') => {
                    text.push(b'{');
                    self.pos += 2;
                }
                b'}' if interpolated && self.at(1) == Some(b'}') => {
                    text.push(b'}');
                    self.pos += 2;
                }
                b'}' if interpolated => {
                    return Err(Diag::error(Span::new(self.pos, self.pos + 1), "unmatched `}`").hint("write `}}` for a literal brace"))
                }
                b'{' if interpolated => {
                    if !text.is_empty() {
                        parts.push(Part::Text(String::from_utf8(std::mem::take(text)).expect("source is valid UTF-8")));
                    }
                    let (code, spec) = self.interpolation_code(start)?;
                    parts.push(Part::Code(code, spec));
                }
                byte => {
                    text.push(byte);
                    self.pos += 1;
                }
            }
        }
        Ok(false)
    }

    fn unterminated(&self, start: usize) -> Diag {
        Diag::error(Span::new(start, self.pos), "unterminated string").hint("close it with `\"` before the line ends")
    }

    fn escape(&mut self) -> Result<char, Diag> {
        let start = self.pos;
        self.pos += 1;
        let value = match self.at(0) {
            Some(b'n') => '\n',
            Some(b't') => '\t',
            Some(b'r') => '\r',
            Some(b'0') => '\0',
            Some(b'\\') => '\\',
            Some(b'"') => '"',
            Some(b'\'') => '\'',
            // `\u{1F600}`: a character by its code, in hex, as `print` writes one it cannot show.
            Some(b'u') if self.at(1) == Some(b'{') => {
                let digits_start = self.pos + 2;
                let mut end = digits_start;
                let bytes = self.src.as_bytes();
                while bytes.get(end).is_some_and(u8::is_ascii_hexdigit) {
                    end += 1;
                }
                let digits = &self.src[digits_start..end];
                let closed = bytes.get(end) == Some(&b'}');
                let span = Span::new(start, if closed { end + 1 } else { end });
                if !closed || digits.is_empty() || digits.len() > 6 {
                    return Err(Diag::error(span, "a character by its code is written `\\u{...}` with one to six hex digits").hint("`\\u{e9}` is `é`"));
                }
                let code = u32::from_str_radix(digits, 16).expect("checked to be hex");
                let Some(value) = char::from_u32(code) else {
                    return Err(Diag::error(span, format!("0x{digits} is not the code of a character"))
                        .note("a character is from 0 to 0x10FFFF, and not from 0xD800 to 0xDFFF"));
                };
                self.pos = end;
                value
            }
            _ => {
                return Err(Diag::error(Span::new(start, self.pos + self.char_len()), "unknown escape sequence")
                    .hint("valid ones: \\n \\t \\r \\0 \\\\ \\\" \\' \\u{...}"))
            }
        };
        self.pos += 1;
        Ok(value)
    }

    /// `'+'`, `'\n'`, `'é'`: exactly one character between single quotes.
    fn character(&mut self) -> Result<(), Diag> {
        let start = self.pos;
        self.pos += 1;
        let unterminated = |lexer: &Self| {
            Diag::error(Span::new(start, lexer.pos), "unterminated character").hint("close it with `'`: `'+'`")
        };
        let value = match self.at(0) {
            None | Some(b'\n') => return Err(unterminated(self)),
            Some(b'\'') => {
                self.pos += 1;
                return Err(Diag::error(Span::new(start, self.pos), "there is no character between the quotes")
                    .hint("a space is written `' '`"));
            }
            Some(b'\\') => self.escape()?,
            Some(_) => {
                let ch = self.src[self.pos..self.end].chars().next().expect("a character starts here");
                self.pos += ch.len_utf8();
                ch
            }
        };
        if self.at(0) != Some(b'\'') {
            // More than one character: find the closing quote to mark the whole of it.
            let mut end = self.pos;
            while end < self.end && self.bytes[end] != b'\n' && self.bytes[end] != b'\'' {
                end += 1;
            }
            if end == self.end || self.bytes[end] == b'\n' {
                return Err(unterminated(self));
            }
            let text = &self.src[start + 1..end];
            return Err(Diag::error(Span::new(start, end + 1), "single quotes hold exactly one character")
                .hint(format!("text is written with double quotes: `\"{text}\"`")));
        }
        self.pos += 1;
        self.push(Tok::Char(value), start);
        Ok(())
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
                Some(b'\\') => {
                    let value = self.escape()?;
                    text.extend_from_slice(value.encode_utf8(&mut [0; 4]).as_bytes());
                }
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
                    let (code, spec) = self.interpolation_code(start)?;
                    parts.push(Part::Code(code, spec));
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

    /// Called on `{`; moves past the matching `}` and returns the range of the value in
    /// between, with what is written after it. `:` and `,` belong to the value only inside a
    /// bracket of its own: no expression has a `:`, and a `,` outside brackets is a width when
    /// a number follows it (`{n,5}`) and part of a type otherwise (`{HashMap<string, int>()...}`).
    fn interpolation_code(&mut self, string_start: usize) -> Result<(Span, Option<Spec>), Diag> {
        let open = self.pos;
        self.pos += 1;
        let code_start = self.pos;
        let (mut depth, mut nest) = (0, 0);
        let (mut comma, mut colon) = (None, None);
        loop {
            match self.at(0) {
                None | Some(b'\n') => return Err(self.unterminated(string_start)),
                Some(b'"') => {
                    let inner = self.pos;
                    self.pos += 1;
                    self.string_body(inner)?;
                }
                Some(b'\'') => {
                    self.pos += 1;
                    while !matches!(self.at(0), None | Some(b'\n' | b'\'')) {
                        self.pos += if self.at(0) == Some(b'\\') { 2 } else { 1 };
                    }
                    if self.at(0) == Some(b'\'') {
                        self.pos += 1;
                    }
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
                Some(b'(' | b'[') => {
                    nest += 1;
                    self.pos += 1;
                }
                Some(b')' | b']') => {
                    nest -= 1;
                    self.pos += 1;
                }
                Some(b':') if depth == 0 && nest <= 0 && colon.is_none() => {
                    colon = Some(self.pos);
                    self.pos += 1;
                }
                Some(b',') if depth == 0 && nest <= 0 && colon.is_none() => {
                    comma = Some(self.pos);
                    self.pos += 1;
                }
                Some(_) => self.pos += 1,
            }
        }
        let close = self.pos;
        self.pos += 1;
        let mut code_end = colon.unwrap_or(close);
        let mut width = 0;
        let mut spec_start = colon;
        if let Some(at) = comma {
            let written = self.src[at + 1..code_end].trim();
            let digits = written.strip_prefix('-').unwrap_or(written);
            if !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()) {
                let span = Span::new(at, code_end);
                if digits.len() > 3 {
                    return Err(Diag::error(span, "a width is at most 999 characters"));
                }
                width = written.parse::<i32>().expect("checked to be a number");
                (code_end, spec_start) = (at, Some(at));
            }
        }
        let code = Span::new(code_start, code_end);
        if self.src[code.start..code.end].trim().is_empty() {
            return Err(Diag::error(Span::new(open, self.pos), "no expression inside `{ }`")
                .hint("write `{{` for a literal brace"));
        }
        let letter = match colon {
            Some(at) => Some(self.format_letter(Span::new(at + 1, close))?),
            None => None,
        };
        let spec = spec_start.map(|start| Spec { width, letter, span: Span::new(start, close) });
        Ok((code, spec))
    }

    /// What follows `:` inside `{ }`: one letter and the number of digits, as in C#.
    fn format_letter(&self, span: Span) -> Result<(Letter, Option<u32>), Diag> {
        const KNOWN: &str = "formats: `x` and `X` (hex), `b` (binary), `D` (at least so many digits), `F` (so many digits after the point), `e` and `E` (C's `%e`), `g` and `G` (C's `%g`)";
        let written = &self.src[span.start..span.end];
        let Some(first) = written.chars().next() else {
            return Err(Diag::error(Span::new(span.start - 1, span.end), "nothing after `:`").note(KNOWN));
        };
        let digits = &written[first.len_utf8()..];
        let letter = match first {
            'x' => Letter::Hex,
            'X' => Letter::HexUpper,
            'b' | 'B' => Letter::Binary,
            'd' | 'D' => Letter::Decimal,
            'f' | 'F' => Letter::Fixed,
            'e' => Letter::Exponent,
            'E' => Letter::ExponentUpper,
            'g' => Letter::General,
            'G' => Letter::GeneralUpper,
            'n' | 'N' => {
                return Err(Diag::error(span, format!("`{first}` is not a format here"))
                    .note("`N` would put separators between groups of digits, which change from one country to another")
                    .hint(format!("`F{digits}` writes the digits after the point")))
            }
            _ => return Err(Diag::error(span, format!("`{written}` is not a format")).note(KNOWN)),
        };
        if !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(Diag::error(span, format!("`{written}` is not a format"))
                .note(format!("after `{first}` comes a number of digits, and nothing else: `{first}2`")));
        }
        if digits.len() > 3 {
            return Err(Diag::error(span, "the number after the letter is at most 999"));
        }
        let count = (!digits.is_empty()).then(|| digits.parse::<u32>().expect("checked to be a number"));
        if letter == Letter::Fixed && count.is_none() {
            return Err(Diag::error(span, "`F` needs the number of digits after the point")
                .hint(format!("`{first}2`")));
        }
        Ok((letter, count))
    }

    fn punct(&mut self) -> Result<(), Diag> {
        let start = self.pos;
        let next = self.at(1);
        let (tok, len) = match (self.bytes[start], next) {
            (b'<', Some(b'<')) if self.at(2) == Some(b'=') => (Tok::ShlEq, 3),
            (b'>', Some(b'>')) if self.at(2) == Some(b'=') => (Tok::ShrEq, 3),
            (b'<', Some(b'<')) => (Tok::Shl, 2),
            (b'&', Some(b'=')) => (Tok::AmpEq, 2),
            (b'|', Some(b'=')) => (Tok::PipeEq, 2),
            (b'^', Some(b'=')) => (Tok::CaretEq, 2),
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
            (b'.', Some(b'.')) if self.at(2) == Some(b'=') => (Tok::DotDotEq, 3),
            (b'.', Some(b'.')) => (Tok::DotDot, 2),
            (b'?', Some(b'?')) => (Tok::QuestionQuestion, 2),
            (b'?', _) => (Tok::Question, 1),
            (b'=', _) => (Tok::Assign, 1),
            (b'!', _) => (Tok::Bang, 1),
            (b'&', _) => (Tok::Amp, 1),
            (b'|', _) => (Tok::Pipe, 1),
            (b'^', _) => (Tok::Caret, 1),
            (b'~', _) => (Tok::Tilde, 1),
            (b'<', _) => (Tok::Lt, 1),
            (b'>', _) => (Tok::Gt, 1),
            (b'+', _) => (Tok::Plus, 1),
            (b'-', _) => (Tok::Minus, 1),
            (b'*', _) => (Tok::Star, 1),
            (b'/', _) => (Tok::Slash, 1),
            (b'%', _) => (Tok::Percent, 1),
            (b'.', _) => (Tok::Dot, 1),
            (b',', _) => (Tok::Comma, 1),
            (b':', _) => (Tok::Colon, 1),
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

/// Bytes of the UTF-8 character that starts with `lead`.
fn utf8_len(lead: u8) -> usize {
    match lead {
        0xF0.. => 4,
        0xE0.. => 3,
        0xC0.. => 2,
        _ => 1,
    }
}
