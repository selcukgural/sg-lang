//! The store of the server lane in Rust (choices 1701, 1702), as `../c/resp.c` describes it: a part of
//! RESP2 (PING, GET, SET, DEL) over 64 maps, each behind a lock of its own; each connection reads into a
//! buffer of 4 KB, which grows when one command does not fit, answers every whole command the buffer
//! holds and sends the answers together; a command of more than 1,024 parts or a part of more than
//! 1 MB closes the connection.
//!
//!     resp PORT
//!
//! Each connection is a task of `tokio::spawn`. The maps are `std::collections::HashMap` with its
//! default hasher, SipHash-1-3 with a key drawn at the start; the shard is chosen by the FNV-1a of
//! the key, as in every version. The sockets are given TCP_NODELAY, as a server is.

use std::collections::HashMap;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const SHARDS: usize = 64;
const FIRST_BUFFER: usize = 4096;
const MOST_PARTS: usize = 1024;
const MOST_PART: i64 = 1 << 20;

type Store = Vec<Mutex<HashMap<Vec<u8>, Vec<u8>>>>;

fn shard_of(key: &[u8]) -> usize {
    let mut h: u64 = 14695981039346656037;
    for &b in key {
        h = (h ^ b as u64).wrapping_mul(1099511628211);
    }
    (h & (SHARDS as u64 - 1)) as usize
}

fn put_number(out: &mut Vec<u8>, kind: u8, n: i64) {
    out.push(kind);
    out.extend_from_slice(n.to_string().as_bytes());
    out.extend_from_slice(b"\r\n");
}

enum Line {
    Whole(i64, usize),
    Incomplete,
    Wrong,
}

fn read_number(bytes: &[u8], at: usize) -> Line {
    let mut i = at;
    let minus = i < bytes.len() && bytes[i] == b'-';
    if minus {
        i += 1;
    }
    let digits = i;
    let mut value: i64 = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        if value > 1_000_000_000 {
            return Line::Wrong;
        }
        value = value * 10 + (bytes[i] - b'0') as i64;
        i += 1;
    }
    if i + 1 >= bytes.len() {
        return if bytes.len() - at > 24 { Line::Wrong } else { Line::Incomplete };
    }
    if i == digits || bytes[i] != b'\r' || bytes[i + 1] != b'\n' {
        return Line::Wrong;
    }
    Line::Whole(if minus { -value } else { value }, i + 2)
}

fn is_word(part: &[u8], word: &[u8]) -> bool {
    part.len() == word.len() && part.iter().zip(word).all(|(a, b)| (a | 0x20) == *b)
}

fn run(store: &Store, bytes: &[u8], ranges: &[(usize, usize)], out: &mut Vec<u8>) {
    let mut parts: [&[u8]; 3] = [&[]; 3];
    for (i, &(a, b)) in ranges.iter().take(3).enumerate() {
        parts[i] = &bytes[a..b];
    }
    let count = ranges.len();
    let name = parts[0];
    if is_word(name, b"ping") && count == 1 {
        out.extend_from_slice(b"+PONG\r\n");
    } else if is_word(name, b"get") && count == 2 {
        let shard = store[shard_of(parts[1])].lock().unwrap();
        match shard.get(parts[1]) {
            Some(value) => {
                put_number(out, b'$', value.len() as i64);
                out.extend_from_slice(value);
                out.extend_from_slice(b"\r\n");
            }
            None => out.extend_from_slice(b"$-1\r\n"),
        }
    } else if is_word(name, b"set") && count == 3 {
        let mut shard = store[shard_of(parts[1])].lock().unwrap();
        match shard.get_mut(parts[1]) {
            Some(value) => {
                value.clear();
                value.extend_from_slice(parts[2]);
            }
            None => {
                shard.insert(parts[1].to_vec(), parts[2].to_vec());
            }
        }
        drop(shard);
        out.extend_from_slice(b"+OK\r\n");
    } else if is_word(name, b"del") && count >= 2 {
        let mut gone = 0;
        for &(a, b) in &ranges[1..] {
            let key = &bytes[a..b];
            if store[shard_of(key)].lock().unwrap().remove(key).is_some() {
                gone += 1;
            }
        }
        put_number(out, b':', gone);
    } else {
        out.extend_from_slice(b"-ERR unknown command or wrong number of arguments\r\n");
    }
}

/// Answers every whole command in `bytes`; how many bytes it used, or `None` when the client broke
/// the protocol.
fn serve_all(store: &Store, bytes: &[u8], parts: &mut Vec<(usize, usize)>, out: &mut Vec<u8>) -> Option<usize> {
    let mut used = 0;
    while used < bytes.len() {
        if bytes[used] != b'*' {
            return None;
        }
        let (count, mut at) = match read_number(bytes, used + 1) {
            Line::Whole(v, next) if (1..=MOST_PARTS as i64).contains(&v) => (v as usize, next),
            Line::Incomplete => return Some(used),
            _ => return None,
        };
        parts.clear();
        for _ in 0..count {
            if at >= bytes.len() {
                return Some(used);
            }
            if bytes[at] != b'$' {
                return None;
            }
            match read_number(bytes, at + 1) {
                Line::Whole(n, next) if (0..=MOST_PART).contains(&n) => {
                    let n = n as usize;
                    if bytes.len() - next < n + 2 {
                        return Some(used);
                    }
                    let end = next + n;
                    if bytes[end] != b'\r' || bytes[end + 1] != b'\n' {
                        return None;
                    }
                    parts.push((next, end));
                    at = end + 2;
                }
                Line::Incomplete => return Some(used),
                _ => return None,
            }
        }
        run(store, bytes, parts, out);
        used = at;
    }
    Some(used)
}

async fn serve(mut conn: TcpStream, store: Arc<Store>) -> io::Result<()> {
    let mut buffer = vec![0u8; FIRST_BUFFER];
    let mut length = 0;
    let mut out = Vec::new();
    let mut parts = Vec::new();
    loop {
        if length == buffer.len() {
            buffer.resize(buffer.len() * 2, 0);
        }
        let n = conn.read(&mut buffer[length..]).await?;
        if n == 0 {
            return Ok(());
        }
        length += n;
        out.clear();
        match serve_all(&store, &buffer[..length], &mut parts, &mut out) {
            None => {
                out.extend_from_slice(b"-ERR Protocol error\r\n");
                conn.write_all(&out).await?;
                return Ok(());
            }
            Some(used) => {
                if !out.is_empty() {
                    conn.write_all(&out).await?;
                }
                buffer.copy_within(used..length, 0);
                length -= used;
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let port: u16 = match args.get(1).and_then(|p| p.parse().ok()) {
        Some(port) if args.len() == 2 => port,
        _ => {
            eprintln!("usage: resp PORT");
            std::process::exit(2);
        }
    };
    let store: Arc<Store> = Arc::new((0..SHARDS).map(|_| Mutex::new(HashMap::new())).collect());
    let listener = match TcpListener::bind(("0.0.0.0", port)).await {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("resp: {e}");
            std::process::exit(1);
        }
    };
    loop {
        match listener.accept().await {
            Ok((conn, _)) => {
                let _ = conn.set_nodelay(true);
                // A connection that fails ends alone.
                tokio::spawn(serve(conn, store.clone()));
            }
            // Too many open files, say: try again soon.
            Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
        }
    }
}
