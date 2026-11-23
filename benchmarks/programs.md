# Programs against Rust and Zig

Programs that people write, written in SG, in Rust and in Zig, the same algorithm each time and written
plainly. Their code is in [`trials/`](../trials/README.md), which also says how they are written; how they
are measured is in [How it is measured](method.md).

Apple M3 Max, macOS, 20 November 2026; 100 MB of text from [`trials/inputs/text.sg`](../trials/inputs/text.sg) (seed 1). SG is compiled by
the system's `cc` (Apple clang 21), Rust by its own LLVM (23.1), Zig 0.17.1 (LLVM too) with PCRE2 10.49.

## wc

| Case | SG | SG, checked | Rust | Zig | Zig, safe |
|---|---|---|---|---|---|
| 100 MB | 0.088 s, 1.9 MB | 0.089 s, 1.9 MB | 0.067 s, 2.0 MB | 0.098 s, 2.0 MB | 0.108 s, 2.2 MB |
| 100 MB from the input | 0.088 s, 1.9 MB | 0.089 s, 1.9 MB | 0.067 s, 2.0 MB | 0.099 s, 2.0 MB | 0.107 s, 2.2 MB |
| Size, stripped | 50 KB | 50 KB | 352 KB | 345 KB | 442 KB |
| Lines | 70 | 70 | 73 | 62 | 62 |

The difference with Rust is the C compiler, not SG: the same C built through Homebrew's clang 23.1
(`CC=... sg build`, which 1533 made possible on Linux and macOS) takes 0.043 s.

Measured again after 1543-1546 (the slice after the first), and after 1549-1550, which changed nothing of
`wc`'s work (it reads blocks, not lines): the numbers above are of those runs, the same within a
millisecond.

## grep

| Case | SG | SG, checked | Rust | Zig | Zig, safe |
|---|---|---|---|---|---|
| `-c ERROR` | 0.045 s, 2.0 MB | 0.045 s, 2.0 MB | 0.065 s, 2.8 MB | 0.060 s, 3.4 MB | 0.060 s, 3.6 MB |
| `-n timeout` | 0.051 s, 2.1 MB | 0.050 s, 2.1 MB | 0.068 s, 2.8 MB | 0.068 s, 3.5 MB | 0.070 s, 3.6 MB |
| `-c -i error` | 0.076 s, 2.0 MB | 0.078 s, 2.0 MB | 0.086 s, 3.0 MB | 0.069 s, 3.5 MB | 0.071 s, 3.6 MB |
| `-c '[a-z]+[0-9]+@example\.com (PUT\|DELETE)'` | 0.189 s, 2.1 MB | 0.224 s, 2.0 MB | 0.149 s, 3.1 MB | 0.128 s, 3.5 MB | 0.129 s, 3.6 MB |
| `'in [0-9]{4} ms: .*日本'` | 0.060 s, 2.2 MB | 0.061 s, 2.1 MB | 0.106 s, 3.2 MB | 0.072 s, 3.5 MB | 0.073 s, 3.6 MB |
| `-v -c INFO` | 0.048 s, 2.0 MB | 0.046 s, 2.0 MB | 0.068 s, 2.8 MB | 0.062 s, 3.4 MB | 0.065 s, 3.6 MB |
| Size, stripped | 180 KB | 164 KB | 1684 KB | 362 KB | 442 KB |
| Lines | 101 | 101 | 145 | 137 | 137 |

`-n timeout` prints 136,194 lines: it took 0.086 s, against Rust's and Zig's 0.072 s, until the slice that
followed this one. A line is now written into a block of 64 KB of the program's own, piece by piece where
it is printed, and no longer through C's `fwrite` and its lock with its format read at run time
(1544-1546): printing costs SG 4 ms over the search, as it costs Rust. The search was what was left
(`-c timeout` 0.072 s, Rust 0.065 s), until the slice after that: a plain text is now looked for by its two
rarest bytes, sixteen places at a time (1549), where `memchr` stopped at every `t`, and UTF-8 is checked by
going straight to the first byte past ASCII (1550). `-c timeout` takes 0.049 s (Rust 0.066 s, Zig 0.066 s,
the best of seven, not a case of the table), `-n timeout` 0.052 s and `-c ERROR` 0.046 s; reading the lines
is 0.033 s of them. `-i` and the address are still behind, for the reasons below.

Before the first slice of the trials, `Regex` ran Pike's machine on every line: `-i error` took 1.16 s, the address 2.06 s and
`日本` 0.83 s. An inner text every match holds (1535), a lazy DFA (1536) with its cache kept in the `Regex`
(1538), and the same for where a match is (1539) brought them to 0.150 s and 0.228 s. Go's `regexp` took 1.13 s and 1.33 s on the two hardest.
In the hundred-and-sixty-fourth slice `-i` looks for `error` sixteen places at a time with the processor's
vector instructions, as Rust's `regex` does, and a pattern that is only a text in any case runs no DFA; the
DFA's loop reads its table without checking the index against the table's end in a release build (1803), which
a checked build still does (0.224 s). What is left behind Rust and Zig on the address: SG's DFA reads each line
from its start, where Rust's finds `@example.com ` first and reads back and forth from it; PCRE2's JIT turns the
pattern into machine code, and its search for a pattern that starts with a class or a word in any case goes
as fast as a search for text.

Rust's `\w` and `\d` are Unicode's unless `(?-u)` is written, while SG's are ASCII's as RE2's, and PCRE2
backtracks where SG and Rust never do: the patterns measured mean the same in all three.
