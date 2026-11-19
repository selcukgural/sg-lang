//! JSON, read and written, for the language server (`sg lsp`, choice 1370): enough of RFC 8259
//! for the messages of the Language Server Protocol, so that `sg` keeps no dependencies.

use std::fmt::Write;

#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    /// The members in the order they were written.
    Obj(Vec<(String, Json)>),
}

/// How deep arrays and objects may nest in a message: past that it is refused, so that a
/// message cannot run the reader out of stack.
const MAX_DEPTH: usize = 128;

impl Json {
    pub fn obj(members: Vec<(&str, Json)>) -> Json {
        Json::Obj(members.into_iter().map(|(key, value)| (key.to_string(), value)).collect())
    }

    pub fn str(text: impl Into<String>) -> Json {
        Json::Str(text.into())
    }

    pub fn num(value: impl Into<f64>) -> Json {
        Json::Num(value.into())
    }

    /// The member `key` of an object, `Null` when there is none.
    pub fn get(&self, key: &str) -> &Json {
        match self {
            Json::Obj(members) => members.iter().find(|(name, _)| name == key).map_or(&Json::Null, |(_, value)| value),
            _ => &Json::Null,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Json::Num(value) if *value >= 0.0 && value.fract() == 0.0 && *value <= u32::MAX as f64 * u32::MAX as f64 => Some(*value as u64),
            _ => None,
        }
    }

    pub fn as_array(&self) -> &[Json] {
        match self {
            Json::Arr(items) => items,
            _ => &[],
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Json::Null)
    }

    pub fn write(&self, out: &mut String) {
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
            Json::Num(value) if value.is_finite() && value.fract() == 0.0 && value.abs() < 1e15 => {
                let _ = write!(out, "{}", *value as i64);
            }
            Json::Num(value) if value.is_finite() => {
                let _ = write!(out, "{value}");
            }
            Json::Num(_) => out.push_str("null"),
            Json::Str(text) => write_str(text, out),
            Json::Arr(items) => {
                out.push('[');
                for (at, item) in items.iter().enumerate() {
                    if at > 0 {
                        out.push(',');
                    }
                    item.write(out);
                }
                out.push(']');
            }
            Json::Obj(members) => {
                out.push('{');
                for (at, (key, value)) in members.iter().enumerate() {
                    if at > 0 {
                        out.push(',');
                    }
                    write_str(key, out);
                    out.push(':');
                    value.write(out);
                }
                out.push('}');
            }
        }
    }
}

impl std::fmt::Display for Json {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut out = String::new();
        self.write(&mut out);
        f.write_str(&out)
    }
}

