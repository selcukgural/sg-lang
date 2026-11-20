//! `wc`: the lines, words and bytes of each file named, and their total when there are several; with
//! no file, those of the input. A word is a run of bytes that are not ASCII white space (space, tab,
//! line feed, vertical tab, form feed, carriage return), so a file need not be text. A file that
//! cannot be opened or read is told on the standard error, the others are still counted, and the
//! program ends with exit code 1.

use std::fs::File;
use std::io::{self, Read, Write};
use std::process::ExitCode;

#[derive(Clone, Copy, Default)]
struct Counts {
    lines: u64,
    words: u64,
    bytes: u64,
}

/// The counts of what `reader` gives, read in pieces of 64 KB.
fn count(reader: &mut impl Read) -> io::Result<Counts> {
    let mut buffer = [0u8; 65536];
    let mut c = Counts::default();
    let mut in_word = false;
    loop {
        let got = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        c.bytes += got as u64;
        for &b in &buffer[..got] {
            if b == b'\n' {
                c.lines += 1;
            }
            let space = b == b' ' || (b'\t'..=b'\r').contains(&b);
            if !space && !in_word {
                c.words += 1;
            }
            in_word = !space;
        }
    }
    Ok(c)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    if args.is_empty() {
        return match count(&mut io::stdin().lock()) {
            Ok(c) => {
                writeln!(out, "{} {} {}", c.lines, c.words, c.bytes).unwrap();
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("wc: {e}");
                ExitCode::from(1)
            }
        };
    }
    let mut total = Counts::default();
    let mut code = 0;
    for name in &args {
        match File::open(name).and_then(|mut f| count(&mut f)) {
            Ok(c) => {
                writeln!(out, "{} {} {} {}", c.lines, c.words, c.bytes, name).unwrap();
                total.lines += c.lines;
                total.words += c.words;
                total.bytes += c.bytes;
            }
            Err(e) => {
                out.flush().unwrap();
                eprintln!("wc: {name}: {e}");
                code = 1;
            }
        }
    }
    if args.len() > 1 {
        writeln!(out, "{} {} {} total", total.lines, total.words, total.bytes).unwrap();
    }
    out.flush().unwrap();
    ExitCode::from(code)
}
