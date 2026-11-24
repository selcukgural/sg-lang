//! Reading the input: the files named (or the standard input) one after another as one stream, in
//! pieces of 64 KB. The bytes are checked to be UTF-8 and only where a bad sequence is found is it
//! replaced, as jq's decoder (`jvp_utf8_next`) replaces it in each text. Each value is read by
//! `serde_json`'s `StreamDeserializer` from the bytes kept, the program's own value built straight
//! from its tokens (`json.rs`), the literal of each number kept. A value longer than what has been read waits for more: what is
//! kept grows by doubling, and a value is tried again once the bytes kept have doubled, so a value of
//! 100 MB is read in linear time.

use crate::json::Read;
use crate::value::Value;
use std::fs::File;
use std::io;

const PIECE: usize = 64 * 1024;
/// Below this, a value is tried again as soon as a piece shorter than asked comes (a pipe or a
/// terminal giving what it has), so that `tail -f log | jq -c .` writes each value when it has come.
const SMALL: usize = 1024 * 1024;
const REPLACEMENT: &[u8] = "\u{FFFD}".as_bytes();

/// What the stream gives next.
pub enum Next {
    Value(Value),
    End,
    /// A value that does not read; nothing more is read after it.
    Error(String),
}

pub struct Reader {
    files: Vec<String>,
    next_file: usize,
    source: Option<Box<dyn io::Read>>,
    used_stdin: bool,
    /// The bytes kept: those from `start` are not read yet.
    buf: Vec<u8>,
    start: usize,
    /// The bytes before this are UTF-8 (checked, or replaced where bad).
    checked: usize,
    /// When a value ended with the bytes kept: try it again once this many are kept from `start`.
    retry_at: usize,
    /// No more bytes will come.
    eof: bool,
    last_short: bool,
    bom_looked_for: bool,
    failed: bool,
    /// Where a text begins or ends is known up to here: (position, inside a text, after a `\`).
    scan: (usize, bool, bool),
    /// A file could not be opened or read.
    pub file_error: bool,
    /// A file has just ended: what is kept is tried before the next file is opened, as jq hands on
    /// a value the moment its last byte is read.
    file_ended: bool,
}

impl Reader {
    pub fn new(files: Vec<String>) -> Reader {
        Reader {
            files,
            next_file: 0,
            source: None,
            used_stdin: false,
            buf: Vec::with_capacity(PIECE),
            start: 0,
            checked: 0,
            retry_at: 0,
            eof: false,
            last_short: false,
            bom_looked_for: false,
            failed: false,
            scan: (0, false, false),
            file_error: false,
            file_ended: false,
        }
    }

