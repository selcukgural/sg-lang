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
| `-c ERROR` | 0.046 s, 2.1 MB | 0.045 s, 2.1 MB | 0.066 s, 2.8 MB | 0.060 s, 3.4 MB | 0.061 s, 3.6 MB |
| `-n timeout` | 0.051 s, 2.2 MB | 0.050 s, 2.2 MB | 0.070 s, 2.8 MB | 0.069 s, 3.5 MB | 0.072 s, 3.6 MB |
| `-c -i error` | 0.075 s, 2.1 MB | 0.077 s, 2.1 MB | 0.086 s, 3.0 MB | 0.068 s, 3.5 MB | 0.070 s, 3.6 MB |
| `-c '[a-z]+[0-9]+@example\.com (PUT\|DELETE)'` | 0.133 s, 2.2 MB | 0.155 s, 2.2 MB | 0.148 s, 3.1 MB | 0.129 s, 3.5 MB | 0.131 s, 3.6 MB |
| `'in [0-9]{4} ms: .*日本'` | 0.061 s, 2.2 MB | 0.063 s, 2.2 MB | 0.110 s, 3.2 MB | 0.073 s, 3.5 MB | 0.073 s, 3.6 MB |
| `-v -c INFO` | 0.049 s, 2.1 MB | 0.048 s, 2.1 MB | 0.069 s, 2.8 MB | 0.062 s, 3.4 MB | 0.063 s, 3.6 MB |
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
a checked build still does (0.224 s). In the hundred-and-sixty-fifth the address is searched from its inner
text, `@example.com `, as Rust's `regex` does: what comes before it is read backwards to where a match starts
and the pattern forwards from there, so a line is read around its address and not from its start. It takes
0.133 s, from 0.189 s, ahead of Rust's 0.148 s; PCRE2's JIT, which turns the pattern into machine code, takes
0.129 s. The other cases were measured again in the same run and moved by a millisecond or two at most.

Rust's `\w` and `\d` are Unicode's unless `(?-u)` is written, while SG's are ASCII's as RE2's, and PCRE2
backtracks where SG and Rust never do: the patterns measured mean the same in all three.

## jq

