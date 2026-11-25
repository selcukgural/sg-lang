# SG against C

Each program here is written twice, in SG and in C by hand, and both go through the same C compiler with
`-O2`. The last column is the time of SG divided by the time of C: 1.00 is the same speed, below 1 SG is
faster. How the numbers are taken, and the rules that keep them fair, are in
[How it is measured](method.md); the same programs on Windows are in [SG against C on Windows](against-c-windows.md).

## In short

Of the 61 programs, 36 take the time of C within 5% (the middle of all is 1.00),
14 are faster and 11 are slower.

- **Slower:** `tree` 1.53, `queue` 1.39, `gunzip` 1.31, `signals` 1.27, `tasklist` 1.24, `asynclines` 1.18, `sha256` 1.17, `json` 1.15, `netlines` 1.09, `async` 1.08, `echo` 1.06. Each is in [Slower than C](slower-than-c.md), with what would fix it.
- **Faster:** `floats` 0.23, `dates` 0.29, `counts` 0.35 (a hash table that finds eight slots at once, choices 1882 and 1893), `show` 0.47, `table` 0.52, `copy` 0.54, `asynccopy` 0.59, `checksum` 0.69, `bits` 0.79, `lines` 0.79, `rewrite` 0.86, `classes` 0.87, `bitfields` 0.91, `bytes` 0.94. Several of these are C's library (`printf`, `gmtime_r`) against SG's
  own code, not something C cannot do; the notes below say so program by program.

The numbers move by a few percent from run to run, and sixty small programs are not a benchmark suite.

## The table

Measured with `python3 bench/run.py` in `interpreter/`, on an Apple M3 Max with macOS and Apple clang 21 on
arm64: the best of five runs, `-O2` for all three, the C written by hand with no checks.

