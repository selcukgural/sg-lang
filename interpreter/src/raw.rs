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

// Memory aligned beyond what `malloc` gives: with `posix_memalign`, which `free` gives back, and
// on Windows with `_aligned_malloc`, which only `_aligned_free` gives back. Either way only SG
// gives such a block back, by the type it was made for.
#[cfg(not(windows))]
extern "C" {
    fn posix_memalign(out: *mut *mut c_void, align: usize, size: usize) -> i32;
}
#[cfg(windows)]
extern "C" {
    fn _aligned_malloc(size: usize, align: usize) -> *mut c_void;
    fn _aligned_free(p: *mut c_void);
}

/// `bytes` bytes aligned to `align`, a power of two above `BLOCK_ALIGN`; null when there is no
/// memory. The bytes are not cleared.
fn aligned_alloc(bytes: usize, align: usize) -> *mut c_void {
    #[cfg(not(windows))]
    {
        let mut out = std::ptr::null_mut();
        // SAFETY: `align` is a power of two and a multiple of the size of a pointer.
        match unsafe { posix_memalign(&mut out, align, bytes) } {
            0 => out,
            _ => std::ptr::null_mut(),
        }
    }
    #[cfg(windows)]
    // SAFETY: `align` is a power of two.
    unsafe {
        _aligned_malloc(bytes, align)
    }
}

/// Gives back what `aligned_alloc` gave.
///
/// # Safety
/// `p` came from `aligned_alloc` and is given back once.
unsafe fn aligned_free(p: *mut c_void) {
    #[cfg(not(windows))]
    c_free(p);
    #[cfg(windows)]
    _aligned_free(p);
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
    /// The alignment it was made with, when that is more than `BLOCK_ALIGN`.
    aligned: Option<usize>,
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

/// `count` items of `size` bytes and alignment `align`, all zero.
pub fn allocate(count: usize, size: usize, align: usize) -> Result<RawPtr, AllocError> {
    let bytes = count.checked_mul(size).filter(|bytes| *bytes <= isize::MAX as usize - BLOCK_ALIGN.max(align)).ok_or(AllocError::TooLarge)?;
    let aligned = (align > BLOCK_ALIGN).then_some(align);
    let base = match aligned {
        // SAFETY: `calloc` takes any size; at least one byte is asked for, so that each block
        // has an address of its own.
        None => (unsafe { calloc(bytes.max(1), 1) }) as usize,
        Some(align) => {
            let base = aligned_alloc(bytes.max(1), align);
            if !base.is_null() {
                // SAFETY: the block holds at least this many bytes.
                unsafe { std::ptr::write_bytes(base.cast::<u8>(), 0, bytes.max(1)) };
            }
            base as usize
        }
    };
    if base == 0 {
        return Err(AllocError::OutOfMemory);
    }
    let mut blocks = BLOCKS.lock().unwrap();
    let id = blocks.next;
    blocks.next += 1;
    blocks.by_id.insert(id, Block { base, size: bytes, live: true, aligned });
    blocks.live.insert(base, id);
    Ok(RawPtr { addr: base, block: Some(id) })
}

/// Why `Memory.free` could not give a pointer back.
pub enum FreeError {
    /// The block was given back before.
    Twice,
    /// The address lies inside a block, not where it starts.
    Inside,
    /// The block was made for a type aligned beyond `BLOCK_ALIGN` and is given back through a
    /// pointer to one that is not, or the other way round: compiled code on Windows gives the two
    /// back in two ways. The alignment it was made with, if beyond `BLOCK_ALIGN`.
    Aligned { made: Option<usize> },
}

/// Gives back the block that `p` is the start of; `null` is let be. `align` is the alignment of
/// the type `p` points at.
pub fn free(p: RawPtr, align: usize) -> Result<(), FreeError> {
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
    let given = (align > BLOCK_ALIGN).then_some(align);
    if block.aligned != given {
        return Err(FreeError::Aligned { made: block.aligned });
    }
    block.live = false;
    let base = block.base;
    blocks.live.remove(&base);
    // SAFETY: the block came from `calloc` or `aligned_alloc`, and has not been given back.
    unsafe {
        match given {
            None => c_free(base as *mut c_void),
            Some(_) => aligned_free(base as *mut c_void),
        }
    }
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

/// How C lays out a struct or a union of plain data, beyond the types of its fields.
#[derive(Clone, Debug, Default)]
pub struct Plain {
    /// The types of the fields, in the order they are declared.
    pub fields: Vec<Ty>,
    /// For each field, its width in bits when it is a bit field (`uint version : 4`).
    pub bits: Vec<Option<u32>>,
    /// Bits left empty (`uint _ : 4`): before which field, of which type, how many.
    pub gaps: Vec<(usize, Ty, u32)>,
    /// `union`: every field starts where the value starts.
    pub union: bool,
    /// `packed`: no padding, an alignment of 1.
    pub packed: bool,
    /// `align(N)`: at least this alignment; 0 when none is written.
    pub align: usize,
}

/// The structs and unions of plain data of a program, by full name.
pub type PlainTypes = HashMap<String, Plain>;

/// The rules by which a C compiler lays out bit fields. The rest of a layout is the same on
/// every machine the language runs on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Abi {
    /// macOS, and Linux on x86-64: a bit field goes where the bits before it end, unless it
    /// would cross a unit of its type; bits left empty do not change the alignment.
    Unix,
    /// Linux on 64-bit Arm (AAPCS64): as `Unix`, but bits left empty count for the alignment of
    /// the struct as a field of their type does.
    Arm,
    /// Windows, with every compiler there: a bit field starts a new unit of its type unless the
    /// one before it has the same size and room left.
    Microsoft,
}

/// The rules of the machine the interpreter runs on, which compiled code meets in C.
pub const HOST: Abi = if cfg!(windows) {
    Abi::Microsoft
} else if cfg!(all(target_arch = "aarch64", not(target_vendor = "apple"))) {
    Abi::Arm
} else {
    Abi::Unix
};

/// Where a field lies in the value that holds it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Place {
    /// At this many bytes from the start.
    Bytes(usize),
    /// A bit field: `width` bits from bit `at` of the value, counted from the lowest bit of its
    /// first byte (the machines the language runs on are little-endian).
    Bits { at: usize, width: u32 },
}

