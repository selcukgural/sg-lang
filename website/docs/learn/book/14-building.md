---
title: Building programs
description: Debug and release builds, the C that SG writes, libraries, the three systems, machines without an operating system, and the editor.
sidebar_position: 14
---

# Building programs

`sg file.sg` runs a program in the interpreter, which is the reference: it checks the most and needs no C
compiler unless the program calls C. `sg build` compiles the same program through C into a native program
that prints the same. This chapter is about that second path. Setting up `sg` and a C compiler is in
[Install](../install.md).

## Debug and release builds

```bash
sg build main.sg                 # a debug build: ./main
sg build main.sg -o tool         # the same, named tool
sg build main.sg --release       # optimised
sg build shop                    # a folder is a program too: ./shop
```

A **debug build** is compiled without optimisation (the C compiler's `-O0`) and checks what the interpreter
checks: integer overflow, a cast that does not fit, a shift past the bits of the type, an item of raw memory
at an address that is not aligned for its type, and, when `main` returns, any block of memory the program took and never gave back
(`shared` values that hold each other up are named; raw memory is the program's own business). A **release build** is optimised (`-O2`) and leaves those
checks out: overflow wraps, as in Rust, a cast keeps the bits that fit, and nothing is counted.

This program goes past the end of a `byte`. The interpreter stops it:

```sg panic
void main() {
    byte level = 250
    for _ in 0..10 {
        level += 1
    }
    print(level)
}
```

```text
panic: overflow: 255 + 1 does not fit in byte
  --> example.sg:4:9
 4 |         level += 1
   |         ^^^^^^^^^^
  hint: `tryAdd` gives `null` instead, and `wrappingAdd` wraps around
```

A debug build stops it with the same message, and a release build wraps:

```bash
$ sg build example.sg && ./example
panic: overflow: 255 + 1 does not fit in byte
  ...
$ sg build example.sg --release && ./example
4
```

What keeps memory safe is checked in every build: an index or a range outside a list or a text, division by
zero, a conflict of borrows on a `shared checked` value, `assert` and `panic`. When a program means to wrap or
to look, it says so, and then every build agrees:

```sg
void main() {
    byte level = 250
    print(level.wrappingAdd(10))
    print(level.tryAdd(10) ?? 0)
    print(level.saturatingAdd(10))
}
```

It prints:

```text
4
0
255
```

Another optimisation level is a flag of C (below). Deep recursion ends in a panic in both, `stack overflow`,
when the stack of 1 GB is full. The interpreter looks at what is left before each call and names the call;
compiled code, which would pay for a look on every call, runs on to the guard page of its stack, as C does,
and says so there. A call takes the interpreter about 2.5 KB of its stack, so it goes about 380,000 calls
deep, where compiled code goes past a million.

## The C that SG writes

`sg emit-c main.sg` prints the C that `sg build` compiles (`--release` for a release build). It is plain C11,
for reading, for a sanitizer, or for a build with flags of your own:

```bash
sg emit-c main.sg --release > main.c
cc -std=c11 -O3 -ffp-contract=off main.c -o main -lm -pthread
```

`-ffp-contract=off` keeps the C compiler from fusing a multiply and an add, so fractional results stay the same
as the interpreter's; `-lm` and `-pthread` bring C's math functions and threads. When the program names a
library in an `extern` block, the first line of the C says what to link (`/* sg-link: -lz */`).

`sg build` takes flags of C too, after `--`, on every kind of build; they come after sg's own, so the last
written wins:

```bash
sg build main.sg --release -- -O3 -march=native -flto
sg build main.sg -- -g -fsanitize=address
```

A flag that changes a promise of SG is still given, with a warning on every build: one that changes fractional
results (`-ffast-math`, `-Ofast`), one that takes a guard away (`-fno-stack-clash-protection`, an executable stack),
and a setting of SG's runtime (`-DSG_...`):

```text
warning: -ffast-math: fractional results may differ from the interpreter's and between machines
```

What sg decides itself is refused before anything is compiled: the output and what is made (`-o`, `-c`,
`-shared`), the standard of C (`-std=`), the machine (`--target`), and what changes the size of C's types
(`-m32`, `-funsigned-char`, `-fshort-enums`). A flag of C turns off no check: a debug build with `-O3` still stops
on an overflow, and only `--release` leaves the checks out.

Tests are compiled too: `sg test main.sg --compiled` builds and runs them, each in a process of its own, and
`sg build main.sg --test -o tests` keeps the program that runs them.

A debug build carries what the debugger of its system reads, on the lines of the `.sg` files: that is
[Debugging a program](./15-debugging.md).

## Libraries of C: `-L`

A library an `extern` block names (`extern "sqlite3" { }`) is looked for where the C compiler looks. When it
is somewhere else, `-L folder` names a folder to look in first, for the interpreter (before the file), for
`sg build` and for `sg test`:

```bash
sg -L ./vendor/lib main.sg
sg build main.sg -L ./vendor/lib -o main
```

The folder is written into the program (`rpath` on Linux and macOS), so it runs without `LD_LIBRARY_PATH`.
When a library is not found, the interpreter and the build both name it, with the folder to give:

```text
error: the library `nosuchlibrary` was not found
  looked for with `-lnosuchlibrary` where the C compiler `cc` looks
```

`sg build folder --library` and `--shared` build a library that a program of C links, with its header; that is
in [Working with C](./11-c.md).

## Linux, macOS and Windows

- **Linux**: the C compiler is `cc`, GCC or Clang. A build adds `-fstack-clash-protection`, so a function with
  a large frame cannot start below its stack, in another thread's memory.
- **macOS**: `cc` is Apple's Clang. A shared library is a `.dylib`. Apple's Clang compiles only for Apple's own
  machines, so building for another one (below) takes LLVM's Clang, named in `CC`.
- **Windows**: the C compiler is LLVM's Clang with the Build Tools of Visual Studio, and `CC` may name another
  Clang. A program gets `.exe`, carries C's library inside it so it needs nothing installed, reads and writes
  text as UTF-8, takes long paths, and has 1 GB of stack for `main` and each task, as on the other systems. What a debugger
  reads is beside the program, in `main.pdb`. Windows has no `rpath`, so `sg build` copies the DLLs it
  linked from the `-L` folders beside the program. C's `long` is 32 bits there, which is why SG writes it
  `CLong`. In an `if` on `OperatingSystem.isWindows()`, a function of C of the other system is not linked, so
  one file calls each system's own.

## Without an operating system

`sg build folder --freestanding` gives an object, `folder.o`, for a kernel, a boot loader or a device. It has no
`main`: the code that starts the machine calls its `export` functions. Three functions connect it to the
machine:

- `void onPanic(read string message)` receives the message of a panic, which it writes where it can be seen;
  after it the processor waits.
- `raw byte allocate(nuint size, nuint align)` and `void release(raw byte block, nuint size, nuint align)`
  give it memory. A program without them allocates nothing, and the build checks that statement by statement.

What needs a system (`print`, files, the clock, tasks, `HashMap`) is a compile error where it is written:

```text
$ sg build device.sg --freestanding
error: `print` writes to an output of the process, and this program is built without an operating system
  a program built with `--freestanding` runs where there is no operating system: what needs one is done through the machine itself, with raw memory and `Memory.writeVolatile`, or through a C file beside the program
```

and so is a list in a program that has no `allocate`, whose hint gives the two signatures. The object takes
nothing of C's library: what the runtime needs of it is written in, and `memcpy` and its kin are weak functions
of its own.

The repository has two such programs:

- **`examples/bare`** keeps a log of sensor readings in a `List`, writes through a `board_write` function the
  machine gives, and takes its memory from a block the machine hands it, with a small allocator written in SG.
  Its "machine" is a short C program that calls `start`, `report` and then `readingAt` past the end of the
  readings, so the panic reaches `onPanic` and stops the machine with code 101. It runs on Linux, macOS and
  Windows, and the log's tests run as any others with `sg test`.
- **`examples/kernel`** is a kernel for RISC-V that QEMU's `virt` machine boots with no firmware. It writes to
  the serial port, takes five interrupts of the timer 10 ms apart, counting them in a `static atomic ulong`,
  and powers the machine off. The serial port, the timer and the power device are driven in SG with
  `Memory.readVolatile` and `writeVolatile`; what SG has no words for (the start of the core, the vector of its
  traps, its registers and `wfi`) is in `machine.c`, compiled into the same object through an
  `extern "./machine.c"` block.

`--target` names the machine, and what follows `--` goes to the C compiler:

```bash
sg build examples/kernel --freestanding --target riscv64-unknown-elf -o kernel.o \
    -- -mcmodel=medany -march=rv64gc -mabi=lp64d
clang --target=riscv64-unknown-elf -march=rv64gc -mabi=lp64d -nostdlib -fuse-ld=lld \
    -Wl,--gc-sections -T examples/kernel/kernel.ld kernel.o -o kernel.elf
qemu-system-riscv64 -machine virt -bios none -m 128M -nographic -kernel kernel.elf
```

With `--target`, the build uses Clang (`CC`, or the first `clang` on the path), which compiles for every
machine. Each function lies in a section of its own, so `--gc-sections` leaves out what nothing calls. The
kernel prints:

```text
SG on RISC-V: a kernel in machine mode on QEMU's virt machine
the heap has 127 MB
the timer interrupts every 10 ms
tick 1
tick 2
tick 3
tick 4
tick 5
memory: 6 block(s) taken, 6 given back
powering off
```

## The editor

`sg lsp` is the language server an editor starts. As you type, it shows the first error of the program the file
belongs to, with its notes, its hint and a link to the second place it names, and reads what the editor has not
saved. Some hints are fixes applied with one click: a conversion `(int)x`, `.clone()` of a text that is only
borrowed, `_ = ` before a value dropped on purpose, and a missing `use`. The extension for VS Code
(`editors/vscode`) brings colours, brackets, comments and indentation, and starts `sg lsp`; how to install it is
in [Install](../install.md).

## Help

`sg help read` prints the page of a word in the terminal: its forms, what it means, examples from the simplest
to the hardest, and what the compiler refuses. It is carried in `sg` itself, so it needs no network, and it is
the same text as the page on the site and the manual page. `sg help index` lists every name of SG with what it
is in one line, and `sg help` the commands and the words that have a page. A sign is found by its marks alone:
`sg help ??`. Each command prints its own page with `--help`: `sg build --help` is `sg help sg-build`.
