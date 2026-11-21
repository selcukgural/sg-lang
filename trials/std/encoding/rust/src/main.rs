//! `encoding`: `std`'s hex and Base64 measured against `base64` and `hex` (choices 1553, 1629-1637,
//! 1659, 1660). Four ways of using them:
//!
//!     encoding encode FILE OUT    FILE in Base64, the standard alphabet, padded, written into OUT;
//!                                 FILE is read 48 KB at a time (a multiple of three, so that the
//!                                 pieces written one after another are one text)
//!     encoding decode FILE OUT    FILE, a text in Base64, read back into bytes 64 KB of text at a
//!                                 time and written into OUT
//!     encoding hex COUNT          COUNT values of 32 bytes, each written as hex and read back
//!     encoding tokens COUNT       COUNT tokens of 16 to 48 bytes, each written in the Base64 of URLs
//!                                 and read back
//!
//! `encode` and `decode` print how many bytes they wrote. `hex` and `tokens` make their bytes with
//! SplitMix64 from the seed 1 and print how many they made, how many characters the texts had, a
//! sum of one character of each text and every last 8 bytes read back put together with XOR. A file
//! that cannot be read or written is told on the standard error, with exit code 1, as is a text
//! that is not Base64; a wrong use, with 2.
//!
//! Each crate is used the fastest way it offers, into buffers kept from one call to the next
//! (`encode_slice`, `decode_slice`, `encode_to_slice`, `decode_to_slice`), not a new `String` or
//! `Vec` for each.

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use std::fs::File;
use std::io::{self, BufWriter, Read, Write};
use std::process::exit;

const USAGE: &str = "usage: encoding encode FILE OUT | decode FILE OUT | hex COUNT | tokens COUNT";

struct SplitMix(u64);

impl SplitMix {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }

    /// `buffer` filled from as many values as it needs, each value's lowest byte first.
    fn fill(&mut self, buffer: &mut [u8]) {
        for chunk in buffer.chunks_mut(8) {
            let z = self.next().to_le_bytes();
            chunk.copy_from_slice(&z[..chunk.len()]);
        }
    }
}

fn last_eight(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes[bytes.len() - 8..].try_into().unwrap())
}

/// Fills `buffer` unless the end comes first; how many bytes.
fn read_full(source: &mut File, buffer: &mut [u8]) -> io::Result<usize> {
    let mut count = 0;
    while count < buffer.len() {
        match source.read(&mut buffer[count..]) {
            Ok(0) => break,
            Ok(n) => count += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(count)
}

fn encode(path: &str, out: &str) -> io::Result<()> {
    let mut source = File::open(path)?;
    let mut target = BufWriter::with_capacity(65536, File::create(out)?);
    let mut buffer = vec![0u8; 49152];
    let mut text = vec![0u8; 65536];
    let mut written = 0u64;
    loop {
        let count = read_full(&mut source, &mut buffer)?;
        if count == 0 {
            break;
        }
        let length = STANDARD.encode_slice(&buffer[..count], &mut text).unwrap();
        target.write_all(&text[..length])?;
        written += length as u64;
    }
    target.flush()?;
    println!("bytes {written}");
    Ok(())
}

fn decode(path: &str, out: &str) -> io::Result<()> {
    let mut source = File::open(path)?;
    let mut target = BufWriter::with_capacity(65536, File::create(out)?);
    let mut buffer = vec![0u8; 65536];
    let mut bytes = vec![0u8; 49152];
    let mut written = 0u64;
    loop {
        let count = read_full(&mut source, &mut buffer)?;
        if count == 0 {
            break;
        }
        match STANDARD.decode_slice(&buffer[..count], &mut bytes) {
            Ok(length) => {
                target.write_all(&bytes[..length])?;
                written += length as u64;
            }
            Err(e) => {
                eprintln!("encoding: {path}: not Base64: {e}");
                exit(1);
            }
        }
    }
    target.flush()?;
    println!("bytes {written}");
    Ok(())
}

fn hex(count: u64) {
    let mut random = SplitMix(1);
    let mut value = [0u8; 32];
    let mut text = [0u8; 64];
    let mut back = [0u8; 32];
    let (mut digits, mut sum, mut all) = (0u64, 0u64, 0u64);
    for i in 0..count {
        random.fill(&mut value);
        hex::encode_to_slice(value, &mut text).unwrap();
        hex::decode_to_slice(text, &mut back).expect("hex reads back");
        digits += text.len() as u64;
        sum += text[(i % text.len() as u64) as usize] as u64;
        all ^= last_eight(&back);
    }
    println!("values {count}\ndigits {digits}\nsum {sum}\nxor {all:016x}");
}

fn tokens(count: u64) {
    let mut random = SplitMix(1);
    let mut token = [0u8; 48];
    let mut text = [0u8; 64];
    let mut back = [0u8; 48];
    let (mut characters, mut sum, mut all) = (0u64, 0u64, 0u64);
    for i in 0..count {
        let length = 16 + (random.next() % 33) as usize;
        random.fill(&mut token[..length]);
        let written = URL_SAFE_NO_PAD.encode_slice(&token[..length], &mut text).unwrap();
        let read = URL_SAFE_NO_PAD.decode_slice(&text[..written], &mut back).expect("Base64 reads back");
        characters += written as u64;
        sum += text[(i % written as u64) as usize] as u64;
        all ^= last_eight(&back[..read]);
    }
    println!("tokens {count}\ncharacters {characters}\nsum {sum}\nxor {all:016x}");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let files = args.len() == 3 && (args[0] == "encode" || args[0] == "decode");
    let counts = args.len() == 2 && (args[0] == "hex" || args[0] == "tokens");
    if !files && !counts {
        eprintln!("{USAGE}");
        exit(2);
    }
    if counts {
        let Ok(count) = args[1].parse::<u64>() else {
            eprintln!("{USAGE}");
            exit(2);
        };
        if args[0] == "hex" { hex(count) } else { tokens(count) }
        return;
    }
    let result = if args[0] == "encode" { encode(&args[1], &args[2]) } else { decode(&args[1], &args[2]) };
    if let Err(e) = result {
        eprintln!("encoding: {}: {e}", args[1]);
        exit(1);
    }
}
