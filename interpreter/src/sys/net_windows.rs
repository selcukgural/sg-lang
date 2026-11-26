//! The network of the interpreter on Windows: the sockets are std's, made here where the language
//! sets what std does not (a listener and a datagram socket that no other socket may take the port
//! of, `SO_EXCLUSIVEADDRUSE`; a datagram socket told of a datagram that came back only when it is
//! connected, as on Linux and macOS), the wait for sockets (`WSAPoll`), and a lookup of a name in
//! UTF-16 (`GetAddrInfoW`), as compiled code does them. Through the functions of Winsock declared
//! here, no crate.

use std::ffi::{c_char, c_int, c_void};
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV6, TcpListener, UdpSocket};
use std::os::windows::io::{AsRawSocket, FromRawSocket};
use std::sync::OnceLock;
use std::time::Duration;

type Socket = usize;

const INVALID_SOCKET: Socket = !0;
const AF_INET: c_int = 2;
const AF_INET6: c_int = 23;
const SOCK_STREAM: c_int = 1;
const SOCK_DGRAM: c_int = 2;
const IPPROTO_TCP: c_int = 6;
const IPPROTO_UDP: c_int = 17;
const SOL_SOCKET: c_int = 0xffff;
const IPPROTO_IPV6: c_int = 41;
const IPV6_V6ONLY: c_int = 27;
/// `~SO_REUSEADDR`.
const SO_EXCLUSIVEADDRUSE: c_int = !4;
const WSA_FLAG_OVERLAPPED: u32 = 0x01;
const WSA_FLAG_NO_HANDLE_INHERIT: u32 = 0x80;
const SOMAXCONN: c_int = 0x7fff_ffff;
/// `_WSAIOW(IOC_VENDOR, 12)` and `_WSAIOW(IOC_VENDOR, 15)`.
const SIO_UDP_CONNRESET: u32 = 0x9800_000C;
const SIO_UDP_NETRESET: u32 = 0x9800_000F;
const POLLRDNORM: i16 = 0x0100;
const POLLWRNORM: i16 = 0x0010;
const WSAECONNRESET: i32 = 10054;
const WSAECONNREFUSED: i32 = 10061;

#[repr(C)]
struct PollFd {
    fd: Socket,
    events: i16,
    revents: i16,
}

#[repr(C)]
struct AddrInfoW {
    flags: c_int,
    family: c_int,
    socktype: c_int,
    protocol: c_int,
    addrlen: usize,
    canonname: *mut u16,
    addr: *const u8,
    next: *mut AddrInfoW,
}

#[link(name = "ws2_32")]
extern "system" {
    fn WSAStartup(version: u16, data: *mut c_void) -> c_int;
    fn WSAGetLastError() -> c_int;
    fn WSASocketW(family: c_int, kind: c_int, protocol: c_int, info: *mut c_void, group: u32, flags: u32) -> Socket;
    fn setsockopt(s: Socket, level: c_int, name: c_int, value: *const c_char, len: c_int) -> c_int;
    fn bind(s: Socket, name: *const u8, len: c_int) -> c_int;
    fn listen(s: Socket, backlog: c_int) -> c_int;
    fn accept(s: Socket, address: *mut u8, len: *mut c_int) -> Socket;
    fn connect(s: Socket, name: *const u8, len: c_int) -> c_int;
    fn closesocket(s: Socket) -> c_int;
    fn ioctlsocket(s: Socket, command: i32, value: *mut u32) -> c_int;
    fn WSAIoctl(s: Socket, code: u32, input: *const c_void, in_len: u32, output: *mut c_void, out_len: u32, got: *mut u32, overlapped: *mut c_void, routine: *const c_void) -> c_int;
    fn WSAPoll(fds: *mut PollFd, count: u32, timeout: c_int) -> c_int;
    fn GetAddrInfoW(node: *const u16, service: *const u16, hints: *const AddrInfoW, result: *mut *mut AddrInfoW) -> c_int;
    fn FreeAddrInfoW(info: *mut AddrInfoW);
}

