//! Raw memory in the interpreter.
//!
//! A `raw T` is a real address: `Memory.allocate` takes memory from the allocator of the
//! system, and what lies there is laid out as C lays it out (`layout`), so that the same bytes
//! can be handed to C. Compiled code checks nothing about raw memory; the interpreter remembers
//! every block `Memory.allocate` gave, and each pointer carries the block it was made from, so
//! that a read outside the block, after `Memory.free`, or through `null` is a readable panic
//! instead of undefined behaviour. A pointer that was read back out of raw memory finds its
//! block again by its address; one that came from elsewhere is not checked.
//!
//! The memory comes from C's `calloc` and goes back with C's `free`, as in compiled code, so
//! that what C gave with `malloc` can be given back with `Memory.free`, and the other way round.

use std::collections::{BTreeMap, HashMap};
use std::ffi::c_void;
use std::sync::{LazyLock, Mutex};

use crate::types::{FloatTy, IntTy, Ty};

/// The alignment of every block: what `malloc` gives on the machines the language runs on.
const BLOCK_ALIGN: usize = 16;

extern "C" {
    fn calloc(count: usize, size: usize) -> *mut c_void;
    #[link_name = "free"]
    fn c_free(p: *mut c_void);
}

/// An address and the block it was made from, if the interpreter knows it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RawPtr {
    pub addr: usize,
    pub block: Option<u64>,
}

impl RawPtr {
    pub const NULL: RawPtr = RawPtr { addr: 0, block: None };

    /// `bytes` further on (or back, when negative); it stays in the block it was made from.
    pub fn offset(self, bytes: i128) -> RawPtr {
        RawPtr { addr: (self.addr as i128).wrapping_add(bytes) as usize, block: self.block }
    }
}

struct Block {
    base: usize,
    size: usize,
    live: bool,
}

#[derive(Default)]
struct Blocks {
    by_id: HashMap<u64, Block>,
    /// The blocks that are live, by where they start.
    live: BTreeMap<usize, u64>,
    next: u64,
}

/// Every block of every program run in this process; a block is never named again once it is
/// given back, so a pointer to it is told apart from one into a later block at the same place.
static BLOCKS: LazyLock<Mutex<Blocks>> = LazyLock::new(|| Mutex::new(Blocks::default()));

/// Why `Memory.allocate` gave nothing.
pub enum AllocError {
    /// More bytes than an address can count.
    TooLarge,
    /// The system has no more memory.
    OutOfMemory,
}

/// `count` items of `size` bytes, all zero.
pub fn allocate(count: usize, size: usize) -> Result<RawPtr, AllocError> {
    let bytes = count.checked_mul(size).filter(|bytes| *bytes <= isize::MAX as usize - BLOCK_ALIGN).ok_or(AllocError::TooLarge)?;
    // SAFETY: `calloc` takes any size; at least one byte is asked for, so that each block has
    // an address of its own.
    let base = unsafe { calloc(bytes.max(1), 1) } as usize;
    if base == 0 {
        return Err(AllocError::OutOfMemory);
    }
    let mut blocks = BLOCKS.lock().unwrap();
    let id = blocks.next;
    blocks.next += 1;
    blocks.by_id.insert(id, Block { base, size: bytes, live: true });
    blocks.live.insert(base, id);
    Ok(RawPtr { addr: base, block: Some(id) })
}

/// Why `Memory.free` could not give a pointer back.
pub enum FreeError {
    /// The block was given back before.
    Twice,
    /// The address lies inside a block, not where it starts.
    Inside,
}

