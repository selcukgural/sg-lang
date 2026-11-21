//! `checksum`: `std`'s CRC-32 and Adler-32 measured against `crc32fast` and `adler2` (choices 1553,
//! 1629-1637, 1653, 1655). Three ways of using them:
//!
//!     checksum crc32 FILE      the CRC-32 of FILE, read a piece of 64 KB at a time
//!     checksum adler32 FILE    the Adler-32 of FILE, read the same way
//!     checksum lines FILE      the CRC-32 of each line of FILE, without its end (`\n` or `\r\n`);
//!                              how many lines, and every CRC-32 put together with XOR
//!
//! The checksums are printed in hexadecimal. A file that cannot be read is told on the standard
//! error, with exit code 1; a wrong use, with 2.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use std::process::exit;

fn whole(path: &str, crc: bool) -> io::Result<u32> {
    let mut source = File::open(path)?;
    let mut buffer = vec![0u8; 65536];
    let mut c = crc32fast::Hasher::new();
    let mut a = adler2::Adler32::new();
    loop {
        let count = source.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        if crc {
            c.update(&buffer[..count]);
        } else {
            a.write_slice(&buffer[..count]);
        }
    }
    Ok(if crc { c.finalize() } else { a.checksum() })
}

fn lines(path: &str) -> io::Result<()> {
    let mut reader = BufReader::with_capacity(65536, File::open(path)?);
    let mut line = Vec::new();
    let mut count = 0u64;
    let mut all = 0u32;
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
        all ^= crc32fast::hash(&line);
        count += 1;
    }
    println!("lines {count}\nxor {all:08x}");
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 2 || !["crc32", "adler32", "lines"].contains(&args[0].as_str()) {
        eprintln!("usage: checksum crc32 FILE | adler32 FILE | lines FILE");
        exit(2);
    }
    let result = match args[0].as_str() {
        "lines" => lines(&args[1]),
        mode => whole(&args[1], mode == "crc32").map(|sum| println!("{sum:08x}")),
    };
    if let Err(e) = result {
        eprintln!("checksum: {}: {e}", args[1]);
        exit(1);
    }
}
