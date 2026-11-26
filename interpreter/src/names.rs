//! Names of the system as text (choices 1950 to 1956). A `string` is always UTF-8, and a name of
//! the system is any bytes (on Linux and macOS) or any units of UTF-16 (on Windows, where Rust
//! and the C runtime carry them as WTF-8). A byte that is not part of UTF-8, and each byte of a
//! character from U+10FF80 to U+10FFFF, stands in the text as the character U+10FF00 + the byte;
//! given to the system, each such character is its byte again. So every name comes into SG, and
//! goes back to the system as it came. Only bytes from 0x80 come out of the escape, never `/`,
//! `\`, `.`, `:` or a zero byte. The C runtime does the same (`sg_name_escape`,
//! `sg_name_unescape`, `sg_name_wide` in `emit_c.rs`).

use std::borrow::Cow;
use std::ffi::{OsStr, OsString};

/// The first character of the escape: U+10FF00 + 0x80.
const FIRST: u32 = 0x10FF80;

fn escaped(byte: u8) -> char {
    char::from_u32(0x10FF00 + u32::from(byte)).expect("U+10FF80 to U+10FFFF are characters")
}

/// Bytes of a name of the system as a text.
pub fn text_of(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for chunk in bytes.utf8_chunks() {
        for c in chunk.valid().chars() {
            if u32::from(c) >= FIRST {
                let mut buffer = [0; 4];
                for byte in c.encode_utf8(&mut buffer).bytes() {
                    out.push(escaped(byte));
                }
            } else {
                out.push(c);
            }
        }
        for byte in chunk.invalid() {
            out.push(escaped(*byte));
        }
    }
    out
}

/// The bytes the system is given for a name written as a text.
pub fn bytes_of(text: &str) -> Cow<'_, [u8]> {
    if !text.chars().any(|c| u32::from(c) >= FIRST) {
        return Cow::Borrowed(text.as_bytes());
    }
    let mut out = Vec::with_capacity(text.len());
    for c in text.chars() {
        match u32::from(c) {
            code if code >= FIRST => out.push((code - 0x10FF00) as u8),
            _ => out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes()),
        }
    }
    Cow::Owned(out)
}

/// A name the system gave, as a text.
pub fn text_of_os(name: &OsStr) -> String {
    text_of(name.as_encoded_bytes())
}

/// A name written as a text, as the system takes it.
#[cfg(unix)]
pub fn os_of(text: &str) -> OsString {
    use std::os::unix::ffi::OsStringExt;
    OsString::from_vec(bytes_of(text).into_owned())
}

/// A name written as a text, as Windows takes it: the bytes it stands for read as WTF-8, so a
/// half of a pair of UTF-16 that came alone goes back alone. A byte that is no part of WTF-8,
/// which only a text not made from a name of the system can give, is U+FFFD (choice 1959).
#[cfg(windows)]
pub fn os_of(text: &str) -> OsString {
    use std::os::windows::ffi::OsStringExt;
    OsString::from_wide(&wide_of(&bytes_of(text)))
}

/// Bytes of WTF-8 as units of UTF-16.
#[cfg(any(windows, test))]
fn wide_of(bytes: &[u8]) -> Vec<u16> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        match step(bytes, at) {
            Some((c, length)) => {
                if c >= 0x10000 {
                    let c = c - 0x10000;
                    out.push(0xD800 + (c >> 10) as u16);
                    out.push(0xDC00 + (c & 0x3FF) as u16);
                } else {
                    out.push(c as u16);
                }
                at += length;
            }
            None => {
                out.push(0xFFFD);
                at += 1;
            }
        }
    }
    out
}

