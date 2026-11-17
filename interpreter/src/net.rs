//! The network in the interpreter: what parses an address, binds a listener and connects,
//! and the wait for sockets (`poll(2)`, `WSAPoll` on Windows) that the scheduler does when no
//! fiber can run. What Windows does differently is in `sys::net`.
//!
//! Sockets do not block: a call that would wait gives the turn away, and the fiber is woken
//! when its socket is ready. What has to block (looking a name up, connecting, reading the
//! input of the program) runs on a thread of its own, which tells the scheduler through a
//! socket of its own when it is done.

use std::io;
#[cfg(unix)]
use std::net::ToSocketAddrs;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
#[cfg(unix)]
use std::os::unix::io::AsRawFd;
#[cfg(unix)]
use std::os::unix::net::UnixStream;
#[cfg(windows)]
use std::os::windows::io::AsRawSocket;

use crate::sys::RawFd;

/// What the scheduler hears a thread through: a pair of connected sockets of the machine
/// (`UnixStream`), and on Windows, which has none, a datagram socket of `127.0.0.1` connected to
/// itself.
#[cfg(unix)]
type Bell = UnixStream;
#[cfg(windows)]
type Bell = UdpSocket;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// `host:port`, `[v6]:port`, `:port`: the host and the port; `None` when the address is not
/// `host:port`, or the port is not a number up to 65535. An empty host stays empty.
fn split(address: &str) -> Option<(&str, u16)> {
    let (host, port) = address.rsplit_once(':')?;
    if port.is_empty() || port.len() > 5 || !port.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let port = u16::try_from(port.parse::<u32>().ok()?).ok()?;
    let host = match host.strip_prefix('[') {
        Some(inner) => inner.strip_suffix(']')?,
        // A v6 host is written in brackets: `::1:80` would not say where the port starts.
        None if host.contains(':') => return None,
        None => host,
    };
    Some((host, port))
}

/// The address a listener binds to: a number, `localhost`, or nothing for every address of
/// the machine. No name is looked up, so binding never waits for the network.
pub fn bind_address(address: &str) -> Option<SocketAddr> {
    let (host, port) = split(address)?;
    let ip = match host {
        "" => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        "localhost" => IpAddr::V4(Ipv4Addr::LOCALHOST),
        _ => host.parse::<IpAddr>().ok()?,
    };
    Some(SocketAddr::new(ip, port))
}

/// `TcpListener.bind(address)`: the listener does not block.
pub fn bind(at: SocketAddr) -> io::Result<TcpListener> {
    #[cfg(unix)]
    let listener = TcpListener::bind(at)?;
    #[cfg(windows)]
    let listener = crate::sys::net::listener(at)?;
    listener.set_nonblocking(true)?;
    Ok(listener)
}

/// What a connect gives: the stream, or why there is none. `None` as the error is a host
/// that could not be found, or an address that is not `host:port`.
pub type Connected = Result<TcpStream, Option<io::Error>>;

/// `TcpStream.connect(address)` on a thread of its own: the host is looked up, and each
/// address it has is tried in turn until one takes the connection. The stream does not block.
pub fn connect(address: &str) -> Connected {
    let (host, port) = split(address).ok_or(None)?;
    if host.is_empty() {
        return Err(None);
    }
    #[cfg(unix)]
    let addresses: Vec<SocketAddr> = (host, port).to_socket_addrs().map_err(|_| None)?.collect();
    #[cfg(windows)]
    let addresses = crate::sys::net::lookup(host, port).ok_or(None)?;
    let mut last = None;
    for at in addresses {
        match TcpStream::connect(at) {
            Ok(stream) => {
                stream.set_nonblocking(true).map_err(Some)?;
                return Ok(stream);
            }
            Err(error) => last = Some(error),
        }
    }
    Err(last)
}

/// An address as the language's own module gives it to the system: whether it is v6, the high and
/// the low 64 bits of the host (a v4 host is the last 32 of `low`), and the port.
pub fn address_of(v6: bool, high: u64, low: u64, port: u16) -> SocketAddr {
    match v6 {
        true => SocketAddr::new(IpAddr::V6(Ipv6Addr::from((u128::from(high) << 64) | u128::from(low))), port),
        false => SocketAddr::new(IpAddr::V4(Ipv4Addr::from(low as u32)), port),
    }
}

/// The four numbers of an address, as `address_of` takes them: v6 (1 or 0), high, low, port.
pub fn bits_of(at: SocketAddr) -> [u64; 4] {
    match at.ip() {
        IpAddr::V4(ip) => [0, 0, u64::from(u32::from(ip)), u64::from(at.port())],
        IpAddr::V6(ip) => {
            let bits = u128::from(ip);
            [1, (bits >> 64) as u64, bits as u64, u64::from(at.port())]
        }
    }
}

/// `System.udpBind`: a datagram socket that does not block.
pub fn udp_bind(at: SocketAddr) -> io::Result<UdpSocket> {
    #[cfg(unix)]
    let socket = UdpSocket::bind(at)?;
    #[cfg(windows)]
    let socket = crate::sys::net::udp(at)?;
    socket.set_nonblocking(true)?;
    Ok(socket)
}

