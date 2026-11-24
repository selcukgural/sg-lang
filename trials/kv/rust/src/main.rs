//! `kv`: a store of keys and values held in memory, each change written first to a log in the folder
//! it is given, so that what was answered `OK` is there again when the store starts after a crash.
//!
//!     kv [--sync always|batch|never] DIR
//!
//! The commands come on the input, a line each: `SET key value` (the key ends at the next space, the
//! value is the rest of the line, spaces and all), `GET key`, `DEL key` and `COMPACT`; a key and a
//! value are bytes, UTF-8 or not, and a line ends at `\n` alone. `SET` answers `OK`, `GET` the value or
//! `(nil)`, `DEL` `1` or `0`, `COMPACT` `OK`. A line that is not a command is told on the standard error,
//! and the store ends with 1 once the input is done.
//!
//! The log, `DIR/log`, is records one after another (choice 1845), each
//!
//!     crc u32 | op u8 | key length u32 | value length u32 | key | value
//!
//! little-endian, the CRC-32 that of everything after it; the op is 1 for a set, 2 for a delete, whose
//! value is empty. The input is read in blocks of up to 64 KB; what one block asks is answered after
//! its records are on the disk (`--sync batch`, the default), or with the log only handed to the system
//! (`never`); with `always` each command is answered after its record is on the disk. A record cut off at the end of the log, as a crash in
//! the middle of a write leaves it, is cut away and told; a record whose CRC is wrong with more after
//! it is damage, and the store does not start (choice 1847). When the log is more than twice what the
//! store holds, and more than 1 MB, it is written again with only what is held, keys in the order of
//! their bytes, into `DIR/log.new`, which then takes its place (choice 1848).

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const HEADER_LENGTH: usize = 13;
const INPUT_BLOCK: usize = 65536;
const OP_SET: u8 = 1;
const OP_DELETE: u8 = 2;
const COMPACT_FROM: u64 = 1_048_576;

#[derive(Clone, Copy, PartialEq)]
enum Sync {
    Always,
    Batch,
    Never,
}

struct Store {
    map: HashMap<Vec<u8>, Vec<u8>>,
    folder: PathBuf,
    log_path: PathBuf,
    log: BufWriter<File>,
    sync: Sync,
    /// The bytes of the log, and of the records of what is held.
    log_length: u64,
    live_length: u64,
    /// Records written since the log was last put on the disk.
    unsynced: bool,
}

fn record_length(key: usize, value: usize) -> u64 {
    (HEADER_LENGTH + key + value) as u64
}

/// One record into the log.
fn write_record(log: &mut impl Write, op: u8, key: &[u8], value: &[u8]) -> io::Result<()> {
    let mut header = [0u8; HEADER_LENGTH];
    header[4] = op;
    header[5..9].copy_from_slice(&(key.len() as u32).to_le_bytes());
    header[9..13].copy_from_slice(&(value.len() as u32).to_le_bytes());
    let mut crc = crc32fast::Hasher::new();
    crc.update(&header[4..]);
    crc.update(key);
    crc.update(value);
    header[0..4].copy_from_slice(&crc.finalize().to_le_bytes());
    log.write_all(&header)?;
    log.write_all(key)?;
    log.write_all(value)
}

fn fail(message: &str) -> ! {
    eprintln!("kv: {message}");
    std::process::exit(1);
}

fn uint_at(at: &[u8]) -> u32 {
    u32::from_le_bytes([at[0], at[1], at[2], at[3]])
}

