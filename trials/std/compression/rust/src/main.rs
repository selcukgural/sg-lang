//! `compression`: `std`'s compression measured against `flate2` (choices 1553, 1629-1637, 1642).
//! Three ways of using it, all at level 6, zlib's default:
//!
//!     compression compress FILE OUT  FILE in gzip into OUT, read and written a piece at a time
//!     compression decompress FILE    the gzip FILE opened on the standard output, a piece at a time
//!     compression records FILE       each line of FILE, without its `\n`, compressed in zlib and
//!                                    opened again in memory
//!
//! What `compress` prints: the bytes it read. What `records` prints: how many lines, their bytes,
//! and how many came back the same; the bytes they took compressed go to the standard error, as
//! the size of what `compress` writes is not compared: each compressor finds its own matches. A
//! file that cannot be read or opened is told on the standard error, with exit code 1; a wrong use,
//! with 2.

use flate2::Compression;
use flate2::read::{MultiGzDecoder, ZlibDecoder};
use flate2::write::{GzEncoder, ZlibEncoder};
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};
use std::process::exit;

fn compress(path: &str, out: &str) -> io::Result<u64> {
    let mut source = File::open(path)?;
    let target = BufWriter::with_capacity(65536, File::create(out)?);
    let mut encoder = GzEncoder::new(target, Compression::new(6));
    let mut buffer = vec![0u8; 65536];
    let mut total = 0u64;
    loop {
        let count = source.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        encoder.write_all(&buffer[..count])?;
        total += count as u64;
    }
    encoder.finish()?.flush()?;
    Ok(total)
}

fn decompress(path: &str) -> io::Result<()> {
    let source = BufReader::with_capacity(65536, File::open(path)?);
    let mut decoder = MultiGzDecoder::new(source);
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut buffer = vec![0u8; 65536];
    loop {
        let count = decoder.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        out.write_all(&buffer[..count])?;
    }
    out.flush()
}

fn records(path: &str) -> io::Result<()> {
    let mut reader = BufReader::with_capacity(65536, File::open(path)?);
    let mut line = Vec::new();
    let (mut lines, mut bytes, mut same, mut compressed) = (0u64, 0u64, 0u64, 0u64);
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
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(6));
        encoder.write_all(&line)?;
        let packed = encoder.finish()?;
        let mut opened = Vec::with_capacity(line.len());
        ZlibDecoder::new(&packed[..]).read_to_end(&mut opened)?;
        lines += 1;
        bytes += line.len() as u64;
        compressed += packed.len() as u64;
        if opened == line {
            same += 1;
        }
    }
    let stdout = io::stdout();
    let mut out = stdout.lock();
    writeln!(out, "records {lines}\nbytes {bytes}\nsame {same}")?;
    eprintln!("compressed {compressed}");
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usage = (args.len() == 3 && args[0] == "compress")
        || (args.len() == 2 && (args[0] == "decompress" || args[0] == "records"));
    if !usage {
        eprintln!("usage: compression compress FILE OUT | decompress FILE | records FILE");
        exit(2);
    }
    let result = match args[0].as_str() {
        "compress" => compress(&args[1], &args[2]).map(|total| println!("bytes {total}")),
        "decompress" => decompress(&args[1]),
        _ => records(&args[1]),
    };
    if let Err(e) = result {
        eprintln!("compression: {}: {e}", args[1]);
        exit(1);
    }
}
