//! `environment`: `std`'s variables of the environment measured against `std::env` (choices 1553,
//! 1637, 1795, 1799). Two ways of using them:
//!
//!     environment get NAME...          each variable as text: `NAME value`, `NAME missing`, or
//!                                      `NAME not text` when what it holds is not UTF-8
//!     environment read NAME COUNT      the variable read COUNT times, each time as a new text;
//!                                      prints how many reads and how many bytes they gave
//!
//! A wrong use is told on the standard error, with exit code 2.

use std::env::{self, VarError};
use std::process::exit;

fn get(names: &[String]) {
    for name in names {
        match env::var(name) {
            Ok(value) => println!("{name} {value}"),
            Err(VarError::NotPresent) => println!("{name} missing"),
            Err(VarError::NotUnicode(_)) => println!("{name} not text"),
        }
    }
}

fn read_many(name: &str, count: i64) {
    let mut bytes = 0;
    for _ in 0..count {
        if let Ok(value) = env::var(name) {
            bytes += value.len();
        }
    }
    println!("reads {count}\nbytes {bytes}");
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() >= 2 && args[0] == "get" {
        get(&args[1..]);
        return;
    }
    let count = if args.len() == 3 && args[0] == "read" { args[2].parse::<i64>().unwrap_or(-1) } else { -1 };
    if count < 0 {
        eprintln!("usage: environment get NAME... | read NAME COUNT");
        exit(2);
    }
    read_many(&args[1], count);
}