/// Reads as much as there is up to the length of `buffer`: fewer bytes only at the end.
fn read_full(reader: &mut impl Read, buffer: &mut [u8]) -> io::Result<usize> {
    let mut got = 0;
    while got < buffer.len() {
        match reader.read(&mut buffer[got..]) {
            Ok(0) => break,
            Ok(n) => got += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(got)
}

/// Reads the log into the map. A record cut off at the end is cut away; one whose CRC or op is
/// wrong with more after it stops the store.
fn recover(store: &mut Store) -> io::Result<()> {
    let file = match File::open(&store.log_path) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    let size = file.metadata()?.len();
    let mut reader = BufReader::with_capacity(INPUT_BLOCK, file);
    let mut header = [0u8; HEADER_LENGTH];
    let mut bytes = Vec::new();
    let mut at = 0u64;
    let mut cut = None;
    while at < size {
        if read_full(&mut reader, &mut header)? < HEADER_LENGTH {
            cut = Some(at);
            break;
        }
        let key_length = uint_at(&header[5..9]) as u64;
        let value_length = uint_at(&header[9..13]) as u64;
        let length = HEADER_LENGTH as u64 + key_length + value_length;
        if length > size - at {
            cut = Some(at);
            break;
        }
        let body = (key_length + value_length) as usize;
        if bytes.len() < body {
            bytes.resize(body, 0);
        }
        read_full(&mut reader, &mut bytes[..body])?;
        let mut crc = crc32fast::Hasher::new();
        crc.update(&header[4..]);
        crc.update(&bytes[..body]);
        let right = crc.finalize() == uint_at(&header[0..4]);
        let op = header[4];
        let known = op == OP_SET || (op == OP_DELETE && value_length == 0);
        if !right || !known {
            // The last record of the log, written only in part before a crash.
            if at + length == size && !right {
                cut = Some(at);
                break;
            }
            fail(&format!("log: a damaged record at {at}"));
        }
        let k = key_length as usize;
        if op == OP_SET {
            let value = bytes[k..body].to_vec();
            match store.map.get_mut(&bytes[..k]) {
                Some(slot) => {
                    store.live_length -= record_length(k, slot.len());
                    *slot = value;
                }
                None => {
                    store.map.insert(bytes[..k].to_vec(), value);
                }
            }
            store.live_length += length;
        } else if let Some(old) = store.map.remove(&bytes[..k]) {
            store.live_length -= record_length(k, old.len());
        }
        at += length;
    }
    store.log_length = at;
    if let Some(cut) = cut {
        eprintln!("kv: log: a record cut off at {cut}, {} bytes cut", size - cut);
        let log = OpenOptions::new().append(true).open(&store.log_path)?;
        log.set_len(cut)?;
        if store.sync != Sync::Never {
            log.sync_data()?;
        }
    }
    Ok(())
}

/// The log written again with only what the store holds, in the order of the keys' bytes.
fn compact(store: &mut Store) -> io::Result<()> {
    store.log.flush()?;
    let mut entries: Vec<(&Vec<u8>, &Vec<u8>)> = store.map.iter().collect();
    entries.sort_unstable_by(|a, b| a.0.cmp(b.0));
    let fresh = store.folder.join("log.new");
    let mut log = BufWriter::with_capacity(INPUT_BLOCK, File::create(&fresh)?);
    let mut length = 0;
    for (key, value) in entries {
        write_record(&mut log, OP_SET, key, value)?;
        length += record_length(key.len(), value.len());
    }
    let log = log.into_inner().map_err(|e| e.into_error())?;
    if store.sync != Sync::Never {
        log.sync_data()?;
    }
    drop(log);
    fs::rename(&fresh, &store.log_path)?;
    if store.sync != Sync::Never {
        File::open(&store.folder)?.sync_all()?;
    }
    store.log = BufWriter::with_capacity(INPUT_BLOCK, OpenOptions::new().append(true).open(&store.log_path)?);
    store.log_length = length;
    store.live_length = length;
    Ok(())
}

/// A record written; after `always`, on the disk.
fn logged(store: &mut Store, op: u8, key: &[u8], value: &[u8]) -> io::Result<()> {
    write_record(&mut store.log, op, key, value)?;
    store.log_length += record_length(key.len(), value.len());
    if store.sync == Sync::Always {
        store.log.flush()?;
        store.log.get_ref().sync_data()?;
    } else {
        store.unsynced = true;
    }
    Ok(())
}

fn after_change(store: &mut Store) -> io::Result<()> {
    if store.log_length > COMPACT_FROM && store.log_length > 2 * store.live_length {
        compact(store)?;
    }
    Ok(())
}

/// What one line asks, its answer put into `out`; false for a line that is not a command.
fn run(store: &mut Store, line: &[u8], out: &mut Vec<u8>) -> io::Result<bool> {
    let Some(s) = line.iter().position(|&b| b == b' ') else {
        if line == b"COMPACT" {
            compact(store)?;
            out.extend_from_slice(b"OK\n");
            return Ok(true);
        }
        return Ok(false);
    };
    let (word, rest) = (&line[..s], &line[s + 1..]);
    match word {
        b"SET" => {
            let Some(k) = rest.iter().position(|&b| b == b' ') else { return Ok(false) };
            let (key, given) = (&rest[..k], &rest[k + 1..]);
            logged(store, OP_SET, key, given)?;
            match store.map.get_mut(key) {
                Some(slot) => {
                    store.live_length -= record_length(k, slot.len());
                    *slot = given.to_vec();
                }
                None => {
                    store.map.insert(key.to_vec(), given.to_vec());
                }
            }
            store.live_length += record_length(k, given.len());
            out.extend_from_slice(b"OK\n");
            after_change(store)?;
        }
        b"GET" => match store.map.get(rest) {
            Some(value) => {
                out.extend_from_slice(value);
                out.push(b'\n');
            }
            None => out.extend_from_slice(b"(nil)\n"),
        },
        b"DEL" => {
            if !store.map.contains_key(rest) {
                out.extend_from_slice(b"0\n");
                return Ok(true);
            }
            logged(store, OP_DELETE, rest, b"")?;
            if let Some(old) = store.map.remove(rest) {
                store.live_length -= record_length(rest.len(), old.len());
            }
            out.extend_from_slice(b"1\n");
            after_change(store)?;
        }
        _ => return Ok(false),
    }
    Ok(true)
}

/// The output has gone: the program ends as a C program does, by SIGPIPE.
fn closed_pipe() -> ! {
    #[cfg(unix)]
    unsafe {
        unsafe extern "C" {
            fn signal(signal: i32, handler: usize) -> usize;
            fn raise(signal: i32) -> i32;
        }
        const SIGPIPE: i32 = 13;
        const SIG_DFL: usize = 0;
        signal(SIGPIPE, SIG_DFL);
        raise(SIGPIPE);
    }
    std::process::exit(0);
}

/// What a block asked is answered once its records are where `--sync` says.
fn answered(store: &mut Store, out: &mut Vec<u8>, stdout: &mut impl Write) -> io::Result<()> {
    if store.unsynced {
        store.log.flush()?;
        if store.sync == Sync::Batch {
            store.log.get_ref().sync_data()?;
        }
        store.unsynced = false;
    }
    if let Err(e) = stdout.write_all(out).and_then(|()| stdout.flush()) {
        if e.kind() == io::ErrorKind::BrokenPipe {
            closed_pipe();
        }
        return Err(e);
    }
    out.clear();
    Ok(())
}

fn usage() -> ! {
    eprintln!("usage: kv [--sync always|batch|never] DIR");
    std::process::exit(2);
}

fn serve(sync: Sync, folder: &Path) -> io::Result<ExitCode> {
    if !folder.is_dir() {
        fs::create_dir(folder)?;
    }
    let log_path = folder.join("log");
    let left_over = folder.join("log.new");
    if left_over.exists() {
        fs::remove_file(&left_over)?;
    }
    let made = !log_path.exists();
    let log = OpenOptions::new().append(true).create(true).open(&log_path)?;
    let mut store = Store {
        map: HashMap::new(),
        folder: folder.to_path_buf(),
        log_path,
        log: BufWriter::with_capacity(INPUT_BLOCK, log),
        sync,
        log_length: 0,
        live_length: 0,
        unsynced: false,
    };
    if made && sync != Sync::Never {
        File::open(folder)?.sync_all()?;
    }
    recover(&mut store)?;

    let stdin = io::stdin();
    let mut input = stdin.lock();
    let stdout = io::stdout();
    let mut output = stdout.lock();
    let mut block = vec![0u8; INPUT_BLOCK];
    // A line that goes on past the block it started in.
    let mut carried = Vec::new();
    let mut out = Vec::new();
    let mut line_number = 0u64;
    let mut bad = false;
    loop {
        let got = match input.read(&mut block) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        let mut start = 0;
        while start < got {
            let Some(end) = block[start..got].iter().position(|&b| b == b'\n') else {
                carried.extend_from_slice(&block[start..got]);
                break;
            };
            let e = start + end;
            line_number += 1;
            let ok = if carried.is_empty() {
                run(&mut store, &block[start..e], &mut out)?
            } else {
                carried.extend_from_slice(&block[start..e]);
                let line = std::mem::take(&mut carried);
                let ok = run(&mut store, &line, &mut out)?;
                carried = line;
                carried.clear();
                ok
            };
            if !ok {
                eprintln!("kv: line {line_number}: not a command");
                bad = true;
            }
            // Each answer as soon as its record is on the disk.
            if store.sync == Sync::Always {
                answered(&mut store, &mut out, &mut output)?;
            }
            start = e + 1;
        }
        answered(&mut store, &mut out, &mut output)?;
    }
    // The last line, without its `\n`.
    if !carried.is_empty() {
        line_number += 1;
        let line = std::mem::take(&mut carried);
        if !run(&mut store, &line, &mut out)? {
            eprintln!("kv: line {line_number}: not a command");
            bad = true;
        }
    }
    answered(&mut store, &mut out, &mut output)?;
    store.log.flush()?;
    Ok(if bad { ExitCode::from(1) } else { ExitCode::SUCCESS })
}

fn main() -> ExitCode {
    let mut sync = Sync::Batch;
    let mut folder = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--sync" {
            sync = match args.next().as_ref().and_then(|mode| mode.to_str()) {
                Some("always") => Sync::Always,
                Some("batch") => Sync::Batch,
                Some("never") => Sync::Never,
                _ => usage(),
            };
        } else if folder.is_none() && !arg.to_string_lossy().starts_with('-') {
            folder = Some(PathBuf::from(arg));
        } else {
            usage();
        }
    }
    let Some(folder) = folder else { usage() };
    match serve(sync, &folder) {
        Ok(code) => code,
        Err(e) => fail(&e.to_string()),
    }
}