fn write_str(text: &str, out: &mut String) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Reads one JSON value that is the whole of `text`.
pub fn parse(text: &str) -> Result<Json, String> {
    let mut reader = Reader { bytes: text.as_bytes(), pos: 0 };
    let value = reader.value(0)?;
    reader.space();
    match reader.pos == reader.bytes.len() {
        true => Ok(value),
        false => Err(format!("more after the value at byte {}", reader.pos)),
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn space(&mut self) {
        while self.bytes.get(self.pos).is_some_and(|byte| matches!(byte, b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn fail<T>(&self, what: &str) -> Result<T, String> {
        Err(format!("{what} at byte {}", self.pos))
    }

    fn word(&mut self, word: &str, value: Json) -> Result<Json, String> {
        match self.bytes[self.pos..].starts_with(word.as_bytes()) {
            true => {
                self.pos += word.len();
                Ok(value)
            }
            false => self.fail("not a value"),
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json, String> {
        if depth > MAX_DEPTH {
            return self.fail("nested too deep");
        }
        self.space();
        match self.bytes.get(self.pos) {
            None => self.fail("the end, where a value was expected"),
            Some(b'n') => self.word("null", Json::Null),
            Some(b't') => self.word("true", Json::Bool(true)),
            Some(b'f') => self.word("false", Json::Bool(false)),
            Some(b'"') => self.string().map(Json::Str),
            Some(b'[') => {
                self.pos += 1;
                let mut items = Vec::new();
                self.space();
                if self.bytes.get(self.pos) == Some(&b']') {
                    self.pos += 1;
                    return Ok(Json::Arr(items));
                }
                loop {
                    items.push(self.value(depth + 1)?);
                    self.space();
                    match self.bytes.get(self.pos) {
                        Some(b',') => self.pos += 1,
                        Some(b']') => {
                            self.pos += 1;
                            return Ok(Json::Arr(items));
                        }
                        _ => return self.fail("`,` or `]` expected"),
                    }
                }
            }
            Some(b'{') => {
                self.pos += 1;
                let mut members = Vec::new();
                self.space();
                if self.bytes.get(self.pos) == Some(&b'}') {
                    self.pos += 1;
                    return Ok(Json::Obj(members));
                }
                loop {
                    self.space();
                    if self.bytes.get(self.pos) != Some(&b'"') {
                        return self.fail("a key expected");
                    }
                    let key = self.string()?;
                    self.space();
                    if self.bytes.get(self.pos) != Some(&b':') {
                        return self.fail("`:` expected");
                    }
                    self.pos += 1;
                    let value = self.value(depth + 1)?;
                    members.push((key, value));
                    self.space();
                    match self.bytes.get(self.pos) {
                        Some(b',') => self.pos += 1,
                        Some(b'}') => {
                            self.pos += 1;
                            return Ok(Json::Obj(members));
                        }
                        _ => return self.fail("`,` or `}` expected"),
                    }
                }
            }
            Some(b'-' | b'0'..=b'9') => {
                let start = self.pos;
                while self.bytes.get(self.pos).is_some_and(|byte| matches!(byte, b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')) {
                    self.pos += 1;
                }
                let text = std::str::from_utf8(&self.bytes[start..self.pos]).unwrap_or("");
                text.parse().map(Json::Num).or_else(|_| self.fail("not a number"))
            }
            Some(_) => self.fail("not a value"),
        }
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let digits = self.bytes.get(self.pos..self.pos + 4).and_then(|digits| std::str::from_utf8(digits).ok());
        let Some(value) = digits.and_then(|digits| u32::from_str_radix(digits, 16).ok()) else { return self.fail("four hex digits expected") };
        self.pos += 4;
        Ok(value)
    }

    fn string(&mut self) -> Result<String, String> {
        self.pos += 1;
        let mut out = Vec::new();
        loop {
            match self.bytes.get(self.pos) {
                None => return self.fail("a text that does not end"),
                Some(b'"') => {
                    self.pos += 1;
                    return String::from_utf8(out).or_else(|_| self.fail("a text that is not UTF-8"));
                }
                Some(b'\\') => {
                    self.pos += 1;
                    let escaped = self.bytes.get(self.pos).copied();
                    self.pos += 1;
                    let c = match escaped {
                        Some(b'"') => '"',
                        Some(b'\\') => '\\',
                        Some(b'/') => '/',
                        Some(b'b') => '\u{8}',
                        Some(b'f') => '\u{c}',
                        Some(b'n') => '\n',
                        Some(b'r') => '\r',
                        Some(b't') => '\t',
                        Some(b'u') => {
                            let high = self.hex4()?;
                            let code = match high {
                                0xD800..=0xDBFF if self.bytes[self.pos..].starts_with(b"\\u") => {
                                    self.pos += 2;
                                    let low = self.hex4()?;
                                    match low {
                                        0xDC00..=0xDFFF => 0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00),
                                        _ => 0xFFFD,
                                    }
                                }
                                code => code,
                            };
                            char::from_u32(code).unwrap_or('\u{FFFD}')
                        }
                        _ => return self.fail("an unknown escape"),
                    };
                    let mut buf = [0; 4];
                    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
                Some(&byte) => {
                    out.push(byte);
                    self.pos += 1;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_it_writes() {
        let text = r#"{"a":[1,-2.5,true,null,"x\"\n\u00e9\ud83d\ude00"],"b":{}}"#;
        let value = parse(text).unwrap();
        assert_eq!(value.get("a").as_array()[4].as_str(), Some("x\"\né😀"));
        assert_eq!(parse(&value.to_string()).unwrap(), value);
    }

    #[test]
    fn refuses_what_is_not_json() {
        for text in ["", "{", "[1,]", "{\"a\" 1}", "nul", "\"\\x\"", "1 2"] {
            assert!(parse(text).is_err(), "{text}");
        }
        assert!(parse(&"[".repeat(1000)).is_err());
    }
}
