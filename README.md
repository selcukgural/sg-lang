# SG

<p align="center">
  <img src="brand/pasha.png" alt="Pasha, a white Van cat with one blue and one amber eye, curled up in a circle" width="240">
  <br>
  <em>Pasha, the mascot of SG</em>
</p>

SG is a small systems language: about as small and as fast as C, memory safe like Rust, and
written in a notation that reads like C#.

It is still being designed. There is a checker, an interpreter and a back end that compiles
through C, all of them a few days old. Nothing is released and everything can change. This
repository is the design, the code and the background reading behind both, kept in the open.

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
and both go through the same C compiler with `-O2`. Some of the results (time of SG divided by
time of C; 1.00 is the same speed):

| Program | SG / C |
|---|---|
| Recursive `fib(40)` | 0.98 |
| A linked list of a million nodes, built, walked and freed | 1.01 |
| A list of a million numbers, filled, changed, added up | 1.02 |
| Three hundred million lookups that give back a borrow or nothing | 1.01 |
| Counting in a hash table | 1.20 |
| Printing two million structs | 0.57 |

The hash table is slower because it uses a keyed hash by default, which protects against
crafted keys; with a plain hash it is 0.99. These are small programs on one machine, not
proof. All the numbers, and how they were taken, are in the
[interpreter README](interpreter/README.md#the-c-back-end).

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

Working today, in the interpreter and in compiled code: integers, `float` and `double` with their
constants, math, reading out of text and printing with the fewest digits that read back, `bool`, `char` and text (split, trimmed and searched without
copying, replaced and repeated into a new text, built piece by piece in place, numbers written with a width, in hex or binary, or with so many
digits after the point: `$"{crc:x8} {price,8:F2}"`); structs, enums with methods, and `match` with patterns inside
patterns, on text, joined with `or` and in ranges (`Add(Number(0), right)`, `"if" => ...`, `'a'..='z' or '_'`,
`null`), and `is` whose names reach along `&&` (`if int.parse(read text) is Ok(n) && n > 0`) and, for `is not`,
past an `if` that leaves (`if int.parse(read text) is not Ok(n) { return }`); `T*`, `T?`
(narrowed along `&&` and `||`, under `!` and after `while x is null`) and `Result` with `try` and `??`; `List`, `HashMap` and `HashSet`; fixed-size arrays (`int[8]`) and slices (`read byte[]`), with bytes
gathered into a list in one copy (`buffer.addAll(piece)`), numbers read out of bytes and written into them in
either order (`ushort.fromLittleEndian(data[6..8])`, `n.toBigEndian()`), and ranges sorted where they are (`xs[2..5].sort()`);
function values; ownership, moves and the borrow check; several owners of one value (`shared T`, `share x`,
and `weak T`, which looks without owning), changed through any of them with `shared checked T` (each access
checked against the borrows in progress, a certain conflict a compile error, the values that hold each
other up reported when the program ends) and type aliases (`using NodeRef = shared Node`);
modules (`use`, `public`, `readonly`); `test` blocks. A local variable nothing reads is a compile error, also one only written
(`var p = rect.corner; p.x = 0` writes to a copy, and the hint is `var p = ref rect.corner`), with `_` for a name that is not needed.

Programs read and write files whole or piece by piece, as text or as bytes, and append to them (a writer left open is a compile error), read
their input line by line and see their command line. Tasks (`Task.run`, `t.wait()`, every task waited for) and
values that several tasks change (`shared locked T`, a `lock` block that cannot deadlock) run in the
interpreter and in compiled code, where a task is a thread of the system. Async functions in the way of
C# (`async`, `await`, `Task.delay`, waiting the wrong way a compile error), cancellation tokens that a
task looks at where it chooses, lists of tasks waited for all at once or as they end (`Task.whenAll`,
`Task.whenAny`), and async reading and writing of files and the input, streams included, run in the
interpreter and in compiled code, where an async function becomes a machine whose variables live in a
frame, run by a pool of threads that each keep a queue of tasks and steal from the others when they have none, as Tokio's do, and input and output the async way runs on threads of its own. TCP
connections (`TcpListener`, `TcpStream`, with and without async) run in both too: compiled, a thread of
the pool with nothing to do waits for every socket a task waits for, with epoll or kqueue, as Tokio's
driver, each socket put on the loop once, and a cancel ends a wait for a socket or for a line of the input at once. A connection is read line by line through a `StreamReader` that borrows it
while it goes on sending, and a line longer than 64 KB is an error, not memory without end. Raw memory is
there for what the checker cannot follow (`raw T`, `Memory.allocate<T>(n)`, `p[i]` inside `unsafe { }`):
the interpreter checks every item against the block it came from, and compiled code checks nothing. C is
called through `extern` blocks, each call inside `unsafe`, with C text (`CString`) and the items of a buffer
handed over for the call, structs of plain data by value and types of C seen only behind a pointer; a
module can keep a library of C behind a safe interface, as `examples/zlib` does for zlib, and a
`Disposable` type gives back what C made when its last owner ends. A C file beside the program
(`extern "./native.c"`) is compiled with it, so a macro, a `static inline` function or assembly is
reached and inlined; the variables of C (`optind`) are read and written inside `unsafe`, `errno` is read
after a call (`IoError.fromErrno(C.errno())`), C's `long` is a type of its own that is 32 bits on Windows and 64 elsewhere and has no arithmetic, so no sum overflows on one system only (`CLong`), and an address is a number when it has to be (`(nuint)p`). C calls functions of SG back (`export int compare(raw void a, raw void b)`), through pointers to functions of C that go both ways (`CFunc<raw void, raw void, int>`), and a callback reaches its state through the `void*` C gives back (`unsafe (ref State)data`). A function of C that takes a varying count of arguments is called with the types each value says (`printf`, `snprintf`), and SQLite and libcurl are wrapped in modules whose users write no `unsafe` (`examples/sqlite`, and `examples/curl`, which fetches over HTTPS with only `http` and `https` allowed, certificates always checked and a limit on what a response brings); the bytes C hands a callback are looked at as a slice (`Memory.view`), and a call of C that waits holds up no other task. The promise of memory safety holds outside `unsafe`, as in Rust. The clock is there (`Instant`, `Duration`, and `DateTime` in UTC), written in SG itself, the first piece of a standard library that is otherwise still the compiler's built-in types, and so are random numbers (`Random` with a seed, `SecureRandom` from the system), folders and paths (`Directory`, `File.move` that never writes over a file, `Path`, and `Path.joinInside`, which keeps a name from outside inside its folder on every system), the variables of the environment, which are only read, and other programs, run without a shell, waited for or talked to through pipes while they run, and stopped (`Command`, `Process`, with async versions); a method can be async. A program listens for signals and takes them one at a time instead of writing a handler (`Signals.listen`, `next`, `nextAsync`), so a server ends cleanly on Ctrl-C (`examples/shutdown`). What C lays out is written as C has it: unions, bit fields, `packed` structs and `align(64)`, laid out as C lays them out on each system, Windows' rules included, and read out of bytes with no `unsafe` (`Header.fromBytes`, `examples/headers`: a BMP header and an IPv4 header); an item of raw memory reached where its type may not lie is a panic in a debug build. Tasks share atomic values (`atomic long`, `count.fetchAdd(1)`, `flag.load(MemoryOrder.Acquire)`) with the orders of C11, an order an operation cannot keep being an error of the compiler, and a queue without locks for many producers and many consumers is written with no `unsafe` (`examples/queue`); the registers of a device are read and written with `Memory.readVolatile` and `writeVolatile`. A folder of SG is built into a static or a shared library that a program of C links, with its header (`sg build calclib --library`): C holds a value of SG by its address and lends it back (`export int calc_eval(ref Calculator calc, read CString text, raw double out)`), with no `unsafe` for what C gives back, and `examples/calclib` is a calculator in SG used by a program of C. A program is built for a machine without an operating system (`sg build kernel --freestanding --target riscv64-unknown-elf`): an object that takes nothing of C's library, whose panics go to the program's `onPanic` and whose memory comes from its `allocate`, what needs a system being a compile error where it is written (`examples/bare`), and a small kernel for RISC-V boots in QEMU, writes to its serial port and takes interrupts of the timer, its devices driven in SG and only the instructions of the core in C (`examples/kernel`). A character is tested against the tables of Unicode 17.0 (`isLetter`, `isUpper`) and a text changes its case by Unicode's full mappings (`toUpper`, `equalsIgnoreCase`), the same in every country. Datagrams are sent and received over UDP (`UdpSocket`), with an address read and printed in SG (`SocketAddress`). A name never hides another: a local variable with the name of one still in scope around it, or of a parameter, is a compile error, as in C#, not a second variable as with Rust's shadowing. Generics of your own are put off. There is no full standard library,
no package manager and no editor support.

No preview was to come out while there was work that C can do and SG cannot. With the kernel, each of the
seven programs set as that threshold runs and is tested (design/04, "Reaching what C reaches, the direction"):
SQLite, libcurl, a server that ends on Ctrl-C, a library of SG for C, the kernel, a queue without locks and
a binary header read with a union and bit fields. SG is to run on Linux, macOS and Windows: Linux and macOS came first,
and nothing was built in a way Windows cannot take. How Windows is reached has been talked through (design/04,
"Windows, the direction"): Windows 10 and 11 on x86-64 and 64-bit Arm, tested on GitHub's machines with Windows,
two slices of what every system gains first (both are written: C's `long`, a name kept inside its folder, the
codes of 32 bits a program of Windows ends with, the system's last error; then `-L folder`, a library named by
each of its names on the systems, tests run without `fork`, and a debug build that counts the blocks of its
values and reports a leak on every system), then six slices of Windows, and a preview of the language after
them. The first slice of Windows is written: the interpreter runs there, and `sg build` builds with Clang a
program that prints to the console in UTF-8 without touching its code page, reads and writes files, lists,
moves and deletes them as on Linux, reads the clock, random bytes, its command line and the environment, and
runs its tests; a program can ask which system it is built for (`OperatingSystem.isWindows()`), and what
Windows does not run yet is a compile error that names the slice that brings it.

The plan is to write a well-known, widely used program in it, one that has not been rewritten in
Rust yet, and see how the claim holds on real software. The program is
[gifsicle](https://www.lcdf.org/gifsicle/), the C tool that shrinks and edits GIF images: its work is
bytes and bits, so it can be measured against the C original, and it has had a heap overflow of the kind
SG rules out. It is taken up when the language is further along.

## What is in this repository

| Folder | What it holds |
|---|---|
| [design](design) | The language itself, on paper |
| [interpreter](interpreter/README.md) | The code, in Rust: checker, interpreter, C back end, tests and benchmarks |
| [topics](topics) | Background: essays on C, Rust, Zig and Go, and on the problem SG sets out to solve |
| [sources](sources/reading-list.md) | Reading list: books, Hylo, Vale, Rust writings, tools |

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
picture of Pasha, the mascot (`brand/pasha.png`). The Unicode tables
(`interpreter/src/unicode_tables.rs`) are made from the Unicode Character Database, under the
Unicode License v3, whose notice is at the top of that file.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion
in this work by you, as defined in the Apache-2.0 license, shall be dual licensed as above,
without any additional terms or conditions.