impl Place {
    /// The byte the field starts in.
    pub fn byte(self) -> usize {
        match self {
            Place::Bytes(at) => at,
            Place::Bits { at, .. } => at / 8,
        }
    }
}

/// The size, the alignment and where each field lies, of a struct or a union of plain data.
pub struct Shape {
    pub size: usize,
    pub align: usize,
    pub places: Vec<Place>,
}

/// The size and the alignment of a value of plain data, as C has them on this machine.
pub fn layout(ty: &Ty, plain: &PlainTypes) -> (usize, usize) {
    layout_for(ty, plain, HOST)
}

/// The size and the alignment of a value of plain data under the rules of `abi`.
pub fn layout_for(ty: &Ty, plain: &PlainTypes, abi: Abi) -> (usize, usize) {
    match ty {
        // C's `long` is 32 bits on Windows (LLP64) and 64 elsewhere (LP64).
        Ty::Int(int) if int.is_c() => match abi {
            Abi::Microsoft => (4, 4),
            Abi::Unix | Abi::Arm => (8, 8),
        },
        Ty::Int(int) => {
            let bytes = (int.bits() / 8) as usize;
            (bytes, bytes)
        }
        Ty::Float(FloatTy::Float) => (4, 4),
        Ty::Float(FloatTy::Double) => (8, 8),
        Ty::Bool => (1, 1),
        Ty::Char => (4, 4),
        Ty::Raw(_) | Ty::CFunc(..) => (std::mem::size_of::<usize>(), std::mem::align_of::<usize>()),
        // An atomic value is aligned to its size, also where its type is aligned to less (a
        // `long` on 32-bit x86), as C11 and Rust lay it out.
        Ty::Atomic(inner) => {
            let (size, _) = layout_for(inner, plain, abi);
            (size, size)
        }
        Ty::Array(item, length) => {
            let (size, align) = layout_for(item, plain, abi);
            (size * length, align)
        }
        Ty::Struct(name) => {
            let shape = shape_for(name, plain, abi);
            (shape.size, shape.align)
        }
        other => unreachable!("the checker lets a raw pointer point only at plain data, not at `{other}`"),
    }
}

