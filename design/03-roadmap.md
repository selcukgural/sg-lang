# Where to start designing a language

*Source: the conversation of 4 November 2026. The questions and answers are given as they were in the conversation; for correction notes see [decisions and open questions](04-decisions-and-open-questions.md).*

## Question

> so if I started designing a language, where should I start

## Answer

Start not from syntax but from **a single idea**. You already have the summary of this conversation: "Is memory safety without a GC possible with a notation as simple as Zig's?" If a language's reason to exist cannot be said in one sentence, every other decision becomes arbitrary, and that is exactly how a second C++ is born.

**The order should be this:**

1. **Write the code first, not the language.** Write 10 to 15 example programs in your language: a linked list, an HTTP parser, an arena allocator, a counter shared between threads. Without a compiler. While reading these programs, ask "what is hidden here?" How will you show lifetimes, how will you reject shared mutation, how will you carry errors? 80 percent of the design is finished at this stage, on paper. Rust's `'a` mistake would have been caught at this stage.

2. **Decide on the memory model, the rest follows from it.** The options: Rust-style borrow checking, Hylo-style mutable value semantics (no references, only values and `inout`), Swift-style ARC, Vale-style generational references, or none of them, like Zig. Each one dictates the syntax. Do not write a lexer before making this decision.

3. **Start with a tree-walking interpreter, not with a compiler.** Bob Nystrom's book *Crafting Interpreters* is exactly this path; it is free and the first half takes a week. The goal: `print(1 + 2)` runs by the end of the first week. Type system, memory model, backend all come later. Without something that runs, language design turns into endless debate.

4. **Choose Rust as the language you write it in.** You already know it, and the tools are ready: `logos` for the lexer, `chumsky` or a hand-written recursive descent parser, `inkwell` for LLVM, `cranelift` for a faster but less optimising backend. Prefer the hand-written parser; it is much better than parser generators for error messages and recovery.

5. **The type checker is the third step.** First simple nominal types, then generics, and the memory rules last. If you are going to write something like a borrow checker, read Niko Matsakis's Polonius and "Rust belts" writings and Hylo's design documents. This is the real research part and the place that will take the most time.

6. **The backend comes last.** Hooking up to LLVM is easier than people think, a few days with `inkwell`. The real issue is designing your own IR; borrow checking and optimisation are not done on the type tree but on an SSA-like IR. Rust's MIR is a good example.

**Traps to avoid:**

- Syntax bikeshedding. The `fn` or `func` debate produces zero value. Take Zig's and move on.
- Supporting everything. In the first version no generics, no traits, no async, no macros. A language that starts big on day one never gets finished.
- The self-hosting dream. Writing the language in itself is not a milestone, it is a distraction.
- The standard library. For the first year `print` and a C FFI are enough.

**Reading list, in order:**

- *Crafting Interpreters*, Nystrom. Practical, you can start right away.
- *Types and Programming Languages*, Pierce. The one book for type systems. Heavy, take it chapter by chapter.
- The design documents of Hylo and Vale. For memory safety ideas outside Rust.
- The Rust RFC archive. Why each decision was made, and which ones became regrets, is written there.
- Graydon Hoare's post on "what Rust would have been like if I had made it" (2023). The language creator's own regrets.

**A note for your profile:** This project takes years, and nobody uses it for the first six months. Given your need for fast feedback, set concrete weekly goals: week 1 the interpreter, week 2 functions, week 3 structs, week 4 the first type error. If you cannot see something running every week, the project dies. Language design is not "a product", it is research; do not mix it up with your income goal.

If you like, we can start on step 1 together: choosing the memory model and writing the same program in three different notations to see which one stays readable can be done in an afternoon, without touching a compiler.
