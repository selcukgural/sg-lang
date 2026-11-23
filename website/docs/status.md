---
title: Status
description: Where SG stands before its preview.
---

# Status

SG is before its preview. The language, its checker, an interpreter and a back end that compiles through C
are built and tested on Linux, macOS and Windows; what is written here can still change.

## What is there

- **The language**: integers of every width with checked overflow, `float` and `double`, `char` and text in
  UTF-8, structs, enums with values and `match`, `T?` and `Result<T, E>`, modules, consts computed when the
  program compiles, statics, arrays, slices, lists, maps and sets.
- **Memory safety without a collector**: one owner for each value, `read` and `ref` borrows checked when the
  program compiles, `shared` values with several owners, and `shared checked` values changed through any of them
  with a check when the program runs. `unsafe` marks raw memory and C.
- **Tasks**: tasks that are always waited for, groups of tasks with a limit and a graceful end, `async` and
  `await`, cancellation with a time that can be set again, locks with a condition, channels, and atomic
  values with the orders of C11.
- **C**: calling C and being called by it, C's types laid out as C lays them out, and libraries of SG that C
  links. SQLite, libcurl and zlib are wrapped in modules whose users write no `unsafe`.
- **A library**: files, folders and paths, other programs, the network (TCP and UDP), signals, time and time
  zones, random numbers, Unicode, hex and Base64, CRC-32 and Adler-32, SHA-256 and HMAC, the command line,
  JSON, a log, regular expressions, compression (DEFLATE, zlib and gzip), and zip archives.
- **A machine without a system**: a program built for one (`--freestanding`), and a small kernel for RISC-V.
- **Tools**: `sg test`, a debug build that counts every block of memory and reports leaks, `sg lsp` with an
  extension for VS Code, and packages without a registry: `sg add` fetches them by `git`, pinned by the SHA-256
  of their files in `sg.lock`.

## How it is tested

Every program in the repository's tests runs in the interpreter and compiled, under the address,
undefined-behaviour and thread sanitizers, and the two must print the same. Before the preview, what is built
was also reviewed for soundness, its front end fuzzed, generated programs run both ways (programs that
borrow among them, the checker refusing what breaks a rule and what it takes read under the address
sanitizer), and the functions that make the rules of safety tried with mutants; each finding was fixed after
a test that shows it.

Now it is tried on programs people write: each is written in SG, in Rust and in Zig, the three are measured
against each other, and each difference is a choice that is fixed and measured again. `grep` and `wc` were
the first; `jq` came next, held byte for byte to jq 1.8.2 and pushed as far as the world will (a text of
100 MB, ten million members, 10,001 levels, bad UTF-8, numbers past a `double`), and it brought the
language a reader of JSON's tokens, `Clonable`, copy on write for `shared` values and a stack of 1 GB that
says when it overflows. The
[trials](https://github.com/selcukgural/sg-lang/blob/main/trials/README.md) keep the programs, and
[Benchmarks](./benchmarks/index.md) their numbers.

Each module of the library is measured the same way, against the crates Rust's people use and Zig's own
library, and a new module comes in with its measurement. JSON was the first: with its values made a third of
their size, SG reads, walks and writes 100 MB of it faster than `serde_json` and Zig's `std.json`, in less
memory. Compression and zip came next: SG opens gzip faster than `flate2`'s default and Zig, compresses in
their time with zlib's bytes, reads zip archives faster than the `zip` crate and Zig, and lists 100,000
entries in less memory than the `zip` crate; `flate2` with `zlib-rs` compresses faster by another algorithm. SHA-256, HMAC, CRC-32 and Adler-32 followed: a compiled program hands them to the processor's own
instructions where it has them, and takes at most 1.3 times the time of Rust and Zig, where it took six to
nine. Then hex and Base64, made five to seven times faster, to between 1.25 and 1.54 times the faster of Rust
and Zig and three times as fast as Rust's `hex`; and the categories and normal forms of Unicode, as fast as
ICU4X or faster. Then time, random numbers and the log: dates are written in the time of `chrono`, where they
took 2.4 times as long, and numbers and lines of a log in the time of Rust and Zig; a moment in a time zone,
which SG asks of the C library each time, takes four times `chrono`'s. Then the network, against `std::net`,
`tokio` and Zig: in their time where the functions block, 0.92 to 1.06 times the best of them the async way, but
1.25 to 1.30 times `tokio` where the two ends talk inside one program, 1,000 connections at once in the memory of
`tokio`'s, and addresses read and written in the time of Zig and faster than Rust. Last came the command line,
the environment and paths: SG's programs are the smallest, a tenth of the size of Rust's and Zig's, and a
million paths are built and taken apart faster than Rust and Zig. On the network SG is never to
be worse than the best rival, under load or not. Servers are measured on Linux against Rust, Zig and C
([the tables](benchmarks/server.md)): a store that speaks a part of Redis's protocol and an HTTP server hold less
memory in SG than in Rust and Zig in all but two cases, 100,000 idle connections at 4.96 KB each, and since each
thread of the pool has a loop of the network of its own, 50 connections get 1.49 times the answers of Zig's
threads and 0.91 times C's. At 70% of the load the tail is C's, not `tokio`'s, and what is still slower is
listed. Under a load that rises until they break, SG's servers answer as much as Rust's and nearly as much as
C's, and keep answering past that point where C's do not; against clients that reset, close half, trickle, do
not read, send what is not the protocol or outnumber the descriptors they stand
([what the servers stand](benchmarks/robustness.md)). The HTTP server is now written on the language's own
[`std/http`](library/http.md), which reads HTTP/1.1 to the rules of RFC 9112, 2.2% behind the server written by hand before it.

## Not yet

Generics of your own and a full standard library. Closures are left out on purpose: a
function value is the address of a named function. On this site, the reference of
the language is still to be written, and the pages of the library are written by hand until `sg doc` makes them
from the source. The repository keeps the full list:
[Not here yet](https://github.com/selcukgural/sg-lang/blob/main/interpreter/docs/not-here-yet.md).

## What comes next

A well-known program written in SG, one that has not been rewritten in Rust yet:
[gifsicle](https://www.lcdf.org/gifsicle/), measured against its C original
([how it got here, and what comes next](./about/history.md#what-comes-next)).
