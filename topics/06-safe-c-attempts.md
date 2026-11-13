# Attempts at a small, macro-free C with safer pointers

Could there be a C stripped of macros, as small as possible, that makes the use of pointers as safe as possible? It can be designed, and it has been, at least six times. The description is close to Zig's manifesto. So the real question is not "can it be done" but "how safe should pointers be, and what is the price".

**Attempts at the same idea:**

- **Cyclone** (2001): a safe dialect of C. Fat pointers, non-null pointers, regions. It stayed in academia; too many annotations were needed to fit in with existing C code.
- **Checked C** (Microsoft, 2016): an extension that adds `_Ptr<T>`, `_Array_ptr<T>` and bounds annotations to C. It aimed at converting existing code step by step. Microsoft dropped the project in 2022, the community maintains it.
- **Zig** (2016): no macros, comptime instead. Slices, optional types, explicit allocators. The most mature of them.
- **Odin** (2016): close to Zig, focused on games and graphics. No macros, multiple return values, the allocator is in the context.
- **Hare** (2022): Drew DeVault's language. Even smaller than Zig, QBE backend, the standard library is deliberately narrow. It claims to be "a C that will last a hundred years".
- **C3** (2019): a language that keeps C's syntax as much as possible and adds macros, slices and modules. The option that "touches C the least".

All six added slices and put null into the type, and the four that are new languages replaced the C preprocessor (C3 with macros of its own; Checked C, being an extension of C, keeps the preprocessor). This part can now be considered solved. None of them managed to solve use-after-free at compile time. That is exactly where the line is.

**What "safer pointers" means, layer by layer:**

| Layer | Technique | Cost | Who did it |
|---|---|---|---|
| Null | Optional type `?*T` | Zero | Zig, Odin, Hare, C3 |
| Array bounds | Slice (pointer + length), bounds check | Runtime, can be turned off in release | All |
| Type confusion | No implicit conversion, explicit cast | Zero | All |
| Undefined value | Reading uninitialised memory is an error | Zero | Zig, Odin |
| Leak / double free | Debug allocator tracking | Only in test mode | Zig |
| Use-after-free | ? | ? | Nobody, apart from the borrow checker |
| Data race | ? | ? | Nobody, apart from Rust |

The last two rows are the knot of the whole matter. To know whether a pointer is valid you have to answer the question "who does this memory still belong to, and when will it be freed". The known ways to answer this question:

1. **Prove it at compile time.** A borrow checker. Lifetime notation and restrictions come with it. Rust.
2. **Check it at run time.** Put a generation counter on every object and compare on every dereference. Vale's generational references, a cost of 2 to 10 percent. Or ARC, like Swift.
3. **Remove the question.** Throw pointers out of the language and leave only values and `inout` parameters. Hylo. Not proven yet.
4. **Limit it structurally.** Arena allocator: no freeing one by one, the whole arena dies together. Use-after-free is still possible but the window gets narrower. Zig's standard library and game engines do this by convention.
5. **Leave it to the hardware.** ARM MTE, CHERI. A tag on the pointer, a mismatch is a hardware fault. Independent of the language, but not widespread.

**Where is the unexplored territory?** Zig does way 4 by convention and does not write it into the language. Rust does way 1 completely, but the price is heavy. Between the two there is a point that has hardly been tried: **tying the arena to the language's type system.** That is, carrying the information "this pointer belongs to that arena and is valid as long as the arena lives" with a single lifetime parameter, not for every reference as in Rust, but only at arena boundaries. Cyclone's regions were close to this, and it died before Rust. Tofte and Talpin's region inference inferred regions automatically, but for ML. "Region inference + Zig's ergonomics" in a C-like language has practically not been tried. It is a concrete candidate for the single idea a new language could be built around.

**The honest answer:** solving the null, bounds, cast and macro problems is easy; Zig has done it. Without solving use-after-free a language cannot be called safe; the biggest share in Microsoft's and Google's memory bug statistics is there. No free way of solving it is known: it is either compile-time notation (Rust), or runtime cost (Vale, Swift), or giving up the concept of a reference (Hylo). A new language has to pick one of these three or find a fourth way.
