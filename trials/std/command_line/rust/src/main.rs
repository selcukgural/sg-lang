//! `command_line`: `std`'s command line measured against `clap` and `lexopt` (choices 1553, 1637,
//! 1795-1798). The program is the front of a packer of files, `pack [OPTIONS] FORMAT FOLDER
//! [FILES]...`: four flags, three options (`--exclude` may be given again), two words in their
//! places and the rest. It packs nothing; it prints what it read, one line for each name:
//!
//!     verbose true
//!     output out.zip           (`none` when it was not given)
//!     exclude 2 *.tmp *.log    (how many, then each)
//!     files 3 22 notes.txt -   (how many, their bytes, the first and the last)
//!
//! `-h` and `--help` print the help, which each library writes its own way. A wrong use is told on
//! the standard error, with exit code 2. Built with `clap` by default, and with `lexopt` in its place
//! with `--no-default-features --features lexopt`.

use std::fmt::Write;

struct Read {
    verbose: bool,
    quiet: bool,
    force: bool,
    dry_run: bool,
    output: Option<String>,
    level: Option<String>,
    exclude: Vec<String>,
    format: String,
    folder: String,
    files: Vec<String>,
}

#[cfg(feature = "clap")]
#[path = "with_clap.rs"]
mod line;

#[cfg(not(feature = "clap"))]
#[path = "with_lexopt.rs"]
mod line;

fn main() {
    let r = line::read();
    let mut out = String::with_capacity(256);
    let none = || "none".to_string();
    writeln!(out, "verbose {}\nquiet {}", r.verbose, r.quiet).unwrap();
    writeln!(out, "force {}\ndry-run {}", r.force, r.dry_run).unwrap();
    writeln!(out, "output {}\nlevel {}", r.output.unwrap_or_else(none), r.level.unwrap_or_else(none)).unwrap();
    write!(out, "exclude {}", r.exclude.len()).unwrap();
    for pattern in &r.exclude {
        write!(out, " {pattern}").unwrap();
    }
    writeln!(out, "\nformat {}\nfolder {}", r.format, r.folder).unwrap();
    let bytes: usize = r.files.iter().map(|f| f.len()).sum();
    write!(out, "files {} {bytes}", r.files.len()).unwrap();
    if let (Some(first), Some(last)) = (r.files.first(), r.files.last()) {
        write!(out, " {first} {last}").unwrap();
    }
    println!("{out}");
}
