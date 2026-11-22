//! The input of the program in the interpreter, read through a block of its own.
//!
//! An async read of an input that can make the program wait (a pipe, a terminal) does not
//! hold the run: a thread reads once into the block of the input while the fiber waits, and a
//! cancel ends the wait without losing what the thread reads, which stays in the block for the
//! next read. Only one such thread is ever out at a time.

use std::io::{self, BufRead, Read};
use std::sync::{Arc, Mutex, MutexGuard};

/// What has been read and not given out yet, shared with the thread that fills it.
#[derive(Default)]
struct Ahead {
    bytes: Vec<u8>,
    /// The thread met the end of the input.
    end: bool,
    /// The thread met an error: the next read meets it too.
    failed: bool,
    /// The work of the thread that is reading now, if one is.
    filling: Option<u64>,
}

struct Shared {
    source: Mutex<Box<dyn Read + Send>>,
    ahead: Mutex<Ahead>,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// What an async read needs before it can be done without waiting.
#[derive(Clone, Copy)]
pub enum Need {
    Line,
    Bytes(usize),
    End,
    /// Anything: `readSome`.
    Some,
    /// More than the block holds now, which held this many: `fill`.
    More(usize),
}

pub struct Input {
    shared: Arc<Shared>,
    block: Vec<u8>,
    at: usize,
    /// The input can make the program wait: a pipe, a terminal, a socket.
    pub waits: bool,
}

impl Input {
    pub fn new(source: Box<dyn Read + Send>, waits: bool) -> Input {
        let shared = Arc::new(Shared { source: Mutex::new(source), ahead: Mutex::new(Ahead::default()) });
        Input { shared, block: Vec::new(), at: 0, waits }
    }

    /// What the thread has read goes behind what the block holds.
    fn take_ahead(&mut self) -> bool {
        let mut ahead = locked(&self.shared.ahead);
        if ahead.bytes.is_empty() {
            return false;
        }
        if self.at == self.block.len() {
            self.block.clear();
            self.at = 0;
        }
        self.block.append(&mut ahead.bytes);
        true
    }

    /// Whether a read that needs `need` can be done now without waiting for the input.
    pub fn ready(&mut self, need: Need) -> bool {
        self.take_ahead();
        let ahead = locked(&self.shared.ahead);
        if ahead.end || ahead.failed {
            return ahead.filling.is_none();
        }
        let held = &self.block[self.at..];
        match need {
            Need::Line => held.contains(&b'\n'),
            Need::Bytes(count) => held.len() >= count,
            Need::End => false,
            Need::Some => !held.is_empty(),
            Need::More(base) => held.len() > base,
        }
    }

    /// The thread that reads once into the block: the work it is (`id` when a new one is
    /// needed), and what to run on a thread of its own when it is not out already. `None` when
    /// a thread has read something since `ready` looked: it is looked at again.
    pub fn fill(&mut self, id: u64) -> Option<(u64, Option<impl FnOnce() + Send + 'static>)> {
        let mut ahead = locked(&self.shared.ahead);
        if let Some(out) = ahead.filling {
            return Some((out, None));
        }
        if !ahead.bytes.is_empty() || ahead.end || ahead.failed {
            return None;
        }
        ahead.filling = Some(id);
        let shared = self.shared.clone();
        let work = move || {
            let mut bytes = vec![0u8; 64 * 1024];
            let read = loop {
                match locked(&shared.source).read(&mut bytes) {
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                    other => break other,
                }
            };
            let mut ahead = locked(&shared.ahead);
            match read {
                Ok(0) => ahead.end = true,
                Ok(count) => ahead.bytes.extend_from_slice(&bytes[..count]),
                Err(_) => ahead.failed = true,
            }
            ahead.filling = None;
        };
        Some((id, Some(work)))
    }
}

impl Input {
    /// What the block holds and has not been given out.
    pub fn buffer(&self) -> &[u8] {
        &self.block[self.at..]
    }

    /// `fill`: one more read behind what the block holds, which held `base` bytes when the
    /// call began; what a thread read since then counts as that read. How many bytes came, `0`
    /// at the end, or `None` when the block holds `limit` bytes already.
    pub fn more(&mut self, base: usize, limit: usize) -> io::Result<Option<usize>> {
        self.take_ahead();
        let grown = |input: &Input| input.block.len() - input.at > base;
        if grown(self) {
            return Ok(Some(self.block.len() - self.at - base));
        }
        let shared = self.shared.clone();
        let mut source = locked(&shared.source);
        if self.take_ahead() && grown(self) {
            return Ok(Some(self.block.len() - self.at - base));
        }
        let mut ahead = locked(&self.shared.ahead);
        let end = std::mem::take(&mut ahead.end);
        ahead.failed = false;
        drop(ahead);
        if end {
            return Ok(Some(0));
        }
        self.block.drain(..self.at);
        self.at = 0;
        let held = self.block.len();
        if held >= limit {
            return Ok(None);
        }
        self.block.resize(limit, 0);
        let read = loop {
            match source.read(&mut self.block[held..]) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                other => break other,
            }
        };
        self.block.truncate(held + *read.as_ref().unwrap_or(&0));
        read.map(Some)
    }
}

impl Read for Input {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let held = self.fill_buf()?;
        let count = held.len().min(out.len());
        out[..count].copy_from_slice(&held[..count]);
        self.consume(count);
        Ok(count)
    }
}

impl BufRead for Input {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if self.at == self.block.len() && !self.take_ahead() {
            // A thread that is reading holds the source: what it reads comes first.
            let shared = self.shared.clone();
            let mut source = locked(&shared.source);
            if !self.take_ahead() {
                // The end the thread met is given once; after an error it met, the input is
                // read again, which meets the error again or goes on.
                let mut ahead = locked(&self.shared.ahead);
                let end = std::mem::take(&mut ahead.end);
                ahead.failed = false;
                drop(ahead);
                if !end {
                    self.block.resize(64 * 1024, 0);
                    self.at = 0;
                    let count = loop {
                        match source.read(&mut self.block) {
                            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                            Err(error) => {
                                self.block.clear();
                                return Err(error);
                            }
                            Ok(count) => break count,
                        }
                    };
                    self.block.truncate(count);
                }
            }
        }
        Ok(&self.block[self.at..])
    }

    fn consume(&mut self, count: usize) {
        self.at = (self.at + count).min(self.block.len());
    }
}
