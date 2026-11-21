//! `zip`: `std`'s zip archives measured against the `zip` crate (choices 1553, 1629-1637, 1643).
//! Three ways of using them:
//!
//!     zip write FILE OUT    FILE cut into parts of 10,000 bytes, `logs/part-00000.txt` and on,
//!                           each deflated at level 6 into the archive OUT
//!     zip read ARCHIVE      every entry of ARCHIVE opened into memory, its CRC-32 checked
//!     zip list ARCHIVE      the name and the size of every entry, as the archive lists them
//!
//! What `write` and `read` print: how many entries and their bytes. An archive that cannot be read
//! is told on the standard error, with exit code 1; a wrong use, with 2.

use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::process::exit;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

type Error = Box<dyn std::error::Error>;

fn write(path: &str, out: &str) -> Result<(), Error> {
    let data = std::fs::read(path)?;
    let when = DateTime::from_date_and_time(2026, 11, 20, 13, 45, 30)?;
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(6))
        .last_modified_time(when);
    let mut zip = ZipWriter::new(BufWriter::with_capacity(65536, File::create(out)?));
    let mut entries = 0;
    for part in data.chunks(10000) {
        zip.start_file(format!("logs/part-{entries:05}.txt"), options)?;
        zip.write_all(part)?;
        entries += 1;
    }
    zip.finish()?.flush()?;
    println!("entries {entries}\nbytes {}", data.len());
    Ok(())
}

fn read(path: &str) -> Result<(), Error> {
    let mut zip = ZipArchive::new(BufReader::with_capacity(65536, File::open(path)?))?;
    let (mut entries, mut bytes) = (0u64, 0u64);
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let mut data = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut data)?;
        entries += 1;
        bytes += data.len() as u64;
    }
    println!("entries {entries}\nbytes {bytes}");
    Ok(())
}

fn list(path: &str) -> Result<(), Error> {
    let mut zip = ZipArchive::new(BufReader::with_capacity(65536, File::open(path)?))?;
    let stdout = io::stdout();
    let mut out = BufWriter::with_capacity(65536, stdout.lock());
    for i in 0..zip.len() {
        let entry = zip.by_index_raw(i)?;
        writeln!(out, "{} {}", entry.name()?, entry.size())?;
    }
    out.flush()?;
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usage = (args.len() == 3 && args[0] == "write")
        || (args.len() == 2 && (args[0] == "read" || args[0] == "list"));
    if !usage {
        eprintln!("usage: zip write FILE OUT | read ARCHIVE | list ARCHIVE");
        exit(2);
    }
    let result = match args[0].as_str() {
        "write" => write(&args[1], &args[2]),
        "read" => read(&args[1]),
        _ => list(&args[1]),
    };
    if let Err(e) = result {
        eprintln!("zip: {}: {e}", args[1]);
        exit(1);
    }
}
