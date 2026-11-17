//! What the interpreter asks of the operating system, one file for each kind of system: the rest
//! of the interpreter calls these functions and never the system itself, where the systems differ.

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub(crate) use unix::*;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(crate) use windows::*;
#[cfg(windows)]
#[path = "net_windows.rs"]
pub(crate) mod net;
