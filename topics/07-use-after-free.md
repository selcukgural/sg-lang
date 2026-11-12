# Use-after-free: an explicit error on access to freed memory

*Source: the conversation of 4 November 2026. The questions and answers are given as they were in the conversation; for correction notes see [decisions and open questions](../design/04-decisions-and-open-questions.md).*

## Question

> this is going great, can we record everything we have talked about somewhere? I think both of us will need to come back, look at it and remember what we discussed.
>
> First let us make the problem clear: the use-after-free problem is the case where a resource that has been freed, that is, handed back, is used again. As far as I know C is very clear here: at many points it shows undefined behaviour, and I think many of the crashes may come from this. Because where it gives no error on one run, it can give an error on another run. So wouldn't throwing an explicit error when a handed-back resource is accessed solve this problem to a large extent? What if allocated objects were tracked after being freed and an explicit throw was raised when they are used again

## Answer

Your diagnosis is right: use-after-free is undefined behaviour in C, and the "sometimes it works, sometimes it crashes" behaviour comes exactly from that. After memory is freed the allocator can give it to another object; if you read through the old pointer you read that new object's data, and if you write you corrupt it. This is the favourite class of exploits, because the attacker can control what that new object will be.

**Your proposal is not new, but it is valid.** The idea "detect access to a freed object and raise an explicit error" is the second of the ways I listed in the previous message: a runtime check. It has been implemented in three different forms:

- **AddressSanitizer** (Clang/GCC, 2012): It marks every freed region as "poisoned" and stops on access. Exactly the behaviour you want. The price is a 2x slowdown and 2 to 3 times the memory; that is why it is used only for testing and fuzzing, not in production.
- **Vale's generational references**: A generation counter at the start of every allocation, and a copy of the counter in every pointer. On dereference the two are compared, and a mismatch is a panic. Freeing increments the counter, so old pointers no longer match. The measured price is 2 to 10 percent.
- **ARM MTE and CHERI**: The same idea in hardware. A tag in the upper bits of the pointer, a tag on 16-byte granules in memory, a mismatch is a CPU fault. Enabled on Android Pixels, at a price of a few percent.

**So the short answer: yes, it solves it to a large extent.** What it solves is exactly this: it turns undefined behaviour into a defined crash. From a security point of view this is a very big difference; the attacker can no longer corrupt memory, only bring the program down. Go and Java do the same thing for array bounds and nobody counts it as a shortcoming.

**But it cannot be done with "a simple tracking table", and that is the point.** The naive implementation is imagined like this: keep the freed addresses in a set, and look in the set on dereference. This does not work, because:

1. **Addresses are reused.** The freed address 0x1000 is given to a new object at the next malloc. The set says "0x1000 was freed", but it is alive now. An address alone is not an identity. That is why Vale adds a generation number to the pointer: identity = address + generation.
2. **Looking in a hash table on every dereference** brings C down to the speed of Python. The check has to be a constant-time comparison carried next to the pointer, not a table lookup.
3. **Pointers can point into the middle of an object.** `&arr[5]`, `&s.field`. If the generation counter is at the start of the object, how do you reach it from an interior pointer? Vale's answer is to inflate the pointer: address + generation + offset of the start. The pointer goes from 8 bytes to 16 bytes, all your data structures grow, and the cache is less efficient.
4. **Freed memory cannot be given back.** If the generation counter sits in the object's header, that header has to stay alive, otherwise there is nothing left to check. So the allocator keeps the slot in the same size class and preserves the counter; the memory cannot be fully returned to the OS. Vale accepts this.
5. **Objects on the stack.** Taking a pointer to a function's local variable and using it after the function returns is also use-after-free. Heap tracking does not see this; you have to put generations on stack frames too, or forbid taking pointers to the stack.
6. **There is no "throw" in C.** If your language has no exceptions, this becomes a panic/abort: the program stops. If you want to make it a recoverable error you need unwinding and destructor infrastructure, which conflicts with the "small language" goal. Zig's choice is panic.

**What it does not solve, honestly:**

- Data races: two threads check the same pointer, both see it as "valid", one frees it, the other writes. If the generation check is not atomic, a window remains.
- The error shows up in production, not at compile time. An untested path crashes at the customer's site. This is exactly where Rust's advantage is; and you know its price.
- Bounds, null and uninitialised memory need separate solutions; this is only for dangling pointers.

**My assessment from a language design point of view:** This way is the most realistic one for a side project. Writing a borrow checker is a research project of years; a fat pointer with a generation counter is a few weeks of work in the allocator and codegen. Vale got this far as a one-person project. If you add arenas on top (when an arena dies all its generations become invalid, with a single comparison) the cost drops further, and Zig's convention turns into a rule of the language.

There are three things you need to decide, in order: whether the pointer is 8 bytes or 16 bytes (performance and C compatibility), whether taking a pointer to the stack is allowed (scope of safety), and whether the error is a panic or recoverable (language size). Once you choose these three, the memory model is settled on paper and we can move on to the stage of writing the example programs.
