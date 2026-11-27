<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="brand/logo/sg-lang-horizontal-dark.svg">
    <img src="brand/logo/sg-lang-horizontal.svg" alt="sg-lang: a cat whose head and body make an S, with one blue and one amber eye" width="400">
  </picture>
</p>

<p align="center"><strong>Small enough to know by heart. Safe and fast by construction.</strong></p>

<p align="center">
  <a href="https://sg-lang.org">Web site</a> ·
  <a href="https://sg-lang.org/docs/learn/tour">Tour</a> ·
  <a href="https://sg-lang.org/docs/learn/book/getting-started">Book</a> ·
  <a href="https://sg-lang.org/docs/learn/install">Install</a> ·
  <a href="https://sg-lang.org/docs/status">Status</a> ·
  <a href="https://sg-lang.org/docs/about/community">Community</a>
</p>

SG is a small systems language that makes fast software safe by construction.

It is before its preview. The language, its checker, an interpreter and a back end that compiles
through C are built and tested on Linux, macOS and Windows; nothing is released yet and everything
can still change. This repository is the design, the code and the background reading behind them,
kept in the open.

## Why SG?

- **Memory safe without a collector.** One owner for each value, and borrows (`read`, `ref`) checked
  when the program compiles: the use after free, the dangling pointer and the forgotten error are
  compile errors, each with the fix when it is known.
- **Fast.** SG compiles through C with no runtime to carry. It is held to safe Rust doing the same
  work, and measured against C: of 61 programs written in SG and in C by hand, 37 take the time of C within
  5% and 13 are faster ([the numbers](benchmarks/README.md)).
- **Real programs.** [gifsicle](https://www.lcdf.org/gifsicle/) 1.99, the GIF tool, is written in SG as
  [sg-gifsicle](https://github.com/selcukgural/sg-gifsicle): every option, the same bytes as gifsicle in 58,767
  runs, 1.01 to 1.31 times its time, and frames resized on several threads with no lock
  ([the measurements](benchmarks/programs.md#gifsicle)).
- **Small.** A feature pays for itself in what it costs when the program runs and in what it costs
  to learn; the safe way is the default and the risky way is written on purpose (`unsafe`).

More on the site: [why SG](https://sg-lang.org/docs/about/why), with a program, what the compiler
says and the measurements, and [where it stands](https://sg-lang.org/docs/status).

## Quick start

You need Rust (to build `sg`) and a C compiler (`cc`, clang or gcc) for compiled programs.

```
git clone https://github.com/selcukgural/sg-lang
cd sg-lang/interpreter

cargo run -- examples/stack.sg                     # run a program in the interpreter
cargo run -- test examples/shop                    # run the tests of a program with modules
cargo run -- build examples/primes.sg --release    # compile it through C
cargo test                                         # everything, also under the sanitizers and a leak check
```

A small program, to see what SG looks like:

```sg
struct Item {
    string name
    int count
}

Result<int, string> total(read List<Item> items) {
    var sum = 0
    for item in items {
        if item.count < 0 {
            return Error($"{item.name} has a negative count".toString())
        }
        sum += item.count
    }
    return Ok(sum)
}

void main() {
    var items = List<Item> {
        Item { name = "bolts", count = 40 },
        Item { name = "nuts", count = 2 },
    }
    match total(read items) {
        Ok(sum) => print($"{sum} items in stock")
        Error(why) => print(why)
    }
}
```

```text
42 items in stock
```

Types come first and lines need no semicolons. `read` lends the list to `total` without giving it
away, so `main` still owns it; an error is a value (`Result`) that must be handled, and `match`
names every case.

Then [take the tour](https://sg-lang.org/docs/learn/tour) or read [the book](https://sg-lang.org/docs/learn/book/getting-started).
The programs in [interpreter/examples](interpreter/examples) show the language as it is today.

## Where to find what

| To | Read |
|---|---|
| Learn the language | The [tour](https://sg-lang.org/docs/learn/tour) and the [book](https://sg-lang.org/docs/learn/book/getting-started) on the site, [SG for Rust programmers](design/06-rust-to-sg.md), and the [example programs](interpreter/examples) |
| See what runs today | [What runs today](interpreter/docs/features/README.md), by topic: [the language](interpreter/docs/features/language.md) · [ownership and borrowing](interpreter/docs/features/ownership.md) · [collections](interpreter/docs/features/collections.md) · [numbers and text](interpreter/docs/features/numbers-and-text.md) · [tasks and async](interpreter/docs/features/concurrency.md) · [C and the machine](interpreter/docs/features/c-and-the-machine.md) · [the library](interpreter/docs/features/library.md) · [tests and the editor](interpreter/docs/features/tools.md) |
| See what is missing | [Not here yet](interpreter/docs/not-here-yet.md), and in short the [status page](https://sg-lang.org/docs/status) |
| Know the rules a program is held to | [Rules already enforced](interpreter/docs/rules-enforced.md) |
| Know why it is so | [The decisions by topic](design/rules/README.md), each with its reason; [the idea and the principle behind them](design/04-decisions-and-open-questions.md); [the choices of each slice](design/decisions/README.md), day by day |
| Follow how it was built, and what comes next | [How far it has come, and what is next](interpreter/docs/progress.md) |
| See how fast it is | [Benchmarks](benchmarks/README.md): [against C](benchmarks/against-c.md) · [programs against Rust and Zig](benchmarks/programs.md) · [the library against Rust and Zig](benchmarks/library.md) · [slower than C](benchmarks/slower-than-c.md) · [how it is measured](benchmarks/method.md) |
| Build it, run it, change it | [The interpreter](interpreter/README.md): [the C back end](interpreter/docs/c-back-end.md) · [how it is built](interpreter/docs/architecture.md) · [how it is tested](interpreter/docs/testing.md) |
| Find your way in the repository | [The repository](https://sg-lang.org/docs/about/repository): every folder, the design documents and the background essays |
| Read the background | [Essays on C, Rust, Zig and Go](topics), the [reading list](sources/reading-list.md), and [how it got here](https://sg-lang.org/docs/about/history) |
| Ask a question, share an idea | The [mailing list](https://groups.google.com/g/sg-lang) (join by an empty message to [sg-lang+subscribe@googlegroups.com](mailto:sg-lang+subscribe@googlegroups.com)) or [Discussions](https://github.com/selcukgural/sg-lang/discussions) on GitHub, whichever you like; bugs go to [the issues](https://github.com/selcukgural/sg-lang/issues), security problems to [the security policy](SECURITY.md). [Community](https://sg-lang.org/docs/about/community) says where each thing goes |

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option. This is the way Rust is licensed. It covers the design documents, the
interpreter and the back end, and the C that `sg build` writes into a compiled program: a
program written in SG is yours, under whatever license you choose. The same license covers the
picture of Pasha, the mascot (`brand/pasha.png`), and the logo (`brand/`). The letters of the
wordmark are outlines of the Outfit font, and the font in `brand/src/` is under the SIL Open Font
License 1.1 (`brand/src/OFL-Outfit.txt`). The Unicode tables
(`interpreter/src/unicode_tables.rs`) are made from the Unicode Character Database, under the
Unicode License v3, whose notice is at the top of that file; so is Unicode's
`NormalizationTest.txt` in `interpreter/tests/normalization-suite`, kept unchanged beside its licence.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion
in this work by you, as defined in the Apache-2.0 license, shall be dual licensed as above,
without any additional terms or conditions.