/// Winsock is started once, before the first socket made here (std starts it for its own).
fn start() {
    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| {
        // WSADATA is 408 bytes on 64-bit Windows.
        let mut data = [0u64; 64];
        // SAFETY: `data` is larger than the WSADATA it is written.
        unsafe { WSAStartup(0x0202, data.as_mut_ptr().cast()) };
    });
}

fn wsa_error() -> io::Error {
    // SAFETY: no argument; it reads the last error of this thread.
    io::Error::from_raw_os_error(unsafe { WSAGetLastError() })
}

/// An address as Winsock takes it, and its length.
fn sockaddr_of(at: SocketAddr) -> ([u8; 28], c_int) {
    let mut raw = [0u8; 28];
    raw[2..4].copy_from_slice(&at.port().to_be_bytes());
    match at {
        SocketAddr::V4(v4) => {
            raw[0..2].copy_from_slice(&(AF_INET as u16).to_le_bytes());
            raw[4..8].copy_from_slice(&v4.ip().octets());
            (raw, 16)
        }
        SocketAddr::V6(v6) => {
            raw[0..2].copy_from_slice(&(AF_INET6 as u16).to_le_bytes());
            raw[4..8].copy_from_slice(&v6.flowinfo().to_le_bytes());
            raw[8..24].copy_from_slice(&v6.ip().octets());
            raw[24..28].copy_from_slice(&v6.scope_id().to_le_bytes());
            (raw, 28)
        }
    }
}

/// The address Winsock wrote, if it is one of v4 or v6.
///
/// SAFETY: `raw` points to a `sockaddr` of `len` bytes.
unsafe fn address_from(raw: *const u8, len: usize) -> Option<SocketAddr> {
    let bytes = std::slice::from_raw_parts(raw, len);
    let family = u16::from_le_bytes([*bytes.first()?, *bytes.get(1)?]);
    let port = u16::from_be_bytes([*bytes.get(2)?, *bytes.get(3)?]);
    match c_int::from(family) {
        AF_INET if len >= 8 => Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(bytes[4], bytes[5], bytes[6], bytes[7])), port)),
        AF_INET6 if len >= 28 => {
            let ip: [u8; 16] = bytes[8..24].try_into().ok()?;
            let flow = u32::from_le_bytes(bytes[4..8].try_into().ok()?);
            let scope = u32::from_le_bytes(bytes[24..28].try_into().ok()?);
            Some(SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::from(ip), port, flow, scope)))
        }
        _ => None,
    }
}

/// A socket for `at` that is not handed to programs this one starts, and that no other socket may
/// take the port of.
fn exclusive(at: SocketAddr, kind: c_int, protocol: c_int) -> io::Result<Socket> {
    start();
    let family = if at.is_ipv4() { AF_INET } else { AF_INET6 };
    // SAFETY: plain values; a socket or `INVALID_SOCKET` comes back.
    let socket = unsafe { WSASocketW(family, kind, protocol, std::ptr::null_mut(), 0, WSA_FLAG_OVERLAPPED | WSA_FLAG_NO_HANDLE_INHERIT) };
    if socket == INVALID_SOCKET {
        return Err(wsa_error());
    }
    let one: c_int = 1;
    // SAFETY: `one` is a whole `int`, as the option takes.
    unsafe { setsockopt(socket, SOL_SOCKET, SO_EXCLUSIVEADDRUSE, (&one as *const c_int).cast(), 4) };
    // Every v6 address takes v4 too, as on Linux and macOS, where Windows would take v6 alone (1969).
    if at.is_ipv6() && at.ip().is_unspecified() {
        let zero: c_int = 0;
        // SAFETY: as above.
        unsafe { setsockopt(socket, IPPROTO_IPV6, IPV6_V6ONLY, (&zero as *const c_int).cast(), 4) };
    }
    Ok(socket)
}

