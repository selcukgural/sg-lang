# Trials

Programs that people write, written in SG, in Rust and in Zig, the same algorithm each time and written
plainly, then measured against each other (choices 1418, 1540). Rust and Zig are the languages in SG's
lane: compiled ahead of time, no collector, memory held and freed by the program; Go, measured in the first
trial, was left for them. Each difference found is a numbered choice in
the [interpreter's README](../interpreter/README.md), fixed and measured again.

| Trial | What it is | Slice |
|---|---|---|
| [wc](wc) | The lines, words and bytes of files, read in pieces of 64 KB | 133 |
| [grep](grep) | The lines a regular expression matches, with `-c`, `-i`, `-n` and `-v` | 133 |

Still to come, in this order: a `jq` of JSON, a key-value store with a write-ahead log, a pool of workers,
an HTTP/1.1 file server under load, a GIF tool, and an interpreter of Lox.

## How they are written (1525-1532)

- Each trial has a folder with `sg/`, `rust/` and `zig/`. Rust uses its standard library and the crates
  people use for the job (`regex`), pinned in `Cargo.lock`, and threads rather than an async runtime. Zig
  uses its standard library, and a library of C where Zig has none, as Zig programs do: `grep` links
  PCRE2 (with its JIT) through `build.zig` (1541). SG uses only what comes with the language.
- The three give the same output, byte for byte, on the same input; their errors are worded each in its
  own language's way. `tests/cli.rs` (`the_trials`) runs the SG versions interpreted and compiled on a
  megabyte and holds them to what Rust and Zig print.
- Nothing large is kept here: [`inputs/text.sg`](inputs/text.sg) makes lines of a log from a seed, the
  same bytes on every machine.

## Measuring

From the repository's root, with Rust, Zig 0.17, PCRE2 and a C compiler installed:

```
python3 trials/measure.py               # every trial: checked, then measured
python3 trials/measure.py grep          # one trial
python3 trials/measure.py --check       # only that the outputs are the same
```

The time is the best of five runs after one to warm up, the memory the most the program held (its peak
resident set), the size that of the program stripped, and the lines those that are neither blank nor only
a comment. SG is built with `sg build --release` and once more with its arithmetic checked (the C of
`sg emit-c` at `-O2`); Rust with `cargo build --release`; Zig twice (1542), `ReleaseFast` and
`ReleaseSafe`, which keeps its checks of bounds and overflow. No LTO and no profile. The size of Zig's
`grep` leaves out PCRE2, which it loads as a shared library (about 650 KB).

## Results

Apple M3 Max, macOS, 20 November 2026; 100 MB of text from `inputs/text.sg` (seed 1). SG is compiled by
the system's `cc` (Apple clang 21), Rust by its own LLVM (23.1), Zig 0.17.1 (LLVM too) with PCRE2 10.49.

### wc

| Case | SG | SG, checked | Rust | Zig | Zig, safe |
|---|---|---|---|---|---|
| 100 MB | 0.089 s, 1.9 MB | 0.089 s, 1.9 MB | 0.071 s, 2.0 MB | 0.101 s, 2.0 MB | 0.111 s, 2.2 MB |
| 100 MB from the input | 0.092 s, 1.9 MB | 0.090 s, 1.9 MB | 0.067 s, 2.0 MB | 0.098 s, 2.0 MB | 0.111 s, 2.2 MB |
| Size, stripped | 50 KB | 50 KB | 352 KB | 345 KB | 442 KB |
| Lines | 70 | 70 | 73 | 62 | 62 |

The difference with Rust is the C compiler, not SG: the same C built through Homebrew's clang 23.1
(`CC=... sg build`, which 1533 made possible on Linux and macOS) takes 0.043 s.

### grep

| Case | SG | SG, checked | Rust | Zig | Zig, safe |
|---|---|---|---|---|---|
| `-c ERROR` | 0.063 s, 2.0 MB | 0.064 s, 2.0 MB | 0.068 s, 2.8 MB | 0.061 s, 3.4 MB | 0.064 s, 3.6 MB |
| `-n timeout` | 0.086 s, 2.0 MB | 0.087 s, 2.0 MB | 0.072 s, 2.8 MB | 0.072 s, 3.5 MB | 0.072 s, 3.6 MB |
| `-c -i error` | 0.162 s, 2.0 MB | 0.170 s, 2.0 MB | 0.087 s, 3.0 MB | 0.071 s, 3.5 MB | 0.072 s, 3.6 MB |
| `-c '[a-z]+[0-9]+@example\.com (PUT\|DELETE)'` | 0.233 s, 2.0 MB | 0.238 s, 2.0 MB | 0.152 s, 3.1 MB | 0.131 s, 3.4 MB | 0.133 s, 3.6 MB |
| `'in [0-9]{4} ms: .*日本'` | 0.069 s, 2.0 MB | 0.070 s, 2.1 MB | 0.109 s, 3.2 MB | 0.076 s, 3.5 MB | 0.077 s, 3.6 MB |
| `-v -c INFO` | 0.052 s, 2.0 MB | 0.052 s, 2.0 MB | 0.073 s, 2.8 MB | 0.065 s, 3.4 MB | 0.068 s, 3.6 MB |
| Size, stripped | 164 KB | 164 KB | 1684 KB | 362 KB | 442 KB |
| Lines | 101 | 101 | 145 | 137 | 137 |

Before this slice, `Regex` ran Pike's machine on every line: `-i error` took 1.16 s, the address 2.06 s and
`日本` 0.83 s. An inner text every match holds (1535), a lazy DFA (1536) with its cache kept in the `Regex`
(1538), and the same for where a match is (1539) brought them to the numbers above. Go's `regexp` took 1.13 s and 1.33 s on the two hardest. What is left behind Rust and Zig:
`-i` looks for `error` in any case a byte at a time where Rust's `regex` uses the processor's vector
instructions, and the DFA's loop checks each read of its table against the table's end; PCRE2's JIT turns the
pattern into machine code, and its search for a pattern that starts with a class or a word in any case
goes as fast as a search for text.

Rust's `\w` and `\d` are Unicode's unless `(?-u)` is written, while SG's are ASCII's as RE2's, and PCRE2
backtracks where SG and Rust never do: the patterns measured mean the same in all three.
