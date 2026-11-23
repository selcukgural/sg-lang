//! The command line read by `lexopt`, built with `--no-default-features --features lexopt`
//! (choice 1796).

use std::process::exit;

const HELP: &str = "\
Packs files into an archive.

Usage: pack [OPTIONS] <FORMAT> <FOLDER> [FILES]...

Arguments:
  <FORMAT>    The kind of archive: zip or tar
  <FOLDER>    The folder to pack
  [FILES]...  Files to pack besides the folder

Options:
  -v, --verbose            Print each file as it is packed
  -q, --quiet              Print nothing but errors
  -f, --force              Write over an archive that exists
  -n, --dry-run            Show what would be packed, and pack nothing
  -o, --output <FILE>      Write the archive to FILE
  -l, --level <N>          Compress at level N
  -x, --exclude <PATTERN>  Leave out the files that match PATTERN; may be given again
  -h, --help               Print help";

fn parse() -> Result<super::Read, lexopt::Error> {
    use lexopt::prelude::*;
    let mut r = super::Read {
        verbose: false,
        quiet: false,
        force: false,
        dry_run: false,
        output: None,
        level: None,
        exclude: Vec::new(),
        format: String::new(),
        folder: String::new(),
        files: Vec::new(),
    };
    let mut words = Vec::new();
    let mut parser = lexopt::Parser::from_env();
    while let Some(arg) = parser.next()? {
        match arg {
            Short('v') | Long("verbose") => r.verbose = true,
            Short('q') | Long("quiet") => r.quiet = true,
            Short('f') | Long("force") => r.force = true,
            Short('n') | Long("dry-run") => r.dry_run = true,
            Short('o') | Long("output") => r.output = Some(parser.value()?.string()?),
            Short('l') | Long("level") => r.level = Some(parser.value()?.string()?),
            Short('x') | Long("exclude") => r.exclude.push(parser.value()?.string()?),
            Short('h') | Long("help") => {
                println!("{HELP}");
                exit(0);
            }
            Value(word) => words.push(word.string()?),
            _ => return Err(arg.unexpected()),
        }
    }
    let mut words = words.into_iter();
    r.format = words.next().ok_or("missing <FORMAT>")?;
    r.folder = words.next().ok_or("missing <FOLDER>")?;
    r.files = words.collect();
    Ok(r)
}

pub fn read() -> super::Read {
    parse().unwrap_or_else(|e| {
        eprintln!("pack: {e}\nTry 'pack --help'.");
        exit(2)
    })
}
