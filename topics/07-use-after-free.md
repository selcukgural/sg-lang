# Use-after-free: an explicit error on access to freed memory

Use-after-free is the use of a resource after it has been freed, that is, handed back. In C it is undefined behaviour, and many crashes come from it: a program that gives no error on one run can fail on the next. Would it not solve most of the problem to track freed objects and raise an explicit error when one is used again?

The "sometimes it works, sometimes it crashes" behaviour comes exactly from the undefined behaviour. After memory is freed the allocator can give it to another object; a read through the old pointer reads that new object's data, and a write corrupts it. This is the favourite class of exploits, because the attacker can control what that new object will be.

**The idea is not new, but it is valid.** "Detect access to a freed object and raise an explicit error" is a runtime check, the second of the ways listed in [safe C attempts](06-safe-c-attempts.md). It has been implemented in three different forms:

- **AddressSanitizer** (Clang/GCC, 2012): It marks every freed region as "poisoned" and stops on access. Exactly this behaviour. The price is a 2x slowdown and 2 to 3 times the memory; that is why it is used only for testing and fuzzing, not in production.
- **Vale's generational references**: A generation counter at the start of every allocation, and a copy of the counter in every pointer. On dereference the two are compared, and a mismatch is a panic. Freeing increments the counter, so old pointers no longer match. The measured price is 2 to 10 percent.
- **ARM MTE and CHERI**: The same idea in hardware. A tag in the upper bits of the pointer, a tag on 16-byte granules in memory, a mismatch is a CPU fault. Enabled on Android Pixels, at a price of a few percent.

**So the short answer: yes, it solves it to a large extent.** What it solves is exactly this: it turns undefined behaviour into a defined crash. From a security point of view this is a very big difference; the attacker can no longer corrupt memory, only bring the program down. Go and Java do the same thing for array bounds and nobody counts it as a shortcoming.

**But it cannot be done with "a simple tracking table", and that is the point.** The naive implementation is imagined like this: keep the freed addresses in a set, and look in the set on dereference. This does not work, because:

1. **Addresses are reused.** The freed address 0x1000 is given to a new object at the next malloc. The set says "0x1000 was freed", but it is alive now. An address alone is not an identity. That is why Vale adds a generation number to the pointer: identity = address + generation.
2. **Looking in a hash table on every dereference** brings C down to the speed of Python. The check has to be a constant-time comparison carried next to the pointer, not a table lookup.
3. **Pointers can point into the middle of an object.** `&arr[5]`, `&s.field`. If the generation counter is at the start of the object, how is it reached from an interior pointer? Vale's answer is to inflate the pointer: address + generation + offset of the start. The pointer goes from 8 bytes to 16 bytes, every data structure grows, and the cache is less efficient.
4. **Freed memory cannot be given back.** If the generation counter sits in the object's header, that header has to stay alive, otherwise there is nothing left to check. So the allocator keeps the slot in the same size class and preserves the counter; the memory cannot be fully returned to the OS. Vale accepts this.
5. **Objects on the stack.** Taking a pointer to a function's local variable and using it after the function returns is also use-after-free. Heap tracking does not see this; either stack frames get generations too, or taking pointers to the stack is forbidden.
6. **There is no "throw" in C.** In a language with no exceptions this becomes a panic/abort: the program stops. A recoverable error needs unwinding and destructor infrastructure, which conflicts with the "small language" goal. Zig's choice is panic.

**What it does not solve, honestly:**

- Data races: two threads check the same pointer, both see it as "valid", one frees it, the other writes. If the generation check is not atomic, a window remains.
- The error shows up in production, not at compile time. An untested path crashes at the customer's site. This is exactly where Rust's advantage is, at the price described in [Rust syntax and C#](04-rust-syntax-and-csharp.md).
- Bounds, null and uninitialised memory need separate solutions; this is only for dangling pointers.

**From a language design point of view** this way is the most realistic one for a small project. Writing a borrow checker is a research project of years; a fat pointer with a generation counter is a few weeks of work in the allocator and codegen. Vale got this far as a one-person project. With arenas on top (when an arena dies all its generations become invalid, with a single comparison) the cost drops further, and Zig's convention turns into a rule of the language.

Three things then have to be decided, in order: whether the pointer is 8 bytes or 16 bytes (performance and C compatibility), whether taking a pointer to the stack is allowed (scope of safety), and whether the error is a panic or recoverable (language size). With these three chosen, a memory model is settled on paper. SG's first model was built this way ([memory model](../design/01-memory-model.md)); it was dropped a day later for ownership.