/// `TcpListener.bind(address)`.
pub(crate) fn listener(at: SocketAddr) -> io::Result<TcpListener> {
    let socket = exclusive(at, SOCK_STREAM, IPPROTO_TCP)?;
    // SAFETY: the socket was just made, and is owned by the listener from here, which closes it.
    let listener = unsafe { TcpListener::from_raw_socket(socket as u64) };
    let (raw, len) = sockaddr_of(at);
    // SAFETY: `raw` holds an address of `len` bytes.
    if unsafe { bind(socket, raw.as_ptr(), len) } != 0 || unsafe { listen(socket, SOMAXCONN) } != 0 {
        return Err(wsa_error());
    }
    Ok(listener)
}

/// Whether Windows tells the next receive of `socket` of a datagram that came back as "port
/// unreachable" (`SIO_UDP_CONNRESET`), or as "time to live ran out" (`SIO_UDP_NETRESET`).
fn tell(socket: Socket, code: u32, on: bool) {
    let on = i32::from(on);
    let mut got = 0u32;
    // SAFETY: the input is a whole BOOL; no output, no overlapped call.
    unsafe { WSAIoctl(socket, code, (&on as *const i32).cast(), 4, std::ptr::null_mut(), 0, &mut got, std::ptr::null_mut(), std::ptr::null()) };
}

/// `System.udpBind`: a datagram socket that, as on Linux and macOS, is not told of a datagram
/// that came back while it is not connected.
pub(crate) fn udp(at: SocketAddr) -> io::Result<UdpSocket> {
    let socket = exclusive(at, SOCK_DGRAM, IPPROTO_UDP)?;
    // SAFETY: as in `listener`.
    let udp = unsafe { UdpSocket::from_raw_socket(socket as u64) };
    tell(socket, SIO_UDP_CONNRESET, false);
    tell(socket, SIO_UDP_NETRESET, false);
    let (raw, len) = sockaddr_of(at);
    // SAFETY: `raw` holds an address of `len` bytes.
    if unsafe { bind(socket, raw.as_ptr(), len) } != 0 {
        return Err(wsa_error());
    }
    Ok(udp)
}

/// A connected datagram socket is told of a datagram that came back, as a refused connection.
pub(crate) fn udp_connected(socket: &UdpSocket) {
    tell(socket.as_raw_socket() as Socket, SIO_UDP_CONNRESET, true);
}

/// Windows tells of a datagram that came back as a reset; Linux and macOS as a refused connection.
pub(crate) fn udp_error(error: io::Error) -> io::Error {
    match error.raw_os_error() {
        Some(WSAECONNRESET) => io::Error::from_raw_os_error(WSAECONNREFUSED),
        _ => error,
    }
}

/// The socket a wait looks at, as the number the interpreter keeps: Windows gives handles of 32
/// bits, which may be cut to that size and widened back.
pub(crate) fn number_of<T: AsRawSocket>(socket: &T) -> i32 {
    socket.as_raw_socket() as i32
}

/// The socket of a number `number_of` gave.
fn socket_of(number: i32) -> Socket {
    number as u32 as Socket
}

/// The datagram socket of a number `number_of` gave.
///
/// SAFETY: as `FromRawSocket`: the number is that of a socket open now, which the caller owns.
pub(crate) unsafe fn udp_of(number: i32) -> UdpSocket {
    UdpSocket::from_raw_socket(socket_of(number) as u64)
}

/// A socket of the machine at a path (`AF_UNIX`, Windows 10 1803 and later): a listener of
/// `UnixListener`, or what makes a connection of `UnixStream`, which std's `TcpStream` then holds, as
/// its calls (`recv`, `send`, `shutdown`) are those of any connection. Closed where it ends.
#[derive(Debug)]
pub(crate) struct UnixSocket(Socket);

