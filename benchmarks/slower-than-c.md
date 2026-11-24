# Slower than C, and could be better

Places where a measurement found SG slower than the same program in C and a fix is in sight. They are kept
here to be talked over when their time comes; none of them breaks a program, and none is fixed by turning a
check off.

Since the trials (choice 1418) the list also holds the cases where SG is slower than Rust or Zig. Each entry
says what is slower, by how much and against what, the choice that measured it, and what would fix it.

## The library

- **A regular expression where the DFA reads every byte** (`grep` on the address, 1.03 times Zig's PCRE2,
  choices 1535, 1536, 1541, 1803, slices 164 and 165): since the hundred-and-sixty-fifth slice SG searches
  from the inner text (`@example.com `) as Rust's `regex` does and is ahead of Rust (0.133 s against 0.148 s);
  PCRE2's JIT, which makes machine code of the pattern, takes 0.129 s. `findAll` over 100 MB is now at about
  Rust's time where a pattern has a rare inner text (`\bcache\b` 0.032 s against 0.037 s), but 1.42 times Rust's
  on `[a-z]+ [a-z]+`, whose inner text, a space, is almost everywhere (0.301 s against 0.212 s), and 1.26 times
  on `(?i)error` (0.063 s against 0.050 s). For the first the search from the space costs what the search
  before it cost (0.303 s); the candidate is a cheaper loop of the DFA. The second is not looked into yet.

- **Decomposing a text** (NFD of 16 MB in eight languages, 1.24 times Rust's `icu_normalizer`, choice
  1612, 1613): SG looks up the record of each character a decomposition gives, for its class, and pushes the
  characters of a piece one at a time; ICU4X keeps the classes with the decompositions and copies a run of
  characters that have none at once. Candidates: the class of each character of a decomposition kept beside
  it in `NORM_DECOMPOSED`, and a run of starters with nothing to change written straight to the output. On the
  text of the trial of `std/unicode` (choice 1662), each line of 100 MB decomposed on its own, SG takes the
  time of ICU4X (0.650 s against 0.657 s).

- **Hex and Base64** (`std/encoding`, choices 1660, 1664-1666): 1.54 times Zig on ten million values of 32
  bytes in hex and back (0.621 s against 0.402 s; Rust's `hex` takes 1.852 s), 1.37 times Rust and Zig on ten
  million tokens in the Base64 of URLs (0.605 s against 0.443 s), 1.38 times Rust writing 100 MB in Base64
  (0.069 s against 0.050 s, as Zig) and 1.25 times Zig reading it back (0.066 s against 0.053 s). Each call
  writes through a block of 64 bytes on the stack, which SG fills with zeros first, as it does every array, and
  hands the block to the text through `string.fromUtf8` and a copy, or to the list through `addAll`; for 32
  bytes that is most of the work. Zig's `bytesToHex` gives an array whose size is known when the program is
  compiled, with nothing made or copied, and Rust's `base64` writes eight digits into the buffer it is given
  with one store. Candidates: an array that is not filled with zeros where every byte of it is written before
  it is read, which the compiler can prove here; bytes written into a text where they are known to be ASCII.

- **CRC-32 of short pieces, and of much at once** (`std/checksum`, 1.26 times `crc32fast` on the 921,952
  lines of the log, 0.044 s against 0.035 s, and 1.3 times on 100 MB, 0.013 s against 0.010 s; choices
  1415, 1656): since 1656 the processor's carry-less product folds what is 64 bytes or longer, eight streams
  at a time, and on ARM `crc32x` takes what is shorter, faster than zlib on macOS (`checksum`, 0.69 times).
  What is left: reading a line takes 3 ms more over the log than Rust's `read_until`, as SG checks it is
  UTF-8; each call goes from `Crc32.of` through `std` to the C that holds the instructions, and asks once
  more whether the processor has them; and `crc32fast` folds sixteen streams from 4 KB, which in SG's C
  took twice the time, the streams no longer kept in registers. Candidates: sixteen streams written out
  by hand rather than as a loop; the question of the processor asked once for a program, not at each call.

- **Opening DEFLATE** (`gunzip`, 1.31 times zlib on macOS, 1.74 before the CRC-32 of 1656 and 2.60 before choice 1647; choice 1486): the
  decoder reads its codes from tables as zlib's does, with the bits, the place in the input and the length of
  the output in locals and the lists looked at through slices, and copies a match eight bytes at a time when
  it lies 8 back or more (1647), but each byte it writes and each entry it reads is checked against the end
  of its slice, a literal is decoded one at a time and a match closer than 8 is copied a byte at a time. In
  the trial of `std/compression` it opens 100 MB in 0.084 s, as `zlib-rs` does (0.086 s), and faster than
  `flate2`'s default (0.175 s) and Zig (0.171 s). Candidates: two literals decoded in one step, a short distance
  copied by widening its pattern, and the checks the compiler can see are needed once a block.

- **Compressing as zlib-ng does** (`std/compression`, 1.72 times `flate2` with `zlib-rs` on 100 MB of a log,
  1.19 times on bytes that do not compress, 1.41 times the `zip` crate writing 10,001 entries; choices 1485,
  1651): `zlib-rs` is zlib-ng in Rust, whose level 6 is a way of its own (its "medium"): it writes 3 per
  cent more bytes (19,275,743 against 18,728,988) in 0.58 of the time. SG gives zlib's bytes at every level
  (1485) and takes the time of `flate2`'s default (`miniz_oxide`, 1.675 s against SG's 1.616 s) and of Zig
  (1.556 s). Candidate, if speed is wanted over zlib's bytes: zlib-ng's levels as a choice of their own.

