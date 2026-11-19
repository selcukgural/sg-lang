---
title: Library
description: The types that come with SG, each with its methods, their signatures and programs that run.
sidebar_position: 0
---

# Library

SG's library is small, and comes in two parts.

**Built into the language** are the types the compiler knows by themselves: the numbers, `bool`, `char` and
`string`; `T?` and `Result<T, E>`; arrays, slices, `List<T>`, `HashMap<K, V>` and `HashSet<T>`; `File`,
`StreamReader`, `StreamWriter` and the standard streams; `Task<R>`, `Channel<T>`, `shared locked T`, `atomic T`
and cancellation; `Memory` and the types of C. Their methods are checked by the compiler itself, which is how
it can say, for example, that `name.trim()` alone does nothing.

**The module `std`** is the part written in SG: the clock (`Duration`, `Instant`, `DateTime`,
`DateTimeOffset`), random numbers (`Random`, `SecureRandom`), hex and Base64 (`Hex`, `Base64`), CRC-32 (`Crc32`), folders and paths (`Directory`, `Path`,
`FileInfo`), other programs (`Command`, `Process`), `Environment`, `Signals`, `MemoryOrder`, and the network
beyond a TCP connection. It comes with the compiler. A program gets it by naming one of its types, from every
module, with nothing written: `use std` is an error whose hint is to take the line away. Only `std` itself can
reach the calls of the system under it, so a program always goes through a type that gives the call its
meaning.

```sg
void main() {
    var pause = Duration.fromMilliseconds(250)
    var r = Random.fromSeed(7)
    var path = Path.join("logs", "today.txt")
    var roll = r.nextInt(1, 7)
    print($"{pause} {path} {roll}")
}
```

It prints, on Linux and macOS:

```text
250ms logs/today.txt 5
```

## How the pages are written

Each page takes a few types. For each type there is a paragraph on what it is for, then a table of its methods
with their signatures as SG writes them, what each one does and what it gives or panics with when it cannot do
its work, and short programs that run. Each program is checked: it runs, and prints what is shown under it, in
the interpreter and compiled.

In a signature, `read this` means the method looks at its value and `ref this` that it changes it; a method
with neither is called on the type (`Duration.fromSeconds(1)`). A `Result<T, E>` is a value or an error, and a
`T?` a value or `null`. The ideas behind them are in the [book](../learn/book/01-getting-started.md).

These pages are written by hand for now. Later they will be made from the source by `sg doc`, as Rust's
documentation and Go's pkg.go.dev are: a page for each type, from the comments written in the code, with a link
from each item to its source.

## The pages

- [Text](./string.md): `string`, `read string` and `char`, searching, splitting, case, building text and
  writing values into it.
- [Numbers](./numbers.md): the integer and fractional types, their constants, conversions, checked, wrapping
  and saturating arithmetic, bits, math, `parse` and bytes in either order.
- [Collections](./collections.md): `T[N]`, slices, `List<T>`, `HashMap<K, V>` and `HashSet<T>`.
- [Files and folders](./files.md): `File`, `StreamReader` and `StreamWriter`, `Stdin`, `Stdout` and `Stderr`,
  `Directory`, `Path`, `FileInfo` and `IoError`.
- [Time](./time.md): `Duration`, `Instant`, `DateTime`, `DateTimeOffset` and the clocks under them.
- [Tasks](./tasks.md): `Task<R>`, `Channel<T>` and `Sender<T>`, `shared locked T`, atomics and cancellation.
- [Random numbers](./random.md): `Random`, with a seed, and `SecureRandom`.
- [Programs and the system](./process.md): `Command`, `Process`, `ExitStatus`, `Environment` and `Signals`.

The network (`TcpListener`, `TcpStream`, `UdpSocket`, `SocketAddress`) does not have a page yet.
