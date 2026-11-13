# Memory model

**Dropped.** On 5 November 2026 Rust's ownership rules were chosen instead of the generation model. This file is kept for the record; the current state is in [decisions and open questions](04-decisions-and-open-questions.md).

The first model answered the three questions that a runtime check for [use-after-free](../topics/07-use-after-free.md) leaves open: pointer size, stack references and error behaviour. The starting points were that compatibility with C does not matter, performance does, and an error is a panic. Its known gaps are listed in [decisions and open questions](04-decisions-and-open-questions.md) ("Gaps in the first memory model").

**1. Pointer size: 8 bytes, generation in the upper bits.** Since C compatibility is not needed, there is no need for Vale's 16-byte fat pointer. x86-64 and ARM64 use 48-bit addresses in practice; the upper 16 bits are free. Put the generation counter there: the pointer is still 8 bytes, it fits in a register, data structures do not grow, and cache behaviour is the same as in C. A dereference takes two operations: mask the upper bits, compare the generation with the one in the object header. On ARM, TBI (Top Byte Ignore) gives the mask for free. A 16-bit counter overflows after 65,536 frees; with Vale's solution a slot whose counter overflows is never used again (retired), the memory loss is negligible, and the ABA risk is zero.

How the interior pointer problem (`&arr[5]`, `&s.field`) is solved here with 8 bytes: the allocator has to be slab based, with a single size class per page. Round the address down to the start of the page, read the slot size from the page header, compute the start of the slot, compare the generation in the header. A few integer operations; mimalloc already uses this layout. The allocator becomes part of the language, it is not replaceable. For the performance goal this is the right trade-off.

A later optimisation, only a note for now: for pointers whose lifetime is known (within the life of an arena, a fresh allocation inside a function) the compiler skips the check. This is Vale's regions idea. Not in the first version.

**2. The stack, from a safety point of view: second-class references.** There were two options: put generations on stack frames too (a cost on every call, little gain), or forbid at compile time that a pointer taken to the stack escapes. The second is the right one. The rule is simple:

- A reference to a local variable can be taken with `&x`.
- This reference only goes **down**: it becomes a function argument, or it is assigned to a local variable.
- It cannot be written to a struct field, returned from a function, captured by a closure or put on the heap.
- Anything that has to live longer is allocated on the heap or in an arena and gets a generation-checked pointer.

This check is not a borrow checker, it is a simple escape analysis: the compiler looks at where each `&` expression goes, with no lifetime notation and no inference. This is exactly Graydon's regret that "in Rust `&` should have been a parameter passing mode, not a first-class type". Hylo's `let`/`inout` parameters are the same idea. Zero runtime cost, none of Rust's noise, and stack use-after-return is impossible.

The price: there is no "pointer to a local variable inside a struct" pattern. It is common in C, but it can always be moved to the heap; the performance loss is measurable, the safety gain is certain.

**3. Panic.** Two complementary decisions are needed: a panic stops the whole process (not the thread, because the other threads could reach corrupted state), and the checks **stay on in release too**. Zig's ReleaseFast turns safety off; this language must have no option to turn it off, otherwise the reason for the whole design is lost. That is why the checks are cheap in the first place.

**The resulting model, on one page:**

| Thing | How |
|---|---|
| Heap pointer `*T` | 8 bytes, upper 16 bits are the generation, compared with the header on dereference, panic on mismatch |
| Stack reference `&T` | Second class, only goes down, escaping forbidden at compile time |
| Null | Optional type `?*T`, unwrapped with `orelse` |
| Array | Slice: pointer + length, bounds check panics |
| Allocator | Part of the language, slab based, arena API is standard |
| Arena | When the arena dies all slot generations are incremented, bulk invalidation in a single operation |
| Error | Panic, cannot be turned off in release |
| Macro | None; comptime later |
| Data race | Not solved, explicitly out of scope (first version) |
