# Benchmarks

SG's promise is software that is safe and runs as fast as C. These pages are where that is measured: the
numbers, how they were taken, and where SG is still slower. A case SG loses is shown as plainly as one it
wins ([the rules](method.md#fairness)).

## The pages

| Page | What it holds |
|---|---|
| [SG against C](against-c.md) | 61 small programs, each written in SG and in C by hand, on macOS |
| [SG against C on Windows](against-c-windows.md) | The same programs on Windows 11 on Arm |
| [Programs against Rust and Zig](programs.md) | `wc`, `grep` and `jq`, written in SG, Rust and Zig |
| [The library against Rust and Zig](library.md) | The modules of `std` against Rust's crates and Zig's library: JSON, compression, zip, hashes, checksums, hex and Base64, Unicode, time, random numbers, the log, the network |
| [Servers against Rust, Zig and C](server.md) | A store that speaks Redis's protocol and an HTTP server, on Linux, under one load: answers a second, p99 and p99.9, memory for each connection, idle and under load, the breaking point, and a run of 30 minutes |
| [What the servers stand](robustness.md) | The same servers against clients that reset, close half, send a byte at a time, do not read, send what is not the protocol, outnumber the descriptors, and SIGTERM |
| [Slower than C, and could be better](slower-than-c.md) | Every place where SG was found slower, and what would fix it |
| [How it is measured](method.md) | The rules of fairness, the machines, the compilers, and how to run it all again |

## At a glance: against C

The time of SG divided by the time of the same program in C, both built by the same C compiler with `-O2`;
1.00 is the same speed. Of 61 programs, 37 are within 5% of C, 13 are faster and
11 are slower ([all of them](against-c.md)).

| Program | SG / C |
|---|---|
| Recursive `fib(40)` | 0.98 |
| A linked list of a million nodes, built, walked and freed | 1.01 |
| A list of a million numbers, filled, changed, added up | 1.02 |
| Three hundred million lookups that give back a borrow or nothing | 1.01 |
| Counting in a hash table, with a keyed hash against crafted keys | 1.20 |
| A tree of folders with several owners each (`shared checked`, `weak`) | 1.53 |
| Printing two million structs | 0.47 |

## At a glance: against Rust and Zig

Every case of every trial and module, SG's release build against the fastest of Rust and Zig in their
release builds; below 1 SG is faster. SG is faster in 8 of the 25 cases, slower in 14. The
memory each held, the size of each program, and the builds with checks are on the pages of each.

| Area | Case | SG | Fastest of the others | SG / fastest |
|---|---|---|---|---|
| wc | 100 MB | 0.088 s | 0.067 s (Rust) | 1.31 |
| wc | 100 MB from the input | 0.088 s | 0.067 s (Rust) | 1.31 |
| grep | `-c ERROR` | 0.046 s | 0.060 s (Zig) | 0.77 |
| grep | `-n timeout` | 0.051 s | 0.069 s (Zig) | 0.74 |
| grep | `-c -i error` | 0.075 s | 0.068 s (Zig) | 1.10 |
| grep | `-c '[a-z]+[0-9]+@example\.com (PUT\|DELETE)'` | 0.133 s | 0.129 s (Zig) | 1.03 |
| grep | `'in [0-9]{4} ms: .*日本'` | 0.061 s | 0.073 s (Zig) | 0.84 |
| grep | `-v -c INFO` | 0.049 s | 0.062 s (Zig) | 0.79 |
| json | Records, read and summed | 0.495 s | 0.548 s (Zig) | 0.90 |
| json | Records, read and written back | 0.645 s | 0.716 s (Zig) | 0.90 |
| json | A document a line, each read and summed | 0.412 s | 0.471 s (Zig) | 0.87 |
| json | Coordinates, read and summed | 0.363 s | 0.413 s (Rust) | 0.88 |
| compression | 100 MB of a log, compressed | 1.616 s | 0.949 s (Rust, `zlib-rs`) | 1.70 |
| compression | 100 MB of a log, opened | 0.084 s | 0.086 s (Rust, `zlib-rs`) | 0.98 |
| compression | 99,640 records, each compressed and opened | 1.562 s | 1.551 s (Rust) | 1.01 |
| compression | 100 MB that do not compress, compressed | 1.483 s | 1.267 s (Rust, `zlib-rs`) | 1.17 |
| zip | 100 MB in 10,001 entries, written | 1.068 s | 0.770 s (Rust) | 1.39 |
| zip | 10,001 entries, read | 0.139 s | 0.183 s (Rust) | 0.76 |
| zip | 100,000 entries, listed | 0.027 s | 0.008 s (Zig) | 3.38 |
| hash | 100 MB of a log, hashed | 0.049 s | 0.041 s (Zig) | 1.20 |
| hash | 10,000,000 hashes, chained | 0.331 s | 0.284 s (Zig) | 1.17 |
| hash | 921,952 lines, each signed | 0.138 s | 0.113 s (Zig) | 1.22 |
| checksum | 100 MB of a log, CRC-32 | 0.013 s | 0.010 s (Rust) | 1.30 |
| checksum | 100 MB of a log, Adler-32 | 0.024 s | 0.022 s (Rust) | 1.09 |
| checksum | 921,952 lines, each a CRC-32 | 0.044 s | 0.035 s (Rust) | 1.26 |

## Where the code is

| Folder | What is in it |
|---|---|
| [`interpreter/bench/`](../interpreter/bench) | The programs against C, `name.sg` and `name.c`, and `run.py`, which measures them |
| [`trials/`](../trials/README.md) | The programs and the modules of `std` in SG, Rust and Zig, and `measure.py`, which measures them |

How to run them again is at the end of [How it is measured](method.md#running-them-again).