/// Gives back the block that `p` is the start of; `null` is let be.
pub fn free(p: RawPtr) -> Result<(), FreeError> {
    if p.addr == 0 {
        return Ok(());
    }
    let mut blocks = BLOCKS.lock().unwrap();
    let id = match p.block.or_else(|| blocks.live.get(&p.addr).copied()) {
        Some(id) => id,
        // No block of `Memory.allocate` starts there: the memory is C's, given with `malloc`.
        None => {
            drop(blocks);
            // SAFETY: the program says, inside `unsafe`, that C gave this address.
            unsafe { c_free(p.addr as *mut c_void) };
            return Ok(());
        }
    };
    let block = blocks.by_id.get_mut(&id).expect("a pointer names a block that was made");
    if !block.live {
        return Err(FreeError::Twice);
    }
    if block.base != p.addr {
        return Err(FreeError::Inside);
    }
    block.live = false;
    let (base, size) = (block.base, block.size);
    blocks.live.remove(&base);
    let _ = size;
    // SAFETY: the block came from `calloc` and has not been given back.
    unsafe { c_free(base as *mut c_void) };
    Ok(())
}

/// What is wrong with reaching `size` bytes at a pointer.
pub enum Fault {
    Null,
    Freed,
    /// Outside the block: how many bytes from its start the access begins, and its size.
    Outside { at: i128, block: usize },
}

/// Whether `size` bytes can be reached at `p`.
pub fn check(p: RawPtr, size: usize) -> Result<(), Fault> {
    if p.addr == 0 {
        return Err(Fault::Null);
    }
    let Some(id) = p.block else { return Ok(()) };
    let blocks = BLOCKS.lock().unwrap();
    let block = &blocks.by_id[&id];
    if !block.live {
        return Err(Fault::Freed);
    }
    let at = p.addr as i128 - block.base as i128;
    if at < 0 || at + size as i128 > block.size as i128 {
        return Err(Fault::Outside { at, block: block.size });
    }
    Ok(())
}

/// How many bytes lie at `p` before a zero byte. In a block of `Memory.allocate` the zero byte
/// must come before the end of the block; elsewhere C said it is there.
pub fn text_length(p: RawPtr) -> Result<usize, Fault> {
    let Some(id) = p.block else {
        // SAFETY: the program says, inside `unsafe`, that a text C ended lies there.
        return Ok(unsafe { std::ffi::CStr::from_ptr(p.addr as *const std::ffi::c_char) }.to_bytes().len());
    };
    let blocks = BLOCKS.lock().unwrap();
    let block = &blocks.by_id[&id];
    if !block.live {
        return Err(Fault::Freed);
    }
    let at = p.addr as i128 - block.base as i128;
    if at < 0 || at >= block.size as i128 {
        return Err(Fault::Outside { at, block: block.size });
    }
    // SAFETY: the bytes from `p` to the end of the block are in the block, which is live.
    let rest = unsafe { std::slice::from_raw_parts(p.addr as *const u8, block.size - at as usize) };
    rest.iter().position(|byte| *byte == 0).ok_or(Fault::Outside { at: block.size as i128, block: block.size })
}

/// Memory lent to C for one call: a borrow or the items of a buffer, copied there and back.
/// Given back when it ends.
pub struct Scratch {
    base: usize,
}

impl Scratch {
    /// `size` bytes, all zero, aligned for any value.
    pub fn new(size: usize) -> Scratch {
        // SAFETY: at least one byte is asked for.
        let base = unsafe { calloc(size.max(1), 1) } as usize;
        assert!(base != 0, "no memory for a value lent to C");
        Scratch { base }
    }

    pub fn addr(&self) -> usize {
        self.base
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // SAFETY: the memory came from `calloc` and is given back once.
        unsafe { c_free(self.base as *mut c_void) };
    }
}

/// The live block an address lies in, for a pointer read back out of raw memory.
pub fn block_at(addr: usize) -> Option<u64> {
    if addr == 0 {
        return None;
    }
    let blocks = BLOCKS.lock().unwrap();
    let (_, id) = blocks.live.range(..=addr).next_back()?;
    let block = &blocks.by_id[id];
    (addr < block.base + block.size.max(1)).then_some(*id)
}

