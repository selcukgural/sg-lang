//! The signals of the system on Windows, which come in the hundredth slice (Ctrl-C and the
//! closing of the console). Until then the checker refuses a program that listens for one, and
//! nothing here is reached.

use std::sync::Arc;

use crate::net::Done;

const LATER: &str = "signals come to Windows in the 100th slice, and the checker refuses them before";

pub fn listen(_mask: u32) -> std::io::Result<u64> {
    unreachable!("{LATER}")
}

pub fn stop(_id: u64) {
    unreachable!("{LATER}")
}

pub fn take_or_wait(_id: u64, _work: u64, _done: Arc<Done>) -> Option<u8> {
    unreachable!("{LATER}")
}

pub fn forget_waiter(_id: u64) {
    unreachable!("{LATER}")
}

pub fn raise_kind(_kind: usize) {
    unreachable!("{LATER}")
}

pub fn number(_kind: usize) -> i32 {
    unreachable!("{LATER}")
}