const AF_UNIX: c_int = 1;
/// `FIONBIO`.
const NONBLOCKING: i32 = 0x8004_667E_u32 as i32;
const WSAEWOULDBLOCK: i32 = 10035;

impl UnixSocket {
    /// A socket of the machine and the address of `path` (`SOCKADDR_UN`: the family and the bytes of
    /// the path, ended by a zero byte; the path fits, see `net::unix_path`).
    fn made(path: &str) -> io::Result<(UnixSocket, [u8; 110], c_int)> {
        start();
        // SAFETY: plain values; a socket or `INVALID_SOCKET` comes back.
        let socket = unsafe { WSASocketW(AF_UNIX, SOCK_STREAM, 0, std::ptr::null_mut(), 0, WSA_FLAG_OVERLAPPED | WSA_FLAG_NO_HANDLE_INHERIT) };
        if socket == INVALID_SOCKET {
            return Err(wsa_error());
        }
        let mut raw = [0u8; 110];
        raw[0..2].copy_from_slice(&(AF_UNIX as u16).to_le_bytes());
        raw[2..2 + path.len()].copy_from_slice(path.as_bytes());
        Ok((UnixSocket(socket), raw, (2 + path.len() + 1) as c_int))
    }

    pub(crate) fn bind(path: &str) -> io::Result<UnixSocket> {
        let (socket, raw, len) = UnixSocket::made(path)?;
        // SAFETY: `raw` holds an address of `len` bytes.
        if unsafe { bind(socket.0, raw.as_ptr(), len) } != 0 || unsafe { listen(socket.0, SOMAXCONN) } != 0 {
            return Err(wsa_error());
        }
        Ok(socket)
    }

    /// A connection to the listener at `path`, which a listener with no room refuses.
    pub(crate) fn connect(path: &str) -> io::Result<std::net::TcpStream> {
        let (socket, raw, len) = UnixSocket::made(path)?;
        // SAFETY: as in `bind`.
        if unsafe { connect(socket.0, raw.as_ptr(), len) } != 0 {
            let error = wsa_error();
            // Windows refuses a path where nothing is; Linux and macOS say nothing is there.
            return Err(match error.raw_os_error() {
                Some(WSAECONNREFUSED) if matches!(std::fs::symlink_metadata(crate::names::os_of(path)), Err(e) if e.kind() == io::ErrorKind::NotFound) => io::Error::from_raw_os_error(2),
                _ => error,
            });
        }
        let number = socket.0;
        std::mem::forget(socket);
        // SAFETY: the socket was just made and connected, and is owned by the stream from here.
        Ok(unsafe { std::net::TcpStream::from_raw_socket(number as u64) })
    }

    pub(crate) fn set_nonblocking(&self, on: bool) -> io::Result<()> {
        let mut value = u32::from(on);
        // SAFETY: `value` is a whole `u_long` that lives across the call.
        match unsafe { ioctlsocket(self.0, NONBLOCKING, &mut value) } {
            0 => Ok(()),
            _ => Err(wsa_error()),
        }
    }

    /// The next connection, which does not block, or `WouldBlock` when none is there.
    pub(crate) fn accept(&self) -> io::Result<std::net::TcpStream> {
        // SAFETY: no address is asked for.
        let socket = unsafe { accept(self.0, std::ptr::null_mut(), std::ptr::null_mut()) };
        if socket == INVALID_SOCKET {
            let error = wsa_error();
            return Err(match error.raw_os_error() {
                Some(WSAEWOULDBLOCK) => io::Error::from(io::ErrorKind::WouldBlock),
                _ => error,
            });
        }
        // SAFETY: as in `connect`.
        let stream = unsafe { std::net::TcpStream::from_raw_socket(socket as u64) };
        stream.set_nonblocking(true)?;
        Ok(stream)
    }

    pub(crate) fn number(&self) -> i32 {
        self.0 as i32
    }
}

