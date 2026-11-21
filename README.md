<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="brand/logo/sg-lang-horizontal-dark.svg">
    <img src="brand/logo/sg-lang-horizontal.svg" alt="sg-lang: a cat whose head and body make an S, with one blue and one amber eye" width="400">
  </picture>
</p>

<p align="center"><strong>Safe by design. Fast by default.</strong></p>

<p align="center">
  <a href="https://sg-lang.org">Web site</a> ·
  <a href="https://sg-lang.org/docs/learn/tour">Tour</a> ·
  <a href="https://sg-lang.org/docs/learn/book/getting-started">Book</a> ·
  <a href="https://sg-lang.org/docs/learn/install">Install</a> ·
  <a href="https://sg-lang.org/docs/status">Status</a>
</p>

SG is a small systems language: about as small and as fast as C, memory safe like Rust, and
written in a notation that reads like C#.

Its web site is [sg-lang.org](https://sg-lang.org): a tour of the language, how to install it, SG for Rust
programmers, and where it stands.

It is still being designed. There is a checker, an interpreter and a back end that compiles
through C, all of them a few days old. Nothing is released and everything can change. This
repository is the design, the code and the background reading behind both, kept in the open.

## Where to find what

| To | Read |
|---|---|
| Learn the language | The [tour](https://sg-lang.org/docs/learn/tour) and the [book](https://sg-lang.org/docs/learn/book/getting-started) on the site, [SG for Rust programmers](design/06-rust-to-sg.md), and the [example programs](interpreter/examples) |
| See what runs today | [What runs today](interpreter/docs/features/README.md), by topic: [the language](interpreter/docs/features/language.md) · [ownership and borrowing](interpreter/docs/features/ownership.md) · [collections](interpreter/docs/features/collections.md) · [numbers and text](interpreter/docs/features/numbers-and-text.md) · [tasks and async](interpreter/docs/features/concurrency.md) · [C and the machine](interpreter/docs/features/c-and-the-machine.md) · [the library](interpreter/docs/features/library.md) · [tests and the editor](interpreter/docs/features/tools.md) |
| See what is missing | [Not here yet](interpreter/docs/not-here-yet.md), and in short the [status page](https://sg-lang.org/docs/status) |
| Know the rules a program is held to | [Rules already enforced](interpreter/docs/rules-enforced.md) |
| Know why it is so | [Decisions and open questions](design/04-decisions-and-open-questions.md), and [the choices of each slice](design/decisions/README.md), day by day |
| Follow how it was built, and what comes next | [How far it has come, and what is next](interpreter/docs/progress.md) |
| See how fast it is | [Benchmarks](benchmarks/README.md): [against C](benchmarks/against-c.md) · [programs against Rust and Zig](benchmarks/programs.md) · [the library against Rust and Zig](benchmarks/library.md) · [slower than C](benchmarks/slower-than-c.md) · [how it is measured](benchmarks/method.md) |
| Build it, run it, change it | [The interpreter](interpreter/README.md): [the C back end](interpreter/docs/c-back-end.md) · [how it is built](interpreter/docs/architecture.md) · [how it is tested](interpreter/docs/testing.md) |
| Read the background | [Essays on C, Rust, Zig and Go](topics) and the [reading list](sources/reading-list.md) |

## A taste

```
struct Product {
    string code
    long price
}

enum StockError : Formattable {
    Unknown(string code)
}

// A look at a product, or nothing. `from products`: what comes back points into the list.
read Product? find(read List<Product> products, read string code) from products {
    for product in read products {
        if product.code == code {
            return product
        }
    }
    return null
}

Result<long, StockError> priceOf(read List<Product> products, read string code) {
    var product = find(products, code) ?? return Error(StockError.Unknown(code.clone()))
    return Ok(product.price)
}

Result<void, StockError> main() {
    var products = List<Product> {
        Product { code = "pen", price = 1500 },
        Product { code = "notebook", price = 4000 },
    }
    var total = try priceOf(read products, "pen") + try priceOf(read products, "notebook")
    print($"total: {total} cents")
    try priceOf(read products, "ink")
    return Ok()
}
```

```
total: 5500 cents
error: Unknown("ink")
```

A few things to notice:

- **A value has one owner**, and it is freed when the owner ends. There is no garbage collector
  and no `free` to forget.
- **`read` lends a value to look at, `ref` lends it to change.** The word is written where the
  value is lent, so the call site shows what a function can do to your data.
- **`T?` is a value that may be missing, `Result<T, E>` one that may have failed.** Neither can be
  used before it is opened, with `??`, `try`, `is null` or `match`. There are no exceptions and
  no null pointer to follow by accident.
- **`find` gives back a pointer into the list**, and the compiler knows it. In the compiled
  program it is the plain pointer you would write in C.

## The idea

Most of what makes Rust safe is a handful of rules about who owns a value and who may look at
it. SG keeps those rules and tries to leave out nearly everything else. The aim is a language
you can hold in your head the way you can hold C, where the compiler still catches the use
after free, the dangling pointer and the forgotten error.

Three principles decide most questions:

1. **The safe way is the default; the risky way is chosen on purpose.** If something can go
   wrong, the language stops you or makes you say so.
2. **What can be caught when compiling is not left to run time**, and an error whose fix is
   known comes with the fix.
3. **Small and fast come first.** A feature has to pay for itself twice: in what it costs when
   the program runs, and in what it costs to learn. Lambdas, your own generics and your own
   interfaces were taken out or put off for that reason.

## What the compiler tells you

Using a value after it was given away:

```
error: `names` was moved and can no longer be used
  --> moved.sg:4:11
 4 |     print(names.length())
   |           ^^^^^
  it was moved here:
  --> moved.sg:3:16
 3 |     var kept = names
   |                ^^^^^
```

Returning a pointer to something that is about to disappear:

```
error: this returns a borrow of `best`, and `best` is gone when the function returns
  --> dangle.sg:8:12
 8 |     return read best
   |            ^^^^^^^^^
  hint: return the value itself instead of a borrow of it
```

## Is it fast?

That is the claim, so it is measured. Each benchmark is written twice, in SG and in C by hand,
and both go through the same C compiler with `-O2`. Of the 61 programs, 37 take the time of C
within 5%, 13 are faster and 11 are slower. Some of them (time of SG divided by time of C; 1.00 is the same
speed):

| Program | SG / C |
|---|---|
| Recursive `fib(40)` | 0.98 |
| A linked list of a million nodes, built, walked and freed | 1.01 |
| A list of a million numbers, filled, changed, added up | 1.02 |
| Three hundred million lookups that give back a borrow or nothing | 1.01 |
| Counting in a hash table | 1.20 |
| Printing two million structs | 0.47 |

The hash table is slower because it uses a keyed hash by default, which protects against
crafted keys; with a plain hash it is 0.99. The same programs were measured on Windows 11 too
(64-bit Arm, in a virtual machine): most take the time of C there as well, and those that lean on
Windows' C library (`printf`, `fgets`) are several times faster in SG. These are small programs on
two machines, not proof. All the numbers, and how they were taken, are in
[benchmarks](benchmarks/README.md).

Real programs are measured too, written the same way in SG, Rust and Zig: [the trials](benchmarks/programs.md).
Each module of the library is measured the same way against Rust's crates and Zig's own library, JSON
first, where SG reads, walks and writes 100 MB faster than `serde_json` and Zig's `std.json`, in less memory.
Against the faster of Rust and Zig, SG is faster in 8 of the 25 cases and slower in 14:
[every case](benchmarks/README.md#at-a-glance-against-rust-and-zig).

## Try it

You need Rust (to build `sg`) and a C compiler (`cc`, clang or gcc) for compiled programs.

```
git clone https://github.com/selcukgural/sg-lang
cd sg-lang/interpreter

cargo run -- examples/stack.sg                     # run a program in the interpreter
cargo run -- examples/shop/main.sg                 # a program with modules
cargo run -- test examples/shop                    # run its tests
cargo run -- test examples/shop --compiled         # the same tests, compiled through C
cargo run -- build examples/primes.sg --release    # compile it through C
cargo test                                         # everything, also under the sanitizers and a leak check
```

The programs in [interpreter/examples](interpreter/examples) are the best place to see the
language as it is today.

## Where it stands

SG is before its preview. The language, its checker, an interpreter and a back end that compiles through C
are built and tested on Linux, macOS and Windows (x86-64 and Arm); what is written here can still change.

- **The language**: integers of every width, `float` and `double`, `char` and UTF-8 text, structs, enums
  and `match`, `T?` and `Result<T, E>`, modules, consts computed when the program compiles, statics,
  default values of fields and parameters, arrays, slices, lists, maps and sets.
- **Memory safety without a collector**: one owner for each value, `read` and `ref` borrows checked when
  the program compiles, `shared` values with several owners; `unsafe` marks raw memory and C.
- **Tasks**: threads and `async`/`await`, locks, channels and atomics with the orders of C11.
- **C**: calling C and being called by it, C's layouts (unions, bit fields, `packed`), and libraries of SG
  that C links. SQLite, libcurl and zlib are wrapped in modules whose users write no `unsafe`.
- **A small library**: files, folders and paths, other programs, TCP and UDP, signals, time and time
  zones, random numbers, Unicode with its normal forms, hex and Base64, CRC-32, SHA-256 and HMAC, the command line, JSON, a log,
  regular expressions whose patterns are checked when the program compiles, gzip and zlib with zlib's
  bytes, and zip archives that refuse Zip Slip and bombs before they are read.
- **No operating system needed**: a program can be built freestanding, and a small kernel for RISC-V boots
  in QEMU.
- **Tools**: `sg test`, a debug build that reports leaks, `sg lsp` with an extension for VS Code, and packages
  without a registry: `sg add` fetches them by `git`, pinned by the SHA-256 of their files in `sg.lock`.

Not yet: generics of your own and a full standard library. Left out on purpose:
closures; a function value is the address of a named function.

Every test program runs in the interpreter and compiled, under the sanitizers, and the two must print the
same; the front end was fuzzed, generated programs that borrow are run both ways, and the rules of safety
were tried with mutants. The [status page](https://sg-lang.org/docs/status)
says where it stands in short; [decisions and open questions](design/04-decisions-and-open-questions.md) and
[the choices of every slice](design/decisions/README.md) keep the full record, day by day.

The plan is to write a well-known, widely used program in it, one that has not been rewritten in
Rust yet, and see how the claim holds on real software. The program is
[gifsicle](https://www.lcdf.org/gifsicle/), the C tool that shrinks and edits GIF images: its work is
bytes and bits, so it can be measured against the C original, and it has had a heap overflow of the kind
SG rules out. It is taken up when the language is further along.

## What is in this repository

| Folder | What it holds |
|---|---|
| [design](design/README.md) | The language itself, on paper, and [every choice made while building it](design/decisions/README.md), slice by slice |
| [interpreter](interpreter/README.md) | The code, in Rust: checker, interpreter, C back end, tests, and the programs measured against C; [its documents](interpreter/docs/README.md) |
| [benchmarks](benchmarks/README.md) | The measurements: SG against C, Rust and Zig, where it is still slower, and how it is measured |
| [trials](trials/README.md) | Programs people write (`grep`, `wc`, ...) and the modules of the library (JSON, compression, zip, SHA-256 and CRC-32 so far) in SG, Rust and Zig, measured against each other ([the numbers](benchmarks/README.md)) |
| [topics](topics) | Background: essays on C, Rust, Zig and Go, and on the problem SG sets out to solve |
| [sources](sources/reading-list.md) | Reading list: books, Hylo, Vale, Rust writings, tools |
| [website](website/README.md) | The web site, [sg-lang.org](https://sg-lang.org), built with Docusaurus |
| [editors](editors) | The extension for VS Code, which runs `sg lsp` |
| [brand](brand) | The logo, its icons and how they are made; Pasha, the mascot, and Pasha's badge |

If you read one design document, read
[decisions and open questions](design/04-decisions-and-open-questions.md): every decision with
its reason and its cost, what is still open, and what comes next. The others:

| File | Contents |
|---|---|
| [Example programs](design/05-examples.md) | Twelve programs written before there was a compiler, and the questions they raised |
| [From Rust to SG](design/06-rust-to-sg.md) | What a Rust programmer writes and what SG writes for it, with how far each line is built; updated with every slice |
| [Syntax draft](design/02-syntax-draft.md) | The ownership notation, and the earlier drafts that were dropped |
| [Memory model](design/01-memory-model.md) | The first model (generation-checked pointers), kept for the record; it was dropped |
| [Roadmap](design/03-roadmap.md) | Where to start when designing a language, the steps and the traps |

The background essays, best read in order:

| File | Contents |
|---|---|
| [C and manual memory](topics/01-c-and-manual-memory.md) | Why memory management was left to the developer |
| [History of Rust ownership](topics/02-rust-ownership-history.md) | The roots of the idea, why it took 40 years, why C++ could not do it |
| [Zig](topics/03-zig.md) | Where it stands on safety, how long the move from C takes |
| [Rust syntax and C#](topics/04-rust-syntax-and-csharp.md) | Where the noise comes from, the "second C++?" question |
| [Go](topics/05-go.md) | Its philosophy, its weak sides |
| [Safe C attempts](topics/06-safe-c-attempts.md) | From Cyclone to C3, the layers of pointer safety |
| [Use-after-free](topics/07-use-after-free.md) | The explicit error idea, why a simple table is not enough |

## How it got here

SG started on 4 November 2026 from a handful of questions about C, Rust, Zig, Go and C#: why C left
memory to the programmer, why ownership took forty years to arrive, and whether its
guarantees could be had in a smaller language. The name is the author's initials; source files
end in `.sg`.

The first design used generation-checked pointers and arenas. It was dropped a day later for
Rust's ownership rules, which cost nothing when the program runs. Over the next week twelve
example programs were written on paper, and each question they raised was settled and written
down. On 11 November the goal was put in one line, performance and simplicity like C, and the
design was made smaller to fit it.

The first code followed on 12 November: an interpreter in Rust, built one slice of the
language at a time, then the borrow check, then a back end that compiles through C, with every
compiled program compared against the interpreter and run under the sanitizers. No single
language is the model. The ownership rules are Rust's, the notation is close to C#'s, modules
work as in Go, tests are written as in Zig.

The full record, slice by slice, is under "Next steps" in
[decisions and open questions](design/04-decisions-and-open-questions.md).

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
