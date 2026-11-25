# Programs against Rust and Zig

Programs that people write, written in SG, in Rust and in Zig, the same algorithm each time and written
plainly. Their code is in [`trials/`](../trials/README.md), which also says how they are written; how they
are measured is in [How it is measured](method.md). From slice 180 on a trial is written in SG alone and
measured against the C program it mirrors: [lox](#lox) against clox. The showcase, [gifsicle](#gifsicle) written in SG,
is measured against gifsicle itself.

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

Measured again on 24 November 2026 (slice 174), after the search in any case was made faster. Since slice 173
the three read a line as bytes and search it as bytes, so that a line that is not UTF-8 is written as it is
(choice 1840): SG with `readLineBytes` and `isMatchBytes`, Rust with `read_until` and `regex::bytes`, Zig with
PCRE2's `MATCH_INVALID_UTF`.

| Case | SG | SG, checked | Rust | Zig | Zig, safe |
|---|---|---|---|---|---|
| `-c ERROR` | 0.036 s, 2.0 MB | 0.036 s, 2.0 MB | 0.043 s, 2.8 MB | 0.036 s, 3.4 MB | 0.037 s, 3.6 MB |
| `-n timeout` | 0.044 s, 2.2 MB | 0.044 s, 2.1 MB | 0.046 s, 2.8 MB | 0.045 s, 3.5 MB | 0.047 s, 3.6 MB |
| `-c -i error` | 0.042 s, 2.0 MB | 0.048 s, 2.0 MB | 0.063 s, 3.0 MB | 0.047 s, 3.5 MB | 0.048 s, 3.6 MB |
| `-c '[a-z]+[0-9]+@example\.com (PUT\|DELETE)'` | 0.127 s, 2.1 MB | 0.139 s, 2.1 MB | 0.128 s, 3.1 MB | 0.105 s, 3.4 MB | 0.106 s, 3.6 MB |
| `'in [0-9]{4} ms: .*日本'` | 0.049 s, 2.2 MB | 0.052 s, 2.2 MB | 0.086 s, 3.2 MB | 0.049 s, 3.5 MB | 0.049 s, 3.6 MB |
| `-v -c INFO` | 0.037 s, 2.0 MB | 0.038 s, 2.0 MB | 0.046 s, 2.8 MB | 0.039 s, 3.4 MB | 0.040 s, 3.6 MB |
| Size, stripped | 181 KB | 181 KB | 1684 KB | 362 KB | 442 KB |
| Lines | 104 | 104 | 145 | 146 | 146 |

`-c -i error` took 0.065 s in slice 173 (checked 0.066 s), 1.44 times Zig's. It looked for the first two bytes
of `error` sixteen places at a time, and `er` is found about once a line; it now looks for the two rarest,
`rr`, found only where `error` is, and a last block that overlaps the one before it takes the place of the
places looked at one at a time at the end of each line. Reading the lines is 0.033 s of the 0.042 s. The
other cases moved by a millisecond at most.

Each line is no longer checked to be UTF-8, which each of the three did before searching it. Beside the
SG of the slice before, built from its own source the same day, best of nine: `-c ERROR` 0.044 → 0.034 s,
`-n timeout` 0.052 → 0.044 s, `-c -i error` 0.078 → 0.064 s, the address 0.136 → 0.133 s, `日本` 0.060 →
0.050 s, `-v -c INFO` 0.048 → 0.036 s, the same output. PCRE2's JIT gained most on the address, which it now
searches with no check of the bytes either: Zig is ahead there, 0.104 s against SG's 0.127 s and Rust's
0.129 s. SG's search of a `string`, which now checks the bytes it reads past ASCII, takes what it took: the
old `grep`, built by the compiler of this slice, is within a millisecond of what the old compiler gives.

The table before this slice, of 20 November, with `-i` and the address as the slices after it left them
(measured on 23 November):

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
`measure.py jq --check` (every build gives jq's output and exit code), by the two suites of
`trials/jq/tests`, 3,687 cases written with the Rust version and 6,389 with the Zig version, each version
passing both. jq 1.8.2 (Homebrew) and jaq 3.2.0 are
measured beside them; jaq is another algorithm and is not held to jq's output. Apple M3 Max, macOS, 24
November 2026, after slice 171; 100 MB of records, of a record a line and of numbers from
[`trials/inputs/json.sg`](../trials/inputs/json.sg) (seed 1), 1 GB of records, and the inputs that break
things written by `measure.py`; the best of five.

| Case | SG | SG, checked | Rust | Zig | Zig, safe | jq 1.8.2 | jaq 3.2.0 |
|---|---|---|---|---|---|---|---|
| records, written back | 1.019 s, 713.2 MB | 1.119 s, 959.2 MB | 2.170 s, 1114.2 MB | 0.866 s, 557.7 MB | 1.027 s, 608.7 MB | 4.495 s, 1183.0 MB | 1.568 s, 1216.1 MB |
| select and an object | 0.888 s, 713.3 MB | 0.991 s, 959.3 MB | 2.057 s, 1114.2 MB | 0.726 s, 557.9 MB | 0.893 s, 608.8 MB | 1.478 s, 1131.4 MB | 1.062 s, 1216.3 MB |
| group_by | 1.326 s, 753.2 MB | 1.557 s, 1011.6 MB | 2.348 s, 1142.8 MB | 1.018 s, 624.5 MB | 1.237 s, 679.6 MB | 2.137 s, 1155.4 MB | 1.174 s, 1270.5 MB |
| sort_by | 1.435 s, 750.1 MB | 1.506 s, 1008.4 MB | 2.468 s, 1143.2 MB | 1.122 s, 624.0 MB | 1.326 s, 679.2 MB | 1.926 s, 1152.2 MB | 1.210 s, 1241.3 MB |
| csv, raw | 0.931 s, 713.3 MB | 1.036 s, 959.3 MB | 2.143 s, 1114.3 MB | 0.764 s, 557.8 MB | 0.930 s, 608.8 MB | 1.604 s, 1145.9 MB | 1.216 s, 1216.3 MB |
| counted by city | 0.958 s, 713.2 MB | 1.089 s, 959.3 MB | 2.093 s, 1114.2 MB | 0.748 s, 557.8 MB | 0.904 s, 608.8 MB | 1.950 s, 1129.4 MB | 0.865 s, 1220.3 MB |
| every number added | 2.033 s, 859.0 MB | 2.366 s, 1206.1 MB | 2.438 s, 965.8 MB | 0.975 s, 588.9 MB | 1.169 s, 628.2 MB | 6.019 s, 1545.1 MB | 4.563 s, 1043.4 MB |
| a record a line | 0.782 s, 3.8 MB | 0.880 s, 3.8 MB | 0.800 s, 3.8 MB | 0.658 s, 3.1 MB | 0.815 s, 3.2 MB | 1.307 s, 3.3 MB | 0.844 s, 99.7 MB |
| 1 GB written back | 9.711 s, 7080.6 MB | 10.813 s, 9540.0 MB | 19.643 s, 11115.0 MB | 8.293 s, 5536.8 MB | 9.907 s, 6028.8 MB | 33.841 s, 11852.7 MB | 13.394 s, 12125.4 MB |
| ten million lines out | 0.151 s, 2.5 MB | 0.134 s, 2.6 MB | 0.390 s, 3.1 MB | 0.165 s, 2.2 MB | 0.206 s, 2.5 MB | 1.912 s, 3.1 MB | 8.301 s, 4.4 MB |
| a text of 100 MB | 0.304 s, 222.1 MB | 0.334 s, 222.1 MB | 0.130 s, 331.1 MB | 0.217 s, 112.6 MB | 0.246 s, 112.8 MB | 0.701 s, 222.4 MB | 0.106 s, 337.1 MB |
| an object of ten million | 3.512 s, 1068.0 MB | 3.561 s, 1127.6 MB | 3.629 s, 1468.2 MB | 1.857 s, 1447.1 MB | 2.120 s, 1514.6 MB | 3.660 s, 2313.5 MB | 1.307 s, 1477.8 MB |
| ten million sorted | 0.132 s, 157.0 MB | 0.130 s, 157.1 MB | 0.266 s, 310.3 MB | 0.285 s, 309.0 MB | 0.396 s, 385.6 MB | 1.238 s, 1417.5 MB | 0.173 s, 413.2 MB |
| added to, a million times | 0.076 s, 19.7 MB | 0.089 s, 19.8 MB | 0.107 s, 20.4 MB | 0.073 s, 19.2 MB | 0.082 s, 19.4 MB | 0.259 s, 95.2 MB | 0.136 s, 34.8 MB |
| set by name, a million times | 0.555 s, 121.8 MB | 0.614 s, 121.8 MB | 0.377 s, 127.2 MB | 0.312 s, 123.3 MB | 0.408 s, 125.2 MB | 1.024 s, 164.5 MB | 0.437 s, 138.3 MB |
| a def a million deep | 0.164 s, 109.8 MB | 0.194 s, 156.2 MB | 0.087 s, 125.6 MB | 0.085 s, 156.2 MB | 0.100 s, 156.5 MB | 0.255 s, 150.3 MB | fails |
| a def ten million in tail | 0.906 s, 2.3 MB | 1.036 s, 2.4 MB | 0.389 s, 3.0 MB | 0.430 s, 2.3 MB | 0.520 s, 2.6 MB | 1.720 s, 3.1 MB | 1.448 s, 4.4 MB |
| nested ten million deep | 0.704 s, 1073.5 MB | 0.891 s, 1536.6 MB | 1.043 s, 1074.2 MB | 0.700 s, 1847.4 MB | 0.808 s, 2001.4 MB | 1.616 s, 309.6 MB | fails |
| Size, stripped | 488 KB | 537 KB | 2255 KB | 652 KB | 749 KB | — | — |
| Lines | 6394 | 6394 | 5027 | 5517 | 5517 | — | — |

What it shows:

- **SG is ahead of jq 1.8.2 in every case and of Rust in all but four**; Rust's `serde_json` builds a
  `Value` of its own before the program's, where SG's `JsonReader` (1814) and Zig's `Scanner` hand the
  tokens straight to the program. **Zig is ahead of SG in most cases**, by 18 to 30% where records are read
  and written; SG is ahead in ten million lines out and ten million sorted, and holds less memory in the
  object of ten million (1,068 MB against 1,447 MB), in ten million sorted and nested ten million deep.
- **Where SG is behind** (in [Slower than C](slower-than-c.md)): a call of a `def` (0.91 s against 0.43 s
  in tail position, and `set by name`, 0.56 s against 0.31 s); `every number added`, 2.03 s against
  0.98 s, where the rest is the machine of `..` and not the numbers, and 859 MB against 589 MB; the
  memory of the records (713 MB against 558 MB), where SG holds each fraction read with its literal in two
  blocks; and a text of 100 MB, which SG holds twice while it is read (222 MB against Zig's 113 MB).
- **Slices 170 and 171** (choices 1836, 1824, 1825, 1837): a function whose values may take a page of its
  stack touches it only near the bottom of the stack (a `def` ten million deep in tail position 1.23 s →
  0.91 s), and the three read and write numbers one way: a whole number of up to 15 digits is its
  `double`, any other keeps its literal and works out its `double` when wanted, two are compared by their
  `double`s first, and a whole `double` is written as an integer. SG had written every long number into
  text as it read it: every number added 3.05 s → 2.03 s, ten million lines out 0.66 s → 0.15 s (Zig
  0.46 s → 0.17 s). The fractions of the records are literals now, so SG holds 713 MB where it held 614 MB
  and its `sort_by` takes 1.44 s where it took 1.23 s; Zig's whole numbers no longer take a block (558 MB
  where 588 MB), and its `sort_by` compares `double`s (1.12 s where 1.43 s).
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

## kv

A store of keys and values with a log written before each change (`trials/kv/README.md`, choices
1844-1853). The three write the same log byte for byte; 16 cases of `measure.py kv --check` hold their output,
exit code and log to each other, and `trials/kv/tests/check.py` holds them to a model written in Python on
streams drawn at random, logs cut at every byte and with bits turned, and the store killed in the middle of its
sets (every set answered `OK` there after it starts again). Apple M3 Max, macOS, 24 November 2026, slice 175,
after the slots of a table kept 32 bits of each key's hash. A key is 16 bytes and a value 100; each run starts
in a folder made anew, empty or with a log Python wrote. The case of `always` is measured once after its
warming run, being the disk's time: a sync with `F_FULLFSYNC` takes about 4 ms on this machine.

| Case | SG | SG, checked | Rust | Zig | Zig, safe |
|---|---|---|---|---|---|
| A million sets, never synced | 0.240 s, 196.7 MB | 0.241 s, 196.7 MB | 0.317 s, 321.2 MB | 0.382 s, 216.7 MB | 0.446 s, 216.9 MB |
| A million sets, synced each block | 8.492 s, 197.2 MB | 8.556 s, 196.6 MB | 8.707 s, 321.2 MB | 9.085 s, 216.7 MB | 8.923 s, 216.9 MB |
| Ten thousand sets, each synced (once) | 41.938 s, 4.2 MB | 40.828 s, 4.5 MB | 40.445 s, 4.9 MB | 40.189 s, 4.4 MB | 40.232 s, 4.7 MB |
| A million gets, sets and deletes over 100,000 keys | 4.484 s, 35.4 MB | 4.612 s, 37.6 MB | 4.612 s, 24.6 MB | 4.760 s, 20.2 MB | 4.693 s, 20.4 MB |
| A value of 100 MB, set and read back | 0.097 s, 304.1 MB | 0.099 s, 304.1 MB | 0.099 s, 304.2 MB | 0.335 s, 302.2 MB | 0.355 s, 402.2 MB |
| A log of 1 GB read back | 2.697 s, 194.0 MB | 2.718 s, 194.0 MB | 2.374 s, 315.1 MB | 3.237 s, 216.6 MB | 3.355 s, 216.8 MB |
| Ten million keys read and compacted | 4.285 s, 1852.5 MB | 4.199 s, 1937.2 MB | 5.361 s, 1876.9 MB | 12.078 s, 1064.6 MB | 10.258 s, 1064.9 MB |
| Size, stripped | 100 KB | 116 KB | 385 KB | 394 KB | 474 KB |
| Lines | 369 | 369 | 367 | 348 | 348 |

- Three cases are the disk's: a sync of each block of the input, and of each set, take the same time in the
  three within the noise of the disk.
- SG holds less than Rust where the map is large: a key and a value are each a list of their own (24 bytes and
  the bytes), where Rust's `Vec` takes the same and its table keeps a whole hash; Zig keeps a key as a slice
  and its table is the smallest. Over 100,000 keys changing all the time SG holds the most, 35.4 MB against
  24.6 MB, in [Slower than C](slower-than-c.md).
- The log of 1 GB is read in 2.70 s against Rust's 2.37 s: the map's work, the reading and the CRC-32 taking
  0.3 s of it. Measured the same day before the table kept its hashes, SG took 2.67 s against Rust's 2.16 s
  (a quieter run) and the compaction of ten million keys 6.53 s against 4.82 s: the search compared the key
  of each entry it passed, and growing hashed every key again. In [Slower than C](slower-than-c.md).
- A compaction sorts the keys by their bytes. SG moves the entries out of the map to sort them and puts them
  back, since a list is not `Comparable`; Rust and Zig sort references to them. Zig's map grows by hashing
  each key again, which the compaction of ten million keys shows.
- The checked builds differ by the noise of the disk; with nothing on the disk, by 2% or less either way.

## hashsum

The SHA-256 of each file from a pool of workers, written in the order given, byte for byte as GNU's `sha256sum`
writes it (`trials/hashsum/README.md`, choices 1854-1860). 22 cases of `measure.py hashsum --check` hold the
output and exit code of the five builds to each other, and `trials/hashsum/tests/check.py` holds them to a model
of GNU's `sha256sum`, itself held to GNU on Linux. Apple M3 Max (16 processors), macOS, 24 November 2026, slice
176. Each worker reads a file in blocks of 64 KB; the files are random bytes Python made, read from the
system's cache after the warming run; without `-j` each program runs as many workers as there are processors.
The system's `sha256sum`, which hashes one file after another, is measured beside them for reference; it takes
no `-j`.

| Case | SG | SG, checked | Rust | Zig | Zig, safe | sha256sum |
|---|---|---|---|---|---|---|
| 20,000 files of 4 KB | 0.310 s, 4.4 MB | 0.304 s, 4.5 MB | 0.290 s, 4.5 MB | 0.302 s, 6.1 MB | 0.304 s, 7.0 MB | 0.385 s, 2.1 MB |
| 16 files of 64 MB | 0.049 s, 3.3 MB | 0.055 s, 3.3 MB | 0.052 s, 3.5 MB | 0.048 s, 3.9 MB | 0.048 s, 4.0 MB | 0.569 s, 1.8 MB |
| A file of 1 GB | 0.527 s, 2.3 MB | 0.528 s, 2.3 MB | 0.515 s, 2.6 MB | 0.463 s, 2.5 MB | 0.467 s, 3.6 MB | 0.569 s, 1.8 MB |
| 2,000 files of 1 KB to 16 MB | 0.322 s, 3.7 MB | 0.322 s, 3.7 MB | 0.315 s, 3.8 MB | 0.288 s, 4.3 MB | 0.289 s, 4.5 MB | 1.767 s, 1.9 MB |
| 16 files of 64 MB, `-j 1` | 0.529 s, 2.0 MB | 0.526 s, 2.0 MB | 0.516 s, 2.2 MB | 0.465 s, 2.2 MB | 0.467 s, 2.4 MB | — |
| 16 files of 64 MB, `-j 2` | 0.268 s, 2.1 MB | 0.269 s, 2.1 MB | 0.262 s, 2.3 MB | 0.239 s, 2.3 MB | 0.238 s, 2.5 MB | — |
| 16 files of 64 MB, `-j 4` | 0.138 s, 2.3 MB | 0.139 s, 2.2 MB | 0.135 s, 2.5 MB | 0.122 s, 2.5 MB | 0.123 s, 2.7 MB | — |
| 16 files of 64 MB, `-j 8` | 0.072 s, 2.7 MB | 0.073 s, 2.6 MB | 0.071 s, 2.8 MB | 0.065 s, 3.0 MB | 0.065 s, 3.1 MB | — |
| 16 files of 64 MB, `-j 16` | 0.052 s, 3.3 MB | 0.050 s, 3.3 MB | 0.051 s, 3.5 MB | 0.045 s, 3.9 MB | 0.046 s, 4.0 MB | — |
| 2,000 files of 1 KB to 16 MB, `-j 1` | 1.641 s, 2.4 MB | 1.645 s, 2.4 MB | 1.602 s, 2.5 MB | 1.450 s, 2.7 MB | 1.462 s, 2.8 MB | — |
| 2,000 files of 1 KB to 16 MB, `-j 2` | 1.096 s, 2.5 MB | 1.102 s, 2.5 MB | 1.075 s, 2.6 MB | 0.967 s, 2.8 MB | 0.975 s, 2.9 MB | — |
| 2,000 files of 1 KB to 16 MB, `-j 4` | 0.740 s, 2.7 MB | 0.741 s, 2.7 MB | 0.722 s, 2.8 MB | 0.652 s, 3.0 MB | 0.655 s, 3.1 MB | — |
| 2,000 files of 1 KB to 16 MB, `-j 8` | 0.494 s, 3.1 MB | 0.497 s, 3.0 MB | 0.489 s, 3.1 MB | 0.442 s, 3.5 MB | 0.446 s, 3.6 MB | — |
| 2,000 files of 1 KB to 16 MB, `-j 16` | 0.320 s, 3.7 MB | 0.321 s, 3.8 MB | 0.315 s, 3.9 MB | 0.287 s, 4.3 MB | 0.289 s, 4.5 MB | — |
| Size, stripped | 85 KB | 85 KB | 436 KB | 361 KB | 458 KB | — |
| Lines | 185 | 185 | 180 | 160 | 160 | — |

- The pools scale alike: from one worker to sixteen, SG's time on 16 files of 64 MB falls 10.2 times, Rust's
  10.1, Zig's 10.3. On 20,000 small files the three are within 7%: what a job costs is the opening of a file.
- What is left between them is the speed of SHA-256 on one processor: a file of 1 GB takes SG 0.527 s, Rust
  0.515 s, Zig 0.463 s, all three with the processor's SHA instructions. SG's way to them is in
  [Slower than C](slower-than-c.md) (SHA-256 one call at a time).
- SG holds the least memory with a worker or more at work: a worker's reads go straight into its own block. Before
  that, measured the same day, a `readSome` of a file read through the reader's block of 64 KB and copied out:
  sixteen workers held 4.9 MB and the mixed files took 0.330 s.
- The first SG version made the channel's sender after the workers had their receivers; the channel had ended
  for those that started first, and one worker of sixteen hashed every file: 16 files of 64 MB took 0.548 s.
  Such a program is now an error of the compiler (choice 1860).
- Rust is measured with `sha2` 0.11, which uses the processor's SHA instructions on this machine; 0.10 does not,
  and took 2.37 s for the file of 1 GB.

## giftool

A GIF read, changed and written again, or told as `gifsicle -I` tells it, held to gifsicle 1.99 byte for byte
where it tells and pixel for pixel where it draws (`trials/giftool/README.md`, choices 1874-1881). Written from
the GIF89a specification: LZW decoded and encoded by hand the same way in the three, frames drawn on a screen of
colors by their disposals for `--unoptimize` and `-O`. 24 cases of `measure.py giftool --check` hold the output
and exit code of the five builds to each other, and `trials/giftool/tests/check.py` holds them to a model in
Python, itself held to gifsicle: on macOS SG, SG interpreted, Rust and the two Zig builds, 1,653 runs of three
builds and 1,102 of two, no problem; on Linux SG, Rust and the interpreter. Apple M3 Max, macOS, 25 November
2026, slice 179. The GIFs are made from a seed by `trials/giftool/tests/inputs.py`: an animation of 2,000 frames
of 480 by 270, the first whole and the others small parts of the screen, with tables of their own, every
disposal, transparency and interlacing; one of 300 frames each up to the whole screen; one frame of 8192 by
8192 (28 MB); 10,000 small GIFs. gifsicle is measured beside the three; it is another algorithm, and writes other
bytes.

| Case | SG | SG, checked | Rust | Zig | Zig, safe | gifsicle |
|---|---|---|---|---|---|---|
| 2,000 frames, read and written | 0.048 s, 7.6 MB | 0.050 s, 7.6 MB | 0.049 s, 7.7 MB | 0.052 s, 10.5 MB | 0.053 s, 12.0 MB | 0.089 s, 8.8 MB |
| 8192 by 8192, read and written | 0.488 s, 148.8 MB | 0.504 s, 148.8 MB | 0.487 s, 148.9 MB | 0.505 s, 147.0 MB | 0.520 s, 176.4 MB | 0.378 s, 239.1 MB |
| 2,000 frames, `-O2` | 0.852 s, 13.5 MB | 0.716 s, 13.6 MB | 0.806 s, 13.7 MB | 0.830 s, 32.0 MB | 0.911 s, 17.9 MB | 0.350 s, 22.1 MB |
| 300 whole frames, `-O2` | 0.288 s, 16.0 MB | 0.290 s, 16.0 MB | 0.252 s, 16.0 MB | 0.291 s, 23.9 MB | 0.325 s, 26.0 MB | 0.276 s, 24.4 MB |
| 300 whole frames, `-O2`, after slice 180 | 0.275 s, 15.8 MB | 0.275 s, 16.0 MB | 0.254 s, 16.0 MB | 0.298 s, 23.9 MB | 0.325 s, 26.0 MB | 0.276 s, 24.4 MB |
| 300 whole frames, `-O2`, after slice 181 | 0.262 s, 15.9 MB | 0.264 s, 16.0 MB | 0.252 s, 16.0 MB | 0.289 s, 23.9 MB | 0.321 s, 26.0 MB | 0.276 s, 24.4 MB |
| 2,000 frames, `--unoptimize` | 2.224 s, 81.9 MB | 2.251 s, 82.0 MB | 1.791 s, 82.0 MB | 2.209 s, 182.2 MB | 2.103 s, 176.5 MB | 0.089 s, 8.8 MB |
| 2,000 frames, `--unoptimize`, after slice 180 | 2.084 s, 81.8 MB | 2.091 s, 81.9 MB | 1.796 s, 82.0 MB | 2.235 s, 182.2 MB | 2.131 s, 176.5 MB | 0.090 s, 8.8 MB |
| 2,000 frames, `--unoptimize`, after slice 181 | 1.936 s, 81.9 MB | 1.962 s, 81.9 MB | 1.791 s, 82.1 MB | 2.225 s, 182.2 MB | 2.116 s, 176.5 MB | 0.089 s, 8.8 MB |
| 8192 by 8192, cropped and turned | 0.370 s, 162.7 MB | 0.389 s, 162.7 MB | 0.378 s, 162.8 MB | 0.375 s, 160.9 MB | 0.394 s, 196.9 MB | 0.296 s, 201.6 MB |
| 8192 by 8192, flipped | 0.521 s, 212.8 MB | 0.544 s, 212.9 MB | 0.532 s, 213.0 MB | 0.529 s, 211.0 MB | 0.570 s, 250.0 MB | 0.396 s, 239.1 MB |
| 10,000 small GIFs, `--info` | 0.144 s, 3.9 MB | 0.149 s, 3.8 MB | 0.160 s, 4.0 MB | 0.140 s, 3.5 MB | 0.143 s, 3.7 MB | 0.318 s, 76.9 MB |
| Size, stripped | 148 KB | 181 KB | 401 KB | 474 KB | 555 KB | — |
| Lines | 1460 | 1460 | 1398 | 1356 | 1356 | — |

- Reading, writing, cropping, flipping and turning, where the work is LZW and the walk over the pixels, the three
  are within 4% of each other. gifsicle encodes with another algorithm (a tree of codes, not a table of
  hashes); it is faster on the large frame and writes 0.05% more.
- Where the colors of the screen are given indices (`--unoptimize`, `-O2` of whole frames), SG took 24% and 14%
  more than Rust: a lookup in SG's `HashMap<uint, nint>` took 9.4 ns against Rust's 3.8 (a hundred million
  lookups, 0.94 s against 0.38). Slice 180 (choice 1882) looks at the slots eight at a time, as hashbrown does:
  0.48 s for the hundred million, and 16% and 8% more than Rust here (the rows "after slice 180", measured
  again the same day). What is left is in [Slower than C](slower-than-c.md).
- `-O2` of the 2,000 frames is faster checked than not (0.716 s against 0.852): clang puts the function that finds
  what changed into the one that calls it in the build without checks, and the loop compiled there is slower. The
  same function with checks stays a function of its own.
- gifsicle's `--unoptimize` with nothing after it writes what it writes with no option (2,466,436 bytes both),
  where the three write each screen whole (79 MB): its time is not that of the same work. Its `-O2` is another
  algorithm, and takes 0.35 s on the 2,000 frames.
- Zig holds more memory where it grows lists frame after frame (`--unoptimize`, 182 MB against 82): the
  allocator it uses, `smp_allocator`, keeps what is freed.

## lox

An interpreter of Lox after clox, the bytecode interpreter of *Crafting Interpreters*, held to clox byte for byte
(`trials/lox/README.md`, choices 1883-1894), and measured against clox alone, the C program it mirrors (the rule
of slice 180): no Rust or Zig. clox is built by its own Makefile in release (`-O3 -flto`) with the same Clang. The
two columns on the right of each are the loop of choice 1894, the place in the code kept in a local variable, made
the same way in both (`lox --ip-local`, and `clox-ip-local` made by `trials/lox/tests/clox.py`). 30 cases of
`measure.py lox --check` hold the five builds to each other, and `trials/lox/tests/check.py` holds SG to clox on
the book's 246 tests and 19 programs of its own, 2,114 runs, and `fuzz.py` on 5,000 programs from a seed: no
difference. Apple M3 Max, macOS, 25 November 2026, slice 181. The programs are the book's benchmarks with
`clock()` made `0`, so that they print the same in every run; fib, binary_trees and trees at three sizes, the
largest the book's; texts made, joined and interned, one of them growing by a character in each thousand; lists
of objects made and dropped five times. `string_equality` does not compile, in clox either (more than 256
constants in one chunk), and `zoo_batch`, which counts the batches done in ten seconds, is below the table.

| Case | SG | SG, checked | SG, `--ip-local` | clox | clox, `ip` local |
|---|---|---|---|---|---|
| fib(25) | 0.010 s, 2.0 MB | 0.012 s, 2.0 MB | 0.009 s, 2.0 MB | 0.008 s, 1.9 MB | 0.008 s, 1.9 MB |
| fib(30) | 0.083 s, 2.0 MB | 0.103 s, 2.0 MB | 0.076 s, 2.0 MB | 0.067 s, 1.9 MB | 0.061 s, 1.9 MB |
| fib(35) | 0.910 s, 2.0 MB | 1.115 s, 2.0 MB | 0.820 s, 2.0 MB | 0.719 s, 1.9 MB | 0.658 s, 1.9 MB |
| binary_trees, depth 10 | 0.060 s, 3.6 MB | 0.066 s, 3.7 MB | 0.053 s, 3.6 MB | 0.044 s, 3.2 MB | 0.041 s, 3.2 MB |
| binary_trees, depth 12 | 0.281 s, 9.2 MB | 0.315 s, 9.3 MB | 0.252 s, 9.2 MB | 0.207 s, 7.1 MB | 0.191 s, 7.1 MB |
| binary_trees, depth 14 | 1.344 s, 24.6 MB | 1.477 s, 24.6 MB | 1.192 s, 24.6 MB | 1.001 s, 23.7 MB | 0.913 s, 23.7 MB |
| trees, depth 6 | 0.075 s, 7.6 MB | 0.093 s, 7.7 MB | 0.069 s, 7.6 MB | 0.051 s, 5.2 MB | 0.048 s, 5.2 MB |
| trees, depth 7 | 0.364 s, 22.3 MB | 0.432 s, 22.4 MB | 0.320 s, 22.3 MB | 0.244 s, 18.4 MB | 0.230 s, 18.4 MB |
| trees, depth 8 | 1.851 s, 100.7 MB | 2.188 s, 100.4 MB | 1.597 s, 100.7 MB | 1.192 s, 84.2 MB | 1.129 s, 84.2 MB |
| texts, 100,000 | 0.024 s, 2.3 MB | 0.027 s, 2.3 MB | 0.022 s, 2.2 MB | 0.026 s, 2.1 MB | 0.027 s, 2.1 MB |
| texts, 1,000,000 | 0.487 s, 4.8 MB | 0.513 s, 4.8 MB | 0.458 s, 4.8 MB | 1.252 s, 3.4 MB | 1.253 s, 3.4 MB |
| texts, 5,000,000 | 7.936 s, 8.2 MB | 8.141 s, 8.2 MB | 7.820 s, 8.2 MB | 27.435 s, 4.3 MB | 26.884 s, 4.3 MB |
| objects, 20,000 five times | 0.019 s, 10.9 MB | 0.022 s, 10.9 MB | 0.018 s, 10.9 MB | 0.015 s, 8.6 MB | 0.014 s, 8.6 MB |
| objects, 200,000 five times | 0.164 s, 78.3 MB | 0.191 s, 78.3 MB | 0.155 s, 78.3 MB | 0.131 s, 69.3 MB | 0.128 s, 69.3 MB |
| objects, 1,000,000 five times | 0.818 s, 298.9 MB | 0.930 s, 299.0 MB | 0.746 s, 298.9 MB | 0.633 s, 271.5 MB | 0.620 s, 271.5 MB |
| equality | 3.414 s, 2.0 MB | 3.692 s, 2.0 MB | 2.098 s, 2.0 MB | 2.104 s, 1.9 MB | 2.101 s, 1.9 MB |
| instantiation | 0.416 s, 5.7 MB | 0.446 s, 5.7 MB | 0.393 s, 5.7 MB | 0.393 s, 3.1 MB | 0.410 s, 3.1 MB |
| invocation | 0.256 s, 2.0 MB | 0.301 s, 2.0 MB | 0.240 s, 2.0 MB | 0.155 s, 1.9 MB | 0.153 s, 1.9 MB |
| method_call | 0.186 s, 2.0 MB | 0.219 s, 2.0 MB | 0.166 s, 2.0 MB | 0.115 s, 1.9 MB | 0.102 s, 1.9 MB |
| properties | 0.390 s, 2.1 MB | 0.449 s, 2.1 MB | 0.366 s, 2.1 MB | 0.215 s, 2.0 MB | 0.224 s, 1.9 MB |
| zoo | 0.300 s, 2.0 MB | 0.344 s, 2.0 MB | 0.287 s, 2.0 MB | 0.159 s, 1.9 MB | 0.169 s, 1.9 MB |
| Size, stripped | 148 KB | 197 KB | 148 KB | 66 KB | 66 KB |
| Lines | 2181 | 2181 | 2181 | 2609 | 2609 |

- Where the work is the loop of bytecode alone, SG with `ip` in a local takes clox's time: `equality` 2.098 s
  against 2.104, `instantiation` 0.393 against 0.393. As the book writes clox, reading `ip` through the frame at
  each byte, SG takes 1.6 times (3.414 s): clox keeps the frame in a register and reads `frame->ip`, SG has no
  pointer local in safe code and loads the list of frames, the frame and its `ip` at each byte. clox gains
  little from the change; SG gains 39% on `equality`.
- Texts: SG takes a third of clox's time (7.94 s against 27.4 at five million). Each text joined is interned,
  and clox hashes it with FNV-1a a byte at a time, SG with SipHash-1-3 eight bytes at a time; the text that grows
  reaches 5,000 characters.
- Fields and methods: 1.6 to 1.9 times clox (`properties` 0.366 s against 0.224, `zoo` 0.287 against 0.169),
  with the hash of choice 1893 (0.75 s before it). clox keeps a string's hash in the string and finds a field
  by comparing addresses in one slot; SG's `HashMap` hashes the handle of the name and goes from the control
  bytes to the index to the entry. The rest, bounds checks on the stack, the frames and the code (25% on
  `properties`, measured with them taken out of the C by hand).
- Calls and trees: 1.2 to 1.4 times (`fib(35)` 0.820 s against 0.658, `trees` 1.597 against 1.129, in 19% more
  memory): the checks, and an object that takes the size of the largest variant of `Obj`.
- `zoo_batch`, batches in ten seconds (more is faster): SG 4,947, with `ip` in a local 5,627; clox 9,621, with `ip`
  in a local 9,028.

## gifsicle

The showcase: gifsicle 1.99 translated file by file into SG, in [a repository of its
own](https://github.com/selcukgural/sg-gifsicle) under gifsicle's license (choices 1895-1905), held to gifsicle
byte for byte by a differential test of 58,767 runs and gifsicle's own tests. Its `tests/measure.py` runs 24 cases
in three sizes on GIFs made from a seed, against gifsicle as Homebrew builds it and as the same compiler builds it
with `-O2` and the configuration of its `configure` (vector types and threads); the bytes each writes are held to
Homebrew's gifsicle. One run to warm up and the best of five. Apple M3 Max, macOS, Apple clang 21, 25 November
2026. Some of the cases (all 24 are in the repository's README):

| Case | SG | SG, checked | gifsicle | gifsicle, `-O2` here |
|---|---|---|---|---|
| 10,000 small GIFs, `-I` | 0.307 s, 96.6 MB | 0.305 s, 96.6 MB | 0.312 s, 78.0 MB | 0.304 s, 78.1 MB |
| 2,000 frames of 480 by 270, read and written | 0.110 s, 15.1 MB | 0.120 s, 14.8 MB | 0.092 s, 8.8 MB | 0.093 s, 8.8 MB |
| 2,000 frames, `-O3` | 0.769 s, 28.4 MB | 0.827 s, 30.6 MB | 0.589 s, 22.2 MB | 0.572 s, 22.2 MB |
| 300 whole frames, `-U -O2` | 0.346 s, 32.4 MB | 0.371 s, 32.1 MB | 0.282 s, 24.4 MB | 0.267 s, 24.5 MB |
| 2,000 frames, `--lossy=80 -O2` | 1.155 s, 31.2 MB | 1.362 s, 30.9 MB | 1.010 s, 22.2 MB | 0.852 s, 22.3 MB |
| 40 frames of 800 by 600, `--scale 0.5 --resize-method lanczos3` | 1.279 s, 43.4 MB | 1.396 s, 43.5 MB | 1.259 s, 37.5 MB | 1.226 s, 37.6 MB |
| 40 frames, the same with `-j8` | 0.365 s, 135.8 MB | 0.400 s, 135.9 MB | 0.350 s, 129.4 MB | 0.366 s, 129.2 MB |
| 4000 by 4000, read and written | 0.105 s, 56.5 MB | 0.127 s, 56.6 MB | 0.098 s, 56.3 MB | 0.113 s, 56.3 MB |
| 4000 by 4000, `--rotate-90` | 0.146 s, 71.9 MB | 0.171 s, 72.0 MB | 0.124 s, 65.2 MB | 0.139 s, 65.2 MB |
| 4000 by 3000, `--resize 1920x_ --resize-method lanczos3` | 0.745 s, 367.9 MB | 0.798 s, 367.1 MB | 0.739 s, 358.8 MB | 0.729 s, 358.8 MB |
| 4000 by 3000, `--colors 64` | 0.259 s, 49.9 MB | 0.279 s, 49.9 MB | 0.235 s, 42.8 MB | 0.245 s, 42.8 MB |
| 4000 by 3000, `--colors 64 --dither` | 1.159 s, 51.1 MB | 1.318 s, 51.2 MB | 1.238 s, 42.8 MB | 1.158 s, 42.8 MB |
| 4000 by 3000, `--lossy=80` | 0.434 s, 50.2 MB | 0.464 s, 50.2 MB | 0.413 s, 49.9 MB | 0.389 s, 50.0 MB |
| Size, stripped | 683 KB | 942 KB | 201 KB | 234 KB |
| Lines | 12,773 | 12,773 | 12,964 | 12,964 |

- Large frames, turning, resizing and color reduction: 1.01 to 1.17 times Homebrew's gifsicle; the dithers are
  faster (0.94 and 0.95). Most of the rest is the LZW encoder every write goes through ([Slower than C](slower-than-c.md)).
- `-O3` and `-U` on animations: 1.23 to 1.31 times, `--lossy=80 -O2` 1.14. They compress each frame many times, so the
  encoder counts more, and each time the port makes a table of codes of 100 KB it must fill, where C takes the
  memory as it is (about 3%). Built in place by hand in the C, the table took the same time: the cost is the
  filling, not copies.
- Memory: the same on large frames, 1.3 to 1.7 times on animations. The port reads the whole file into memory,
  and the allocator keeps more of what it frees; the most it holds at once in `-O3` is gifsicle's (14.9 MB
  against 14.6).
- `-j`: the frames of an unoptimized GIF resized on several threads, as gifsicle does, with the bytes of one
  thread. gifsicle's threads share the stream behind two locks and an atomic counter; in the port each frame is
  moved, with its local colormap, through a channel to a thread that owns it until it gives it back, and each
  thread has its own copy of the global colormap, which no thread changes. The thread rule allows no other way,
  and no lock is needed; a build with ThreadSanitizer finds no race. `-j8` on 40 whole frames: 3.5 times as
  fast as one thread, as gifsicle (0.365 s against 0.350).
- The measuring found three things in the port, changed with the same bytes: the GIF written was held whole in
  memory before it went to the file (19.1 MB to 14.8 on the 2,000 frames), `-O` took a new screen for each frame
  disposed to "previous", and turning looked the image up twice for each pixel (0.169 s to 0.144).
