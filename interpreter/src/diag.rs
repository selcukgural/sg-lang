//! Error and panic messages. Each carries a source location, optional notes and a hint.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Span {
        Span { start, end }
    }

    pub fn to(self, other: Span) -> Span {
        Span { start: self.start, end: other.end.max(self.start) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// Everything that will be a compile error in the compiler: syntax, names, types.
    Error,
    /// Data-dependent failures only: overflow, division by zero, `assert`, `panic`.
    Panic,
    /// `shared` values still alive when a program or a test ends: they hold each other.
    Leak,
}

#[derive(Debug)]
pub struct Diag {
    pub kind: Kind,
    pub msg: String,
    pub span: Span,
    pub notes: Vec<String>,
    /// A second place worth showing, such as where a value was moved.
    pub related: Option<Box<(Span, String)>>,
    pub hint: Option<String>,
    /// The hint as an edit an editor applies with one click (choice 1371): only where the code it
    /// gives is certain to be what the hint says.
    pub fix: Option<Box<Fix>>,
}

/// An edit of the source: the text at `span` becomes `text` (an empty span inserts it).
#[derive(Clone, Debug, PartialEq)]
pub struct Fix {
    pub span: Span,
    pub text: String,
}

impl Diag {
    pub fn error(span: Span, msg: impl Into<String>) -> Diag {
        Diag { kind: Kind::Error, msg: msg.into(), span, notes: Vec::new(), related: None, hint: None, fix: None }
    }

    pub fn panic(span: Span, msg: impl Into<String>) -> Diag {
        Diag { kind: Kind::Panic, msg: msg.into(), span, notes: Vec::new(), related: None, hint: None, fix: None }
    }

    pub fn leak(span: Span, msg: impl Into<String>) -> Diag {
        Diag { kind: Kind::Leak, msg: msg.into(), span, notes: Vec::new(), related: None, hint: None, fix: None }
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Diag {
        self.hint = Some(hint.into());
        self
    }

    pub fn fix(mut self, span: Span, text: impl Into<String>) -> Diag {
        self.fix = Some(Box::new(Fix { span, text: text.into() }));
        self
    }

    pub fn related(mut self, span: Span, text: impl Into<String>) -> Diag {
        self.related = Some(Box::new((span, text.into())));
        self
    }

    pub fn note(mut self, note: impl Into<String>) -> Diag {
        self.notes.push(note.into());
        self
    }

    /// Renders the message with the source line and a marker under the span.
    pub fn render(&self, src: &str, path: &str) -> String {
        self.render_in(&crate::source::Sources::single(src, path))
    }

    /// The same for a program of several files: each place is shown in its own file.
    pub fn render_in(&self, sources: &crate::source::Sources) -> String {
        let label = match self.kind {
            Kind::Error => "error",
            Kind::Panic => "panic",
            Kind::Leak => "leak",
        };
        let mut text = format!("{label}: {}\n", unprefixed(&self.msg));
        for note in &self.notes {
            text.push_str(&format!("  {}\n", unprefixed(note)));
        }
        text.push_str(&sources.snippet(self.span));
        if let Some((span, note)) = self.related.as_deref() {
            text.push_str(&format!("  {}:\n", unprefixed(note)));
            text.push_str(&sources.snippet(*span));
        }
        if let Some(hint) = &self.hint {
            text.push_str(&format!("  hint: {}\n", unprefixed(hint)));
        }
        text
    }
}

/// A message names the types of the language's own module as a program writes them: `Duration`,
/// not `std.Duration`. No module of a program is named `std`, so the prefix is always theirs.
fn unprefixed(text: &str) -> std::borrow::Cow<'_, str> {
    let prefix = format!("{}.", crate::source::STD_MODULE);
    if !text.contains(&prefix) {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(&prefix) {
        let joined = rest[..at].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.');
        out.push_str(&rest[..at]);
        if joined {
            out.push_str(&prefix);
        }
        rest = &rest[at + prefix.len()..];
    }
    out.push_str(rest);
    std::borrow::Cow::Owned(out)
}

/// Location, source line and a marker under the span.
pub fn snippet(span: Span, src: &str, path: &str) -> String {
    // A span from a byte count may end inside a character: the marker takes the whole character.
    let start = src.floor_char_boundary(span.start.min(src.len()));
    let line_start = src[..start].rfind('\n').map_or(0, |i| i + 1);
    let line_end = src[start..].find('\n').map_or(src.len(), |i| start + i);
    let line_no = src[..start].matches('\n').count() + 1;
    let line = &src[line_start..line_end];
    let col = src[line_start..start].chars().count();
    let span_end = src.ceil_char_boundary(span.end).clamp(start, line_end);
    let width = src[start..span_end].chars().count().max(1);
    let num = line_no.to_string();
    let pad = " ".repeat(num.len());
    format!(
        "  --> {path}:{line_no}:{}\n {num} | {}\n {pad} | {}{}\n",
        col + 1,
        line.replace('\t', " "),
        " ".repeat(col),
        "^".repeat(width)
    )
}

/// Finds the closest name, for "did you mean" hints. Of two names equally close the first in
/// the alphabet wins, so the hint does not depend on the order the candidates come in.
pub fn nearest<'a>(name: &str, candidates: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let limit = (name.chars().count() / 3).max(1);
    candidates
        .map(|c| (distance(name, c), c))
        .filter(|(d, _)| *d <= limit)
        .min()
        .map(|(_, c)| c)
}

fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut prev = row[0];
        row[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            let next = (prev + cost).min(row[j] + 1).min(row[j - 1] + 1);
            prev = row[j];
            row[j] = next;
        }
    }
    row[b.len()]
}
