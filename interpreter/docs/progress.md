# How far it has come, and what is next

The order in which the language was built, slice by slice, each stretch with the numbers of its choices
(in [`design/decisions/`](../../design/decisions/README.md)), and the slices that come next.

## The back end, piece by piece

The back end, one piece of the language at a time, each compared with the interpreter, run under the
sanitizers and measured against C. Every example that runs in the interpreter and has no `test` block is
now compiled, and printing a value allocates nothing. Modules (`use`, `public`, `readonly`) are compiled
too, and with them the stock and invoice example. A `match` gives its value inside a larger expression, and
`try` and `?? return` stand after a value that owns memory and has no name; the few positions still refused are
listed in 81. The test command runs on compiled code too (`sg test --compiled`). Arrays (`T[N]`) and slices
(`read T[]`, `ref T[]`) are compiled, and a buffer on the stack takes the time of the same buffer in C.
The bit operators and numbers in hex and binary are in the checker, the interpreter and compiled code,
and the sensor example is written as the design wrote it. So are the integer types the design names, the
unsigned ones included, and a hash written with them takes the time of the same hash in C. So are `float`
and `double`, and the n-body simulation takes the time of the same program in C. So are `parse`, the
constants of the number types, the conversions that do not panic and the math methods, and reading a
million numbers out of text takes about the time of `strtoll` and `strtod`. So are `char`, going over a
text (`chars`, `split`, `splitWhitespace`, `lines`), `trim` and the searches, and the log summary is
written as the design wrote it. A text is built piece by piece where it is (`write`, `clear`,
`string.withCapacity`), and building a million lines takes the time of `sprintf` into one buffer in C.
A program reads and writes files whole, reads its input line by line into one text, and sees its command
line (`main(List<string> args)`); it writes to the standard error with `printError` and ends with a code of
its choice with `exit`. Reading two million lines takes 0.82 times `getline` in C, which does not check
that they are text. A file is read and written piece by piece through a `StreamReader` and a
`StreamWriter`, `Stdin`, `Stdout` and `Stderr` are such streams, and a writer left open in a local
variable is a compile error; copying the log line by line into another file takes 0.83 times the time
of `getline` and `fprintf` in C. A writer appends with `File.openAppend`, and bytes are read and
written piece by piece (`readBytes`, `writeBytes`); copying the log as bytes takes 0.94 times the time of
`fread` and `fwrite` in C. A value in `$"..."` takes a width and a letter, as in C# (`{n,5}`, `{b:X2}`,
`{mask:b8}`, `{n:D3}`, `{x:F2}`), and `parseHex` reads hex; printing two million lines with them takes
0.89 times the time of `printf` in C. `list.addAll(slice)` copies a slice of `Copyable` items to the end of
a list, and gathering a gigabyte of bytes in pieces of up to 255 with it takes the time of `memcpy` in C;
`sort` and `reverse` work on an array and through a `ref T[]`, and on a range in front of them. A pattern
stands inside a pattern (`Add(Number(0), right)`), a `match` looks at text, and an enum has methods; a
`match` on text takes the time of a chain of `memcmp` in C. Patterns are joined with `or`, numbers and
characters come in ranges (`'a'..='z'`), and `null`, `true` and `false` are patterns; sorting bytes into
the classes of a lexer takes 0.87 times the time of the chain of `if`s in C. The names of `is` reach along
`&&` (`if int.parse(read text) is Ok(value) && value > 0 { ... }`), and `is not null` and `is null` narrow
along `&&` and `||`, as in C#; in compiled code a name is one pointer set inside the condition. An
assignment computes its right side first and then finds the place, as Rust does, so the right side can
read a borrow of the value it replaces (`b = Box { n = r.n + 1 }`) and can grow the list it is written
into. `text.replaceAll(old, new)` and `text.repeat(n)` give a new text, and a call of them whose result is
thrown away is a compile error; rewriting lines with them takes 0.94 times the time of the same program in C.
So is every other built-in method that only gives a value (`line.trim()`, `xs.contains(3)` alone); since
the seventy-seventh slice every call that gives a value is, `removeLast` and the functions you write too
(`_ = xs.removeLast()` drops one on purpose). `!` swaps what
a test of `null` says (`if !(x is null)`), and after `while x is null { ... }` the variable is not `null`.
The names of `is not` are seen where it fails, after the `if` too (`if int.parse(read text) is not Ok(n) {
return }` and then `n`), and so are those of an `is` after an `if` whose `else` leaves.
`ulong.fromLittleEndian(data[at..at + 8])` reads a number out of exactly as many bytes as its type takes,
`n.toLittleEndian()` gives its bytes as an array, and the big-endian forms read and write the highest byte
first; XXH64 written with them takes 1.03 times the time of C, and 1.23 times in a checked build, where it
took 3.6.

