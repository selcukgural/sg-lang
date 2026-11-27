# How it is measured

Every number in these pages is taken by a script in this repository, so anyone can take it again. This page says how, on what, and by which rules.

## Fairness

No measurement is made, chosen or reported so that it favours SG. In particular:

- **The other languages are written as their own people write them.** Rust and Zig get the idiomatic
  program and the library people use for the job, in their fastest ordinary build (`--release`,
  `ReleaseFast`); nothing is slowed down, left unbuffered or written the long way. When a faster ordinary way
  is shown for Rust or Zig, it is taken and everything is measured again, also when SG loses by it.
- **The same work.** The same algorithm where one is named, the same input, the same output byte for byte,
  checked before anything is timed; a case where the programs do different work (a check one skips, a
  Unicode rule one ignores) is said beside the numbers or made the same.
- **Every case is published.** The cases are chosen before the numbers are seen, and none is dropped,
  changed or put out of sight because SG loses it; a case where SG loses stays in the table, and the
  difference goes into [Slower than C](slower-than-c.md) with what would fix it.
- **SG as it ships.** SG is measured in its release build with every check of safety it has by default, and
  once more with its arithmetic checked. Nothing is turned off, and nothing is tuned for a measurement alone
  (1543): a change counts only where it serves every program.
- **Said plainly.** The machine, the compilers and their versions, the build flags, how time, memory and
  size are taken, and what is left out (a shared library Zig loads) are written beside every table, and a
  difference that comes from the tools rather than the language (another C compiler, another LLVM) is
  named as such.

For the programs against C: the C is the ordinary program a C programmer writes, written by hand with no
checks; every program measured is shown, also where SG loses; and SG keeps every check it has by default.

## The rival and the yardstick

What SG is held to is **safe Rust doing the same work**: SG is not to be slower (choice 2078, 27 November 2026).
Rust gives the guarantee SG gives, memory kept safe by the language, and pays for it where a check has to run
while the program does. C gives none and pays for none, so "not slower than C" cannot hold at every place where
safety needs a check.

C is still what most tables here are measured against. SG compiles through C, so a program of C written by hand is
the ceiling of what SG can reach, and the distance to it shows what safety costs. A place where SG is slower than C
is read by what safe Rust does there:

- safe Rust pays the same: the price of the guarantee;
- safe Rust does not: ours to mend, in the code SG writes or in its library.

[Slower than C](slower-than-c.md) says of each place which it is, as the places are looked at again; nothing was
measured again for it. Where a mature program of Rust does the job (a regular expression, an HTTP server, zip), Rust
is the rival in the table. Where the program that exists is C (a port of a tool of C), SG is measured against that,
and the cause of a difference is written beside it; nothing new is written in Rust only to have a rival.

## Against C

`interpreter/bench/run.py` compiles each program of [`interpreter/bench/`](../interpreter/bench) three
ways: SG as a release build (overflow wraps), SG with overflow checked, and the C written by hand. All three
go through the C compiler `sg build` uses (`cc` on Linux and macOS, Clang on Windows) with `-O2`, without
fused multiply-add (SG's rule for fractional numbers). It prints the best of five runs of each.

The tables were taken on an Apple M3 Max with macOS and Apple clang 21 on arm64, and on Windows 11 on 64-bit
Arm in a virtual machine on the same Mac ([SG against C on Windows](against-c-windows.md) says how that one
was run).

## Against Rust and Zig

`trials/measure.py` checks that SG, Rust and Zig give the same output, byte for byte, then measures them.
The time is the best of five runs after one to warm up, the memory the most the program held (its peak
resident set), the size that of the program stripped, and the lines those that are neither blank nor only
a comment. SG is built with `sg build --release` and once more with its arithmetic checked (the C of
`sg emit-c` at `-O2`); Rust with `cargo build --release`; Zig twice (1542), `ReleaseFast` and
`ReleaseSafe`, which keeps its checks of bounds and overflow. `std/compression` builds Rust once more with
`flate2`'s feature `zlib-rs`, which people turn on for speed (1632). No LTO and no profile. The size of Zig's
`grep` leaves out PCRE2, which it loads as a shared library (about 650 KB).

Apple M3 Max, macOS, 20 November 2026; 100 MB of text from `trials/inputs/text.sg` (seed 1). SG is compiled by
the system's `cc` (Apple clang 21), Rust by its own LLVM (23.1), Zig 0.17.1 (LLVM too) with PCRE2 10.49.

The modules of `std` were measured on the same machine on 21 November 2026, with macOS 27.2, Apple clang
21.0, rustc 1.99.1 and Zig 0.17.1.

## Running them again

From the `interpreter` folder, on Linux, macOS or Windows:

```
python3 bench/run.py                  # every program against C
python3 bench/run.py fib nbody        # only these
python3 bench/run.py --runs 1 spawn   # one run instead of the best of five
```

From the repository's root, with Rust, Zig 0.17, PCRE2 and a C compiler installed:

```
python3 trials/measure.py               # every trial: checked, then measured
python3 trials/measure.py grep          # one trial
python3 trials/measure.py std/json      # one module of std
python3 trials/measure.py --check       # only that the outputs are the same
```
