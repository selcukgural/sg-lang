//! Writing values as jq 1.8.2 writes them, into a block of 64 KB of the program's own that is given
//! to the system when full and at the end. A closed pipe ends the program at once.

use crate::number::write_double;
use crate::value::{MAX_DEPTH, Map, Value, sorted_keys};
use std::io::{self, IsTerminal, Write};

/// How values are laid out.
#[derive(Clone, Copy)]
pub enum Style {
    /// One line, no spaces.
    Compact,
    /// A line for each member, indented by this many spaces a level (0 to 7).
    Spaces(usize),
    /// A line for each member, indented by a tab a level.
    Tab,
}

#[derive(Clone, Copy)]
pub struct Format {
    pub style: Style,
    pub sort_keys: bool,
}

/// Where bytes are written: a `Vec` (for `tojson`) or the output.
pub trait Sink {
    fn bytes(&mut self, bytes: &[u8]);
}

impl Sink for Vec<u8> {
    fn bytes(&mut self, bytes: &[u8]) {
        self.extend_from_slice(bytes);
    }
}

const BLOCK: usize = 64 * 1024;

/// The standard output, through a block of 64 KB.
pub struct Output {
    block: Vec<u8>,
    stdout: io::Stdout,
    /// On a terminal each value is given at once, as jq's line-buffered output gives it.
    terminal: bool,
}

impl Sink for Output {
    fn bytes(&mut self, bytes: &[u8]) {
        if self.block.len() + bytes.len() > BLOCK {
            self.flush();
            if bytes.len() >= BLOCK {
                self.give(bytes);
                return;
            }
        }
        self.block.extend_from_slice(bytes);
    }
}

impl Output {
    pub fn new() -> Output {
        let stdout = io::stdout();
        Output { block: Vec::with_capacity(BLOCK), terminal: stdout.is_terminal(), stdout }
    }

    /// Gives the block to the system.
    pub fn flush(&mut self) {
        let block = std::mem::take(&mut self.block);
        self.give(&block);
        self.block = block;
        self.block.clear();
    }

    fn give(&mut self, bytes: &[u8]) {
        let mut lock = self.stdout.lock();
        if let Err(e) = lock.write_all(bytes).and_then(|_| lock.flush()) {
            if e.kind() == io::ErrorKind::BrokenPipe {
                // The reader has gone: jq ends here too (by SIGPIPE).
                std::process::exit(0);
            }
            eprintln!("jq: error: could not write: {e}");
            std::process::exit(2);
        }
    }

    /// One value a line; with `raw`, a text without its quotes and escapes.
    pub fn value(&mut self, value: &Value, format: Format, raw: bool) {
        match value {
            Value::String(s) if raw => self.bytes(s.as_bytes()),
            _ => dump(self, value, format, 0),
        }
        self.bytes(b"\n");
        if self.terminal {
            self.flush();
        }
    }
}

/// Writes a value as JSON.
pub fn dump<S: Sink>(out: &mut S, value: &Value, format: Format, level: usize) {
    if level > MAX_DEPTH {
        out.bytes(b"<skipped: too deep>");
        return;
    }
    match value {
        Value::Null => out.bytes(b"null"),
        Value::False => out.bytes(b"false"),
        Value::True => out.bytes(b"true"),
        Value::Number(d) | Value::Exact(d) => {
            let mut text = Vec::with_capacity(24);
            write_double(&mut text, *d);
            out.bytes(&text);
        }
        Value::Literal(l) => out.bytes(l.text.as_bytes()),
        Value::String(s) => write_string(out, s),
        Value::Array(items) => {
            if items.is_empty() {
                out.bytes(b"[]");
                return;
            }
            out.bytes(b"[");
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.bytes(b",");
                }
                indent(out, format, level + 1);
                dump(out, item, format, level + 1);
            }
            indent(out, format, level);
            out.bytes(b"]");
        }
        Value::Object(map) => dump_object(out, map, format, level),
    }
}

fn dump_object<S: Sink>(out: &mut S, map: &Map, format: Format, level: usize) {
    if map.is_empty() {
        out.bytes(b"{}");
        return;
    }
    out.bytes(b"{");
    let member = |out: &mut S, i: usize, key: &str, value: &Value| {
        if i > 0 {
            out.bytes(b",");
        }
        indent(out, format, level + 1);
        write_string(out, key);
        out.bytes(if matches!(format.style, Style::Compact) { b":" } else { b": " });
        dump(out, value, format, level + 1);
    };
    if format.sort_keys {
        for (i, key) in sorted_keys(map).into_iter().enumerate() {
            member(out, i, key, &map[key.as_str()]);
        }
    } else {
        for (i, (key, value)) in map.iter().enumerate() {
            member(out, i, &key.0, value);
        }
    }
    indent(out, format, level);
    out.bytes(b"}");
}

fn indent<S: Sink>(out: &mut S, format: Format, level: usize) {
    const SPACES: &[u8] = &[b' '; 64];
    match format.style {
        Style::Compact => {}
        Style::Spaces(n) => {
            out.bytes(b"\n");
            let mut left = n * level;
            while left > 0 {
                let piece = left.min(SPACES.len());
                out.bytes(&SPACES[..piece]);
                left -= piece;
            }
        }
        Style::Tab => {
            out.bytes(b"\n");
            for _ in 0..level {
                out.bytes(b"\t");
            }
        }
    }
}

/// A text in quotes: `"`, `\`, the bytes below 0x20 and 0x7F escaped, as jq escapes them.
pub fn write_string<S: Sink>(out: &mut S, s: &str) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    out.bytes(b"\"");
    let bytes = s.as_bytes();
    let mut start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        let mut code = [b'\\', b'u', b'0', b'0', HEX[(b >> 4) as usize], HEX[(b & 15) as usize]];
        let escape: &[u8] = match b {
            b'"' => b"\\\"",
            b'\\' => b"\\\\",
            b'\n' => b"\\n",
            b'\t' => b"\\t",
            b'\r' => b"\\r",
            0x08 => b"\\b",
            0x0c => b"\\f",
            0..0x20 | 0x7f => &mut code,
            _ => continue,
        };
        if start < i {
            out.bytes(&bytes[start..i]);
        }
        out.bytes(escape);
        start = i + 1;
    }
    if start < bytes.len() {
        out.bytes(&bytes[start..]);
    }
    out.bytes(b"\"");
}

/// A value as compact JSON in a `String` (`tojson`, `tostring`).
pub fn to_json(value: &Value) -> String {
    let mut out = Vec::new();
    dump(&mut out, value, Format { style: Style::Compact, sort_keys: false }, 0);
    String::from_utf8(out).unwrap()
}