impl Drop for UnixSocket {
    fn drop(&mut self) {
        // SAFETY: the socket is this value's, closed once.
        unsafe { closesocket(self.0) };
    }
}

/// `stream.setKeepAlive(idle)` (1967), as on Linux and macOS: the idle time and the time between two
/// asks are both `ms`, rounded up to whole seconds as there; 0 turns it off (`SIO_KEEPALIVE_VALS`).
pub(crate) fn keep_alive(stream: &std::net::TcpStream, ms: u32) -> io::Result<()> {
    /// `_WSAIOW(IOC_VENDOR, 4)`.
    const SIO_KEEPALIVE_VALS: u32 = 0x9800_0004;
    let ms = ms.div_ceil(1000).saturating_mul(1000);
    // `struct tcp_keepalive { onoff, keepalivetime, keepaliveinterval }`, three `ULONG`s.
    let values: [u32; 3] = [u32::from(ms > 0), ms, ms];
    let mut got = 0u32;
    // SAFETY: the input is the 12 bytes of `values`, which live across the call; nothing is written back.
    let done = unsafe { WSAIoctl(stream.as_raw_socket() as Socket, SIO_KEEPALIVE_VALS, values.as_ptr().cast(), 12, std::ptr::null_mut(), 0, &mut got, std::ptr::null_mut(), std::ptr::null()) };
    match done {
        0 => Ok(()),
        _ => Err(wsa_error()),
    }
}

/// As `crate::net::wait`.
pub(crate) fn wait(sockets: &[(i32, bool)], limit: Option<Duration>) -> Vec<bool> {
    let mut fds: Vec<PollFd> = sockets.iter().map(|&(fd, write)| PollFd { fd: socket_of(fd), events: if write { POLLWRNORM } else { POLLRDNORM }, revents: 0 }).collect();
    let timeout = match limit {
        Some(limit) => limit.as_nanos().div_ceil(1_000_000).min(i32::MAX as u128) as c_int,
        None => -1,
    };
    // SAFETY: `fds` holds `fds.len()` initialised entries, and `WSAPoll` writes only `revents`.
    let count = unsafe { WSAPoll(fds.as_mut_ptr(), fds.len() as u32, timeout) };
    if count <= 0 {
        return vec![false; fds.len()];
    }
    fds.iter().map(|fd| fd.revents != 0).collect()
}

/// The addresses of `host`, with `port`, in the order the system gives them; `None` when it has
/// none. The name is given to the system in UTF-16, so one that is not ASCII is not turned
/// through the code page of the machine.
pub(crate) fn lookup(host: &str, port: u16) -> Option<Vec<SocketAddr>> {
    if host.contains('\0') {
        return None;
    }
    start();
    let node: Vec<u16> = host.encode_utf16().chain([0]).collect();
    let service: Vec<u16> = port.to_string().encode_utf16().chain([0]).collect();
    let hints = AddrInfoW { flags: 0, family: 0, socktype: SOCK_STREAM, protocol: 0, addrlen: 0, canonname: std::ptr::null_mut(), addr: std::ptr::null(), next: std::ptr::null_mut() };
    let mut list: *mut AddrInfoW = std::ptr::null_mut();
    // SAFETY: both names end with a zero; `list` is written only on success, and freed below.
    if unsafe { GetAddrInfoW(node.as_ptr(), service.as_ptr(), &hints, &mut list) } != 0 {
        return None;
    }
    let mut found = Vec::new();
    let mut at = list;
    while !at.is_null() {
        // SAFETY: `at` is an entry of the list the system gave, which is not freed yet.
        let entry = unsafe { &*at };
        if !entry.addr.is_null() {
            // SAFETY: the system gave the address with its length.
            if let Some(address) = unsafe { address_from(entry.addr, entry.addrlen) } {
                found.push(address);
            }
        }
        at = entry.next;
    }
    // SAFETY: the list the system gave, freed once.
    unsafe { FreeAddrInfoW(list) };
    Some(found)
}