## Several owners

A value can have several owners: `new shared x`, `share x` and `weak x`, and a type has a second name
with `using`. What a `shared checked` value points to is changed through any owner, each access checked
against the borrows in progress: a conflict through the same variable is a compile error, one through
another a panic with both lines. `read x as n { }` and `ref x as n { }` lend it for a block, a method takes
the owner with `read FolderRef this`, and the `shared` values that hold each other up when a program or a
test ends are reported. The design's folder tree and its cities and roads run as it wrote them (the
cities without the lambda). All of it is compiled too: a `shared` value is a pointer with the counts in
front of it, the borrows of a `shared checked` one are checked where the checker opened it, and a debug
build reports the leaks; the folder tree takes 1.53 times the same tree in C with plain pointers.

## Tasks

Tasks start with `Task.run(work, x)` and are waited for with `t.wait()`; a task that is not waited for is a
compile error, and what goes to a task owns all it holds. What several tasks change is a `shared locked`
value, reached only inside a `lock` block, and nothing in such a block waits, so locks and waits cannot
deadlock. In the interpreter a task runs on a thread of its own when it is waited for, and the output is the
same on every run; the counter of the syntax draft and the two files of the examples run as the design wrote
them.

All of it is compiled too: a task is a thread of the system with a stack of 8 MB, a `locked` value has a
mutex in front of its counts, the output stays whole and a panic names its task as in the interpreter, and
every program with tasks runs clean under the thread sanitizer. Four tasks adding to a locked total take the
time of the same program in C with threads and a mutex.

## Async

Async functions run in the interpreter as C# runs them: `async int count()` gives a `Task<int>` that has
started, `await` waits without holding the thread, and `Task.delay(ms)` lets time go by. Waiting the wrong
way is a compile error: `wait`, input and output that holds the thread and a function that does either, in
an async function; `await` in a `lock` block; an async function that awaits itself. Every task is a fiber
that gets the turn where another waits, and the clock moves only when all wait, so the output is the same
on every run. A task is cancelled only where it looks at its token, with `token.check()` or a delay given
the token, and a cancellation is an error of its own, `Cancelled`, which `try` turns into an `IoError`; files
and the input are read and written the async way, with a token that may come last.

