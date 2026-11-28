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
    /// The message and its notes are the program's own text (a panic's, an assert's, a key's):
    /// shown as written, never worded (1596). Its hint is still the tool's.
    pub as_written: bool,
}

/// An edit of the source: the text at `span` becomes `text` (an empty span inserts it).
#[derive(Clone, Debug, PartialEq)]
pub struct Fix {
    pub span: Span,
    pub text: String,
}

impl Diag {
    pub fn error(span: Span, msg: impl Into<String>) -> Diag {
        Diag { kind: Kind::Error, msg: msg.into(), span, notes: Vec::new(), related: None, hint: None, fix: None, as_written: false }
    }

    pub fn panic(span: Span, msg: impl Into<String>) -> Diag {
        Diag { kind: Kind::Panic, msg: msg.into(), span, notes: Vec::new(), related: None, hint: None, fix: None, as_written: false }
    }

    pub fn leak(span: Span, msg: impl Into<String>) -> Diag {
        Diag { kind: Kind::Leak, msg: msg.into(), span, notes: Vec::new(), related: None, hint: None, fix: None, as_written: false }
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

    /// The message and the notes are the program's own text, shown as written.
    pub fn as_written(mut self) -> Diag {
        self.as_written = true;
        self
    }

    /// The message as it is shown.
    pub fn shown_msg(&self) -> String {
        self.shown(&self.msg)
    }

    /// A note as it is shown.
    pub fn shown(&self, text: &str) -> String {
        if self.as_written { text.to_string() } else { worded(text) }
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
        let mut text = format!("{label}: {}\n", self.shown_msg());
        for note in &self.notes {
            text.push_str(&format!("  {}\n", self.shown(note)));
        }
        text.push_str(&sources.snippet(self.span));
        if let Some((span, note)) = self.related.as_deref() {
            text.push_str(&format!("  {}:\n", worded(note)));
            text.push_str(&sources.snippet(*span));
        }
        if let Some(hint) = &self.hint {
            text.push_str(&format!("  hint: {}\n", worded(hint)));
        }
        text
    }
}

/// A message, a note or a hint as it is shown: the types of the language's own module without
/// their prefix, and `a` or `an` before code as the code is said (choice 1401).
pub fn worded(text: &str) -> String {
    articled(&unprefixed(text))
}

/// `a` or `an` before a name in backticks, by how the name starts: `an `IoError``, `a `List<int>``,
/// `a `uint`` (said "you-int"), `an `unsafe``. A message is written with either and fixed here,
/// since the name is often only known when the message is made (`a `{ty}``). Only an article
/// outside code is looked at, and only before a name that starts with a letter.
fn articled(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    let mut in_code = false;
    let mut rest = text;
    while let Some(at) = rest.find('`') {
        let (before, after) = (&rest[..at], &rest[at + 1..]);
        if in_code {
            out.push_str(before);
        } else {
            out.push_str(&with_article(before, after));
        }
        out.push('`');
        in_code = !in_code;
        rest = after;
    }
    out.push_str(rest);
    out
}

/// `before` with its last word, if it is an article followed by one space, set for the code that
/// starts `code`.
fn with_article(before: &str, code: &str) -> String {
    let Some(head) = before.strip_suffix(' ') else { return before.to_string() };
    // The word starts after the last character that is not a letter of ASCII, which may take
    // more than one byte (`değer `x``).
    let word_start = head.char_indices().rev().find(|(_, c)| !c.is_ascii_alphabetic()).map_or(0, |(at, c)| at + c.len_utf8());
    let word = &head[word_start..];
    if !matches!(word, "a" | "an" | "A" | "An") {
        return before.to_string();
    }
    let mut letters = code.chars();
    let Some(first) = letters.next().filter(|c| c.is_ascii_alphabetic()) else { return before.to_string() };
    let second = letters.next().unwrap_or(' ').to_ascii_lowercase();
    let third = letters.next().unwrap_or(' ').to_ascii_lowercase();
    let vowel = match first.to_ascii_lowercase() {
        'a' | 'e' | 'i' | 'o' => true,
        // `unsafe`, `unchecked`, but `union`, `uint`, `ulong`, `Utf8`, `UdpSocket`.
        'u' => second == 'n' && third != 'i',
        // `sbyte`, said "ess-byte".
        's' => second == 'b',
        _ => false,
    };
    let capital = word.starts_with('A');
    let article = match (vowel, capital) {
        (true, true) => "An",
        (true, false) => "an",
        (false, true) => "A",
        (false, false) => "a",
    };
    format!("{}{article} ", &head[..word_start])
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

/// The one candidate that starts with the name, for a name cut short, else `nearest`:
/// `wrappingMul` is `wrappingMultiply`, which `nearest` finds too far, and `wrappingAbs` near
/// (choice 2239). Three letters at least, and only when one candidate starts so.
pub fn nearest_or_start<'a>(name: &str, candidates: impl Iterator<Item = &'a str> + Clone) -> Option<&'a str> {
    let mut starting = candidates.clone().filter(|c| name.chars().count() >= 3 && c.len() > name.len() && c.starts_with(name));
    let started = starting.next().filter(|first| starting.all(|other| other == *first));
    started.or_else(|| nearest(name, candidates))
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

#[cfg(test)]
mod tests {
    use super::worded;

    #[test]
    fn an_article_follows_the_code() {
        assert_eq!(worded("a `Address?` field"), "an `Address?` field");
        assert_eq!(worded("this is an `long` already"), "this is a `long` already");
        assert_eq!(worded("An `List<int>` is"), "A `List<int>` is");
        assert_eq!(worded("a `int`, a `uint`, a `unsafe` block, a `union`, a `UdpSocket`"), "an `int`, a `uint`, an `unsafe` block, a `union`, a `UdpSocket`");
        assert_eq!(worded("a `std.IoError`"), "an `IoError`");
        // Inside code, and before what does not start with a letter, nothing changes.
        assert_eq!(worded("`var a `x`` and a `[` and a `8`"), "`var a `x`` and a `[` and a `8`");
        assert_eq!(worded("a `Inner`; data `a `"), "an `Inner`; data `a `");
        assert_eq!(worded("(a `Inner`)"), "(an `Inner`)");
    }
}