/// `System.lookup`: the addresses of a host name, in the order the system gives them; `None`
/// when it has none.
pub fn lookup(host: &str) -> Option<Vec<IpAddr>> {
    #[cfg(unix)]
    let found: Vec<SocketAddr> = (host, 0).to_socket_addrs().ok()?.collect();
    #[cfg(windows)]
    let found = crate::sys::net::lookup(host, 0)?;
    Some(found.into_iter().map(|at| at.ip()).collect())
}

/// A connected datagram socket is told of a datagram that came back undelivered (Windows sets
/// this apart, see `sys::net`).
pub fn udp_connected(_socket: &UdpSocket) {
    #[cfg(windows)]
    crate::sys::net::udp_connected(_socket);
}

/// The error of a datagram socket, as Linux and macOS tell it.
pub fn udp_error(error: io::Error) -> io::Error {
    #[cfg(windows)]
    let error = crate::sys::net::udp_error(error);
    error
}

/// A finished piece of work of a thread, which the scheduler hands to the fiber that waits
/// for it.
pub struct Done {
    ids: Mutex<Vec<u64>>,
    tell: Mutex<Bell>,
}

/// The end of the scheduler: it waits on `hear` with the sockets, and finds in `done` which
/// work has ended.
pub struct Wake {
    pub hear: Bell,
    pub done: Arc<Done>,
}

/// The two ends of a bell: one waited on, one rung.
#[cfg(unix)]
fn bell() -> io::Result<(Bell, Bell)> {
    UnixStream::pair()
}

#[cfg(windows)]
fn bell() -> io::Result<(Bell, Bell)> {
    let hear = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))?;
    hear.connect(hear.local_addr()?)?;
    let tell = hear.try_clone()?;
    Ok((hear, tell))
}

impl Wake {
    pub fn new() -> io::Result<Wake> {
        let (hear, tell) = bell()?;
        hear.set_nonblocking(true)?;
        Ok(Wake { hear, done: Arc::new(Done { ids: Mutex::new(Vec::new()), tell: Mutex::new(tell) }) })
    }

    /// The work that has ended since the last time, in the order it ended.
    pub fn ended(&self) -> Vec<u64> {
        let mut drained = [0u8; 64];
        #[cfg(unix)]
        while matches!(io::Read::read(&mut &self.hear, &mut drained), Ok(n) if n > 0) {}
        #[cfg(windows)]
        while self.hear.recv(&mut drained).is_ok() {}
        std::mem::take(&mut *self.done.ids.lock().unwrap_or_else(|poisoned| poisoned.into_inner()))
    }
}

impl Done {
    /// Work `id` has ended: the scheduler hears of it.
    pub fn tell(&self, id: u64) {
        self.ids.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).push(id);
        let tell = self.tell.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        #[cfg(unix)]
        let _ = io::Write::write(&mut &*tell, &[1]);
        #[cfg(windows)]
        let _ = tell.send(&[1]);
    }
}

#[cfg(unix)]
#[repr(C)]
struct PollFd {
    fd: RawFd,
    events: i16,
    revents: i16,
}

#[cfg(unix)]
const POLLIN: i16 = 0x1;
#[cfg(unix)]
const POLLOUT: i16 = 0x4;

#[cfg(target_os = "linux")]
type Nfds = std::ffi::c_ulong;
#[cfg(all(unix, not(target_os = "linux")))]
type Nfds = std::ffi::c_uint;

#[cfg(unix)]
extern "C" {
    fn poll(fds: *mut PollFd, nfds: Nfds, timeout: std::ffi::c_int) -> std::ffi::c_int;
}

/// Waits until one of the sockets is ready (`true` to write), or `limit` has gone by; `None`
/// waits as long as it takes. Gives which are ready. A wait that a signal cut short gives
/// none, and is waited again by the caller.
#[cfg(windows)]
pub fn wait(sockets: &[(RawFd, bool)], limit: Option<Duration>) -> Vec<bool> {
    crate::sys::net::wait(sockets, limit)
}

#[cfg(unix)]
pub fn wait(sockets: &[(RawFd, bool)], limit: Option<Duration>) -> Vec<bool> {
    let mut fds: Vec<PollFd> = sockets.iter().map(|&(fd, write)| PollFd { fd, events: if write { POLLOUT } else { POLLIN }, revents: 0 }).collect();
    let timeout = match limit {
        // Rounded up: a wait that ends a little early would wake to find the timer not due.
        Some(limit) => limit.as_nanos().div_ceil(1_000_000).min(i32::MAX as u128) as std::ffi::c_int,
        None => -1,
    };
    // SAFETY: `fds` holds `fds.len()` initialised entries, and `poll` writes only `revents`.
    let count = unsafe { poll(fds.as_mut_ptr(), fds.len() as Nfds, timeout) };
    if count <= 0 {
        return vec![false; fds.len()];
    }
    fds.iter().map(|fd| fd.revents != 0).collect()
}

/// The descriptor a wait looks at.
#[cfg(unix)]
pub fn fd_of<T: AsRawFd>(socket: &T) -> RawFd {
    socket.as_raw_fd()
}

#[cfg(windows)]
pub fn fd_of<T: AsRawSocket>(socket: &T) -> RawFd {
    crate::sys::net::number_of(socket)
}