Async functions, delays and cancellation are compiled too: an async function is a machine whose variables
live in a frame, `await f()` runs `f` in a frame inside the caller's, and a task is one allocation that runs
on the caller's thread until it first waits, then on a pool of threads, one for each core. Every such
program gives the interpreter's output and runs clean under the thread sanitizer, and what async adds to a
call that does not wait is 1.08 times the same calls in C. Input and output the async way is compiled as
well: the work is done on threads of input and output, started as they are needed, while the task waits,
and a read that the block of a reader already holds is done in place, so reading the input line by line
the async way takes 1.28 times the same program in C (without async, less than the time of C). Tasks are kept
in a list (`List<Task<R>>`) and waited for all at once (`await Task.whenAll(tasks)`, what each gave in the
order of the list) or as they end (`await Task.whenAny(ref tasks)`), and a list whose tasks are not all
waited for is a compile error; compiled, each list has a group that its tasks tell as they end. Streams
are opened, written and closed the async way (`File.createTextAsync`, `await writer.writeLineAsync(x)`,
`await writer.closeAsync()`), and a writer an async function drops on its way out is written and closed
the async way too, awaited where it is dropped, so a thread of the pool never writes a file; copying the
log line by line the async way takes 1.04 times the same program in C. The network is there too: a
`TcpListener` takes connections and a `TcpStream` receives and sends bytes, without async and with it;
compiled, a thread of the pool with nothing to do waits for every socket a task waits for, with epoll or
kqueue, and a cancel ends a wait for a socket, or for a line of the input, at once. An echo server with
eight clients takes the time of the same program in C with blocking threads the async way on macOS (1.03
times), and the time of C without async; with work in each message, the time of C. A connection
is read line by line through a `StreamReader` of its own (`conn.reader()`), which borrows it while it
goes on sending, and a line longer than 64 KB is an error; reading two million lines sent over the
loopback takes 1.14 times `getline` in C, which does not check that they are text (0.89 times without the
check): a reader checks the text of its block at once, not line by line. A fractional number is printed
with Dragonbox, in a third of the time of `printf("%.17g")` in C. Raw memory is there (`raw T`, `unsafe`,
`Memory.allocate`, `Memory.copy`, `Memory.free`), checked by the interpreter and not by compiled code, and a
ring buffer over it takes the time of C. C is called through `extern` blocks, linked with the libraries
they name, with C text (`CString`) and the items of a buffer (`buffer.pointer()`), and zlib wrapped in a
module of its own needs no `unsafe` where it is used; sixteen gigabytes through zlib's `crc32` take the time
of the same calls in C. A type of your own is `Disposable` (its `dispose` runs where its last owner ends)
and `Closable` (a local of it is closed), a type of C is only pointed at, and a struct of plain data goes
to C by value; zlib's stream, wrapped in a `Deflater`, compresses a gigabyte in 1.04 times the time of the
same calls in C. The clock is there: `Instant` for measuring, `Duration` for a span of time and `DateTime`
for the calendar in UTC, written in SG in the language's own module; ten million readings of the clock
take the time of `clock_gettime` in C, and taking ten million moments apart takes 0.29 times `gmtime_r`.
Random numbers are there too: `Random`, xoshiro256\*\* with a seed, the same numbers everywhere, and
`SecureRandom`, the system's bytes for keys and tokens; a hundred million numbers in a range, and a
gigabyte of the system's bytes, take the time of the same code in C. Folders, paths and the environment are there: `File.move` that never
writes over a file, `Directory.list` sorted the same way everywhere, `deleteAll` that never follows a link,
`Path`, and `Environment.get`; listing a folder of ten thousand files and asking about a file take the time
of `readdir` and `stat` in C. Another program is run and waited for (`Command`, with `run`, `output` and
`status`), with no shell, a table of the environment of its own, an input of its own written while its
output is read, and no descriptor of ours handed to it; starting a program takes 0.70 times
`posix_spawnp` in C, and keeping its output 1.49 times reading the pipe, all of it the check that it is text. The other places in "Slower than C"
wait. A program is also started and talked to while it runs (`start`, its pipes taken out of the
`Process` as a writer and readers, `wait`, `kill`, `terminate`), and one that is not waited for is a
compile error, or stopped where it ends; the async versions take a token, and a cancel stops the program.
A method can be async. Reading the output of a started program line by line takes 0.85 times `popen` and `getline` in C. A character is tested against the tables of Unicode 17.0 and a text changes its case by Unicode's full mappings, which takes a quarter of the time of `towupper` in C. Datagrams are sent and received (`UdpSocket`), and an address is read and printed in SG (`SocketAddress`); an echo over UDP takes the time of C without async, and 1.11 times C (1.49 on Linux) the async way. With them the plan made before a preview of the language (04, "Calling C, the direction") is done. The scheduler of async tasks came close to Tokio's in three slices written in the C of the runtime with no library: a thread of the pool waits on the loop of the network itself, a socket goes on the loop once, and each thread has a queue of its own that the others steal from. An echo over TCP takes 0.89 times C on macOS (1.86 before) and 7.4 times on Linux (28 before, where Tokio takes 5.5), and an echo over UDP 1.06 and 0.57 times C.

