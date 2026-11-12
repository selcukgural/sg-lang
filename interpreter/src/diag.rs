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
}

impl Diag {
    pub fn error(span: Span, msg: impl Into<String>) -> Diag {
        Diag { kind: Kind::Error, msg: msg.into(), span, notes: Vec::new(), related: None, hint: None }
    }

    pub fn panic(span: Span, msg: impl Into<String>) -> Diag {
        Diag { kind: Kind::Panic, msg: msg.into(), span, notes: Vec::new(), related: None, hint: None }
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Diag {
        self.hint = Some(hint.into());
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
        let label = match self.kind {
            Kind::Error => "error",
            Kind::Panic => "panic",
        };
        let mut text = format!("{label}: {}\n", self.msg);
        for note in &self.notes {
            text.push_str(&format!("  {note}\n"));
        }
        text.push_str(&snippet(self.span, src, path));
        if let Some((span, note)) = self.related.as_deref() {
            text.push_str(&format!("  {note}:\n"));
            text.push_str(&snippet(*span, src, path));
        }
        if let Some(hint) = &self.hint {
            text.push_str(&format!("  hint: {hint}\n"));
        }
        text
    }
}

/// Location, source line and a marker under the span.
fn snippet(span: Span, src: &str, path: &str) -> String {
    let start = span.start.min(src.len());
    let line_start = src[..start].rfind('\n').map_or(0, |i| i + 1);
    let line_end = src[start..].find('\n').map_or(src.len(), |i| start + i);
    let line_no = src[..start].matches('\n').count() + 1;
    let line = &src[line_start..line_end];
    let col = src[line_start..start].chars().count();
    let span_end = span.end.clamp(start, line_end);
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

/// Finds the closest name, for "did you mean" hints.
pub fn nearest<'a>(name: &str, candidates: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let limit = (name.chars().count() / 3).max(1);
    candidates
        .map(|c| (distance(name, c), c))
        .filter(|(d, _)| *d <= limit)
        .min_by_key(|(d, _)| *d)
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