/// The character of WTF-8 at `at`, a half of a pair alone among them, and its length.
#[cfg(any(windows, test))]
fn step(bytes: &[u8], at: usize) -> Option<(u32, usize)> {
    let first = bytes[at];
    let (length, start, least) = match first {
        0x00..=0x7F => return Some((u32::from(first), 1)),
        0xC2..=0xDF => (2, u32::from(first & 0x1F), 0x80),
        0xE0..=0xEF => (3, u32::from(first & 0x0F), 0x800),
        0xF0..=0xF4 => (4, u32::from(first & 0x07), 0x10000),
        _ => return None,
    };
    let rest = bytes.get(at + 1..at + length)?;
    let mut c = start;
    for byte in rest {
        if byte & 0xC0 != 0x80 {
            return None;
        }
        c = (c << 6) | u32::from(byte & 0x3F);
    }
    (c >= least && c <= 0x10FFFF).then_some((c, length))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round(bytes: &[u8]) {
        let text = text_of(bytes);
        assert_eq!(&*bytes_of(&text), bytes, "{bytes:02X?} as {text:?}");
    }

    #[test]
    fn text_stays_as_it_is() {
        for text in ["", "a", "çalışma klasörü", "日本語", "😀", "\u{10FF7F}", "a/b\\c.d:e"] {
            assert_eq!(text_of(text.as_bytes()), text);
            assert!(matches!(bytes_of(text), Cow::Borrowed(_)));
        }
    }

    #[test]
    fn every_byte_that_is_not_utf8_goes_and_comes_back() {
        for byte in 0x80..=0xFF_u8 {
            let text = text_of(&[b'a', byte, b'b']);
            assert_eq!(text.chars().count(), 3);
            assert_eq!(u32::from(text.chars().nth(1).unwrap()), 0x10FF00 + u32::from(byte));
            round(&[b'a', byte, b'b']);
        }
    }

    #[test]
    fn broken_sequences() {
        // Cut short, overlong, a half of a pair, past U+10FFFF, a lone continuation.
        for bytes in [&[0xE6, 0x97][..], &[0xC0, 0x80], &[0xED, 0xA0, 0x80], &[0xF4, 0x90, 0x80, 0x80], &[0x80], &[0xF0, 0x9F, 0x41], &[0xFF, 0xFE]] {
            let text = text_of(bytes);
            assert!(text.chars().all(|c| u32::from(c) >= FIRST || c.is_ascii()), "{bytes:02X?} as {text:?}");
            round(bytes);
        }
        assert_eq!(text_of(&[0xF0, 0x9F, 0x41]).chars().last(), Some('A'));
    }

    #[test]
    fn the_escape_itself_is_escaped() {
        // A name that holds U+10FF80 as UTF-8 (F4 8F BE 80) comes as four escaped bytes, so it
        // goes back as it came and not as the one byte 0x80.
        let bytes = "x\u{10FF80}\u{10FFFF}".as_bytes();
        let text = text_of(bytes);
        assert_eq!(text.chars().count(), 9);
        round(bytes);
        // A text written with the character itself is its byte.
        assert_eq!(&*bytes_of("\u{10FF80}"), &[0x80]);
    }

    #[test]
    fn only_high_bytes_come_out_of_the_escape() {
        for code in FIRST..=0x10FFFF {
            let c = char::from_u32(code).unwrap();
            let bytes = bytes_of(&c.to_string()).into_owned();
            assert_eq!(bytes.len(), 1);
            assert!(bytes[0] >= 0x80);
        }
    }

    #[test]
    fn random_bytes_go_and_come_back() {
        // A small generator of its own, so the test needs nothing and repeats.
        let mut state = 0x2545_F491_4F6C_DD1D_u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..100_000 {
            let length = (next() % 24) as usize;
            let bytes: Vec<u8> = (0..length)
                .map(|_| match next() % 4 {
                    0 => (next() % 0x80) as u8,
                    1 => 0x80 + (next() % 0x40) as u8,
                    2 => [0xC3, 0xE6, 0xF0, 0xF4, 0xED, 0x8F, 0xBE, 0xBF][(next() % 8) as usize],
                    _ => next() as u8,
                })
                .collect();
            round(&bytes);
            let text = text_of(&bytes);
            assert_eq!(text_of(&bytes_of(&text)), text);
        }
    }

    #[test]
    fn halves_of_pairs_go_back_alone() {
        // WTF-8 of a lone high half, of a lone low half, and of a real pair.
        assert_eq!(wide_of(&[0xED, 0xA0, 0x80]), [0xD800]);
        assert_eq!(wide_of(&[0x61, 0xED, 0xBF, 0xBF, 0x62]), [0x61, 0xDFFF, 0x62]);
        assert_eq!(wide_of("😀".as_bytes()), [0xD83D, 0xDE00]);
        // A lone half comes as escaped bytes and goes back as the half.
        let text = text_of(&[0xED, 0xA0, 0x80]);
        assert_eq!(wide_of(&bytes_of(&text)), [0xD800]);
        // A byte no WTF-8 has is U+FFFD (1959).
        assert_eq!(wide_of(&[0x80]), [0xFFFD]);
    }
}
