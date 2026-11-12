# New language design

A record of the conversation we had on 4 November 2026 about C, Rust, Zig, Go and C#, and of the language design
that came out of it. The first code was written on 12 November: [interpreter](interpreter/README.md). The language is named SG (the author's initials) and its source files end in `.sg`. The repository (git) exists as of the same day.

**Idea:** a systems language as fast as C, without macros, that gives Rust's compile-time guarantees in a notation
that reads like C#.

**Core principle:** if something can go wrong, the default behaviour is to warn or to stop; the risky path is chosen
on purpose. An error that can be caught at compile time is not left to run time, and an error whose fix is known comes with its fix.

**Where we left off:** On 5 November the generation model and the arena were dropped and Rust's ownership rules were chosen.
On 6 November shared ownership, the `?` rule, generics, interface, `enum`, `match` and the error model were decided. The first
example programs were written, and the questions they raised were settled. On 11 November `char`, narrowing conversion, the `print` error,
`NaN` and the named function were closed; the 8th example (graph), the 9th example (module, `use`, `public`) and the 10th example (HTTP request, byte slice) were written; the visibility rule and the slice type were decided. On the same day the goal became clear: performance and simplicity like C, a small language. The decision table was reviewed with this in mind, and five features were removed or made smaller (automatic `Ok`, `ThreadFunc`, two small rules; `Closable` was reduced to a single rule that looks only at local variables). With the 11th example (sensor window) the fixed-size array `T[N]` was decided; a list or an array is passed with `read xs` where a slice is expected. On 12 November the 12th example (stock tests) was written; the `test` block and `assert` were decided. Noted on the same day: we are not tied to a single language, and the best side of each language is taken in the form that suits us. On the same day the first slice of a tree-walking interpreter was written in Rust (functions, integers, `if`/`while`/`for`, `print`, `$"..."`, overflow panic), followed by its second slice (struct, `T*`, `T?`, `read`/`ref`, static types and move checking; the linked list example runs). The rules for line endings, the place of `{`/`else`, and statements were approved and are in the decision table. The three rules of the second slice (struct literal in a condition, assigning to a `ref`/`read` variable, looking does not move) were approved too. The third slice was written on the same day as well: methods and `this`, `T(args)`, `take`/`replace`, per-field move checking; the stack example runs, and its six small choices were approved on 12 November 2026. The fourth slice followed: `enum`, `match` and `is` with a variant; an expression tree runs, and its six small choices were approved the same day. The fifth slice added the built-in `Result<T, E>`, `try`, `??` on a `Result` and the unused `Result` error; the calculator runs from the tokens on, and its seven small choices were approved the same day. The sixth slice (`byte`, characters, `text[i]`, ranges of text; the calculator reads real text) was approved as well. The seventh slice (`: Copyable`, `: Formattable`) and the eighth (the built-in `List<T>` and `HashMap<K, V>`, `for` over a collection; the log summary runs) followed, and then the ninth: `HashSet<T>`, searching and sorting a list, and the compile error for a collection changed inside its own `for`. Their choices were approved on 12 November 2026. The tenth slice added `Equatable`, `Hashable` and `Comparable` as built-in names (`==`, `<`, `sort`, keys of your own types; a collection is compared with `==` when its items are); its choices were approved the same day. The eleventh slice added `test` blocks and the test command; the stock tests run. On the same day the goal was restated (safe and fast software, a language as small as C; performance and simplicity are not given up) and the design was made smaller: lambdas and capture were removed, a function value is the address of a named function, and generics of your own, `where` and your own `interface` were deferred. Function values and the first part of the borrow check (borrows that do not go together, until the last use) followed in the twelfth and thirteenth slices, and the second part (`from`, a borrow does not outlive its owner) in the fourteenth. The fifteenth added the third: types that carry a borrow (`struct Token<from source>`); the calculator no longer copies its input. The details are in [interpreter/README.md](interpreter/README.md). The code, comments, error messages and interpreter docs are in English; since 12 November 2026 the design documents are in English too (all files translated). When you come back, read
[decisions and open questions](design/04-decisions-and-open-questions.md) first.

## Design

| File | Contents |
|---|---|
| [Decisions and open questions](design/04-decisions-and-open-questions.md) | Decisions made, what is still open, next steps, gaps noticed later |
| [Memory model](design/01-memory-model.md) | Dropped: generation-checked pointer, second-class stack reference, panic |
| [Syntax draft](design/02-syntax-draft.md) | Current ownership notation, the dropped generation variant and the first Zig draft |
| [Roadmap](design/03-roadmap.md) | Where to start designing a language, the steps and the traps |
| [Example programs](design/05-examples.md) | Calculator, stack, threads, `List`, log summary, folder tree, order rules, graph, stock and invoice (module), HTTP request, sensor window (fixed-size array), stock tests, and the questions the examples raised |

## Code

| Folder | Contents |
|---|---|
| [interpreter](interpreter/README.md) | Tree-walking interpreter in Rust: what it runs, how it is built, what comes next |

## Topics

In the order of the conversation. Each file contains the question and the answer as they were in the conversation.

| File | Contents |
|---|---|
| [C and manual memory](topics/01-c-and-manual-memory.md) | Why memory management was left to the developer |
| [History of Rust ownership](topics/02-rust-ownership-history.md) | The roots of the idea, why it took 40 years, why C++ could not do it |
| [Zig](topics/03-zig.md) | Where it stands on safety, how long the move from C takes; C/Zig/Rust table |
| [Rust syntax and C#](topics/04-rust-syntax-and-csharp.md) | Where the noise comes from, the "second C++?" question |
| [Go](topics/05-go.md) | Its philosophy, its weak sides; C/Zig/Rust/Go table |
| [Safe C attempts](topics/06-safe-c-attempts.md) | Attempts from Cyclone to C3, the layers of pointer safety |
| [Use-after-free](topics/07-use-after-free.md) | The explicit error idea, why a simple table is not enough |

## Sources

- [Reading list](sources/reading-list.md): books, Hylo, Vale, Rust writings and links to tools.
