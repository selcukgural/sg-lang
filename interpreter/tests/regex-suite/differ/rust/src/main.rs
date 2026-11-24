//! The Rust side of the differential test of SG's `Regex` (choice 1841), with the `regex` crate:
//! reads cases, a line each, the hex of a pattern, a space, the hex of a text, and writes for each
//! what `sg/differ.sg` writes. With `bytes`, the text is bytes searched with `regex::bytes`.
use std::fmt::Write as _;
use std::io::{BufRead, Write};

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

/// `y`/`n`, the places of the matches one after another, `|`, the groups of the first match.
fn written(matched: bool, all: impl Iterator<Item = (usize, usize)>, groups: Option<Vec<Option<(usize, usize)>>>) -> String {
    let mut s = String::from(if matched { "y " } else { "n " });
    for (i, (start, end)) in all.enumerate() {
        if i > 0 {
            s.push(',');
        }
        write!(s, "{start}-{end}").unwrap();
    }
    s.push_str(" |");
    for group in groups.into_iter().flatten() {
        match group {
            Some((start, end)) => write!(s, " {start}-{end}").unwrap(),
            None => s.push_str(" -"),
        }
    }
    s
}

fn main() {
    let bytes = std::env::args().nth(1).is_some_and(|mode| mode == "bytes");
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap();
        let (p, t) = line.split_once(' ').unwrap();
        let (pattern, text) = (String::from_utf8(unhex(p)).unwrap(), unhex(t));
        let line = if bytes {
            match regex::bytes::Regex::new(&pattern) {
                Ok(re) => written(
                    re.is_match(&text),
                    re.find_iter(&text).map(|m| (m.start(), m.end())),
                    re.captures(&text).map(|c| (0..c.len()).map(|g| c.get(g).map(|m| (m.start(), m.end()))).collect()),
                ),
                Err(e) => format!("E {}", e.to_string().replace('\n', " ")),
            }
        } else {
            let text = String::from_utf8(text).unwrap();
            match regex::Regex::new(&pattern) {
                Ok(re) => written(
                    re.is_match(&text),
                    re.find_iter(&text).map(|m| (m.start(), m.end())),
                    re.captures(&text).map(|c| (0..c.len()).map(|g| c.get(g).map(|m| (m.start(), m.end()))).collect()),
                ),
                Err(e) => format!("E {}", e.to_string().replace('\n', " ")),
            }
        };
        writeln!(out, "{line}").unwrap();
    }
}