/// The size and the alignment of a value of plain data, as C has them. `plain` holds the types
/// of the fields of each struct of plain data.
pub fn layout(ty: &Ty, plain: &HashMap<String, Vec<Ty>>) -> (usize, usize) {
    match ty {
        Ty::Int(int) => {
            let bytes = (int.bits() / 8) as usize;
            (bytes, bytes)
        }
        Ty::Float(FloatTy::Float) => (4, 4),
        Ty::Float(FloatTy::Double) => (8, 8),
        Ty::Bool => (1, 1),
        Ty::Char => (4, 4),
        Ty::Raw(_) => (std::mem::size_of::<usize>(), std::mem::align_of::<usize>()),
        Ty::Array(item, length) => {
            let (size, align) = layout(item, plain);
            (size * length, align)
        }
        Ty::Struct(name) => {
            let (mut size, mut align) = (0usize, 1usize);
            for field in &plain[name] {
                let (field_size, field_align) = layout(field, plain);
                size = size.next_multiple_of(field_align) + field_size;
                align = align.max(field_align);
            }
            (size.next_multiple_of(align), align)
        }
        other => unreachable!("the checker lets a raw pointer point only at plain data, not at `{other}`"),
    }
}

/// Where each field of a struct of plain data lies, from its start.
pub fn offsets(name: &str, plain: &HashMap<String, Vec<Ty>>) -> Vec<usize> {
    let mut size = 0usize;
    plain[name]
        .iter()
        .map(|field| {
            let (field_size, field_align) = layout(field, plain);
            let at = size.next_multiple_of(field_align);
            size = at + field_size;
            at
        })
        .collect()
}

/// The integer of type `ty` that lies at `addr`.
///
/// # Safety
/// `addr` holds as many bytes as the type takes.
pub unsafe fn read_int(addr: usize, ty: IntTy) -> i128 {
    let at = addr as *const u8;
    match ty {
        IntTy::Sbyte => at.cast::<i8>().read_unaligned() as i128,
        IntTy::Byte => at.read_unaligned() as i128,
        IntTy::Short => at.cast::<i16>().read_unaligned() as i128,
        IntTy::Ushort => at.cast::<u16>().read_unaligned() as i128,
        IntTy::Int => at.cast::<i32>().read_unaligned() as i128,
        IntTy::Uint => at.cast::<u32>().read_unaligned() as i128,
        IntTy::Long | IntTy::Nint => at.cast::<i64>().read_unaligned() as i128,
        IntTy::Ulong | IntTy::Nuint => at.cast::<u64>().read_unaligned() as i128,
    }
}

/// Writes the integer `value` of type `ty` at `addr`.
///
/// # Safety
/// `addr` holds as many bytes as the type takes.
pub unsafe fn write_int(addr: usize, ty: IntTy, value: i128) {
    let at = addr as *mut u8;
    match ty {
        IntTy::Sbyte => at.cast::<i8>().write_unaligned(value as i8),
        IntTy::Byte => at.write_unaligned(value as u8),
        IntTy::Short => at.cast::<i16>().write_unaligned(value as i16),
        IntTy::Ushort => at.cast::<u16>().write_unaligned(value as u16),
        IntTy::Int => at.cast::<i32>().write_unaligned(value as i32),
        IntTy::Uint => at.cast::<u32>().write_unaligned(value as u32),
        IntTy::Long | IntTy::Nint => at.cast::<i64>().write_unaligned(value as i64),
        IntTy::Ulong | IntTy::Nuint => at.cast::<u64>().write_unaligned(value as u64),
    }
}

/// Copies `bytes` bytes from `source` to `target`; the two may overlap.
///
/// # Safety
/// Both hold `bytes` bytes.
pub unsafe fn copy(source: usize, target: usize, bytes: usize) {
    std::ptr::copy(source as *const u8, target as *mut u8, bytes);
}
