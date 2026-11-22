# Slower than C, and could be better

Places where a measurement found SG slower than the same program in C and a fix is in sight. They are kept
here to be talked over when their time comes; none of them breaks a program, and none is fixed by turning a
check off.

Since the trials (choice 1418) the list also holds the cases where SG is slower than Rust or Zig. Each entry
says what is slower, by how much and against what, the choice that measured it, and what would fix it.

## The library

- **A regular expression where the DFA reads every byte** (`grep`, 1.5 times Rust's `regex` and 1.8 times
  Zig's PCRE2 on the address, 1.9 and 2.3 times on `-i error`, choices 1535, 1536, 1541): Rust's `regex` looks for an inner text in any case with
  the processor's vector instructions (Teddy), where SG's `(?i)` search goes a byte at a time, and unrolls its
  DFA's loop without checks, where SG's checks each read of its table against the table's end; PCRE2's JIT
  makes machine code of the pattern. Candidates:
  a search for a short text in any case that the C compiler can make vector code of; a loop over the table
  that reads through `unsafe` once the row is known to be inside it. `findAll` over 100 MB takes 1.4 to 45
  times Rust's (`\bcache\b` the most), for the same reasons and because Rust's looks for an inner text
  through the whole text with vector instructions before its DFA starts.

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

- **The network the async way, against Tokio and Zig** (`net`, choice 1689): a stream of 1 GB in pieces of
  64 KB takes 0.172 s against 0.153 s for `tokio` (1.12 times), 50,000 connections one after another 4.460 s
  against 4.061 s for Zig (1.10), datagrams 1.05 to 1.07 times Zig's threads, and echoes 1.01 to 1.03 times
  `tokio`; the same programs with functions that block take the time of the best of the two. The time is in
  the calls of the system: `tokio` spends a third of SG's time in `recvfrom` for the stream, most likely
  because the task that sends and the one that receives run on one thread there, while the bytes are still in
  its cache. A send the socket takes only part of and a receive that gives fewer bytes than there was room
  for already mark the socket not ready, as `tokio` does (1687). Candidates: keep a task woken by the loop on
  the thread that woke it, as `tokio`'s slot does for the driver, and measure on Linux, where the servers run
  and epoll may place them otherwise. A red line since 22 November 2026: on the network SG is never to be
  worse than the best rival, under load or not.

- **Each connection of async code holds 1.4 KB** (`net`, choice 1690): 1,000 connections at once, a task
  on each side, hold 3.8 MB as `tokio`'s, but each further connection costs 1.39 KB against 0.80 KB for
  `tokio` and 0.51 KB for Zig. Most of it is the frames of the two tasks, 672 and 560 bytes, which keep
  every temporary of the function in a field of its own and, in the room of the call of the network, a
  wait whose delay (72 bytes) only a wait for the clock uses. Candidates: temporaries that are never alive at
  once share a field, as the calls of the network now share one, and the wait for a socket carries only what
  it uses.

- **A server on more than one core** (the server lane, [Servers](server.md), choice 1698): 50 connections
  with a request in flight on each get 578,000 answers a second from SG's store, 607,000 from `tokio`'s, 900,000
  from Zig's thread for each connection and 1,472,000 from C's thread on each core (0.64 times the best rival,
  0.39 times C); HTTP the same. On one core SG answers 396,000 a second, more than C (349,000) and Rust (360,000):
  the program is not slow, the spread over cores is. Each answer costs SG 2.7 µs of the kernel on four cores and
  1.4 µs on one, C 1.4 µs on both, with the same calls of the system: SG's loop of the network is waited on by
  one thread, which wakes the tasks of the sockets onto its ring, and the other threads take them from there, so
  a request is read by one core and served by another (a wake of `futex`, the socket's data in another core's
  cache). `tokio` is built the same way and does the same. Candidates: a loop of the network on each thread,
  each socket on the loop of the thread that first waits for it and kept there, tasks taken by another thread
  only when one has too many; the wakes of other tasks through an `eventfd` on each loop.
- **The tail with 1,000 connections and 16 requests in flight on each** (the server lane): the answers a second
  are those of the others (2.1 million, the load's own limit), but p99 is 15.0 ms and p99.9 19.2 ms, against
  10.4 and 10.9 ms for C and 11.3 and 11.9 ms for `tokio`: some connections wait while others are served.
  Candidates: those of the server on more than one core, and a look at the slot of the last task woken, which
  runs a woken task before those waiting.
- **A server that takes each connection through one task** (the server lane): with a connection for each
  request SG answers 149,000 a second, Zig 198,000, C 202,000, with the least time of the processor of all
  (5.0 ms for each thousand, Zig 14.4): every connection goes through the task that holds the list of tasks, a
  task to take the next connection and one for the connection, and `Task.whenAny` looks for the task that ended
  through the list and moves those after it, so a server with many connections pays for each that ends.
  Candidates: a group of tasks that takes its ended tasks out by itself, which a server hands its connections
  to, and `whenAny` that finds its task without a search.
- **One connection, and smaller gaps** (the server lane; one run, which the median of three is to confirm): one
  connection with a request in flight gets 27,000 answers a second from SG, 34,000 from Zig and 33,000 from C and
  `tokio`, and with 16 in flight 497,000 from SG and 580,000 from `tokio`; HTTP at 70% of the slowest's answers a
  second has a p99 of 292 µs in SG and 189 µs in hyper; 10,000 connections that each ask once a second hold
  53.2 MB in SG and 50.5 MB in `tokio`; in the long run of 30 minutes (1,000 connections, each opened again after
  1,000 answers) SG holds 22.2 MB and `tokio` 13.3 MB, neither growing. Not looked into yet; the first is likely the wake between cores of the
  server on more than one core.
- **100,000 idle connections are not quite idle** (the server lane): in 10 seconds with every connection silent
  SG spends 0.45 s of the processor, `tokio` 0.36 s and C 0.22 s. Not looked into yet.

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

- **Copying a file the async way** (`asynccopy`, 1.04 times C, choice 464; 1.09 before choice 491; 0.83 without async): the
  same hand-over as above, for the reader's blocks and the writer's: each block of 64 KB filled or
  written out goes to a thread of input and output and the task is woken through the pool after it,
  about 0.04 s for some four thousand blocks. Candidates: those of the reader, and for the writer, a
  block written out while the task fills the next one (writing behind), waited for only when that one
  is full too.

- **Taking tasks out of a list** (`tasklist`, 1.24 times C, choice 451; the same tasks awaited one by one
  take 1.06): each list makes a group with a mutex and a condition, and each task takes its lock once when
  it ends and once when it is taken. Candidates: no group for a list whose tasks had all ended when they
  were added (a count is enough), a group kept from a list that ended for the next one, and room for the
  tasks reserved at once when the list is filled in a loop.

- **Waiting for the network the async way** (`echo`, 0.89 times C on macOS and 7.4 times on Linux in a
  container, choices 478, 674, 688 and 700; 1.03 and 0.91 without async): since the eighty-first slice the
  thread the system wakes for a socket runs the task, since the eighty-second a socket goes on the loop
  once, and since the eighty-third each thread of the pool has a queue of its own that the others steal
  from. Tokio's echo takes 0.42 s on Linux and 0.97 s on macOS, against 0.55 s and 1.00 s for SG. What is
  left on Linux is mostly wakes, about 450,000 calls of `futex`: a round of the loop that wakes more than
  one task wakes a thread to take some, and a thread that takes a task while nobody waits on the loop
  wakes one to wait there (676, 0.03 s of it). Candidates: Tokio's way of letting the loop go to the next
  thread that parks instead of waking one (676 was taken so that a long task does not keep the other
  connections waiting), and a thread that spins a little before it sleeps. Most of the distance from C on
  Linux is the C's: blocking threads woken by the system directly are cheap there (0.074 s), and Tokio is
  5.5 times C too.

- **Datagrams the async way** (`udpecho`, 1.06 times C on macOS and 0.57 on Linux, choices 664, 674, 688
  and 700; 1.01 and 1.00 without async): what is left of `echo` on macOS. Candidates: those of `echo`.

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