The jq people use (`trials/jq/README.md`, choices 1804-1822), held byte for byte to jq 1.8.2 by 44 cases of
`measure.py jq --check` (every build gives jq's output and exit code), by a suite of 3,681 cases written with
the Rust version and one of 6,128 written with the Zig version. jq 1.8.2 (Homebrew) and jaq 3.2.0 are
measured beside them; jaq is another algorithm and is not held to jq's output. Apple M3 Max, macOS, 24
November 2026; 100 MB of records, of a record a line and of numbers from
[`trials/inputs/json.sg`](../trials/inputs/json.sg) (seed 1), 1 GB of records, and the inputs that break
things written by `measure.py`; the best of five.

| Case | SG | SG, checked | Rust | Zig | Zig, safe | jq 1.8.2 | jaq 3.2.0 |
|---|---|---|---|---|---|---|---|
| records, written back | 1.121 s, 614.0 MB | 1.205 s, 810.5 MB | 2.369 s, 1031.6 MB | 0.878 s, 588.1 MB | 1.036 s, 639.0 MB | 4.505 s, 1183.0 MB | 1.585 s, 1216.1 MB |
| select and an object | 0.911 s, 614.1 MB | 1.003 s, 810.7 MB | 2.157 s, 1031.7 MB | 0.741 s, 588.3 MB | 0.904 s, 639.1 MB | 1.496 s, 1131.4 MB | 1.077 s, 1216.3 MB |
| group_by | 1.305 s, 654.0 MB | 1.531 s, 863.0 MB | 2.433 s, 1060.1 MB | 1.027 s, 654.9 MB | 1.242 s, 710.0 MB | 2.135 s, 1155.4 MB | 1.136 s, 1270.5 MB |
| sort_by | 1.231 s, 650.8 MB | 1.335 s, 859.9 MB | 2.467 s, 1060.5 MB | 1.427 s, 654.4 MB | 1.694 s, 709.6 MB | 1.940 s, 1152.1 MB | 1.217 s, 1241.3 MB |
| csv, raw | 0.962 s, 614.0 MB | 1.056 s, 810.5 MB | 2.270 s, 1031.6 MB | 0.763 s, 588.4 MB | 0.940 s, 639.2 MB | 1.615 s, 1146.0 MB | 1.226 s, 1216.3 MB |
| counted by city | 0.970 s, 613.9 MB | 1.071 s, 810.5 MB | 2.210 s, 1031.6 MB | 0.753 s, 588.2 MB | 0.919 s, 639.2 MB | 1.965 s, 1129.4 MB | 0.874 s, 1220.4 MB |
| every number added | 3.048 s, 325.1 MB | 3.262 s, 444.1 MB | 3.365 s, 864.9 MB | 0.980 s, 589.2 MB | 1.154 s, 628.6 MB | 6.063 s, 1545.0 MB | 4.613 s, 1043.4 MB |
| a record a line | 0.806 s, 3.8 MB | 0.875 s, 3.9 MB | 0.851 s, 3.8 MB | 0.665 s, 3.1 MB | 0.828 s, 3.1 MB | 1.311 s, 3.3 MB | 0.851 s, 99.7 MB |
| 1 GB written back | 10.783 s, 6085.3 MB | 11.581 s, 8052.9 MB | 21.408 s, 10288.8 MB | 8.369 s, 5842.9 MB | 10.048 s, 6350.3 MB | 33.739 s, 11852.7 MB | 13.301 s, 12125.4 MB |
| ten million lines out | 0.662 s, 2.7 MB | 0.710 s, 2.7 MB | 0.397 s, 3.0 MB | 0.464 s, 2.3 MB | 0.542 s, 2.5 MB | 1.930 s, 3.1 MB | 8.448 s, 4.3 MB |
| a text of 100 MB | 0.303 s, 222.1 MB | 0.336 s, 222.1 MB | 0.130 s, 331.0 MB | 0.214 s, 112.6 MB | 0.244 s, 112.8 MB | 0.702 s, 222.4 MB | 0.106 s, 337.1 MB |
| an object of ten million | 3.410 s, 1067.8 MB | 3.469 s, 1068.2 MB | 3.566 s, 1468.1 MB | 2.025 s, 1764.1 MB | 2.332 s, 1831.6 MB | 3.560 s, 2313.5 MB | 1.284 s, 1477.8 MB |
| ten million sorted | 0.112 s, 157.1 MB | 0.159 s, 157.2 MB | 0.237 s, 310.2 MB | 0.261 s, 309.1 MB | 0.379 s, 385.6 MB | 1.244 s, 1417.5 MB | 0.174 s, 413.2 MB |
| added to, a million times | 0.099 s, 19.8 MB | 0.113 s, 19.8 MB | 0.103 s, 20.3 MB | 0.071 s, 19.2 MB | 0.078 s, 19.5 MB | 0.259 s, 95.2 MB | 0.133 s, 34.8 MB |
| set by name, a million times | 0.640 s, 122.0 MB | 0.692 s, 122.0 MB | 0.374 s, 126.8 MB | 0.333 s, 123.3 MB | 0.412 s, 125.2 MB | 1.017 s, 164.5 MB | 0.431 s, 138.3 MB |
| a def a million deep | 0.200 s, 109.9 MB | 0.230 s, 156.3 MB | 0.088 s, 125.6 MB | 0.084 s, 156.2 MB | 0.094 s, 156.4 MB | 0.253 s, 150.3 MB | fails |
| a def ten million in tail | 1.226 s, 2.4 MB | 1.340 s, 2.4 MB | 0.395 s, 3.0 MB | 0.434 s, 2.3 MB | 0.470 s, 2.5 MB | 1.723 s, 3.1 MB | 1.437 s, 4.4 MB |
| nested ten million deep | 0.804 s, 1073.5 MB | 0.992 s, 1536.6 MB | 1.046 s, 1074.2 MB | 0.699 s, 1847.4 MB | 0.782 s, 2001.4 MB | 1.608 s, 309.6 MB | fails |
| Size, stripped | 472 KB | 521 KB | 2255 KB | 636 KB | 765 KB | — | — |
| Lines | 6323 | 6323 | 4927 | 5456 | 5456 | — | — |

What it shows:

- **SG is ahead of jq 1.8.2 in every case and of Rust in all but five**; Rust's `serde_json` builds a
  `Value` of its own before the program's, where SG's `JsonReader` (1814) and Zig's `Scanner` hand the
  tokens straight to the program. **Zig is ahead of SG in most cases**, by 20 to 30% where records are read
  and written; SG holds less memory in the object of ten million (1,068 MB against 1,764 MB) and in ten
  million sorted.
- **Where SG is behind** (in [Slower than C](slower-than-c.md)): a call of a `def` (the two cases of `def`, 1.2 s against 0.4 s in tail position, and
  `set by name`, 0.64 s against 0.37 s), the numbers of `every number added` (Zig works out a `double` the first time it is
  wanted, which the others do not, 1819), ten million lines written (0.66 s against Rust's 0.40 s), and a
  text of 100 MB, which SG holds twice while it is read (222 MB against Zig's 113 MB).
- **Found and fixed in the slice, from these cases**: a `shared` value was copied whole at each change
  (`makeUnique`, 1813: `added to, a million times` did not end in 300 s); `sort` merged pairs of one item,
  then two, whatever the order (it now merges the runs the items make: ten million sorted 1.16 s → 0.11 s);
  a value of SG's `jq` was 32 bytes where Rust's and Zig's are 16 (1 GB written back: 7.1 GB → 6.1 GB);
  `[f]` kept the room it grew by (nested ten million deep: 2.1 GB → 1.1 GB); `..` was jq's own `def` (every
  number added: 4.6 s → 3.0 s; Rust and Zig walk it with a stack of their own, and so does SG now).
- **jaq** fails where a value is nested ten million deep and where a `def` is a million deep (its stack
  overflows), writes another output or exit code in 9 of the 44 cases of the check (bad UTF-8, a byte order
  mark, numbers past a `double`, an error in the middle, 10,001 levels), and panics before it starts when a
  variable of the environment is not UTF-8 (`measure.py` sets one for another trial; jaq runs without it).
- The sizes are stripped; the lines are the trial's own, without comments. SG's version is the longest: it
  writes its own reader of numbers (decNumber's rules) and Perl's `$` for `test`, which Rust and Zig take
  from their libraries.
