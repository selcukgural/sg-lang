//! `net`: the network measured against `std::net` and `tokio` (choices 1553, 1629-1637, 1678-1682).
//! Everything runs in one program, over the loopback: the server and its clients are tasks of it.
//! MODE is `async` (`tokio`, with the runtime of several threads `#[tokio::main]` makes) or
//! `blocking` (`std::net`, a thread for each task):
//!
//!     net echo MODE SIZE CLIENTS ROUNDS   CLIENTS connections, each sending SIZE bytes and waiting
//!                                          for them to come back, ROUNDS times
//!     net connect MODE COUNT               COUNT connections one after another, each sending a byte,
//!                                          taking it back and seeing the server close
//!     net stream MODE MEGABYTES BUFFER     MEGABYTES megabytes sent one way, BUFFER bytes at a time,
//!                                          and received BUFFER bytes at a time
//!     net lines MODE FILE                  FILE sent over a connection in pieces of 64 KB and read
//!                                          on the other side line by line
//!     net udp MODE SIZE CLIENTS ROUNDS     CLIENTS sockets, each sending datagrams of SIZE bytes to
//!                                          one that sends each back, ROUNDS times
//!     net address FILE                     each line of FILE read as an address with its port and
//!                                          written back into a text kept from one to the next
//!     net print FILE OUT                   the same, each address written into OUT on a line
//!
//! The clients of `echo` connect one after another and then all talk at once. The server closes each
//! connection once it has sent back all that a client sends, and the client sees it close, so the
//! wait of TCP after a close (TIME_WAIT) is on the server's side and the ports of the clients are
//! free again (choice 1684). What is printed is counted from what came back: the last byte of each
//! message is the number of its round, so the sums say that every message came back whole and in
//! order. A wrong use is told on the standard
//! error, with exit code 2; a failure of the network, with exit code 1.

use std::fmt::Write as _;
use std::io::{self, BufRead, BufReader, BufWriter, ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::process::exit;
use std::thread;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

const USAGE: &str = "usage: net echo MODE SIZE CLIENTS ROUNDS | connect MODE COUNT | stream MODE MEGABYTES BUFFER | lines MODE FILE | udp MODE SIZE CLIENTS ROUNDS | address FILE | print FILE OUT\n       MODE is async or blocking";

/// The bytes of `stream`: byte `i` of the buffer, sent again and again.
fn pattern(i: usize) -> u8 {
    ((i * 31 + 7) & 255) as u8
}

fn ended() -> io::Error {
    io::Error::from(ErrorKind::ConnectionReset)
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Runtime::new().expect("a runtime")
}

// ---- echo

fn echo_blocking(size: usize, clients: usize, rounds: usize) -> io::Result<i64> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let server = thread::spawn(move || -> io::Result<()> {
        let mut threads = Vec::new();
        for _ in 0..clients {
            let (mut conn, _) = listener.accept()?;
            threads.push(thread::spawn(move || -> io::Result<()> {
                let mut buffer = vec![0u8; size];
                let mut left = size * rounds;
                while left > 0 {
                    let n = conn.read(&mut buffer)?;
                    if n == 0 {
                        return Err(ended());
                    }
                    conn.write_all(&buffer[..n])?;
                    left -= n;
                }
                Ok(())
            }));
        }
        for t in threads {
            t.join().unwrap()?;
        }
        Ok(())
    });
    let mut asking = Vec::new();
    for _ in 0..clients {
        let mut conn = TcpStream::connect(address)?;
        asking.push(thread::spawn(move || -> io::Result<i64> {
            let mut message = vec![7u8; size];
            let mut back = vec![0u8; size];
            let mut sum = 0i64;
            for round in 0..rounds {
                message[size - 1] = (round & 255) as u8;
                conn.write_all(&message)?;
                let mut got = 0;
                while got < size {
                    let n = conn.read(&mut back[got..])?;
                    if n == 0 {
                        return Err(ended());
                    }
                    got += n;
                }
                sum += back[size - 1] as i64;
            }
            // The end: the server has let the connection go.
            if conn.read(&mut back)? != 0 {
                return Err(ended());
            }
            Ok(sum)
        }));
    }
    let mut sum = 0;
    for t in asking {
        sum += t.join().unwrap()?;
    }
    server.join().unwrap()?;
    Ok(sum)
}

