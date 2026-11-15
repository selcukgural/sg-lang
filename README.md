# SG

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
frame, run by a pool of threads, and input and output the async way runs on threads of its own. TCP
connections (`TcpListener`, `TcpStream`, with and without async) run in both too: compiled, one thread
waits for every socket a task waits for, with epoll or kqueue, and a cancel ends a wait for a socket or for
a line of the input at once. A connection is read line by line through a `StreamReader` that borrows it
while it goes on sending, and a line longer than 64 KB is an error, not memory without end. Raw memory is
there for what the checker cannot follow (`raw T`, `Memory.allocate<T>(n)`, `p[i]` inside `unsafe { }`):
the interpreter checks every item against the block it came from, and compiled code checks nothing. C is
called through `extern` blocks, each call inside `unsafe`, with C text (`CString`) and the items of a buffer
handed over for the call, structs of plain data by value and types of C seen only behind a pointer; a
module can keep a library of C behind a safe interface, as `examples/zlib` does for zlib, and a
`Disposable` type gives back what C made when its last owner ends. The promise of memory safety holds outside `unsafe`, as in Rust. The clock is there (`Instant`, `Duration`, and `DateTime` in UTC), written in SG itself, the first piece of a standard library that is otherwise still the compiler's built-in types, and so are random numbers (`Random` with a seed, `SecureRandom` from the system), folders and paths (`Directory`, `File.move` that never writes over a file, `Path`) and the variables of the environment, which are only read. Generics of your own are put off. There is no UDP yet, no full standard library,
no package manager and no editor support.

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
program written in SG is yours, under whatever license you choose.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion
in this work by you, as defined in the Apache-2.0 license, shall be dual licensed as above,
without any additional terms or conditions.
