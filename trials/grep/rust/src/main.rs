//! `grep`: the lines of each file named, or of the input, that a regular expression matches, with
//! the syntax of the `regex` crate. `-i` ignores case, `-v` gives the lines that do not match, `-n`
//! writes each line's number in front of it, `-c` gives only how many lines there were. With more
//! than one file, each line starts with its file's name. A line is read as bytes without its
//! `\n` or `\r\n` and searched with `regex::bytes`, where a byte that is not UTF-8 matches nothing
//! in the pattern, and it is written as it is (choice 1840). The exit code is 0 when a line was given, 1 when none was, 2 when a file could
//! not be read or the pattern is not one (as GNU grep's).

use regex::bytes::Regex;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::process::ExitCode;

#[derive(Clone, Copy, Default)]
struct Options {
    count: bool,
    invert: bool,
    numbers: bool,
    names: bool,
}

/// Writes the lines of `reader` that are wanted; how many there were.
fn search(
    reader: &mut impl BufRead,
    regex: &Regex,
    options: Options,
    name: &str,
    out: &mut impl Write,
) -> io::Result<u64> {
    let mut line = Vec::new();
    let mut number = 0u64;
    let mut found = 0u64;
    loop {
        line.clear();
        if reader.read_until(b'\n', &mut line)? == 0 {
            break;
        }
        if line.ends_with(b"\n") {
            line.pop();
            if line.ends_with(b"\r") {
                line.pop();
            }
        }
        number += 1;
        if regex.is_match(&line) == options.invert {
            continue;
        }
        found += 1;
        if options.count {
            continue;
        }
        if options.names {
            write!(out, "{name}:")?;
        }
        if options.numbers {
            write!(out, "{number}:")?;
        }
        out.write_all(&line)?;
        out.write_all(b"\n")?;
    }
    if options.count {
        if options.names {
            writeln!(out, "{name}:{found}")?;
        } else {
            writeln!(out, "{found}")?;
        }
    }
    Ok(found)
}

fn usage(message: &str) -> ExitCode {
    eprintln!("grep: {message}");
    eprintln!("usage: grep [-c] [-i] [-n] [-v] PATTERN [FILE...]");
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut options = Options::default();
    let mut ignore_case = false;
    let mut at = 0;
    while at < args.len() {
        let arg = &args[at];
        match arg.as_str() {
            "--" => {
                at += 1;
                break;
            }
            "--count" => options.count = true,
            "--ignore-case" => ignore_case = true,
            "--line-number" => options.numbers = true,
            "--invert-match" => options.invert = true,
            _ if arg.starts_with('-') && arg.len() > 1 => {
                for letter in arg[1..].chars() {
                    match letter {
                        'c' => options.count = true,
                        'i' => ignore_case = true,
                        'n' => options.numbers = true,
                        'v' => options.invert = true,
                        _ => return usage(&format!("unknown option -{letter}")),
                    }
                }
            }
            _ => break,
        }
        at += 1;
    }
    let Some(pattern) = args.get(at) else {
        return usage("no pattern");
    };
    let files = &args[at + 1..];
    let pattern = if ignore_case { format!("(?i){pattern}") } else { pattern.clone() };
    let regex = match Regex::new(&pattern) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("grep: {e}");
            return ExitCode::from(2);
        }
    };
    options.names = files.len() > 1;

    let stdout = io::stdout();
    let mut out = io::BufWriter::with_capacity(65536, stdout.lock());
    let mut found = 0;
    let mut failed = false;
    if files.is_empty() {
        let stdin = io::stdin();
        match search(&mut stdin.lock(), &regex, options, "", &mut out) {
            Ok(n) => found += n,
            Err(e) => {
                eprintln!("grep: {e}");
                failed = true;
            }
        }
    }
    for name in files {
        let result = File::open(name).and_then(|f| {
            search(&mut BufReader::with_capacity(65536, f), &regex, options, name, &mut out)
        });
        match result {
            Ok(n) => found += n,
            Err(e) => {
                let _ = out.flush();
                eprintln!("grep: {name}: {e}");
                failed = true;
            }
        }
    }
    if out.flush().is_err() {
        return ExitCode::from(2);
    }
    if failed {
        ExitCode::from(2)
    } else if found == 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