    /// Reads one more piece, from the next file when one has ended.
    fn read_more(&mut self) {
        if self.start > 0 {
            self.buf.drain(..self.start);
            self.checked -= self.start;
            self.scan.0 = self.scan.0.saturating_sub(self.start);
            if self.scan.0 == 0 {
                self.scan = (0, false, false);
            }
            self.start = 0;
        }
        loop {
            if self.source.is_none() {
                if self.next_file < self.files.len() {
                    let name = &self.files[self.next_file];
                    self.next_file += 1;
                    if name == "-" {
                        self.source = Some(Box::new(io::stdin()));
                        continue;
                    }
                    match File::open(name) {
                        Ok(file) => self.source = Some(Box::new(file)),
                        Err(e) => {
                            eprintln!("jq: error: Could not open {name}: {}", describe(&e));
                            self.file_error = true;
                        }
                    }
                    continue;
                }
                if self.files.is_empty() && !self.used_stdin {
                    self.used_stdin = true;
                    self.source = Some(Box::new(io::stdin()));
                    continue;
                }
                self.eof = true;
                return;
            }
            let length = self.buf.len();
            self.buf.resize(length + PIECE, 0);
            let read = self.source.as_mut().unwrap().read(&mut self.buf[length..]);
            match read {
                Ok(0) => {
                    self.buf.truncate(length);
                    self.source = None;
                    if self.next_file < self.files.len() {
                        self.file_ended = true;
                        return;
                    }
                }
                Ok(n) => {
                    self.buf.truncate(length + n);
                    self.last_short = n < PIECE;
                    return;
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => self.buf.truncate(length),
                Err(e) => {
                    self.buf.truncate(length);
                    eprintln!("jq: error: Could not read: {}", describe(&e));
                    self.file_error = true;
                    self.source = None;
                }
            }
        }
    }

    /// All the bytes there will be are kept.
    fn complete(&self) -> bool {
        self.eof && self.checked == self.buf.len()
    }

    /// The next value of the stream.
    pub fn next(&mut self) -> Next {
        if self.failed {
            return Next::End;
        }
        loop {
            if !self.bom_looked_for && (self.buf.len() >= 3 || self.eof) {
                // A byte order mark is skipped; as jq, a part of one is too when nothing follows,
                // and is an error when something does.
                self.bom_looked_for = true;
                let matched = self.buf.iter().zip(b"\xEF\xBB\xBF").take_while(|(a, b)| a == b).count();
                if matched == 3 || (matched > 0 && matched == self.buf.len()) {
                    self.start = matched;
                    self.checked = self.checked.max(matched);
                } else if matched > 0 {
                    self.failed = true;
                    return Next::Error("Malformed BOM".to_string());
                }
            }
            self.check_utf8();
            // Blanks are passed over only once the mark is looked for: a byte after them is not at
            // the start of the input (` \xEF` is an error, `\xEF` alone is nothing).
            if self.bom_looked_for {
                let blanks = self.buf[self.start..self.checked]
                    .iter()
                    .take_while(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
                    .count();
                self.start += blanks;
            }
            let kept = self.checked - self.start;
            if kept == 0 || !self.bom_looked_for {
                if self.complete() {
                    return Next::End;
                }
                self.read_more();
                continue;
            }
            if !self.complete() && kept < self.retry_at && !(self.last_short && kept < SMALL) && !self.file_ended {
                self.read_more();
                continue;
            }
            self.file_ended = false;
            let slice = &self.buf[self.start..self.checked];
            let mut deserializer = serde_json::Deserializer::from_slice(slice);
            deserializer.disable_recursion_limit();
            let mut stream = deserializer.into_iter::<Read>();
            match stream.next() {
                None => {
                    self.start = self.checked;
                }
                Some(Ok(Read(value))) => {
                    let end = stream.byte_offset();
                    let delimited = matches!(value, Value::String(_) | Value::Array(_) | Value::Object(_));
                    if !delimited {
                        // A number at the end of what is kept may go on in the next piece.
                        if end == slice.len() && !self.complete() {
                            self.retry_at = kept + 1;
                            self.read_more();
                            continue;
                        }
                        // jq takes a number, `true`, `false` or `null` written straight before
                        // one of these as an error, and loses the value.
                        if let Some(&c) = slice.get(end)
                            && b"]},:".contains(&c) {
                                self.failed = true;
                                return Next::Error(format!("Unexpected '{}' after a value", c as char));
                            }
                    }
                    self.start += end;
                    self.retry_at = 0;
                    return Next::Value(value);
                }
                Some(Err(e)) => {
                    if e.is_eof() && !self.complete() {
                        self.retry_at = kept * 2;
                        self.read_more();
                        continue;
                    }
                    self.failed = true;
                    return Next::Error(e.to_string());
                }
            }
        }
    }

    /// Checks the bytes kept to be UTF-8 from `checked` on, replacing what is bad, as far as can be
    /// told: an unfinished sequence at the end, or a bad one in a text whose end has not come, waits.
    fn check_utf8(&mut self) {
        while self.checked < self.buf.len() {
            let error = match std::str::from_utf8(&self.buf[self.checked..]) {
                Ok(_) => {
                    self.checked = self.buf.len();
                    return;
                }
                Err(error) => error,
            };
            self.checked += error.valid_up_to();
            if error.error_len().is_none() && !self.eof {
                return;
            }
            if !self.repair() {
                return;
            }
        }
    }

    /// Replaces the bad sequence at `checked`, and what jq's decoder takes with it. False when the
    /// end of its text has not come yet.
    fn repair(&mut self) -> bool {
        let bad = self.checked;
        let (mut at, mut in_text, mut escaped) = self.scan;
        if at < self.start || at > bad {
            (at, in_text, escaped) = (self.start, false, false);
        }
        while at < bad {
            let b = self.buf[at];
            if in_text {
                if escaped {
                    escaped = false;
                } else if b == b'\\' {
                    escaped = true;
                } else if b == b'"' {
                    in_text = false;
                }
            } else if b == b'"' {
                in_text = true;
            }
            at += 1;
        }
        self.scan = (bad, in_text, escaped);
        if !in_text || escaped {
            // Outside a text (or straight after a `\`) a bad byte is an error wherever it is.
            self.buf.splice(bad..bad + 1, REPLACEMENT.iter().copied());
            self.checked = bad + REPLACEMENT.len();
            self.scan = (self.checked, in_text, false);
            return true;
        }
        // The end of the text: the next `"` that is not escaped.
        let mut end = bad;
        let mut escape = false;
        while end < self.buf.len() {
            let b = self.buf[end];
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                break;
            }
            end += 1;
        }
        if end == self.buf.len() && !self.eof {
            return false;
        }
        let repaired = repair_text(&self.buf[bad..end], true);
        let length = repaired.len();
        self.buf.splice(bad..end, repaired);
        self.checked = bad + length;
        self.scan = (self.checked, true, false);
        true
    }
}

fn describe(e: &io::Error) -> String {
    match e.kind() {
        io::ErrorKind::NotFound => "No such file or directory".to_string(),
        io::ErrorKind::PermissionDenied => "Permission denied".to_string(),
        _ => e.to_string(),
    }
}

/// The length of the UTF-8 sequence a byte begins, as jq's decoder takes it; 0 when it is bad alone.
fn sequence_length(b: u8) -> usize {
    match b {
        0x00..=0x7F => 1,
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        _ => 0,
    }
}

fn is_continuation(b: u8) -> bool {
    (0x80..=0xBF).contains(&b)
}

/// Bytes made into a text, what is not UTF-8 replaced by U+FFFD as jq replaces it (`@base64d`).
pub fn replace_bad_utf8(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => String::from_utf8(repair_text(bytes, false)).unwrap(),
    }
}