async fn echo_async(size: usize, clients: usize, rounds: usize) -> io::Result<i64> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let server = tokio::spawn(async move {
        let mut tasks = Vec::new();
        for _ in 0..clients {
            let (mut conn, _) = listener.accept().await?;
            tasks.push(tokio::spawn(async move {
                let mut buffer = vec![0u8; size];
                let mut left = size * rounds;
                while left > 0 {
                    let n = conn.read(&mut buffer).await?;
                    if n == 0 {
                        return Err(ended());
                    }
                    conn.write_all(&buffer[..n]).await?;
                    left -= n;
                }
                Ok::<(), io::Error>(())
            }));
        }
        for t in tasks {
            t.await.unwrap()?;
        }
        Ok::<(), io::Error>(())
    });
    let mut asking = Vec::new();
    for _ in 0..clients {
        let mut conn = tokio::net::TcpStream::connect(address).await?;
        asking.push(tokio::spawn(async move {
            let mut message = vec![7u8; size];
            let mut back = vec![0u8; size];
            let mut sum = 0i64;
            for round in 0..rounds {
                message[size - 1] = (round & 255) as u8;
                conn.write_all(&message).await?;
                let mut got = 0;
                while got < size {
                    let n = conn.read(&mut back[got..]).await?;
                    if n == 0 {
                        return Err(ended());
                    }
                    got += n;
                }
                sum += back[size - 1] as i64;
            }
            if conn.read(&mut back).await? != 0 {
                return Err(ended());
            }
            Ok(sum)
        }));
    }
    let mut sum = 0;
    for t in asking {
        sum += t.await.unwrap()?;
    }
    server.await.unwrap()?;
    Ok(sum)
}

// ---- connect: the server takes a connection, sends its byte back and lets it go, so the wait of
// TCP after a close (TIME_WAIT) is on its side and the ports of the clients are free again.

fn connect_blocking(count: usize) -> io::Result<i64> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let server = thread::spawn(move || -> io::Result<()> {
        let mut buffer = [0u8; 1];
        for _ in 0..count {
            let (mut conn, _) = listener.accept()?;
            let n = conn.read(&mut buffer)?;
            conn.write_all(&buffer[..n])?;
        }
        Ok(())
    });
    let mut buffer = [0u8; 1];
    let mut bytes = 0i64;
    for _ in 0..count {
        let mut conn = TcpStream::connect(address)?;
        conn.write_all(&[42])?;
        bytes += conn.read(&mut buffer)? as i64;
        bytes += conn.read(&mut buffer)? as i64;
    }
    server.join().unwrap()?;
    Ok(bytes)
}

async fn connect_async(count: usize) -> io::Result<i64> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let server = tokio::spawn(async move {
        let mut buffer = [0u8; 1];
        for _ in 0..count {
            let (mut conn, _) = listener.accept().await?;
            let n = conn.read(&mut buffer).await?;
            conn.write_all(&buffer[..n]).await?;
        }
        Ok::<(), io::Error>(())
    });
    let mut buffer = [0u8; 1];
    let mut bytes = 0i64;
    for _ in 0..count {
        let mut conn = tokio::net::TcpStream::connect(address).await?;
        conn.write_all(&[42]).await?;
        bytes += conn.read(&mut buffer).await? as i64;
        bytes += conn.read(&mut buffer).await? as i64;
    }
    server.await.unwrap()?;
    Ok(bytes)
}

// ---- stream: what was received, and the sum of every 64th byte of it.

fn filled(size: usize) -> Vec<u8> {
    (0..size).map(pattern).collect()
}

/// Adds every 64th byte of the stream in `buffer`, which starts at byte `bytes` of it.
fn every_64th(buffer: &[u8], bytes: i64, sum: &mut i64) {
    let mut at = ((64 - bytes % 64) % 64) as usize;
    while at < buffer.len() {
        *sum += buffer[at] as i64;
        at += 64;
    }
}

fn stream_blocking(megabytes: usize, size: usize) -> io::Result<(i64, i64)> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let total = megabytes * 1048576;
    let sender = thread::spawn(move || -> io::Result<()> {
        let mut conn = TcpStream::connect(address)?;
        let buffer = filled(size);
        let mut sent = 0;
        while sent < total {
            conn.write_all(&buffer)?;
            sent += size;
        }
        Ok(())
    });
    let (mut conn, _) = listener.accept()?;
    let mut buffer = vec![0u8; size];
    let (mut bytes, mut sum) = (0i64, 0i64);
    loop {
        let n = conn.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        every_64th(&buffer[..n], bytes, &mut sum);
        bytes += n as i64;
    }
    sender.join().unwrap()?;
    Ok((bytes, sum))
}

async fn stream_async(megabytes: usize, size: usize) -> io::Result<(i64, i64)> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let total = megabytes * 1048576;
    let sender = tokio::spawn(async move {
        let mut conn = tokio::net::TcpStream::connect(address).await?;
        let buffer = filled(size);
        let mut sent = 0;
        while sent < total {
            conn.write_all(&buffer).await?;
            sent += size;
        }
        Ok::<(), io::Error>(())
    });
    let (mut conn, _) = listener.accept().await?;
    let mut buffer = vec![0u8; size];
    let (mut bytes, mut sum) = (0i64, 0i64);
    loop {
        let n = conn.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        every_64th(&buffer[..n], bytes, &mut sum);
        bytes += n as i64;
    }
    sender.await.unwrap()?;
    Ok((bytes, sum))
}