| Program | SG, release | SG, overflow checked | C | release / C |
|---|---|---|---|---|
| `fib`: recursive `fib(40)` | 0.277 s | 0.383 s | 0.282 s | 0.98 |
| `primes`: trial division below five million | 0.493 s | 0.476 s | 0.499 s | 0.99 |
| `collatz`: longest chain below three million | 0.359 s | 0.570 s | 0.359 s | 1.00 |
| `words`: bytes read, words compared, each word copied to the heap and freed | 0.874 s | 0.888 s | 0.854 s | 1.02 |
| `list`: a linked list of a million nodes built with `new`, walked and freed, twenty times | 0.238 s | 0.243 s | 0.236 s | 1.01 |
| `expr`: a tree of enum values built, evaluated twenty times with `match`, rewritten and freed | 0.309 s | 0.302 s | 0.303 s | 1.02 |
| `parse`: numbers read out of text, an error found three calls deep and passed up with `try` | 0.290 s | 0.320 s | 0.293 s | 0.99 |
| `vector`: a list of a million numbers filled with `add`, changed through `xs[i]`, added up with `for`, turned around and freed, six hundred times | 0.331 s | 0.529 s | 0.324 s | 1.02 |
| `counts`: a million numbers counted in a `HashMap` of fifty thousand keys, read back and a third removed, twenty times | 0.106 s | 0.106 s | 0.305 s | 0.35 |
| `show`: two million structs and half a million lists of numbers printed, one on each line | 0.176 s | 0.183 s | 0.377 s | 0.47 |
| `lookup`: three hundred million lookups in a list of records of 128 bytes, through a function that gives back a `read Record?` | 0.498 s | 0.488 s | 0.491 s | 1.01 |
| `buffer`: 32 bytes on the stack written as hex into a buffer of 64 through a `ref byte[]`, added up through a `read byte[]` and a range of it, fifteen million times | 0.280 s | 0.290 s | 0.283 s | 0.99 |
| `bits`: a sieve below sixty-four million with one bit for each number, then thirty million numbers written seven bits at a time into a buffer on the stack and read back | 0.322 s | 0.321 s | 0.407 s | 0.79 |
| `hash`: XXH64, the 64-bit xxHash, written with `ulong`, over one megabyte, two thousand times, the bytes read eight at a time with `ulong.fromLittleEndian` | 0.128 s | 0.129 s | 0.128 s | 1.00 |
| `nbody`: the sun and the four giant planets moved twenty million steps in `double`, a square root for each pair at each step | 0.661 s | 0.662 s | 0.679 s | 0.97 |
| `numbers`: a million lines of whole and fractional numbers read ten times with `long.parse` and `double.parse` and added up | 0.186 s | 0.191 s | 0.196 s | 0.95 |
| `floats`: a million fractional numbers, every other one of seventeen digits, written into a text with `$"{x}\n"` three times over and read back once (the C writes each with `%.17g`, always seventeen digits) | 0.176 s | 0.179 s | 0.777 s | 0.23 |
| `fields`: a million lines like ` Ada17 , 120, -45 ,6789` gone over twenty times with `lines`, `split`, `trim`, `long.parse` and `chars` | 1.228 s | 1.259 s | 1.202 s | 1.02 |
| `lines`: two million lines of a log (125 MB) read from the input with `Stdin.readLine`, each checked to be UTF-8, and counted | 0.070 s | 0.070 s | 0.089 s | 0.79 |
| `copy`: the same log read from a file with `File.openText` and written to another, each line with its number in front, through `File.createText` | 0.155 s | 0.160 s | 0.285 s | 0.54 |
| `bytes`: the same log read in pieces of 4 KB with `readBytes` and written with `writeBytes` through a writer from `File.openAppend` | 0.030 s | 0.030 s | 0.032 s | 0.94 |
| `table`: two million lines printed with widths, hex, binary and two digits after the point (`{i:x8} {a,10}  {name,-8}\|{x,10:F3}  {mask:b16} {code:X6}`) | 0.287 s | 0.296 s | 0.547 s | 0.52 |
| `blocks`: eight million pieces of 1 to 255 bytes (about a gigabyte) gathered with `addAll` into a `List<byte>` that is emptied at 64 MB | 0.035 s | 0.035 s | 0.035 s | 1.00 |
| `keywords`: a million words sorted forty times into twelve keywords and the rest by a `match` on text | 0.199 s | 0.200 s | 0.194 s | 1.03 |
| `classes`: ten million bytes sorted twenty times into the classes of a lexer by a `match` of ranges and `or` (`'a'..='z' or 'A'..='Z' or '_'`) | 0.239 s | 0.299 s | 0.274 s | 0.87 |
| `rewrite`: two hundred thousand lines gone over twenty times, the tabs replaced by four spaces and a word by another with `replaceAll`, and an indent made with `repeat` | 0.529 s | 0.525 s | 0.614 s | 0.86 |
| `tree`: the design's folder tree, 111,111 `shared checked` folders that look at their parent with `weak`, each adding a size to every folder above it, built and freed twenty times | 0.095 s | 0.102 s | 0.062 s | 1.53 |
| `tasks`: four tasks at a time over the numbers below 400,000, ten times; each adds up Collatz lengths and adds them to a `shared locked` total every 100 numbers | 0.115 s | 0.188 s | 0.115 s | 1.00 |
| `async`: for each number below a million, an async function starts a task for one Collatz length and awaits another in place; nothing has to wait | 0.244 s | 0.372 s | 0.226 s | 1.08 |
| `asynclines`: `lines` the async way, the two million lines read with `await Stdin.readLineAsync` in an async `main` (the C is that of `lines`) | 0.104 s | 0.105 s | 0.088 s | 1.18 |
| `tasklist`: in each of 200,000 rounds, eight async tasks for Collatz lengths taken with `Task.whenAll` and eight with `Task.whenAny`; none has to wait (the C keeps each result in a block it allocates, in an array for the list) | 0.476 s | 0.689 s | 0.384 s | 1.24 |
| `asynccopy`: `copy` the async way, the log read with `await reader.readLineAsync` and written with `await writer.writeLineAsync` in an async `main` (the C is that of `copy`) | 0.170 s | 0.169 s | 0.289 s | 0.59 |
| `echo`: an echo server and eight clients in one async program over the loopback, each client sending 64 bytes and waiting for them to come back 20,000 times (the C blocks, with a thread for each connection and each client) | 1.588 s | 1.599 s | 1.497 s | 1.06 |
| `echosync`: `echo` without async, the server's connections and the clients on threads of `Task.run` (the C is that of `echo`) | 1.108 s | 1.118 s | 1.115 s | 0.99 |
| `echowork`: `echo` with work in it, the server going through 300,000 steps of arithmetic for each message before it sends it back, and each client sending 300 (the C is that of `echo`, with the same work) | 0.376 s | 0.372 s | 0.369 s | 1.02 |
| `netlines`: the two million lines of `lines` sent over the loopback by one task and read line by line by another with `await reader.readLineAsync(ref line)` (the C blocks, with `getline` from a `FILE` over the socket, which checks nothing) | 0.088 s | 0.088 s | 0.081 s | 1.09 |
| `ring`: a ring buffer of its own over raw memory (`raw long`, `unsafe`), four hundred million numbers in at one end and out at the other through 4096 items (the C is the same over `calloc`) | 0.328 s | 0.326 s | 0.328 s | 1.00 |
| `crc`: sixteen gigabytes through zlib's `crc32`, 64 KB of an array at a time, handed over with `block.pointer()` (the C calls zlib through its header) | 0.418 s | 0.417 s | 0.421 s | 0.99 |
| `deflate`: a gigabyte at level 1 through zlib's stream, 64 KB at a time, behind the safe `Deflater` of `examples/zlib` (the C calls zlib through its header) | 1.001 s | 1.001 s | 0.962 s | 1.04 |
| `clock`: ten million times `start.elapsed()` on the monotonic clock, each compared with the one before (the C calls `clock_gettime`) | 0.191 s | 0.180 s | 0.189 s | 1.01 |
| `dates`: ten million moments between 1900 and 2200 taken apart into year, month, day, hour, minute, second and the day of the week by the calendar of the language's own module, in SG (the C calls `gmtime_r`) | 0.121 s | 0.295 s | 0.413 s | 0.29 |
| `rng`: a hundred million numbers from 0 up to 1000 of a `Random` with a fixed seed, added up: xoshiro256\*\* and Lemire's way to a range, in SG in the language's own module (the C is the same algorithm) | 0.112 s | 0.112 s | 0.112 s | 1.00 |
| `secure`: a gigabyte of the system's random bytes, 64 KB at a time with `SecureRandom.fill` (the C calls `arc4random_buf` on macOS, `getrandom` on Linux) | 0.241 s | 0.242 s | 0.242 s | 1.00 |
| `listing`: a folder of ten thousand files listed a hundred times with `Directory.list`, each name with its kind, in the order of the bytes (the C reads with `readdir`, copies each name and sorts with `qsort`) | 0.407 s | 0.436 s | 0.412 s | 0.99 |
| `status`: a million `File.info` of one file, its kind, size and time of the last write (the C calls `stat`) | 0.618 s | 0.627 s | 0.619 s | 1.00 |
| `inline`: a function of a C file beside the program (`extern "./inline_helpers.c"`, a step of a hash written `static inline`) called five hundred million times in a loop (the C is the same loop with the function in its own file) | 1.072 s | 1.059 s | 1.044 s | 1.03 |
| `callback`: ten million numbers sorted by C's `qsort` with a comparator written in SG (`export int compareInts(raw void a, raw void b)`), which C calls through the pointer it is given (the C is the same sort with the comparator in C) | 0.904 s | 0.907 s | 0.902 s | 1.00 |
| `sqlite`: a million rows put into an SQLite database in memory with one prepared statement in one transaction, each text made by `snprintf` (which takes `...`), then read back with another (the C is the same program through SQLite's header) | 0.320 s | 0.321 s | 0.321 s | 1.00 |
| `curl`: a thousand fetches of a body of one megabyte over the loopback with libcurl, on one kept connection, from a server that is a task of the program; each piece of the body added to a list by an `export` callback through `Memory.view` (the C is the same program through libcurl's header, its server a thread, each piece copied with `memcpy`) | 0.163 s | 0.161 s | 0.160 s | 1.02 |
| `signals`: a hundred thousand rounds of `Signals.raise(Signal.User1)` and `listener.next()` (the C is a handler that writes into a pipe the program reads itself, after each `raise`) | 0.318 s | 0.313 s | 0.250 s | 1.27 |
| `bitfields`: four million pixels of 16 bits in bit fields of 5, 6 and 5 made brighter field by field, five hundred times, then four thousand IPv4 headers (bit fields of 4) read out of their bytes with `fromBytes`, twenty thousand times (the C is the same bit fields and `memcpy`) | 0.326 s | 0.323 s | 0.358 s | 0.91 |
| `atomics`: four tasks add to two counters five million times each, one sequentially consistent and one `Relaxed` (the C is pthreads and the `__atomic` builtins) | 0.274 s | 0.256 s | 0.268 s | 1.02 |
| `queue`: four tasks push 500,000 numbers each through a queue without locks of 1024 places while four take them out (the C is the same queue with pthreads and `__atomic`) | 0.475 s | 0.456 s | 0.341 s | 1.39 |
| `statics`: CRC-32 of 64 KB, 4096 times, byte by byte through a table that is a static found the first time it is read, so every read looks whether it is found (the C keeps the table in a plain global filled before the loop) | 0.626 s | 0.626 s | 0.633 s | 0.99 |
| `library`: a program of C calls a static library built from SG fifty million times through a handle (`ref Stats`), then hashes 256 MB of its own bytes with it through `Memory.view` (the C is the same program linked against the same library written in C) | 0.506 s | 0.507 s | 0.503 s | 1.01 |
| `checksum`: four gigabytes through `Crc32` of `std`, 64 KB of an array at a time, folded by the processor's carry-less product since choice 1656 (1.095 s before, through sixteen tables; the C calls zlib's `crc32`, which on macOS uses the processor's CRC instructions too) | 0.077 s | 0.076 s | 0.111 s | 0.69 |
| `sha256`: a gigabyte through `Sha256` of `std`, 64 KB of an array at a time, through the processor's SHA instructions since choice 1657 (3.138 s before, in plain SG; the C calls OpenSSL 3.5's SHA-256, which uses them too) | 0.413 s | 0.411 s | 0.353 s | 1.17 |
| `json`: a JSON document of 100,000 records (34 MB) read five times by `Json.parse` of `std`, written in SG, and walked once (the C reads it with cJSON 1.7.19) | 0.773 s | 0.789 s | 0.675 s | 1.15 |
| `log`: a billion `Log.debug($"...")` calls while the level is `Info`, in a loop that does little else, so no text is built: the static of the level, a load and a compare (the C tests an atomic global the same way; between 1.01 and 1.10 over several runs) | 0.811 s | 0.808 s | 0.805 s | 1.01 |
| `gzip`: sixteen megabytes of text made of words compressed at level 6 by `Gzip.compress` of `std`, written in SG (the C calls zlib 1.2.12 of macOS; the bytes are the same, so both print the same length and CRC-32) | 0.469 s | 0.471 s | 0.495 s | 0.95 |
| `gunzip`: the same text compressed at level 1 and opened twenty times by `Gzip.decompress` of `std`, each time with its CRC-32 (the C calls zlib's `inflate`) | 0.477 s | 0.654 s | 0.364 s | 1.31 |

The code of each program is in [`interpreter/bench/`](../interpreter/bench): `name.sg` and `name.c`.

## Notes on each program

What each of the first programs found, and what was changed because of it. The programs from `tasks` on
are explained in the choices of the slice that added them, in [the choices of each slice](../design/decisions/README.md).

### `fib`, `primes` and `collatz`

A release build runs the three integer programs in the time of C. The "overflow checked" column is what that
check alone costs, and why the rule became Rust's on 12 November 2026.

### `words`

`words` is the first program with
memory: the few percent are what the checks that stay cost (every index and every range is checked, and a
range may not cut a character in half), against C that checks nothing. The first version of that C program
looked 20% faster: the C compiler had removed its `malloc` and `free` because the copies were never read; the
program now compares the copies.

### `list`

`list` follows pointers: a `Node*?` is one pointer, `is not null` is one
comparison, and there is nothing else to pay for. Its first version took 1.10 times the time of C, because
freeing a node called a function for its (already empty) `next`; the loop that frees a chain now drops the
other fields itself.

### `expr`

`expr` is the first program with an enum: a `match` is a test on one byte, and the C it
is compared with is a tagged union and a `switch`. The part that builds, rewrites and frees the tree first took
1.13 times the time of C: after a value was moved, the drop of the (empty) variable was still written, and the
C compiler did not remove it. Those drops are no longer written (choice 24) and that part takes 1.03 times.

### `parse`

`parse` is the first program with errors as values: eighteen million lines are read, a third of them end in
an error that is found three calls deep, handed up with `try` and looked at with `match` or dropped with
`??`. The C it is compared with returns an error code and gives the value through a pointer. The two take the
same time (0.99 to 1.02 over several runs): a `Result` is a tag and a value, `try` is one test, and nothing
is allocated.

### `vector`

`vector` is the first program with a list. A `List<int>` is what the C program keeps by hand (a block, a
length, a capacity, twice the room when it is full), `for x in read xs` is a pointer walking the block, and
the index check of `xs[i]` inside `for i in 0..xs.length()` is removed by the C compiler, which sees that
it cannot fail. Its first version took 1.28 times the time of C, and none of that was the list: the number
that was added was `(i * 7 + round) % 1000`. In C a signed overflow is undefined, so the C compiler may
assume `i * 7 + round` never overflows and turn it into a counter that goes up by seven; SG's release
arithmetic wraps, which is defined, and the same compiler then keeps two more instructions in a loop of
eight. With `i % 1000` in both programs they take the same time. This is a cost of the overflow rule that
the three integer programs did not show: where a loop is nothing but arithmetic on its counter, wrapping
can cost what C's undefined overflow saves. Rust wraps in release in the same way and gets the same code.
The overflow-checked build took 2.1 times the time of C; since the fifty-third slice the `sum += x` of
`total` runs without its test (315), and what is left (1.6 times) are the tests of `xs[i] * 3 + 1`.

### `counts`

`counts` is the first program with a hash table, and was the first that was clearly slower than its C: 1.20
times. All of it was the hash. The C program hashes a key with one multiplication, as a C programmer does;
SG used SipHash-1-3 with a key drawn when the program starts, as Rust does, so that someone who chooses
the keys (a request, a file) cannot make them all land in one place and turn every lookup into a walk
through the whole table. With the C program's hash put into the generated code the two took the same time
(0.316 s against 0.319 s): the table itself cost nothing. The safe hash stayed the default (choice 50).
Since slice 181 (choice 1893) a key of a fixed size, a number as here, is hashed by a folded multiplication
with that secret key, as hashbrown's foldhash does, and text keeps SipHash; with the slots found eight at a
time since slice 180 (choice 1882), `counts` takes 0.35 times the time of the C, whose table finds a slot one
after another (0.106 s against 0.305, 25 November 2026).

### `show`

`show` prints values: a struct with two numbers and a list of eight. Before the twenty-fifth slice it took
1.22 times the time of C (0.44 s against 0.36 s): every printed value was written into a text that was
allocated and freed. It now goes into one block that the program keeps (and gives back when one line
made it larger than 64 KB), and a number inside a value is
turned into digits by a few lines of the runtime instead of `printf`. The C it is compared with is what a C
programmer writes first, a `printf` with a format, and `printf` reads that format again on every call; that
is the whole difference, not something C cannot do. The honest reading is "printing a struct costs no more
than a `printf`", not "twice as fast as C".

### `buffer`

`buffer` is the first program with arrays and slices: an id of 32 bytes on the stack is written as hex
into a buffer of 64 bytes through a `ref byte[]`, and the hex is added up through a `read byte[]` and
through a range of it, fifteen million times. The C it is compared with passes a pointer and a count by
hand and checks nothing. The two take the same time (0.99, the same over three runs): the arrays are
C arrays and a slice is the pointer and the count the C program passes by hand. Every index and the
range are checked in the C that is generated; here the C compiler, once it has put the small functions
into `main`, sees both lengths and removes every one of those checks, so the built program holds none.
That is this program, not a promise: where a length is not known, the check stays, one comparison that
is never taken. The overflow-checked build took 2.6 times as long: the overflow test of `sum += b` in
`total` kept the C compiler from adding several bytes at a time (choice 313). Since the fifty-third slice
the loop is written twice and runs without that test for as many rounds as the sum has room for, which
for a `long` adding bytes is every round, and the checked build takes the time of C too (0.290 s against 0.733 s,
choice 320); the tests on `i * 2`, on `i * 2 + 1` and on the cast to `byte` are still there.

### `bits`

`bits` is the first program with the bit operators, in two halves. The sieve keeps one bit for each
number in a list of `long`s and sets and tests it with `|=`, `&`, `<<` and `>>`: that half takes the
time of C (0.25 s for both). A shift by `n & 63` is one instruction in both programs, and the cut of
the count that a release build writes is the one the instruction does anyway. The other half writes a
number seven bits at a time into ten bytes on the stack and reads it back, and there SG takes 0.06 s
against 0.15 s. That is not the bit operators. SG tests every `target[at]` against the ten bytes, so
the C compiler knows the loop cannot go round more than ten times; it writes the loop out, keeps the
bytes in registers and never reads them back from memory. The C program states no such bound. With
one line added to it (`if (at >= 10) abort();`) it takes 0.07 s. The honest reading is "the bit
operators cost what they cost in C", and that a check can tell the C compiler something a C program
left out; not "faster than C". The overflow-checked build takes the same time as the release build:
the shifts by a constant and by `n & 63` need no test, and the others are one comparison.

### `hash`

`hash` is the first program with the unsigned types: XXH64 on `ulong`, with rotations, `>>` that brings
in zeros, and multiplications that wrap on purpose (`wrappingMultiply`). The C it is compared with reads
eight bytes at a time with `memcpy`, as xxHash does. SG first put the number together from the bytes,
after cutting the eight out of the data (`data[at..at + 8]`): one check, and the C compiler joined the eight
reads into one. Written without the cut, with each of `data[at]` to `data[at + 7]` checked by itself, the
same program took 3.5 times the time of C (choice 136). The overflow-checked build took 3.6 times: the
additions `at + 8`, `at + 16` and `at + 24` that give the place of each read are checked, and those tests
stood between the eight reads. Since the fifty-first slice it reads with `ulong.fromLittleEndian(data[at..at
+ 8])`: the range is checked once, the count of eight is then known to the C compiler, and the bytes are one
load. The release build takes the time of C as before, and the checked build took 1.23 times, since its
tests no longer stood between eight reads (choice 307). Since the fifty-second slice the sum `at + 8` that
ends the window is tested with the window (choice 309), and the checked build takes the time of the
release build.

### `nbody`

`nbody` is the first program with fractional numbers: five bodies in a `Body[5]` on the stack, ten
pairs at each step, a `sqrt` for each pair. The two take the same time (0.97 here; 1.00 when timed by
hand before): a `double` is C's `double`, `sqrt` is one instruction, and nothing on a fractional number
is checked, so the overflow-checked build takes the same time too. Every `bodies[i]` is checked against
the five; the C compiler sees that `i` and `j` stay below five and removes those checks. Both programs
are compiled without fused multiply-add, the rule for SG (choice 142): with it, clang on arm64 fuses
`dx * dx + dy * dy` and the C program prints other last digits than the interpreter. Both print the same
two energies, digit for digit.

### `numbers`

`numbers` reads numbers out of text: a million lines, whole numbers and fractional ones in turn, read
ten times over and added up. The SG program finds each line end and gives the line to `long.parse` or
`double.parse`; the C program gives the place to `strtoll` or `strtod`, which find the end themselves.
The text is built once at the start, in both from whole numbers (`12.1345` is two of them and a point),
and in both into one block taken for the whole text: `write` in SG, `sprintf` in C. Reading alone takes a
little less time than in C (0.135 s against 0.143 s for the ten rounds), and building the text takes the
same (0.056 s against 0.055 s). Until the thirty-seventh slice the text was a million small ones, each
from `toString`, put together with `string.join` (0.071 s). The first version took 2.7 times
the time of C, for three reasons. The fractional numbers were built with `toString`, and printing a
`double` tried up to seventeen lengths (since the sixty-eighth slice one Dragonbox, 497); every text made by `toString` was written
twice, once to find its length (now once, choice 166); and every fractional number went to `strtod`
through a copy, where most are short enough to be one correctly rounded division (choice 164). The
overflow-checked build takes the same time: the digits are added up by the runtime on 128 bits, where
nothing can overflow, and the one checked addition for each line is a test that is never taken.

### `floats`

`floats` writes fractional numbers: a million, every other one with all seventeen digits
(`512.0471906447226`) and the others with a few (`1234.567`), each written into one text with `$"{x}\n"`,
three times over; the last text is then read back and added up, which shows that each number read back
as itself (both programs print the same sum). The C program writes each number with `%.17g`, the usual way
in C to write a number that reads back: `printf` cannot give the fewest digits, so it always gives
seventeen. SG finds the fewest with Dragonbox (497), one or two multiplications by a power of ten from a
table; until the sixty-eighth slice it wrote each number with 1, 2, ... 17 digits and read each back,
and took 4.627 s, six times the time of C.

### `fields`

`fields` splits text: a million lines of a name and three numbers, with spaces around the fields, `\r\n`
at the ends and an `é` in every seventh name, gone over twenty times. Each line comes from `lines()`, its
fields from `split(',')`, each field is trimmed and read with `long.parse`, and the characters of the
name are counted with `chars()`. The C program is what one writes by hand: `memchr` for the line and the
comma, a loop that skips ASCII spaces, `strtoll` straight on the field (it skips the spaces itself), and
the bytes that do not continue a character counted. Going over the text takes about the time of C (1.15 s
against 1.08 s for the twenty rounds; SG's `trim` reads characters and knows Unicode's spaces, the C loop
only ASCII), and building it with `write` takes about the same (0.149 s against 0.145 s; 0.17 s when
each line was a text of its own, joined at the end). A
search for a separator of one byte without the `memcmp` after `memchr` was tried and made the program
slower (1.54 s, in every run); why was not looked into, and the change was left out.

### `lines`

`lines` reads its input: two million lines of a log, 125 MB with some `\r\n` ends and a `café` here
and there, read with `Stdin.readLine` into one text that is used for every line, and counted (the
lines, their bytes, the longest, the ones that start with `ERROR`). The C program reads them with
`getline` into one buffer, as one does. SG takes 0.82 times its time, and checks that every line is
UTF-8, which `getline` does not: it finds a line with `memchr` in a block of its own where `getline` goes
through `FILE*`, and it checks the block at once, so the lines inside the part known to be text cost
nothing more (choice 491). The check first took the program to about 1.15 times; looking at eight bytes
at a time while they are ASCII brought it to 1.08 (choice 202), and checking a block at a time to 0.82.

### `copy`

`copy` reads the same log from a file, opened with `File.openText` and read with `readLine` into one
text, and writes each line with its number in front (`17: INFO 350 ...`) with `writeLine` through a
writer from `File.createText`, 140 MB in all. The C program reads with `getline` and writes with
`fprintf` into a `FILE*` with a buffer of 64 KB. SG takes 0.54 times its time (0.155 s against 0.285 s;
0.93 before the check of a block at a time, 0.83 before the runtime wrote its lines itself, choice 1336): it
checks every line and still reads faster, as `lines` showed, and the writing is the runtime's own few
conversions into the writer's block, where C's `fprintf` reads its format on every line.

### `bytes`

`bytes` copies the same log as bytes: `readBytes` into a buffer of 4 KB on the stack, and `writeBytes` of
each piece into a writer from `File.openAppend`, after the file is emptied. The C program does the same
with `fread` and `fwrite` on `FILE*`s with buffers of 64 KB. SG takes 0.94 times its time (0.030 s against
0.032 s, the best of fifteen runs: the program is short, and one run of five gave a cold start of 0.09 s).
Both are copies between blocks; neither looks at the bytes.

### `table`

`table` prints two million lines, each one `print` of a formatted text with a width on two numbers and on a
name, hex with eight and with six digits, sixteen binary digits and a fractional number with three digits
after the point. The C program is one `printf` a line (`%08llx %10lld  %-8s|%10.3f  %s %06X`), with the
binary digits put together by a small function first, since `printf` has no conversion for them. SG takes
0.52 times its time (0.89 before the runtime wrote its lines itself, choice 1336). Its first version took 1.30 times: the fractional number was written by `snprintf`
and then copied by `printf` (the format read twice), and the spaces of a width were a conversion of their
own beside the value. Now the digits after the point are worked out by a function of the program, exactly
(choice 232), and a width is one conversion; `printf`'s own `%10.3f` is the slower part of the C program.

### `blocks`

`blocks` gathers bytes as a GIF decoder gathers the data of an image, which comes in sub-blocks of at most
255 bytes: a range of an array on the stack is added with `addAll` to a `List<byte>`, eight million times.
The C program keeps the same block, length and capacity by hand and adds each piece with `memcpy`. The two
take the same time (0.035 s over three runs of fifteen): `addAll` is one test for room and one `memcpy`.
Adding the same bytes one at a time with `add` took ten times as long (0.344 s).

### `keywords`

`keywords` sorts words as a lexer sorts the names it reads: a `match` on text with twelve keywords and `_`.
The C program is the chain a person writes by hand, one test of the length and one `memcmp` for each
keyword, and that is what the arms compile to (`sg_eq` is the same two tests); the million words are
copied to the heap first in both. 1.03 over seven runs is within what moves from run to run. A pattern
inside a pattern is not measured on its own: it compiles to the tests of the tags and the values joined
by `&&`, which is what `expr` already measures one level at a time.

### `classes`

`classes` sorts bytes as a lexer sorts the characters it reads: letters and `_`, digits, spaces, brackets
and operators, each class one arm of ranges and single bytes joined by `or`. The C program is the chain of
`if`s a person writes by hand, `c >= 'a' && c <= 'z'` and `c == '_'` joined by `||`, and that is what the
arms compile to: a range is two comparisons (one, when an end is the first or the last value of the type),
an `or` is `||`. The 0.87 is the C compiler's doing on two programs that say the same; the checked build pays
for the overflow test of the sum, not for the patterns.

### `rewrite`

`rewrite` rewrites lines as a formatter of source text does: each line gets its tabs replaced by four
spaces, then one word replaced by another, and an indent of up to seven steps is made, so each line makes
three new texts that are freed at once. The C program does the same with blocks it allocates and frees,
and finds each place as SG does, `memchr` for the first byte and `memcmp` for the rest; with `memmem` in
its place the C program took 0.70 s (0.84), because the `memmem` of macOS is slower on these short
needles. Most of both programs is `malloc`, `memcpy` and `free`.
The names of an `is` along `&&` (forty-fifth slice) are not measured on their own either: each is one
pointer set inside the condition where its test holds, `(s.tag == 0 && (r = &s.u.v0.m_radius, 1)) &&
*r > 10`, which is the C a person writes with the same names, and a narrowing writes nothing at all, so
the narrowing under `!` and after a `while` (forty-ninth slice) is not measured either. Nor are the names
of `is not` and the names seen after a statement (fiftieth slice): the same pointer, set in the same
place, under a `!`.

### `tree`

`tree` is the folder tree of the design's examples (05), the first program with several owners: each
folder owns its subfolders through `shared checked` values in a list and looks at its parent with `weak`,
and each new folder adds a size to itself and to every folder above it, getting an owner of each with
`share cur.parent` on the way up. The C program is the same tree with plain pointers: a parent pointer, an
array of the subfolders, and one free of the whole tree from the root. Of the 0.033 s between them, about
0.010 s are the atomic counts (the same C with plain `++` and `--` took 0.086 s), about 0.003 s the checks
of the borrows (0.084 s without them), and the rest is what the model asks for and C does not do: each
folder is 32 bytes larger (the counts and the borrows), the walk up takes and lets go of an owner at each
step, and each folder is freed when its count reaches zero, its `weak` parent let go with it. The checked
build pays for the overflow test of `made` and of the sizes (choices 347-355).