- **Listing a zip archive** (`std/zip`, 3.4 times Zig's `std.zip` on 100,000 entries, 0.027 s against
  0.008 s, and 19.6 MB against 2.2 MB; choices 1503, 1652): Zig reads each record of the central directory
  as the program asks for the next and keeps nothing, and checks no name against another. SG reads the
  directory once and checks every name before anything is given (a name that could lead out of a folder,
  two names of one key in any case and normal form, data that lie over each other), so it keeps the
  entries; it takes a tenth of the time of Rust's `zip` (0.251 s) in less memory (22.7 MB). Candidate: an
  entry that keeps its name and its numbers in one block, the size of a record of the directory.

- **SHA-256 one call at a time** (`std/hash`, 1.16 times Zig on ten million hashes of 32 bytes, 0.331 s
  against 0.284 s, as Rust's `sha2` takes; 1.22 times Zig on an HMAC of each line of the log, 0.138 s
  against 0.113 s, faster than Rust's 0.144 s; 1.20 times Zig on 100 MB, 0.049 s against 0.041 s; `sha256`
  1.17 times OpenSSL on macOS; choices 1422, 1657): since 1657 the blocks go through the SHA instructions
  of the processor, 8.93 times OpenSSL before. The C of those instructions takes a hash of one block in
  25 ns alone, where the whole call takes 33 in SG and 28 in Zig: the rest is the way to it, the eight
  words lent as a slice, a call of the system for each run of blocks that asks again whether the
  processor has the instructions, the checks of the last block's slices. The HMAC of a line reads it as
  text, checked to be UTF-8, where Zig's `takeDelimiter` lends the bytes of its buffer. Candidates: the
  question of the processor asked once for a program; the last block made in C beside the rounds.