/// Where each field of a struct or a union of plain data lies, on this machine.
pub fn places(name: &str, plain: &PlainTypes) -> Vec<Place> {
    shape_for(name, plain, HOST).places
}

/// The layout of a struct or a union of plain data under the rules of `abi`.
pub fn shape_for(name: &str, plain: &PlainTypes, abi: Abi) -> Shape {
    let decl = &plain[name];
    // The fields and the bits left empty, in the order C sees them: `None` for a gap.
    let mut items: Vec<(Option<usize>, &Ty, Option<u32>)> = Vec::new();
    for (index, field) in decl.fields.iter().enumerate() {
        items.extend(decl.gaps.iter().filter(|gap| gap.0 == index).map(|gap| (None, &gap.1, Some(gap.2))));
        items.push((Some(index), field, decl.bits[index]));
    }
    items.extend(decl.gaps.iter().filter(|gap| gap.0 == decl.fields.len()).map(|gap| (None, &gap.1, Some(gap.2))));
    let mut places = vec![Place::Bytes(0); decl.fields.len()];
    let mut align = 1usize;
    // Bits used so far.
    let mut end = 0usize;
    // Microsoft: the unit a bit field was put in, by its first bit, its size in bits and the
    // bits it has used.
    let mut unit: Option<(usize, usize, usize)> = None;
    for (index, ty, width) in items {
        let (size, natural) = layout_for(ty, plain, abi);
        let type_align = if decl.packed { 1 } else { natural };
        let at = match width {
            _ if decl.union => {
                align = align.max(type_align);
                end = end.max(size * 8);
                match width {
                    Some(width) => Place::Bits { at: 0, width },
                    None => Place::Bytes(0),
                }
            }
            None => {
                unit = None;
                let at = end.next_multiple_of(type_align * 8);
                end = at + size * 8;
                align = align.max(type_align);
                Place::Bytes(at / 8)
            }
            Some(width) => {
                let width = width as usize;
                match abi {
                    Abi::Unix | Abi::Arm => {
                        let unit_bits = size * 8;
                        let at = if width == 0 || (end % unit_bits) + width > unit_bits { end.next_multiple_of(type_align * 8) } else { end };
                        end = at + width;
                        if index.is_some() || abi == Abi::Arm {
                            align = align.max(type_align);
                        }
                        Place::Bits { at, width: width as u32 }
                    }
                    Abi::Microsoft => {
                        let at = match unit {
                            // A gap of no bits closes the unit before it, and is let be where
                            // no bit field comes before it.
                            _ if width == 0 => {
                                if unit.take().is_some() {
                                    end = end.next_multiple_of(type_align * 8);
                                    align = align.max(type_align);
                                }
                                end
                            }
                            Some((start, bits, used)) if bits == size * 8 && used + width <= bits => {
                                unit = Some((start, bits, used + width));
                                start + used
                            }
                            _ => {
                                let start = end.next_multiple_of(type_align * 8);
                                end = start + size * 8;
                                unit = Some((start, size * 8, width));
                                align = align.max(type_align);
                                start
                            }
                        };
                        Place::Bits { at, width: width as u32 }
                    }
                }
            }
        };
        if let Some(index) = index {
            places[index] = at;
        }
    }
    let align = align.max(decl.align);
    // A struct with no fields holds one byte (`c_definition`), as in C++, so that C's compiler
    // and C++'s lay it out alike and no value has the size 0 (choice 1335).
    let end = end.max(8);
    Shape { size: end.div_ceil(8).next_multiple_of(align), align, places }
}