// ---- lines: the file is read whole first, then sent in pieces of 64 KB.

#[derive(Default)]
struct Lines {
    lines: i64,
    bytes: i64,
    longest: i64,
}

impl Lines {
    /// Counts a line read with its end, which is not counted (`\n` or `\r\n`).
    fn add(&mut self, line: &str) {
        let line = line.strip_suffix('\n').unwrap_or(line);
        let line = line.strip_suffix('\r').unwrap_or(line);
        self.lines += 1;
        let length = line.len() as i64;
        self.bytes += length;
        self.longest = self.longest.max(length);
    }
}

fn lines_blocking(text: Vec<u8>) -> io::Result<Lines> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let sender = thread::spawn(move || -> io::Result<()> {
        let mut conn = TcpStream::connect(address)?;
        for piece in text.chunks(65536) {
            conn.write_all(piece)?;
        }
        Ok(())
    });
    let (conn, _) = listener.accept()?;
    let mut reader = BufReader::new(conn);
    let mut line = String::new();
    let mut counts = Lines::default();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        counts.add(&line);
    }
    sender.join().unwrap()?;
    Ok(counts)
}

async fn lines_async(text: Vec<u8>) -> io::Result<Lines> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let sender = tokio::spawn(async move {
        let mut conn = tokio::net::TcpStream::connect(address).await?;
        for piece in text.chunks(65536) {
            conn.write_all(piece).await?;
        }
        Ok::<(), io::Error>(())
    });
    let (conn, _) = listener.accept().await?;
    let mut reader = tokio::io::BufReader::new(conn);
    let mut line = String::new();
    let mut counts = Lines::default();
    loop {
        line.clear();
        if reader.read_line(&mut line).await? == 0 {
            break;
        }
        counts.add(&line);
    }
    sender.await.unwrap()?;
    Ok(counts)
}

// ---- udp: one socket sends each datagram back where it came from.

fn udp_blocking(size: usize, clients: usize, rounds: usize) -> io::Result<i64> {
    let socket = UdpSocket::bind("127.0.0.1:0")?;
    let to = socket.local_addr()?;
    let count = clients * rounds;
    let server = thread::spawn(move || -> io::Result<()> {
        let mut buffer = vec![0u8; size];
        for _ in 0..count {
            let (n, from) = socket.recv_from(&mut buffer)?;
            socket.send_to(&buffer[..n], from)?;
        }
        Ok(())
    });
    let mut asking = Vec::new();
    for _ in 0..clients {
        asking.push(thread::spawn(move || -> io::Result<i64> {
            let socket = UdpSocket::bind("127.0.0.1:0")?;
            let mut message = vec![7u8; size];
            let mut back = vec![0u8; size];
            let mut sum = 0i64;
            for round in 0..rounds {
                message[size - 1] = (round & 255) as u8;
                socket.send_to(&message, to)?;
                let (n, _) = socket.recv_from(&mut back)?;
                sum += back[n - 1] as i64;
            }
            Ok(sum)
        }));
    }
    let mut sum = 0;
    for t in asking {
        sum += t.join().unwrap()?;
    }
    server.join().unwrap()?;
    Ok(sum)
}

async fn udp_async(size: usize, clients: usize, rounds: usize) -> io::Result<i64> {
    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await?;
    let to = socket.local_addr()?;
    let count = clients * rounds;
    let server = tokio::spawn(async move {
        let mut buffer = vec![0u8; size];
        for _ in 0..count {
            let (n, from) = socket.recv_from(&mut buffer).await?;
            socket.send_to(&buffer[..n], from).await?;
        }
        Ok::<(), io::Error>(())
    });
    let mut asking = Vec::new();
    for _ in 0..clients {
        asking.push(tokio::spawn(async move {
            let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await?;
            let mut message = vec![7u8; size];
            let mut back = vec![0u8; size];
            let mut sum = 0i64;
            for round in 0..rounds {
                message[size - 1] = (round & 255) as u8;
                socket.send_to(&message, to).await?;
                let (n, _) = socket.recv_from(&mut back).await?;
                sum += back[n - 1] as i64;
            }
            Ok::<i64, io::Error>(sum)
        }));
    }
    let mut sum = 0;
    for t in asking {
        sum += t.await.unwrap()?;
    }
    server.await.unwrap()?;
    Ok(sum)
}

// ---- addresses

