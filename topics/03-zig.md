# Zig: where it stands, how long the move from C takes

*Source: the conversation of 4 November 2026. The questions and answers are given as they were in the conversation; for correction notes see [decisions and open questions](../design/04-decisions-and-open-questions.md).*

## Question

> So where does Zig stand on this?

## Answer

Zig deliberately chooses the opposite of Rust: instead of guaranteeing memory safety through the type system, it keeps C's manual model and tries to make bugs more visible and easier to catch. Its claim is to be "a better C", not "a safe C".

**The parts of Zig's approach:**

- **The allocator is explicit everywhere.** There is no hidden `malloc` in Zig. Every function that needs memory takes an `Allocator` parameter. The standard library is written this way too. This makes the question "does this function allocate memory" readable from the signature, and makes strategies such as arena, fixed-buffer and page allocator interchangeable. In C this discipline exists but is a convention; in Zig it is structural.

- **`defer` and `errdefer`.** An explicit mechanism that takes the place of RAII. You write the release right below the allocation. Destructors do not run in secret; this follows from Zig's "no hidden control flow" principle.

- **Runtime checks in Debug and ReleaseSafe modes.** Array bounds, integer overflow, null unwrap and part of use-after-free (`GeneralPurposeAllocator` reports leaks and double frees) are caught at run time. In ReleaseFast these checks are removed. So safety is not a compile-time proof, it is a bug caught during testing.

- **Optional types and error unions.** The null pointer problem is solved in the language with `?*T`. Error handling with `try`/`catch` is mandatory. These parts are real progress over C.

- **What it does not solve.** Use-after-free, dangling pointers and data races are not caught at compile time. If you hold a slice and grow the ArrayList underneath it, Zig does not warn you. This is exactly what Rust's borrow checker forbids.

**Why is this a choice and not a shortcoming?** Andrew Kelley's argument is this: the price of a borrow checker is language complexity, compile time, and making certain data structures (doubly linked lists, graphs, self-referential structures) harder to write. Zig chooses not to pay this price, and in return offers a small language that takes seconds to read, does macro-free metaprogramming with comptime, and works with C with zero friction. Being able to compile C with `zig cc` and importing C headers directly are part of this position.

**Comparison table:**

| Topic | C | Zig | Rust |
|---|---|---|---|
| Null pointer | None | Solved in the type system | Solved in the type system |
| Array bounds | None | Runtime in safe modes | Runtime + often compile time |
| Use-after-free | None | Partly runtime (debug allocator) | Compile time |
| Data race | None | None | Compile time |
| Hidden control flow | None | None (principle) | Yes (Drop, operator overload, panic) |
| Language complexity | Low | Low | High |
| Maturity | Full | Pre-1.0, breaking changes continue | Stable |

**The answer to "where does it stand":** On the safety spectrum it is between C and Rust, but close to C, not to Rust. Zig's audience is people who say "I like writing C but I want the tools of 2026"; Rust's audience is those who say "I want memory bugs to be impossible". So the two are less rivals than answers to different questions. Projects such as Bun and TigerBeetle chose Zig for performance and control; Android, the Windows kernel and Linux accepted Rust to bring down the number of security vulnerabilities.

A practical note: Zig is not 1.0 yet, and the standard library breaks with every release. "Sustainable institutional support", one of Rust's five pieces, is for Zig still limited to the donations of the Zig Software Foundation. This is a risk of the ecosystem, not of the language.

## Question

> so how much of Zig does someone who knows C have to learn, how big is the part that needs learning? With how much effort can one move from C to Zig?

## Answer

Short answer: for someone who knows C, Zig is one of the easiest systems languages to learn. The memory model is the same, pointers are the same, structs are the same. The effort is about a quarter of moving to Rust. But it has to be split in two, into "knowledge" and "habit".

**The part that comes almost for free (1-2 days):**

- Pointers, structs, unions, enums, the stack/heap distinction, bit operations. The concepts are one to one.
- Using C headers directly with `@cImport`. You do not need to write bindings to call your existing C code from Zig.
- Compiling an existing C project with `zig cc` and replacing the Makefile with `build.zig`.

**Syntax differences (1 week):**

- The type name is on the right: `var x: i32 = 5`. Pointer `*T`, slice `[]T`, array `[N]T`. Slices are the thing C does not have: a pointer + length pair that carries bounds checking.
- `const` is the default, `var` is explicit. An unused variable is a compile error.
- Casts are explicit and in function form: `@intCast`, `@ptrCast`, `@as`. No implicit int conversion, overflow panics in debug.
- `switch` has to be exhaustive, and there is no `break`.
- Instead of null, `?T` and `orelse`. Instead of error codes, `!T` and `try`.

**The three things that really have to be learned (2-4 weeks):**

1. **The discipline of passing allocators.** Instead of calling `malloc`, every function that needs memory takes `allocator: std.mem.Allocator`. This changes the structure of your code. Understanding when to use an arena allocator is the art of it. Getting rid of the global heap habit from C is the hardest part.

2. **Error unions.** Writing `fn f() !T` and passing errors up with `try`, cleaning up on the error path with `errdefer`. It replaces the `goto cleanup` pattern of C. Mechanically easy, but designing error sets takes getting used to.

3. **comptime.** Ordinary Zig code that runs at compile time, in place of C macros. Generics are functions that return a type, in the form `fn List(comptime T: type) type`. The concept feels foreign at first, but it is much simpler than C++ templates. Mastering it takes a month, starting to use it takes a day.

**The hard things:**

- The documentation of the standard library is weak. You learn by reading the source code. The good news: the source code is readable Zig.
- Breakage between versions. The blog posts are for 0.11, you are using 0.14, it does not compile. This is what wastes the most time while learning.
- The error messages do not guide you as well as Rust's.
- `@cImport` cannot translate some C macros, and they have to be written by hand.

**Time estimate:**

| Level | Time |
|---|---|
| Compiling C code with Zig and calling a Zig function from C | 1 day |
| Writing a small tool from scratch in Zig | 1 week |
| Using the allocator and error model naturally | 1 month |
| Library-level generic code with comptime | 2-3 months |

**Migration strategy:** Do not convert an existing C project all at once. Compile the C with `build.zig`, write new modules in Zig, bridge them with `@cImport`. Zig was designed for this; in Rust the same thing needs `unsafe` at the FFI boundary and a binding generator.

For comparison with Rust: using ownership intuitively in Rust takes someone who knows C 3 to 6 months, because you have to unlearn some of the patterns you know to be right. In Zig the only thing you have to unlearn is thinking that `malloc` is global.

As learning resources, start with ziglings (ziglings.org) and zig.guide; both keep up with the current version.
