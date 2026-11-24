//! `hashsum`: the SHA-256 of each file it is given, worked out by a pool of workers, written in the
//! order the files were given, as GNU's `sha256sum` writes them (choice 1854).
//!
//!     hashsum [-j N] [--] [FILE...]
//!
//! Each line is the hash in hex, two spaces and the name; a name with `\`, a line's end or a CR in it
//! is written with them as `\\`, `\n` and `\r`, and a `\` before the hash. `-`, or no file at all, is
//! the standard input. A file that cannot be read is told on the standard error, the others go on, and
//! the program ends with 1. `-j N` is the number of workers, by default the processors the program may
//! run on (`available_parallelism`).
//!
//! The pool (choice 1855): the files go to the workers on a channel with room for twice as many as
//! there are workers, each worker reads a file in blocks of 64 KB into the hash and sends what it found
//! back on a second channel, and the main thread writes the answers in the order of the files, holding
//! those that come early. It never gives out more than that room past the first file not yet written,
//! so neither channel is ever full and what waits is bounded. The standard input is read by the main
//! thread itself, when its turn comes: a second `-` finds it at its end, as `sha256sum` does.

use std::ffi::OsString;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::exit;

use crossbeam_channel::{Receiver, Sender, bounded};
use sha2::{Digest, Sha256};

const BLOCK: usize = 65536;
const OUTPUT_BLOCK: usize = 65536;

struct Job {
    index: usize,
    path: OsString,
}

/// What a worker found: the line of the output, or the message of the error.
struct Done {
    index: usize,
    failed: bool,
    text: Vec<u8>,
}

/// The line `sha256sum` writes for a hash and a name.
fn line_of(digest: &[u8], name: &[u8]) -> Vec<u8> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let escaped = name.iter().any(|&b| b == b'\\' || b == b'\n' || b == b'\r');
    let mut line = Vec::with_capacity(68 + name.len());
    if escaped {
        line.push(b'\\');
    }
    for &b in digest {
        line.push(HEX[usize::from(b >> 4)]);
        line.push(HEX[usize::from(b & 15)]);
    }
    line.extend_from_slice(b"  ");
    if !escaped {
        line.extend_from_slice(name);
    } else {
        for &b in name {
            match b {
                b'\\' => line.extend_from_slice(b"\\\\"),
                b'\n' => line.extend_from_slice(b"\\n"),
                b'\r' => line.extend_from_slice(b"\\r"),
                _ => line.push(b),
            }
        }
    }
    line.push(b'\n');
    line
}

fn hash_reader(reader: &mut impl Read, block: &mut [u8]) -> io::Result<[u8; 32]> {
    let mut hash = Sha256::new();
    loop {
        let got = match reader.read(block) {
            Ok(0) => break,
            Ok(got) => got,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        hash.update(&block[..got]);
    }
    Ok(hash.finalize().into())
}

fn done_of(index: usize, path: &OsString, hashed: io::Result<[u8; 32]>) -> Done {
    match hashed {
        Ok(digest) => Done { index, failed: false, text: line_of(&digest, path.as_encoded_bytes()) },
        Err(e) => Done { index, failed: true, text: format!("hashsum: {}: {e}", Path::new(path).display()).into_bytes() },
    }
}

fn work(jobs: Receiver<Job>, done: Sender<Done>) {
    let mut block = vec![0u8; BLOCK];
    for job in jobs {
        let hashed = File::open(&job.path).and_then(|mut file| hash_reader(&mut file, &mut block));
        if done.send(done_of(job.index, &job.path, hashed)).is_err() {
            return;
        }
    }
}

/// Gives the files out and writes the answers in order; whether one could not be read. `jobs` ends
/// with it, so the workers end once they have the last.
fn serve(paths: &[OsString], jobs: Sender<Job>, results: &Receiver<Done>, room: usize) -> io::Result<bool> {
    let total = paths.len();
    let mut held: Vec<Option<Done>> = (0..room).map(|_| None).collect();
    let mut block = Vec::new();
    let mut stdout = io::stdout().lock();
    let mut out = Vec::with_capacity(OUTPUT_BLOCK + 4096);
    let (mut given, mut written, mut failed) = (0, 0, false);
    while written < total {
        // What is there in order is written.
        while written < given {
            let Some(done) = held[written % room].take() else { break };
            if done.failed {
                stdout.write_all(&out)?;
                out.clear();
                stdout.flush()?;
                let mut error = done.text;
                error.push(b'\n');
                io::stderr().write_all(&error)?;
                failed = true;
            } else {
                out.extend_from_slice(&done.text);
                if out.len() >= OUTPUT_BLOCK {
                    stdout.write_all(&out)?;
                    out.clear();
                }
            }
            written += 1;
        }
        if written == total {
            break;
        }
        if given < total && given - written < room {
            if paths[given] == "-" {
                if block.is_empty() {
                    block = vec![0u8; BLOCK];
                }
                held[given % room] = Some(done_of(given, &paths[given], hash_reader(&mut io::stdin().lock(), &mut block)));
            } else if jobs.send(Job { index: given, path: paths[given].clone() }).is_err() {
                break;
            }
            given += 1;
            continue;
        }
        let done = results.recv().unwrap_or(Done { index: written, failed: true, text: b"hashsum: a worker stopped".to_vec() });
        let at = done.index % room;
        held[at] = Some(done);
    }
    stdout.write_all(&out)?;
    stdout.flush()?;
    Ok(failed)
}

fn usage() -> ! {
    eprintln!("usage: hashsum [-j N] [--] [FILE...]");
    exit(2)
}

fn main() {
    // A closed output ends the program by SIGPIPE, as it ends `sha256sum` and a program of C.
    #[cfg(unix)]
    unsafe {
        unsafe extern "C" {
            fn signal(signum: i32, handler: usize) -> usize;
        }
        signal(13, 0);
    }
    let mut workers = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let mut paths = Vec::new();
    let mut options = true;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if options && arg == "--" {
            options = false;
        } else if options && arg == "-j" {
            match args.next().and_then(|n| n.into_string().ok()).and_then(|n| n.parse().ok()) {
                Some(n) => workers = n,
                None => usage(),
            }
        } else if options && arg.as_encoded_bytes().starts_with(b"-") && arg != "-" {
            usage();
        } else {
            paths.push(arg);
        }
    }
    if workers < 1 {
        usage();
    }
    if paths.is_empty() {
        paths.push(OsString::from("-"));
    }
    let room = 2 * workers;
    let (job_sender, jobs) = bounded::<Job>(room);
    let (done, results) = bounded::<Done>(room);
    let served = std::thread::scope(|scope| {
        for _ in 0..workers {
            let (jobs, done) = (jobs.clone(), done.clone());
            scope.spawn(move || work(jobs, done));
        }
        drop(jobs);
        drop(done);
        serve(&paths, job_sender, &results, room)
    });
    match served {
        Ok(false) => {}
        Ok(true) => exit(1),
        Err(e) => {
            eprintln!("hashsum: {e}");
            exit(1)
        }
    }
}
