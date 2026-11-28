# Decisions, slice by slice

SG is built in slices: each adds one piece of the language, its library or its tools, in the checker, the
interpreter and compiled code, with tests. Before a slice, the questions it raises are put to the author with
a recommended answer; what is approved is written here, in the words it was approved in, with what was found
while the slice was built and how it was tested. This folder is that record, one file per day.

- The decisions that shape the language itself (its rules, its types, its library as designed) are in
  [the decisions by topic](../rules/README.md); the files here hold the smaller
  choices made while building it, and the reasons.
- From the sixteenth slice on, every choice has a number that runs across all slices (`choice 886`, `1340`); the
  code, the tests and the other documents cite them by it. The first fifteen slices number their rules from 1
  each. A few numbers are not here: choices about the design itself (703-718, for one) went straight into 04.
- To find a choice by its number, look for it in the column *Choices* below, or search this folder for `886.`;
  a choice approved in one slice is sometimes written in a later one, so the ranges of two slices can overlap.
- What is not built yet is in the interpreter's [Not here yet](../../interpreter/docs/not-here-yet.md).

## By day

| Day | Slices | What |
|---|---|---|
| [12 November 2026](2026-11-12.md) | 1–23 | The first slices of the interpreter (the grammar, structs, enums, `Result`, collections, the borrow check), and the C back end begins. |
| [13 November 2026](2026-11-13.md) | 24–39 | Collections, printing and modules compiled; arrays and slices; the number types; text; files and the command line. |
| [14 November 2026](2026-11-14.md) | 40–57 | Patterns and narrowing, bytes, the overflow checks of a checked build, `shared` and `weak`, tasks and `locked`. |
| [15 November 2026](2026-11-15.md) | 58–76 | Tasks compiled, async, the network, raw memory, calling C, the clock, random numbers, paths. |
| [16 November 2026](2026-11-16.md) | 77–91 | Other programs, the Unicode tables, datagrams, the async pool, and the slices that reach what C reaches: C files, callbacks, SQLite, libcurl, signals, the layout of C, atomics, libraries for C. |
| [17 November 2026](2026-11-17.md) | 92–105 | Building without an operating system and a kernel in QEMU, Windows, consts and statics. |
| [18 November 2026](2026-11-18.md) | 106–117 | Named loops, numbered enums, `with` and `?.`, files, `<math.h>`, channels, dates, and the first review of safety. |
| [19 November 2026](2026-11-19.md) | 118–129 | Fuzzing and mutants, editors, the web site, and the library: hex, Base64, CRC-32, SHA-256, the command line, JSON, a log. |
| [20 November 2026](2026-11-20.md) | 130–144 | Regular expressions, compression, zip, real programs, packages, and the fixes of the second review of safety. |
| [21 November 2026](2026-11-21.md) | 145–152 | A const without calls, `copyFrom`, the normal forms of Unicode, default values of parameters, and `std` measured against Rust and Zig. |
| [22 November 2026](2026-11-22.md) | 153–158 | Time, random numbers and the log measured against Rust and Zig; the network; an exit with an error lets its tasks go; servers measured against Rust, Zig and C; a loop of the network on each thread; a group of tasks, limits and a graceful end for a server. |
| [23 November 2026](2026-11-23.md) | 159, 160, 161, 162, 163, 164, 165 | A list of borrows, a reader that lends slices and a send of several pieces for a server; what the servers stand, and where they break; a slice that may be missing and a linked source; `std/http`, the server's side of HTTP/1.1; the command line, the environment and paths measured; a search in any case and the DFA's reads; a search from the inner text. |
| [24 November 2026](2026-11-24.md) | 166, 167, 168, 169, 170, 171, 172, 173, 174, 175, 176, 177 | A `jq` of JSON in SG, Rust and Zig, held to jq 1.8.2; a reader of JSON's tokens, `Clonable` and `makeUnique`, `try` in an argument of compiled code, `(?x)`, and a stack of 1 GB that says when it overflows; what the interpreter and the JSON of `std` walk, walked with a loop; JSON read and written by a recursion until the stack runs low; the walks of the interpreter measured, and a sort that was quadratic; a large frame touched only near the bottom of the stack; the numbers of `jq` read and written one way in its three versions; its three versions held to jq 1.8.2 by both suites; `Regex` held to Rust's `regex` and PCRE2, and lines and patterns of bytes that may not be UTF-8; a search in any case by its two rarest bytes; a store of keys and values with a log, a file put on the disk and a list as a key; a pool of workers that hashes files; a file server under load, the calls of the file system as tasks and what HTTP needs to send a file. |
| [25 November 2026](2026-11-25.md) | 178, 179, 180, 181 | What each request costs a server: a file opened only when it is a regular one, and the info of an open file; a GIF tool held to gifsicle; the lookup of a `HashMap` as hashbrown's; an interpreter of Lox held to clox, with the bits of a `double`, `%e` and `%g`, and a hash of a key of a fixed size. |
| [26 November 2026](2026-11-26.md) | 186, 187, 188, 189, 190, 191, 192, 193, 194, 195, 196, 197 | A static program with musl linked in, measured against glibc and set aside; the descriptors a started program must not get, also with musl and an older glibc; flags of C on every build; a compiled program in the debugger, on the lines of SG; names of the system that are not text; sockets of the machine, the options of a connection, one side closed, pieces written to a file; links, permissions, the lock of a file and a file in memory; `select`, waiting on several channels at once; the panic of a program whose every task waits, compiled too; two `ref` windows of one list at once; four places the borrow check let through, closed; the streams of a program, programs joined by pipes, a look without waiting, a program let go of, programs by their numbers; groups, sessions, the signals of job control and a job in front at the terminal. |
| [27 November 2026](2026-11-27.md) | 198, 199, 200 | A program at a terminal made for it, the raw mode of a terminal, and its size; issues 10 and 7: a pipe of another program read and written at once in the interpreter, and the writers of the standard streams into a pipe no one reads; watching a folder; a folder as the root of paths from outside, one file read and written through one opening, and a file with no name |
| [28 November 2026](2026-11-28.md) | fixes of issues | The open `bug` issues fixed, with their tests, documents and pages: messages that name the way out, and the rules they found (#15–#51) |

## Every slice

### 12 November

| Slice | What it decided | Choices |
|---|---|---|
| [1](2026-11-12.md#slice-1) | The grammar without semicolons | its own 1–4 |
| [2](2026-11-12.md#slice-2) | Struct literals in conditions, `ref` variables, looking without taking | its own 1–3 |
| [3](2026-11-12.md#slice-3) | Methods, names and `take` | its own 1–6 |
| [4](2026-11-12.md#slice-4) | `is` and `match` | its own 1–6 |
| [5](2026-11-12.md#slice-5) | `Result`, `Ok` and `Error` | its own 1–7 |
| [6](2026-11-12.md#slice-6) | Bytes, characters, text indexes and ranges | its own 1–8 |
| [7](2026-11-12.md#slice-7) | `Copyable` and `Formattable` | its own 1–7 |
| [8](2026-11-12.md#slice-8) | `List` and `HashMap` | its own 1, 3–11 |
| [9](2026-11-12.md#slice-9) | `HashSet`, and a collection is not changed inside its own `for` | its own 1–2 |
| [10](2026-11-12.md#slice-10) | `Equatable`, `Hashable` and `Comparable` | its own 1–9 |
| [11](2026-11-12.md#slice-11) | `test` blocks | its own 1–6 |
| [12](2026-11-12.md#slice-12) | Function values | its own 1–4 |
| [13](2026-11-12.md#slice-13) | The borrow check, first part: borrows that do not go together | its own 1–6 |
| [14](2026-11-12.md#slice-14) | The borrow check, second part: a borrow does not outlive its owner | its own 1–6 |
| [15](2026-11-12.md#slice-15) | The borrow check, third part: types that carry a borrow | its own 1–7 |
| [16](2026-11-12.md#slice-16) | The C back end | 1–6 |
| [17](2026-11-12.md#slice-17) | Compiled: text and owned values | 7–11 |
| [18](2026-11-12.md#slice-18) | Compiled: the heap and missing values | 12–17 |
| [19](2026-11-12.md#slice-19) | Compiled: enums and `match` | 18–26 |
| [20](2026-11-12.md#slice-20) | Compiled: `Result` and `try` | 27–32 |
| [21](2026-11-12.md#slice-21) | Compiled: `List<T>` | 33–38 |
| [22](2026-11-12.md#slice-22) | Compiled: writing and comparing | 39–43 |
| [23](2026-11-12.md#slice-23) | Compiled: types that carry a borrow, function values | 44–47 |

### 13 November

| Slice | What it decided | Choices |
|---|---|---|
| [24](2026-11-13.md#slice-24) | Compiled: `HashMap` and `HashSet` | 48–53 |
| [25](2026-11-13.md#slice-25) | Printing without an allocation, values that have no name | 54–59 |
| [26](2026-11-13.md#slice-26) | Modules: `use`, `public`, `readonly` | 60–72 |
| [27](2026-11-13.md#slice-27) | Modules in compiled code, a `read T?` that a function gives back | 73–77 |
| [28](2026-11-13.md#slice-28) | A `match` inside an expression | 78–82 |
| [29](2026-11-13.md#slice-29) | The test command on compiled code | 83–89 |
| [30](2026-11-13.md#slice-30) | Slices and fixed-size arrays | 90–103 |
| [31](2026-11-13.md#slice-31) | Arrays and slices in compiled code | 104–112 |
| [32](2026-11-13.md#slice-32) | The bit operators, numbers in hex and binary | 113–124 |
| [33](2026-11-13.md#slice-33) | The other integer types | 125–137 |
| [34](2026-11-13.md#slice-34) | `float` and `double` | 138–152 |
| [35](2026-11-13.md#slice-35) | The tools for numbers | 153–166 |
| [36](2026-11-13.md#slice-36) | Text: `char`, going over a text, trimming and searching | 167–181 |
| [37](2026-11-13.md#slice-37) | Building a text piece by piece | 182–191 |
| [38](2026-11-13.md#slice-38) | Files, the input and the command line | 192–204 |
| [39](2026-11-13.md#slice-39) | Files read and written piece by piece | 205–215 |

### 14 November

| Slice | What it decided | Choices |
|---|---|---|
| [40](2026-11-14.md#slice-40) | Appending, and bytes read and written piece by piece | 216–222 |
| [41](2026-11-14.md#slice-41) | Writing a number: a width and a letter, `parseHex` | 223–233 |
| [42](2026-11-14.md#slice-42) | `list.addAll(slice)`, `sort` and `reverse` on arrays | 234–241 |
| [43](2026-11-14.md#slice-43) | Patterns inside patterns, `match` on text, methods on enums | 242–253 |
| [44](2026-11-14.md#slice-44) | `or`, ranges, `null`, `true` and `false` in patterns | 254–262 |
| [45](2026-11-14.md#slice-45) | The names of `is`, narrowing along `&&` and `||` | 263–269 |
| [46](2026-11-14.md#slice-46) | The order of an assignment | 270–274 |
| [47](2026-11-14.md#slice-47) | `replaceAll` and `repeat` | 275–281 |
| [48](2026-11-14.md#slice-48) | The value of a built-in method that is not used | 282–286 |
| [49](2026-11-14.md#slice-49) | Narrowing under `!` and after a `while` | 287–292 |
| [50](2026-11-14.md#slice-50) | The names of `is not`, the names seen after an `if` or a `while` | 293–300 |
| [51](2026-11-14.md#slice-51) | Numbers read out of bytes and written into them | 301–308 |
| [52](2026-11-14.md#slice-52) | The overflow test of an index written `a + c` | 309–314 |
| [53](2026-11-14.md#slice-53) | The overflow tests of a sum kept in a loop | 315–321 |
| [54](2026-11-14.md#slice-54) | `shared`, `weak` and type aliases | 322–333 |
| [55](2026-11-14.md#slice-55) | `shared checked`, the `as` blocks and the leak report | 334–346 |
| [56](2026-11-14.md#slice-56) | `shared`, `weak` and `shared checked` in compiled code | 347–355 |
| [57](2026-11-14.md#slice-57) | Tasks, `locked` and `lock` | 356–370 |

### 15 November

| Slice | What it decided | Choices |
|---|---|---|
| [58](2026-11-15.md#slice-58) | Tasks, `locked` and `lock` in compiled code | 371–383 |
| [59](2026-11-15.md#slice-59) | Async, first part: async functions, `await`, `Task.delay` | 384–402 |
| [60](2026-11-15.md#slice-60) | Async, second part: cancellation, async input and output | 403–416 |
| [61](2026-11-15.md#slice-61) | Async, third part: async functions, delays and cancellation compiled | 417–431 |
| [62](2026-11-15.md#slice-62) | Async, fourth part: async input and output compiled | 432–440 |
| [63](2026-11-15.md#slice-63) | Async, fifth part: `Task.whenAll` and `Task.whenAny` | 441–452 |
| [64](2026-11-15.md#slice-64) | Async, sixth part: streams the async way | 453–465 |
| [65](2026-11-15.md#slice-65) | The network: `TcpListener`, `TcpStream`, a cancel that ends a wait | 466–479 |
| [66](2026-11-15.md#slice-66) | Lines over a connection | 480–490 |
| [67](2026-11-15.md#slice-67) | Checking UTF-8 a block at a time | 491–495 |
| [68](2026-11-15.md#slice-68) | Printing a fractional number with Dragonbox | 496–500 |
| [69](2026-11-15.md#slice-69) | Raw memory: `raw T`, `unsafe`, `Memory` | 501–516 |
| [70](2026-11-15.md#slice-70) | Calling C: `extern` blocks, `CString`, `buffer.pointer()` | 517–530 |
| [71](2026-11-15.md#slice-71) | `Disposable` and `Closable`, opaque C types, C structs by value | 531–544 |
| [72](2026-11-15.md#slice-72) | The clock | 545–555 |
| [73](2026-11-15.md#slice-73) | Random numbers | 556–566 |
| [74](2026-11-15.md#slice-74) | Folders, paths and the environment | 567–580 |
| [75](2026-11-15.md#slice-75) | Three leftovers, and compiled C without warnings | 581–587 |
| [76](2026-11-15.md#slice-76) | A variable nothing reads is a compile error | 588–596 |

### 16 November

| Slice | What it decided | Choices |
|---|---|---|
| [77](2026-11-16.md#slice-77) | Running another program and waiting for it | 597–616 |
| [78](2026-11-16.md#slice-78) | A program talked to while it runs, the async way | 617–634 |
| [79](2026-11-16.md#slice-79) | The Unicode tables: what a character is, case, comparing without case | 635–647 |
| [80](2026-11-16.md#slice-80) | Datagrams: `UdpSocket`, `SocketAddress` | 648–666 |
| [81](2026-11-16.md#slice-81) | The pool of async tasks waits on the loop of the network | 667–677 |
| [82](2026-11-16.md#slice-82) | A socket goes on the loop of the network once | 678–689 |
| [83](2026-11-16.md#slice-83) | A queue for each thread of the pool, and stealing | 690–702 |
| [84](2026-11-16.md#slice-84) | A C file beside the program, the variables of C, `errno` | 719–730 |
| [85](2026-11-16.md#slice-85) | Functions of SG that C calls, pointers to functions of C | 731–749 |
| [86](2026-11-16.md#slice-86) | Functions of C with a varying count of arguments, SQLite | 750–760 |
| [87](2026-11-16.md#slice-87) | libcurl behind a safe module | 761–778 |
| [88](2026-11-16.md#slice-88) | Signals | 779–799 |
| [89](2026-11-16.md#slice-89) | The layout of C | 800–829 |
| [90](2026-11-16.md#slice-90) | Atomics and `volatile` | 830–859 |
| [91](2026-11-16.md#slice-91) | A library built from SG for C to call | 860–885 |

### 17 November

| Slice | What it decided | Choices |
|---|---|---|
| [92](2026-11-17.md#slice-92) | A name that hides another | 886–894 |
| [93](2026-11-17.md#slice-93) | Building without an operating system | 895–913 |
| [94](2026-11-17.md#slice-94) | A kernel in QEMU | 914–927 |
| [95](2026-11-17.md#slice-95) | What Windows needs of the language | 943–962 |
| [96](2026-11-17.md#slice-96) | Where libraries lie, tests without `fork` | 963–979 |
| [97](2026-11-17.md#slice-97) | The base of Windows | 980–998 |
| [98](2026-11-17.md#slice-98) | Threads, tasks, locks, atomics and async on Windows | 999–1014 |
| [99](2026-11-17.md#slice-99) | The network and async input and output on Windows | 1015–1031 |
| [100](2026-11-17.md#slice-100) | Other programs and signals on Windows | 1032–1049 |
| [101](2026-11-17.md#slice-101) | C on Windows | 1050–1067 |
| [102](2026-11-17.md#slice-102) | Everything tested on Windows, measured against C | 1068–1079, 1120–1123 |
| [103](2026-11-17.md#slice-103) | Consts | 1109–1119 |
| [104](2026-11-17.md#slice-104) | Calls in a const | 1124–1134 |
| [105](2026-11-17.md#slice-105) | Statics | 1135–1147 |

### 18 November

| Slice | What it decided | Choices |
|---|---|---|
| [106](2026-11-18.md#slice-106) | Loops with a name, counting down, raw text, `const assert` | 1157–1166 |
| [107](2026-11-18.md#slice-107) | An enum with numbers, the bits of an integer | 1184–1192 |
| [108](2026-11-18.md#slice-108) | The default value of a field, `with`, `?.` | 1202–1210 |
| [109](2026-11-18.md#slice-109) | Seeking in a file, `File.openWrite`, `File.createTemp` | 1215–1223 |
| [110](2026-11-18.md#slice-110) | The rest of `<math.h>`, `abs`, `clamp`, saturating operations | 1241–1248 |
| [111](2026-11-18.md#slice-111) | `binarySearch`, `lowerBound`, `upperBound` | 1255–1258 |
| [112](2026-11-18.md#slice-112) | `lock ... when` | 1266–1275 |
| [113](2026-11-18.md#slice-113) | Channels | 1276–1299 |
| [114](2026-11-18.md#slice-114) | The local time zone, writing and reading dates | 1300–1317 |
| [115](2026-11-18.md#slice-115) | Making it sturdier: a safety review, fuzzing, mutants, tests of `sg` | 1318–1331 |
| [116](2026-11-18.md#slice-116) | Fixing what the first safety review found | 1332–1339, 1341–1346 |
| [117](2026-11-18.md#slice-117) | The rules of borrowing, first part | 1340, 1347–1348 |

### 19 November

| Slice | What it decided | Choices |
|---|---|---|
| [118](2026-11-19.md#slice-118) | Fuzzing the front end, mutants of the rules of safety | 1349–1360, 1377–1382, 1391 |
| [119](2026-11-19.md#slice-119) | Fuzzing for differences | 1361–1368 |
| [120](2026-11-19.md#slice-120) | Support for editors | 1369–1376 |
| [121](2026-11-19.md#slice-121) | The web site | 1383–1390, 1392 |
| [122](2026-11-19.md#slice-122) | What the web site teaches | 1393–1397 |
| [123](2026-11-19.md#slice-123) | What writing the site found in `sg` | 1398–1405 |
| [124](2026-11-19.md#slice-124) | The library: hex and Base64 | 1406–1407 |
| [125](2026-11-19.md#slice-125) | The library: CRC-32 | 1408, 1414–1415 |
| [126](2026-11-19.md#slice-126) | The library: SHA-256 and HMAC | 1409, 1418–1422 |
| [127](2026-11-19.md#slice-127) | The library: the command line | 1410, 1423–1433 |
| [128](2026-11-19.md#slice-128) | The library: JSON | 1411, 1434–1446 |
| [129](2026-11-19.md#slice-129) | The library: a log | 1412, 1416–1417, 1447–1456 |

### 20 November

| Slice | What it decided | Choices |
|---|---|---|
| [130](2026-11-20.md#slice-130) | The library: regular expressions | 1457–1481 |
| [131](2026-11-20.md#slice-131) | The library: compression | 1482–1500 |
| [132](2026-11-20.md#slice-132) | The library: zip archives | 1501–1524 |
| [133](2026-11-20.md#slice-133) | Real programs: `grep` and `wc` | 1525–1542 |
| [134](2026-11-20.md#slice-134) | What the trials found, first: printing | 1543–1546 |
| [135](2026-11-20.md#slice-135) | A `print` that cannot write | 1547–1548 |
| [136](2026-11-20.md#slice-136) | What the trials found, second: reading lines, searching for a plain text | 1549–1554 |
| [137](2026-11-20.md#slice-137) | Packages, first: the project file, packages from folders, `internal` | 1555–1570 |
| [138](2026-11-20.md#slice-138) | Packages, second: fetching, the lock, the cache and the commands | 1571–1588 |
| [139](2026-11-20.md#slice-139) | The second safety review, first: packages | 1590–1591 |
| [140](2026-11-20.md#slice-140) | The second safety review, second: the rules of borrowing | 1592 |
| [141](2026-11-20.md#slice-141) | The second safety review, third: a generator of programs that borrow | 1593 |
| [142](2026-11-20.md#slice-142) | The second safety review, fourth: modules of `std` that read what others write | 1594 |
| [143](2026-11-20.md#slice-143) | The second safety review, fifth: compiled code and its runtime | 1595, 1601–1603 |
| [144](2026-11-20.md#slice-144) | The second safety review, sixth: the tool itself | 1596, 1604 |

### 21 November

| Slice | What it decided | Choices |
|---|---|---|
| [145](2026-11-21.md#slice-145) | A const calls no function | 1606–1608 |
| [146](2026-11-21.md#slice-146) | A `ref` range counted in a call, `copyFrom`, the Pike machine | 1597–1598, 1609–1610 |
| [147](2026-11-21.md#slice-147) | The normal forms of Unicode | 1599, 1611–1615 |
| [148](2026-11-21.md#slice-148) | The default values of parameters, the depth of written JSON | 1600, 1617–1628 |
| [149](2026-11-21.md#slice-149) | `std` measured against Rust and Zig, first: JSON | 1629–1640 |
| [150](2026-11-21.md#slice-150) | Compression and zip measured against Rust and Zig | 1642–1652 |
| [151](2026-11-21.md#slice-151) | SHA-256, HMAC, CRC-32 and Adler-32 measured against Rust and Zig | 1653–1658 |
| [152](2026-11-21.md#slice-152) | Hex, Base64 and Unicode measured against Rust and Zig | 1659–1666 |
| [153](2026-11-22.md#slice-153) | Time, random numbers and the log measured against Rust and Zig | 1667–1677 |
| [154](2026-11-22.md#slice-154) | The network measured against Rust and Zig | 1678–1690 |
| [155](2026-11-22.md#slice-155) | An exit with an error lets its tasks go | 1691–1696 |
| [—](2026-11-22.md#golden-tests-several-at-a-time) | The golden tests built and run several at a time | 1697 |
| [156](2026-11-22.md#slice-156) | The server lane: servers measured against Rust, Zig and C | 1698–1707 |
| [157](2026-11-22.md#slice-157) | The scheduler of the server lane: a loop of the network on each thread | 1708–1714 |
| [158](2026-11-22.md#slice-158) | What a server needs of the library, first: a group of tasks, limits and a graceful end | 1715–1730 |
| [159](2026-11-23.md#slice-159) | What a server needs of the library, second: a list of borrows, a reader that lends slices, `writev` | 1731–1749 |
| [160](2026-11-23.md#slice-160) | What the servers stand, and where they break | 1750–1764 |
| [161](2026-11-23.md#slice-161) | A slice that may be missing, and a linked source | 1774–1775 |
| [162](2026-11-23.md#slice-162) | `std/http`: the server's side of HTTP/1.1 | 1765–1773, 1776–1794 |
| [163](2026-11-23.md#slice-163) | The command line, the environment and paths, measured; `Path.push` | 1795–1802 |
| [164](2026-11-23.md#slice-164) | A regular expression that reads every byte: a search in any case, and the DFA's reads | 1803 |
| [165](2026-11-23.md#slice-165) | A search from the inner text, as Rust's | none |
| [166](2026-11-24.md#slice-166) | A `jq` of JSON; a reader of JSON's tokens; `Clonable` and `makeUnique`; `try` in an argument; `(?x)`; a stack of 1 GB that says when it overflows | 1804–1826 |
| [167](2026-11-24.md#slice-167) | What the interpreter and the JSON of `std` walk, walked with a loop | 1827–1831 |
| [168](2026-11-24.md#slice-168) | The JSON of `std` read and written by a recursion, with a list where the stack runs low | 1832–1834 |
| [169](2026-11-24.md#slice-169) | The walks of the interpreter measured: they stay on their list, made at once where a value has no deeper parts | 1835 |
| [170](2026-11-24.md#slice-170) | A large frame touched only where it may reach the bottom of the stack | 1836 |
| [171](2026-11-24.md#slice-171) | A number's `double` worked out when wanted, a whole number written as an integer, in the three versions of `jq` | 1824, 1825, 1837 |
| [172](2026-11-24.md#slice-172) | The three versions of `jq` pass each other's suites, kept in `trials/jq/tests` | 1839 |
| [173](2026-11-24.md#slice-173) | `Regex` held to Rust's `regex` and PCRE2; lines and patterns of bytes that may not be UTF-8 | 1840–1843 |
| [174](2026-11-24.md#slice-174) | A search in any case by its two rarest bytes; a text that starts every match | none |
| [175](2026-11-24.md#slice-175) | A store of keys and values with a log; `syncData`, `sync`, `setLength` and `Directory.sync`; a `List` as the key of a map | 1844–1853 |
| [176](2026-11-24.md#slice-176) | A pool of workers that hashes files, as GNU's `sha256sum` writes them; `Task.processorCount()` | 1854–1860 |
| [177](2026-11-24.md#slice-177) | A file server under load; the calls of the file system as tasks; `Percent`, the dates of HTTP, an answer of a known length; the disk asked where the kernel will not wait | 1861–1872 |
| [178](2026-11-25.md#slice-178) | What each request costs a server; `File.openRegular` and `reader.info()` | 1873 |
| [179](2026-11-25.md#slice-179) | A GIF tool in SG, Rust and Zig, held to gifsicle | 1874–1881 |
| [180](2026-11-25.md#slice-180) | The lookup of a `HashMap` and a `HashSet` as hashbrown's; a key of four bytes hashed in one block | 1882 |
| [181](2026-11-25.md#slice-181) | An interpreter of Lox held to clox; `toBits` and `fromBits`, `%e` and `%g` in `$"..."`, a hash of a key of a fixed size as foldhash | 1883–1894 |
| [Showcase](2026-11-25.md#the-showcase) | gifsicle written in SG, in a repository of its own | 1895–1900 |
| [182](2026-11-25.md#slice-182) | Whether a stream is a terminal: `isTerminal()` | 1901 |
| [183](2026-11-25.md#slice-183) | The conditional operator `?:` | 1910–1916 |
| [184](2026-11-25.md#slice-184) | A type alias written `alias`, not `using` | 1918–1920 |
| [185](2026-11-25.md#slice-185) | An index counted from the end, `xs[^1]` | 1921–1926 |
| [186](2026-11-26.md#slice-186) | A static program with musl, measured and set aside; a started program gets no other descriptor where the C library cannot close them | 1927–1934, 1940, 1942 (1932 built) |
| [187](2026-11-26.md#slice-187) | Flags of C on every build, with a warning where they change a promise | 1935–1941 |
| [188](2026-11-26.md#slice-188) | A compiled program in the debugger, on the lines of SG | 1943–1949 |
| [189](2026-11-26.md#slice-189) | Names of the system that are not text, carried without a loss | 1950–1959 |
| [190](2026-11-26.md#slice-190) | Sockets of the machine, the options of a connection, one side closed, pieces written to a file | 1960–1972 |
| [191](2026-11-26.md#slice-191) | Links, permissions, the lock of a file and a file in memory | 1973–1986 |
| [192](2026-11-26.md#slice-192) | `select`: waiting on several channels at once | 1987–2001 |
| [193](2026-11-26.md#slice-193) | When every task waits, compiled too | 2002–2010 |
| [194](2026-11-26.md#slice-194) | Two `ref` windows of one list at once, held apart | 2011–2015 |
| [195](2026-11-26.md#slice-195) | Four places the borrow check let through, closed | none: the rules as they were |
| [196](2026-11-26.md#slice-196) | The streams of a program, `Pipeline`, `tryWait`, `detach`, programs by their numbers; the plan of 197 and 198 | 2016–2029 |
| [197](2026-11-26.md#slice-197) | Groups, sessions, the signals of job control, and a job in front at the terminal | 2023–2026, 2030–2041 |
| [198](2026-11-27.md#slice-198) | A program at a terminal of its own, a terminal made raw, and its size | 2027–2028, 2042–2054 |
| [Issue 7](2026-11-27.md#issue-7) | The writers of the standard streams into a pipe no one reads | 2055 |
| [199](2026-11-27.md#slice-199) | Watching a folder | 2056–2065 |
| [200](2026-11-27.md#slice-200) | A folder as a root, a file read and written through one opening, a file with no name | 2066–2077 |
| [The rule of speed](2026-11-27.md#the-rule-of-speed) | SG is held to safe Rust doing the same work; C is the yardstick | 2078 |
| [The sentence and the tagline](2026-11-27.md#the-sentence-and-the-tagline) | Small like C, readable like C#, fast and memory safe like Rust; "Plain to read. Fast to run. Memory safe by design." (replaced by 2080) | 2079 |
| [The tagline, again](2026-11-27.md#the-tagline-again) | "Small enough to know by heart. Safe and fast by construction."; no other language named | 2080 |
| [The plain site](2026-11-27.md#the-plain-site) | Plain pages made by one script, a page for each word, manual pages, the wildcat logo; Docusaurus kept | 2081 |
| [Help in the terminal](2026-11-27.md#help-in-the-terminal) | `sg help <name>` prints the page of a word from `sg` itself, with no network; the packages of the preview install the manual pages | 2082-2083 |
| [The README and the shared card](2026-11-27.md#the-readme-and-the-shared-card) | The README: a paragraph, one program, the steps to build; the wildcat in the README and in the card a shared link shows | 2084 |
| [`return` as an arm of a `match`](2026-11-27.md#return-as-an-arm-of-a-match) | `return` inside an expression: on the right of `??`, and as an arm of a `match` that gives a value too | 2085 |
| [A sign by its marks in `sg help`](2026-11-27.md#a-sign-by-its-marks-in-sg-help) | `sg help ??` finds `x ?? y`: a sign by its name without letters, digits and spaces | 2086 |
| [The pages of the commands](2026-11-27.md#the-pages-of-the-commands) | A page for each command, `sg-build(1)` in section 1 of the manual; `sg build --help` prints it | 2087-2088 |
| [A build never writes over a source](2026-11-27.md#a-build-never-writes-over-a-source) | `sg build x.sg -o x.sg` is refused: an output never replaces a file of SG or C, or a header of one's own | 2089 |

### 28 November

| Slice | What it decided | Choices |
|---|---|---|
| [`bool`, conditions, `$"..."`, `a..b` and `xs[^1]` (issues 30 and 47)](2026-11-28.md#bool-conditions--ab-and-xs1-issues-30-and-47) | A `$"..."` kept in a variable borrows what it names; messages around `bool`, ranges and `^` | 2110–2112 |
| [Shared values, `lock` and `readonly`](2026-11-28.md#shared-values-lock-and-readonly) | A `shared atomic` in `when` refused, items of a list of checked values lent, `readonly` through a shared field | 2150–2151 |
| [Numbers, characters and bytes (issues 29, 31, 42, 46)](2026-11-28.md#numbers-characters-and-bytes-issues-29-31-42-46) | Messages about numbers name the way out; a known overflow is a compile error; float NaN bits | 2090–2096 |
| [Pointers, raw memory and the edge with C](2026-11-28.md#pointers-raw-memory-and-the-edge-with-c) | `==` through `T*`, raw memory reached through a number checked, `embed` in about its size | 2170–2177 |
| [Patterns, aliases, `with`, interfaces and the loader of modules](2026-11-28.md#patterns-aliases-with-interfaces-and-the-loader-of-modules) | `when` and `not` as names, an alias of a built-in type, a derived `equals`, the loader of modules | 2210–2215 |
| [Statements, names and the messages around them](2026-11-28.md#statements-names-and-the-messages-around-them) | A line that never runs, the types of `std` named again, `try` in a new list, `assert` lined up | 2190–2194 |
| [`T?`, `null` and `?:`](2026-11-28.md#t-null-and-) | `==` on `T?`, narrowing kept by an assignment and below `Go(null)`, `?:` | 2130–2133 |
| [The commands](2026-11-28.md#the-commands) | A failed build says whose mistake it is, a shared library is linked by its own name, `sg build .` names the program after the folder, the program of tests refuses what it cannot read | 2230–2242 |
