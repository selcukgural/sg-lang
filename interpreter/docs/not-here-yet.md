# Not here yet

- **Of default values of parameters (1620-1628)**: named arguments (`connect("a", retries: 5)`), so a call
  can leave out a parameter that is not last; a default found when the program runs (Kotlin and C++ run
  any expression at each call); a default carried by a `Func` value.
- **Of two windows of one place (2011-2015)**: the check knows a variable declared with its window and given
  no other, and the parts of one call. A window taken from a window (`var inner = ref low[1..]`), a `read`
  variable given another window, a window kept in a struct or a list, and an item that grows through its own
  method beside another item (`rows[i].addAll(read rows[j])`; `var row = ref rows[i]` first is checked) are
  refused beside a window of the same place, as before. The items of a list of lists are counted in the outer
  list (`grid[i][j]` beside `grid[i][k]` is one item, `grid[i]`). Two windows given to two threads at once.
  A window named through a variable that holds the whole place (`ref all[..1]`, with `var all = ref names`)
  beside a window of the place itself is refused, however far apart (195).

- **Of packages (1552, 1555-1583)**: a package in a folder of a repository (Go's `sub/v1.0.0` tags), an
  address that leads to another through `?go-get=1`, versions before a release, branches and commits, a
  mirror on the network and a page of packages (1575, 1577, 1552); `vcpkg` and `pkg-config` for the
  libraries of C (1560). Git cannot limit what it downloads, so a repository that is too large is found
  only once it is down (1574). A folder `tmp-` that an `sg` stopped while fetching leaves in the cache is
  not cleaned, and nothing takes old packages out of the cache yet.

- **Of the output (1543-1547)**: on Windows, the message of an output that cannot be written is C's text of
  the error compiled and Windows' interpreted (1547). The padding inside a struct of
  plain data lent to C is left as C leaves it, whatever the stack held, where the interpreter writes it 0:
  a `memcmp` over it differs between the two (found by `ok_extern_libc`, whose struct has no padding since).
- **Of zip archives (1501-1524)**: encryption (ZipCrypto, AES) and the methods other than stored and deflated
  (bzip2, LZMA, Zstandard); a reader of one entry as a stream (1511); the comments of the archive and of its
  entries; the mode of Unix and links (an entry that is a link reads as a file that holds its target); an
  archive in memory, or one read from a source that cannot seek; adding to an archive that is written; a level
  other than 6; archives on several disks.
- **Of compression (1482-1500)**: what a gzip header holds
  (name, time, comment), given when a program asks for it; flushing in the middle of a stream (zlib's
  `Z_SYNC_FLUSH`), a preset dictionary, zlib's strategies (`Z_RLE`, `Z_HUFFMAN_ONLY`) and its smaller windows
  and memory levels; the `async` versions of a `Compressor` and a `Decompressor`; a `Decompressor` over a
  `TcpStream` or another source than a `StreamReader`, which waits for interfaces or a push of bytes.
- **Of regular expressions (1457-1481)**: matching bytes (`read byte[]`, 1470); the scripts of Unicode in
  `\p` (`\p{Greek}`) and its long names (`\p{Letter}`); POSIX's leftmost-longest match (1473); the groups of every match at once (Go's `FindAllStringSubmatch`), a
  replacement made by a function, and going over the matches without a list. The tests of Rust's `regex`,
  which 1468 named, are not taken: RE2's are, and the comparison with Go's `regexp` was run while the slice
  was written, not kept as a test. `(?i)` with `\p` goes over every code point up to U+1E943 once in a
  program (the table of each category's foldings), which takes 3 ms compiled and a quarter of a second in
  the interpreter.
- **Of fuzzing for differences (1361-1368)**: the generator writes stages 1 and 2, and the borrows of stage 3
  (1593); moves of other values than lists, and tasks with channels, compared in any order (stage 4), are still
  to come. The reducer takes statements and blocks away, not parts of an expression.
- **Of editor support (1369-1376)**: one error at a time, since the checker stops at the first; the type of what
  the pointer is over, going to where a name is defined and the names of a file (part 3), and completion as you
  type, which needs a parser that goes on after an error (part 4). Four hints have a fix so far; the others are
  shown as text and get theirs one by one. Not published to the Marketplace or Open VSX: that comes with the
  preview. A panic of the checker is caught and shown as one error, but no program is known that makes one, so
  that way is not tested.
- **Of the hundred-and-fourteenth slice**: the offset of the system's time zone is read again at each call
  (`toLocal`, `local`), each a call of the C library (`localtime_r`), which on glibc takes a lock; a cache of
  the rules is a candidate if a program that converts many moments asks for it. On Windows the exact zone of
  a run cannot be set from outside, so only `ok_time_offset`, which checks the zone of the machine against
  itself, runs there (1310).
- **Of the count of waiting tasks (193)**: compiled code shows the wait of the task started last on a thread of
  its own, as the interpreter does; where only async tasks wait, at several places, the place is the last wait of a
  task on one of the threads of the pool, which may not be the interpreter's. A wait for a thread of input and
  output that a cancel ended still holds the count while the call of the system it began goes on, so a deadlock
  after it is seen only once that call returns. Tasks that wait for each other while others go on are not seen, in
  compiled code or in the interpreter, as in Go.
- **Of `select` (192)**: an arm that waits on what is not
  a channel (a task, a socket, a delay: `Task.whenAny` and a token do those today); a condition on an arm; an
  item sent out of a field kept in it when its arm is not chosen (1999: it is computed and dropped).
- **Of the hundred-and-thirteenth slice**: a `for` that awaits (`for await job in jobs`, with
  `linesAsync`), a channel of a `StreamWriter` (1293), and a `Sender` this function did not make by name followed
  by the rule of 1285 (1294).
- **Of the hundred-and-twelfth slice**: a time limit and a cancellation of `lock ... when` (`cnd_timedwait`; 1264).
- **Of the hundred-and-eighth slice**: `a?.b = 5` and `a?[i]` (1200); a default found where the value is built,
  as C#'s initializer (1194); a default on a field of a variant, on one that borrows and on one of a union
  (1203); `with` on a variant of an enum, and in a const; a field narrowed by `a.b?.c is not null` (fields are
  not narrowed); `?.` on a `Result` (`try` and `??` open it).
- **Of the hundred-and-seventh slice**: an enum open to numbers that are no variant (Zig's `_`); an enum with
  numbers in a signature of C, a struct of plain data or raw memory (1176); a variant's number named by the
  variant alone (`Image + 1`: it is `(byte)Block.Image + 1`), or given by a function (1184); flags joined with
  `|` (a `uint` and consts do it); the list of the variants (`Block.values()`); `tryNextPowerOfTwo` (1181).
- **Of the hundred-and-sixth slice**: a range goes down only by one (no step, Rust's `step_by`), and only a
  range is gone over backwards: a list is gone over from its end by its positions,
  `for i in (0..xs.length()).reversed()`. C#'s `$$"""`, where a brace is written once, is not taken (1154).
- **Of consts**: a text inside an array, a struct or an enum (1110); an array of arrays, as anywhere.
- **Of statics**: a static that holds a function is copied to a local to be called (`var f = Double`, 1140); a `static
  locked` value aligned to more than 16 bytes is refused by the C compiler (1145). Without an operating
  system a circle through a function value spins instead of panicking (1142).
- **Of Windows**, after its six slices: no thread sanitizer there (1011), and a library built from SG is not run
  under the sanitizers there (1062). Job objects, so that a program started ends with the one that started
  it (934), and services of Windows (933). A DLL a DLL of `-L` needs and that lies elsewhere is not copied
  beside the program (1054). Windows on x86-64 is not measured against C (1076). (Two tests were red on
  GitHub's machines until 18 November 2026, both from the tests, not from SG: `shutdown_on_signals` closed
  a console through a window that is only a stand-in there (1046 says how it closes one now), and the
  server of `examples/curl`'s tests, by all that was seen, ended at a reset from a client that gave up,
  which took the next request down with its listener; a reset there now ends only that connection.)
- **Of datagrams**: multicast, sending to every host of a network (broadcast), the options of a socket (the
  time to live, the size of its buffers), a look at a datagram that leaves it in the socket (`peek`), and the
  zone of a v6 address (`fe80::1%en0`), so a link-local address cannot be answered (648).
- **Of other programs**: groups of processes and a session of its own (slice 197), a terminal of its own
  (pty, slice 198); a stream of a program given an open `File` or a socket (Rust's `Stdio::from`; a
  path is given, 2017); a wait with a time limit is `waitAsync` with a token of
  `CancellationSource.after(ms)`. A program let go of by `detach` in the interpreter writes what it
  wrote to an output it shared with the run straight to the output of the process once it has ended. A program
  run by the interpreter sees a pipe for its output and standard error (609), and what a started one
  writes there comes out when it is waited for (629). A `Process` named by `is Ok(process)` is lent, so
  the rule that it is waited for does not see it; it is stopped where the value it is lent from ends.
- **Of raw memory**: a place in it lent (`read p[i]`, `ref p[i]`, a method called on `p[i]`, 511); items that
  own memory (the design's `put`, `take` and `moveItems`); `Memory.resize`; the address of a variable (85,
  `Memory.addressOf`). In the interpreter a window of `Memory.viewRef` over memory C gave does not see what
  C writes there after it was made (819).
- **Of the layout of C**: `packed(2)` and the other packings of `#pragma pack`; an alignment written on one
  field; `Memory.offsetOf`; `toBytes`; bit fields in a union, and in a `packed` struct (808); a union or a
  struct without a name inside another; a struct by value to a function of the program's own C file (824).
- **Of a program without an operating system**: machines of 32 bits (906); a `HashMap` and a `HashSet`,
  whose key would have to come from the machine; the functions of `<math.h>` written in SG or taken from
  the compiler's builtins where the machine has the instruction; `double.parse`; `{x:F}` beyond 18 digits
  or for a number too large or too small for the exact way, which is a panic (904); a C compiler other
  than `cc` or Clang for a target (`CC` names a Clang: a cross GCC, `riscv64-unknown-elf-gcc`, does not take
  `--target`, 921); a panic on a second core while the first is told is not
  told (899); `memcpy` and its kin are loops of bytes (905); `sg test --compiled` of such a program; the
  tests of a root module whose `extern` block names functions of the machine, which the interpreter cannot
  link (908).
- **Of the kernel**: supervisor mode, under OpenSBI or its own machine mode below it, and pages of virtual
  memory (914); more than one core (the others wait in `_start`); interrupts of devices through the PLIC,
  and so input from the serial port; a timer for each of several tasks; a kernel for 64-bit ARM or x86-64;
  `sg build` linking it itself with a linker script, or taking a file of assembly (916, 920); the panic
  of a second core while the first is told, and a fault inside `onTrap` (the vector would run on the same
  stack again).
- **Of a library built from SG**: constants in the header (C's program writes the numbers the functions
  give, as `main.c` does); a text given back to C other than into its buffer; a `T*` inside a plain struct
  or a list; a header of its own for a program's `export` functions; `pkg-config`; names of the header
  with a prefix chosen for them; Windows (874). C can still give a freed or forged address to compiled
  code, which only the interpreter catches (865), as C can give one to any library of C.
- **Of atomics**: `fetchMax`, `fetchMin`, `fetchNand` and `fetchUpdate` (a loop of `compareExchangeWeak`
  does their work); atomic values of 128 bits; waiting on an atomic value until it changes (C++'s `wait` and
  `notify`, a futex); an atomic `shared T`; an order given as a variable (835); a queue of
  `examples/queue` for values of any type, which waits for generics. In the interpreter an atomic value in
  raw memory is read and written as plain memory, so a thread of C that changes it at the same time is not
  seen in one step (850), and since one task runs at a time a program that leans on an order weaker than it
  needs runs right there; compiled tests run under the thread sanitizer.
- **Of calling C**: a `va_list` (`vprintf`), a function
  of SG that takes `...`, and a `CFunc` to a function that takes it (753); the address of a `CString`
  kept by C, a folder of libraries looked in relative to the program's own place (`$ORIGIN`), `pkg-config`,
  frameworks of macOS, Windows. A C function whose name the C library gives another symbol (`stat` on
  some systems) is bound to the plain name. A variable of C is lent to nothing (`ref opterr`), and its
  address is not taken. The C files of one program share one unit, so two of them cannot both define a
  `static` function of one name (730); a C file sees the program's flags, `-std=c11` among them, so
  assembly is written `__asm__`. A variable of the C library keeps its value from one program to the next
  when the tests run them in one process (`opterr`). A C file does not call an `export` function that
  takes or gives a struct by value by its name (747); `CLong` has no arithmetic of its own (947), and
  C's other types of a size that differs (`size_t` is `nuint` already, `wchar_t` is 16 bits on Windows)
  have no type of their own;
  the interpreter runs a function C calls only on a thread that is in a call of C (736).
- **Of the debugger (1943-1949)**: the names of SG in place of those of the C (`total` for `v_total_1`), and
  printers of `List`, `HashMap` and `string` for gdb and lldb, as `rust-gdb` has; the text of the language's own
  module, which lies inside `sg` and is named `<built-in>/std/...`; a path written shorter into the program
  (Rust's `--remap-path-prefix`); the columns of a line. In an optimised build given `-g` a line that only
  computes may have no place to stop on, its arithmetic inlined from helpers of the runtime that keep the lines
  of the C.
- **Of tests**: running the compiled tests at once, several processes side by side (974); the bytes and
  the places of the blocks a debug build reports as not given back, which a build under the sanitizers
  shows (975).
- **Of signals**: the signals of faults (SIGSEGV and the others), SIGALRM, SIGCHLD and those of job control
  (SIGTSTP, SIGCONT, SIGTTIN, SIGTTOU); real-time signals and the value they carry; how many of one kind
  came (796 for one that comes as the last listener of its kind ends); a signal to a group of processes.
  A handler of C set inside `unsafe` and a listener of the same signal are not checked against each
  other (787).
- **Of `examples/curl`**: async requests (libcurl's multi interface on the loop of the network),
  cookies, a proxy, uploads from a file, a choice of HTTP/2, decompressing a body, and a way to turn the
  check of certificates off (there is none, on purpose). The numbers of libcurl's options are written in
  the module with their names in a comment, since SG has no constants of its own.
- **What the borrow check does not do yet.** A type with more than one source (`struct Match<from text, from
  pattern>`); storing a borrow from outside into a value reached through `ref` (the design's
  `where r from name`); a borrow as the error of a `Result`; a `match` used as a value that gives a borrow
  is not followed. A list of borrows is `List<read T>` only: no `List<ref T>`, and no borrows in a map, a
  set or an array (1732). A list of windows (`List<read byte[]>`) has no `get` and no `removeLast`, which
  would give a slice that may be missing (1743).
- `swap(ref a, ref b)`.
- A text constant has the type `string` here, not the design's `read string` (04, "String constant"). What the
  design wants of it is here all the same: it is not allocated or freed, has no capacity, and its first write or
  growth copies it, so a constant goes where a `string` or a `read string` goes with no `.clone()`. What differs:
  `var s = "x"` is a `string`, and nothing else: a constant goes where a `read string?` goes as it is too
  (`greet("")`, 1403).
- **Found writing the site (1402), in compiled code**: three things the interpreter runs and `sg build`
  refuses with "not in the C back end yet" and a hint to name the value first: `try` inside a larger
  expression (`print((try command.output()).trim())`), a text and then a value that lends with `ref` in one
  `$"..."` (`$"{pause} {path} {random.nextInt(1, 7)}"`), and a `$"..."` inside another. Since the
  hundred-and-sixty-sixth slice (choice 1817) a `try` reached through calls and methods with only reads
  before it is computed first (`args.add(try f())`, `b.put(2, try word(i))`); one after an index, a call or
  a value moved is still refused, as running it first would change what happens.
- The names of an `is` inside a `!` around a whole chain (`if !(r is Ok(v) && v > 0) { return }` and then
  `v`, 295), and a note that names the `break` which keeps a variable a `T?` after a `while` (289). An `is`
  with names on a value that owns memory, made by a call after the first `&&` or `||`, in compiled code
  (268, 299).
- A name for a value whose pattern is tested at the same time (Rust's `n @ Number(_)`), a text tested
  for how it starts (choices 243, 248), a constant as an end of a range (`int.MaxValue`), a range open at
  one end (`5..=`), `..=` outside a pattern (254, 255). A pattern of an `or` inside the parentheses that the
  ones before it already take is not caught (256).
- Interfaces of your own (`interface`, `where`). `Disposable` and
  `Closable` on an enum. A `Disposable` value that holds a `raw T` going to another task. `.toString()` on a `Formattable` struct or enum.
- Generics other than the built-in `Result<T, E>`, `List<T>`, `HashMap<K, V>` and `HashSet<T>` (generics of your own are deferred
  in the design), and so an alias with type parameters (`alias Link<T> = ...`).
- The task of `Task.whenAll` kept in a variable to be awaited later (`var all =
  Task.whenAll(tasks)`), and a `for` over a list of tasks (a `break` would leave tasks); a method or a function of a struct
  started as a task (a `Func` is a function written at the top level). Of async: `linesAsync` (a `for`
  that awaits), an async function or method kept as a `Func`. A token cancelled while a file is
  read or written does not stop the work (433); one cancelled while a task waits for the network or for a
  line of the input that can make the program wait does (471, 472). Of the network: a stream received from
  by one task and sent to by another (and so `send` through a `shared` stream), the reader of a connection
  kept in a field or given back (it carries a borrow; `StreamReader<from conn>`), a writer of a connection,
  `sendfile`, and the block of a reader let go while its connection is idle, as Go's pool does (1741). One
  function for a `TcpStream` and a `UnixStream` (there is no interface of the program's own, so a server that
  listens at a port and at a path writes the code of a connection twice), and so HTTP over a socket of the
  machine (`HttpConnection.create` takes a `TcpStream`, 1960). Of sockets of the machine: the abstract names of
  Linux, datagrams, who the other side is (`SO_PEERCRED`), a descriptor sent over one, and `socketpair` (1964).
  The sizes of a socket's buffers, `SO_LINGER`, `SO_REUSEPORT` and timeouts of a call of the network (1968).
  `checked` on its own, in a value with one
  owner (`checked int hits` in a plain struct, the design's `RefCell` outside `Rc`). `List<shared T>.clone()`,
  since a list has no `clone` yet, so the design's hint "loop over a copy" is not given. A conflict through two
  items of one list (`xs[i]` and `xs[j]`) is left to run time even when `i` and `j` are the same name.
- An alias of a collection used to build it (`Names()`, 330). Running the tests of one module only.
- A collection changed inside its own `for` through another `ref` to it is not caught (it is part of the
  borrow check); the direct forms are. Here a list is gone over by position and a map or a set as it was when
  the loop started.
- `reserve`, `clone`; a `Hashable`
  type with its own `equals` or `hash`.
- What Unicode has beyond the tables of 635 and the normal forms (1599): grapheme clusters, sorting by a language, the width
  of a character, the title case, the Turkish and Azeri case mappings (640), and names in Unicode in the
  source. Replacing only the first places
  (`replacen`), replacing in place without a new text; a `padLeft` is a width: `$"{name,10}"` (229); the positions
  of the characters (`charIndices`); splitting at most n times. Writing the type of an
  iterator, and a `for` over a type of your own that has `next` (with generics). `text.capacity()` and
  `text.reserve(n)`; a `ref string` given to a `format` method in place of its `Formatter`.
- A number is written in decimal, hex and binary (`{n:x}`, `{n:b}`) and read in decimal and hex
  (`parse`, `parseHex`), not in binary. C#'s other letters (`N`, `C`, `P`) and formats made of
  characters (`{x:0.00}`), a fill other than a space, centring, and a width that comes from a variable
  (choices 224, 229); `e` and `g` are C's, not C#'s (1892), and not in a program built with
  `--freestanding`, which has no `snprintf`. A fractional number is not read out of bytes or written into
  them (`double.fromLittleEndian`); its bits are `toBits` (1891). Of `<math.h>`: `nextafter` toward a
  number (`nextUp` and `nextDown` do it), the sign of Γ beside `logGamma`, `lround` and the other roundings
  to an integer (`round()` and then a conversion do it), `fdim`, `fenv` and the rounding modes (1225-1236).
  `saturatingDivide`, `saturatingNegate`, `saturatingAbs` and a saturating shift.
- Arrays of arrays, `read T[]?`, an array as a key, looking for a run of items in a slice. `addAll` of
  items that are not `Copyable`, moving every item of one list into another, adding a range of a list to
  the same list (`extend_from_within`), a sort that does not allocate (choices 235, 236, 239). A sort or
  a search in order with a function that compares, or by a key (`sortBy`, `binarySearchBy`; 1254).
- A panic of a shift names no way out: there is no `tryShiftLeft` yet.
- `IoError` does not carry the path
  (choice 195). `PermissionDenied` and `NoSpace` are mapped but have no test: the tests cannot make a file
  they may not read or fill a disk.
- **Of folders and the environment**: changing the owner of a file (`chown`), watching a folder, patterns of
  names (`*.txt`), a listing of a whole tree, all the variables at once, the path of the program itself. A path
  opened by the handle of a folder (Go's `os.Root`, `openat`), so that a link inside the folder cannot lead out of
  it as it can after `Path.joinInside` (954). On Windows a junction is no link to `File.linkInfo` and
  `File.readLink` (`Other`, `InvalidInput`), though `Directory.list` calls it `Link` (1976); the times of a link
  itself are not set.
- **Of the lock and the memory of a file (191)**: a lock of a part of a file (`fcntl`'s ranges, `LockFileEx` of
  a range), the order in which waiting programs get a lock (`lockAsync` tries it every 1 to 10 ms, 1983), a file
  put in memory to be written or of a part only (`MAP_SHARED` with `PROT_WRITE`, an offset), and advice to the
  system on how it is read (`madvise`). Permissions are not written in octal by `$"{mode:o}"`.
- **Of seeking in a file**: reading and
  writing one file through one stream (C's `"r+"`), a temporary file with no name that goes when it is closed
  (C's `tmpfile`), and where a stream stands without moving it other than by `seek(SeekFrom.Current(0))`
  (1211-1222).
- **Of the clock**: time zones by name (`Europe/Istanbul`, the IANA database; Windows has no files of it) and
  `TZ` on Windows (1300, 1309), a date without a time and a time without a date, the text of HTTP's dates
  (`Tue, 18 Nov 2026 09:30:05 GMT`) and the names of the months, minutes and hours in the text of a `Duration`
  (`1m30s`), `Task.delay` given a `Duration`, `abs` and `dividedBy` on a `Duration`, the number of a day of the
  week and an order of the days. A pattern of your own (`yyyy-MM-dd`, `%Y`) is not taken (1306).
- **Of random numbers**: shuffling a list and picking one of its items (`xs.shuffle(ref r)`,
  `r.choose(xs)`) wait for generics or a built-in method; Fisher-Yates is four lines (`ok_random_seeded`
  has it). `nextFloat`, numbers of a normal distribution (`nextGaussian`), and `split` to give a task a
  generator of its own that does not overlap.
- **Of values that are never read**: a value written over before it is read (`x = 1` then `x = 2`), or given
  after the last read, passes; Rust warns on both, with a look at every path. A list only added to is read by
  `add`, a method called on it, and passes too.
- **Of JSON**: a reader of a stream (a value read while the text comes, `serde_json::StreamDeserializer`),
  turning a struct into JSON and back on its own (no reflection or macros: the program writes both by hand),
  JSON Pointer (`/a/0/b`) and JSON Lines (1411).
- **Of the log**: files and their rotation, structured fields (JSON lines, `key = value`), a level for each
  module (`RUST_LOG=my_crate=debug`), the file and line of the call, and a way to force colours on (1412,
  1455). A general lazy argument, of which `Log`'s is one case, would be a closure (1447).
- Freeing a long linked list recurses, the open stack-depth question from the design; here it runs on a 1 GB stack.
  Compiled code frees a chain of `T*` in a loop, but a chain of `shared` values recursively, one call for each.

Words from the design that are not implemented produce "is not in the interpreter yet" instead of a confusing
error.
