# Reading list

**Getting started**

- *Crafting Interpreters*, Bob Nystrom. All of it is free: https://craftinginterpreters.com
- *Types and Programming Languages*, Benjamin Pierce. MIT Press page: https://mitpress.mit.edu/9780262162098 (there is no free legal edition; alongside it there is the author's course site https://www.cis.upenn.edu/~bcpierce/tapl)

**Memory safety ideas outside Rust**

- Hylo home page and design overview: https://www.hylo-lang.org
- Hylo language tour: https://docs.hylo-lang.org/language-tour/bindings
- Hylo specification (GitHub): https://github.com/hylo-lang/specification
- The object model document Hylo wrote for the Swift designers: https://github.com/hylo-lang/Documentation
- Vale generational references: https://vale.dev/vision/safety-generational-references
- Evan Ovadia's detailed post: https://verdagon.dev/blog/generational-references
- Vale regions post: https://verdagon.dev/blog/zero-cost-memory-safety-regions-overview

A note on Vale: a secondary source says development stopped in 2026. The ideas are still worth reading, but consider it dead as a language.

**Rust's own reckoning**

- Graydon Hoare, "The Rust I Wanted Had No Future" (2023): https://graydon2.dreamwidth.org/307291.html
- Rust RFC archive: https://github.com/rust-lang/rfcs and the readable version https://rust-lang.github.io/rfcs
- Niko Matsakis, "Polonius revisited, part 1": https://smallcultfollowing.com/babysteps/blog/2023/09/22/polonius-part-1
- Part 2: https://smallcultfollowing.com/babysteps/blog/2023/09/29/polonius-part-2
- The same author's post "Borrow checking without lifetimes": https://smallcultfollowing.com/babysteps/blog/2024/03/04/borrow-checking-without-lifetimes
- The Polonius book and its list of related reading: https://rust-lang.github.io/polonius/see_also.html
- Rust MIR design (as an example of an IR): https://rustc-dev-guide.rust-lang.org/mir/index.html

**Tools for writing a language in Rust**

- logos lexer: https://github.com/maciejhirsz/logos
- chumsky parser: https://github.com/zesterer/chumsky
- inkwell LLVM bindings: https://github.com/TheDan64/inkwell
- Cranelift: https://cranelift.dev

A reading order: the first half of Crafting Interpreters, then Graydon's post, then Hylo's home page; Pierce and Polonius when the type checker is being written.

Sources: [Graydon Hoare mirror](https://anilkeshwani.github.io/garden/Clippings/graydon2--The-Rust-I-Wanted-Had-No-Future), [Hylo docs](https://docs.hylo-lang.org/language-tour/bindings), [Hylo site repo](https://github.com/hylo-lang/hylo-lang.github.io/blob/main/index.md), [Vale on GitHub](https://github.com/Verdagon/Vale), [Vale regions](https://hn.jakechampion.name/reader/https://verdagon.dev/blog/zero-cost-memory-safety-regions-overview), [Polonius see also](https://rust-lang.github.io/polonius/see_also.html), [Polonius part 1](https://b.hatena.ne.jp/entry/s/smallcultfollowing.com/babysteps/blog/2023/09/22/polonius-part-1), [TAPL MIT Press](https://mitpressbookstore.mit.edu/book/9780262162098), [TAPL Wikipedia](https://en.wikipedia.org/wiki/Types_and_Programming_Languages)
