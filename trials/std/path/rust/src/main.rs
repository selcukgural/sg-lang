//! `path`: `std`'s paths measured against `std::path` (choices 1553, 1637, 1795, 1800-1802). Two
//! ways of using them:
//!
//!     path show PATH...    for each path its name, its extension, its folder and itself joined with
//!                          `more.txt`: `logs/report.txt: report.txt .txt logs logs/report.txt/more.txt`
//!     path many COUNT [WAY]
//!                          COUNT paths made from the seed 1, each joined from one to four folders and
//!                          a name, then taken apart into its name, its extension and its folder;
//!                          prints how many paths and the bytes of each kind of part. WAY (`push`
//!                          or `join`) picks one of SG's two ways; Rust has one, and takes the word
//!                          only so that every program runs the same case
//!
//! The paths are made by MINSTD (`x = x * 48271 % 2147483647`), the same in each language: a fifth
//! start at the root, the names are ASCII and not (`café`, `日本`), and an extension is one of six
//! or none. `Path::extension` gives the extension without its `.`, which is counted as SG and Zig
//! count it. A path is built as Rust builds one, a `PathBuf` pushed onto. A wrong use is told on the
//! standard error, with exit code 2.

use std::path::{Path, PathBuf};
use std::process::exit;

const FOLDERS: [&str; 10] = ["src", "lib", "docs", "tests", "assets", "build", "images", "2026", "café", "日本"];
const NAMES: [&str; 8] = ["report", "main", "index", "photo", "README", "data", "archive", "notes"];
const ENDINGS: [&str; 7] = ["", ".txt", ".tar.gz", ".rs", ".sg", ".json", ".jpeg"];

fn text(part: Option<&std::ffi::OsStr>) -> &str {
    part.map_or("", |p| p.to_str().unwrap())
}

fn show(paths: &[String]) {
    for path in paths {
        let p = Path::new(path);
        let extension = p.extension().map_or(String::new(), |e| format!(".{}", e.to_str().unwrap()));
        let parent = p.parent().map_or("", |d| d.to_str().unwrap());
        let joined = p.join("more.txt");
        println!("{path}: {} {extension} {parent} {}", text(p.file_name()), joined.display());
    }
}

fn many(count: i64) {
    let mut x: i64 = 1;
    let (mut bytes, mut names, mut extensions, mut parents, mut with_extension) = (0, 0, 0, 0, 0);
    for _ in 0..count {
        x = x * 48271 % 2147483647;
        let depth = 1 + x % 4;
        x = x * 48271 % 2147483647;
        let mut path = PathBuf::from(if x % 5 == 0 { "/" } else { "" });
        for _ in 0..depth {
            x = x * 48271 % 2147483647;
            path.push(FOLDERS[(x % 10) as usize]);
        }
        x = x * 48271 % 2147483647;
        let name = format!("{}{}", NAMES[(x % 8) as usize], ENDINGS[(x / 8 % 7) as usize]);
        path.push(&name);
        bytes += path.as_os_str().len();
        names += path.file_name().map_or(0, |n| n.len());
        if let Some(extension) = path.extension() {
            extensions += extension.len() + 1;
            with_extension += 1;
        }
        parents += path.parent().map_or(0, |d| d.as_os_str().len());
    }
    println!("paths {count}\nbytes {bytes}\nnames {names}\nextensions {extensions} in {with_extension}\nfolders {parents}");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() >= 2 && args[0] == "show" {
        show(&args[1..]);
        return;
    }
    let known = args.len() == 2 || args.len() == 3 && (args[2] == "push" || args[2] == "join");
    let count = if known && args[0] == "many" { args[1].parse::<i64>().unwrap_or(-1) } else { -1 };
    if count < 0 {
        eprintln!("usage: path show PATH... | many COUNT [push | join]");
        exit(2);
    }
    many(count);
}
