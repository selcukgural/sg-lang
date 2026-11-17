//! The network in the interpreter: what parses an address, binds a listener and connects,
//! and the wait for sockets (`poll(2)`) that the scheduler does when no fiber can run.
//!
//! Sockets do not block: a call that would wait gives the turn away, and the fiber is woken
//! when its socket is ready. What has to block (looking a name up, connecting, reading the
//! input of the program) runs on a thread of its own, which tells the scheduler through a
//! socket of its own when it is done.

use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
#[cfg(unix)]
use std::os::unix::io::AsRawFd;
#[cfg(unix)]
use std::os::unix::net::UnixStream;

use crate::sys::RawFd;

/// What the scheduler hears a thread through: a pair of sockets. Windows has the scheduler's
/// wait in a later slice, and nothing that needs it runs there before.
#[cfg(windows)]
pub struct UnixStream;
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
    let listener = TcpListener::bind(at)?;
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
    let addresses: Vec<SocketAddr> = (host, port).to_socket_addrs().map_err(|_| None)?.collect();
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
    let socket = UdpSocket::bind(at)?;
    socket.set_nonblocking(true)?;
    Ok(socket)
}

/// `System.lookup`: the addresses of a host name, in the order the system gives them; `None`
/// when it has none.
pub fn lookup(host: &str) -> Option<Vec<IpAddr>> {
    let found = (host, 0).to_socket_addrs().ok()?;
    Some(found.map(|at| at.ip()).collect())
}

/// A finished piece of work of a thread, which the scheduler hands to the fiber that waits
/// for it.
pub struct Done {
    ids: Mutex<Vec<u64>>,
    #[cfg_attr(windows, allow(dead_code))]
    tell: Mutex<UnixStream>,
}

/// The end of the scheduler: it waits on `hear` with the sockets, and finds in `done` which
/// work has ended.
pub struct Wake {
    pub hear: UnixStream,
    pub done: Arc<Done>,
}

impl Wake {
    #[cfg(windows)]
    pub fn new() -> io::Result<Wake> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "tasks come to Windows in the 98th slice"))
    }

    #[cfg(unix)]
    pub fn new() -> io::Result<Wake> {
        let (hear, tell) = UnixStream::pair()?;
        hear.set_nonblocking(true)?;
        Ok(Wake { hear, done: Arc::new(Done { ids: Mutex::new(Vec::new()), tell: Mutex::new(tell) }) })
    }

    /// The work that has ended since the last time, in the order it ended.
    #[cfg(windows)]
    pub fn ended(&self) -> Vec<u64> {
        std::mem::take(&mut *self.done.ids.lock().unwrap_or_else(|poisoned| poisoned.into_inner()))
    }

    /// The work that has ended since the last time, in the order it ended.
    #[cfg(unix)]
    pub fn ended(&self) -> Vec<u64> {
        let mut drained = [0u8; 64];
        while matches!(io::Read::read(&mut &self.hear, &mut drained), Ok(n) if n > 0) {}
        std::mem::take(&mut *self.done.ids.lock().unwrap_or_else(|poisoned| poisoned.into_inner()))
    }
}

impl Done {
    /// Work `id` has ended: the scheduler hears of it.
    pub fn tell(&self, id: u64) {
        self.ids.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).push(id);
        #[cfg(unix)]
        let _ = io::Write::write(&mut &*self.tell.lock().unwrap_or_else(|poisoned| poisoned.into_inner()), &[1]);
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
pub fn wait(_sockets: &[(RawFd, bool)], _limit: Option<Duration>) -> Vec<bool> {
    unreachable!("tasks come to Windows in the 98th slice, and the checker refuses them before")
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
pub fn fd_of<T>(_socket: &T) -> RawFd {
    unreachable!("the network comes to Windows in the 99th slice, and the checker refuses it before")
}
