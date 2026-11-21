//! `hash`: `std`'s SHA-256 and HMAC-SHA-256 measured against `sha2` and `hmac` (choices 1553,
//! 1629-1637, 1653-1654). Three ways of using them:
//!
//!     hash sha256 FILE    the SHA-256 of FILE, read a piece of 64 KB at a time, as `sha256sum` does
//!     hash chain COUNT    32 zero bytes hashed COUNT times, each time the hash of the one before
//!     hash hmac FILE      each line of FILE, without its end (`\n` or `\r\n`), signed with
//!                         HMAC-SHA-256 under one key of 32 bytes; how many lines, and every tag
//!                         put together with XOR
//!
//! The hashes are printed in hexadecimal. A file that cannot be read is told on the standard error,
//! with exit code 1; a wrong use, with 2.

use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use std::process::exit;

fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        write!(text, "{b:02x}").unwrap();
    }
    text
}

fn sha256(path: &str) -> io::Result<()> {
    let mut source = File::open(path)?;
    let mut buffer = vec![0u8; 65536];
    let mut h = Sha256::new();
    loop {
        let count = source.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        h.update(&buffer[..count]);
    }
    println!("{}", hex(&h.finalize()));
    Ok(())
}

fn chain(count: u64) {
    let mut h = [0u8; 32];
    for _ in 0..count {
        h = Sha256::digest(h).into();
    }
    println!("{}", hex(&h));
}

fn hmac(path: &str) -> io::Result<()> {
    let key: Vec<u8> = (0..32).collect();
    let mut reader = BufReader::with_capacity(65536, File::open(path)?);
    let mut line = Vec::new();
    let mut lines = 0u64;
    let mut all = [0u8; 32];
    loop {
        line.clear();
        if reader.read_until(b'\n', &mut line)? == 0 {
            break;
        }
        if line.last() == Some(&b'\n') {
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
        }
        let mut mac = Hmac::<Sha256>::new_from_slice(&key).expect("any length of key");
        mac.update(&line);
        let tag = mac.finalize().into_bytes();
        for (a, t) in all.iter_mut().zip(tag.iter()) {
            *a ^= t;
        }
        lines += 1;
    }
    println!("lines {lines}\nxor {}", hex(&all));
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() == 2 && args[0] == "chain" {
        if let Ok(count) = args[1].parse::<u64>() {
            chain(count);
            return;
        }
    }
    if args.len() != 2 || (args[0] != "sha256" && args[0] != "hmac") {
        eprintln!("usage: hash sha256 FILE | chain COUNT | hmac FILE");
        exit(2);
    }
    let result = if args[0] == "sha256" { sha256(&args[1]) } else { hmac(&args[1]) };
    if let Err(e) = result {
        eprintln!("hash: {}: {e}", args[1]);
        exit(1);
    }
}