- **JSON read one allocation at a time** (`json`, 1.15 times cJSON on macOS, choice 1444): every string,
  list and object of a `JsonValue` is a block of its own from `malloc`, and each value is moved through a
  `Result` on its way up, where cJSON allocates one node for each value and links them. Candidate: the
  strings of a document kept in one block, or the lists made at their size once the closing bracket is
  found (simdjson's way of counting first); a `Result` that is filled where the value goes.

- **A moment in the time zone** (`std/time`, 3.8 times `chrono` on a million moments, 0.203 s against 0.054,
  as Zig's program that also calls `localtime_r`, 0.189; choice 1675): each `toLocal()` asks the C library,
  which takes a lock and looks at `TZ` again on every call. `chrono` reads the TZif file of the zone once and
  finds each offset by a search through its transitions. Candidate: a reader of TZif of SG's own on Linux and
  macOS, the transitions kept once a program asks; Windows keeps the rules of its system.

- **A token from the random bytes of the system** (`std/random`, 4.7 times Zig on a million tokens of 32
  bytes, 0.242 s against 0.051, four times as fast as Rust's `SysRng`, 0.957; choice 1676): `SecureRandom`
  asks the system for each token (`arc4random_buf` on macOS, `getrandom` on Linux), where Zig's `Io.random`
  is a ChaCha kept in the process and seeded by the system. Kept so on purpose: a generator in the process has
  to see a `fork` and keep its key out of a dump of memory. Candidate, if it is ever wanted: the bytes of one
  call of the system shared by several small requests, under the same rules.

- **Reading RFC 3339, and lines of a log** (`std/time` and `std/log`, 1.09 times Zig's hand-written reader on
  3,407,285 moments, 0.136 s against 0.125, and 1.09 times Zig on a million lines, 1.864 s against 1.708;
  choices 1673, 1677): each line read is copied into a text checked to be UTF-8, where Zig's `takeDelimiter`
  lends the bytes of its buffer, and `DateTimeOffset.parse` gives each part through a `Result`. A copy of a
  few bytes with stores of a fixed size in place of `memcpy` was tried and gained nothing. The log's line is
  made in a text and written by `fwrite` under a lock, Zig's straight from its buffer.

## The trials

- **`jq`: a call of a function of the filter** (choice 1819, slice 166): the machine of SG's `jq` makes
  each call of a `def` with a frame of `shared` values on the heap, and its continuation with them; a `def`
  ten million deep in tail position takes 0.91 s against Rust's 0.39 s and Zig's 0.43 s, a million deep
  0.16 s against 0.09 s, and a million `.[$k] = $i` 0.56 s against 0.31 s (slice 171's table). A quarter of
  the first was the touching of the stack at each call of the machine, whose frame may take more than a
  page, until slice 170 (choice 1836) touched it only near the bottom of the stack (1.23 s before). The
  same machine walks `..`: of `every number added`, 2.03 s against Zig's 0.98 s, the numbers are no longer
  the part that differs. The machines are the same algorithm; Rust's and Zig's frames are cheaper to make
  and let go. What would fix it: frames kept in a list of the machine's own and reused, a count changed
  without an atomic where a frame never leaves its thread.
- **`jq`: the memory of numbers read** (slice 171, choice 1837): a number read that is not a whole
  number of up to 15 digits keeps its literal, in all three. SG holds it in two blocks, the `shared`
  value and its text, where Zig holds the text in the same block: 100 MB of coordinates of 17 digits take
  859 MB against Zig's 589 MB, and the records, whose fractions are such numbers too, 713 MB against
  558 MB (614 MB before, when a fraction of up to 15 digits was its `double`; its `sort_by` 1.44 s, 1.23 s
  before). What would fix it, put to the author and left: a fraction of up to 15 digits held as its
  `double` in the three (its `double` found from its digits by one division, as for whole numbers), and
  SG's short literal kept in a `byte[24]` of its value.
- **`jq`: a text of 100 MB** (`length`): 0.30 s and 222 MB against Zig's 0.21 s and 113 MB (Rust 0.13 s,
  331 MB): the text is read whole into the reader's buffer and then copied into the value; Zig unescapes
  it from the buffer into the value once.

## Compiled code

- **Bytes read one index at a time** (`hash`, choice 136): `data[at] | data[at + 1] << 8 | ...` checks
  each index, the C compiler cannot join the reads, and the program takes 3.5 times the time of C. Cutting
  the window first (`data[at..at + 8]`) already takes the time of C, and `ulong.fromLittleEndian` (301)
  does too, in the checked build as well. What is left: a hint from the checker where it sees the pattern.

- **Overflow tests on a product in a loop, in a checked build** (`vector` 1.6 times): `xs[i] = xs[i] * 3 +
  1` tests the product and the sum of an item, whose size nothing tells before the loop, and the tests
  keep the C compiler from working on several items at once. A release build has none, and the tests are
  real. No candidate yet. The rest of this entry is done: the overflow test of an index (`hash`, 1.23
  times) since the fifty-second slice (309), and the test of a sum kept in a loop (`buffer` 2.6 times,
  `vector`'s `sum += x`) since the fifty-third, which runs a loop without it for as many rounds as the sum
  has room for (315); the checked `buffer` now takes the time of C. A sum in a `while`, in a `for` over
  the parts of a text, written `sum = sum + x`, or in a loop with a loop inside or with a call keeps its
  test (316, 317); none of the measured programs has one where it costs.

- **The hash of a `HashMap`** (`counts`, 1.17 times): SipHash-1-3 with a key drawn at start against C's
  one multiplication. Keeping it is a decision (13 November, choice 50); a faster hash for integer keys
  that still resists chosen keys is the open question.

- **Wrapping arithmetic in a loop that is only arithmetic** (`vector`'s first version, 1.28 times): C may
  assume a signed overflow never happens and turn `i * 7 + round` into a counter; wrapping is defined, so
  the C compiler keeps the arithmetic. Rust pays the same. No candidate yet.

- **Several owners** (`tree`, 1.53 times C with plain pointers, choice 354): of the 0.033 s, the atomic
  counts are about 0.010 s and the checks of the borrows about 0.003 s; the rest is the larger blocks and
  the owner taken and let go at each step up the tree. Candidates: counts that are not atomic in a
  program with no tasks, which the compiler now knows (`SG_THREADS`, 378; the design keeps them atomic,
  348), and fewer owners taken on a walk (an owner
  of the parent is needed only because a `weak` look gives one).

- **Handing a buffer to C across calls** (`deflate`, 1.04 times C, choice 543): zlib reads its input
  between calls, and `buffer.pointer()` lasts only for one (523), so the `Deflater` copies each piece into
  raw memory a byte at a time, and copies what zlib wrote back the same way. Candidates: `Memory.copy`
  between a slice and raw memory, which compiles to `memcpy`, and a buffer lent to a value for as long as
  the value lives (a `raw` view that carries a borrow).

- **Checking that a line is UTF-8** (`netlines`, 1.09 times `getline`, which checks nothing, 0.89 without
  the check, choice 493; 1.14 before 1550; `lines` takes 0.79 times with it): the reader checks its block at
  once (491), with sixteen bytes at a time while they are ASCII (492), and goes straight to the first byte
  past ASCII among them (1550), about 10 GB/s on a log with a character past ASCII every forty bytes; the
  check is still some 0.01 s of the 0.09 s. The output a program keeps of another (`capture`,
  1.49 times reading the pipe in C, 1.02 without the check, choice 612) has the same check, 0.1 s for 626
  MB. Candidates: a check with vector instructions (as
  `simdutf` does), measured at 8.8 GB/s with NEON on arm64; on x86 it needs SSSE3, which a program has to
  look for when it starts.

- **The size of a program without an operating system** (`kernel`, 2.31 times the same kernel in C, 5168
  bytes against 2236, choice 924): the runtime's part is fixed, about three kilobytes: its own `vsnprintf`
  writes each `$"..."` and each panic, the text of a panic carries its place and the line with a caret, and
  the message is checked to be UTF-8 before `onPanic` has it. Candidates: writers of each kind of value
  called directly instead of a format read at run time, and a short panic (the message and the place
  without the line) for a build that asks for it, as Rust's `panic_immediate_abort` and
  `-Zlocation-detail=none` do.

## Tasks, async and signals

- **A read given a token** (`trials/server`, `--idle-limit`, the 158th slice, choices 1720 and 1724): every wait
  of the network given a token goes on the token's list and off it under the clock's one lock, which every
  thread shares, so the store with a limit on each request gets 3.2% fewer answers with 50 connections and 3.9%
  fewer with 1,000, and a p99 a tenth higher (76 against 72 µs, 1.18 against 1.08 ms). `cancelAfter` itself is
  one atomic write. A list of waits for each thread, or a token that takes its waits only when it is
  cancelled, would take the lock away.

- **The network the async way, against Tokio and Zig** (`net`, choices 1689 and 1708-1714): since each thread
  has a loop of the network of its own (the 157th slice), a stream of 1 GB in pieces of 64 KB takes 0.140 s
  against 0.153 s for `tokio`, and 50,000 connections one after another 4.000 s against 3.972 s for Zig (1.01
  times); datagrams take 1.03 times Zig's threads. Where the two ends talk inside one program, see below. A
  red line since 22 November 2026: on the network SG is never to be worse than the best rival, under load or
  not.
- **Two ends that talk inside one program** (`net`, `echo`, `udpecho`, the 157th slice, choices 1708-1714): the
  echo of 64 bytes, server and clients tasks of one program, takes 1.546 s against 1.185 s for `tokio` (1.30
  times; 1.208 s before the slice), 1,000 connections at once 0.959 s against 0.765 s (1.25 times; 0.788 s
  before), and on Linux the datagrams of `udpecho` take 0.78 s where they took 0.41 s, now C's time. A socket
  goes on the loop of the thread with the fewest, so the two ends of a connection lie on the loops of two
  threads, and every message wakes the other's core, which slept in its loop, and reaches it with cold caches;
  `tokio` waits on one loop, whose thread runs both ends. Put on one loop, the sockets of `udpecho` take 0.45 s
  and those of `echo` on Linux 0.44 s, but then a server with outside clients is not spread over the cores, and
  putting a socket on the loop of the thread that first waits for it spreads neither (50 connections of the
  store get 382,000 answers a second). Candidates: the two ends of a connection made inside the program put on
  one loop, and a thread that looks at its loop a little longer before it sleeps while another thread works,
  which a first try of 50 µs made the tail at a fixed rate worse with (below).

- **Each connection of async code holds 1.4 KB** (`net`, choice 1690): 1,000 connections at once, a task
  on each side, hold 3.8 MB as `tokio`'s, but each further connection costs 1.39 KB against 0.80 KB for
  `tokio` and 0.51 KB for Zig. Most of it is the frames of the two tasks, 672 and 560 bytes, which keep
  every temporary of the function in a field of its own and, in the room of the call of the network, a
  wait whose delay (72 bytes) only a wait for the clock uses. Candidates: temporaries that are never alive at
  once share a field, as the calls of the network now share one, and the wait for a socket carries only what
  it uses.

- **A server on more than one core** (the server lane, [Servers](server.md), choices 1698 and 1708-1714): since
  each thread of the pool has a loop of the network of its own and a socket stays on the loop it was put on (the
  157th slice), 50 connections with a request in flight on each get 1,342,000 answers a second from SG's store,
  against 1,478,000 from C's thread on each core (0.91 times) and 903,000 from Zig's (1.49 times Zig), where
  they got 578,000 before; with 1,000 connections 1,331,000 against C's 1,503,000 (0.89 times). Not looked into
  yet; C has a listener on each thread and no pool, so no task moves between threads.
- **The tail with 1,000 connections and 16 requests in flight on each** (the server lane): the answers a second
  are those of the others (2.0 million, the load's own limit), but p99 is 15.8 ms, against 10.5 ms for C and
  11.3 ms for `tokio`: some connections wait while others are served. Candidate: a look at the slot of the last
  task woken, which runs a woken task before those waiting.
- **A server that takes each connection through one task** (the server lane): with a connection for each
  request SG answers 171,000 a second, as many as `tokio`, against Zig's 200,000 and C's 203,000, with the least time of the processor of all
  (4.0 ms for each thousand, C 9.3, Zig 14.6): every connection goes through the task that holds the list of tasks, a
  task to take the next connection and one for the connection, and `Task.whenAny` looks for the task that ended
  through the list and moves those after it, so a server with many connections pays for each that ends.
  Candidates: a group of tasks that takes its ended tasks out by itself, which a server hands its connections
  to, and `whenAny` that finds its task without a search.
- **One connection with 16 requests in flight, and memory** (the server lane; one run, which the median of three
  is to confirm): one connection with 16 requests in flight on it gets 477,000 answers a second from SG and
  580,000 from `tokio` (504,000 before the 157th slice, by the median of three); 10,000 connections that each ask
  once a second hold 53.2 MB in SG and 47.8 MB in `tokio`, and 1,000 connections with 16 in flight 10.0 MB
  against 9.8 MB; in the long run of 30 minutes (1,000 connections, each opened again after 1,000 answers, with
  the runtime before the 157th slice) SG holds 22.2 MB and `tokio` 13.3 MB, neither growing. Not looked into
  yet.
- **The tail at a fixed rate since a loop on each thread** (the server lane, the 157th slice, choices
  1708-1714): with 100 connections at 70% of the slowest's answers a second over HTTP, p99 is 1.01 ms in SG
  against 195 µs in hyper (441 µs before the slice), and with 10,000 idle connections beside 100 busy ones 805
  µs against 339 µs for `tokio` (364 µs before); 10,000 connections that each ask once a second wait 6.8 ms at
  p99, against 2.1 ms for Zig and 3.0 ms for C (1.9 ms before). C, with a loop on each thread too, has a p99 near
  SG's in the first two (997 and 668 µs), and the p50 is lower than `tokio`'s (51 µs against 67): a request that
  comes to the loop of a thread that is busy, or asleep in a core of the virtual machine that takes about a
  millisecond to wake, waits, where with one loop any thread awake took it. Neither a look at the loops of the
  other threads before sleeping, nor a look at its own for 50 µs more, nor the watch taken out mended it (each
  measured, the first two worse). To be measured on a machine of its own before more; a candidate is a loop
  whose ready sockets a thread with nothing to do hears of at once (an epoll of the loops' epolls).
- **100,000 idle connections are not quite idle** (the server lane): in 10 seconds with every connection silent
  SG spends 0.42 s of the processor, `tokio` 0.32 s and C 0.21 s. Not looked into yet.
- **The breaking point of the store** (the server lane, the 160th slice, choice 1760): with two cores for the
  server and a load that rises by 25,000 requests a second every 10 s, SG's store answered at most 404,000 a
  second in time, against 474,000 for C and 449,000 for `tokio`. A profile with the kernel at a fixed 350,000 a
  second found SG reading its sockets with `read`, which on Linux goes the longer way through the kernel (with
  SELinux on, as in the virtual machine of the measurements, a check of the policy on every read); C and Rust
  call `recv`. Now `recv` reads a socket and `read` only the pipe of another program: the processor for the same
  load fell by 8% (968 ticks of 10 ms in 8 s to 888; Rust 906, C 800), and the store answers 449,000 a second in
  time (C 474,000, Rust 449,000), the HTTP server 470,000 (C 499,000, hyper 321,000 in that run; 438,000 for SG
  before). What is left, 11% more of the processor than C at that rate, is in the runtime: a fill of a reader
  takes the lock of the socket's slot twice (whether it is dry, and marking it dry), a wake is a compare and swap
  on the task, and the parsing in SG takes about 2.5 times C's. Most of it is in the processor's own time (165
  ticks against C's 106; in the kernel 720 against 693). A look at the slot without its lock saved 1%, within the
  noise, and the watch every millisecond, a pool that never steals and `TCP_NODELAY` changed nothing (each
  measured); the parsing and the wake are the candidates left.

- **Eight tasks that spin on one queue** (`queue`, 1.39 times on macOS and 1.42 on Linux, choice 859): four producers and four
  consumers of `examples/queue`'s queue take 0.475 s against 0.341 s for the same queue in C. The loop of a
  producer is the same instructions as C's (one more for the bound of the list), and neither taking that
  test out, nor laying the queue and the counter out on the heap in C as SG lays them, nor giving the
  counter a line of the cache of its own changes either time; with two producers and two consumers SG
  takes 1.10 times C. What is left is where the eight threads that spin run on a machine of twelve fast
  and four slow cores, and in which order they start. Candidate: compare the order and the moment each
  task starts with C's, and the cores the threads land on.

- **A signal raised by the program wakes the relay** (`signals`, 1.27 times on macOS and 2.36 on Linux,
  choice 792): the handler writes each signal into the pipe, so the thread that waits for the pipe wakes
  and takes the lock even when `raise` has handed the signal on itself; without that wake the program
  takes the time of C on macOS and 1.16 times it on Linux. Candidate:
  the handler of a signal the thread raises itself keeps the kind in a variable of the thread, which
  `raise` hands on, so the pipe is left to signals from outside. Whether a handler may write a variable
  of the thread in every case is to be checked first.

- **Starting a task that ends at once** (`async`, 1.08 times C, choice 430): a task that never waits
  still takes its allocation and the atomic steps of a task that might be woken from another thread.
  Candidates: a task that ends before it first waits could skip those steps, and its block could come
  from a list of freed ones.

- **Reading the input the async way** (`asynclines`, 1.18 times C, choice 439; 1.37 before choice 491, 1.28 before 1550): a line the block holds
  is read in place, but each block of 64 KB is filled on a thread of input and output, and the task is
  woken through the pool after it: about 12 µs a block, 0.023 s of the 0.035 s. Candidates: the thread
  fills the next block while the task reads this one (reading ahead), a larger block for a reader read
  the async way, and a task that goes on on the thread that woke it.

- **Taking tasks out of a list** (`tasklist`, 1.24 times C, choice 451; the same tasks awaited one by one
  take 1.06): each list makes a group with a mutex and a condition, and each task takes its lock once when
  it ends and once when it is taken. Candidates: no group for a list whose tasks had all ended when they
  were added (a count is enough), a group kept from a list that ended for the next one, and room for the
  tasks reserved at once when the list is filled in a loop.

- **Waiting for the network the async way** (`echo`, 1.06 times C on macOS and 7.5 times on Linux in a
  container, choices 478, 674, 688, 700 and 1708-1714; 1.00 and 0.91 without async): since the 157th slice each
  thread of the pool waits on a loop of its own, and the server's end and the client's of a connection lie on
  the loops of two threads, so each message wakes the other's thread (0.89 times C on macOS and 7.2 times on
  Linux before); see the two ends that talk inside one program, above. Put on one loop, the sockets take 0.44 s
  on Linux, against 0.60 s now. Most of the distance from C on Linux is the C's: blocking threads woken by the
  system directly are cheap there (0.080 s), and Tokio was 5.5 times C too.

- **Datagrams the async way** (`udpecho`, 1.07 times C on macOS and 1.05 on Linux, choices 664, 674, 688,
  700 and 1708-1714; 1.01 and 1.00 without async): what is left of `echo`. On Linux the datagrams took 0.55
  times C with one loop, before the 157th slice. Candidates: those of `echo`.

## On Windows only

- **The calendar against the same algorithm in C** (`dates`, 1.16 times on Windows, choice 1077): on Windows
  the C takes a moment apart by hand with Howard Hinnant's `civil_from_days` (Windows' `gmtime_s` refuses a
  moment before 1970), and SG's calendar, written in SG in the language's own module, takes 0.123 s against
  0.106 s; the checked build takes 0.303 s. On macOS the C calls `gmtime_r`, which SG passes (0.29). Not
  looked into yet. Candidate: compare the C the back end writes for the calendar with the C by hand, and
  what the checked build tests in it.

- **Listing a folder on Windows** (`listing`, 1.14 times `FindFirstFileExW` in C, choice 1077; 0.99 on
  macOS): SG takes 0.316 s against 0.276 s, both with the same call and the names turned into UTF-8. Not
  looked into yet. Candidate: what SG does for each name besides (the kind of the entry, the check of the
  name) and the size of what it asks of each call.

- **The echo on Windows** (`echo` and `echosync`, 1.12 and 1.13 times C's threads with blocking sockets,
  choice 1077; 0.89 and 0.99 on macOS): eight clients sending 64 bytes and waiting for them, 20,000 times,
  through the loop of `\Device\Afd` and a completion port, against a thread for each connection that blocks
  in `recv`. Not looked into yet. Candidate: the round trips of a read that finds nothing and waits, against
  Tokio's numbers on Windows.

## The interpreter

- **The interpreter keeps every item in a cell of its own** (measured on 19 November 2026): a `List<T>` or
  a `T[N]` holds each item as a value on the heap of its own (`Rc<RefCell<Value>>`, about a hundred
  bytes), so that an item can be lent. A list of 64 million `int`s that a const builds while the program
  is compiled took 13 GB, where compiled code holds it in 256 MB, and a `byte[N]` takes about a hundred
  times its size. This touches programs run by the interpreter and the consts every build finds, not
  compiled programs. Candidate: hold plain data (numbers, `bool`, `char`, and structs and arrays
  of them) as its bytes, laid out as C lays it out, as Rust's evaluation of consts does (an allocation of
  bytes, with a mask of the bytes set and a table of the ones that hold a pointer); `raw.rs` already reads
  and writes values in that layout. Zig keeps an array of bytes as one slice of bytes too. The work is in
  how the interpreter lends an item that is no longer a cell, so it is a choice of its own.