/// The definition of a struct or a union of plain data in C, a type named `name` (a `typedef`
/// of the tag of that name), as the compiler of C lays it out
/// on every machine: `name` is its C name, `c_type` the C type of a field (an array gives the type
/// of its items), `member` the C name of the field at an index. `SG_ALIGNED` is defined where
/// this is used: `__declspec(align(n))` with Microsoft's compiler, `__attribute__((aligned(n)))`
/// with the others.
pub fn c_definition(name: &str, decl: &Plain, c_type: &dyn Fn(&Ty) -> String, member: &dyn Fn(usize) -> String) -> String {
    let mut members = String::new();
    let gap = |gap: &(usize, Ty, u32)| format!(" {} : {};", c_type(&gap.1), gap.2);
    for (index, field) in decl.fields.iter().enumerate() {
        members.extend(decl.gaps.iter().filter(|found| found.0 == index).map(gap));
        let named = member(index);
        members.push_str(&match (field, decl.bits[index]) {
            (_, Some(width)) => format!(" {} {named} : {width};", c_type(field)),
            // An atomic value is aligned to its size, where its type may be aligned to less.
            (Ty::Array(item, length), None) if matches!(**item, Ty::Atomic(_)) => {
                let ctype = c_type(item);
                format!(" _Alignas(sizeof({ctype})) {ctype} {named}[{length}];")
            }
            (Ty::Atomic(_), None) => {
                let ctype = c_type(field);
                format!(" _Alignas(sizeof({ctype})) {ctype} {named};")
            }
            (Ty::Array(item, length), None) => format!(" {} {named}[{length}];", c_type(item)),
            (other, None) => format!(" {} {named};", c_type(other)),
        });
    }
    members.extend(decl.gaps.iter().filter(|found| found.0 == decl.fields.len()).map(gap));
    // A struct with no fields, which C does not have and C++ gives one byte, holds one byte.
    if members.is_empty() {
        members.push_str(" char sg_empty;");
    }
    let kind = if decl.union { "union" } else { "struct" };
    let aligned = if decl.align > 0 { format!(" SG_ALIGNED({})", decl.align) } else { String::new() };
    let text = format!("typedef {kind}{aligned} {name} {{{members} }} {name};\n");
    match decl.packed {
        true => format!("#pragma pack(push, 1)\n{text}#pragma pack(pop)\n"),
        false => text,
    }
}

/// What a C file that holds `c_definition`s needs before them.
pub const C_ALIGNED: &str = "#ifndef SG_ALIGNED\n#ifdef _MSC_VER\n#define SG_ALIGNED(n) __declspec(align(n))\n#else\n#define SG_ALIGNED(n) __attribute__((aligned(n)))\n#endif\n#endif\n";

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
        IntTy::CLong => at.cast::<std::ffi::c_long>().read_unaligned() as i128,
        IntTy::CULong => at.cast::<std::ffi::c_ulong>().read_unaligned() as i128,
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
        IntTy::CLong => at.cast::<std::ffi::c_long>().write_unaligned(value as std::ffi::c_long),
        IntTy::CULong => at.cast::<std::ffi::c_ulong>().write_unaligned(value as std::ffi::c_ulong),
    }
}

/// Copies `bytes` bytes from `source` to `target`; the two may overlap.
///
/// # Safety
/// Both hold `bytes` bytes.
pub unsafe fn copy(source: usize, target: usize, bytes: usize) {
    std::ptr::copy(source as *const u8, target as *mut u8, bytes);
}