fn addresses(path: &str) -> io::Result<()> {
    let mut reader = BufReader::new(std::fs::File::open(path)?);
    let mut line = String::new();
    let mut text = String::with_capacity(64);
    let (mut count, mut v6, mut ports, mut characters, mut invalid) = (0i64, 0i64, 0i64, 0i64, 0i64);
    let mut sum = 0u64;
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let trimmed = line.strip_suffix('\n').unwrap_or(&line);
        let trimmed = trimmed.strip_suffix('\r').unwrap_or(trimmed);
        match trimmed.parse::<SocketAddr>() {
            Ok(address) => {
                text.clear();
                write!(text, "{address}").unwrap();
                if address.is_ipv6() {
                    v6 += 1;
                }
                ports += address.port() as i64;
                characters += text.len() as i64;
                sum += text.as_bytes()[(count as usize) % text.len()] as u64;
                count += 1;
            }
            Err(_) => invalid += 1,
        }
    }
    println!("addresses {count}\nv6 {v6}\nports {ports}\ncharacters {characters}\nsum {sum}\ninvalid {invalid}");
    Ok(())
}

fn print_addresses(path: &str, out: &str) -> io::Result<()> {
    let mut reader = BufReader::new(std::fs::File::open(path)?);
    let mut writer = BufWriter::new(std::fs::File::create(out)?);
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let trimmed = line.strip_suffix('\n').unwrap_or(&line);
        let trimmed = trimmed.strip_suffix('\r').unwrap_or(trimmed);
        match trimmed.parse::<SocketAddr>() {
            Ok(address) => writeln!(writer, "{address}")?,
            Err(_) => writeln!(writer, "invalid")?,
        }
    }
    writer.flush()
}

// ---- the command line

fn usage() -> ! {
    eprintln!("{USAGE}");
    exit(2);
}

/// The number at `args[at]`, from 1 up to `most`; a wrong use when it is not one.
fn number(args: &[String], at: usize, most: usize) -> usize {
    match args[at].parse::<usize>() {
        Ok(value) if value >= 1 && value <= most => value,
        _ => usage(),
    }
}

fn fail(cause: io::Error) -> ! {
    eprintln!("net: {cause}");
    exit(1);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        usage();
    }
    let command = args[0].as_str();
    if command == "address" && args.len() == 2 {
        if let Err(cause) = addresses(&args[1]) {
            fail(cause);
        }
        return;
    }
    if command == "print" && args.len() == 3 {
        if let Err(cause) = print_addresses(&args[1], &args[2]) {
            fail(cause);
        }
        return;
    }
    let awaited = match args[1].as_str() {
        "async" => true,
        "blocking" => false,
        _ => usage(),
    };
    if (command == "echo" || command == "udp") && args.len() == 5 {
        let size = number(&args, 2, 65536);
        let clients = number(&args, 3, 100000);
        let rounds = number(&args, 4, 1000000000);
        if command == "udp" && size > 8192 {
            usage();
        }
        let sum = match (command, awaited) {
            ("echo", true) => runtime().block_on(echo_async(size, clients, rounds)),
            ("echo", false) => echo_blocking(size, clients, rounds),
            (_, true) => runtime().block_on(udp_async(size, clients, rounds)),
            (_, false) => udp_blocking(size, clients, rounds),
        };
        match sum {
            Ok(s) => {
                let messages = clients * rounds;
                let name = if command == "echo" { "connections" } else { "sockets" };
                println!("{name} {clients}\nmessages {messages}\nbytes {}\nsum {s}", messages * size);
            }
            Err(cause) => fail(cause),
        }
        return;
    }
    if command == "connect" && args.len() == 3 {
        let count = number(&args, 2, 1000000000);
        let bytes = if awaited { runtime().block_on(connect_async(count)) } else { connect_blocking(count) };
        match bytes {
            Ok(b) => println!("connections {count}\nbytes {b}"),
            Err(cause) => fail(cause),
        }
        return;
    }
    if command == "stream" && args.len() == 4 {
        let megabytes = number(&args, 2, 1000000);
        let size = number(&args, 3, 16777216);
        if 1048576 % size != 0 {
            usage();
        }
        let streamed =
            if awaited { runtime().block_on(stream_async(megabytes, size)) } else { stream_blocking(megabytes, size) };
        match streamed {
            Ok((bytes, sum)) => println!("bytes {bytes}\nsum {sum}"),
            Err(cause) => fail(cause),
        }
        return;
    }
    if command == "lines" && args.len() == 3 {
        let text = match std::fs::read(&args[2]) {
            Ok(text) => text,
            Err(cause) => {
                eprintln!("net: {}: {cause}", args[2]);
                exit(1);
            }
        };
        let counted = if awaited { runtime().block_on(lines_async(text)) } else { lines_blocking(text) };
        match counted {
            Ok(c) => println!("lines {}\nbytes {}\nlongest {}", c.lines, c.bytes, c.longest),
            Err(cause) => fail(cause),
        }
        return;
    }
    usage();
}