## Reaching what C reaches

No preview comes out while there is work that C can do and SG cannot (04, "Reaching what C reaches, the
direction", choices 703-718). The slices from the eighty-fourth close that gap in this order: a C file
compiled beside the program, the variables of C, `errno` and a pointer as a number; functions of SG that
C calls (`export`, `CFunc`); calls of C with a varying count of arguments and sqlite; libcurl; signals;
unions, bit fields, `packed` and alignment; atomics and `volatile`; a library built from SG; building
without an operating system; and a RISC-V kernel in QEMU. Windows is talked through after them. The eighty-fourth is
written: a C file is compiled into the unit of the program, so a function of it called in a loop takes the
time of the same loop in C, the variables of C are read and written inside `unsafe`, `errno` is read after a
call and made an `IoError`, and an address is a number when it has to be (719-730). The eighty-fifth is
written: C calls functions of SG (`export`), pointers to functions go both ways (`CFunc`), and a callback
reaches its state through the `void*` C gives back; ten million numbers sorted by `qsort` with a
comparator of SG take the time of the same sort with a comparator of C (731-749). The eighty-sixth is
written: a function of C that takes `...` is called with the types each value says, a number without a
type being an error there, and SQLite is wrapped in a module whose users write no `unsafe`, as fast as C
(750-760). The eighty-seventh is written: libcurl is wrapped the same way, with safe defaults (only
`http` and `https`, certificates always checked, a limit on what a response brings), `Memory.view` gives
the bytes C hands a callback to a list in one copy, and a call of C no longer holds up the other tasks in
the interpreter (761-778). The eighty-eighth is written: a program listens for signals and takes them one
at a time (`Signals.listen`, `next`, `nextAsync`), with no handler of its own, and `examples/shutdown` is a
server that ends cleanly on Ctrl-C or `kill`, tested with real signals sent from outside (779-799). The eighty-ninth is
written: unions, bit fields, `packed` and `align` are laid out as C lays them out on each system, Windows
included, and a binary header is read out of its bytes with no `unsafe` (800-829). The ninetieth is written:
atomic values with the orders of C11, checked when the program is compiled, and `volatile` reads and writes
of raw memory; `examples/queue` is a queue without locks for many producers and many consumers written with
no `unsafe` (830-859). The ninety-first is written: `sg build --library` and `--shared` make a static or
a shared library of SG with its header, C holds values of SG by their address and lends them back, and
`examples/calclib` is a calculator in SG that a program of C uses (860-885). The ninety-third is written:
`sg build --freestanding` gives an object for a machine without an operating system, which takes nothing of
C's library, gives a panic to the program's `onPanic` and takes memory from its `allocate`; what needs a
system is a compile error where it is written, and `examples/bare` runs with a machine of this computer
(895-913). The ninety-fourth is written: `examples/kernel` boots in QEMU's `virt` machine, writes to its
serial port and takes interrupts of the timer, its devices driven in SG and only the instructions of the
core in C, and links with nothing but its own object (914-927). With it, every program of the threshold
runs and is tested, so the threshold of 04 is reached. Next is Windows, talked through (718), and then a
preview of the language.

## Windows

Windows has been talked through (04, "Windows, the direction", choices 928-942): Windows 10 and 11 on
x86-64 and 64-bit Arm, built by Clang with C's library inside the program, tested by GitHub's machines with
Windows on every push to its branch. What does not need Windows comes first, on Linux and macOS: the
ninety-fifth slice is written: `CLong` and `CULong` (C's `long`, 32 bits on Windows), `Path.joinInside`,
`ExitStatus.Code(int)`, `C.lastError()` and `IoError.fromErrno` (943-962); the ninety-sixth is written:
`-L folder`, an `extern` block named by several names of its library, `sg test --compiled` without `fork`,
and a count of the blocks of values that reports a leak on every system (963-979). Six slices of Windows
follow; the first, the base, is written: the interpreter and compiled programs run there with the console,
files and paths, the clock, random bytes and the environment, `OperatingSystem.isWindows()` tells a
program its system, and what Windows does not run yet is a compile error that names its slice (980-998).
The second is written too: tasks, threads, locks, atomics and the async scheduler run on Windows, a delay
of a millisecond waits about a millisecond there, and a write to the console holds its stream (999-1014).
The third is written: the network and async input and output run on Windows, the loop asking the driver
under the sockets on a completion port, a port held alone, and a datagram that came back told as Linux
and macOS tell it (1015-1031). The fourth is written: other programs and signals run on Windows, a
program started through `CreateProcessW` with its words quoted as it reads them back, a batch file only
by its extension and escaped, named pipes overlapped on our end, Ctrl-C, Ctrl-Break and the console
closed as `Interrupt` and `Terminate`, and a command line read without `shell32` (1032-1049). The fifth
is written: C runs on Windows, the interpreter's wrappers a DLL looked for only in the program's folder,
the folders of `-L` and `System32`, a library of SG built as `x.lib` or `x.dll`, the DLLs of `-L` copied
beside a program, and the libraries of the examples taken from vcpkg on the CI (1050-1067). The sixth
and last is written: everything is tested on Windows, the program without an operating system and the
kernel in QEMU included, a debug build no longer needs the C compiler's library for a division of 128
bits, and the benchmarks are measured against C there too, most in its time and those that lean on
Windows' C library several times faster (1068-1079).

## What C has and SG lacked

Before a preview of the language (942) comes what the
language and its library still lack beside C. Consts first, decided with the statics before them (1080-1099):
the hundred-and-third slice is written: `const` at the top of a file and in a block, arrays whose items are
counted, the bytes of a file with `embed`, the count of an array from a const and a const in a pattern, its
value found when the program is compiled and kept in compiled code as one `static const` (1109-1119). The
hundred-and-fourth is written: a const given the value of any function, run by the interpreter when the
program is compiled, with what the function and those it calls do checked to give the same value on every
machine (1100-1108, 1124-1134). The hundred-and-fifth is written: statics, read only, `atomic` and `locked`,
a value known from the start lying in the program's data and any other found the first time it is used, once,
by whichever task comes first (1088-1097, 1135-1147). The lacks of the language measured against C23 and the
draft of C2y were put in a table, with what Rust, C#, Go and Zig have that costs nothing when the program runs,
and those taken come in three slices before the library's. The hundred-and-sixth is written: loops with a name
(`break outer`, `continue outer`), counting down with `(a..b).reversed()`, C#'s raw text and `const assert`,
which alone measures the size of a type; on the way, what a loop moved is followed at every way out of it
(1148-1166). The hundred-and-seventh is written: an enum with numbers of a type it names (`enum Block : byte {
Image = 0x2C }`, `(byte)block`, `Block.fromValue(b)`) and the operations on the bits of an integer (C23's
`<stdbit.h>`: `countOnes`, `rotateLeft`, `reverseBytes`, `log2` and the others) (1167-1192). The
hundred-and-eighth is written: the default value of a field (`int port = 80`), `p with { x = 3 }` and `a?.b`
(1193-1210); on the way, a `match` that gives a borrow keeps what it looks at lent. Then the library, by the
order of C11. The hundred-and-ninth is written: a reader and a writer of a file move with `seek(SeekFrom...)`,
`File.openWrite` writes over what a file holds, and `File.createTemp` makes a file only its user can open
(1211-1222). The hundred-and-tenth is written: the rest of `<math.h>` as methods (`sinh`, `cbrt`, `hypot`,
`erf`, `gamma`, `expMinusOne`, `roundToEven`, `exponent`, `nextUp` and the others), `abs`, `clamp` and the
saturating operations and conversions on integers, `tryNegate`, and `Memory.zeroSecurely` (1225-1248). The
hundred-and-eleventh is written: `binarySearch` on items in order, which gives the first equal one, with
`lowerBound` and `upperBound` (1249-1257). The hundred-and-twelfth is written: `lock ... when`, a block that
waits under a lock until its condition holds and is woken by every `lock` block on the value that ends, in
place of a condition variable that is told by hand, `await lock ... when` in async code (1259-1275). The
hundred-and-thirteenth is written: channels, `Channel<T>(n)` and the `Sender<T>`s it makes, which end for their
receivers when the last sender ends, with no `close` to forget (1276-1299). The hundred-and-fourteenth is written:
the local time zone, `DateTimeOffset` (`d.toLocal()`, `withOffset`, `DateTimeOffset.local`), and dates written
and read in RFC 3339 (1300-1317).

## Hardening

The hundred-and-fifteenth begins the hardening, before the preview: the
command line tested end to end, with its exit codes written down, and the few command lines it took that
were not usages refused; an output closed early ends a program the same way interpreted and compiled
(1318-1330); a review of the soundness of the checker, the boundary of `unsafe` and C, the C runtime and the
tool runs beside it. The hundred-and-sixteenth fixes what that review found, first the small faults, each in
one place: the files a build makes on the way are in a folder only its user opens (1332, 1333); an `export`
function is called from any thread, and C's names are not taken by one (1334); a struct with no fields
holds one byte (1335); a line is written with the lengths of its texts, the byte 0 and more than 2 GB
included, and faster (1336); eight faults of the tool itself, where a program made `sg` panic, overflow its
stack or wait, are errors or panics of the program (1337); what had no bound has one, said where it is met:
a value takes at most 2^61 - 1 bytes, a program nests at most 1000 deep, a const stops growing at 64 MB, a
value nested too deep for the interpreter's stack is a panic, and a large frame is touched page by page
before it is used, so a function never runs past its stack into another thread's (1338, 1341-1345); a
file a program takes in lies in the folder of the file that names it, a name goes to the linker whole, a
DLL is copied by its name only, a task lets go of its task while it has the turn, and C is lent a value at
the alignment of its type (1339); a function whose frame no stack holds is an error of `sg build` (1346). The
hundred-and-seventeenth and the next fix the rules of borrowing: what a part of a call lends lasts until the
call runs, with no exception (1340, 1347); an item a loop lends borrows the collection, borrows join where
ways join and are held into the next round of a loop, writing through a `ref` variable uses it, and
what `lock` and `read ... as` open is lent for their block only, a `ref` reached through `read` only
reads, what a collection keeps it keeps all of, a `Disposable` value uses its borrow where its block ends, and
what a `shared checked` value holds, looked at, is lent for its line (1340); compiled code finds the
receiver of a built-in method before it computes the arguments (1348);
and then the front end was fuzzed, its rules of safety tried with mutants and its programs fuzzed against
compiled code (1323-1325, 1349-1368), which found an end of a channel behind a pointer that `sg build` could
not compile, a `ref` of a `shared checked` value and an index that changes its text that compiled code turned
into reads of freed memory, and the loops whose overflow check is left out untested (1377-1382). An editor shows the first error as a
program is written and applies four hints with one click (`sg lsp`, `editors/vscode`, 1369-1376); the type
under the pointer, going to a definition and completion come after it.

## The site, and the library grows

The web site teaches the language in a book and pages of its library, every program on them run (1383-1397),
and what writing it found is fixed, each after a test that shows it: a build whose program would take the name
of its folder asks for `-o`, the tool's own output ends quietly when closed, a type of `std` named only in
`$"..."` is found, and a score of hints and messages lead to code that compiles, the index panic among them
naming `xs.get(i)` (1398-1402). The library grows before the reference (1406): hex and Base64 are written
(`Hex`, `Base64`, read strictly, 1407), CRC-32 (`Crc32`, sixteen bytes in a step, 1408, 1414, 1415) and
SHA-256 with HMAC (`Sha256`, `Hmac`, `Bytes.equalInConstantTime`, 1409) and the command line (`CommandLine`,
GNU's way of writing, 1410, 1423-1433) and JSON (`Json`, `JsonValue`, read strictly and tested with
JSONTestSuite, 1411, 1434-1445) and a log (`Log`, a level that is off building nothing, 1412, 1447-1456), and regular expressions (`Regex`, RE2's
meaning, a pattern of the program checked when it is compiled, and the general category of Unicode with
`c.category()`, 1457-1481), and compression (`Deflate`, `Zlib`, `Gzip`, zlib's bytes at every level, opened with a limit, 1465, 1466,
1482-1500), and zip archives (`ZipReader`, `ZipWriter`, checked before anything is read, 1467, 1490, 1491,
1501-1524).

## Trials against Rust and Zig

Real programs are now written in SG, Rust and Zig and measured against each other, each
difference a choice (1418, `trials/`): `grep` and `wc` first, which brought a lazy DFA to `Regex`, the C
compiler `CC` names and an `IoError` written as a sentence (1525-1539). What they found is made right before
the next trial (1543): `print` first, which writes into a block of the program's own, its lines put
together where they are printed (1544-1546), and ends compiled code where it cannot write, as it ends the
interpreter (1547); then reading lines and a search for a plain text, found by its two rarest bytes, and
UTF-8 checked faster (1549, 1550).

## Packages

Packages are here (1552, 1555-1583): a program's `sg.project` names
them, `sg add` fetches them by `git`, with no registry, pinned by the SHA-256 of their files in `sg.lock`
and kept in a cache, the versions chosen by minimal version selection, the `unsafe` of each allowed only
by the program, and a folder `internal` keeps a library's own modules to itself.

## The second review of safety

A second review of safety
(1589) came after them, and what it found is fixed first (1590): what a package or a project brings is
never run or followed, and a package is kept only as itself (1591); the rules of borrowing are made rules,
not cases (1592), and a generator of programs that borrow tries them (1593); the modules of `std` that read
what others write hold on what they are given (1594); compiled code does what the interpreter does where
it did not (1595, 1601-1603); and the tool holds on what a program or a folder gives it (1596, 1604).
A const is made of constant expressions alone, with no calls, as in C# and Go (1606-1608), and a range lent
as `ref` is counted among what a call lends (1610), and `copyFrom` copies the items of a window over as many,
inside one place too (1597, 1609), and Pike's machine copies a thread's slots once, as Rust's does (1598).
A text is put in a normal form of Unicode by the compiler's own tables, as its case is, and every line of
Unicode's tests gives what it should (1599, 1611-1614); zip names are compared in it (1615). The last
parameters of a function may have default values, which a call that leaves them out gets as if it wrote
them (1620-1624), the token of `std` among them (1623), and JSON is written to the depth it is read, which
`JsonOptions` changes for both (1600, 1617-1619).

## Every module measured

Every module of `std` is now measured against Rust and
Zig, and a new one comes in with its measurement (1553, 1629-1637): JSON first, where a `JsonValue` takes 32
bytes and not 96 and an object gives its values in order (1638-1640), then compression and zip, where a
match is compared and copied eight bytes at a time, a list is made at its size (`List<T>.repeat`,
`List<T>.withCapacity`) and a zip reader lends its entries and holds 20 MB for 100,000 of them
(1642-1652), then SHA-256, HMAC, CRC-32 and Adler-32, which go through the processor's own instructions
where it has them and lose to Rust and Zig by at most 1.3 times, from six to nine (1653-1658), then hex,
Base64 and Unicode, where hex and Base64 go through tables and blocks on the stack, a character is written
into a text without a call, and a text or a list kept by the program takes what they write (`encodeInto`,
`decodeInto`), from nine to twelve times slower down to 1.25 to 1.54, and the categories and normal forms
of Unicode were ahead of ICU4X and ICU4C as they stood (1659-1666), then time, random numbers and the log,
where a number written with a least count of digits no longer goes through `snprintf`, which wrote dates in
2.4 times the time of `chrono` and now in its time, the lines of tasks on macOS wait on a lock of the
program's own, the monotonic clock of macOS is the one Rust reads, and a moment in a time zone and a token
of the system's bytes are left slower on purpose or for later (1667-1677), then the network, where an
address is written into the text where it goes and reads ten million in the time of Zig and three quarters of
Rust's, a hex number in `$"..."` no longer goes through a format read at run time, a socket found empty or
full is waited for on the loop at once, as Tokio does, and each call of the network in an async function
takes 224 bytes of one room in its frame, where each took 600 of its own, so 1,000 connections hold what
Tokio's hold; the program measured stopped for good once, when the ports of the machine ran out and the task
that waited for clients was waited for, so the server closes first, a measurement stops at a run that hangs
or fails, and an exit with an error will cancel the tasks it lets go (1678-1690); then an exit with an error
lets its tasks go before it waits for them, as Swift's task groups and Kotlin's `coroutineScope` do: a wait in
them that can say so ends with `Cancelled`, and a delay or a channel without a token runs on (1691-1695).
The hundred-and-fifty-sixth slice measured servers on Linux: a store that speaks a part of Redis's protocol and
an HTTP server, each in SG, Rust, Zig and C under one load written in C, idle, under load, at fixed rates and for
30 minutes. SG holds the least memory of the three languages in all cases but two and the best tail where many connections wait,
and on one core answers more than C, but on four it gets 0.64 times the answers of Zig's threads with few
connections, because a request is read on one core and served on another; the pool now counts the cores the
program may run on, and `is Ok(text)` on a borrowed text names a `read string` (1698-1707).
The hundred-and-fifty-seventh gave each thread of the pool a loop of the network of its own, which a thread with
nothing to do waits on and is woken through; a socket goes on the loop of the thread with the fewest and stays
there, and the clock's thread looks at the loop of a thread busy with one task for long. With 50 connections
the store answers 1,342,000 times a second on Linux where it answered 578,000, 1.49 times Zig's threads and 0.91
times C, and the three systems work the same way; at 70% of the load the tail grew to C's, and two ends that talk inside one program
lost to `tokio`, both kept to be mended (1708-1714).
The hundred-and-fifty-eighth gave a server what keeps it standing: a `TaskGroup`, which holds tasks that give
nothing, lets go of each as it ends and has `whenRoom` wait while a limit of them run, and is waited for with
`whenAll`, which with a token cuts the tasks left and still waits for them; and `cancelAfter`, which sets the
time a source cancels itself again with one atomic write, so a silent connection is closed. The servers keep
their connections in a group. A token made and used in one line no longer keeps its source for ever, a task can
no longer be put on the heap where nobody waits for it, and `CancellationSource.after(0)` is cancelled at once
in the interpreter too (1715-1730). The group costs nothing on the servers' numbers; a limit on the time of a request costs 3 to 4% of the
answers, and beside 10,000 slow clients answers the busy ones within 807 µs at p99 where `tokio` takes 2.47 ms.
The hundred-and-fifty-ninth begins with a list of borrows, `List<read T>`, which follows each source on its own
and which `clear()` lets go of, so one list of pieces is filled again in every round of a loop (1731-1745).
Before it, a thread of the pool that went to sleep on its own loop of the network could lose the wake sent to
it, when the clock's watch held that loop for a short round just then, and wait for ever; it now looks at its
flag once it holds the loop.

## Next

Next comes the rest of the hundred-and-fifty-ninth (a reader that lends slices and `sendPieces`, the system's
`writev`, both built on the list of borrows), the tests of robustness, then HTTP/1.1; and a slice for the command line,
the environment and paths. Then the checks of the DFA's reads and a search in any case
with vector instructions; then a `jq` of JSON, a key-value store with a write-ahead log, a pool of workers,
an HTTP/1.1 file server, a GIF tool and an interpreter of Lox. XML is not in `std` (1551, in place of 1413)
and will be a package of the language's own. After them come the reference of the language on the site
(1394), the rest of the third stage of the generator of programs, which moves, and the preview (942) with
`sg doc` (1392).
