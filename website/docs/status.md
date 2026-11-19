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
- **Tasks**: tasks that are always waited for, `async` and `await`, locks with a condition, channels, and
  atomic values with the orders of C11.
- **C**: calling C and being called by it, C's types laid out as C lays them out, and libraries of SG that C
  links. SQLite, libcurl and zlib are wrapped in modules whose users write no `unsafe`.
- **A library**: files, folders and paths, other programs, the network (TCP and UDP), signals, time and time
  zones, random numbers.
- **A machine without a system**: a program built for one (`--freestanding`), and a small kernel for RISC-V.
- **Tools**: `sg test`, a debug build that counts every block of memory and reports leaks, and `sg lsp` for
  editors.

## How it is tested

Every program in the repository's tests runs in the interpreter and compiled, under the address,
undefined-behaviour and thread sanitizers, and the two must print the same. Before the preview, what is built
was also reviewed for soundness, its front end fuzzed, generated programs run both ways, and the functions
that make the rules of safety tried with mutants; each finding was fixed after a test that shows it.

## Not yet

Generics of your own, a package manager and a full standard library. Closures are left out on purpose: a
function value is the address of a named function. On this site, the reference of
the language is still to be written, and the pages of the library are written by hand until `sg doc` makes them
from the source. The repository keeps the full list:
[Not here yet](https://github.com/selcukgural/sg-lang/blob/main/interpreter/README.md#not-here-yet).