/// The bytes of a text from a bad sequence to the text's end, with what is bad replaced by U+FFFD as
/// jq replaces it. In JSON (`escapes`) jq reads the escapes first and then decodes, so a sequence the
/// text ends inside is one U+FFFD up to the end, escapes and all.
fn repair_text(raw: &[u8], escapes: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len() + 8);
    let mut i = 0;
    while i < raw.len() {
        let b = raw[i];
        if b == b'\\' && escapes {
            out.extend_from_slice(&raw[i..(i + 2).min(raw.len())]);
            i += 2;
            continue;
        }
        let length = sequence_length(b);
        if length == 1 {
            out.push(b);
            i += 1;
            continue;
        }
        if length == 0 {
            out.extend_from_slice(REPLACEMENT);
            i += 1;
            continue;
        }
        let after = if escapes { unescaped_after(raw, i + 1, length - 1) } else { raw.len() - i - 1 };
        if after < length - 1 {
            out.extend_from_slice(REPLACEMENT);
            break;
        }
        let mut k = 1;
        while k < length && is_continuation(raw[i + k]) {
            k += 1;
        }
        if k < length {
            out.extend_from_slice(REPLACEMENT);
            i += k;
            continue;
        }
        let mut code = u32::from(b) & (0x7F >> length);
        for &c in &raw[i + 1..i + length] {
            code = (code << 6) | u32::from(c & 0x3F);
        }
        let first = [0, 0, 0x80, 0x800, 0x10000][length];
        if code < first || (0xD800..=0xDFFF).contains(&code) || code > 0x10FFFF {
            out.extend_from_slice(REPLACEMENT);
        } else {
            out.extend_from_slice(&raw[i..i + length]);
        }
        i += length;
    }
    out
}

/// How many bytes the text has from `from` once its escapes are read, counting up to `need`.
fn unescaped_after(raw: &[u8], from: usize, need: usize) -> usize {
    let hex = |at: usize| -> Option<u32> {
        let digits = raw.get(at..at + 4)?;
        u32::from_str_radix(std::str::from_utf8(digits).ok()?, 16).ok()
    };
    let mut count = 0;
    let mut j = from;
    while count < need && j < raw.len() {
        if raw[j] != b'\\' {
            count += 1;
            j += 1;
            continue;
        }
        if raw.get(j + 1) != Some(&b'u') {
            count += 1;
            j += 2;
            continue;
        }
        let code = hex(j + 2).unwrap_or(0);
        if (0xD800..0xDC00).contains(&code)
            && raw.get(j + 6) == Some(&b'\\')
            && raw.get(j + 7) == Some(&b'u')
            && hex(j + 8).is_some_and(|low| (0xDC00..0xE000).contains(&low))
        {
            count += 4;
            j += 12;
        } else {
            count += match code {
                0..0x80 => 1,
                0x80..0x800 => 2,
                _ => 3,
            };
            j += 6;
        }
    }
    count
}

/// A text read as one JSON value (`fromjson`).
pub fn parse_text(text: &str) -> Result<Value, String> {
    let mut deserializer = serde_json::Deserializer::from_str(text);
    deserializer.disable_recursion_limit();
    let mut stream = deserializer.into_iter::<Read>();
    let value = match stream.next() {
        Some(Ok(Read(value))) => value,
        Some(Err(e)) => return Err(format!("{e} (while parsing '{text}')")),
        None => return Err(format!("Expected JSON value (while parsing '{text}')")),
    };
    if !text[stream.byte_offset()..].trim_matches([' ', '\t', '\n', '\r']).is_empty() {
        return Err(format!("Unexpected extra JSON values (while parsing '{text}')"));
    }
    Ok(value)
}
