# Interpreter

A tree-walking interpreter for SG, written in Rust with no dependencies. Its job is to test the design by
running real programs, not to be fast. Source files end in `.sg`; the crate and the command are named `sg`.

```
cargo run -- examples/stack.sg                  # runs the program
cargo run -- examples/wc.sg examples/hello.sg   # the words after the file are the program's command line
cargo run -- test examples/tests_stock.sg       # runs its tests
cargo run -- examples/shop/main.sg              # a program with modules: the folders next to the file
cargo run -- test examples/shop                 # the tests of every module of the program in that folder
cargo run -- test examples/shop --compiled      # the same tests, compiled through C
cargo run -- examples/zlib/main.sg              # a library of C (zlib) behind a safe module
cargo run -- build examples/primes.sg -o primes # compiles it through C (needs `cc`)
cargo run -- emit-c examples/primes.sg          # prints the C
cargo test
cargo run -- build examples/primes.sg --release # the same as a release build: optimised, overflow wraps
cargo run -- build examples/bare --freestanding # an object for a machine without an operating system
examples/kernel/run.sh                          # a RISC-V kernel booted in QEMU (Clang, LLD and QEMU)
python3 bench/run.py                            # compiled SG against C written by hand
```

On Windows the same (but `examples/kernel/run.sh`, a script of `sh`), with LLVM's Clang (`winget install LLVM.LLVM`) and the Build Tools of Visual Studio;
the examples of zlib, SQLite and curl and their benchmarks take their libraries from vcpkg (`-L` for its
`lib` and `bin`, `CPATH` for its `include`), and the kernel's test QEMU
(`winget install SoftwareFreedomConservancy.QEMU`, which goes to `C:\Program Files\qemu`).

`cargo test` boots that kernel in QEMU too (`tests/kernel.rs`), so it needs a Clang that knows RISC-V
(`CC`, or `clang`), LLD and `qemu-system-riscv64`, and fails, saying what to install, without them. On
macOS, whose own Clang knows only its own processor: `brew install llvm lld qemu`, then
`CC=$(brew --prefix llvm)/bin/clang PATH=$(brew --prefix lld)/bin:$PATH cargo test`. On Linux the image
of `tests/Containerfile` has them.

## What runs today (19 November 2026)

| Area | Supported |
|---|---|
| Functions | Top-level functions, type-first signature, recursion, any order. `void main()` is the entry point; `void main(List<string> args)` gets the words of the command line |
| Modules | A folder is a module: the files in it see each other with nothing written. `use stock` at the top of a file brings in the `public` names of the folder `stock`; `use shop.stock` for a folder inside a folder. The full name works without `use`: `billing.Line { ... }`, `stock.total(read xs)`, `stock.Color.Red`, `List<stock.Product>`. `public` on a struct, an enum, a function, a field or a method; without it the name stays inside its module. `readonly` on a field. The program is `sg main.sg` (the file is the root module, the folders next to it are the modules) or `sg folder` (every file of the folder is the root module). `sg build main.sg` and `sg build folder` compile the same program |
| Types | `sbyte`, `byte`, `short`, `ushort`, `int` (32 bit), `uint`, `long`, `ulong`, `nint`, `nuint`, `CLong` and `CULong` (C's `long` and `unsigned long`), `float`, `double`, `bool`, `char`, `string`, `void` as return type, structs, enums, `T*`, `T?`, `shared T`, `weak T`, `shared checked T`, `weak checked T`, `shared locked T`, `weak locked T`, `Task<R>`, `CancellationSource`, `CancellationToken`, `TcpListener`, `TcpStream`, `UdpSocket`, `SocketAddress`, `CString`, `Duration`, `Instant`, `DateTime`, `Random`, `Result<T, E>`, `List<T>`, `HashMap<K, V>`, `HashSet<T>`, arrays `T[N]`, slices `read T[]` and `ref T[]`, raw pointers `raw T`, pointers to functions of C `CFunc<A, R>`, types of C seen only behind a raw pointer (`struct sqlite3` in an `extern` block), `read T` and `ref T` in signatures |
| Type aliases | `using NodeRef = shared Node`, `public using P = Point`: a second name for the same type, which also builds it (`new NodeRef(1, null)`, `P(1, 2)`, `P { x = 1, y = 2 }`) |
| Types that carry a borrow | `struct Word<from source> { read string text` / `int start }`, `enum Token<from source> { Name(read string text) ... }`: a field can be a borrow when the type names its source. In a method `from source` says "from what this value borrowed, not from the value itself"; outside the type it is `from words.source` |
| Structs | `struct Node { int value` / `Node*? next }`, `Node { value = 1, next = null }`, field access and assignment through pointers and borrows. A field may have a default value, which a literal that leaves it out gives it (`struct Config { int port = 80 }`, `Config {}`): what a const can be, found when the program is compiled, `null` or an empty collection. `p with { x = 3 }` is a new value with the fields written changed: `p` is moved, or copied when it is `Copyable`, and what those fields held is dropped there |
| Methods | Written in the body of a struct or an enum with `this`, `read this` or `ref this` as the first parameter; fields are reached as `this.x`, and an enum looks at itself with `match this`. Called as `stack.push(1)`, `shape.area()`, also through pointers and borrows. With a type in front of `this` (`read FolderRef this`, `FolderRef this`) a method is called on the `shared` value that holds the struct. A function without `this` is called on the type: `Stack.create()`, `Shape.unit()`. `Stack(args)` is short for `Stack.create(args)` |
| Enums | `enum Shape { Circle(int radius)` / `Rect(int width, int height)` / `Empty }`, one variant per line, and methods after or between them. Built with the full name: `Shape.Circle(2)`, `Shape.Empty`, `new Expr.Number(1)`. An enum with numbers names their integer type first: `enum Block : byte, Copyable { Image = 0x2C` / `Trailer = 0x3B }`, its variants carry nothing, and one without a number has one more than the variant above; `(byte)block` gives the number, `Block.fromValue(b)` the variant or `null`, and `: Comparable` orders by the numbers without a `compare` written |
| `match` | `match x { Circle(r) => ... }` as an expression or a statement, on an enum, also behind pointers and borrows. Patterns: a variant with a pattern for each value it carries (a name, `_`, or a pattern inside: `Add(Number(0), right)`, `Word("let")`, `Paint(Red)`), `_`, `var other`; a condition with `when`. On an integer the patterns are numbers and characters, on a `char` characters (`match c { '+' => ... _ => ... }`), on a text texts (`match word { "if" => ... _ => ... }`), on a `bool` `true` and `false`. Patterns are joined with `or` (`Add(l, _) or Mul(l, _)`, `'+' or '-'`), and each names the same values; numbers and characters come in ranges that take both ends (`'a'..='z'`, `0..=9`). On a `T?` the patterns are `null` and those of the `T`, also inside a variant (`Node(null, _)`); a name below an arm `null` takes a value that is not `null`. A value no arm takes, or an arm that can never run, is a compile error that names the value: `does not handle Add(Name(_), _)`, `no arm takes 128..=255`. An arm is an expression, a statement (`r += 1`, `return x`, `break`) or a block whose last line is the value |
| Interfaces | Seven built-in ones, written after the name: `struct Point : Copyable`, `enum AppError : Formattable`. A `Copyable` value is copied where another would be moved. A `Formattable` value is accepted by `print`, `$"..."` and `f.write`; a struct or an enum can write its own `void format(read this, ref Formatter f)`, otherwise the text is made from the fields. `Equatable` gives `==` and `!=` (field by field, or the type's own `bool equals(read this, read T other)`), `Hashable` lets the type be a key of a `HashMap` or an item of a `HashSet`, `Comparable` gives `<` `<=` `>` `>=` and `sort` through the type's `int compare(read this, read T other)`. A `Disposable` struct has a `void dispose(ref this)` that runs by itself, once, where the last owner of the value ends, before its fields end (not on a panic or `exit`; `x.dispose()` is an error, and no field is moved out of it). A `Closable` struct has a `close(this)` that gives a `Result`, and a local of it is closed before it ends, as a `StreamWriter` is |
| `List<T>` | Built in. `List<int>()`, `List<int> { 1, 2 }`; `xs.add(v)`, `xs.length()`, `xs[i]` to read and to write (`xs[i] = v`, `xs[i] += 1`, `xs[i].name = ...`, `ref xs[i]`), `xs.get(i)` gives `null` where `xs[i]` would panic, `xs.removeLast()`, `xs.reverse()`, `xs.clear()`, `xs.addAll(items)` copies the items of a slice to the end (`buffer.addAll(text.asBytes())`, `xs.addAll(read ys)`, `xs.addAll(ys[2..])`) when they are `Copyable`. `xs.indexOf(v)` and `xs.contains(v)` on `Equatable` items, `xs.sort()` on `Comparable` ones (numbers and text are both); on items in order, `xs.binarySearch(v)` gives the place of the first equal one or `null` by halving, and `xs.lowerBound(v)` and `xs.upperBound(v)` the first place whose item is not less than `v`, or greater than it (`upperBound(b) - lowerBound(a)` counts the items from `a` up to `b`). `string.join(", ", read names)` |
| `HashMap<K, V>` | Built in; a key is an integer, a `bool`, a `string` or a type written `: Equatable, Hashable`. `HashMap<string, int>()`, `HashMap<string, int> { {"x", 10} }`; `map.add(key, value)`, `map[key]` to read and to write what is under a key that is there, `map.get(key)` gives `null` for a missing key, `map.getOrAdd(key, initial)` gives a `ref V`, `map.contains(key)`, `map.remove(key)`, `map.length()`, `map.clear()` |
| Text of a collection | `print(xs)`, `$"{xs}"`: `[1, 2, 3]`, `{"ada": 36}`, `{1, 2, 3}`, when the items can be written as text |
| `HashSet<T>` | Built in; an item is what a key of a `HashMap` can be. `HashSet<int>()`, `HashSet<int> { 1, 2 }`; `set.add(v)` gives `true` if the item was not there, `set.contains(v)`, `set.remove(v)` gives `true` if it was there, `set.length()`, `set.clear()` |
| Arrays | `int[8] values`: a fixed number of items, kept where the value is (in the variable, inside the struct), with the length in the type. Built as `int[3] { 1, 2, 3 }` (the items), `byte[16](255)` (one value in every item) or `byte[16]` (zeros: 0, `false`, `null`). `xs[i]` to read and to write, `xs.length()`, `xs.get(i)`, `xs.indexOf(v)`, `xs.contains(v)`, `xs.binarySearch(v)`, `xs.lowerBound(v)`, `xs.upperBound(v)`, `xs.sort()`, `xs.reverse()`, `==`, `print`. An array of items that can be copied is copied like a number; any other is moved |
| Consts | `const int BlockSize = 4096`, at the top of a file (`public const` for other modules) or in a block, where it is seen to the end of the block. A number, `bool`, `char`, a text, or an array, a struct or an enum of numbers, `bool` and `char` (`: Copyable`). `const int[] Primes = { 2, 3, 5, 7 }` counts the items, `const byte[] Logo = embed("logo.gif")` holds the bytes of a file beside the source. The value is found when the program is compiled: numbers, texts, other consts, operators, casts, `MinValue` and the others, arrays, structs and variants built of them, `.length()` and an item or a field of a const, and calls: `const int[256] Crc = crcTable()` runs any function when the program is compiled, whose body, and those of the functions it calls, may not read or write, look at the clock or the system, use raw memory, C, tasks or a `Func`, or call `sin` and its kin (an error shows the calls that lead there), within 100 000 000 steps and 64 MB; an overflow or a panic there is a compile error. An integer const gives the count of an array (`byte[BlockSize]`, `int[N](0)`), and a const of a number, a `char`, a text or a `bool` is a pattern (`match tag { Trailer => ... }`). An array, a struct or an enum is lent where it is used, for the whole run, and never changed (`var copy = Masks` copies it); in compiled code it is one `static const`, which nothing copies. `const assert(cond)` and `const assert(cond, "message")` check a condition made as a const is, at the top of a file or in a block; a failed one says both sides of a comparison. `Memory.sizeOf<T>()` and `Memory.alignOf<T>()` are written only there: compiled code hands such an assert to C's `_Static_assert` too, for the machine it is compiled for |
| Statics | `static uint[256] Crc = crcTable()`, `static atomic long Requests = atomic 0`, `static locked Config Settings = Config.defaults()`: one value for the whole program, never assigned, at the top of a file (`public static` for other modules) or in a function, where its name is seen to the end of the block. A value made as a const's is (numbers, consts, operators, no call; inside `atomic` and `locked` too) lies in the program from its start; any other is found the first time the static is used, once, whichever task comes first, and the others wait. What finds it cannot `lock`, `wait` or `await` through the calls, and one that needs itself is a compile error, or a panic through a `Func`. A number, a `bool`, a `char` or a function is read as a value, anything else lent as `read`; `static atomic` changes by its methods, `static locked` only inside `lock`. What a static holds follows the thread rule and is never closed or disposed of, so no file, stream or socket; each test starts with the statics not used yet, and what they hold is not a leak. In compiled code a static is a global of C, a `static const` when it is known from the start and holds no atomic value; a read of one found later is a load and a branch |
| Slices | `read T[]` looks at items that something else owns, `ref T[]` can also write them. `read xs` and `ref xs` give one where a slice is expected, for a `List<T>` and a `T[N]`; a range gives one too: `xs[a..b]`, `xs[..b]`, `xs[a..]`, `xs[..]`, and `ref xs[a..b]` for a range that is written through. `s[i]`, `s.length()`, `s.get(i)`, `s.indexOf(v)`, `s.contains(v)`, `s.binarySearch(v)`, `s.lowerBound(v)`, `s.upperBound(v)` (on a range the place is counted from its start), `==`, `print`, `for`; `s.sort()` and `s.reverse()` through a `ref T[]`, and on a range in front of them: `xs[2..5].sort()` puts those items of `xs` in order where they are. `text.asBytes()` looks at a text as a `read byte[]`; `string.fromUtf8(bytes)` looks at bytes as text and gives a `Result<read string, TextError>` |
| `for` over a collection | `for x in xs` hands the collection over and `x` owns each item; `for x in read xs` and `for x in ref xs` lend, and so does a collection that is already borrowed. Over a `HashMap` the variable has a `key` and a `value`; a `HashSet` is gone over by hand-over or `read`. An array is gone over like a list; a slice lends its items |
| `is` | `if s is Circle(r) { ... }`, `if e is Add(Number(0), right) { ... }`, `if e is Add(l, _) or Mul(l, _) { ... }`, `while next() is not End { ... }`. The names reach along `&&`: `if int.parse(read text) is Ok(n) && n > 0 { ... }`, `return s is Circle(r) && r > 10`. The names of `is not` are seen where it fails: `if int.parse(read text) is not Ok(n) { return }` and `n` after it, `if r is not Ok(n) \|\| n < 0 { return }`, `else`, and after `while r is not Ok(n) { ... }` |
| Errors | `Result<T, E>` is built in: `return Ok(value)`, `return Ok()` for `Result<void, E>`, `return Error(cause)`. `try x` gives the value or returns the error to the caller, and on a `T?` returns `null`; on a `read T?` it gives a `read T`. `x ?? fallback` drops the error; the right side can be `return ...` or `panic(...)`. `match` and `is` see the variants `Ok` and `Error`. `_ = f()` drops a value on purpose. `Result<void, E> main()` prints the error it ends with and exits with code 1 |
| Ownership | `new x` gives `T*`; `read x` and `ref x` lend; passing or assigning a non-copyable value moves it. `slot.take()` on a `T?` gives the value and leaves `null`; `slot.replace(value)` gives the old value and leaves the new one |
| Several owners | `new shared x` gives a `shared T`, one of the owners of a value on the heap; `share x` makes one more without moving `x` (so does `x.clone()`), and a move hands one over. What a `shared T` points to is read (`node.value`, a method that takes `read this`, `match`, `for item in read node.items`, `read node` where a `read T` goes) and never written or taken out; what a `shared checked T` points to is changed too (the next row). `weak x` gives a `weak T`, which looks without owning: `share w` gives a `shared T?`, `null` once the last owner is gone. `same(a, b)` tells whether two point to one object, `==` compares what they point to |
| Changed through several owners | `new shared checked x` gives a `shared checked T`: what it points to is changed through any of its owners (`a.count += 1`, `a.items.add(x)`, a method that takes `ref this`, `ref a` where a `ref T` goes), and each access is checked against the borrows of it in progress, one that changes it or any number that read it. A read or a write borrows nothing after it; a value lent to a call is borrowed until the call returns, the head of a `for` for the loop, a `match` and an `is` with names for the statement. `read x as n { ... }` and `ref x as n { ... }` lend it for a block. A conflict through the same variable is a compile error, one through another variable a panic that shows both lines. A method can take the owner itself, `read FolderRef this`, so that it can write `share this` and `weak this`. The `shared` values still alive when a program or a test ends hold each other up and are reported (`leak: ...`; in compiled code by a debug build) |
| Tasks | `Task.run(work, x, share total)` starts `work(x, share total)` as a task and gives a `Task<R>`, `R` being what `work` returns; a `Func` can be started too. `t.wait()` waits for it and gives that value (`try t.wait()` passes its error on). A task that is not waited for on every path is a compile error, and so is `Task.run(...)` alone or dropped with `_ =`. What a task is given and gives back owns all it holds: no borrow, no `shared checked` value, no `Task` (the thread rule, also inside fields). In the interpreter a task runs on a thread of its own when it is waited for, so the output is the same on every run; compiled code starts a thread of the system for it at once. A panic inside one names the line that started it |
| Changed by several tasks | `new shared locked x` gives a `shared locked T`, which several tasks own; what it holds follows the thread rule and is reached only inside `lock total as c { c += 1 }`, where `c` is a `ref T` for the block. `lock a as x, b as y { }` takes two together; the same value twice is a panic. A `lock` block does not wait: no other `lock`, no `wait`, and no call of a function that does one, followed through the calls (a call of a `Func` there is refused when it could be one), so locks and waits cannot deadlock. `lock jobs as q when q.items.length() > 0 \|\| q.done { ... }` waits first, with the locks let go, until the condition holds: it is computed under the locks with each name lent to read, and every `lock` block on the value that ends wakes it to look again, so nothing is told by hand; an async function writes `await lock ... when`. The condition does not read an `atomic`, the clock, random bytes or input and output, which change while no `lock` ends. A task left the only owner of the values, with no `weak` look at them, waits for what nobody could change: a panic, whose hint is a `done` field the giver sets |
| Raw memory | `raw T` is an address that owns nothing and may be `null`; it points at plain data (numbers, `bool`, `char`, raw pointers, and arrays and structs of them), laid out as C lays it, or is a `raw void`. `Memory.allocate<T>(n)` gives room for `n` items, all zero (C's `calloc`, in the interpreter too). `p[i]` reads and writes an item (`p[i] = v`, `p[i].x += 1`), `p + n` and `p - n` move by items, `(raw byte)p` reads the memory as another type, `Memory.copy(source, target, count)` copies items (also over itself), `Memory.view(p, count)` looks at `count` items as a `read T[]` (C's memory added to a list in one copy, read as text where it lies), `Memory.viewRef(p, count)` writes them as a `ref T[]` (a buffer C wants filled), `Memory.readUnaligned(p)` and `Memory.writeUnaligned(p, v)` reach an item at any address, and `Memory.free(p)` gives the block back, or memory C gave with `malloc`; each of them is written inside `unsafe { ... }`, or with `unsafe` in front of a value (`var first = unsafe p[0]`), and an `unsafe` that nothing needs is an error. `p == q` and `p == null` need no `unsafe`, nor do `p < q` and `p - q` (the order of two addresses, and the count of items between them) and `(nuint)p` (an address as a number); `unsafe (raw T)n` makes a pointer out of a `nint` or a `nuint` (`(raw void)(nint)-1`). The interpreter checks every item against the block it came from and panics outside it, after `Memory.free` and through `null`; compiled code checks nothing of that, and a `raw T` is a `T*`. An item reached at an address that is not a multiple of the alignment of its type is a panic in the interpreter and in a debug build, and costs nothing in a release build. `Memory.sizeOf<T>()` and `Memory.alignOf<T>()` give what C has for a type of plain data. `Memory.zeroSecurely(ref key)` writes zeros over a list, an array or a window of plain data that the C compiler keeps although nothing reads them again, without `unsafe`. A window of `Memory.view` borrows the variable or the field that holds its pointer, so it does not outlive it; a count below 0, and `null` with items, are panics in both. A raw pointer does not go to another task |
| Calling C | `extern "z" { int compress(raw byte dest, ref nuint destLen, raw byte source, nuint sourceLen) }` declares functions of C, a block for each library (linked as `-lz`; `extern { }` is the C library itself), each called inside `unsafe`. `extern "z", "zlib" { }` gives the names a library has on the systems, and the first one the C compiler finds is linked; `-L folder` (on `sg build`, `sg test`, and before the file on `sg -L folder main.sg`) names a folder looked in first, written into the program so that it runs without `LD_LIBRARY_PATH`; on Windows, which has no such thing, `sg build` copies the DLLs it linked from there beside the program. A C function takes integers, `float`, `double`, `bool`, `raw T`, `ref T` and `read T` of plain data (C's `T*` and `const T*`, for the time of the call; `ref raw T` is `T**`), a struct of plain data by value, laid out as C lays it out, and `read CString`, and gives a number, a `bool`, a `raw T`, a struct of plain data or nothing. `struct sqlite3` without a body in an `extern` block is a type of C that SG only points at (`raw sqlite3`). `CString.from(text)` gives a `Result<CString, TextError>`, an error for a zero byte inside; `unsafe string.fromCString(p)` copies what C ended with a zero byte and checks that it is UTF-8. `buffer.pointer()` on a list, an array or a slice of plain data is written only as an argument of a C function. The functions are private to their module unless `public` is written on one, so a module can hide a library behind a safe interface, and a `Disposable` type gives back what C made when it ends (`examples/zlib`, whose `Deflater` holds zlib's stream). `extern "./native.c" { ... }` names a C file of the module's folder in place of a library: it is compiled with the program, in the same unit, so its macros, `static inline` functions, builtins and assembly are reached through functions it declares, and an inline one is inlined; a library block of the module (`extern "z" { }`, also empty) links what the file needs. `int opterr` in an `extern` block, without parentheses, is a variable of C, read and written inside `unsafe` (`unsafe opterr`, `unsafe { opterr = 0 }`, `unsafe { origin.y = 4 }`) as an item of raw memory. `C.errno()` gives what the last call of C left in `errno`, read right after it, `C.setErrno(n)` sets what the next one finds, and `IoError.fromErrno(n)` makes an `IoError` of it; `C.lastError()` and `C.setLastError(n)` are the system's last error (`errno` on Linux and macOS, `GetLastError` on Windows), made an `IoError` by `IoError.fromCode(n)`, and the one taken for the other is an error. C's `long` and `unsigned long` are `CLong` and `CULong` (64 bits on Linux and macOS, 32 on Windows): an `int` becomes one and one becomes a `long` on their own, a `long` crosses with a cast, a number written for one fits in 32 bits, and they are compared and printed but have no arithmetic. `int snprintf(raw byte s, nuint n, read CString format, ...)` declares a function that takes a varying count of arguments: each value after the parameters says the type C reads, so a `byte`, a `bool` or a `float` there, and a number or `null` written without a type, are errors whose hint is the cast (`(long)30`). The interpreter builds a small library of wrappers with the C compiler and opens it (`dlopen`, on Windows a DLL with `LoadLibraryExW`, looked for in the program's folder, the folders of `-L` and `System32`); compiled code calls C directly. In an `if` on `OperatingSystem.isWindows()`, a function of C of the other system is not linked, so one file calls each system's own. While a call of C waits, the other tasks run, in the interpreter too. `examples/sqlite` and `examples/curl` wrap SQLite and libcurl in modules whose users write no `unsafe` |
| The layout of C | `union Bits { uint word` / `float number }` holds one field at a time in the same bytes: built with one (`Bits { number = 1.5 }`), written whole (`bits.word = 3`), read inside `unsafe` (`unsafe bits.word`), never lent. `uint version : 4` is a bit field of a struct, signed or not, `bool` too, and `uint _ : 4` leaves bits empty; a value that does not fit is a panic, as overflow is (the lowest bits in a release build). `packed struct Header { ... }` has no padding, and its fields are copied out, not lent. `align(64) struct Slot { ... }` lies at a multiple of 64 bytes wherever it is kept, on the heap too. Each is laid out as C lays it out on the machine, Windows' rules for bit fields included. `Header.fromBytes(bytes)` reads a struct of integers, fractions, bit fields and unions out of exactly as many bytes as it takes, with no `unsafe` (`examples/headers`: a BMP header and an IPv4 header) |
| C calling SG | `export int compareInts(raw void a, raw void b) { ... }` is a function C calls by the name written, without its module: it takes and gives only the types C has (integers, `float`, `double`, `bool`, `raw T`, `CFunc`, structs of plain data by value), and SG calls it by its name too. `CFunc<raw void, raw void, int>` is a pointer to a function as C calls it, plain data that may be `null`: the name of an `export` function or of a function of C is one (`unsafe qsort((raw void)items, n, 4, compareInts)`, `CFunc<int, int> f = abs`), `unsafe (CFunc<int, int>)p` takes one from a `raw void` (`dlsym`), `(raw void)f` gives it back, and every call through one is inside `unsafe`; calling a `null` one is a panic. The state a callback reaches goes through C's `void*`: `unsafe Memory.addressOf(ref state)` gives the address of a variable or a field, and `unsafe (ref State)data` (or `read`) lends it again inside the callback; the interpreter keeps the addresses it gave and panics on a wrong type, a value that ended or moved, or an address it did not give. A C file of the program calls an `export` function by its name. A panic inside one ends the process with its message, since nothing unwinds through C; the interpreter runs one only on a thread that is in a call of C, and compiled code on any thread. A `lock` block does not call C when an `export` function waits. A struct or an enum of SG that is not plain data goes to C by its address: an `export` function gives it as `T*` (C keeps it), takes it back as `T*` or `T*?` (it ends there; `T*?` takes `NULL`), and is lent it as `read T` or `ref T` for the call; `read CString` is C's `const char*`, which `string.fromCString(text)` copies with no `unsafe`. `NULL` where none is taken is a panic; the interpreter also keeps every address it gave and panics on one it did not give, of another type, given back already, or lent against the rules of borrowing |
| A library of SG for C | `sg build folder --library` gives `libfolder.a`, `--shared` gives `libfolder.so` (`.dylib` on macOS), each with `folder.h`; on Windows `folder.lib`, and `folder.dll` with its import library `folder.dll.lib`, the header of its `export` functions: the types C holds by their address declared without fields (`typedef struct Calculator Calculator;`), plain structs laid out as SG lays them out, pointers to functions written out, `extern "C"` for C++ and a macro for Windows' DLLs. A library has no `main` and at least one `export` function; only those are seen from outside, so two libraries of SG live in one program, each with its runtime, which starts what it needs on first use from any thread. A panic aborts. A shared library is not unloaded. `examples/calclib` is a calculator called from `main.c` |
| Without an operating system | `sg build folder --freestanding` gives `folder.o` for a kernel, a boot loader or a device: no `main`, the code that starts the machine calls the `export` functions; `--target riscv64-unknown-elf` compiles it with Clang for that machine, and what follows `--` goes to the C compiler. A panic gives its message to the program's `void onPanic(read string message)`, after which the processor waits; memory comes from its `raw byte allocate(nuint size, nuint align)` and goes back through `void release(raw byte block, nuint size, nuint align)`, and a program without them allocates nothing, which the build checks statement by statement. What needs a system (`print`, files, the clock, tasks, `HashMap`, `<math.h>`) is a compile error where it is written, also through a function of the language's own module. The object takes nothing of C's library: what the runtime used of it is written in, and `memcpy` and its kin are weak functions of its own. `examples/bare` runs with a machine of this computer, on Linux, macOS and Windows; on Windows the object asks for no library of C either |
| A kernel | `examples/kernel` is a kernel for RISC-V that QEMU's `virt` machine boots in machine mode, with no firmware: it writes to the serial port, takes five interrupts of the timer and powers the machine off. The serial port, the timer and the device that powers off are driven in SG with `Memory.readVolatile` and `writeVolatile`; what SG has no words for (the start of the core, the vector of its traps, its own registers and `wfi`) is the C file beside it, compiled into the same object. The vector hands `onTrap` the kernel's state through `mscratch`, and the interrupt counts in an `atomic ulong`. An unexpected trap is a panic told on the serial port. `sg build` takes the Clang `CC` names; the object links alone, with nothing of the C compiler's library, and `--gc-sections` leaves out what nothing calls |
| Async | `async int count(string path) { ... }` is an async function: a call gives a `Task<int>` that has started and runs until it first waits, then the caller goes on, as in C#. `await t` waits for a task without holding the thread and gives what it returns (`try await f()` passes its error on); `await f(read xs)` runs `f` as part of this task, so it can borrow, while a call without `await` follows the thread rule. `Task.delay(ms)` is a task that ends after so many milliseconds. `async void main()` and `async Result<void, E> main()` await too. In an async function `wait` is an error (`await` instead), and so is anything that holds the thread: input and output that is not async, and a function that does it, followed through the calls (`await Task.run(work, x)` runs such work apart). `await` in a `lock` block is an error, and an async function that awaits itself, directly or around. Compiled too: a machine with its variables in a frame, run by a pool of threads, each with a queue of its own that the others steal from. A method is async too (`public async int total(read this)`): one that lends `this` is awaited where it is written, one that takes it by value may start as a task; `create`, `format`, `equals`, `hash` and `compare` are not async |
| Cancellation | `CancellationSource()` or `CancellationSource.after(ms)` makes a source; `source.token()` gives a `CancellationToken` (`token.clone()` one more, `CancellationToken.None` one never cancelled), `source.cancel()` cancels, through a `read` borrow too. A task looks at its token where it chooses: `token.isCancelled()`, or `try token.check()`, which gives `Error(Cancelled { })`; `try` turns it into `IoError.Cancelled(...)` in a function that returns an `IoError`. `await Task.delay(ms, read token)` ends early with that error. Compiled too |
| Channels | `var jobs = Channel<int>(16)` is the end that receives, with room for 16 items; `jobs.sender()` gives a `Sender<int>`, and `.clone()` one more end of either kind. `tx.send(x)` waits while the channel is full and gives `false` when no end receives any more (the item is dropped); `jobs.receive()` waits while it is empty and gives `null` once every sender has ended and nothing is left, so there is no `close` to forget. `for job in jobs` receives until then. Several receivers share the items, each going to one of them in the order they were sent. In async code `await tx.sendAsync(x)` and `await jobs.receiveAsync()`; a token may come last on all four, which then give a `Result` whose error is `Cancelled`. An item goes to another task, so it follows the thread rule, and is never a `T?`; a send or a receive waits, so it is not in a `lock` block; a `for` over a channel while a sender this function made of it lives is an error, since it would never end. Tasks that wait on channels for each other stop the interpreter with a panic |
| A list of tasks | `var tasks = List<Task<int>>()`, `tasks.add(count(path))`: tasks kept in a list, which has only `add` and `length`. `await Task.whenAll(tasks)` takes the list and gives what every task gave, in the order of the list; `await Task.whenAny(ref tasks)` gives what the first task to end gave and takes it out of the list. Both are awaited, or waited for with `.wait()` in code that is not async, where they are written. A list of tasks is given to `Task.whenAll` or emptied by a loop `while tasks.length() > 0` without a `break`, or it is a compile error; a way out with an error waits for what is left in it. It is kept where a task is kept, and it does not go to another task. In the interpreter the tasks are waited for in the order of the list; compiled, each list has a group that its tasks tell as they end |
| Async input and output | `File.readTextAsync(read path)`, `readBytesAsync`, `writeTextAsync`, `writeBytesAsync`, `appendTextAsync`, `openTextAsync`, `createTextAsync`, `openAppendAsync`, on a reader (`Stdin` too) `readLineAsync(ref line)`, `readTextAsync()`, `readBytesAsync(ref buffer)`, `readSomeAsync(ref buffer)`, and on a writer (`Stdout` and `Stderr` too) `writeAsync(x)`, `writeLineAsync(x)`, `writeBytesAsync(bytes)`, `flushAsync()` and `closeAsync()`: the work of the function without `Async`, as a task, with a token that may come last (`read token`; not for `closeAsync`). A call given a borrow, and `closeAsync`, is awaited where it is written. A writer an async function drops without `closeAsync` is written and closed the async way, awaited where it is dropped. In the interpreter the work is done when it is called; the token is looked at first. Compiled too: the work is done on a thread of input and output while the task waits, and what the block of a reader or a writer can do is done in place. A read of an input that can make the program wait (a pipe, a terminal) waits on a thread in both, and a cancel ends the wait; what comes later stays for the next read |
| Network | `TcpListener.bind("127.0.0.1:8080")` (a number, `localhost`, or `":8080"` for every address of the machine; port 0 lets the system choose, `listener.port()` tells which), `listener.accept()`, `TcpStream.connect("example.com:80")` (the host looked up by name, each of its addresses tried in turn), `stream.receive(ref buffer)` (what has come, at most what the buffer holds; how many, 0 once the other side has let go) and `stream.send(bytes)` (all of them, through a `read` borrow): each gives a `Result` with an `IoError` (`ConnectionRefused`, `ConnectionReset`, `AddressInUse`, `HostNotFound`, `LineTooLong` are the network's). `stream.reader()` gives a `StreamReader` that borrows the stream and reads it through a block of its own (`readLine`, `lines()`, `readText`, `readBytes`, `readSome` and their async versions), while the stream goes on sending; a line or a text longer than 64 KB (`stream.reader(limit)` for another limit) is `LineTooLong`, and so is every read after it. A `receive` while the reader is used again is a compile error, and neither `send` nor `reader()` goes through a `shared` stream. `acceptAsync`, `connectAsync`, `receiveAsync` and `sendAsync` do the same work as a task, awaited where they are written, with a token that may come last; a cancel ends a wait at once and loses nothing that came. A socket sends without a buffer and is closed when it ends. In an async function the calls without `Async` are an error, as for files. `stream.localAddress()` and `stream.remoteAddress()` give the two ends as a `SocketAddress`. Datagrams: `UdpSocket.bind("127.0.0.1:0")`, `socket.sendTo(bytes, to)` (one message, whole) and `socket.receiveFrom(ref buffer)` (one message and who sent it, a `Received`), `connect(to)` with `send` and `receive`, all through a `read` borrow so a `shared` socket serves several tasks; a datagram that does not fit is `MessageTooLong`. A `SocketAddress` is read by `SocketAddress.parse("[::1]:53")` and printed the same way, and `SocketAddress.lookup("example.com:53")` gives the addresses of a name. In the interpreter a socket that is not ready gives the turn away, and the scheduler waits for the sockets with `poll` when no task can run. Compiled too: a call that has to wait waits with `poll`, and an async one on the loop of the network (epoll, kqueue), which a thread of the pool with nothing to do waits on. On Windows the same, through Winsock: the loop asks the driver under the sockets (AFD) on an I/O completion port, as mio and Tokio do, and the interpreter waits with `WSAPoll`; a listener and a datagram socket hold their port alone, and a datagram socket that is not connected is not told of a datagram that came back, as on Linux and macOS. Windows tells of a refused connection after about two seconds, and `localhost` tries `::1` before `127.0.0.1` (1025) |
| Time | `Instant.now()` reads the monotonic clock: `start.elapsed()` and `later.since(earlier)` give a `Duration`, and an `Instant` is not printed, since only the time between two means something. `Duration.fromMilliseconds(250)` (and `fromNanoseconds`, `fromMicroseconds`, `fromSeconds`) is a span of time that may be negative; `d.milliseconds()` and the other units give the whole count cut toward zero, `d.totalSeconds()` a `double`, `plus`, `minus` and `times` the arithmetic, and `print(d)` writes `1.5s`, `250ms`, `3µs`. `DateTime.now()` reads the clock of the system in UTC; `DateTime.utc(2026, 11, 15, 9, 30, 0)` gives a `Result<DateTime, DateError>`, `DateTime.fromUnixSeconds(n)` a moment from the seconds since 1970; `year()`, `month()`, `day()`, `hour()`, `minute()`, `second()`, `nanosecond()`, `dayOfWeek()`, `unixSeconds()`, and `print` writes ISO 8601: `2026-11-15T09:30:00Z`. `d.toLocal()` gives a `DateTimeOffset`, the moment seen at the offset of the system's time zone (`2026-11-18T12:30:05+03:00`, its parts those of the place), `d.withOffset(offset)` one at an offset of your own, `DateTimeOffset.local(2026, 3, 29, 2, 30, 0)` the moment the clocks of the place show that at (`SkippedLocalTime` in the hour summer time skips, the earlier of the two in the hour it repeats), and `t.toUtc()` the moment back; `DateTime.parse(text)` and `DateTimeOffset.parse(text)` read RFC 3339 (`InvalidText(position)` where the text stops being one). A format of your own is written with the parts: `$"{t.day():D2}.{t.month():D2}.{t.year()}"`. All four are compared, hashed and copied. They are written in SG, in the language's own module `std`, which a program that names one of them gets |
| Random numbers | `Random.fromSeed(42)` is a generator that gives the same numbers for the same seed, in the interpreter, in compiled code and on every system (xoshiro256\*\*); `Random.fromSystem()` one seeded by the system, different on every run. `r.nextInt(1, 7)` and `r.nextLong(low, high)` give a number from `low` up to `high`, without `high`, each as likely as the others; `r.nextDouble()` a fraction from 0 up to 1, `r.nextBool()`, `r.nextUlong()` 64 random bits, `r.fill(ref bytes)`. A `Random` is lent with `ref` and is not copied, so that two never give the same numbers. It is not for a key or a token: `SecureRandom.fill(ref bytes)`, `SecureRandom.nextInt(low, high)`, `SecureRandom.nextLong(low, high)`, `SecureRandom.nextUlong()` and `SecureRandom.hexString(32)` (32 hex digits, 128 bits) read the random bytes of the system. Written in SG in the module `std`, like the clock |
| Folders and the environment | `File.delete(path)`, `File.move(from, to)` (never takes the place of a file: `AlreadyExists`, told in the same step as the rename), `File.replace(from, to)` (takes its place in one step) and `File.copy(from, to)` (a new file, with the permissions) give a `Result<void, IoError>`. `File.info(path)` gives a `FileInfo` with `kind` (`FileKind.File`, `Directory`, `Link`, `Other`), `size` and `modified` (a `DateTime`); `File.exists(path)` and `Directory.exists(path)` a `bool`. `Directory.create`, `createAll`, `delete` (an empty folder) and `deleteAll` (all it holds, links not followed); `Directory.list(path)` gives a `List<DirectoryEntry>` (`name`, `kind`) in the order of the bytes of the names. `Directory.current()`, `Directory.temp()`, `Directory.createTemp()` (a new folder only the user can go into), `File.createTemp()` (a new empty file only the user can read and write, its path given). `Path.join(a, b)` gives a new path; `Path.joinInside(folder, name)` a `Result<string, IoError>` that is an `InvalidInput` for a name that could lead out of the folder on any system (`..`, a root, a `\`, a `:`, a device of Windows); `Path.fileName`, `Path.extension` and `Path.parent` a view of it. `Environment.get(name)` gives a `Result<string, VariableError>` (`Missing`, `InvalidText(position)`); the variables are only read. Written in SG in the module `std`; only the calls of the system are the compiler's |
| Other programs | `var git = Command("git")`, then `git.arg(word)`, `git.args(read words)`, `git.directory(path)`, `git.setVariable(name, value)`, `git.removeVariable(name)`, `git.clearVariables()` and `git.input(text)`; no shell reads the words. `git.run()` gives a `Result<void, ProcessError>` (our input, output and standard error), `git.output()` a `Result<string, ProcessError>` (what it wrote, as text), each taking an end with anything but 0 for `Failed(status)`; `git.status()` a `Result<ExitStatus, IoError>` (`Code(int code)`, `Signal(int number)`). A name is looked for in the folders of `PATH` that start with `/`. Written in SG in the module `std`. On Windows the same through `CreateProcessW`: a name without an extension is looked for as `name.exe` in the absolute folders of `PATH` (apart at `;`, a quoted folder taken whole) and never in the folder the program works in; the words go on one command line, quoted so that the program reads them back as written; a `.bat` or `.cmd` runs only when named with its extension, through `cmd.exe`, its arguments escaped as Rust escapes them, a line break in one an `InvalidInput`; a cleared environment keeps `SystemRoot`; a code with its highest bit set is written in hex (`exited with code 0xC0000005`); `kill`, `terminate` and `signal` end the program with the code 1, since Windows has no polite stop to send (`Signal.WindowChange` does nothing). `git.start()` gives a `Result<Process, IoError>` and goes on: the streams asked for with `pipeInput()`, `pipeOutput()` and `pipeError()` are taken out once (`process.takeInput()` a `StreamWriter?`, `takeOutput()` and `takeError()` a `StreamReader?`), `process.wait()` takes it and gives how it ended, `kill()` and `terminate()` stop it, `signal(Signal.Interrupt)` sends it a signal by its name, `id()` gives its number. A `Process` in a local variable is waited for on every path; one that ends without that stops its program with the signal 9. `runAsync`, `outputAsync`, `statusAsync` and `waitAsync` take a token that may come last, and a cancel stops the program |
| Signals | `var signals = Signals.listen(read List<Signal> { Signal.Interrupt, Signal.Terminate })`: while the listener lives, those signals do not end the program; `signals.next()` waits for the next and gives it, `await signals.nextAsync(read token)` without holding the thread. Several of one kind before a call are one; every listener gets each signal it listens for. `Signal` is `Interrupt`, `Terminate`, `Hangup`, `Quit`, `User1`, `User2` or `WindowChange`. `Signals.raise(signal)` sends one to this program. Written in SG in the module `std`; the handler only writes into a pipe, and a thread hands each signal on. On Windows `Interrupt` is Ctrl-C and Ctrl-Break, `Terminate` the console closed, the user logging off and the system shutting down (the program is then held until it ends, within the few seconds Windows gives); the others never come there, and `raise` hands them to listeners all the same; with no listener a raised signal ends the program as Ctrl-C does (`0xC000013A`), and `WindowChange` does nothing |
| Atomic values | `atomic T` for an integer, `bool`, `float`, `double` or `raw T`, made with `atomic 0` and written anywhere a value is (a field, a local, a list, an array `atomic uint[4] { ... }`, raw memory as `Memory.allocate<atomic long>(n)` or `(raw atomic int)p`); tasks share one as `new shared atomic 0` or in a `shared` struct, since its methods work through `read` and `shared`. `count.load()`, `count.store(v)`, `count.exchange(v)`, `count.compareExchange(expected, desired)` and `compareExchangeWeak` (a `Result<T, T>`: `Ok` with the value it wrote over, `Error` with the one it found), `count.fetchAdd(n)` and `fetchSub` on numbers (the value before; overflow is a panic in a debug build and wraps in a release build, `wrappingFetchAdd` wraps in both), `fetchAnd`, `fetchOr`, `fetchXor` on integers and `bool`. Each is sequentially consistent unless an order follows the values, written as a variant: `count.load(MemoryOrder.Acquire)`, `count.compareExchange(a, b, MemoryOrder.AcquireRelease, MemoryOrder.Relaxed)`; an order the operation cannot keep is a compile error. `Atomic.fence(order)`, `Atomic.compilerFence(order)`, `Atomic.spinLoop()`. `struct Pool : unsafe Sendable` goes to another task with its raw pointers, as its writer promises. `examples/queue` is a queue without locks for many producers and many consumers, with no `unsafe` |
| `volatile` | `unsafe Memory.readVolatile(regs[0].status)` and `unsafe Memory.writeVolatile(regs[0].control, 1)`: a place in raw memory read or written once, where it is written, for the registers of a device; it orders nothing between tasks |
| Null | `null`, `x is null`, `x is not null`, `a ?? b`, `a ?? panic("...")`, narrowing of local variables and parameters, also along `&&` and `\|\|` (`if name is not null && name.length() > 2`, `if text is null \|\| text.length() == 0 { return }`), under `!` (`if !(x is null)`) and after `while x is null { ... }`. `a?.b.c`, `a?.m()`, `a?.b?.c`: `null` when `a` is, and the rest of the chain does not run; a field that is not `Copyable` is lent (`a?.name` is a `read string?`), and `a?.b is not null` narrows `a` |
| Variables | `var x = 1`, `int x = 1`, assignment, `+=` `-=` `*=` `/=` `%=`, `&=` `\|=` `^=` `<<=` `>>=`. A variable keeps its type. An assignment computes its right side first, then finds the place. A name never hides another: a new local, the name of a `for` or of a pattern with the name of a local or a parameter of a block around it is a compile error, as in C#; blocks side by side, and code after a block has ended, may use one name |
| Control flow | `if` / `else if` / `else`, `while`, `for i in a..b`, `for i in (a..b).reversed()` (from `b - 1` down to `a`), `for x in xs`, `break`, `continue`, `return`. A loop may have a name, `outer: for ...` or `outer: while ...`, which `break outer` and `continue outer` use from a loop inside it; a name not used is an error. What a loop moved is gone after it as it was at each `break`, before its first round and at the end of each round |
| Operators | `+ - * / %`, `== != < <= > >=`, `&& \|\| !` (short circuit), unary `-`, cast `(int)x`. On integers `& \| ^ ~ << >>`; they bind tighter than a comparison, so `flags & mask == 0` tests the masked value |
| Numbers | `42`, `0xde`, `0b1010`, and `_` between digits: `1_000_000`, `0xdead_beef` |
| Text | `"..."` with `\n \t \r \0 \\ \" \'` and `\u{1F600}`, `$"{a}: {b}"` with `{{` and `}}`, raw text `"""a "quoted" C:\path"""` with no escapes, on lines with the indentation of its closing quotes taken off, and `$"""...{a}..."""`, `.toString()`, `text.length()`, `text.clone()`. `text[i]` reads one byte. `text[a..b]`, `text[..b]`, `text[a..]` and `text[..]` give a `read string`. `trim()`, `trimStart()`, `trimEnd()` give a `read string`; `indexOf(x)` and `lastIndexOf(x)` a `nint?` in bytes; `contains(x)`, `startsWith(x)`, `endsWith(x)`, where `x` is a `char` or a `read string` (or a `byte` for the two positions); `equalsIgnoreAsciiCase(other)`. `replaceAll(old, new)` (each a `char` or a `read string`) and `repeat(n)` give a new `string`; a call of them whose result is not used is an error |
| Building text | A `string` grows at its end: `line.write(x)` adds whatever `print` takes (`$"{name}: {count}\n"`, a number, a `char`, a text, a `Formattable` value), `line.clear()` empties it and keeps its memory, `string.withCapacity(n)` is an empty text with room for `n` bytes |
| Writing a number | Inside `$"..."` a width and a letter may follow the value, as in C#: `{n,5}` takes at least five characters with spaces in front, `{name,-10}` with spaces after (counted in characters, for every value `print` takes); `{b:X2}` and `{crc:x8}` write hex with at least so many digits, `{mask:b8}` binary, `{n:D3}` decimal with at least three digits, `{price:F2}` a fractional number with two digits after the point; `{price,8:F2}` both |
| Going over text | `for c in text.chars()` gives each `char`; `text.split(',')` (a `char` or a `read string`), `text.splitWhitespace()` and `text.lines()` give each part as a `read string`, nothing copied. Kept in a `var`, the iterator gives the next part with `parts.next()`, `null` at the end |
| Characters | `char`: one Unicode character. `'+'`, `'é'`, `'\n'`, `'\u{e9}'` is a `char`, and a `byte` where an integer goes (`byte b = '+'`, `text[i] == ':'`). `(int)c` is its code, `(char)n` the character of a code, `n.tryToChar()` gives `null` where `(char)n` would panic. `==`, `<`, `match`, keys, `sort`, `print`. `isAsciiDigit()`, `isAsciiLetter()`, `isAsciiAlphanumeric()`, `isAsciiUpper()`, `isAsciiLower()`, `isAsciiWhitespace()`, `isAscii()`, `toAsciiUpper()`, `toAsciiLower()` on `char` and on `byte`; `isWhitespace()` and `utf8Length()` on `char` |
| Unicode | On `char`, from the tables of Unicode 17.0: `isLetter()` (category L), `isNumeric()` (N: `'٣'`, `'½'`), `isAlphanumeric()`, `isUpper()` (Lu), `isLower()` (Ll), `isControl()` (Cc), and `toUpper()`, `toLower()`, one character for one (`'ß'.toUpper()` is `'ß'`). On a text: `toUpper()` and `toLower()` give a new text by the full mappings (`"straße".toUpper()` is `"STRASSE"`, `Σ` at the end of a word is `ς`), `foldCase()` the text as it is compared without case, `a.equalsIgnoreCase(b)` compares so without allocating. No language is taken into account: `"i".toUpper()` is `"I"` |
| Function values | `Func<int, bool>`: the address of a named function, the last type is what it returns (`Func<void>` takes and returns nothing). The name of a function is the value (`apply(double, 5)`, `Func<int, int> f = double`); it is copied freely, kept in variables, fields, variants and lists, and called like a function: `f(4)`, `this.onClick(read this.label)` |
| Built-ins | `print(x)`, `printError(x)` (the same, to the standard error), `panic("...")`, `assert(condition)`, `exit(code)` (ends the program with that code), `same(a, b)` (two `shared` values point to one object) |
| Files and the input | `File.readText(path)` gives a `Result<string, IoError>`, `File.readBytes(path)` a `Result<List<byte>, IoError>`; `File.writeText(path, text)`, `File.writeBytes(path, bytes)` and `File.appendText(path, text)` a `Result<void, IoError>`. `File.openText(path)` gives a `Result<StreamReader, IoError>`, `File.createText(path)` a `Result<StreamWriter, IoError>`, `File.openAppend(path)` one that writes after what the file holds, and `File.openWrite(path)` one that writes from the start over what it holds (a missing file is made). A reader has `readLine(ref line)` (the next line into `line`, `false` at the end), `readText()` (what is left), `lines()` (each line as a `Result<string, IoError>`) `readBytes(ref buffer)` (bytes into the buffer, as many as it holds unless the end comes first, and how many: a `Result<nint, IoError>`) and `readSome(ref buffer)` (what the block holds, or what one read of the system gives when it is empty; how many, 0 at the end); a writer has `write(x)` and `writeLine(x)` (what `print` takes), `writeBytes(bytes)` (a `read byte[]` as it is), `flush()` and `close()`, each with a `Result<void, IoError>`. A reader and a writer of a file move with `seek(SeekFrom.Start(n))`, `SeekFrom.End(n)` or `SeekFrom.Current(n)`, which gives the new place as a `Result<long, IoError>`; a reader moves inside its block without the system, a writer writes what it holds first (`seekAsync` in an async function); a place before the start is `InvalidInput`, and a pipe, a writer of `openAppend` or `Stdin` lent on is `NotSeekable`. `Stdin` is a `StreamReader`, `Stdout` and `Stderr` are `StreamWriter`s, lent on with `ref Stdin`. The built-in `enum IoError { NotFound` / `PermissionDenied` / `IsDirectory` / `NoSpace` / `AlreadyExists` / `NotDirectory` / `DirectoryNotEmpty` / `InvalidInput` / `NotSeekable` / `InvalidText(long position)` / `Other(int code) }`, with the variants of the network and `Cancelled` |
| Tests | `test "name" { ... }` at the top level of a file, run by `sg test file.sg`: one line per test (`ok` or `FAIL` with the reason), exit code 1 if any failed. In a test `try` passes on an error of any type, and the test fails with it. A test that passes and leaves `shared` values alive fails with the leak report. Compiled (`sg test --compiled`), each test runs in a process of its own, the program of tests started again with `--only N`; a debug build counts the blocks of its values, and a program or a test that ends with some not given back fails with a report. A normal run leaves the tests out |
| Editor | `sg lsp`, the language server an editor starts: the first error of the program a file belongs to as it is written, with its notes and hint and a link to a second place it names, what the editor has not saved read instead of the disk; a fix applied with one click for a conversion `(int)x`, `.clone()` of a text only borrowed, `_ = ` before a value dropped on purpose, and a missing `use`. `editors/vscode`: colours (a TextMate grammar), brackets, comments and indentation for VS Code, and its client of `sg lsp` |
| Comparing by name | `a.compare(b)` on numbers, `char` and text (negative, 0, positive), `a.equals(b)` on those and `bool`: what `<` and `==` are short for |
| Integer methods | `wrappingAdd`, `wrappingSubtract`, `wrappingMultiply`; `tryAdd`, `trySubtract`, `tryMultiply`, `tryDivide` give `null` where the operator would panic; `saturatingAdd`, `saturatingSubtract`, `saturatingMultiply` give the nearest end of the type. `tryToInt()` and the others give `null` where a cast would panic, `wrappingToInt()` and the others keep the bits that fit, `saturatingToInt()` and the others give the nearest end; `min`, `max`, `clamp(low, high)`. `abs` on the signed types (`int.MinValue.abs()` overflows as `-x` does), `tryAbs`, `wrappingAbs`, `unsignedAbs` (a `uint` for an `int`); `tryNegate`, `wrappingNegate`. The bits (C23's `<stdbit.h>`): `countOnes`, `countZeros`, `leadingZeros`, `trailingZeros`, `leadingOnes`, `trailingOnes` (an `int`), `rotateLeft(n)`, `rotateRight(n)`, `reverseBytes`, `reverseBits` on every integer type; `isPowerOfTwo`, `nextPowerOfTwo`, `log2` on the unsigned ones |
| Fractional methods | `abs`, `sqrt`, `floor`, `ceil`, `round`, `roundToEven`, `truncate`, `fraction`, `isNaN`, `isInfinite`, `isFinite`, `isSignNegative`; `exp`, `exp2`, `expMinusOne`, `log`, `log2`, `log10`, `logOnePlus`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`, `sinh`, `cosh`, `tanh`, `asinh`, `acosh`, `atanh`, `cbrt`, `hypot`, `pow`, `erf`, `erfc`, `gamma`, `logGamma`, `fusedMultiplyAdd`, `ieeeRemainder` (beside `%`, which is C's `fmod`), `copySign`, `exponent` (an `int?`), `scaleByPowerOfTwo`, `nextUp`, `nextDown`, `min`, `max`, `clamp`; `tryToInt()`, `saturatingToInt()` and the others |
| Numbers from text | `int.parse(text)`, and the same on every number type, gives a `Result<int, NumberError>`; the built-in `enum NumberError { Invalid(nint position)` / `OutOfRange }` says where the text stops being a number, or that the type does not hold it. `int.parseHex(text)`, and the same on every integer type, reads hex digits (`ff`, `FF`, no `0x`) |
| Numbers as bytes | `ulong.fromLittleEndian(bytes)` reads the number that exactly as many bytes as the type takes write, the lowest byte first (`ushort.fromLittleEndian(data[at..at + 2])`); `fromBigEndian` reads the highest first. `n.toLittleEndian()` and `n.toBigEndian()` give the bytes of the number as a `byte[N]`. On `short`, `ushort`, `int`, `uint`, `long` and `ulong` |
| Constants | `int.MinValue`, `int.MaxValue` and the same on every integer type; `double.MinValue`, `double.MaxValue`, `double.NaN`, `double.Infinity`, `double.Pi`, `double.E` and the same on `float` |

Design rules that are already enforced:

- Integer overflow panics in the interpreter, in tests and in a debug build; a release build wraps (Rust's
  way). `wrappingAdd` wraps in every build, `tryAdd` is checked in every build.
- A shift count follows the same rule: below zero, or not below the bits of the type, it is a compile
  error where it is known, a panic in the interpreter, in tests and in a debug build, and cut to the bits
  of the type in a release build. The bits that `<<` pushes out are gone in every build.
- An integer type widens to another on its own only when the other holds every one of its values on every
  machine: `int` to `long`, `byte` to `ushort`, `uint` to `long` and `ulong`, never a signed type to an
  unsigned one. `int + uint` is a compile error whose hint names a type that holds both (`long`).
  Narrowing is written `(int)x` and panics if the value does not fit.
- `float` and `double` (`1.5`, `1e9`) take the type they are used as. A number becomes fractional on its own
  only when the type holds it exactly (`int` to `double`, not `long`); the arithmetic is the hardware's and
  never panics, and nothing is fused. `==` and `<` are the hardware's, `equals`, `compare`, `sort` and `==`
  on a struct a total order. A cast to an integer drops the fraction and follows the overflow rule. A
  number is printed with the fewest digits that read back as it, the nearest of them (`0.1`, `2`, `1e+21`).
- `parse` reads a sign and digits for an integer, and also a point and an exponent for a fractional
  number; nothing else (no spaces, no `_`, no `0x`). What `print` writes it reads back. A whole number
  past the type is `OutOfRange`; a fractional one past the type is `Infinity`, as in Rust, C# and C.
- A panic of a cast or of an overflow names the method that does not panic (`tryToInt`, `tryAdd`). A
  `tryTo...` or `wrappingTo...` that cannot fail is an error, and so is `wrappingTo...` on a fractional
  number. A constant is a value of its type: `uint u = int.MaxValue` needs `(uint)`.
- `$"..."` is not a `string`; `string text = $"..."` is an error whose hint is `.toString()`.
- `readText`, `readLine` and `lines()` check the text: a byte that is not UTF-8 is
  `IoError.InvalidText` with its place, counted from the start of the file or of the input. A path is a
  `read string`; one with a zero byte in it is `NotFound`. A line comes without its `\n` or `\r\n`.
- A `StreamWriter` kept in a local variable (or given to a function by value) is closed, or moved
  somewhere, on every path before it ends: at the end of its block, at `return`, `break`, `continue` and
  `exit`, and before the variable gets another writer. An exit with an error (`try`, `return Error(...)`)
  lets it go: it writes its buffer as it ends. The hint is `try w.close()`, or `_ = w.close()` to drop the
  error on purpose. A writer in a field, a list or a `T?` is not followed. A panic and `exit` end the
  program without writing what a writer still holds.
- A `Task` kept in a local variable (or given to a function by value) is waited for on every path before
  it ends, as a `StreamWriter` is closed: at the end of its block, at `return`, `break`, `continue` and
  `exit`, and before the variable gets another task. An exit with an error (`try`, `return Error(...)`)
  lets it go: the task is waited for there and what it gives is dropped, since the caller gets the error
  that made the function stop. A `Task` is kept only as it is, in a local variable, a parameter or a return
  value: not in a field, a list, a `T?` or behind `read`. In an async function a task is waited for with
  `await t`, which takes it as `wait` does; the call of an async function alone on a line is an error, as
  `Task.run(...)` alone is.
- `Stdin`, `Stdout` and `Stderr` are lent, never moved or closed, and no variable may have their names.
  Reading and writing go on from where a stream is, so they need it as `ref`, not `read`.
- `exit` takes a `byte`, so `exit(256)` is a compile error. It gives no value, so a function that ends in
  it needs no `return`. In a test it fails the test.
- `write` and `clear` change the text, so they need it where it may change: not through `read` (the hint is
  `ref string`) and not in a `readonly` field. Writing a text into itself, or anything that looks at it
  (`s.write(s[..3])`, `s.write(s.trim())`), is an error whose hint is `.clone()`; a number computed from
  it is fine (`s.write($"{s.length()}")`). `text += "x"` is an error whose hint is `write`.
- `addAll` copies, so the items must be `Copyable` (for text the hint is `add(item.clone())`), and a list
  is not added to itself (`xs.addAll(xs[1..])`; the hint is `xs.add(xs[i])`). `sort` and `reverse` change
  the order of the items and not their number, so an array and a `ref T[]` have them and a `read T[]` does
  not; a range in front of them (`xs[2..5].sort()`) is lent as `ref`, with what `ref xs[2..5]` needs.
- `assert` prints the expression text and, for a comparison, the value of both sides.
- Deep recursion ends in a readable panic (limit: 5000 nested calls), not a crash.
- Errors come with the source line, a marker and, where the fix is known, a hint.
- Types are checked before the program runs. An integer literal takes the type it is used as
  (`long x = 5`); one that does not fit is an error.
- `T` is never `null`; only `T?` is. Reaching into a `T?` without a test is an error. After
  `if x is not null { ... }`, `while x is not null { ... }` or `if x is null { return }` the variable counts as
  `T` until it is assigned again. The same holds along a chain: `x is not null` on the left of `&&`, and
  `x is null` on the left of `||`, narrow `x` on the right (choice 264). `!` swaps what a test says, so
  `!(x is null)` narrows like `x is not null` (choice 287), and after `while x is null { ... }` the variable
  counts as `T` unless a `break` leaves the loop (choice 288). Fields are not narrowed: copy to a
  local first.
- The names of `x is Circle(r)` are seen on the right of `&&` and in the block of the `if` or `while` whose
  condition the chain is, not in `else`, and after the `if` only when its `else` leaves (choices 263, 294).
  The names of `x is not Circle(r)` (or `!(x is Circle(r))`) are seen where the test fails: on the right of
  `||`, in `else`, after an `if` whose block leaves, and after a `while` that only its test ends (choices
  293, 295).
- A value that was moved cannot be used again; the error shows both places. This covers branches (moved in
  either one) and loops (moved in one iteration, gone in the next).
- A field can be moved out of a value that is owned (`this.top = node.next`); the other fields stay usable,
  the value as a whole does not until the field gets a value again. Through `ref` a field is moved out only
  with `take` or `replace`, and the error names the one that fits.
- A field reached through `read` cannot be written or lent as `ref`, and a `ref this` method cannot be called
  on it.
- A method that takes `this` by value uses the receiver up, like passing it to a function.
- The receiver is never written with `read` or `ref` at the call; the method's first parameter decides.
- Inside a method a bare field name is an error whose hint is `this.name`.
- Every field of a struct literal gets a value, written or its default. A struct or an enum cannot contain itself by value; the hint
  is `T*`.
- A `match` handles every variant, or the error names the missing ones. An arm with `when` does not settle its
  variant. A bare name that is not a variant is an error whose hint is `var name`.
- `match x` is `var y = x`: an owner is moved in and the names in the arms own what the variant carried;
  `match read x` and `match ref x` borrow, and so does a `match` on something that is already a borrow.
- A borrow of a `T*` is accepted where a borrow of `T` is expected (`eval(left)` with `left` a `read Expr*`).
- A `Result` that is not used is an error, as a statement (`save()`) and as a variable that nothing looks at.
  The hint names `try` and `_ = ...`.
- A built-in method that only gives a value is an error when the value is not used: `line.trim()`,
  `distance.abs()` or `xs.contains(3)` alone does nothing. The built-in methods that change their value and
  give one too (`removeLast`, `next`, the `add` and `remove` of a set) and the functions you write may stand
  alone.
- A local variable whose value nothing reads is an error: one never used, and one only written (`x = 1`,
  `x += 1`, a write to a field or an item). A write through a borrow, a `shared` value or raw memory is a use,
  and so is every other use, a method called on it and `ref x` among them. A parameter is not looked at, nor a
  variable of a `Disposable` type or of a struct that holds one. The name of a `for`, of a pattern and of an
  `as` or `lock` block is held to it too; `_` stands for one that is not needed. A write to a copy of a struct
  has the borrow as its hint (`var p = ref rect.corner`).
- `try` needs a function that can pass the error on: one that returns a `Result` (or a `T?` for `try` on a
  `T?`). With a different error type, `try` wraps the error in the one variant of the caller's error type that
  carries it; none or more than one is an error.
- A value where a `Result` is expected is an error whose hint is `Ok(x)` or `Error(x)`; a `Result` where its
  value is expected is an error whose hint is `try`.
- A function that returns `Result<void, E>` ends with `return Ok()`.
- A `string` is indexed by byte and stays valid UTF-8: `text[i] = 'a'` is an error, an index or a range outside
  the text panics with the index and the length, and a range that cuts a character in half panics and names
  the character.
- A character that takes more than one byte (`'é'`) is an error where a `byte` goes; the hint is `char`.
- A `char` is not a number: `c + 1`, `c == 65` and `int x = c` are errors whose hint is `(int)c`; `char c =
  n` is one whose hint is `(char)n` or `n.tryToChar()`. `(char)n` panics in every build when no character
  has the code, and is a compile error when the code is known (`(char)0xD800`).
- While a `for` goes over a text (`for part in line.split(',')`), the text cannot be changed or moved; a
  part kept from `parts.next()` or `line.trim()` keeps the text from changing while it is used. An iterator
  over a text that has no name is an error whose hint is to name it. `split("")` is an error, and a
  separator that is empty at run time panics.
- A conversion whose value is known and does not fit is an error, not a panic: `(byte)300`.
- A `match` on a number needs `_` or `var name` for the values it does not list.
- `: Copyable` holds only if every field can be copied; the error names the field that cannot (`string`, `T*`,
  a struct that is not `Copyable`).
- `: Formattable` without a `format` method needs every field to be `Formattable`; with one, the method must be
  `void format(read this, ref Formatter f)`.
- `print(x)` of a struct or an enum that is not `Formattable` is an error whose hint is the two words to add.
- The error type of `Result<void, E> main()` must be `Formattable` (or a `string`), because it is printed.
- `==` on a struct or an enum that is not `Equatable` is an error whose hint is the word to add; so are `<` on
  one that is not `Comparable`, `sort` and `indexOf` on such items, and a key that is not `Hashable`.
- `: Equatable` without an `equals` needs every field to be `Equatable`; `: Hashable` needs `Equatable` and
  every field `Hashable`; `: Comparable` needs a `compare` method. A method with the wrong signature is an
  error that shows the right one.
- A failed `assert(a == b)` writes both sides, also when they are structs, enums or collections.
- `xs < ys` on two collections is an error, and so is a collection as a key.
- A test that fails (a false `assert`, a panic, an error passed on by `try`) does not stop the others. Two
  tests with the same name are an error; so is an error type passed on by `try` that is not `Formattable`.
- An index outside a `List` panics with the index and the length; a key that is not in a `HashMap` panics with
  the key. The message names `get`, which gives `null` instead.
- An item is never moved out of a collection through an index: `consume(names[0])` is an error whose hint is
  `.clone()`, and `var name = users[0].name` is one whose hint is `read` or `replace`.
- A collection reached through `read` cannot be changed: `add`, `clear`, `xs[i] = v` and the rest are errors
  whose hint is `ref`.
- A collection passed where a borrow is expected needs `read` or `ref` written: `count(xs)` is an error whose
  hint is `count(read xs)`.
- While a variable holds a borrow and is still used later, what it borrowed from cannot be changed, moved or
  lent as `ref`; with a `ref` borrow it cannot be touched at all. The error shows where the borrow was taken.
  After the last use of the variable the place is free again. The same place cannot be lent twice in one call
  when one of the two is `ref`, and what a part of a call lends (the value a method is called on, an argument,
  the value an index or a range looks into) cannot be changed or moved by a later part of it.
- A function that returns a borrow returns one that comes from a borrowed parameter. With one borrowed
  parameter nothing is written; with more, `from a, b` after the parameters says which, and leaving it out is
  an error that shows what to write. Returning a borrow of a local, of a parameter taken by value, or from a
  parameter `from` does not name is an error. With no borrowed parameter only a constant can be returned.
- A borrow kept in a variable cannot outlive the block of what it borrowed from.
- A struct or an enum with a field that borrows names its source (`struct Token<from source>`), and so does
  one that holds such a type; the errors show what to write. A value of such a type keeps what it borrowed
  from, like a `read` variable does. A function that returns one always says `from`.
- A collection that a `for` goes over by borrowing it cannot be changed inside the loop: `xs.add(v)`,
  `xs[i] = v`, `xs = ...`, `ref xs`, moving `xs`, and a `ref this` method on what holds it are errors that show
  the loop. The hint is to go over the positions, or to keep the changes until the loop ends. Looking at it and
  changing other fields is fine.
- After `for x in xs` the list is gone (it was handed over); the error shows the loop.
- `List<int> xs = ...` needs its item type, and a `HashMap` a key type that can be a key.
- A name of another module that is not `public` (a type, a function, a field, a method) is an error that
  names the module and, for a type or a function, shows where it is defined. A struct with a field that is
  not `public` cannot be built field by field outside its module; the hint is `T(args)` when the struct has
  a `public` `create`.
- A name that two modules named with `use` both have is an error where it is used; the hint is the full
  name (`stock.Point`). A name that is not found but is `public` in some module gets the hint `use stock`.
  `use` of a folder that is not there, `use` written twice and `use` below the first definition are errors.
- A `public` function, field or method of a module that shows a type which is not `public` is an error
  that shows both places.
- A `readonly` field cannot be assigned, lent as `ref`, or changed through (`account.history.add(1)`).
- An index that is known and lies outside an array is an error, not a panic: `buf[16]` with `byte[16] buf`.
  An index that depends on data panics with the index and the length, as for a list; so does a range
  outside a list, an array or a slice.
- An array gets every item when it is built: `int[3] { 1, 2 }` is an error, `string[2]("ada")` is one
  (text is not copied silently) and so is `string[2]` (text has no zero value). There is no memory that
  was never written.
- `byte[]` without `read` or `ref` is an error whose hint is `List<byte>` for an owner and `read byte[]`
  for a window. A list or an array passed where a slice is expected needs `read` or `ref` written; a text
  passed where bytes are expected gets the hint `.asBytes()`.
- A slice is a borrow: while one is kept and used later, what it looks at cannot be changed or moved; one
  cannot be returned if it looks at a local; a struct that keeps one names its source (`struct
  Packet<from data>`). A `read T[]` cannot be written through or turned into a `ref T[]`.
- A borrow of a value that has no name cannot be kept: `var part = make()[..5]`, `var label =
  make().label()`, `return first(read make())` and the same put into a field are errors whose hint is to
  give the value a name. Inside one line it is fine: `print(make()[..5])`.
- `ulong.fromLittleEndian(bytes)` takes exactly as many bytes as the type: a window whose width the program
  shows (`data[at..at + 4]`, `data[..4]`, an array) and that is another is an error whose hint is the window
  of the right width; one whose width depends on data panics with both numbers. `byte`, `sbyte`, `nint`,
  `nuint` and the fractional types have neither these functions nor `toLittleEndian`, each with a hint.

## The C back end

Since the sixteenth slice (12 November 2026) a program can be compiled: `sg build file.sg` writes it as C and
hands that to the C compiler (`sg build folder` for a program that is a folder; the modules it names are
compiled with it). The interpreter stays as the reference: `cargo test` compiles every program the
back end accepts and requires the same output as the interpreter, panic messages included, then runs the
same C once more under the address and undefined-behaviour sanitizers, and once more to see that it left no
memory behind (327 of 1319 programs for now; the others are almost all programs the checker rejects, on purpose; of the programs that run, one uses what it does not compile yet, 268). A
program that ends in a panic is not looked at for leaks: a panic does not unwind, and the system takes back
what the program owned (382); nor is one that a signal ends (797). A
program with tasks or `locked` values runs once more under the thread sanitizer, which stops it where two
threads reach the same memory without a lock between them; before the first of them, the thread sanitizer
is tried on a C program that races on purpose, and a check that does not see that race fails the test.

What is compiled so far: every integer type (`sbyte` to `nuint`), `bool`; text (`string`, `read string`, `ref string`:
constants, `text[i]`, ranges, `==` and `<`, `.length()`, `.clone()`, `$"..."` and `.toString()`); functions and
recursion; `if`, `while`, `for i in a..b`, `break`, `continue`; structs, by value, with methods; `read` and
`ref`; the checked operators, casts, the wrapping methods and the `try...` methods; `print`, `assert` and
`panic`. Since the eighteenth slice also the heap and missing values: `T*` and `new`, `T?` with `null`,
`is null` / `is not null` and what the checker learns from them, `??`, `?? return`, `?? panic(...)`, `try` on a
`T?`, `take()` and `replace()`, a `read T*` used as a `read T`, and methods and fields reached through a
pointer. Since the nineteenth slice enums: variants with and without values, `match` on an enum (owned,
borrowed with `read` or `ref`, behind a `T*`) and on an integer, `when`, `var other`, `is` / `is not` with
names in `if` and `while`, enums inside structs, `T?` and `T*`, and enums that own text, structs and pointers.
Since the twentieth slice `Result<T, E>`: `Ok(...)`, `Ok()` and `Error(...)`, `try` (the error goes to the
caller as it is, or inside the variant of the caller's error type that carries it), `??` and `?? return` on a
`Result`, `match` and `is` on one, `_ = f()`, and `Result<void, E> main()` whose error is text or a number.
Since the twenty-first slice `List<T>`: `List<T>()` and `List<T> { ... }`, `add`, `length`, `xs[i]` as a
value and as a place (`xs[i] = v`, `xs[i] += 1`, `xs[i].name`, `rows[1].add(4)`), `get`, `removeLast`,
`reverse`, `clear`, `for` over a list that is handed over, read or changed, lists in fields, lists of text,
structs, pointers and lists; `indexOf`, `contains` and `sort` on lists of integers, `bool` and text,
`string.join` and `text.indexOf`.
Since the twenty-second slice writing and comparing: `print`, `$"..."`, `panic` and `assert` write a struct,
an enum or a list as the interpreter does (also behind a `T*`, with `T?` fields and the struct's own
`format` and `f.write`); `==` and `!=` on structs, enums and lists (the struct's own `equals` where it
wrote one), `<` and the others through `compare`; `indexOf`, `contains` and `sort` on lists of such
items; `equals` and `compare` on numbers and text; and a `Formattable` error that `main` ends with.
Since the twenty-third slice types that carry a borrow (`struct Word<from source>` with a `read string`
field, an enum whose variants borrow, a `read Config` field) and function values (`Func<A, R>` in a
variable, a parameter, a field, a variant or a list, and calls through them); the order rules example is
compiled.
Since the twenty-fourth slice `HashMap<K, V>` and `HashSet<T>`: building them with and without items,
`add`, `map[key]` as a value and as a place, `get`, `getOrAdd`, `contains`, `remove`, `length`, `clear`,
`for` in its three forms (a map gives entries with `key` and `value`), `print` and `==`, with numbers,
`bool`, text and `Hashable` structs and enums as keys. The calculator and the log summary are compiled.
Since the twenty-fifth slice a struct, an enum or a collection is printed without an allocation, and most of
the places where the back end asked for a name are gone: a `for` over `read make()`, a `match` that borrows
from what a call gave, `if make() is Say(text)` and the same in a `while`, a method called on a `T*` that
has no name, a right side of `&&`, `||` or `??` that needs a text or a list for a moment, and a `panic`
message with a struct in it on the right of `??`.
Since the twenty-eighth slice a `match` gives its value inside a larger expression, and `try` and
`?? return` stand after an argument or a field that owns memory and has no name.
Since the thirty-first slice arrays and slices: `T[N]` in a variable, a field, a variant, a parameter and
a return type, built in its three forms, copied or moved as its items ask; `read T[]` and `ref T[]` from
a list, an array or a range of either; `x[i]`, `length`, `get`, `indexOf`, `contains`, `==`, `print` and
`for` on both; `text.asBytes()` and `string.fromUtf8`. The sensor window example is compiled.
Since the thirty-second slice the bit operators (`&`, `|`, `^`, `~`, `<<`, `>>` and their assignments),
written together with the checker and the interpreter. Since the thirty-third slice the other integer
types, `sbyte`, `short`, `ushort`, `uint`, `ulong` and `nuint`, also written together with the checker and
the interpreter. Since the thirty-fourth slice `float` and `double`, the same way. Since the thirty-fifth
slice `parse`, the constants, the conversions that do not panic and the math methods, the same way.
Since the thirty-sixth slice `char`, `chars`, `split`, `splitWhitespace`, `lines`, `trim` and the
searches in a text, the same way. Since the thirty-seventh slice building a text with `write`, `clear` and
`string.withCapacity`, the same way. Since the thirty-eighth slice files read and written whole, the input
read line by line, the command line, `printError` and `exit`, the same way. Since the thirty-ninth slice
files read and written piece by piece (`StreamReader`, `StreamWriter`, `Stdin`, `Stdout`, `Stderr`), the
same way. Since the forty-first slice the width and the letter after a value in `$"..."` (`{n,5}`,
`{b:X2}`, `{x:F2}`) and `parseHex`, the same way. Since the forty-second slice `list.addAll(slice)` and
`sort` and `reverse` on an array and through a `ref T[]`, the same way. Since the forty-third slice
patterns inside patterns, `match` on text and methods on enums, the same way. Since the forty-fourth slice
`or`, ranges, `null`, `true` and `false` in patterns, and `match` on a `bool` and on a `T?`, the same way.
Since the forty-fifth slice the names of `is` and the narrowing of `is not null` along `&&` and `||`, the
same way. Since the forty-sixth slice an assignment computes its right side before it finds the place, as
the interpreter always did (choice 270). Since the forty-seventh slice `replaceAll` and `repeat`, written
together with the checker and the interpreter. The forty-eighth slice is in the checker alone, and so is
the forty-ninth (narrowing under `!` and after a `while`). Since the fiftieth slice the names of `is not`,
and the names seen after an `if` or a `while`, written together with the checker and the interpreter; an
`is` with names on a value made by a call after the first `&&` or `||` is compiled when that value owns
no memory (299). Since the fifty-first slice `fromLittleEndian`, `fromBigEndian`, `toLittleEndian`
and `toBigEndian`, written together with the checker and the interpreter. The fifty-second slice is in
compiled code alone: in a checked build an index or an end of a range written `a + c` is tested for
overflow with the place, one comparison where there were two, and the program stops as it did (309). So is
the fifty-third: in a checked build a sum kept in a loop runs without its overflow test for as many rounds
as it has room for, the loop written twice (315); on the way the variable of `for i in a..b` became a copy
of the count in compiled code too, as it was in the interpreter (319). The fifty-fourth slice (`shared`,
`weak` and `using`) and the fifty-fifth (`shared checked`, the `as` blocks, `this` with a type and the leak
report) were written in the checker and the interpreter; since the fifty-sixth they are compiled too: a
`shared` value is a pointer with the counts of its owners and of its `weak` lookers in front of it, a
`shared checked` one has the borrows of it in progress there as well, and a debug build keeps the values
that are alive in a list for the leak report (347-355). The fifty-seventh slice (tasks, `locked` and
`lock`) was written in the checker and the interpreter, and since the fifty-eighth it is compiled: a task
is a thread of the system with a stack of 8 MB, a `locked` value has a mutex in front of its counts, and a
`lock` block takes its values in the order of their addresses and gives them back on every way out
(371-383). The fifty-ninth slice (async functions, `await` and `Task.delay`, 384-402) and the sixtieth
(cancellation and async input and output, 403-416) were written in the checker and the interpreter; since
the sixty-first, async functions, delays and cancellation are compiled too: an async function is a machine
whose variables live in a frame, a task is one block with its frame inside, and a pool of threads, one
for each core, runs the tasks that can go on (417-431). Since the sixty-second, async input and output is
compiled too: the work is done on threads of their own, started as they are needed, while the task waits,
and a read the block of a reader can answer is done in place (432-440). The sixty-third slice (a list of
tasks, `Task.whenAll` and `Task.whenAny`) was written in the checker, the interpreter and compiled code
together: a list of tasks has a group that its tasks tell as they end (441-452). So was the sixty-fourth
(streams opened, written and closed the async way): a writer an async task drops is put on the task's
list and closed on a thread of input and output where the task awaits it (453-465). So was the
sixty-fifth (the network: `TcpListener` and `TcpStream`, and a cancel that ends a wait for a socket or for
the input): one thread waits for every socket a task waits for, with epoll or kqueue (466-479; since the
eighty-first slice that thread is one of the pool, 667-677, and since the eighty-second a socket goes on
the loop once, 678-688). So was the
sixty-sixth (lines over a connection): `conn.reader()` is a `StreamReader` that borrows the connection
and keeps what has come in its block until a read can take all it needs, `send` goes through a `read`
borrow, and `readSome` gives what is there (480-490). The sixty-seventh made no new feature: it checks the
text a reader reads a block at a time, not a line at a time, and every program that reads lines got
faster (491-495), and the sixty-eighth prints a fractional number with Dragonbox (496-500). The sixty-ninth
(raw memory: `raw T`, `unsafe`, `Memory`) was written in the checker, the interpreter and compiled code
together: a `raw T` is a `T*`, `p[i]` is `p[i]`, and nothing is checked, while the interpreter checks every
item against its block (501-516). The seventieth (calling C: `extern` blocks, `CString`, `buffer.pointer()`)
was written in all three too: a call of C in compiled code is a call of C, through an inline adapter, while
the interpreter calls it through a library of wrappers it builds with `cc` (517-530). So was the seventy-first
(`Disposable` and `Closable` on types of your own, opaque C types, C structs by value): a `Disposable` struct
has a byte that a move sets, and its drop runs `dispose` before the fields when the byte is not set (531-544).
So was the eighty-fourth (a C file beside the program, the variables of C, `errno`, a pointer as a number):
the C file is taken into the unit of the program at its end, and a function of it is called by its name
from an adapter written after it, so it is inlined (719-730). So was the eighty-fifth (functions of SG that
C calls, `CFunc`, the state of a callback): an `export` function is a function of C's types and of its
own name that calls the function of SG, and the interpreter writes one into its wrappers that hands the
addresses of the arguments to the interpreter running the call of C on that thread (731-749). So was
the eighty-sixth (a varying count of arguments): each list of types a function that takes `...` is called
with gets an adapter of its own, which calls C as C calls it (750-760). So was the eighty-seventh (libcurl):
`Memory.view` is a pointer and a count, checked once, and the adapters of `...` are made per list of the
types of SG, since a `read CString` and a `raw byte` are one type of C but go over differently (761-778).
So was the eighty-eighth (signals): the handler of the runtime only writes into a pipe, and a thread hands
each signal to its listeners, in compiled code and in the interpreter alike (779-799). So was the
eighty-ninth (the layout of C): unions, bit fields, `packed` and `align` are written as C has them, and C
places them; the interpreter places them by the rules of the machine it runs on, which a test compares
with C for every system (800-829). So was the ninetieth (atomics and `volatile`): an atomic value is its
type in C, reached only through GCC's and Clang's `__atomic` builtins with the order written, and a
volatile access is the place read or written through a `volatile` pointer (830-859). So was the
ninety-first (a library built from SG): the unit of a program is a library once `main` gives way to a
constructor, every name of the runtime being `static` already; an `export` function that takes a value of
SG from C takes a `void*` and checks it for `NULL`, and a panic ends in `abort` (860-885). So was the
ninety-third (without an operating system): the runtime is the same, with the little of C's library it
used written in front of it (a `vsnprintf`, `strlen`, `memchr`, the bits of `frexp` and `ldexp`), the standard
error a buffer that a panic gives to `onPanic`, `malloc` the program's `allocate`, and `memcpy` and its kin
weak functions of the object (895-913). The ninety-fourth (a kernel in QEMU) added to it only the division of
128 bits a C compiler calls its library for, weak in the same way, and a section of its own for each function
(914-927).
What is still missing is an error that names it (not silence): the few positions listed with the
choices below (81, 111, 268).

Who frees memory: the checker has already decided who owns each value, and the back end follows it. An owned
value (a `string`, a `T*`, a struct, an enum or a `T?` that holds one) is freed where its variable ends, on every way
out of the block (`return`, `break`, `continue`, and the `return` inside `try` and `?? return`). A use that moves the value empties the place it came from, so the later free
finds nothing to do; a text constant has no capacity and is never allocated or freed. There is no garbage
collector, and a count of owners only where the program asks for several (`shared`).

A debug build (`sg build file.sg`) checks integer overflow and is not optimised; a release build
(`sg build file.sg --release`) wraps on overflow and is optimised. The interpreter and `sg test` behave like a
debug build; a cast that does not fit follows the same rule. An index or a range outside a text, division by
zero, `assert` and `panic` stop the program in every build.

Measured (`python3 bench/run.py`, Apple clang 21 on arm64, best of five, `-O2` for all three; the C is written by
hand with no checks):

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
| `counts`: a million numbers counted in a `HashMap` of fifty thousand keys, read back and a third removed, twenty times | 0.382 s | 0.376 s | 0.319 s | 1.20 |
| `show`: two million structs and half a million lists of numbers printed, one on each line | 0.176 s | 0.183 s | 0.377 s | 0.47 |
| `lookup`: three hundred million lookups in a list of records of 128 bytes, through a function that gives back a `read Record?` | 0.498 s | 0.488 s | 0.491 s | 1.01 |
| `buffer`: 32 bytes on the stack written as hex into a buffer of 64 through a `ref byte[]`, added up through a `read byte[]` and a range of it, fifteen million times | 0.280 s | 0.290 s | 0.283 s | 0.99 |
| `bits`: a sieve below sixty-four million with one bit for each number, then thirty million numbers written seven bits at a time into a buffer on the stack and read back | 0.322 s | 0.321 s | 0.407 s | 0.79 |
| `hash`: XXH64, the 64-bit xxHash, written with `ulong`, over one megabyte, two thousand times, the bytes read eight at a time with `ulong.fromLittleEndian` | 0.128 s | 0.129 s | 0.128 s | 1.00 |
| `nbody`: the sun and the four giant planets moved twenty million steps in `double`, a square root for each pair at each step | 0.661 s | 0.662 s | 0.679 s | 0.97 |
| `numbers`: a million lines of whole and fractional numbers read ten times with `long.parse` and `double.parse` and added up | 0.186 s | 0.191 s | 0.196 s | 0.95 |
| `floats`: a million fractional numbers, every other one of seventeen digits, written into a text with `$"{x}\n"` three times over and read back once (the C writes each with `%.17g`, always seventeen digits) | 0.176 s | 0.179 s | 0.777 s | 0.23 |
| `fields`: a million lines like ` Ada17 , 120, -45 ,6789` gone over twenty times with `lines`, `split`, `trim`, `long.parse` and `chars` | 1.228 s | 1.259 s | 1.202 s | 1.02 |
| `lines`: two million lines of a log (125 MB) read from the input with `Stdin.readLine`, each checked to be UTF-8, and counted | 0.075 s | 0.077 s | 0.091 s | 0.82 |
| `copy`: the same log read from a file with `File.openText` and written to another, each line with its number in front, through `File.createText` | 0.155 s | 0.160 s | 0.285 s | 0.54 |
| `bytes`: the same log read in pieces of 4 KB with `readBytes` and written with `writeBytes` through a writer from `File.openAppend` | 0.030 s | 0.030 s | 0.032 s | 0.94 |
| `table`: two million lines printed with widths, hex, binary and two digits after the point (`{i:x8} {a,10}  {name,-8}\|{x,10:F3}  {mask:b16} {code:X6}`) | 0.287 s | 0.296 s | 0.547 s | 0.52 |
| `blocks`: eight million pieces of 1 to 255 bytes (about a gigabyte) gathered with `addAll` into a `List<byte>` that is emptied at 64 MB | 0.035 s | 0.035 s | 0.035 s | 1.00 |
| `keywords`: a million words sorted forty times into twelve keywords and the rest by a `match` on text | 0.199 s | 0.200 s | 0.194 s | 1.03 |
| `classes`: ten million bytes sorted twenty times into the classes of a lexer by a `match` of ranges and `or` (`'a'..='z' or 'A'..='Z' or '_'`) | 0.239 s | 0.299 s | 0.274 s | 0.87 |
| `rewrite`: two hundred thousand lines gone over twenty times, the tabs replaced by four spaces and a word by another with `replaceAll`, and an indent made with `repeat` | 0.588 s | 0.597 s | 0.625 s | 0.94 |
| `tree`: the design's folder tree, 111,111 `shared checked` folders that look at their parent with `weak`, each adding a size to every folder above it, built and freed twenty times | 0.095 s | 0.102 s | 0.062 s | 1.53 |
| `tasks`: four tasks at a time over the numbers below 400,000, ten times; each adds up Collatz lengths and adds them to a `shared locked` total every 100 numbers | 0.115 s | 0.188 s | 0.115 s | 1.00 |
| `async`: for each number below a million, an async function starts a task for one Collatz length and awaits another in place; nothing has to wait | 0.244 s | 0.372 s | 0.226 s | 1.08 |
| `asynclines`: `lines` the async way, the two million lines read with `await Stdin.readLineAsync` in an async `main` (the C is that of `lines`) | 0.113 s | 0.114 s | 0.088 s | 1.28 |
| `tasklist`: in each of 200,000 rounds, eight async tasks for Collatz lengths taken with `Task.whenAll` and eight with `Task.whenAny`; none has to wait (the C keeps each result in a block it allocates, in an array for the list) | 0.476 s | 0.689 s | 0.384 s | 1.24 |
| `asynccopy`: `copy` the async way, the log read with `await reader.readLineAsync` and written with `await writer.writeLineAsync` in an async `main` (the C is that of `copy`) | 0.283 s | 0.284 s | 0.271 s | 1.04 |
| `echo`: an echo server and eight clients in one async program over the loopback, each client sending 64 bytes and waiting for them to come back 20,000 times (the C blocks, with a thread for each connection and each client) | 1.004 s | 1.009 s | 1.129 s | 0.89 |
| `echosync`: `echo` without async, the server's connections and the clients on threads of `Task.run` (the C is that of `echo`) | 1.108 s | 1.118 s | 1.115 s | 0.99 |
| `echowork`: `echo` with work in it, the server going through 300,000 steps of arithmetic for each message before it sends it back, and each client sending 300 (the C is that of `echo`, with the same work) | 0.376 s | 0.372 s | 0.369 s | 1.02 |
| `netlines`: the two million lines of `lines` sent over the loopback by one task and read line by line by another with `await reader.readLineAsync(ref line)` (the C blocks, with `getline` from a `FILE` over the socket, which checks nothing) | 0.092 s | 0.089 s | 0.081 s | 1.14 |
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

Measured on Windows (`python3 bench/run.py`, the hundred-and-second slice): Windows 11 on 64-bit Arm in a
virtual machine on the Mac of the table above, Clang 23 with C's library in the program (`/MT`, as `sg build`
links), best of five after a run that is not timed, which also leaves out the first start of a new program
(which on Windows took seconds now and then; why was not looked into); `spawn`, `spawnasync`, `capture`,
`pipelines` and `pipesasync` are one timed run each (`--runs 1`), since five thousand starts take over twenty
seconds there. The last column is the same ratio on macOS, from the table above or from the program's
choice. Windows on x86-64 is not measured: there is no such machine here, and the machines of the CI are
shared and give no steady numbers.

| Program | SG, release | SG, overflow checked | C | release / C | on macOS |
|---|---|---|---|---|---|
| `fib` | 0.276 s | 0.393 s | 0.273 s | 1.01 | 0.98 |
| `primes` | 0.488 s | 0.475 s | 0.471 s | 1.04 | 0.99 |
| `collatz` | 0.339 s | 0.540 s | 0.340 s | 1.00 | 1.00 |
| `words` | 1.155 s | 1.094 s | 1.100 s | 1.05 | 1.02 |
| `list` | 0.433 s | 0.444 s | 0.431 s | 1.00 | 1.01 |
| `expr` | 0.342 s | 0.346 s | 0.338 s | 1.01 | 1.02 |
| `parse` | 0.286 s | 0.315 s | 0.284 s | 1.01 | 0.99 |
| `vector` | 2.117 s | 2.371 s | 2.114 s | 1.00 | 1.02 |
| `counts` | 0.408 s | 0.404 s | 0.343 s | 1.19 | 1.20 |
| `show` | 2.876 s | 3.025 s | 8.363 s | 0.34 | 0.47 |
| `lookup` | 0.501 s | 0.488 s | 0.483 s | 1.04 | 1.01 |
| `buffer` | 0.287 s | 0.300 s | 0.287 s | 1.00 | 0.99 |
| `bits` | 0.339 s | 0.303 s | 0.369 s | 0.92 | 0.79 |
| `hash` | 0.132 s | 0.134 s | 0.131 s | 1.01 | 1.00 |
| `nbody` | 0.669 s | 0.669 s | 0.673 s | 0.99 | 0.97 |
| `numbers` | 0.222 s | 0.225 s | 0.498 s | 0.45 | 0.95 |
| `floats` | 0.182 s | 0.195 s | 0.735 s | 0.25 | 0.23 |
| `fields` | 1.349 s | 1.358 s | 1.487 s | 0.91 | 1.02 |
| `lines` | 0.072 s | 0.098 s | 0.454 s | 0.16 | 0.82 |
| `copy` | 0.160 s | 0.187 s | 0.910 s | 0.18 | 0.54 |
| `bytes` | 0.061 s | 0.059 s | 0.064 s | 0.95 | 0.94 |
| `table` | 2.524 s | 2.476 s | 3.325 s | 0.76 | 0.52 |
| `blocks` | 0.107 s | 0.120 s | 0.117 s | 0.91 | 1.00 |
| `keywords` | 0.233 s | 0.234 s | 0.216 s | 1.08 | 1.03 |
| `classes` | 0.209 s | 0.269 s | 0.234 s | 0.89 | 0.87 |
| `rewrite` | 0.575 s | 0.625 s | 0.720 s | 0.80 | 0.94 |
| `tree` | 0.194 s | 0.244 s | 0.114 s | 1.71 | 1.53 |
| `tasks` | 0.121 s | 0.189 s | 0.123 s | 0.99 | 1.00 |
| `async` | 0.236 s | 0.354 s | 0.208 s | 1.14 | 1.08 |
| `asynclines` | 0.153 s | 0.171 s | 0.458 s | 0.33 | 1.28 |
| `tasklist` | 0.534 s | 0.746 s | 0.445 s | 1.20 | 1.24 |
| `asynccopy` | 0.384 s | 0.434 s | 0.920 s | 0.42 | 1.04 |
| `echo` | 0.861 s | 0.850 s | 0.769 s | 1.12 | 0.89 |
| `echosync` | 0.873 s | 0.778 s | 0.769 s | 1.13 | 0.99 |
| `echowork` | 0.456 s | 0.457 s | 0.551 s | 0.83 | 1.02 |
| `netlines` | 0.096 s | 0.132 s | 0.111 s | 0.86 | 1.14 |
| `ring` | 0.343 s | 0.342 s | 0.343 s | 1.00 | 1.00 |
| `crc` | 2.800 s | 2.798 s | 2.798 s | 1.00 | 0.99 |
| `deflate` | 1.312 s | 1.311 s | 1.279 s | 1.03 | 1.04 |
| `clock` | 0.237 s | 0.228 s | 0.236 s | 1.01 | 1.01 |
| `dates` | 0.123 s | 0.303 s | 0.106 s | 1.16 | 0.29 |
| `rng` | 0.115 s | 0.115 s | 0.115 s | 1.00 | 1.00 |
| `secure` | 0.137 s | 0.137 s | 0.139 s | 0.98 | 1.00 |
| `listing` | 0.316 s | 0.316 s | 0.276 s | 1.14 | 0.99 |
| `status` | 11.688 s | 11.795 s | 15.677 s | 0.75 | 1.00 |
| `spawn` | 22.829 s | 22.798 s | 22.761 s | 1.00 | 0.70 |
| `capture` | 1.096 s | 1.279 s | 1.337 s | 0.82 | 1.49 |
| `pipelines` | 0.091 s | 0.113 s | 0.502 s | 0.18 | 0.85 |
| `pipesasync` | 0.153 s | 0.166 s | 0.507 s | 0.30 | 1.03 |
| `spawnasync` | 22.644 s | 23.007 s | 22.968 s | 0.99 | 0.71 |
| `case` | 0.176 s | 0.179 s | 5.273 s | 0.03 | 0.23 |
| `letters` | 0.295 s | 0.376 s | 4.692 s | 0.06 | 0.10 |
| `udpecho` | 1.089 s | 1.080 s | 1.365 s | 0.80 | 1.19 |
| `udpechosync` | 1.299 s | 1.306 s | 1.284 s | 1.01 | 1.00 |
| `inline` | 1.015 s | 1.012 s | 1.012 s | 1.00 | 1.03 |
| `callback` | 0.914 s | 0.925 s | 0.913 s | 1.00 | 1.00 |
| `sqlite` | 0.300 s | 0.301 s | 0.301 s | 1.00 | 1.00 |
| `curl` | 0.366 s | 0.367 s | 0.365 s | 1.00 | 1.02 |
| `signals` | 0.008 s | 0.007 s | 0.246 s | 0.03 | 1.27 |
| `bitfields` | 0.371 s | 0.335 s | 0.352 s | 1.05 | 0.91 |
| `atomics` | 0.226 s | 0.212 s | 0.260 s | 0.87 | 1.02 |
| `queue` | 0.247 s | 0.250 s | 0.255 s | 0.97 | 1.39 |
| `library` | 0.582 s | 0.576 s | 0.568 s | 1.02 | 1.01 |
| `kernel` (size) | 5168 B | | 2236 B | 2.31 | 2.31 |

The C is the same program wherever it can be. Where POSIX has a function Windows has not, the C takes
Windows' own (`bench/windows/posix.h`): threads of `_beginthreadex` and locks of `SRWLOCK`, as SG's runtime
has them there; Winsock; `getline` written over `fgets`, and the input read as bytes; `popen` through
`cmd.exe`. `clock` reads `QueryPerformanceCounter`, `secure` calls `BCryptGenRandom`, `listing` lists with
`FindFirstFileExW`, `spawn` and `capture` start `CreateProcessW`, `netlines` cuts its lines out of what
`recv` gives (Windows' C library cannot open a `FILE` over a socket), and `signals` is C's `signal` and
`raise` with `SIGTERM` and a pipe of C's library, since Windows has no `SIGUSR1`. `dates` takes the moments
apart by hand (Howard Hinnant's `civil_from_days`), since `gmtime_s` refuses a moment before 1970: so on
Windows it is the same algorithm in both, where on macOS it is the C library's `gmtime_r`. Windows has no
`cat` and no `true`, so `run.py` builds both from `bench/windows` and puts them first on `PATH`, for SG and
C alike. `case` is compared by the size of its text only: its C maps a character to one by the tables of
the C library, and Windows' writes three bytes more each line.

Most of the rest is the time of C, as on macOS. Where SG is much faster, the difference is Windows' C
library: `printf` (`show`, `numbers`, `floats`), `fgets` (`lines`, `copy`, `asynccopy`, `pipelines`,
`pipesasync`) and the functions of wide characters (`case`, `letters`) are several times slower there than
on macOS, and SG uses none of them. `signals` is 0.03 because `Signals.raise` on Windows hands the signal on
inside the program, where C's `raise` calls the handler, which writes into a pipe. Where SG is slower:
`tree`, `counts`, `async` and `tasklist` as on macOS; `dates`, `listing`, `echo` and `echosync` on Windows
only, in "Slower than C" below.

A release build runs the three integer programs in the time of C. The "overflow checked" column is what that
check alone costs, and why the rule became Rust's on 12 November 2026. `words` is the first program with
memory: the few percent are what the checks that stay cost (every index and every range is checked, and a
range may not cut a character in half), against C that checks nothing. The first version of that C program
looked 20% faster: the C compiler had removed its `malloc` and `free` because the copies were never read; the
program now compares the copies. `list` follows pointers: a `Node*?` is one pointer, `is not null` is one
comparison, and there is nothing else to pay for. Its first version took 1.10 times the time of C, because
freeing a node called a function for its (already empty) `next`; the loop that frees a chain now drops the
other fields itself. `expr` is the first program with an enum: a `match` is a test on one byte, and the C it
is compared with is a tagged union and a `switch`. The part that builds, rewrites and frees the tree first took
1.13 times the time of C: after a value was moved, the drop of the (empty) variable was still written, and the
C compiler did not remove it. Those drops are no longer written (choice 24) and that part takes 1.03 times.
`parse` is the first program with errors as values: eighteen million lines are read, a third of them end in
an error that is found three calls deep, handed up with `try` and looked at with `match` or dropped with
`??`. The C it is compared with returns an error code and gives the value through a pointer. The two take the
same time (0.99 to 1.02 over several runs): a `Result` is a tag and a value, `try` is one test, and nothing
is allocated.
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
`counts` is the first program with a hash table, and the first that is clearly slower than its C: 1.20
times. All of it is the hash. The C program hashes a key with one multiplication, as a C programmer does;
SG uses SipHash-1-3 with a key drawn when the program starts, as Rust does, so that someone who chooses
the keys (a request, a file) cannot make them all land in one place and turn every lookup into a walk
through the whole table. With the C program's hash put into the generated code the two take the same time
(0.316 s against 0.319 s): the table itself costs nothing. The safe hash stays the default (choice 50).
`show` prints values: a struct with two numbers and a list of eight. Before the twenty-fifth slice it took
1.22 times the time of C (0.44 s against 0.36 s): every printed value was written into a text that was
allocated and freed. It now goes into one block that the program keeps (and gives back when one line
made it larger than 64 KB), and a number inside a value is
turned into digits by a few lines of the runtime instead of `printf`. The C it is compared with is what a C
programmer writes first, a `printf` with a format, and `printf` reads that format again on every call; that
is the whole difference, not something C cannot do. The honest reading is "printing a struct costs no more
than a `printf`", not "twice as fast as C".
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
`nbody` is the first program with fractional numbers: five bodies in a `Body[5]` on the stack, ten
pairs at each step, a `sqrt` for each pair. The two take the same time (0.97 here; 1.00 when timed by
hand before): a `double` is C's `double`, `sqrt` is one instruction, and nothing on a fractional number
is checked, so the overflow-checked build takes the same time too. Every `bodies[i]` is checked against
the five; the C compiler sees that `i` and `j` stay below five and removes those checks. Both programs
are compiled without fused multiply-add, the rule for SG (choice 142): with it, clang on arm64 fuses
`dx * dx + dy * dy` and the C program prints other last digits than the interpreter. Both print the same
two energies, digit for digit.
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

`floats` writes fractional numbers: a million, every other one with all seventeen digits
(`512.0471906447226`) and the others with a few (`1234.567`), each written into one text with `$"{x}\n"`,
three times over; the last text is then read back and added up, which shows that each number read back
as itself (both programs print the same sum). The C program writes each number with `%.17g`, the usual way
in C to write a number that reads back: `printf` cannot give the fewest digits, so it always gives
seventeen. SG finds the fewest with Dragonbox (497), one or two multiplications by a power of ten from a
table; until the sixty-eighth slice it wrote each number with 1, 2, ... 17 digits and read each back,
and took 4.627 s, six times the time of C.

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

`lines` reads its input: two million lines of a log, 125 MB with some `\r\n` ends and a `café` here
and there, read with `Stdin.readLine` into one text that is used for every line, and counted (the
lines, their bytes, the longest, the ones that start with `ERROR`). The C program reads them with
`getline` into one buffer, as one does. SG takes 0.82 times its time, and checks that every line is
UTF-8, which `getline` does not: it finds a line with `memchr` in a block of its own where `getline` goes
through `FILE*`, and it checks the block at once, so the lines inside the part known to be text cost
nothing more (choice 491). The check first took the program to about 1.15 times; looking at eight bytes
at a time while they are ASCII brought it to 1.08 (choice 202), and checking a block at a time to 0.82.

`copy` reads the same log from a file, opened with `File.openText` and read with `readLine` into one
text, and writes each line with its number in front (`17: INFO 350 ...`) with `writeLine` through a
writer from `File.createText`, 140 MB in all. The C program reads with `getline` and writes with
`fprintf` into a `FILE*` with a buffer of 64 KB. SG takes 0.54 times its time (0.155 s against 0.285 s;
0.93 before the check of a block at a time, 0.83 before the runtime wrote its lines itself, choice 1336): it
checks every line and still reads faster, as `lines` showed, and the writing is the runtime's own few
conversions into the writer's block, where C's `fprintf` reads its format on every line.

`bytes` copies the same log as bytes: `readBytes` into a buffer of 4 KB on the stack, and `writeBytes` of
each piece into a writer from `File.openAppend`, after the file is emptied. The C program does the same
with `fread` and `fwrite` on `FILE*`s with buffers of 64 KB. SG takes 0.94 times its time (0.030 s against
0.032 s, the best of fifteen runs: the program is short, and one run of five gave a cold start of 0.09 s).
Both are copies between blocks; neither looks at the bytes.
`table` prints two million lines, each one `print` of a formatted text with a width on two numbers and on a
name, hex with eight and with six digits, sixteen binary digits and a fractional number with three digits
after the point. The C program is one `printf` a line (`%08llx %10lld  %-8s|%10.3f  %s %06X`), with the
binary digits put together by a small function first, since `printf` has no conversion for them. SG takes
0.52 times its time (0.89 before the runtime wrote its lines itself, choice 1336). Its first version took 1.30 times: the fractional number was written by `snprintf`
and then copied by `printf` (the format read twice), and the spaces of a width were a conversion of their
own beside the value. Now the digits after the point are worked out by a function of the program, exactly
(choice 232), and a width is one conversion; `printf`'s own `%10.3f` is the slower part of the C program.
`blocks` gathers bytes as a GIF decoder gathers the data of an image, which comes in sub-blocks of at most
255 bytes: a range of an array on the stack is added with `addAll` to a `List<byte>`, eight million times.
The C program keeps the same block, length and capacity by hand and adds each piece with `memcpy`. The two
take the same time (0.035 s over three runs of fifteen): `addAll` is one test for room and one `memcpy`.
Adding the same bytes one at a time with `add` took ten times as long (0.344 s).
`keywords` sorts words as a lexer sorts the names it reads: a `match` on text with twelve keywords and `_`.
The C program is the chain a person writes by hand, one test of the length and one `memcmp` for each
keyword, and that is what the arms compile to (`sg_eq` is the same two tests); the million words are
copied to the heap first in both. 1.03 over seven runs is within what moves from run to run. A pattern
inside a pattern is not measured on its own: it compiles to the tests of the tags and the values joined
by `&&`, which is what `expr` already measures one level at a time.
`classes` sorts bytes as a lexer sorts the characters it reads: letters and `_`, digits, spaces, brackets
and operators, each class one arm of ranges and single bytes joined by `or`. The C program is the chain of
`if`s a person writes by hand, `c >= 'a' && c <= 'z'` and `c == '_'` joined by `||`, and that is what the
arms compile to: a range is two comparisons (one, when an end is the first or the last value of the type),
an `or` is `||`. The 0.87 is the C compiler's doing on two programs that say the same; the checked build pays
for the overflow test of the sum, not for the patterns.
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
The numbers move by a few percent from run to run, and a dozen small programs are not a benchmark suite.

Choices made here, approved on 12 November 2026:

1. **The back end writes C**, not machine code and not LLVM's form: the smallest path, every platform has a
   C compiler, and its optimiser does the heavy work.
2. **In a debug build an overflow check is the C compiler's own overflow builtin** and a branch to a panic
   that is marked as never the usual path. **In a release build a wrapping operation is written with unsigned
   arithmetic**, which is defined in C; the plain `a + b` of C would be undefined on overflow, and the C
   compiler would be free to assume it never happens.
3. **A compiled program panics with the same text as the interpreter**, the source line included; the lines
   are kept in the program as constants.
4. **Deep recursion is not caught in compiled code.** The interpreter counts calls; compiled code would pay
   for a count on every call, so the open "stack depth" question of the design stays open here.
5. **`sg build` and `sg emit-c`** are the commands, with `--release` for a release build; the C compiler is
   `cc`, with `-O0` for a debug build and `-O2` for a release build.
6. **A cast that does not fit (`(int)big`) follows the overflow rule**: it panics in a debug build and is
   cut to size in a release build (decided on 12 November 2026).

Choices made in the seventeenth slice (text and owned values), approved on 12 November 2026:

7. **A `read string` is a pointer and a length** passed by value; a `string` is a pointer, a length and a
   capacity. Every other `read T` / `ref T` is a plain pointer.
8. **A move empties the place it leaves** (the capacity is set to 0), and every owned variable is dropped
   where it ends. This is the simplest correct form: a variable moved in only one branch needs no extra
   bookkeeping. The C compiler removes most of these tests; where it cannot, a move costs one store and a
   drop one comparison.
9. **An owned value that has no name** (`greeting("ada", 1).length()`) is kept in a temporary for the
   statement and dropped after it.
10. **Allocation failure stops the program** (`abort`); there is no notation for handling it.
11. **The character-boundary check of a range stays in release builds.** By the criterion of 12 November it
    is a correctness check, not a memory-safety one, but it is not measurable here (0.690 s with it, 0.707 s
    without), so there is nothing to gain.

Choices made in the eighteenth slice (the heap and missing values), approved on 12 November 2026:

12. **A `T*?` is the pointer itself, and `null` is the null pointer**: no flag, no extra word (Rust does the
    same for `Option<Box<T>>`). Any other `T?` (`int?`, `string?`, a struct) is the value and a `bool` beside
    it. Costs nothing at run time and adds no rule to the language.
13. **A struct with exactly one pointer to its own kind (`Node*? next`) is freed in a loop**, link by link.
    Freeing a list of a million nodes does not go a million calls deep; in Rust the same list overflows the
    stack unless `Drop` is written by hand. Other shapes (a tree with two children) are freed by recursion,
    as deep as the tree is. The open "stack depth" question stays open for those.
14. **`try` and `?? return` are compiled only where nothing is left behind** (widened in the twentieth
    slice, see 29; until then only as the whole right side of a statement). Where a value that owns
    something was computed before them in the same statement (`g(a(), try b())` with `a()` a `string`) the
    back end stops with an error and the hint "give the value a name first". The reason: what `a()`
    returned would have to be freed on the way out, and that bookkeeping is not written yet. A limit of
    the back end for now, not a rule of the language; the interpreter runs both forms. Written in the
    twenty-eighth slice for arguments, fields and the values of an enum value (79).
15. **`??`, `try` and `replace` are written as GNU statement expressions**, so they run where they stand and
    the right side of `??` runs only when it is needed. clang and gcc both have them; a C compiler without
    them (MSVC) cannot build the output for now.
16. **A `read int` lent where a `read int?` is expected is not compiled yet** (a pointer to a number is not a
    pointer to a number and a flag; it needs a temporary). The same with a `T*?` works, since both are one
    pointer. It is compiled since the twenty-seventh slice, with nothing made for it (choice 76).
17. **`print(p)` with `int* p` writes the number**, as the interpreter does.

Two things in the checker were found while writing these tests and are fixed since:

- **A move on a right side of `??` that always leaves does not count afterwards.** After
  `var text = key ?? return list` the list is still owned: the next line runs only when the `return` did
  not. The same holds for `?? panic(...)`. A right side that goes on (`size ?? consume(list)`) still moves,
  since the checker cannot tell whether it ran (`tests/cases/error_move_in_fallback.sg`).
- **The built-in methods of numbers, `bool` and text are found through a `T*`**, as fields and the methods
  of a struct already were: `name.length()` with `string* name`, `count.wrappingAdd(1)` with `int* count`.
  The value is read where it is; nothing is copied or moved. On a pointer that has no name
  (`makeName().length()`) the compiled code frees it after the statement.

Choices made in the nineteenth slice (enums and `match`), approved on 12 November 2026:

18. **An enum is a tag of one byte and, beside it, room for the largest variant** (a C union). An enum whose
    variants carry nothing is the tag alone. This is what a C programmer writes by hand, and what Rust does
    without its further packing (Rust hides the tag in unused bit patterns where it can; that is not done
    here). No cost at run time beyond the tag.
19. **`match` is a chain of tests on the tag, not a C `switch`.** Inside a `switch` a `break` would leave the
    `switch`, and in SG `break` and `continue` in an arm mean the loop around the `match`. The C compiler
    turns the chain into the same jumps. An arm with `when` that does not hold goes on to the arms below.
20. **A `match` on an owned value takes it apart.** The names of the arm own what the variant carried, what
    the pattern skips (`_`) is freed as the arm starts, and a `T*` subject gives its block back. A `match` on
    a borrow only looks: the names are pointers into the value. Nothing is copied in either case.
21. **An enum that was moved out of its place gets a tag that is no variant**, so the later drop does
    nothing (one store, like the capacity of a moved `string`). The tag 255 is kept for that, which leaves
    254 variants.
22. **`match` is compiled as a statement or as the whole right side** (`var x = match ...`, `x = match ...`,
    `x += match ...`, `return match ...`, and as the last line of an arm). Inside a larger expression
    (`f(match ...)`) the back end stops with the hint "give the value a name first". The same limit as 14, and
    for the same reason; the interpreter runs both forms. Lifted in the twenty-eighth slice (78).
23. **A `when` condition only looks; it cannot move a value** (decided on 12 November 2026, a rule of the
    checker). If the condition moved a value and then did not hold, the arms below would run without it:
    `Say(text) when isLong(text)` with `bool isLong(string text)` is an error that says so and points at
    `read`. Rust has the same rule. With it a `when` is compiled on every arm, also one that takes an owned
    value out of an owned subject: the names are filled in for the condition and are the arm's only once it
    holds (`tests/cases/error_when_moves.sg`).
24. **The drop of a variable that was certainly moved is not written.** The checker marks every use that
    moves; where such a use is on a line that always runs, the back end leaves the drop out. Where it is not
    certain (moved in one branch, inside a loop, on the right of `??`, `&&` or `||`) the drop is written and
    finds the place empty, as before. Measured on the expression tree: building, rewriting and freeing it
    went from 1.13 to 1.03 times the time of C; the C compiler did not remove those empty drops by itself. `tests/cases/ok_moved_drops.sg` goes through the
    doubtful cases.
25. **The names of `is` and of a borrowing `match` need a value that stays**: on a value that has no name and
    owns something (`if make() is Say(text)`) the back end stopped and asked for a name. Since the
    twenty-fifth slice it keeps the value until the block is done (choice 57).

26. **The parts of an expression are computed from left to right in compiled code too** (asked for on 12
    November 2026). C leaves open in which order it computes the arguments of a call, the two sides of an
    operator and the values of an initialiser, and until now the generated C wrote them as one C expression.
    Where the order can be seen, the earlier parts are now kept in variables first. It can be seen when two
    parts can each stop the program or call a function (`f(next(), next())`, `a / b + c / d`), or when one
    part lends with `ref` what another reads (`f(n, bump(ref n))`). Everything else is written as before, so
    plain arithmetic and calls with plain arguments produce the same C: the measurements did not move. The
    values of a formatted text (`$"{a()} {b()}"`) follow the same rule. One case is refused instead: a text
    written before a value that lends with `ref` in the same `$"..."`, since the text is only looked at and
    could be gone by the time it is written. `tests/cases/ok_evaluation_order.sg` goes through the cases; all
    compiled test programs give the same output when built with gcc 16 instead of clang. An assignment is
    not an expression and has an order of its own: the right side first, then the place (choice 270).

## How it is built

| File | Role |
|---|---|
| `src/lexer.rs` | Tokens, the line-end rule, `$"..."` pieces |
| `src/parser.rs` | Hand-written recursive descent parser |
| `src/types.rs` | Types as the checker sees them |
| `src/check.rs` | Static checks: names, types, `null` safety, use after move (per variable and per field), `match` covering every variant, `return` on every path, unused `Result`, what `try` may pass on, what `Copyable` and `Formattable` promise. Writes resolved details (literal types, field indexes, implicit conversions, what each call runs) into the tree |
| `src/interp.rs` | Runs the checked tree; only data-dependent failures (panics) are left to it. Every task that runs code is a fiber, a thread of its own of which one runs at a time. The fields of a value of a `Disposable` type wait in a list when their last owner lets them go, and `dispose` runs on them between statements |
| `src/emit_c.rs` | The back end: writes the checked tree as C, with the same panics; tasks are threads of the system |
| `src/opening.rs` | After the checker: where a `shared checked` value is reached and for how long it is borrowed (the `Open` and `Within` nodes the interpreter follows), the conflicts through the same variable that are certain before the program runs, and the borrows that would outlive their line |
| `src/waits.rs` | After the checker, on the whole program: which functions wait (take a lock, wait for a task or await one, themselves or through the functions they call), and what waits inside a `lock` block; which functions hold the thread, and what does in an async function; an async function that awaits itself |
| `src/unread.rs` | Last of the checks: the local variables whose value nothing reads, from what the checker noted each name stands for, telling a read from a write along the checked tree of each function and test |
| `src/ffi.rs` | For the interpreter: the library of wrappers for the functions of C a program calls, built with `cc` in a folder of its own and opened with `dlopen` |
| `src/raw.rs` | For the interpreter: raw memory, real addresses from `calloc`, each checked against the block it came from |
| `src/sums.rs` | For the back end: the sums in a loop that cannot overflow for a number of rounds, so that a checked build runs those rounds without their test |
| `src/unicode.rs`, `src/unicode_tables.rs` | What a character is and its case: the tables of Unicode 17.0, one record for each code point found in three steps, read by the interpreter and written into compiled code as C when a program calls a method that needs them. `tools/unicode_tables.py` makes the tables from the Unicode Character Database (each file checked against its SHA-256) and checks every code point against Python's tables of the same version |
| `src/diag.rs` | Error and panic messages |
| `src/source.rs` | The files of a program: which folders are modules and which of them are read. All files are kept as one text, so a position says which file it is in; the language's own module comes last, when a file names one of its types |
| `src/std/time.sg` | The language's own module `std`, written in SG: `Duration`, `Instant`, `DateTime`, `DateTimeOffset`, the calendar and the reading of RFC 3339. Only it calls the readings of the clock and of the time zone, `Clock.monotonic()`, `Clock.wall()` and `Clock.offsetAt(seconds)` |
| `src/std/random.sg` | The same module's random numbers: `Random` (xoshiro256\*\* seeded with SplitMix64) and `SecureRandom`. Only it calls `Entropy.fill`, the random bytes of the system. A program gets each file of `std` when it names one of its types. A panic inside the module is shown at the program's call that led there (`Interp::call`; in compiled code `Emitter::tracked`) |
| `src/std/files.sg`, `src/std/path.sg`, `src/std/environment.sg` | The same module's folders (`Directory`, `DirectoryEntry`, `FileInfo`, `FileKind`, and the work of `File.info` and `File.exists`), paths (`Path`) and variables of the environment (`Environment`, `VariableError`). Only they call `System.list`, `System.status` and the other calls of the system; a file of `std` that names a type of another brings it along (a `FileInfo` holds a `DateTime`) |
| `src/std/process.sg` | The same module's other programs (`Command`, `Process`, `ExitStatus`, `ProcessError`): the search in `PATH`, the checks of what the system cannot take, the pipes taken out of a running program, its stop when it is not waited for, and the reading of how a program ended, over `System.run`, `System.start`, `System.wait`, `System.waitAsync`, `System.signal`, `System.reader`, `System.writer` and `System.executable` |
| `src/std/net.sg` | The same module's datagrams and addresses (`UdpSocket`, `SocketAddress`, `Received`): reading and printing a v4 and a v6 address, and the two ends of a `TcpStream`. Only it calls the sockets of the system (`System.udpBind`, `System.udpSend`, `System.udpReceive`, `System.lookup` and the others) |
| `src/std/signals.sg` | The same module's signals (`Signal`, `Signals`, `SignalListener`). Only it calls `System.signalListen`, `System.signalNext` and the other calls of the signals |
| `src/signals.rs` | For the interpreter: the signals of the process, a handler that writes the kind into a pipe and a thread that hands each to its listeners, waking the fiber that waits through the scheduler |
| `src/lsp.rs` | The language server (`sg lsp`): the messages of the Language Server Protocol on the input and output, the program each open file belongs to, a check 300 ms after the last change on a thread of its own, the first error as a diagnostic and its fix as a quick fix, positions in UTF-16 as the protocol counts them |
| `src/json.rs` | JSON read and written for the language server, so that `sg` keeps no dependencies |
| `src/lib.rs`, `src/main.rs` | Library entry point and the command line |

It is a library with a thin command line on top, so that the language server (`sg lsp`) uses the same parser
and checks as the command. An error carries its hint as text and, where the code it gives is certain, as an
edit (`Diag::fix`), which an editor applies with one click.

The command's exit codes, tested from outside in `tests/cli.rs` (1322):

| Code | When |
|---|---|
| 0 | All went well: the program ended, every test passed, the build was made |
| what `exit` gives | The program called `exit(code)`, a `byte` |
| 1 | An error the checker or the C compiler finds, a panic, a failed test, a program that leaks or races |
| 2 | A command line that is not one of the usages (a second program, a flag given twice, `-o` or `--target` with nothing after it, `-o` with `emit-c`), a folder of `-L` that is not there, a program that cannot be read (no such file, not UTF-8, a folder with no `.sg` file), an output into a folder that is not there |
| 0 or 1, `sg lsp` | The language server ends with 0 when the editor asked it to (`shutdown`, then `exit`), and with 1 when its input ends or it is told `exit` without `shutdown`, as the protocol has it |
| `SIGPIPE` | On Linux and macOS, an output closed before the program was through (`sg x.sg \| head`), interpreted and compiled, as C's and Go's programs end (1330); on Windows that is 0 |

Tests are golden files: every `.sg` file in `examples/` and `tests/cases/` is run and its output (what it
printed, then the error or panic message) is compared with the `.out` file next to it. After an intended change,
regenerate them with `UPDATE_GOLDEN=1 cargo test` and review the diff. What the program wrote to the standard
error comes after what it printed, and an exit code other than 0 and 1 is a last line (`exit code 3`). A
`.args` file next to a program gives its command line, one word on each line, where `{dir}` is an empty folder
of its own to write files into (it is written back as `{dir}` in the output); a `.in` file gives its input.

Programs with modules are in `tests/modules/`: the folders there (`stock`, `billing`, `shop/parts`, `leaky`,
`links`, `aliasleak`, `clib`, `timing`) are modules, and each `.sg` file next to them is a program of one file that uses them. A folder with a `main.sg`
(`tests/modules/rootdir`, `examples/shop`, `examples/zlib`) is a program run as a folder; its output is in `main.out` and what
the test command prints for it in `tests.out`.

Two kinds of trying are not part of `cargo test`. The front end is fuzzed (`fuzz/` at the root of the
repository, a crate of its own with its own `README.md`): on any text, the parser, and all `sg build` does
before the C compiler runs, must end with a result or a message, never a panic, a hang or a stack run out
(1349-1355). And mutants are tried on the functions that make the rules of safety, named in
`.cargo/mutants.toml` with the commands that run them (1356-1360): a mutant the tests do not notice is a
test that is missing. A change to those functions is tried where it touched them with `git diff | cargo
mutants --in-diff /dev/stdin`, with the arguments `.cargo/mutants.toml` gives.

## Grammar rules

The design left the semicolon-free grammar open. These rules were chosen here and approved on 12 November 2026;
each has a test in `tests/cases/`.

1. **Line-end rule.** A line end terminates the statement. Exceptions: inside `(` or `[`; when the line ends with
   an operator, a comma, a dot or an opening bracket; and when the next line starts with a dot, so a method chain
   can be written one call per line.
2. **A line cannot start with an operator.** `+ 2` on its own line is an error with a hint to move the operator
   to the end of the previous line.
3. **`{` and `else` stay on the same line** (`if x {`, `} else {`). A new line is an error with a hint.
4. **Only a call, an assignment or a declaration can be a statement** (C#'s rule). At the start of a statement,
   a name, any `*` and `?`, then another name is a declaration: `int x = 1`, `Node*? list = null`.

## Rules from the second slice

Approved on 12 November 2026.

1. **A struct literal in a condition needs parentheses.** In `if x {`, `while x {` and `for i in a..b {` the `{`
   starts the block, so `if p == Point { x = 1 } {` is not read as a literal; write `if p == (Point { x = 1 }) {`.
   This is Rust's rule, and the design notes already named it as the likely answer.
2. **Assigning to a `ref` variable writes to what it borrows** (`void clear(ref int? slot) { slot = null }`), as
   in C#. A `read` variable is rebound instead (`cur = read cur.next`).
3. **Looking is not taking.** Operands of operators, the receiver of a method, and the arguments of `print`,
   `assert` and `$"..."` are borrowed for the moment, so `print(name)` does not move `name`.

## Rules from the third slice

Approved on 12 November 2026.

1. **A struct cannot define a method named `replace`**, because `x.replace(value)` works on every type. `take`
   can be defined: the built-in one only exists on `T?`, and a method is never called on a `T?`.
2. **A method and a field of the same struct cannot share a name**, and a function cannot have the name of a
   type (`Stack(...)` always means `Stack.create(...)`).
3. **In `Name.f()` a variable called `Name` wins over a type called `Name`.**
4. **`take` on a variable that was known to hold a value ends that knowledge**: after `node.take()` the
   variable is a `T?` again.
5. **A `ref this` method can be called on a value that was just built** (`Stack().push(1)`), as in Rust.
6. **A `this` method called through a `T*` uses the pointer up.**

Smaller placeholders: a panic exits with code 1; a variable may shadow one from an outer block but not one in
the same block (since the ninety-second slice it may not shadow one from an outer block either: 886).

## Rules from the fourth slice

Approved on 12 November 2026. Each has a test in `tests/cases/`.

1. **`x is Circle(r)` only looks.** It never moves `x`; the names are `read` borrows, or `ref` borrows when
   `x` is a `ref` borrow or the test is written `ref x is Circle(r)`. To take the values out, use `match`. The
   design only said that `is` is the counterpart of Rust's `if let`, which moves; here `is null` already only
   looks, and a test that uses its subject up would surprise.
2. **A pattern names or skips every value**: `Rect(w, _)`, never a bare `Rect` for a variant that carries
   data. One way to write it, and adding a field to a variant shows every pattern that needs a look.
3. **An arm that can never run is an error** (the variant is handled above, or an arm above takes every
   value). Rust warns; there are no warnings here.
4. **In a `match` used as a statement an arm follows the statement rule**: a call, an assignment, `return` /
   `break` / `continue`, or a block. `Circle(r) => r * 2` is "the value is not used". `Empty => {}` does
   nothing.
5. **Variants and arms are separated by line ends, never commas**, as the design examples write them.
6. **`match` and `is` look through `T*`**, like field access. `var other` gets the value as it was given
   (the `T*` itself).

## Rules from the fifth slice

Approved on 12 November 2026. Each has a test in `tests/cases/`.

1. **`Ok(...)` and `Error(...)` take their type from where the value goes**, like `null`: a `return`, a
   declaration with a type, an argument, a field. `var r = Ok(1)` is an error, because nothing says what the
   error type is. Rust infers it from later lines; here a line is read on its own.
2. **`Result`, `Ok` and `Error` are taken names**: no type called `Result`, no function called `Ok` or `Error`.
   A variant of your own enum can still be called `Ok` (`Status.Ok`).
3. **"A `Result` in a variable that is never unwrapped" is checked as "never looked at".** Any read of the
   variable counts (`try r`, `r ?? 0`, `match r`, `r is Ok(v)`, passing it on). Exact for the common mistake,
   and no flow analysis.
4. **`_ = e` drops any value, not only a `Result`.** `_ = print("x")` is an error: there is nothing to drop.
5. **`??` on a `Result<void, E>` is an error**: success carries nothing, so there is nothing to fall back to.
   The hint names `try` and `_ = ...`.
6. **`tryDivide` gives `null` for division by zero too**, not only for overflow (Rust's `checked_div`).
7. **`return` inside an expression is written only on the right side of `??`.**

Placeholders: `try` on a borrowed `Result` works only if it can be
copied. A `Result` cannot carry a borrow (`Result<read string, E>`), since `from` is not here.

## Rules from the sixth slice

Approved on 12 November 2026. Each has a test in `tests/cases/`.

1. **An operation happens in the type of its operands.** `byte + byte` is a `byte`, and `200 + 100` in a `byte`
   panics like any other overflow (Rust). C# computes in `int` instead, which is one more rule to know. A `byte`
   widens to `int`, `long` and `nint` on its own; the way back is written `(byte)x`.
2. **A character is a `byte` and nothing more.** `int x = '+'` is 43 through the ordinary widening, and
   `print('+')` writes `43`, as C# does for a `byte`. Writing it as a character waits for `char`.
   (Since the thirty-sixth slice a character is a `char` where no integer is expected: `print('+')` writes
   `+`; where an integer goes it is still a `byte`, 167.)
3. **`-b` on a `byte` is an error**, since the result could only be 0 (Rust). The hint is `-(int)b`.
4. **An index and the ends of a range take any integer that widens to `nint`**: a `byte`, an `int`, a `nint`.
   A `long` is converted with `(nint)`.
5. **A range that ends before it starts panics** (`text[3..1]`), it is not an empty text (Rust).
6. **In a `match` on a number the patterns are numbers and characters, `_` and `var name`.** Listing all 256
   values of a `byte` still needs `_`: the check does not count. The same number twice, or a number that does
   not fit the type, is an error. (Since the forty-fourth slice the check counts, 255: arms that take every
   value need no `_`.)
7. **`\'` is an escape in both kinds of quotes**, and neither quote has to be escaped inside the other:
   `'"'`, `"'"`.

8. **A range is written bare**: `text[a..b]` is a `read string` on its own, and `read text[a..b]` is an error
   whose hint is the bare form. One way to write it. `read items[0]` is something else and stays: that lends
   one element.

## Rules from the seventh slice

Approved on 12 November 2026. Each has a test in `tests/cases/`.

1. **`Copyable` and `Formattable` exist as built-in names.** `interface` of your own, `where`, `Clonable` and
   `Disposable` answer "not in the interpreter yet". (`Equatable`, `Hashable` and `Comparable` came the same
   way in the tenth slice.)
2. **The text the compiler makes** (the design says it is generated, not what it looks like): a struct is
   written the way it is built, `Place { line = 3, file = "a.txt" }`; a variant the way a pattern writes it,
   without its enum, `NotFound(...)`, `Ready`; text inside another value is in quotes, so the parts can be told
   apart; a missing value is `null`; a `T*` is written as what it owns.
3. **`print` of a `T?` or a `Result` is an error**, with the hint `??` or `try`: at the top level a silent
   `null` in the output is more likely a mistake than a wish. Inside a generated text `null` is written.
4. **A struct that writes its own `format` may hold fields that are not `Formattable`.** Only the generated
   body needs them all.
5. **A method that takes `this` by value copies a `Copyable` receiver**, so the value stays usable, also
   through a borrow. This is the argument rule applied to the receiver.
6. **`Formatter` is a taken type name**, and `f.write(x)` takes whatever `print` takes. Write errors that stay
   in the `Formatter` (the design's reason for it) wait for files.
7. **The error `main` ends with** is now written by `Formattable`: `error: Syntax(NumberTooLarge(0))` where
   the placeholder wrote `AppError.Syntax(SyntaxError.NumberTooLarge(0))`. A `string` error is still its text.

The calculator runs whole (`examples/calculator.sg`): lexer, parser, evaluator, error conversion. Since the
fifteenth slice the lexer and the tokens borrow the input instead of copying it (`struct Lexer<from source>`,
`Name(read string text)`), as in the design. What still differs: `Token` is not `Copyable`, so `advance` hands
over the current token with `replace`; `-x` is `0.trySubtract(x)`; and the input is a string in the program
instead of a file. The variables are a `HashMap<string, int>`.

`examples/log_summary.sg` is the log summary from the design without the files: a `List` of lines, a
`HashMap` filled with `getOrAdd`, and a `for` over both. Since the thirty-sixth slice a line is parsed as
the design parses it, with `split`, `try parts.next()` and `int.parse`, into an `Entry<from line>` (179).

## Rules from the eighth slice

Approved on 12 November 2026. Each has a test in `tests/cases/`.

1. **`List<T>` and `HashMap<K, V>` are built in, like `Result<T, E>`.** There are still no generics of your own.
   The methods are the ones the design examples call (the tables above); `reserve` and `clone` wait for `Clonable`.
   (`filter` and `map` left the design with lambdas on 12 November 2026.)
   2. **`xs[i]` lends what cannot be copied.** The design says reading `xs[i]` is `xs.at(i)`, which returns
   `read T`: so `names[0]` is a `read string`, and an `int` item is simply the number. To take an item out there
   is `removeLast()`, `xs[i].replace(value)`, or `.clone()` for a copy.
3. **`removeLast()` gives a `T?`**, `null` for an empty list (Rust's `pop`). The design uses the name once and
   does not say what comes back.
4. **`map.add(key, value)` panics if the key is already there** (C#'s `Add`), also inside
   `HashMap<K, V> { ... }`. Rust's `insert` overwrites silently and gives the old value back.
5. **`map[key] = value` writes only under a key that is there** and panics otherwise, because the design says
   writing `xs[i]` is `refAt`. With rule 4 there is no single way to say "put or overwrite" (in C#
   `map[key] = value` does that): it is written with `contains`, or with `getOrAdd`: a key that was
   expected and is missing should not appear silently.
6. **A method that only looks a key up takes text as `read string`** (`ages.get(read name)`, a literal as it
   is) and a number or a `bool` by value. `add` takes the key itself, since the map keeps it.
7. **`for item in map` gives an entry with `key` and `value`.** The value can be written through `ref`
   (`item.value += 1`), the key never. The entry has no type name that can be written yet. Here the entries come
   in the order of their keys so that a run always prints the same; the design promises no order.
   (Changed in the twenty-fourth slice: a map and a set are hash tables here too, see choice 48.)
8. **A key is an integer, a `bool` or a `string`**; since the tenth slice also a type written
   `: Equatable, Hashable`.
9. **A collection is written as text if its items are**: `print(xs)` writes `[1, 2, 3]`, a map
   `{"ada": 36, "grace": 45}`, a set `{1, 2, 3}`; text inside is in quotes, as inside a struct. A struct that
   holds a list can be `Formattable` without a `format` of its own. (The first choice here was an error; it was
   turned around on approval: one rule less, and it is what is wanted while looking for a mistake.)
10. **`List<int>(` and `List<int> {` start a value**, so `a < b > (c)` is read as a type too (C#'s rule). In a
    condition, where `{` starts the block, `List<int> { 1 }` needs parentheses like a struct literal.
11. **`x ?? fallback` on a borrowed `T?` that can be copied gives the value**, so `xs.get(i) ?? 0` is an `int`.
    A missing `T` where a `T?` was found now has the hint `??` / `is null`.

## Rules from the ninth slice

Approved on 12 November 2026. Each has a test in `tests/cases/`.

1. **A collection is not changed inside its own `for`** (the design already said so). This is the
   narrow form that needs no borrow check: while `for x in read xs` or `for x in ref xs` runs, the place `xs`
   is locked, and so is everything above and below it (`this` for `this.files`). What it does not see: a change
   through another `ref` to the same collection, which needs the borrow check.
2. **`set.add(v)` gives a `bool`**: `true` if the item was added, `false` if it was already there (C#
   and Rust agree). It does not panic like `map.add`: adding what is there is ordinary use of a set.
3. `set.remove(v)` gives a `bool` too. A `HashSet` has no index, and `for x in ref set` is an error: an item
   changed in place would sit under the wrong value.
4. Writing an item (`xs[0] = v`) inside `for x in read xs` counts as changing `xs`. Pointing a `read` variable
   at something else does not.
5. `xs.indexOf(v)` gives a `nint?` (`null` if there is none), as `text.indexOf(byte)` does in the design.
6. `indexOf`, `contains` and `sort` work on integers and text (`bool` for the first two); since the tenth
   slice also on items written `: Equatable` and `: Comparable`. On another item type the error says so.
7. `string.join(separator, read names)` is the design's notation; it takes a `read List<string>` here, where
   the design means a slice.

## Rules from the tenth slice

Approved on 12 November 2026. Each has a test in `tests/cases/`.

1. **`Equatable`, `Hashable` and `Comparable` are built-in names, like `Copyable`.** `interface` of your own is
   still not here. The design has all three with these signatures.
2. **`Equatable` without an `equals` compares field by field**, as the design says. A `T?` field is equal when
   both are `null` or both hold equal values; a `T*` field is compared by what it owns.
3. **`==` wants the same type on both sides** and looks through `read` and `T*`. `a != b` is `!a.equals(b)`.
4. **`Hashable` needs `Equatable`**, and the hash is made from the fields. Here a `Hashable` type cannot write
   its own `equals` or `hash` yet: the interpreter keeps its maps in order of the fields and never calls `hash`.
5. **`Comparable` needs a `compare` that is written**; it is not made from the fields (the design generates
   only `equals` and `hash`). An enum cannot be `Comparable` until enums have methods (they came in the forty-third slice, 251).
6. **`a.compare(b)` and `a.equals(b)` exist on integers and text** (`equals` on `bool` too), as the design's
   `this.major.compare(other.major)` needs.
7. **An argument that is only looked at is lent when it cannot be copied**: `old.compare(read next)`,
   `tags.contains(read tag)`, `board.get(read key)`; a `Copyable` one is passed as it is. The operators need
   no `read` (`old < next`): looking is not taking.
8. **A failed `assert` writes a struct or an enum field by field**, whether it is `Formattable` or not.

9. **A collection is `Equatable` if what it holds is.** A list is equal to another with the same items in the
   same order; a set with the same items and a map with the same keys and equal values, whatever the order.
   So `assert(xs == List<int> { 1, 2, 3 })` can be written, and a struct that holds a list is `Equatable`
   field by field. A collection is not `Hashable` (it is not a key) and not `Comparable` (`xs < ys` is an
   error): an order is written, not made up.

## Rules from the eleventh slice

`test` blocks, as decided in the design ("Test" and "`assert`" rows). What the design left open was chosen and
approved on 12 November 2026; each has a test in `tests/cases/` or `examples/`.

1. **The command is `sg test file.sg`**, and the report is one line per test: `ok    name` or `FAIL  name`
   followed by the panic or `error: ...`, then `4 test(s) passed` or `1 of 4 test(s) failed`.
2. **A test file needs no `main`** under the test command; `sg file.sg` on a file that has only tests says so
   in the hint.
3. **Two tests with the same name are an error.**
4. **`try` on a `T?` in a test is an error** (there is no caller to give `null` to); the hint is
   `assert(x is not null)`.
5. **`test "..."` inside a function is an error** that says where a test goes; `test` stays an ordinary name
   everywhere else (`var test = 5`).
6. **Here the tests run one after the other in the same process**, each with variables of its own; the design
   runs each in a separate process. With no global state and a panic that is caught, the difference cannot be
   seen yet.

`examples/tests_stock.sg` is the stock tests of the design in one file (no modules yet), and it passes. The
golden test runs every file whose name starts with `tests_` through the test command.

## Rules from the twelfth slice

Function values, as decided on 12 November 2026 ("Function value" in the decisions table). The small choices
made here were approved the same day; each has a test in `tests/cases/`.

1. **A variable that holds a function value wins over a function of the same name** when it is called, like
   the rule for `Name.f()`. Calling a variable that is not a function says what it is.
2. **A method and a function without `this` in a struct are not values** (`Stack.create` cannot be passed);
   only top-level functions are. `main` and the built-in functions (`print`) are not values either.
3. **The result of a call is not called directly**: `pick(true)(2)` is written with a variable in between.
4. **`x => ...` in an argument is an error that says there are no lambdas**, with the hint to pass a name.
5. A function value is not written as text and not compared with `==`.

`examples/order_rules.sg` is the order rules of the design.

## Rules from the thirteenth slice

The first part of the borrow check: borrows that do not go together. The rule itself is the design's (one
`ref`, or any number of `read`), and a borrow lasts until its last use (decided on 12 November 2026). How it
is found was chosen here and approved the same day. Each has a test in `tests/cases/`.

1. **What holds a borrow is a variable.** A variable gets one from `read x` / `ref x`, a range of text, an
   item that is lent (`names[0]`), the names of a `match` or `is` pattern, a copy of another borrow, and the
   result of a call that returns a borrow.
2. **A call that returns a borrow is taken to borrow from its receiver and from every borrow it was given**,
   until `from` can say which one. This can only reject too much, never too little.
3. **Fields are told apart**: `ref p.x` and `read p.y` go together; `ref p` and `read p.x` do not. Items of a
   collection are not told apart: a borrow of `xs[0]` is a borrow of `xs`.
4. **"Used again later" follows the shape of the code**: a use in the other branch of an `if` or a `match`
   does not count; inside a loop a use anywhere in the loop counts (it comes round again), unless the variable
   is declared inside the loop and so is new in every round.
5. **In one call the same place is not lent twice when one of the two is `ref`**: `swap(ref x, ref x)`,
   `copyInto(ref xs, read xs)`, `bag.merge(read bag)`. A value read for an argument is not a borrow:
   `xs.add(xs.length())` is fine.
6. **Pointing a `read` variable at something else frees what it held** (`cur = read cur.next`).
7. Known gaps, all on the side of accepting too much: a `match` used as a value that gives a borrow is not
   followed, and a variable re-pointed inside a loop keeps counting as used.

The check found nothing to reject in the 252 programs that existed before it.

## Rules from the fourteenth slice

The second part of the borrow check: a borrow does not outlive its owner. `from` is the design's notation
("Source notation"); what was chosen here was approved on 12 November 2026. Each has a test in
`tests/cases/`.

1. **`from` is checked in both directions.** Inside the function every returned borrow must come from a
   parameter that is named; at the call the result borrows only from the arguments that are named. So with
   `read string find(read this, read string wanted) from this` the text that was looked for is free as soon
   as the call returns. This replaces the stopgap of the thirteenth slice (a result borrowed from everything).
2. **`from` may be written with a single borrowed parameter too**, where it is not needed. It may name fewer
   parameters than are borrowed, never one that is not borrowed.
3. **A built-in method that returns a borrow lends from its receiver**: `xs.get(i)`, `map.getOrAdd(k, v)`,
   `text[a..b]`.
4. **A call through a function value that returns a borrow** is taken to borrow from every borrow it was
   given: a `Func` type has no place for `from`.
5. **`var kept = a`, with `a` a `ref int` or a `read int`, copies the number.** A borrowed variable used as a
   value is the value behind it, as it is in `a + 1` and `print(a)`. It used to make a second borrow, and the
   borrow check found the mistake that invites (a swap written with it). A second borrow is written
   `ref a` / `read a`. What cannot be copied (`read string`) is still a copy of the borrow, and a call that
   returns a `ref` keeps it: `var slot = map.getOrAdd(key, 0)`.
6. **"Gone at the end of its block" is found when the block ends**: a variable declared outside that still
   holds a borrow of something declared inside, and is used afterwards.

## Rules from the fifteenth slice

The third part of the borrow check: types that carry a borrow. The notation is the design's ("Source
notation", "Types that carry a borrow"); what was chosen here was approved on 12 November 2026. Each has a
test in `tests/cases/`.

1. **A value of a type that carries a borrow is followed like a `read` variable**: it keeps what it borrowed
   from, until its last use. So is a list or a map that holds such values, and a struct built from them.
2. **`from source` in a method means what the value holds, not the value.** After `var word = words.next()`
   the `words` can be used and changed again while `word` lives: `word` borrows the text. Returning something
   of the struct's own (`read this.label`) under `from source` is an error whose hint is `from this`.
3. **Lending a field that is itself a borrow lends what the field borrowed** (`this.input[a..b]`, `word.text`),
   not the struct it sits in.
4. **A type that holds a type with a source names a source too** (`struct Parser<from source>` because of its
   `Lexer`), and a type that names one must hold something that borrows.
5. **One source per type for now.** Two (`struct Match<from text, from pattern>`) answer "not in the
   interpreter yet".
6. **A borrow from outside cannot be stored through `ref`** (`this.input = other`): the design's notation for
   it (`where r from name`) went away with `where`. A value is built where its source is known; no notation
   for now (decided: fifteen slices never needed one).
7. **A function that returns such a type with no borrowed parameter writes no `from`**: it can only return
   constants.

Choices made in the twentieth slice (`Result` and `try`), approved on 12 November 2026:

27. **A `Result<T, E>` has the shape of an enum**: a tag of one byte and, beside it, room for the larger of
    the value and the error. A `Result<void, E>` is the tag and the error. It is returned by value like any
    struct: a small one in registers, a larger one written into room the caller made. Rust does the same and
    packs the tag further where it can; that is not done here. C written by hand returns an error code
    and gives the value through a pointer: the same work.
28. **`try` is one test of the tag and, on an error, a `return`.** The error is moved into what is returned,
    not copied, and when the caller's error type wraps it (`ParseError` into `AppError.Parse`) that is one
    more tag written. On the way out the function frees what it owns, as on any `return`. There is no
    unwinding and there are no tables: an error costs what a returned value costs. Rust's `?` is the same.
29. **Where `try` and `?? return` can stand** (replaces the limit of 14): anywhere in a statement, as long
    as no value that owns something was computed before them in that statement and is still without a
    name. So `Ok(try f())`, `print(try f())`, `new try f()`, `add(10, try f())`, `value * 10 + try digit(c)`,
    `if try ready()`, `while try more()` and `match try next() { ... }` are compiled. Still refused, with the
    hint to give the value a name first: after an argument that owns something (`f(name.clone(), try g())`),
    as an argument of a method call, inside `$"..."`, on the right side of `&&`, `||` and `??`, and inside
    `assert`. A limit of the back end, not a rule of the language. The first of these is compiled since
    the twenty-eighth slice (79); what is left is listed in 81.
30. **`??` on a `Result` frees the error, then computes the right side**; with `?? return` the error is
    freed on the way out.
31. **An error that `main` ends with is written by the code the program starts with**: output written so
    far goes out first, then `error: ...` on the error output, and the exit code is 1, as in the interpreter.
    For now the error is text, a number or a `bool`: writing a `Formattable` struct or enum is not in the
    back end yet (it comes with `print` of those).
32. **A `Result` that was moved out of its place gets the tag 255**, like an enum (choice 21).

Choices made in the twenty-first slice (`List<T>`), approved on 12 November 2026:

33. **A `List<T>` is a pointer to its items, a length and a capacity.** An empty list has no block; a full
    one gets room for twice as many (four at first). Rust's `Vec<T>` is the same three words. Because only
    a pointer is kept, a struct can hold a list of its own kind (`List<Node> children`).
34. **`xs[i]` is the item where it is, and the index is checked in every build**: it is what keeps memory
    safe. Nothing is copied to reach `xs[i].name` or `rows[1].add(4)`. Where the index does more than read
    (`xs[next()] += 1`) the place is found once.
35. **`for x in xs` on a list that is handed over gives each item to its round.** Leaving early (`break`,
    `return`, `try`) frees the items that were not reached, and the block. Over `read xs` / `ref xs` the
    name is a pointer to the item, position after position; the checker already keeps the list from
    changing during the loop.
36. **`xs.get(i)` gives a look at the item, or `null`.** In compiled code the look is a flag and a copy of
    the item's bytes kept beside the statement; it owns nothing and is never freed. For that reason `get`
    cannot stand on the right side of `&&`, `||` and `??` yet (the back end asks for a name). Since the
    twenty-seventh slice the look is a pointer to the item where it is, and `get` stands anywhere (76).
37. **`indexOf`, `contains` and `sort` are compiled for integers, `bool` and text.** For structs and enums
    they waited for `==` and `compare` in the back end (there since the twenty-second slice, see 41). `sort`
    was the C library's `qsort` here; it is a stable sort since 42.
38. **`print(xs)` and `xs == ys` were not compiled yet**; they are since the twenty-second slice.

One thing in the checker was found on the way: the node it inserts when a borrowed `T?` is opened with
`??` or `try` (`xs.get(1) ?? -1`) did not carry its type, which only the back end reads. It does now.

Choices made in the twenty-second slice (writing and comparing), approved on 12 November 2026 (writing a struct
without an allocation was asked for, for later):

39. **A struct, an enum or a list is written by a helper of its type**, made once for each type that is
    written: a struct field by field (`Point { x = 1, y = 2 }`) or by its own `format`, an enum as its
    variant, a list between brackets, text inside in quotes; the same text as the interpreter's. The helper
    writes into a text that grows, and that text is then printed: one allocation for a value that is
    written. Numbers and text are printed directly, as before, with none. Rust's `Display` writes straight
    into the output and allocates nothing. Since the twenty-fifth slice printing a value allocates nothing
    here either (choice 54).
40. **A `Formatter` is the text being written**, and `f.write(...)` adds to it.
41. **`==` is a helper of the type too**: the two values are compared where they are, through pointers,
    field by field, by variant, or item by item; a struct that wrote `equals` is compared by it. `<`, `<=`,
    `>` and `>=` call the struct's `compare`. Nothing is copied or allocated. `indexOf` and `contains` use
    the same helper and `sort` the same `compare`, so lists of your own types are searched and sorted.
42. **`sort` is a merge sort that keeps equal items in the order they had**, as the interpreter's does
    (with your own `compare` the difference can be seen: two files of the same size). It uses one block as
    large as the list while it runs. Rust's `sort` is the same kind; C's `qsort` promises no order.
43. **A `T*` and a `T*?` are written and compared by one helper**, since they are one C type; it knows
    `null`.

One thing in the interpreter was found on the way and is fixed: comparing two structs with `==` stopped the
interpreter itself when a `T?` field held a value on one side and `null` on the other and the value was not
one that can be a key (a struct with a list in it). It gives `false` now
(`tests/cases/ok_equal_values.sg`).

Choices made in the twenty-third slice (types that carry a borrow, function values), approved on 12 November 2026:

44. **A field that borrows is a pointer** to what someone else owns (for text: a pointer and a length). It
    is never freed with the value that holds it, and `<from source>` leaves no trace in the compiled
    program: the checker has already seen that the borrow does not outlive its owner. This is what a C
    programmer writes, without the guarantee. Rust's lifetimes cost the same: nothing.
45. **A function value is the address of the function**, one pointer, and a call through it is an indirect
    call. Rust's `fn` pointer and C's function pointer are the same.
46. **Looking at a field that itself borrows gives the pointer the field holds**, not a pointer to the
    field: in `match token { Name(text) => ... }` on a `read Token`, `text` is the `read string` of the
    variant.
47. **A field of a value that has no name can be read** (`longer(a, b).text`): the value is kept for the
    statement and freed after it.

Choices made in the twenty-fourth slice (`HashMap` and `HashSet`), approved on 13 November 2026:

48. **A `HashMap` and a `HashSet` are hash tables**: the entries lie one after the other in one block,
    and a second block of slots says, for each place a hash can point at, which entry is there (open
    addressing, at most three quarters full, twice the room when that is passed). Going over the table is
    walking the first block. An entry that is removed is replaced by the last one. The interpreter now
    builds its tables the same way, so a program prints the same in both; that is the only reason the
    two agree, and the language still promises no order. What falls out: the order of adding, until
    something is removed. Rust's `HashMap` mixes the order on purpose; C#'s `Dictionary` and Python's
    `dict` are built like this one.
49. **`map[key]` is the value where it is; a missing key stops the program** with the key in the message,
    as decided before. `get` is a look or `null`, `getOrAdd` keeps a copy of the key only when it has to
    add it, `remove` gives the value back.
50. **The hash has a secret key** (SipHash-1-3, the key drawn from the system when the program starts), so
    that keys chosen by someone else cannot all be made to collide. Rust does the same by default. It is
    the whole difference to C in the measurement above (1.20 against 0.99 with a plain hash). By the
    criterion of 12 November (a check that memory safety does not need gives way to speed) this is not a
    memory-safety check: a table full of collisions is slow, not unsafe. By the first principle (the
    default is the one that cannot go wrong) it stays. Decided on 13 November: the keyed hash stays the
    default; a plain, faster hash can be offered later as something asked for by name.
51. **Keys are numbers, `bool`, text and the structs and enums written `: Hashable`**; a struct is
    hashed field after field. `getOrAdd` with a key that owns memory other than text is not compiled yet
    (the key would have to be copied).
52. **A `for` over a map that is handed over gives each entry to its round**, as with a list; leaving
    early frees the rest.
53. **A table holds at most about two thousand million entries** (the slots are 32 bits wide, half the
    memory of 64); beyond that the program stops.

Choices made in the twenty-fifth slice (printing without an allocation, values that have no name), approved on
13 November (54 with the 64 KB limit, 55 with the rule for the values of a line):

54. **A printed value is written into one block that the program keeps**, not straight into the output. A
    statement writes above what the block holds and gives the room back when it is done, so a `format`
    that prints while it is writing does not disturb the line around it. Nothing is allocated unless a
    line is longer than every line before it. The block is as large as the longest line so far, up to
    64 KB: a block that a longer line made larger is freed as soon as nothing is written in it, so one
    very long line (a list of a million items) does not hold its memory until the program ends. Rust's
    `Display` writes straight into the output and keeps no block. That was not done for two reasons: the
    line would come out in pieces (a `format` that prints, or a panic half way, would show where the
    interpreter shows nothing), and every piece would be a call into the C library's output, which locks
    the file each time. Measured on `show` on 13 November, with the generated C changed by hand: 0.465 s
    straight into the output against 0.170 s through the block (C's `printf`: 0.358 s). Cost at run time:
    one copy of the line, and one test after it for the size of the block (it does not show in the
    measurement). The block is one for the program, and one for each thread where tasks run (376).
    Approved on 13 November, with the 64 KB limit added that day.
55. **`f.write(...)` writes a struct straight into the text it is adding to**, as a piece between the
    others, with no text of its own, always. What makes that possible is a rule of the language, decided
    on 13 November: **the values of a line (`$"..."`) are computed first, from left to right, and written
    after that.** In `$"{a} {say(3)}"` the call `say(3)` runs before `a`'s own `format` does; before the
    rule each value was written as soon as it was computed, and compiled code had to write a struct into
    a text of its own (an allocation) whenever a call stood behind it on the line. The interpreter was
    changed to the same order. Rust's `write!(f, "{} {}", a, b())` does the same: the arguments first,
    then the writing. Cost at run time: none, and one allocation less. What it costs in rules: the order
    in which `format` methods run is "after the values", not "in its place"; it can be seen only when a
    `format` prints or stops the program (`ok_line_values_first.sg`).
56. **A number inside a printed value is turned into digits by the runtime**, not by `printf`, and the fixed
    pieces (`Point { x = `) are added with their length known. This is where the time in `show` went.
57. **A value that has no name and is lent to a block lives until the block is done**: `for x in read
    make()`, `match read make() { ... }`, `if make() is Say(text) { ... } else { ... }`, and in a `while`
    for one round. It is freed on every way out of the block (`return`, `break`, `continue`). Rust keeps
    such a value until the end of the whole statement too. No cost: it is the free that was there, later.
58. **What the right side of `&&`, `||` or `??` needs for a moment is made and freed there**, when that
    side runs: `ready && name(n) == "x"`. For `??` this holds when the right side gives a number or a
    `bool`; any other value could point at what was made for it, and the back end still asks for a name.
    The message of a `panic` on the right of `??` is put together only when the program stops there.
59. **A method called on a `T*` that has no name** (`make(3).total()`): the box is kept until the statement
    ends; a method that takes `this` gets the value and the box is freed.

Choices made in the twenty-sixth slice (modules: `use`, `public`, `readonly`; checker and interpreter),
approved on 13 November. The rules of 11 November (a folder is a module, `use`, visibility, what follows from
visibility) are in the decision table and are built as written there; these are what building them asked for:

60. **A file is a program of one file; a folder is a program.** `sg main.sg`: that file alone is the root
    module, and the folders next to it are the modules it can name. `sg shop`: every `.sg` file in the
    folder is the root module. The design says "the files of a folder are one module", and for a folder
    that is so; the first form is there so that a folder can hold many small programs (`examples/`,
    `tests/cases/`), each with its own `main`. Go does the same (`go run file.go` against `go run .`).
61. **A module is read when a file names it**, with `use` or with a full name (`billing.Line`), and then
    what that module names, and so on. A folder nobody names is not read, so a mistake in it does not
    stop the program. The test command with a folder (`sg test shop`) reads every folder under it and
    runs the tests of all of them; with a file it runs the tests of that file only.
62. **A module is named by its whole path from the root of the program**: `use shop.stock`, also from
    inside `shop`. There is no short form from a neighbour. The root has no name, so no module can `use`
    it, and `public` says nothing there: the rule "a `public` name does not show a type that is not
    `public`" is checked in the modules, not in the root.
63. **A name of the file's own module comes before the names brought in by `use`**, and a variable comes
    before both. A `Point` of your own next to `use stock` (which has a `Point` too) is yours and no
    error; theirs is `stock.Point`. C# and Rust do the same. Without this, adding a name to a module
    could break every file that uses the module.
64. **The same name in two modules named with `use` is an error where it is used**, as decided, for
    functions as for types; the hint is the full name. Naming both modules is no error by itself.
65. **`readonly` goes all the way down**: nothing behind the field can be changed either. `account.owner =
    ...` is an error, and so are `ref account.owner`, a `ref this` method on it and `account.history.add(1)`
    for a `readonly List`. The value that holds the field can still be replaced as a whole (`account =
    Account("grace")`). It holds inside the module too. C#'s `readonly` stops only the assignment for a
    class and goes all the way for a struct; here every value is owned like a C# struct, so the second
    reading fits. No cost at run time.
66. **`print` writes the name of a type without its module**: `Product { code = "K-1" }`. Error messages
    of the checker write the full name (`stock.Product`) for a type of another module.
67. **The test command writes the module in front of a test's name** (`ok    stock: nothing ships when
    stock is short`), nothing for a test of the root. Two tests may have the same name in two modules,
    not in one. The summary is one line for the whole program, not one for each module as in the design's
    sketch.
68. **The hint for a name that was not found looks into the folders nobody named**: `Product("K-1", 1500)`
    without a `use` says `use stock`. It is the only time those folders are read.
69. **`use` lines stand before everything else in the file**, one module on a line; the same line twice
    and a folder that is not there (or holds no `.sg` file) are errors. An unused `use` is fine, as
    decided.
70. **What `public` on a module's name promises is checked for**: the parameters and the result of a
    `public` function, the `public` fields and the `public` methods of a `public` struct, and every value
    of a `public` enum's variants. A `public` field or method in a struct that is not `public` is allowed
    and says nothing.
71. **A folder is a module only if its name can be written as a name** (letters, digits, `_`; no `-`, no
    dot in front).
72. **A program with modules was not compiled yet**; it is since the twenty-seventh slice.

Choices made in the twenty-seventh slice (modules in the back end, a `read T?` that a function gives back),
approved on 13 November 2026:

73. **A name of a module is written into the C name with each folder and its length in front**:
    `shop.parts.Part` is `S_4shop_5parts_Part`, `stock.total` is `f_5stock_total`. A name of the root is
    written as before (`S_Part`). No name of the root starts with a digit, so a root name and a module name
    never meet, and the lengths keep two modules apart whatever they are called. Nothing of this is seen
    from the language; the whole program is still one C file.
74. **A compiled panic names the file of the module it happened in**, as the interpreter does
    (`--> stock/functions.sg:27:12`).
75. **`sg build` takes what `sg` takes**: a file (the root module alone) or a folder (every file in it is
    the root module); the program is named after the file or the folder unless `-o` says otherwise. Only
    the modules the program names are read and compiled. The `test` blocks are not compiled, as before:
    `sg test` runs them in the interpreter.
76. **A `read T?` is a pointer to the `T`, null when there is nothing** (decided on 13 November 2026; the
    first form of this slice gave back a flag and a copy of the value's bytes, and was replaced). A function
    that gives one back (`read Product? get(read string code) from this`, `return null`, `return read
    this.products[i]`) gives back a pointer straight into what it was lent: nothing is copied, and it is
    what would be written in C by hand. `xs.get(i)` and `map.get(key)` do the same now (36 said a copy),
    so they can stand on the right of `&&`, `||` and `??`. `null` or a borrowed `T` given to a `read T?`
    parameter is that pointer or no pointer, which was not compiled before (16). A `read string?` is the
    pointer and the length of a `read string`, with the length -1 for nothing. Where a `T?` that is kept
    is lent (`read account.limit`, a round of `for n in read maybes`), its flag is tested once, there.
    `ref T?` stays a pointer to the flag and the value, because `null` can be written through it. A `T?`
    that can be copied out of a `read T?` (`var copy = n`) is made there. Memory safety is the borrow
    check's as before: the pointer cannot outlive what it points into. Measured with `lookup` (below):
    the time of the C written by hand. Against the first form an optimised build shows no difference
    here, since the C compiler removed the copy once the small function was inlined; a debug build of
    the same loop went from 4.75 s to 3.66 s.
77. **The stock and invoice example is compiled** (`sg build examples/shop`) and gives the same output as
    in the interpreter, also under the sanitizers; `cargo test` compiles the programs of `tests/modules`
    with the others.

Choices made in the twenty-eighth slice (a `match` inside an expression, leaving after a value that has no
name), approved on 13 November 2026:

78. **A `match` inside a larger expression is written before the statement**, as the statements it is, and
    gives its value through a name: `return 2 * match shape { ... }`, `f(a, match ...)`, `if match ... {`,
    `while match ... {`, a `match` inside an arm of another. The parts of the statement that stand before
    it are computed before it, so the order is still left to right. On the right of `&&`, `||` and `??`
    it runs only when that side runs (for a number or a `bool`, as any value that needs lines of its own
    there). No cost: it is the C that `var x = match ...` gave, without the name being written.
79. **A value that owns something and has no name is kept under a name when something after it can leave
    the function**: an argument of a call, a field of a struct being built, a value of an enum value
    (`Ok(Pair { name = make(), size = try parse(text) })`, `join(make(), make(), half(n) ?? return null)`).
    The way out frees it. When nothing leaves, it is handed over by its bytes and the name is emptied
    right after, so the end of the statement frees nothing twice. Cost: none where nothing after the
    value can leave (the C is what it was); one store for each kept value where something can.
80. **An arm of such a `match` may leave** (`return`, `try`, `break`, `continue`) wherever a `try` may
    stand: what the statement keeps so far is freed on that way out. Whether something "can leave" is
    read off the expression: it holds a `try`, a `?? return` or a `match`. A `match` whose arms all stay
    is counted too, which costs the store of 79 and nothing else.
81. **Still refused, with the hint to give the value a name first**: `try`, `?? return` or a leaving
    `match` after an item of a list being built (`List<string> { make(), try f() }`), after a side of an
    operator that owns something (`make() == make(try f())`), as an argument of a method call, inside
    `$"..."`, on the right of `&&` and `||`, and inside `assert` (but see 88); a `match` that gives anything but a
    number or a `bool` on the right of `??`. Limits of the back end, not rules of the language; the
    interpreter runs them all.
82. **Every compiled program is looked at for leaks in `cargo test`** (asked for on 13 November 2026). The
    first form of this slice freed nothing twice and read nothing freed, but left a value behind on the
    early way out: nothing reads such a value and nothing frees it, so neither the output nor the
    sanitizers showed it. Now each program is run once more after the sanitizers: on macOS under the
    system's `leaks` tool (the sanitizer does not look for leaks there), elsewhere with the sanitizer's own
    leak check. The check is tried first on a C program that leaks on purpose, and the test fails if it
    does not see that leak or cannot be run: a check that checks nothing does not pass. A program the C
    compiler rejects under the sanitizers is a failure too, where it was skipped before. With
    `SG_NO_SANITIZERS=1` both are left out and the test says so. Cost: about half a minute more for
    `cargo test`. The path on Linux was first run in the fifty-eighth slice (382, 383). *(17 November 2026,
    the ninety-sixth slice: a debug build counts the blocks of its values itself, on every system, and
    reports those not given back; the sanitizers still look after it, 975-978.)*

Choices made in the twenty-ninth slice (the test command on compiled code), approved on 13 November 2026:

83. **`sg test <file | folder> --compiled` runs the tests as compiled code.** Without the word the
    interpreter runs them, as before: it is the reference and needs no C compiler. `sg build <path> --test`
    writes the program that runs the tests and keeps it, `sg emit-c <path> --test` prints its C. A debug
    build, so overflow is checked as it is by the interpreter.
84. **Each `test` block runs in a process of its own**, as the design decided: a panic ends that test and
    the ones after it still run. The program starts a copy of itself for each test (`fork`) and waits for
    it. For now that is POSIX only; it does not build on Windows. *(17 November 2026, the ninety-sixth
    slice: the program starts itself again with `--only N` through `posix_spawn`, with no `fork`, 972.)*
85. **The report is the interpreter's, letter for letter**: `ok    name` or `FAIL  name` with the reason
    under it (the panic with its source line, or `error: ...`), the module in front of a test of a
    module, the count at the end, and the exit code 1 if a test failed. What a test prints comes before
    its line. The reason is what the test wrote as it stopped, caught by the program that started it.
86. **A `try` that meets an error in a test writes it and ends the test**, freeing what the test owns:
    the test function returns, it does not stop the process.
87. **`--only N` runs one test in the same process** and ends with 0 if it passed, 1 if it failed, 3 if
    there is no test of that number. It is how a single test is looked at: under a debugger, or for leaks.
88. **`assert(try load() == 42)` is compiled**: each side of a comparison of numbers or `bool`s in an
    `assert`, and a condition that is not a comparison, may hold a `try`. The other limits of 81 stand.
89. **`cargo test` runs the compiled tests** (`compiled_tests`): every program of tests is built, its
    report is compared with the same `.out` file the interpreter's report is compared with, it is run
    again under the sanitizers, and every test that passes is run once more on its own to see that it
    left no memory behind. A test that fails stops where it is and frees nothing, so it is not looked at.

Choices made in the thirtieth slice (slices and fixed-size arrays; the checker and the interpreter, the back
end comes after), approved on 13 November 2026:

90. **A range of items is a `read T[]`; `ref` in front of it asks for a `ref T[]`.** `xs[a..b]` on a
    list, an array or a slice only looks, as a range of text does. `ref xs[a..b]` is the window that can
    be written through (`fill(ref list[4..], 0)`); it needs what `ref xs` needs. `read xs[a..b]` stays an
    error whose hint is the bare range. The design had the two slice types but no way to write a range
    that is written through. Rust writes `&mut xs[a..b]`.
91. **An array is built as the design says, and `byte[16]` alone is told from an index by the name.**
    `int[3] { 1, 2, 3 }` and `byte[16](255)` cannot be anything else. `byte[16]` reads like "item 16 of
    `byte`": it builds an array when `byte` is the name of a type and no variable has that name (a
    variable wins, as it does for `Shape.Circle`). In a condition and in the head of a `for`,
    `int[3] { ... }` needs parentheses, as a struct literal does there.
92. **`T[0]` is an error, and an array of arrays is refused for now.** An array with no items has no use
    and no natural form in C. The design left the notation for two dimensions open on purpose.
93. **A slice that may be missing (`read T[]?`) is refused for now**; nothing needed it yet.
94. **An array and a slice have the methods of a list that only look**: `length`, `get`, `indexOf`,
    `contains`. `add`, `clear` and `removeLast` are errors that say the number of items is fixed and
    name `List<T>`. `sort` and `reverse` through a `ref T[]`, `addAll` on a list and looking for a run
    of items (`data.indexOf(other)`) are not here yet. (The first three came in the forty-second slice,
    choices 234 to 241.)
95. **`==` on two arrays or two slices compares the items in order** when the items are `Equatable`,
    as for a list; `print` writes `[1, 2, 3]`. An array is not a key of a `HashMap` for now. An array
    and a slice are not compared with each other: `read a == s` has two different types.
96. **Only an index on an array is looked at before the program runs.** The ends of a range, and any
    index on a slice or a list, panic when they are wrong, with the same messages as text.
97. **`TextError` is a built-in struct**: `struct TextError : Copyable, Formattable, Equatable` with
    one field, `public readonly nint position`, the place of the first byte that is not valid UTF-8
    (Rust's `Utf8Error::valid_up_to`). It prints as `TextError { position = 2 }`, is seen from every
    module with nothing written, and its name cannot be given to another type. The design named the
    type and nothing more.
98. **`Result<read T, E>` can be written**, since `string.fromUtf8` gives one and a function must be
    able to pass it on. Such a function says where the borrow comes from (`from data`), as for any
    type that carries a borrow. A borrow as the error is still refused.
99. **A borrow of a value that has no name cannot be kept** (a hole in the borrow check, found here
    and closed). `var part = make()[..5]` was accepted, and in compiled code `part` looked at memory
    that was freed when the line ended: the address sanitizer shows it. The value a call returns is
    kept only until its statement ends (choice 9), so a borrow of it may be used inside that statement
    and nowhere else. Now an error in a variable, in a field or an item, and in `return`, for every
    form of borrow: a range, `read make()` given to a function that returns a borrow, a method that
    returns one, `asBytes()`. A text constant is exempt: it is there for the whole run. Rust has the
    same rule ("temporary value dropped while borrowed"). Cost: none at run time.
100. **`for` over a slice lends the items** (`read` or `ref`, as the slice is); over an array it works
     as over a list: handed over, `read` or `ref`. A list cannot be changed inside a `for` over a
     range of it.
101. **In the interpreter a slice of items is a real window**: it holds the cells of the items, so
     what is written through a `ref T[]` lands in the list or the array. The bytes of a text are
     copied for `asBytes()`, and a range of text is still a copy; neither can be told apart from a
     window, since text is never written through.
102. **The sensor window example differs from the design in two lines**: the hex digit is found with
     `/ 16` and `% 16`, and the id is written in decimal, because the bit operators (`>>`, `&`) and
     hex numbers are not in the interpreter; and the average divides by `(long)this.count`, since a
     `long` and an `nint` do not mix on their own (the design text is corrected). (Since the
     thirty-second slice the example uses `>> 4`, `& 15` and `0xde`, as the design wrote it.)
103. **The back end refuses arrays and slices** with the usual "not in the C back end yet". That is
     the next slice: an array as a C array inside a struct, a slice as a pointer and a length. (Done
     in the thirty-first slice: 104 to 112.)

Choices made in the thirty-first slice (arrays and slices in compiled code), approved on 13 November 2026:

104. **An array is a C array inside a struct.** C neither hands a bare array to a function nor gives
     one back; inside a struct it is a value like any other. So an `int[8]` lies where its variable
     or its field lies, nothing is allocated for it, and passing, returning or assigning it copies its
     eight numbers, as the same struct would be copied in C. `values.length()` is the number 8 in the
     C. `values[i]` is checked in every build; where the index is a constant the C compiler sees that
     the check cannot fail, and the checker has already refused the constant that would.
105. **A slice is a pointer and a count, and `read T[]` and `ref T[]` are one C type.** Sixteen bytes,
     handed over in two registers; what `read` forbids is the checker's work, not the C compiler's.
     It is the same whatever the items are kept in: a list, an array, another slice.
106. **`asBytes()` and `string.fromUtf8` copy nothing.** The bytes of a text are the pointer and the
     length the text already is. `fromUtf8` reads the bytes once to see that they are text and gives
     the same pointer and length back as a `read string`. The check is written in the runtime, byte
     by byte; `ok_from_utf8` compares it with the interpreter (Rust's check) on the cases where the
     rules change: characters cut off by the end, surrogates, more bytes than a character needs.
107. **A range checks its two ends once.** After that an index into the window is checked against
     the window's own count, so no index reaches outside the range it was given. Inside
     `for i in 0..s.length()` the C compiler removes the check, as it does for a list.
108. **An end that lies outside is said first** (a change to the interpreter too). `xs[4..]` on three
     items said "range 4..3 ends before it starts"; it now says "range 4..3 is out of range: the
     length is 3", and so does a range of text. "ends before it starts" is left for a range whose two
     ends are both inside (`xs[2..1]`).
109. **An array whose items own something is freed item by item** where its variable ends. Moving it
     empties every item, so the later free finds nothing. A `for` that is handed such an array gives
     each item to its round and frees the ones after it if the loop is left early. An array of numbers
     has none of this: there is nothing to free.
110. **The success of a `Result` can be a borrow in compiled code**: a `read string`, a slice, or a
     `read` of a struct, an enum or a collection (a pointer inside the `Result`). A `read T?` as the
     success is still refused there.
111. **Still refused in compiled code, by name**: a `for` that is handed an array behind a pointer
     (`for x in boxed` with a `T[N]*`), as for a list behind a pointer. `for x in read boxed` is
     compiled.
112. **The measurement is `buffer`** (above): buffers on the stack and slices onto them, against C
     that passes a pointer and a count.

Choices made in the thirty-second slice (the bit operators, numbers in hex and binary; checker, interpreter and
back end together), approved on 13 November 2026:

113. **The bit operators are `&`, `|`, `^`, `~`, `<<` and `>>`, with `&=`, `|=`, `^=`, `<<=` and `>>=`,
     on integers only.** C#'s spelling (for "every bit turned over" Rust writes `!` and Go `^`). Two
     `bool`s have `&&`, `||` and `!=`: `a & b` on them is an error whose hint names the one that
     fits, and `~done` points at `!`. C# and Rust also read `&` on two `bool`s as "and, without the
     short circuit"; that second way to write the same thing is left out. `>>` is read as two `>`
     with nothing between them, so `List<List<int>>` needs no space (C# does the same). A line that
     ends with one of them goes on, and a line cannot start with one, as for the other operators.
114. **`&`, `^` and `|` bind tighter than a comparison.** From loosest to tightest: `||`, `&&`,
     `== !=`, `< <= > >=`, `|`, `^`, `&`, `<< >>`, `+ -`, `* / %`. So `flags & mask == 0` tests the
     masked value. In C and C# the same line means `flags & (mask == 0)`, a leftover from before C
     had `&&` that its authors called a mistake; Rust, Go, Swift and Zig read it as it is read here.
     With C#'s order the line would be a compile error here (an `int` and a `bool`), so the choice is
     between an error on a line that can mean only one thing and the line working. Among themselves
     the operators keep the order of C, C# and Rust: a shift is looser than `+`, so `1 << n + 1` is
     `1 << (n + 1)`. Go has fewer levels (`<<` and `&` with `*`, `|` and `^` with `+`), which
     changes what that line means for a reader from C or C#. A bound of a range takes everything
     tighter than a comparison: `for i in 0..1 << n`. Cost: none when the program runs.
115. **`a & b`, `a | b` and `a ^ b` follow the rule of arithmetic**: the narrower side widens to the
     other, a constant takes the type of the other side, and the result has that type. `byte & byte`
     is a `byte`, and `~x` has the type of `x` (in C and C# both are an `int`).
116. **A shift has the type of its left side, and the count is any integer.** `b >> 4` on a `byte`
     is a `byte` (Rust's rule; an `int` in C and C#). The type that is expected reaches the left side
     alone: `long wide = 1 << 40` shifts a `long`. In C `1 << 40` is an `int` shifted too far, which
     is undefined; here `var big = 1 << 40` is a compile error (117).
117. **The count follows the overflow rule.** A count below zero, or not below the bits of the type
     (8, 32, 64), is a compile error where the count is known, a panic in the interpreter, in tests
     and in a debug build ("shift by 32 is out of range: int has 32 bits"), and is cut to the bits of
     the type in a release build. That is Rust's rule, and the cut is what the shift of the machine
     does by itself, so a release build pays nothing for it. In C such a count is undefined. C# always
     cuts and never tells. Go gives 0 for a count that is too large, with a test in every build, and
     panics below zero. `x << (n & 31)` written out means the same in every build, and the test falls
     away there too. There are no `wrapping...` and `try...` methods for shifts: the mask says it.
118. **The bits that `<<` pushes out are gone in every build**, without a panic: that is what a shift
     is for. `1 << 31` is the smallest `int`. Rust, C# and Go do the same.
119. **`>>` carries the sign on `int`, `long` and `nint` and brings in zeros on `byte`**: the type
     decides, as in C#, Rust and Go. There is no `>>>`. The shift that brings zeros into 32 or 64
     bits comes with `uint` and `ulong` (since the thirty-third slice, 130).
120. **A number can be written in hex and in binary, with `_` between digits**: `0xde`, `0b1010`,
     `1_000_000`, `0xdead_beef`. The prefix is a small letter (`0XFF` is an error that shows the fix),
     the hex digits may be either; there is no octal. Such a number is a number like any other: it
     takes the type it is used as, `var x = 0xff` is an `int`, and it works as a pattern of `match`.
     One that does not fit is an error that shows it as written, with its value: `int mask =
     0xffff_ffff` gives "`0xffff_ffff` is 4294967295, which does not fit in int". In C# that number
     quietly becomes a `uint`; Rust refuses it as here. The design wrote `0xde` only; `0b` and `_`
     are added here (a dozen lines in the lexer, nothing at run time).
121. **A number where a condition is expected gets a hint**, for the habit from C: `if flags & 2 {`
     is "expected `bool`, found `int`" with "a number is not a condition; compare it: `flags & 2 !=
     0`". The same hint now stands under `if n {` (the older test `error_condition_not_bool`).
122. **In compiled code** `&`, `|`, `^` and `~` are the operators of C, cut back to the type (C turns
     a `byte` into an `int` first). `<<` is done on the unsigned number, so that a bit moved into the
     sign is defined in C. `>>` on a negative number relies on the C compiler carrying the sign: the
     C standard leaves that to the compiler, and clang and gcc, which the output already needs, both
     do. A debug build tests the count with one comparison that is never taken in a correct program.
123. **The measurement is `bits`** (above): a sieve with one bit for each number, and numbers
     written seven bits at a time and read back, against C with no checks.
124. **The sensor window example is written as the design wrote it**: `source[i] >> 4`,
     `source[i] & 15`, `0xde` (see 102). Its output did not change.

Choices made in the thirty-third slice (the other integer types; checker, interpreter and back end together).
125 and 126 were decided on 13 November 2026 before the slice was written; the others were approved the same day:

125. **All six types come together**: `sbyte`, `short`, `ushort`, `uint`, `ulong` and `nuint`, the names
     the design took from C# ("Number types"). `nint` and `nuint` are 64 bits on every machine SG runs on
     so far.
126. **A type widens to another on its own only when the other holds every one of its values, on every
     machine.** A signed type never becomes an unsigned one; an unsigned one becomes a signed one with
     more bits (`byte` to `short`, `uint` to `long`). `nint` and `nuint` count as "at least 32 bits":
     `int` becomes `nint` and `uint` becomes `nuint`, but `uint` does not become `nint`, and `long` does
     not become `nint`; `nint` becomes `long` and `nuint` becomes `ulong`. Two types where neither holds
     the other, like `int` and `uint` or `long` and `ulong`, do not mix: "`int` and `uint` do not mix:
     neither holds every value of the other", with a hint that names a type holding both
     (`(long)change`) or, where there is none, the cast to the other side's type. Rust and Go have no
     implicit widening at all; C# turns `int + uint` into a `long` and `long + ulong` into an error;
     C turns the `int` into an `unsigned` without a word, so `-1 < 1u` is false. One change to code
     that ran before: `long + nint` used to be an error and is now a `long`.
127. **A number written in the program takes the type of the other side**, on either side:
     `1 + count` and `count - 1` with a `uint count` are `uint`s, `10 > count` compares two
     `uint`s. A number past every `long`, which only a `ulong` holds, is typed from the right side
     when it stands on the left: `0xffff_ffff_ffff_ffff == top`. Cost: none at run time.
128. **`-x` on an unsigned type is an error**, as it already was on `byte`: "a `uint` is never
     negative", with the hint `-(long)count`. Rust refuses it the same way; C# turns `-uint` into a
     `long` and `-ulong` into an error; in C it wraps around.
129. **A cast follows the overflow rule, and a constant that cannot fit is a compile error**: `(uint)-1`
     is "-1 does not fit in uint" with the note "a `uint` is never negative", and so is `ushort port =
     -1`. A value that is known only when the program runs panics in the interpreter, in tests and in
     a debug build ("value -1 does not fit in ulong") and wraps in a release build, as for the types
     before (Rust's `as` always wraps; C# wraps unless the code is `checked`).
130. **`>>` brings in zeros on every unsigned type** (`uint`, `ulong` and the others), and carries the
     sign on the signed ones (119). So a rotation is `value << by | value >> (32 - by)` on a `uint`,
     with no mask.
131. **A number is never unsigned on its own.** With no type where it goes, a number is an `int`, and a
     `long` if it does not fit in an `int`, as before; a number past every `long` is an error:
     "0xffff_ffff_ffff_ffff does not fit in long", hint "`ulong x = 0xffff_ffff_ffff_ffff`". C# makes
     it a `ulong` (and `0xffff_ffff` a `uint`) without a word; Rust refuses it as here. Written where a
     `ulong` goes, it is one.
132. **An index stays a `nint`.** `uint`, `nuint` and `ulong` are not `nint`s on their own (126: a
     `uint` does not fit in a 32-bit `nint`), so `items[at]` with a `uint at` is an error whose hint
     is `(nint)`. Rust indexes with `usize` only; C# with `int` and also `uint`, `long`, `ulong`; Go
     with any integer. Unsigned indexes would also make `i - 1` below zero a panic of its own.
133. **The rest works on the new types as on the old**: `print`, `$"..."`, `assert` (which writes a
     `ulong` in full), `match` patterns (`18446744073709551615 => ...`), `for i in a..b`, keys of a
     `HashMap`, items of a `HashSet`, fields of `Equatable`, `Hashable` and `Formattable` structs,
     `sort`, `compare`, and the `wrapping...` and `try...` methods.
134. **The interpreter keeps every integer as its value, in 128 bits**, whatever its type: a `ulong`
     needs 64 bits without a sign, and the same value compares, divides and prints right without a
     second path for unsigned numbers. Two `ulong`s can multiply past 128 bits, so that one
     multiplication is checked on the way.
135. **In compiled code** the types are `int8_t` to `uint64_t` (`nint` and `nuint` are `int64_t` and
     `uint64_t`), each with its line of checked operations. A failing operation hands its numbers to
     the panic as `__int128`, so that "overflow: 18446744073709551615 * 2 does not fit in ulong" is
     written as the interpreter writes it, and a cast tests its value the same way. `__int128` is an
     extension of clang and gcc, like the overflow builtins the output already needs; it is used only
     on the way into a panic and in the test of a cast.
136. **The measurement is `hash`** (above): XXH64, the 64-bit xxHash, over one megabyte, two thousand
     times, against C that reads eight bytes at a time with `memcpy`. What was found on the way:
     written byte by byte as `(ulong)data[at] | (ulong)data[at + 1] << 8 | ...`, each of the eight
     indexes is checked, the C compiler cannot join the eight reads into one, and the program took
     3.5 times the time of C (0.46 s against 0.13 s). C written the same way takes the time of
     `memcpy`, because there nothing stands between the reads. Cutting the eight bytes out first,
     `var b = data[at..at + 8]`, checks the range once, and after it the compiler knows every
     `b[0]` to `b[7]` is inside: the reads become one and the time is that of C. The same test
     written by hand into the C output in front of the eight reads did not do it. The benchmark reads that way; a built-in that reads a number
     out of bytes (`ulong.fromLittleEndian(...)`, Rust's `u64::from_le_bytes`, Go's
     `binary.LittleEndian.Uint64`) is a candidate, not part of this slice. (The fifty-first slice adds
     it: 301.)
137. **A release build wraps on the new types too** (`tests/release/unsigned_wraps`): `2 - 3` on a
     `uint` is 4294967295, `127 + 1` on an `sbyte` is -128, `(ulong)-1` is the largest `ulong`.

Choices made in the thirty-fourth slice (`float` and `double`; checker, interpreter and back end together).
The design had decided the types and how they compare ("Number types", "`float`, `double` and `NaN`");
the rest was approved on 13 November 2026:

138. **A fractional number is written with digits on both sides of the point, or with an exponent**:
     `1.5`, `0.25`, `1e9`, `6.674_30e-11`; not `1.` or `.5`. The exponent takes a small `e`, as the
     hex prefix takes a small `x` (`1E5` is an error whose hint is `1e5`), and `_` stands between
     digits as in an integer. There is no suffix: `1.5f` is "a number has no `f` at its end; it takes
     the type it is used as", hint `float x = 1.5`. Like an integer the number takes the type it is
     used as, and is a `double` where nothing says otherwise. C# needs the `f` (`float x = 1.5` is an
     error there); Rust infers the type as here.
139. **A number written in the program must keep its value in its type**: `1e400` is "does not fit in
     double", `float tiny = 1e-50` is "too small for float: it would be 0", as Rust and C# refuse
     them. A whole number written where a fractional one goes becomes one (`double two = 2`,
     `2 * x`, `x / 3`) if the type holds it exactly: `float f = 16777217` is an error.
140. **The mixing rule of the integers (126) goes on to the fractional types**: a number becomes a
     fractional one on its own only when the type holds every value of it exactly. `float` becomes
     `double`; every integer type up to 16 bits becomes `float` and `double`; `int` and `uint` become
     `double`; `long`, `ulong`, `nint` and `nuint` never do ("`long` and `double` do not mix: a
     `double` does not hold every `long` exactly", hint `(double)count`). `double` never becomes
     `float`, and a fractional number never becomes an integer on its own (hint `(int)`, "it drops
     the fraction"). One cost shows at once: `sum / values.length()` needs `(double)`, because a
     length is a `nint`. C# turns a `long` into a `double` (and an `int` into a `float`) without a
     word, losing digits past 2^53 (2^24); Rust and Go convert nothing on their own. Two looser
     forms were weighed and left out: letting `nint` and `nuint` become `double` because they
     mostly hold lengths (the first exception to "every value, on every machine", and silent where a
     `nint` holds a large number), and C#'s way for every integer. A function that takes any number
     takes a `double`; a value that is sometimes whole and sometimes not is an enum written out
     (`enum Number { Whole(long v), Fraction(double v) }`).
141. **The arithmetic is the hardware's, and never panics**: `+`, `-`, `*`, `/`, `%`, `-x` and their
     assignments (`+=` and the others). A division by zero gives `Infinity` or `NaN`, as in C, C#,
     Rust and Go; there is no overflow to check. `%` keeps the sign of the left side (C's `fmod`). A
     `float` is computed as a `float`, not widened to a `double` on the way. The bit operators and the
     shifts are for integers only ("the bit operators work on integers").
142. **Each operation is rounded on its own; `a * b + c` is never fused into one instruction.** The
     generated C says so (`#pragma STDC FP_CONTRACT OFF` for clang) and `sg build` passes
     `-ffp-contract=off` (for gcc, which does not read the pragma). Without it clang fuses on arm64
     and the compiled program printed other last digits than the interpreter. Rust does not fuse;
     C# and Java do not either. Cost: a fused multiply-add is faster and more exact in some loops; a
     method that asks for it (`x.fma(y, z)`, Rust's `mul_add`) is a candidate, not part of this slice.
     (Since the thirty-fifth slice: `a.fusedMultiplyAdd(b, c)`, 162.)
143. **`==` and `<` are the hardware's; `equals` and `compare` a total order** (decided in the design:
     `NaN == NaN` is false, `nan.equals(nan)` is true, `NaN` comes before every other value). Added
     here: `-0.0` and `0.0` are equal for `equals` and `compare` too (C#'s `Equals`). `sort`,
     `indexOf` and `contains` use the total order, so a `NaN` is found and sorts first.
144. **`==` on a struct, a list or an array compares fractional parts with `equals`**:
     `Point { x = nan } == Point { x = nan }` is true while `nan == nan` is false. This is what C#
     does for a record and for `EqualityComparer<double>.Default`; Rust's derived `==` gives false. It
     keeps "a value equals itself" for every type that is `Equatable`, at the cost that `p == q` is not
     always `p.x == q.x && p.y == q.y`.
145. **A fractional number is not `Hashable`** (decided in the design): "`double` cannot be the key of
     a `HashMap`: it is not `Hashable`", with the note that `0.0` and `-0.0` are equal with different
     bits; a struct with a `double` field cannot be `Hashable` field by field.
146. **A cast to an integer drops the fraction, and follows the overflow rule.** `(int)3.7` is 3,
     `(int)-3.7` is -3. A value that does not fit, `NaN` included, panics in the interpreter, in tests
     and in a debug build ("value 10000000000 does not fit in int", "value NaN does not fit in long");
     a release build gives the nearest value the type holds and 0 for `NaN`
     (`tests/release/float_saturates`), which is Rust's `as` since 1.45. In C the same cast is
     undefined behaviour; C# gives a value that depends on the machine. A constant that cannot fit is
     a compile error (`(int)1e10`). `(double)n`, `(float)n` and `(float)d` round to the nearest value
     and never fail; a `double` too large for a `float` becomes `Infinity`.
147. **A fractional number is printed with the fewest digits that read back as the same number**,
     laid out as JavaScript does it: `0.1`, `0.30000000000000004`, `2` (no `.0`), `100`, `1e+21`,
     `1.5e+21`, `1e-7`, `0.000001`, `NaN`, `Infinity`, `-Infinity`, `-0`. A `float` gets the fewest
     digits that read back as that `float` (`0.33333334`). C# has given the same digits since .NET
     Core 3.0 but writes `1E+15` from 10^15 on and `1E-07`; Go's `%v` changes to an exponent at 10^21
     like here; Rust's `{}` never does, and Python and Rust's `{:?}` write `2.0`. Formats such as
     `{x:N2}` stay deferred (design, "After the first version").
148. **The methods**: `abs`, `sqrt`, `floor`, `ceil`, `round` (half away from zero, as in C and Rust;
     C#'s `Math.Round` rounds half to even unless told otherwise), `truncate`, `isNaN`, `isInfinite`,
     `equals`, `compare` and `toString`. Not yet: `double.NaN` and the other constants (`0.0 / 0.0`
     and `1 / zero` give them), `sin`, `pow`, `exp`, `log`, `tryToInt`. (Since the thirty-fifth slice:
     157 to 163.)
149. **A fractional number cannot be a pattern** in `match`: "two fractional numbers are seldom exactly
     equal; compare in an `if`". Rust is taking float patterns away; C# allows them. `if x` with a
     `double` gets the hint `x != 0` (121).
150. **The interpreter keeps a `float` as the `double` that holds it exactly** and does every operation
     on it in 32 bits; `equals` and `compare` look at a 64-bit key in the total order. It finds the
     fewest digits the same way the compiled code does: the number is written with 1, 2, ... 17
     digits, each correctly rounded (Rust's `{:.*e}` and C's `printf("%.*e")` both are), until it
     reads back. So both print the same digits by construction. Cost: up to 17 tries for a number
     that needs 17 digits; a shortest-digits algorithm (Ryu, Grisu) is faster and is a candidate if a
     measurement finds printing slow. (The first version of `numbers` found it: "Slower than C".)
     *Changed in the sixty-eighth slice (496-498):* the digits are the fewest that read back and the
     nearest of them, a tie to the even digit; compiled code finds them with Dragonbox and the
     interpreter with Rust's shortest digits, and the two agree by the rule.
151. **In compiled code** `float` and `double` are C's; `sqrt` and the others come from `<math.h>`, and
     `sg build` links `-lm`. A cast to an integer goes through one helper per integer type, which
     tests the whole part against the two powers of two at the ends of the type (one comparison pair
     that is never taken in a correct program) or, in a release build, gives the nearest value.
152. **The measurement is `nbody`** (above): the sun and the four giant planets moved twenty million
     steps, a square root for each pair at each step, kept in a `Body[5]` on the stack, against C
     with no checks. The C is compiled without fused multiply-add too, so that both do the same
     arithmetic and print the same digits.

Choices made in the thirty-fifth slice (the tools for numbers; checker, interpreter and back end together).
The design had decided that `int.parse` gives a `Result` ("File API") and named `tryToInt` and
`wrappingToInt` ("Narrowing conversion"); the rest was approved on 13 November 2026:

153. **`T.parse(read string text)` on every number type gives `Result<T, NumberError>`**: `int.parse(text)`,
     `ulong.parse(field)`, `double.parse(text)`, called on the type as `string.fromUtf8` is. An owned
     `string` is passed as `read text`, the rule for every borrow. C# throws on a bad text and has
     `TryParse` with an `out` parameter; Rust's `text.parse::<i32>()` gives a `Result`; Go's
     `strconv.Atoi` gives a value and an error.
154. **The error is the built-in `enum NumberError : Copyable, Formattable, Equatable { Invalid(nint
     position), OutOfRange }`.** `Invalid` says where the text stops being a number, in bytes, and the
     length of the text when it ends too early (`"-"` is `Invalid(1)`); `OutOfRange` that it is a whole
     number the integer type does not hold (a fractional one past the type is `Infinity`, 156). An empty text is `Invalid(0)`, not a variant of its own: one case fewer to
     handle, and the position says it. Like `TextError` it is seen from every module and cannot be
     defined again, and `try` puts it into the variant of the caller's error type that carries it
     (`BadNumber(NumberError cause)`). Rust has five kinds and Go two, neither with a position.
155. **A whole number is a sign and digits, and nothing else**: `12`, `-12`, `+7`, `007`. A space, `_`,
     `0x` or a point is `Invalid` where it stands: `" 1"` is `Invalid(0)`, `"1_000"` is `Invalid(1)`.
     Rust is as strict; C# allows spaces around the number. A `-` on an unsigned type is `Invalid(0)`,
     `-0` too, as in Rust. Every byte is looked at before the size, so a long run of digits with a letter
     after it is `Invalid`, not `OutOfRange`.
156. **A fractional number is a sign, digits with at most one point, and an exponent**: `1.5`, `.5`,
     `5.`, `1e3`, `1E-3`, `+2.5e+2`. That is looser than a number written in the program (138), because
     data writes `.5` and `1E5`, and Rust and C# read them. What `print` writes, `parse` reads back:
     `NaN`, `Infinity`, `-Infinity`, `-0`, `1e+21`; not `nan`, `inf`, `-NaN`, hex (`0x1p3`) or a comma.
     The value is the nearest number of the type. One past the largest (`1e400`, `3.5e38` for a
     `float`) is `Infinity`, and one too small for the type becomes 0 or a subnormal number, as in Rust,
     C# and C: the same rounding as the arithmetic, where `double.MaxValue * 2` is `Infinity` too.
     `OutOfRange` for it was weighed and left out (decided on 13 November).
157. **Constants on the type**: `MinValue` and `MaxValue` on every integer type (C#'s names;
     `nint.MaxValue` is the machine's), and on `float` and `double` also `NaN`, `Infinity`, `Pi` and
     `E`; `double.MinValue` is the most negative finite number, as in C#. `Infinity` is the word `print`
     writes, and `-double.Infinity` the other one (C# has `PositiveInfinity` and `NegativeInfinity`).
     `Epsilon` is left out on purpose, and the error says why: in C# it is the smallest number above
     zero, in C and Rust the gap between 1 and the next number, so each habit would read the other's
     code wrong. `long.maxValue` gets `long.MaxValue` as a hint, `int.MaxValue()` "is a constant:
     `int.MaxValue`, without parentheses".
158. **A constant is a value of its type, not a number written out**: `uint u = int.MaxValue` is the
     error of an `int` (hint `(uint)`), `long wide = int.MaxValue` widens, `int.MaxValue + 1` panics as
     `x + 1` does. C# lets a constant that fits go to any type; here `nint.MaxValue` depends on the
     machine and could not, and one rule for all is simpler.
159. **`tryToX()` and `wrappingToX()` on every integer type, to every integer type**: `big.tryToInt()` is
     an `int?`, checked in every build, and `big.wrappingToInt()` keeps the bits that fit, in every
     build. The type is in the name (`tryToUlong`, `wrappingToByte`), as the design wrote it. One that
     cannot fail is an error: `b.tryToInt()` with a `byte b` is "every `byte` fits in `int`: there is
     nothing to convert", hint `int x = value`, and to the same type it is "this is an `int` already".
     `tryToint` gets `tryToInt` as a hint, `tryToDouble` gets "`(double)x` never fails; it rounds".
160. **On a fractional number only `tryToX()`**: toward zero as the cast does, `null` for `NaN` and for
     what lies past the type. There is no `wrappingToX()` on one: "a fractional number has no bits to cut
     to size". The cast in a release build already gives the nearest value (146).
161. **The panic of a cast and of an overflow names the way out** (design: "the panic message shows the
     value and `tryToInt`"): `(int)big` adds "hint: `tryToInt()` gives `null` when the value does not
     fit, and `wrappingToInt()` cuts it to size"; `a + b` adds "hint: `tryAdd` gives `null` instead, and
     `wrappingAdd` wraps around", `-` and `*` the same, `/` names `tryDivide`. Compiled code writes the
     same lines. `-x` and the shifts have no such method yet, so their panics give no hint.
     *Changed in the hundred-and-tenth slice (1243): `-x` names `tryNegate` and `wrappingNegate`.*
162. **The math is methods on `float` and `double`**, as `sqrt` already is (148), not `Math.Sin(x)`:
     `exp`, `log` (the natural one), `log2`, `log10`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`,
     `y.atan2(x)`, `x.pow(y)`, and `a.fusedMultiplyAdd(b, c)`, which is `a * b + c` rounded once (C#'s
     `Math.FusedMultiplyAdd`, Rust's `mul_add`; the candidate of 142). Two numbers meet as with `+`:
     `f.pow(d)` with a `float f` and a `double d` is a `double`. Outside its domain a function gives
     what the C library gives: `(-1.0).log()` is `NaN`, `0.0.log()` is `-Infinity`. On an integer the
     error gives `((double)count).sqrt()` as the hint.
163. **`min` and `max`**: on integers the two meet as with `+` (`b.max(300)` with a `byte b` is an
     `int`); on fractional numbers the answer is `NaN` if either side is one, and `-0` is less than `0`,
     as C#'s `Math.Min` and IEEE 754-2019's `minimum` have it. C's `fmin` and Rust's `f64::min` give the
     other side for a `NaN` and either zero for two zeros, so the answer would depend on the order of
     the two and on the machine.
164. **The interpreter and compiled code give the same digits.** The math functions come from the C
     library in both (Rust's `f64::sin` calls the same `sin`), so on one machine they agree to the last
     digit; on another machine `sin` and the others may differ in the last digit, as in every language
     that uses the C library (the arithmetic, `sqrt` and `fusedMultiplyAdd` are correctly rounded
     everywhere). `parse` is correctly rounded in both: in the interpreter by Rust's own reader; in
     compiled code a number whose digits fit in 53 bits (24 for a `float`) with a power of ten up to
     10^22 (10^10) is one correctly rounded multiplication or division (Clinger's fast path, which Rust
     takes too), and any other goes to `strtod` or `strtof`. The two were compared on 8000 random texts,
     with both C compilers.
165. **The measurement is `numbers`** (above): a million lines, whole numbers and fractional ones in
     turn, read ten times over with `long.parse` and `double.parse`, against `strtoll` and `strtod`.
166. **A short text made by `toString` or `$"...".toString()` is written once**: into a buffer on the
     stack, then copied into its own block. Before, every such text was written twice, once to find its
     length. Found by the measurement of 165; it changes no output.

Choices made in the thirty-sixth slice (text: `char`, going over a text, trimming it and looking in it;
checker, interpreter and back end together). The design had decided `char` (32 bits, the type a character
constant takes where it is used, `text.chars()`) and wrote `text.split(' ')`, `text.lines()`, `trim()` and
`parts.next()` in its examples; the rest was approved on 13 November 2026:

167. **A character constant with nothing around it to decide is a `char`**: `var c = 'é'`, and `print('+')`
     writes `+`. Where an integer goes it is still a `byte` (`byte b = '+'`, `text[i] == ':'`, `int x = '+'`
     is 43), and one that takes more than one byte is an error there, hint `char`. This changes the design's
     "`var v = '+'` gives a `byte`" (the `char` row of the decisions) and rule 2 of the sixth slice: with
     `char` there, a `byte` by default made `print('+')` write 43 and `var separator = ','` a number. C# and
     Rust give a `char`; only C gives a number.
168. **A `char` is not a number.** `c + 1` is an error (note: `(int)c` is the code, `(char)n` the
     character), so is `c == 65` (hint `(int)c` or `(char)`), and a `char` does not widen: `int x = c` is
     an error whose hint is `(int)c`. A cast goes both ways, and only between `char` and the integer types.
     Rust's way; C# adds a `char` like a number, which is how `'a' + 1` became 98.
169. **`(char)n` panics in every build** when no character has the code (below 0, past 0x10FFFF, or
     0xD800 to 0xDFFF), with the hint `n.tryToChar()`, which gives `null`. It is not the overflow rule: a
     bad code written into a text would break the promise that a `string` is valid UTF-8, so the check stays
     in a release build, as the bounds check does. A code known in the program is a compile error.
     `(byte)c` and the other types narrower than the code follow the integer rule (a panic in a debug build,
     wrapping in a release one), hint `(int)c`; `(int)c` cannot fail.
170. **`char` is `Copyable`, `Equatable`, `Hashable`, `Comparable` and `Formattable`**: `==` and `<` by
     code, a key of a map, `sort`, and `match c { 'a' => ... }` (the arms are characters; a number there is
     an error whose hint is the character). `print(c)` writes the character; inside a collection or a
     struct, and in an `assert` message, it is in single quotes with the escapes of a constant: `['a',
     '\n']`, as text is in double quotes there.
171. **`\u{...}` writes a character by its code**, in a text and in a character constant: `'\u{e9}'`,
     `"\u{1F600}"`, one to six hex digits (Rust's and Swift's form). It is how `print` already wrote a
     control character inside a collection, so what is printed reads back. C#'s `\uXXXX` cannot write a
     character past 0xFFFF in one escape.
172. **ASCII methods on `char` and on `byte`**: `isAscii`, `isAsciiDigit`, `isAsciiLetter`,
     `isAsciiAlphanumeric`, `isAsciiUpper`, `isAsciiLower`, `isAsciiWhitespace` (space, `\t`, `\n`, form
     feed, `\r`), `toAsciiUpper`, `toAsciiLower`: Rust's set, and a parser that goes byte by byte
     (`text[i].isAsciiDigit()`) needs no `char`. The tests that need Unicode tables are not here: C#'s
     `char.IsDigit` says yes to the digits of many scripts (`'٣'` among them, which `parse` does not
     read), and `ToUpper` depends on the culture; `isDigit`,
     `toUpper` and the like get the ASCII method as a hint. On `char` only: `isWhitespace` (Unicode's
     White_Space, 25 characters: what `trim` takes off) and `utf8Length` (1 to 4).
     (The Unicode tests and `toUpper` came with the tables, 637 and 638; `isDigit` still gets the hint.)
173. **Going over a text: `text.chars()`, `text.split(sep)`, `text.splitWhitespace()`, `text.lines()`.**
     `chars` gives each `char`; the others give each part as a `read string`, a look at the text where it
     is. `split` keeps the empty parts (`"a,,b"` gives three, `""` gives one), as Rust and C# do;
     `splitWhitespace` skips them (Rust's `split_whitespace`); `lines` ends a line at `\n` and leaves out a
     `\r` right before it, and a last `\n` does not start an empty line (Rust's `lines`). The separator is a
     `char` or a `read string`. A `byte` is an error whose hint is `(char)b`, because a byte past ASCII would
     cut characters in half; an empty separator is an error in the program and a panic at run time.
174. **What they give is an iterator: `for` goes over it, `it.next()` gives the next part or `null`**
     (`char?`, `read string?`), as the design's examples call `parts.next()`. It is not copied (`var b = a`
     moves it), `for` uses it up, and a `for` over `read it` is an error. Its type has a name in messages
     (`Split`, `Chars`, `SplitWhitespace`, `Lines`) but cannot be written yet, so it lives in a `var` or a
     `for`, not in a field or a parameter: it carries a borrow of the text, and a built-in type that does
     comes with generics (the `for` pattern of the design, `Iterator<T>`).
175. **A part borrows the text, not the iterator**: after `var name = parts.next()` the `parts` goes on,
     and `name` keeps the text from changing for as long as it is used (the rule of a `from source`
     method). While a `for` goes over a text, the text cannot be changed or moved; `split` keeps its
     separator too. An iterator over a text with no name (`make().split(' ')`) is an error whose hint is to
     name the text, because it would be gone before the loop starts.
176. **`trim`, `trimStart` and `trimEnd` give a `read string`**, nothing copied, and take off Unicode's
     White_Space at the ends, as Rust and C# do: `"\u{3000}x\t".trim()` is `x`. `.clone()` owns it.
177. **Looking in a text**: `indexOf` and `lastIndexOf` (a `nint?`, in bytes, because `text[a..b]` takes
     bytes), `contains`, `startsWith`, `endsWith`, each with a `char` or a `read string`; `indexOf` and
     `lastIndexOf` also with a `byte`, as `text.indexOf(':')` worked before. An empty text is found at 0,
     and by `lastIndexOf` at the length, as in Rust.
178. **`equalsIgnoreAsciiCase`, not the design's `equalsIgnoreCase`**: only the ASCII letters are folded,
     which is what the name of a protocol field (`Content-Length`) needs, with one answer everywhere;
     `"É".equalsIgnoreAsciiCase("é")` is false. Ignoring the case of any text needs Unicode tables.
     The design's HTTP example changes in that one word. (`equalsIgnoreCase`, which folds every letter,
     came with the tables, 641.)
179. **`try` on a `read T?` gives a `read T`**, as `??` already did: `var level = try parts.next()`, `var
     name = try names.get(1)`, in a function that returns a `T?` or a `read T?`. It was an error ("takes the
     value out"); nothing is taken out, the borrow is passed on. With it the log summary example parses its
     lines as the design wrote them: `struct Entry<from line>`, `split`, `try parts.next()`, `int.parse`.
180. **Compiled code**: a `char` is a `uint32_t`; an iterator a small struct (the text, where it is, the
     separator; a separator of one character is kept inside it, so that the iterator can be moved), and its
     `next` a function the C compiler inlines; a part is a pointer and a length into the text. `trim`,
     `chars` and `splitWhitespace` read the characters out of the UTF-8; the searches are `memchr` and
     `memcmp`. Compared with the interpreter on 400 random texts (every kind of white space, `\r\n`,
     characters of two to four bytes, separators of one and two bytes), with both C compilers.
181. **The measurement is `fields`** (above): a million lines like ` Ada17 , 120, -45 ,6789`, gone over
     twenty times with `lines`, `split`, `trim`, `long.parse` and `chars`, against C written by hand with
     `memchr` and `strtoll`.

Choices made in the thirty-seventh slice (building a text piece by piece; checker, interpreter and back end
together), approved on 13 November 2026:

182. **A `string` is what a text is built in: `text.write(x)` adds at its end.** There is no builder type
     beside it. A `string` already is a pointer, a length and a capacity (7), which is all a builder is,
     and the finished text needs no copy at the end (C#'s `sb.ToString()`). Rust's `String` is its own
     builder too (`push_str`, `write!`); C# and Java keep an immutable `string` and a `StringBuilder`, Go a
     `strings.Builder`.
183. **`write` takes what `print` takes**: a `$"..."`, a number, a `bool`, a `char`, a text, a
     `Formattable` value, a collection. It is the name and the rule of `f.write` inside `format`, so there
     is one way to add to a text. A `$"..."` is written straight into the block, as `print` writes it: no
     text is made for it. Rust has `push`, `push_str` and `write!`; C# `Append` with an overload per type.
184. **`write` and `clear` need the text where it may change**: a variable, a `ref string` parameter, a
     field reached through `ref`, an item of a list, a value in a map, a text behind a `T*`. Through `read`
     they are an error whose hint is `ref string`, and a `readonly` field cannot be written. They are
     `ref this` methods, so the borrow rules hold as they are: a part kept from the text (`var head =
     s[..3]`) and a `for` over it keep it from being written.
185. **A text is not written into itself**: `s.write(s)`, `s.write(s[..3])`, `s.write(s.trim())`, and a
     value that borrows from `s`, are errors whose hint is `.clone()`, because the text may move to a
     larger block while it grows and what is written would be read from the old one. A number, a `bool` or
     a `char` computed from it is fine (`s.write($"{s.length()}")`): the values of a line are computed
     before anything is written (55). Rust's rule; C#'s `Append(sb)` copies first, which a block that moves
     in the middle of the copy cannot do.
186. **`text.clear()` empties the text and keeps its block**, so a line that is built again in every round
     of a loop takes memory once. Rust's `clear`, C#'s `Clear()`.
187. **`string.withCapacity(n)`** is an empty text with room for `n` bytes, so that a text whose size is
     known grows no more than once (Rust's `String::with_capacity`, C#'s `new StringBuilder(n)`). `0` takes
     no memory. A negative `n` panics in every build, and so does an `n` the memory cannot hold, with the
     size in the message. Without it the block starts at 32 bytes and doubles when it is full.
188. **`text += "x"` stays an error**, now with the hint `text.write("x")`; `a + b` keeps its hint
     `$"{a}{b}"`, and `StringBuilder`, `append` and `push` get the hint `write` too. One way to add to a
     text: a `+=` that wrote in place would be a second spelling of `write`, and `+` would make a new text
     every time (in C# that cost is why `StringBuilder` exists).
189. **A constant is copied on the first write**: `var line = ""` takes no memory until something is
     written, and the text of the program is never written. In compiled code a constant is a text with no
     capacity (8); the first write that needs room takes a block and copies the bytes over. The
     interpreter keeps a text that nothing else holds and grows it where it is.
190. **Compiled code**: `write` is the code of `f.write`: each piece is put straight into the block (one
     `vsnprintf` for the numbers and texts of a `$"..."`, a helper for a struct, an enum or a list), and
     `clear` sets the length to 0. The `vsnprintf` now writes into the block itself where it has room,
     instead of into a small buffer that is then copied; that is also what `print` and `f.write` use.
     The place is found before the values of the line are computed, as in the interpreter. Compared with
     the interpreter with both C compilers, in a debug and a release build.
191. **The measurement**: `numbers` and `fields` (above) now build their text with `write` into a block
     taken at the start, as the C programs do with `sprintf`, where they made a text for every line and
     joined them. Building takes the time of C (0.056 s against 0.055 s, 0.149 s against 0.145 s; it was
     0.071 s and 0.17 s), and "Building a text out of many small ones" is gone from "Slower than C".

Choices made in the thirty-eighth slice (files, the input and the command line; checker, interpreter and
back end together), approved on 13 November 2026:

192. **A file is read or written whole**: `File.readText(path)` gives a `Result<string, IoError>`,
     `File.readBytes(path)` a `Result<List<byte>, IoError>`; `File.writeText(path, text)` and
     `File.writeBytes(path, bytes)` make the file, or empty it, and write; `File.appendText(path, text)`
     writes after what the file holds and makes one that is missing. The names follow the design's
     `File.readText` (C#'s `File.ReadAllText` without the `All`); Rust has `fs::read_to_string` and
     `fs::write`, Go `os.ReadFile` and `os.WriteFile`. A file read piece by piece (`File.openText`,
     `File.createText`, the row "File API") needs the `Closable` check and comes in a slice of its own.
193. **A path is a `read string`**: a constant, a variable lent with `read`, or `read $"...".toString()`.
     There is no path type: a path is text, as in C# and Go (Rust's `Path` can hold names that are not
     UTF-8, which a `string` cannot). A path with a zero byte in it names no file, because the system
     would read the name as ending there (`"a.txt\0.png"` would open `a.txt`); it gives `NotFound`, and
     nothing is cut off.
194. **`readText` checks the text**: a byte that is not UTF-8 gives `IoError.InvalidText(position)`, the
     place counted from the start of the file (the row "`string` guarantee"). Raw data is read with
     `readBytes`, and `string.fromUtf8` looks at bytes as text where they are.
195. **`IoError` is a built-in enum**: `NotFound`, `PermissionDenied`, `IsDirectory`, `NoSpace`,
     `InvalidText(long position)`, `Other(int code)`; `Copyable`, `Formattable` and `Equatable`, like
     `NumberError`. The first four are what a program tells apart (make a default when the file is
     missing, tell the user it may not be read); `Other` keeps the code the system gave. The codes map the
     same on Linux and macOS: `ENOENT` and `ENOTDIR` are `NotFound`, `EPERM`, `EACCES` and `EROFS`
     `PermissionDenied`, `EISDIR` `IsDirectory`, `ENOSPC` and `EDQUOT` `NoSpace`. The error does not carry
     the path: that would make it own a text and stop being `Copyable`, and the caller has the path to put
     in its own error (as `wc` does). Rust's `io::Error` leaves the path out too; Go's `PathError` and
     C#'s message keep it. `print` and an error of `main` write the variant: `NotFound`, `InvalidText(12)`.
196. **`main(List<string> args)`** gets the words after the program's name; `void main()` stays, and so does
     `Result<void, E> main(...)`. The program's own name is not among them, as in C# (`args[0]` is the
     first word); C, Rust and Go put the name first. The list is the program's own: it can change it and
     move its texts. In compiled code each text looks at the word where the system keeps it (a text of
     capacity 0, 8), and nothing is copied unless the program writes into it. A word that is not UTF-8
     stops the program before `main` with `error: args[1] is not valid UTF-8` and exit code 1, as Rust's
     `env::args` stops; a form that gives the words as bytes can come when a program needs it.
197. **`Stdin.readLine(ref line)`** puts the next line of the input into `line`, without its `\n` or
     `\r\n`, and gives a `Result<bool, IoError>` that is `false` at the end of the input. The same text
     is used for every line, so a loop takes memory once (the design's `reader.readLine(ref line)`, row
     "File API"), and a part of the line kept past the next `readLine` is the borrow error of the log
     summary in 05. A line that is not UTF-8 is `InvalidText` with its place counted from the start of
     the input; that line is used up and `line` is left empty, so the next call goes on with the next line
     and no loop meets the same error forever. `Stdin.readText()` gives what is left of the input. On a
     terminal what was printed goes out before the program waits for a line, so a prompt shows; on a
     pipe the output keeps its buffer. The name is `Stdin`, as in C (`stdin`), Go (`os.Stdin`), Rust
     (`io::stdin()`) and Zig (`getStdIn`), not C#'s `Console`: the input is often a pipe or a file and not
     a console, and once files are read piece by piece `Stdin` becomes a reader of the same type that
     `File.openText` gives, so `readLine`, `lines()` and `readText()` are one set of methods for both.
     `Stdout` and `Stderr` are kept for the writing side (using them now is an error whose hint is
     `print` or `printError`); the row "`print` error" and 02, which wrote
     `Console.out`, now write `Stdout` (chosen on 13 November).
198. **`printError(x)`** writes as `print` does, to the standard error, and gives nothing: there is no one
     to tell that the standard error cannot be written. What `print` wrote before goes out first, so on a
     terminal the two come in the order they were written. Rust's `eprintln!`, C#'s
     `Console.Error.WriteLine`. `Stdout` and `Stderr` with a `Result` (02, "`print` does not
     return an error") come with the writers of 192. In a test, what `printError` wrote is shown under its
     `FAIL` line and dropped when the test passes, as compiled tests already did.
199. **`exit(code)`** ends the program at once with that code, after what was printed goes out; nothing is
     freed (the system takes the memory back) and nothing after it runs. The code is a `byte`: the system
     keeps eight bits, so `exit(256)` would end as 0; here it is a compile error, and an `int` needs
     `(byte)`, which panics when it does not fit. `exit` gives no value, so it stands where one is needed
     (`int n = x ?? exit(2)`) and a function that ends in it needs no `return`. In a test it fails the
     test: a test cannot end the run of the others, and with each compiled test in a process of its own
     `exit(0)` would make it pass. Rust's `process::exit`, Go's `os.Exit`, C#'s `Environment.Exit`.
200. **The command line of the interpreter**: `sg file.sg a b c` runs the program with the words `a b c`;
     `sg test` and `sg build` are as they were. A compiled program gets its words from the system.
201. **Compiled code** uses the calls of the system (`open`, `read`, `write`, `fstat`), not C's `FILE*`: a
     file is read into one block of its size, and the input in blocks of 64 KB that the program keeps,
     where a line is found with `memchr` and copied once into the text. This part of the runtime is put
     only into a program that uses it. Compared with the interpreter with both C compilers, and on random
     inputs of several megabytes with bad bytes and long lines across the blocks.
202. **Checking UTF-8 looks at eight bytes at a time** while none of them has its high bit set, and at one
     byte at a time from there; `readText`, `readLine`, the command line and `string.fromUtf8` all use it.
     It took the `lines` program below from about 1.15 to 1.08 times the time of C.
203. **The measurement**: `lines` (above) reads two million lines of a log, 125 MB, with
     `Stdin.readLine`, and counts them, their bytes, the longest and the ones that start with `ERROR`.
     It takes 1.08 times the time of `getline` in C, which does not look at what it reads. Without the
     check the SG program takes 0.058 s against 0.090 s: the reading is faster than `getline`, and the
     difference is the guarantee. A new entry in "Slower than C".
204. **Tests**: a `.args` file next to a program gives its command line, one word on each line, `{dir}`
     being an empty folder of its own; a `.in` file gives its input. What a program wrote to the standard
     error comes after what it printed, and an exit code other than 0 and 1 is a last line of the output
     (`exit code 3`). The example `wc` counts the files named on its command line.

Choices made in the thirty-ninth slice (files read and written piece by piece; checker, interpreter and
back end together), approved on 13 November 2026:

205. **`StreamReader` and `StreamWriter` are built-in types** that a program writes: a parameter
     (`ref StreamReader from`), a field, an item of a list, a `T?`, a return value. They are moved, never
     copied, and are not `Formattable` or `Equatable`. The names are the design's (row "File API", C#'s);
     Rust has `BufReader<File>` and `BufWriter<File>`, Go `bufio.Reader` and `bufio.Writer`.
206. **`File.openText(path)`** gives a `Result<StreamReader, IoError>`, and **`File.createText(path)`** a
     `Result<StreamWriter, IoError>` into a file that is made, or emptied. A folder opens on Linux and
     macOS and fails at the first read; here `openText` tells `IsDirectory` at once, in both. Writing
     piece by piece after what a file holds is not here yet (`File.appendText` adds a whole text); since
     the fortieth slice it is `File.openAppend` (216).
207. **A reader** has the methods of `Stdin` (197): `readLine(ref line)` into one text, `readText()` for
     what is left, and **`lines()`**, which gives each line as a text of its own, a
     `Result<string, IoError>`, and ends after the first error (row "File API": no loop meets the same
     error forever). `for item in reader.lines()` goes over them, and `next()` gives a
     `Result<string, IoError>?`. While `lines()` is used it reads through the reader, so the reader
     cannot be used beside it (as a text and its parts, 172). A reader has no `close`: reading loses
     nothing, and its file is closed when it ends.
208. **A writer** has `write(x)` and `writeLine(x)`, which take what `print` takes (numbers, text,
     `$"..."`, a `Formattable` struct, a list), `flush()`, and `close()`, which takes the writer (row "Close
     error"); each gives a `Result<void, IoError>`, and a `Result` is never dropped silently. Writing goes
     on from where the writer is, so it needs the writer as `ref`; through `read` it is an error, and
     `close` through any borrow is one too, since whoever owns a writer closes it.
209. **The buffer is 64 KB, in the interpreter and in compiled code**: a write that brings it to 64 KB
     writes it out, `flush` and `close` write what is left. A program that reads its own file back before
     a `flush` sees the same bytes in both. **The first error is kept**: every later `write`, `flush` and
     `close` gives it again, as C's `ferror` keeps it, so a `close` reports a full disk even when the
     writes before it were dropped with `_ =`.
210. **The `Closable` check** (row "Close error"): a `StreamWriter` kept in a local variable, a parameter
     taken by value, a name of a pattern or the item of a `for` must be closed, or moved somewhere, on
     every path before it ends: at the end of its block, at `return`, `break`, `continue` and `exit`, and
     before the variable gets another writer. Closed on one path of an `if` and not on the other is an
     error of its own ("closed on some paths but not on all"). The hint is `try w.close()`, or
     `_ = w.close()` to drop the error on purpose. Not followed, as the design says: a writer in a field,
     a list or a `T?`; it writes its buffer when it ends, and its error is lost.
211. **An exit with an error lets a writer go**: through `try`, as the design wrote, and also through
     `return Error(...)`, `?? return Error(...)` among them. The caller gets the error that made the
     function stop; a second one from closing would not be read, and the writer writes its buffer as it
     ends. Without this a function would have to close every writer by hand before each `return Error`.
     New beside the design's text.
212. **`exit` is checked, a panic is not**: `exit` ends the program at once and nothing runs after it
     (199), so a writer still open there would lose its buffer, and that is an error where the checker
     can see it (a local). A panic is a bug and does not unwind (row "Copy and cleanup"): what a writer
     holds is lost. The interpreter does the same: a writer that ends while the program stops with a
     panic or `exit` writes nothing.
213. **`Stdin`, `Stdout` and `Stderr`** are the streams of the program, which every function and test has
     as if the program had lent them as `ref` parameters: `Stdin` is a `StreamReader`, `Stdout` and
     `Stderr` are `StreamWriter`s. A function that reads a file or the input, or writes a file or the
     output, takes a `ref StreamReader` or a `ref StreamWriter` and is given `ref Stdin` or `ref Stdout`.
     They are never moved, given another value or closed (the hint for `close` is `flush`), and no
     variable or parameter may have their names. `Stdout` writes where `print` writes, in the order they
     were written; `Stderr` where `printError` writes, after what was printed (in a test, under its `FAIL`
     line). This is what 197 kept the names for. `print` and `printError` stay as they are, giving
     nothing (02, "`print` does not return an error"). Go has `os.Stdin` as a variable, Rust `io::stdin()`
     as a call, C# `Console.In` and `Console.Out`.
214. **Compiled code**: a reader is a block of 64 KB with its file, read with the calls of the system as
     the input was (201), and `Stdin` is the same struct over the input; a writer is a text that serves as
     its block, with its file, written with `write`, and `writer.write(x)` is the code of `text.write(x)`
     into that block. `Stdout` and `Stderr` go through C's `stdout` and `stderr`, so that they keep their
     order with `print`. Compared with the interpreter with both C compilers, in a debug and a release
     build, and on eight random files of up to 180 MB with bad bytes, `\r\n` and lines longer than a
     block.
215. **The measurement and the example**: `copy` (above) reads the log of `lines` from a file and writes
     every line with its number into another; it takes 0.93 times the time of `getline` and `fprintf` in
     C. The log summary is now the design's (05): it reads its log with `File.openText` and `readLine`,
     writes its report with `File.createText` and `close`, and `try` turns an `IoError` into the
     `LogError` variant that carries it.

Choices made in the fortieth slice (a writer that appends, and bytes read and written piece by piece;
checker, interpreter and back end together), approved on 14 November 2026:

216. **`File.openAppend(path)`** gives a `Result<StreamWriter, IoError>` that writes after what the file
     holds; a missing file is made, and a folder is `IsDirectory`. The writer is a `StreamWriter` like any
     other: `write`, `writeLine`, `flush`, `close`, and the `Closable` check. The file is opened to append
     (`O_APPEND`), so every block lands at the end even when another program writes to it too. The name:
     C#'s `File.AppendText(path)` is this stream, but here `File.appendText(path, text)` is the whole write
     (192 left out C#'s `All`), and one name cannot mean both in a language without overloading. Go opens
     with `os.OpenFile(path, O_APPEND|O_CREATE|O_WRONLY, ...)`, Rust with
     `OpenOptions::new().append(true).create(true)`.
217. **`reader.readBytes(ref buffer)`** puts bytes at the start of the buffer and gives how many, a
     `Result<nint, IoError>`. It fills the buffer unless the end comes first, so fewer than the buffer
     holds means the end was met, and the next call gives 0. Rust's `read` and Go's `Read` may give fewer
     at any time, and a program that needs the buffer full loops (`read_exact`, `io.ReadFull`); here there
     is one meaning. Cost: on a pipe or a terminal it waits until the buffer is full or the input ends; a
     read that gives what is ready, for the network, is an open question in 04. An
     empty buffer gives 0 and reads nothing. The buffer is any `ref byte[]`: `ref buffer` for an array or a
     list, `ref xs[a..b]` for a part of one; lent with `read` it is an error whose hint is `ref`.
218. **`writer.writeBytes(bytes)`** writes a `read byte[]` as it is, through the writer's buffer, so it
     keeps its order with `write` and `writeLine`, and on `Stdout` and `Stderr` with `print` and
     `printError`. It gives a `Result<void, IoError>`, and the writer's first error is kept as for the
     others (209). Without it, bytes read piece by piece could only be written whole.
219. **No type and no opening function of their own**: bytes are read through the `StreamReader` that
     `File.openText` and `Stdin` give, and written through the `StreamWriter` that `File.createText`,
     `File.openAppend`, `Stdout` and `Stderr` give. The name `openText` says text, for a file of bytes
     too; a second function (`File.openBytes`) that gave the same reader would be two ways to do one
     thing, and a type of bytes beside the reader would be a second set of methods for one file. Rust has
     one `File` under `BufReader` for both; C# has a `FileStream` beside the `StreamReader`.
220. **Bytes and lines from one reader** share its block: `readLine` starts where `readBytes` stopped, and
     the other way round, and the position of an `InvalidText` counts the bytes read as bytes too. While
     `lines()` reads through the reader, `readBytes` cannot be called beside it (207).
221. **Compiled code**: `readBytes` takes what the block holds first, and a rest of 64 KB or more is read
     straight into the buffer, without passing through the block; `writeBytes` of 64 KB or more writes
     the block and then the bytes straight. `Stdout` and `Stderr` write the bytes through C's `stdout` and
     `stderr`. The interpreter reads through a buffer of 64 KB too; a program sees the same bytes and the
     same counts in both.
222. **The measurement and the tests**: `bytes` (above) copies the log of `lines` in pieces of 4 KB into a
     writer from `File.openAppend`; it takes 0.94 times the time of `fread` and `fwrite` in C. The tests
     write bytes and text into one file, read them back in pieces, into an array, a list and a range of
     one, mix `readBytes` with `readLine`, append to a file that is there and to one that is not, write
     70 000 bytes at once, and read and write the bytes of `Stdin`, `Stdout` and `Stderr`; five tests of
     errors.

Choices made in the forty-first slice (writing a number: a width and a letter inside `$"..."`, `parseHex`;
checker, interpreter and back end together), approved on 14 November 2026:

223. **A width and a letter after the value, as in C#**: `{value,width:letter}`, `{n,5}`, `{b:X2}`,
     `{price,8:F2}`. The design had left "a format specifier inside `$"..."` (`{x:N2}`)" for after the
     first version (04, "Deliberately deferred"); it is taken up now, because a number could only be
     written in decimal and the design's own `toHex` example (05) had to write hex by hand. The form is
     C#'s since `$"..."` is C#'s; Rust writes `{:>8.2}`, C and Go `%8.2f`. It is checked at compile
     time and costs nothing at run time: the width and the letter are part of the program, not text read
     while it runs.
224. **Five letters**: `x` and `X` hex with small or capital letters, `b` binary, `D` decimal, `F` digits
     after the point. As in C#, only `x` and `X` differ by case (`d`, `f` and `B` are the same letters).
     Left out: `N` (the separators between groups of digits change from one country to another; the error
     says so and hints `F2`), `E`, `G`, `C`, `P` and C#'s formats made of characters (`{x:0.00}`); any
     other letter is an error that lists the five.
225. **The number after the letter** is the digits there are at least for `x`, `X`, `b` and `D`, filled
     with zeros (`{7:D3}` is `007`, `{-7:D3}` is `-007`: the sign stands in front of the digits, as in C#),
     and the digits after the point for `F`, which must have one: `F` alone is an error whose hint is `F2`
     (C# takes its default from the country settings). `{0:x0}` and `{0:D}` write `0`. The number is at
     most 999, and nothing may follow it.
226. **A negative integer in hex and in binary is the bits of its type**: `{small:x}` with `sbyte small =
     -1` is `ff`, with an `int` it is `ffffffff`; C# and Rust write the same. No `0x` is put in front; the
     text has it where it is wanted (`0x{n:x}`).
227. **A letter fits the value or the program does not compile**: `x`, `X`, `b` and `D` take an integer of
     any type, through a `read` and behind a `T*` as `print` looks; `F` takes a `float` or a `double`. The
     errors have hints: `{(double)n:F2}` for an integer, `{(int)c:X4}` for a character, `F0` for `D` on a
     fractional number. A text, a `char`, a `bool`, a struct, an enum and a collection take a width only.
228. **`F` rounds the exact value to the nearest, a tie to the even digit**: `{2.5:F0}` is `2`,
     `{0.125:F2}` is `0.12`, `{1.005:F2}` is `1.00` (the `double` is a little below 1.005). Rust, C's
     `printf` and .NET since 3.0 give the same digits. `NaN` and the infinities are written as `print`
     writes them; `-0.0` and a small negative number keep their minus (`-0.00`), as in all three.
229. **A width counts characters**, not bytes: `{"Zürich",8}` has two spaces in front. A positive width
     puts the spaces in front (the value stands to the right), a negative one after it; a longer value is
     not cut (C#). It works on every value `print` takes, a struct with its own `format` and a list
     included. The fill is a space only (zeros come from the digits of `D`, `x` and `b`), there is no
     centring, and the width is a number written in the text, at most 999: `{n,width}` is an error with a
     note, as C# also asks for a constant. There is no `padLeft`: `$"{name,10}".toString()` is that text.
230. **Where `,` and `:` belong**: inside `{ }`, a `:` outside the value's own brackets starts the letter,
     and a `,` outside them starts the width when a number follows it. No expression has a `:`, and a `,`
     outside brackets is otherwise part of a type (`HashMap<string, int>`), which stays as it was. A
     character constant (`{c == ':'}`) and text inside the braces are skipped.
231. **`T.parseHex(text)`** on every integer type gives a `Result<T, NumberError>`: small and capital
     digits, no `0x` (`"0x10"` is `Invalid(1)`), the sign rule of `parse` (`"-1f"` is -31 on a signed type,
     `"-1"` on an unsigned one is `Invalid(0)`), and the value must fit the type: `"80000000"` on an `int` is
     `OutOfRange`, where C#'s `HexNumber` would give -2147483648 (Rust's `from_str_radix` agrees with SG).
     Not `parse(text, 16)` (a parameter and a check more for a base that is almost always 16) and not `0x`
     read by `parse` (it would change what `"0x10"` gives today). On a fractional type it is an error
     whose hint is `parse`.
232. **Compiled code**: hex and decimal digits are `printf`'s, with the digits there are at least as its
     precision (`%.8llx`, `%.3lld`) and a width in the same conversion (`%8lld`). Binary digits are put
     together by a small function. `F` has a function of its own: the `double` is `m` times a power of two
     exactly, `m` times ten to the digits is held in 128 bits, and the rounding is exact; past 18 digits,
     or for a number too large or too small for that, it calls `snprintf`. It was checked against
     `snprintf` on twenty million random values, ties included, with no difference. A width on text, a
     character or what a helper writes is one `%*.*s` whose width in bytes is worked out by counting the
     characters; a struct written straight into a text (`f.write`) is padded where it lands.
233. **The measurement and the tests**: `table` (above) takes 0.89 times the time of `printf` in C. The
     tests write every letter on every integer type, negative numbers, zero, the largest and smallest
     values, ties and `NaN`, widths on numbers, text with an `é` and a `ü`, characters, `bool`, structs
     with and without their own `format`, enums and lists, in `print`, in `f.write`, in `text.write`, in
     `Stdout.writeLine` and in `.toString()`, and `parseHex` on fourteen texts; fourteen tests
     of errors. A program that writes 780 000 fractional numbers with eleven letters gives the same text
     interpreted and compiled, with both C compilers.

Choices made in the forty-second slice (`list.addAll(slice)`, and `sort` and `reverse` on an array and through
a `ref T[]`; checker, interpreter and back end together), approved on 14 November 2026:

234. **`list.addAll(items)` takes a `read T[]`**: whatever a slice is made from, `read ys` for a list or
     an array, a range (`ys[2..]`), `text.asBytes()`. The design's HTTP example (05) already writes
     `buffer.addAll("...".asBytes())`, and the name is the design's. C# calls it `AddRange`, Rust
     `extend_from_slice`, Go `append(xs, ys...)`; those names and `extend`, `pushAll`, `appendAll` and
     `concat` are errors whose hint is `addAll`. A list given without `read` is the error any slice
     parameter gives (hint `read ys`).
235. **The items must be `Copyable`**: numbers, `bool`, `char`, and structs and enums written
     `: Copyable`. A copy of their bytes is then a copy of them, so `addAll` is one `memcpy`. On a
     `List<string>` or a list of structs that own memory it is an error: each text would be copied into a
     block of its own, an allocation per item that the call does not show, and moving them instead would
     empty a source that is only lent with `read`. The hint is `names.add(item.clone())` for text and
     `: Copyable` for a struct. Moving every item of one list to the end of another (Rust's `append`) would
     be a method of its own and is not here yet. Items that carry a borrow are refused ("not in the
     language yet"); a list cannot hold them yet anyway.
236. **A list is not added to itself**: `xs.addAll(read xs)` and `xs.addAll(xs[1..])` are compile errors,
     the rule of `s.write(s)` (choice 185): the list may move to a larger block while it grows, and the
     items would be read from the old one. The hint is `xs.add(xs[i])`, which copies each item before the
     list grows. Rust refuses the same and has `extend_from_within(range)` for it; that can come when a
     program needs it (a decoder that copies from what it already wrote, as `deflate` does; the LZW of GIF
     does not).
237. **`sort` and `reverse` on an array and through a `ref T[]`**: the items change places where they are,
     in the list or the array the slice looks at. The number of items stays the same, which is why these
     two come and `add` or `clear` do not. Through a `read T[]` it is an error whose hint is `ref xs` or
     `ref xs[a..b]`; `sort` asks for `Comparable` items, as on a list.
238. **A range in front of `sort` or `reverse` is lent as `ref`**: `xs[2..5].sort()` puts those items of
     `xs` in order, as `ref xs[2..5]` would, without the brackets `(ref xs[2..5]).sort()` would need. A
     variable in front of a method is lent as the method needs, with nothing written (`xs.sort()`), and a
     range in front of these two follows that rule; a bare range anywhere else still only looks (choice
     90). It needs what `ref xs[a..b]` needs: `xs` can be written, is not `readonly`, is not borrowed and
     is not being gone over by a `for`. Rust writes `xs[2..5].sort()` too.
239. **The same sort as a list's**: a merge sort that keeps equal items in the order they had. In compiled
     code it takes a block of the slice's size from the heap while it sorts, as a list's sort does, so
     sorting an array on the stack allocates. A sort that does not (an insertion sort for a few items, or
     a merge in place) can come when a measurement asks for it.
240. **Compiled code**: `addAll` tests for room once, grows the block to twice its room or to what is needed
     if that is more, and copies with `memcpy`; an empty slice does nothing. `sort` on a slice is the
     list's `sg_sort` on the slice's pointer and count, with the same function that compares two items
     (now one per item type, for lists and slices alike); `reverse` swaps from both ends.
241. **The measurement and the tests**: `blocks` (above) takes the time of `memcpy` in C (1.00); adding the
     same bytes one at a time with `add` takes ten times as long. The tests add text bytes, a list, a range,
     an array, an empty slice and `Copyable` structs (copies, which stay as they were when the source
     changes), five thousand pieces one call at a time and the result in one call; they sort and turn
     around ranges of lists, slices given as `ref T[]` parameters and kept in a variable, arrays, an array
     field and one behind a `T*`, text, a `Comparable` struct with equal keys (the order is kept), one item
     and none; fifteen tests of errors.

Choices made in the forty-third slice (patterns inside patterns, `match` on text, methods on enums;
checker, interpreter and back end together), approved on 14 November 2026:

242. **A pattern stands where a name stands**: inside the parentheses of a variant goes a variant with its
     own parentheses (`Add(Number(0), right)`), a variant that carries nothing (`Paint(Red)`), a number,
     a character, a text (`Word("let")`), `_` or a name, as deep as wanted, in `match` and in `is`. Through
     a `T*` the pattern looks into the box, as `match` and `is` already do. The design had put "nested
     patterns" among the things after the first version; they cost nothing at run time (the arm tests what
     a person would test by hand) and the checker gains the search of 244. C#, Rust and Swift have them.
243. **A name alone inside the parentheses**: it is the variant when the enum of that value has a variant
     of that name, otherwise a name that takes the value. A name that starts with a capital letter and is
     not a variant there is an error (`Full(Empy)`: hint `Empty`; `Number(Empty)` on an `int`), so that a
     mistyped variant does not quietly take every value (the "Patterns" row of the design). Variants
     begin with a capital letter and names with a small one, as everywhere in SG; the rule leans on that
     only here. Rust has the same question and answers it with a warning about the case of the name. `var`
     is not written inside the parentheses (an error whose hint is the name alone).
244. **What a `match` misses is named**: with patterns inside, a `match` that does not take every value is a
     compile error that names one it misses (`this match does not handle Add(Name(_), _)`), and an arm
     that the arms above take together is one too ("the arms above take every value it matches"). The
     messages of a `match` without patterns inside stay as they were: a variant with no arm at all is
     listed by name, and an arm whose variant an arm above settled is "already handled above". The search
     is Maranget's (2007), the one Rust uses, and runs in the checker only. An arm with `when` takes
     nothing for the search, as before.
245. **An arm with a pattern inside does not settle its variant**: after `Add(Number(a), b)` an arm for
     `Add(l, r)` still runs for the other sums; after `Add(l, r)` an arm `Add(Number(a), b)` is
     "already handled above".
246. **Ownership as before**: on a value that is owned, the names take what they name, at any depth; what
     no name takes is dropped once the arm is chosen, and a box the pattern looked into is given back after
     what it held. On a borrowed value the names borrow, and through a `read` field (a type that carries a
     borrow) they are `read`. `is` only looks.
247. **No pattern inside a value that may be `null`**: `Maybe(Number(n))` on an `Expr?` field is an error
     whose hint is to name it and test it in the arm. A `null` pattern is not here yet. (Both came in the
     forty-fourth slice, 257.)
248. **`match` on text**: on a `string`, a `read string` (a range of a text included) and a `ref string`.
     The arms are texts as they are written (with the escapes of a text), `_` and `var name`; as on a
     number, `_` or `var name` is required, and the same text twice is an error. `$"..."` is not a pattern:
     a text that is built is compared with `==`, in a `when` if need be. A `match` on a `string` that is
     owned moves it, as every `match` moves (`match read text` keeps it); `for word in read words` gives
     borrows, so nothing moves there.
249. **A text arm in compiled code is a test of the length and then of the bytes**: `sg_eq(word,
     SG_STR("if", 2))`, the chain a person writes in C with `memcmp`. No hash and no switch on the
     length: with a dozen arms the chain is as fast as the hand-written one (`keywords`, 1.03). A switch on
     the length or on the first byte can come if a measurement with many arms asks for it.
250. **Methods on enums**: written in the enum body, after or between the variants, as in a struct, with
     `this`, `read this`, `ref this` or none; `public` as in a struct. A line that is a name alone or a name
     followed by `(` is a variant, any other line a function (a type, then the name). A function cannot
     have a variant's name, and an enum has no fields: `int size` in an enum is an error whose hint is to
     put the value in each variant. `Shape.unit()` calls a function without `this`; `Shape.Circle(2)` is
     still the variant.
251. **The interfaces on an enum with methods of its own**: `Formattable` with its own `format`,
     `Equatable` with its own `equals`, and `Comparable`, which needs its `compare`, under the rules a
     struct has (the two checks are now one). This settles the design's "an enum cannot be `Comparable`
     until enums have methods". `Hashable` with its own `equals` or `hash` stays out, as for a struct.
252. **Compiled code**: a method of an enum is a C function like a struct's (`f_Shape_area`), and the
     helpers that write, compare and order an enum call its own `format`, `equals` and `compare`. An arm
     with patterns inside is one condition: the tests of the tags and the values joined by `&&` in the
     order they are written, so a value is read only after the tag of its variant was tested. Each arm
     tests from the top; a decision tree that tests a shared part once can come if a measurement shows the
     repeated tests cost something.
253. **The measurement and the tests**: `keywords` (above) takes 1.03 times the time of the chain of
     `memcmp` in C. The tests take apart an expression tree that is owned and one that is borrowed, two
     levels deep, with numbers and text inside, through `T*`, with `ref` names that change the tree, in
     `is` as well; `match` on owned, borrowed and ranged text, on the words of `split`, and texts inside
     the variants of a type that carries a borrow; methods of every kind on enums, an enum's own `format`,
     `compare` (with `<` and `sort`) and `equals` (with `==` and `indexOf`), and a `public` enum method seen
     from another module while a hidden one is not; twenty-two tests of errors. Every compiled one left no
     memory behind.

Choices made in the forty-fourth slice (`or`, ranges, `null`, `true` and `false` in patterns; checker,
interpreter and back end together), approved on 14 November 2026:

254. **A range in a pattern takes both ends and is written `..=`**: `'a'..='z'`, `0..=9`, `-5..=-1`, on an
     integer and on a `char`, also inside the parentheses of a variant (`Digit(0..=4)`). `'a'..'z'` in a
     pattern is an error whose hint is `'a'..='z'`: `..` everywhere else stops before its end (`for i in
     0..n`, `text[a..b]`), and a pattern that meant the letters would quietly leave out `z`. `..=` is
     written only in a pattern; in a `for` or a range of a text it is an error whose hint is `a..b + 1`.
     The ends are numbers or characters as they are written, the first not greater than the last (`9..=0`
     is an error whose hint turns it around), each fitting the type. Rust wrote patterns this way until
     1.80, when it let `..` in too. Decided before the slice, on 14 November 2026.
255. **The check of a `match` on a number counts**: a `match` on an integer or a `char` whose arms take
     every value needs no `_` (`0..=127` and `128..=255` on a `byte`, or all 256 numbers), and an `_` below
     them is an arm that can never run. One that misses values says which (260). This changes the rule
     "the check does not count" (row "`match` on a number", rule 6 of the sixth slice): with ranges a
     `_` that can never run would otherwise be required, and the search that counts is the one that already
     runs for patterns inside patterns. Rust does the same. Decided during the slice, on 14 November
     2026. No constant as an end of a range (not
     `int.MaxValue`) and no open end (`5..=`): both ends are written.
256. **`or` joins patterns, in an arm and inside the parentheses**: `Add(l, _) or Mul(l, _)`, `'+' or
     '-'`, `Word("if" or "else")`, `Negate(inner) or var inner`, in `match` and in `is`. Every pattern of
     an `or` names the same values with the same types, whichever matched (the names of `Number(v) or
     Name(v)` are an `int` and a `string`: an error); a name missing from one of them is an error that
     says which. A pattern of an arm's `or` that the arms above and the patterns before it take is an
     error ("this pattern can never match"); inside the parentheses that is not looked for yet. A line
     may end with `or` and go on below. The word is C#'s: SG writes its patterns as C# does (`is not
     null`), and `|` is an operator on numbers in SG, so `x is A | B` would read as one; with `or` what
     is a pattern and what is a value shows (`x is A or B || ready`). `or` is a keyword now, and `|` or
     `||` between patterns is an error whose hint is `or`. Rust and Swift write `|` and `,`. First
     written `|`, changed to `or` during the slice, on 14 November 2026.
257. **`null` is a pattern, and a pattern on a `T?` looks at the value**: on a `T?` (a field, or the value
     of the `match` itself) `null` takes `null`, any other pattern takes a value that is there and then
     tests it as on a `T`: `Node(null, Leaf(label))`, `Maybe(Number(n))`, `match shape { null => ...
     Circle(r) => ... }`. This removes the error of 247. `null` on a type without `?` is an error ("a
     `Shape` is never `null`"). A `match` on a `T?` takes `null` with an arm of its own or with `_`; the
     search names it when it is missing (`does not handle null`).
258. **A name below `null` takes what is there**: in a `match` on a `T?`, `var text` in an arm under an arm
     `null` without `when` has the type `T` (`match s { null => 0 var text => text.length() }`), the same
     narrowing `if s is not null` gives. Above it, or without such an arm, the name is a `T?`.
259. **`true` and `false` are patterns**, and a `match` can look at a `bool`: `match done { true => ...
     false => ... }`, `Flag(true)`. Both arms make the `match` whole without `_`.
260. **What the search names**: the search of 244 now knows `or`, ranges, `null` and `true`/`false`. The
     values of an integer type or of `char` are cut where the ranges and numbers of the arms begin and end,
     and each piece is taken or not as a whole, so the search stays small however wide the type. A `match`
     on a number or a character that misses values says which (`no arm takes 128..=255`, `no arm takes
     '\0'..='/'`); the ends of the wide types are written by name (`int.MinValue..=-1`). A `match` on an
     enum without these patterns keeps the messages it had.
261. **Compiled code**: a range is `c >= lo && c <= hi`, without the half whose end is the first or the
     last value of the type; an `or` is `||`; `true` is the value, `false` its negation. On a `T?` the test
     of the flag (or of the null pointer) comes before the test of the value. An `or` that names nothing
     drops what an owned value held as `_` would. An `or` with names is opened into one pattern for each way
     to match (`Add(Number(a) or Negate(Number(a)), _)` is two); the names are declared once and the way
     that matched sets them, and for an owned value the same choice drops what that way leaves. Many
     `or`s inside one pattern open into their product; no limit is set, as none was needed yet.
262. **The measurement and the tests**: `classes` (above) takes 0.87 times the time of the chain of `if`s
     in C. The tests sort characters and bytes by ranges, cover a `byte` with ranges alone, take an
     expression tree apart with `or` and names (owned and borrowed, with `when`, inside a variant, in
     `is`), match texts with `or` inside a variant, `bool`s, an owned `T*?` tree, a `read T?` and an owned
     `string?` with a name below `null`, and a `ulong` up to its largest value; twenty-four tests of
     errors (`|` and `||` between patterns among them).
     Every compiled one left no memory behind; the two programs give the same output under gcc.

Choices made in the forty-fifth slice (the names of `is` and narrowing along `&&` and `||`; checker,
interpreter and back end together), approved on 14 November 2026:

263. **The names of `is` reach along `&&`**: an `is` on the left of `&&` names values for what stands on its
     right and for the block of the `if` or `while` whose condition the chain is: `if int.parse(read text)
     is Ok(value) && value > 0 { ... }`, `while tokenAt(i) is Word(w) && w.length() > 0 { ... }`, `if x > 3
     && s is Rect(w, h) && w < h { ... }`. A chain anywhere else gives its names to its own right side and
     ends them with itself: `return s is Circle(r) && r > 10`, `var big = ...`, a `when`, an `assert`, inside
     `!( ... )`. `else` and the code after the `if` do not see them; neither does a `||`. Parentheses do not
     matter: `(a && b) && c` is `a && b && c`. Until now the names existed only when the `is` was the whole
     condition, and `var big = s is Circle(r) && r > 100` was an error. This is C#'s rule (`if (o is int n &&
     n > 0)`), and Rust 2024 chains `if let` with `&&` the same way. Decided before the slice, on 14 November
     2026.
264. **`x is not null` narrows along `&&`, and `x is null` along `||`**: on the left of `&&`, `x is not null`
     makes `x` a `T` on the right and in the block (`if name is not null && name.length() > 2`); on the left
     of `||`, `x is null` makes it a `T` on the right, which runs only when `x` is there (`if text is null ||
     text.length() == 0 { return 0 }`). An `if` whose condition is a `||` chain of `is null` tests narrows
     every one of them in `else`, and after the `if` when the block always leaves (`if a is null || b is null
     { return }` narrows `a` and `b`); a `&&` chain of `is not null` tests narrows them after the `if` when
     `else` always leaves. Only the parts of the chain itself count: `!(x is null)`, and a test inside the
     other operator (`a && (x is not null || b)`), narrow nothing. The first half was decided before the
     slice; the `||` half, its mirror image, was added during it. C# narrows the same way. (The forty-ninth
     slice reads the whole condition, `!` included: 287.)
265. **No names where nothing sees them**: an `is` that names values and is joined by `||` is an error whose
     note says why: with `||` the block also runs when this test does not hold, and then the names have no
     value. An `is` with names under `!`, or as a whole value (`var round = s is Circle(r)`), is an error
     whose hint is `_` or `match`. The message changed from "the names in this pattern exist only when `is`
     is the whole condition of an `if` or `while`" to "nothing here can use the names in this pattern", with
     a note that says where the names of `is` are seen. (The fiftieth slice gives names to `is not` and to
     an `is` under `!`, seen where the test fails: 293.)
266. **A name used where it is not seen says why**: a name of an `is` used in `else`, after the `if` or after
     the chain was "there is no variable named `r`" with a guess at a near name (`a`). The error now points at
     the `is` that named it and says where its names are seen, and the hint is to use the value there or
     take it out with `match`.
267. **The names stay borrows, and a borrow ends at its last use**: lending the tested value as `ref` between
     the `is` and a later use of its names is the usual error (`s is Circle(r) && grow(ref s) && r > 1`);
     after their last use it is fine (`s is Circle(r) && grow(ref s)`, or `t = Token.End` in `else`).
268. **Compiled code**: a chain whose `is` names values declares the names before the statement and sets
     them inside the condition where the test holds: `(s.tag == 0 && (r = &s.u.v0.m_radius, 1)) && *r >
     10`. An `or` with names sets them by the way that matched. A value made by a call at the start of the
     chain stays until the block is done, as for a lone `is` (57). An `is` with names on a value made by a
     call after the first `&&` (`x > 0 && make(x) is Word(w)`) is refused with "not in the C back end yet":
     that value is made only sometimes and would have to stay until the block ends. The interpreter runs it
     (`tests/cases/ok_is_chain_late.sg`). (The fiftieth slice compiles it when the value owns no memory:
     299.) Narrowing writes nothing new: a narrowed `x` is read as its value,
     as before.
269. **The tests**: the names of `is` along `&&` in an `if`, a `while` over values made by a call, a
     `return`, a `var`, a `when`, under `!` and beside `||`, with `or` patterns, with a `ref` change after
     the last use and with a new value given in `else`; narrowing along `&&` and `||` in the block, in `else`, after the `if` and in a
     `while` over a linked list. Twelve tests of errors, one of them the old test of names outside the
     condition, which now names values nothing sees. Every compiled one left no memory behind; the two
     programs give the same output under gcc. Not measured on its own (see the measurements above).

Choices made in the forty-sixth slice (the order of an assignment, and the borrows its right side reads;
checker and back end; the interpreter already worked so), approved on 14 November 2026:

270. **An assignment computes its right side first, then finds the place and writes it**, for `=` and for
     `+=` and the others: `xs[place()] = value()` calls `value` first. This is Rust's order; C# and Java
     find the place first. The interpreter always worked so, but compiled code took the place first:
     `xs[place()] = value()` printed in the other order, and `words[i] = $"{grow(ref words)}".toString()`
     wrote through the address of an item that `grow` had moved (the sanitizer saw it). The place is found
     right before it is written, so its address cannot be stale, and the right side can be made from the
     value it replaces. Decided during the slice, on 14 November 2026, when the two orders were found.
271. **A borrow that the right side reads last ends before the write**: `b = Box { n = r.n + 1 }` where `r`
     borrows `b`, `while s is Circle(r) { s = Shape.Circle(r + 1) }`, `names[1] = $"{first}{first}".toString()`
     where `first` borrows `names[0]`, `t = part.clone()` where `part` is a range of `t`. These were refused
     with "`b` cannot be changed here: `r` borrows from it and is used again later". Rust accepts them.
272. **The place counts as after the right side**: what the place uses must still hold when the right side
     is done. A borrow read by the place (`counts[r.length()] = grow(ref xs)` where `r` borrows `xs[0]`) or
     written through (`r.x = grow(ref ps)` where `r` is a `ref` of `ps[0]`) cannot have its owner changed by
     the right side; the error gets a note: "the right side of an assignment is computed first, then the
     place on the left is found: `r` is used there after this". Inside the place the parts keep their
     order from left to right (`counts[bump(ref ys) + r.length()] = 1` is the usual error, without the note).
273. **Compiled code**: when the place is more than a variable, the right side is computed into a C variable
     first and the address of the item is taken after it. The C compiler removes the variable, so nothing is
     paid: the five measured programs that write items (`bits`, `blocks`, `buffer`, `nbody`, `vector`)
     compile to the same instructions as before, up to the names of registers, and take the same time.
     Where the place is a variable nothing changed. A value that owns something was already made
     before the old one is dropped.
274. **The tests**: the order of the value and the place with `=`, `+=` and a field of an item, printed by
     both programs; five right sides that read a borrow of what they replace (a struct, a struct that owns
     text, an enum in a `while`, an item of a list, a range of a text) and one `+=`; a right side that adds a
     hundred items to the list it is written into, as a number and as a text. Three tests of errors. Every
     compiled one left no memory behind; the two programs give the same output under gcc.

Choices made in the forty-seventh slice (`replaceAll` and `repeat`; checker, interpreter and back end),
approved on 14 November 2026:

275. **The method that replaces the places of a text is `replaceAll(old, new)`**, Go's `ReplaceAll`, not
     `replace(old, new)` as in C# and Rust: `x.replace(value)` already works on every type and puts a new
     value in the place (rule 1 of the third slice), so the same name would change the text with one
     argument and leave it as it was with two. `line.replace(a, b)` is an error with a note on what
     `replace` does and the hint `line.replaceAll(a, b)`; `replaceFirst` and `replacen` get the same hint.
     Decided before the slice, on 14 November 2026.
276. **`old` and `new` are each a `char` or a `read string`**; a `byte` is not, as for `contains`. The places
     are found from the left and do not overlap (`"aaa".replaceAll("aa", "b")` gives `"ba"`), and what
     was put in is not searched again. The result is always a new `string`, also when nothing was found,
     as in Rust and C#.
277. **An empty `old` is refused**, as the separator of `split` is (173): a constant `""` is a compile error,
     and a text that is empty at run time is a panic. Rust and Go put `new` between every two characters
     and at both ends, C# and Zig stop; a loop over `chars()` says the first plainly.
278. **`repeat(n)` takes a `nint`**, as an index does: an `int` or a `byte` is taken as it is, a `long` needs
     `(nint)`. A negative constant is a compile error, a negative number at run time a panic, and a text too
     big for the memory a panic that says so, in both programs (not an abort, as Rust's `repeat` and the
     allocator would give). A `char` has no `repeat`: its hint is `c.toString().repeat(n)`, or `"-".repeat(n)`
     for a constant.
279. **A call of `replaceAll` or `repeat` whose result is not used is a compile error**: `line.replaceAll('\t',
     " ")` alone changes nothing. When the text is a variable or a field the hint assigns the new text to it
     (`line = line.replaceAll(...)`). Rust gives a warning (`#[must_use]`), Zig an error for every value
     that is thrown away; SG has no warnings. The other methods that give a new value (`trim`, `clone`,
     `toString`) are left as they were for now. (The forty-eighth slice took the rule to every built-in
     method that only gives a value: 282.)
280. **Compiled code**: `replaceAll` finds each place with the search of `indexOf` (`memchr` for the first
     byte, then `memcmp`) and copies the pieces into a block that starts with the size of the text and
     doubles when it is full; `repeat` allocates the whole size once and copies what it has written after
     itself, so `n` copies take about log2(n) `memcpy`s. The panics have the interpreter's words.
281. **The measurement and the tests**: `rewrite` (above) takes 0.94 times the time of the same program in
     C. The tests: replacing with a text, a character and nothing, places that would overlap, no place,
     an empty text, characters of two to four bytes, through a `read`, a `T*` and a range, and the result
     assigned back to a field; `repeat` of zero, of an empty text, with an `int` and a `byte`, a thousand
     times, inside `$"..."` and into a list. Fourteen tests of errors and three of panics. Every compiled one
     left no memory behind; the two programs give the same output under gcc.

Choices made in the forty-eighth slice (the value of a built-in method that is not used; checker only),
approved on 14 November 2026:

282. **Every built-in method that only gives a value is a compile error when the value is not used**, not
     only `replaceAll` and `repeat` (279): `line.trim()`, `name.clone()`, `count.toString()`,
     `distance.abs()`, `xs.contains(3)`, `xs.length()`, `total.tryAdd(1)` and `string.join(", ", read
     names)` written alone do nothing. The rule takes the methods that give a new value, which 279 left
     for later, and the ones that answer a question (`contains`, `length`, `indexOf`, `compare`), which
     are as useless alone; so it is one sentence with no list to learn. Zig refuses every value that is
     thrown away; Rust marks most of these methods `#[must_use]` and warns.
283. *(Replaced by 614 in the seventy-seventh slice: these are errors too, written `_ = xs.removeLast()`.)*
     **A built-in method that changes its value and gives one too may stand alone**: `xs.removeLast()`, the
     `add` and `remove` of a `HashSet` (they give whether something changed), the `remove` and `getOrAdd`
     of a `HashMap`, and `next` on an iterator. The change is what they are called for. The ones that give
     nothing (`add` on a list, `sort`, `write`, `clear`) are statements as before.
284. *(Replaced by 614 in the seventy-seventh slice: these are errors too, written `_ = noisy(4)`.)*
     **Functions and methods you write are left as they are**: `noisy(4)` and `stack.push(5)` alone are
     accepted even when they give a value, since they can print, change through `ref` or stop, and the
     checker does not know which ones do nothing else. Zig asks for `_ =` there too; SG keeps the error for
     what it knows does nothing. A `Result` stays an error everywhere (the rule above).
285. **The words of the error**: when the method gives a new value of the type of the variable or the field
     it is called on, the error says "it leaves `x` as it was" and the hint assigns it back (`x =
     x.abs()`, `counter.count = counter.count.max(10)`). `trim` gives a look at the same text, which
     cannot be assigned back to a `string`, so it says "it leaves `line` as it was" with `var text =
     line.trim()` as the hint. The others name what was not used ("the copy that `clone` makes", "the new
     text that `toString` gives", "the value that `contains` gives") with `var` as the hint. The two
     messages of 279 now read "the new text that `replaceAll` gives is not used", like the rest. `_ =
     xs.contains(3)` still throws a value away on purpose.
286. **No cost, and compiled code as it was**: the rule is in the checker alone; the interpreter and the back
     end do not change. The tests: eight tests of errors (`trim`, `abs`, `max` on a field, `contains`,
     `clone`, `toString`, `string.join`, `tryAdd`) and one program that calls the changing methods, a
     function and a method of its own alone and throws values away with `_ =`. It left no memory behind;
     the two programs give the same output under gcc.

Choices made in the forty-ninth slice (narrowing under `!` and after a `while`; checker only), approved on
14 November 2026:

287. **`!` swaps what a test says, and a condition is read as a whole**: each condition says which
     variables are not `null` where it holds and which where it does not. `x is not null` says `x` where
     it holds, `x is null` where it does not, and `!` swaps the two, so `if !(x is null)` narrows `x` in
     the block and `if !(x is not null) { ... } else { ... }` in `else`. Where `a && b` holds both sides
     held; where it does not, only what both sides say when they fail is known. `a || b` is the other way
     round. So `if !(a is null || b is null)` narrows `a` and `b`, `if (a is not null && b > 0) || (a is
     not null && c > 0)` narrows `a`, and `if !(a is null && b is null)` narrows neither. The same reading
     serves a part of a chain (choice 264), `else`, and the code after an `if` whose branch leaves. This
     takes back the sentence of 264 that `!(x is null)` and a test inside the other operator narrow
     nothing. C# reads a condition the same way.
288. **After a `while`, what its failed condition says holds**: a loop ends when its condition fails, so
     after `while x is null { x = ... }` the variable is a `T`, and after `while first is null || second
     is null { ... }` both are. `while x is null && tries < 3` narrows nothing, since the loop may end
     because of `tries`. A `break` that leaves this loop turns it off, wherever it stands, a `match` arm
     inside an expression included; a `break` of an inner loop does not. C# does the same; Rust has no
     `null` to narrow.
289. **No new words in the error**: a variable left a `T?` because of a `break` gets the usual error that
     it may be `null`, as one assigned again does. A note that names the `break` is not here yet. The names
     of an `is` still do not reach under `!` or past the `if` (`if !(r is Ok(v)) { return }` and then `v`,
     which C# allows and Rust writes as `let ... else`); that stays the error of 265. (The fiftieth slice
     brings both: 293, 294; for the names of `is not` the error names the `break`: 298.)
290. **A `break` inside an expression is seen**: the check for a `break` in a loop looked only at a
     `match` standing alone as a statement or a value, so `var n = 1 + match k { 1 => { break } _ => 2 }`
     inside `while true { ... }` counted as a loop that never ends. A function that ended with that loop
     was accepted without a `return` and gave whatever was left in a register in compiled code, and no
     value at all in the interpreter. The check now looks inside every expression, the condition of an
     `if` and the range of an inner loop too, and the function is the error that it does not return a
     value on every path. This was there before the slice; it came up because narrowing after a `while`
     depends on the same check. The search for `take` in a loop goes over expressions the same way now.
291. **A variable the condition empties is not narrowed by it**: `if b is null || b.take() is null { return
     }` narrowed `b` after the `if`, but when that condition fails `b` was there and `take` then emptied it,
     so `b.length()` after it reached into `null` (the interpreter stopped on it). What a whole condition
     says, in `else`, after the `if` and after a `while`, now leaves out the variables the condition empties
     with `take` or assigns in a `match` arm. Along the chain itself it was already right: `take` ends the
     narrowing for the parts after it. This was there since the forty-fifth slice; narrowing after a `while`
     would have had it too.
292. **No cost, and compiled code as it was**: a narrowed variable is read as its value, as before, so the
     interpreter and the back end do not change. The tests: two programs (`!` in a block, in `else`, along
     `&&` and `||` and after an `if`; `while` over a search, and over two variables with an inner loop that
     breaks) and eight tests of errors (a `break` of the loop, a `break` in a `match` arm, `&&` in the
     condition of the `while`, `!` over `&&`, `else` of `!(x is null)`, the function of 290, and `take` in
     the condition of an `if` and of a `while`). Every compiled one left no memory behind; the two programs
     give the same output under gcc.

Choices made in the fiftieth slice (the names of `is not`, and the names seen after an `if` or a `while`;
checker, interpreter and back end), approved on 14 November 2026:

293. **The names of `is not` are seen where it fails**: `x is not Circle(r)` fails exactly where `x is
     Circle(r)` holds, so its names are seen by the code that runs only there: on the right of `||`, in
     `else`, after an `if` whose block leaves (`return`, `break`, `continue`, `panic`, `exit`), and after a
     `while` that only its test ends. `!(x is Circle(r))` is the same test. So `if int.parse(read text) is
     not Ok(value) { return -1 }` and then `return value * 2`, and `while r is not Ok(value) { r = attempt()
     }` and then `value`. Before, `is not` with names, and an `is` with names under `!`, were the errors of
     265. C# 9 allows `if (o is not int n) return;` and then `n`; Rust writes it `let Ok(value) = r else {
     return };`, which moves the value out where SG's `is` only looks.
294. **After an `if` whose `else` leaves, the names of `is`**: `if s is Circle(r) { ... } else { return 0 }`
     and then `r`, its mirror image. Narrowing has worked so after an `if` since the forty-fifth slice (264),
     and now the names follow the same rule: what the code after the `if` sees is what the way that reaches
     it is sure of.
295. **Along `||`, and only as a whole chain**: on the left of `||` an `is not` names values for its right
     side (`if r is not Ok(n) || n < 0 { return }`), and after the `if` every `is not` of the chain has
     failed, so the names of all of them are seen (`if parse(a) is not Ok(x) || parse(b) is not Ok(y) {
     return 0 }` and then `x + y`). An `is not` with names on the left of `&&` is an error, since what comes
     after it runs only where the value is another variant; an `is` with names beside `||` stays the error
     of 265. A `!` around a whole chain (`if !(r is Ok(v) && v > 0) { return }`) is not read for names,
     though C# reads it; it stays an error where the names are used (Not here yet).
296. **The names live in the block that holds the statement**: the names seen after an `if` or a `while`
     are declared in the scope that holds it, from there to its end, as a `var` would be. A name already
     declared in that scope is an error that points at both (`var value = 0` and then `if r is not
     Ok(value)`), with the hint to name the pattern's value otherwise; in an inner block it hides the outer
     one, as a `var` does (since the ninety-second slice, an error there as well: 886). They stay borrows of the tested value, so changing or moving that value while
     they are used later is the usual error (267).
297. **A value with no name stays until the block ends**: after `if fetch() is not Text(body) { return }`,
     `body` borrows from a value that has no name. That value now stays until the block around the `if`
     ends, so the names can be used there; in compiled code its drop moves from the end of the `if` to the
     end of the block. A `while` makes such a value again in every round, so a name after a `while` that
     borrows from one is an error whose hint is to name the value before the loop and change it inside. A
     number is a `read` borrow too (`int.parse(read text) is not Ok(value)`), and its value has no name, so
     this is what lets the common form be written without a `var` first.
298. **Errors where nothing sees the names**: an `if` with no `else` whose block does not leave sees the
     names of its `is not` nowhere: "nothing here can use the names in this pattern", a note that says why,
     and the hint to end the block with `return` or use `match`. A `while` tested with `is not` whose body
     has a `break` gives the same error, and it points at the `break` that leaves the loop while the test
     may still hold (for a `T?` after a `while`, the note that names the `break` is still not here: 289).
     An `is not` on the left of `&&` is "`is not` holds when the value is another variant, so nothing here
     can use the names". A name used where it is not seen (266) now says where the names of `is`, or of
     `is not`, are seen. The error of an `is` beside `||` keeps its own note and drops the general one.
299. **Compiled code**: `x is not Circle(r)` is the test of `x is Circle(r)` that sets the names, under a
     `!`: `(!(s.tag == 0 && (r = &s.u.v0.m_radius, 1)))`. The names are declared before the statement (for a
     `while`, before the loop, so that they outlive it) and stay in scope after it; a chain of `||` is
     written as one of `&&` is. The limit of 268 is narrowed: a value made by a call after the first `&&`
     or `||` is now compiled when it owns no memory: it is declared before the statement and made inside the
     condition, `(t = sg_parse_int(b), (!(t.tag == 0 && (y = &t.u.v0.m_value, 1))))`, so the form with two
     numbers above compiles. One that owns memory is still refused: it would have to be dropped only when it
     was made. Nothing is measured: it is the C a person writes with the same names.
300. **The tests**: one program (`is not` alone, under `!`, along `||` with a call after the first `||`, in
     `else`, through a `ref`, on a value with no name that owns its text, after an `if` whose `else` leaves,
     after a `while`, in a value, with `continue` in a loop, and a name hiding an outer one in an inner
     block) and nine tests of errors (an `if` that does not leave, a `break`, `&&`, the block of `is not`, a
     name already declared, an `else` that does not leave, a value made again by a `while`, and a change and
     a move of the tested value while a name is used later). Every compiled one left no memory behind; the
     program gives the same output under gcc.

Choices made in the fifty-first slice (numbers read out of bytes and written into them; checker, interpreter
and back end), approved on 14 November 2026:

301. **`T.fromLittleEndian(bytes)` and `T.fromBigEndian(bytes)`** on `short`, `ushort`, `int`, `uint`,
     `long` and `ulong`: a function of the type, as `parse` is, that takes a `read byte[]` and gives the
     number the bytes write, the lowest byte first or the highest first. A signed type reads the highest
     bit as the sign (`short.fromLittleEndian` of `ff ff` is -1). Rust has `u64::from_le_bytes`, Go
     `binary.LittleEndian.Uint64`, C# `BinaryPrimitives.ReadUInt64LittleEndian`; the name here says the
     order in whole words, as the other names of the language do, and leaves out "bytes", which the
     argument already is. It is what a file format needs (a GIF keeps its sizes in two bytes, the lowest
     first), and it closes the first place in "Slower than C".
302. **Exactly as many bytes as the type takes**: 2, 4 or 8. Rust (an array `[u8; 8]`) and Zig (a
     `*const [8]u8`) want exactly that many; Go and C# read the first ones and leave the rest. A window of
     another width is nearly always a wrong position or a wrong type, and this rule finds it; the cost is
     writing the width (`data[at..at + 8]`), which also shows the reader what is read. A window whose
     width depends on data panics with both numbers: "`uint.fromLittleEndian` reads 4 bytes, and it was
     given 2".
303. **A width the program shows is checked before it runs**: an array (its length is in the type),
     `xs[..k]` and `xs[a..b]` with constants, a range to the end of an array, and `xs[at..at + k]` where
     `at` is a variable or a field of one, which gives the same value at both ends (a call does not:
     `next()..next() + 8` is checked when it runs). Another width is "`ulong.fromLittleEndian` reads 8
     bytes, and this window has 4", with the hint of the window of the right width (`data[at..at + 8]`).
304. **`n.toLittleEndian()` and `n.toBigEndian()`** give the bytes of the number as a `byte[N]` (2, 4 or
     8): the array lies where the value is, and nothing is allocated, as with Rust's `to_le_bytes`.
     `out.addAll(read bytes)` puts them into a list, and `out.addAll(read n.toLittleEndian())` lends the
     array for that one line. Rust's `to_le` (without "bytes") gives a number with its bytes turned
     around; SG has no such method (Not here yet), so the shorter name is free for the bytes.
305. **What has neither**: `byte` and `sbyte` ("one byte has no order to be read in", with the hint
     `data[i]` or `byte[1] { b }`); `nint` and `nuint` (an address has the size the machine gives it, a
     number kept in bytes has a size of its own; the hint is the `long` or `ulong` form and a cast); the
     fractional types (a note that it is not here yet). `n.fromLittleEndian(...)` on a value gets the hint
     of the type's function, and the names other languages use (`from_le_bytes`, `fromBytes`, `toBytes`,
     `getBytes`) the hint of the name here. A call whose value is not used is an error, as for every
     built-in method that only gives a value (282).
306. **Compiled code**: one C function for each type and order, made where it is first used: the count is
     compared with the width, then `memcpy` into a `uint64_t` (or `uint32_t`, `uint16_t`) and
     `__builtin_bswap64` where the machine keeps its bytes the other way: one load, or one load and one
     instruction. Where the window was cut with a constant width just before, the C compiler sees that
     the count is right and removes the test: in `hash` no call of the panic is left, and `read64` is one
     `ldr`. `toLittleEndian` is one store into the array. The panic writes what the interpreter writes.
307. **Measured**: `hash` now reads with it. The release build takes 0.129 s against 0.125 s in C (1.03,
     as with the window it read through before), and the checked build 0.154 s, where it took 0.479 s
     (3.6 times C, now 1.23): the overflow tests of `at + 8` stood between eight reads that the C compiler
     could not then join, and now stand in front of one load. Put together from the bytes one index at a
     time, the program still takes 3.5 times C; the "Slower than C" list says so.
308. **The tests**: one program (the screen size of a GIF read into a struct, windows cut where a
     variable says, without an end and out of an array, the bytes of a list a call made, the signed and
     unsigned reading of `ff` bytes, the extremes of each type written and read back in both orders,
     through a `ref int` and a field, and bytes gathered into a list and read back), thirteen tests of
     errors (three windows of the wrong width, `byte`, `nint`, `double`, other languages' names on the
     type and on a value, the function on a value, two unused values, and `toLittleEndian` on `byte` and on
     `nint`) and two panics (too few bytes, too many). Every compiled one left no memory behind; the
     program gives the same output under gcc.

Choices made in the fifty-second slice (the overflow test of an index written `a + c`, in a checked build;
compiled code alone, since nothing a program can see changes), approved on 14 November 2026:

309. **An index written `a + c` is tested with the place**: where an index of a list, an array, a slice
     or a text, or an end of a range, is `a + c` (or `c + a`) with `c` a constant and the sum of a signed
     type, a checked build no longer tests the sum by itself before it tests the place. The sum wraps;
     one that overflowed is negative (`c` is not, so a sum can only overflow upward), and the check of the
     place refuses every negative index. Only there, on the way out, is the overflow found, and it is told
     as the test of the sum told it: the same message, the same numbers, the same line. The program stops
     where and as it stopped before, so the interpreter does not change; one comparison stands where two
     did. The idea is the one the "Slower than C" list wrote down after the fifty-first slice.
310. **What keeps its test**: `xs[i - 1]` (a difference that overflows downward wraps to a large positive
     number, which the check refuses only while no length comes near 2^63; that holds on every machine,
     but it is a fact about machines, not something the language promises), `a + b` with two variables
     (`b` may be negative), an unsigned index type (it wraps to a small number, which may be a place),
     `i * 2` (a product can wrap past zero to a small number), and a sum that is not the place itself:
     handed to a function (`read64(data, at + 8)`) or compared (`at + 32 <= length`). Measured, those
     cost nothing in `hash`.
311. **The start of a range waits only for a quiet end**: the overflow of `xs[at + 1..to]` is found
     after the end is computed, so the start is tested with the range only where computing the end can
     neither stop the program nor do anything else (a constant, a variable, a field, or such an end plus
     a constant, which wraps: in `xs[at + 1..at + 3]` both sums are tested with the range, the start's
     overflow told first). `xs[at + 1..loud(2)]` and `xs[at + 1..loud(2) + 1]` keep the test of their
     start, so `loud` does not print before the overflow is told. The end of a range, and an index, are
     the last thing computed before the check.
312. **Compiled code**: each such place gets a `static const sg_sum` with the constant, the type and the
     line of the sum, and calls `sg_ats_...`, `sg_cuts_...`, `sg_afters_...`, `sg_befores_...`, `sg_ats`
     or `sg_slices`, which are the helpers they were with that one pointer more; the pointer is read only
     on the way out (`sg_sum_told`), where `a` is the wrapped sum less `c` in the type of the sum. Other
     places, and every release build, are written as before.
313. **Measured**: `hash` checked takes 0.129 s, where it took 0.154 s; the release build takes 0.128 s and C
     0.122 to 0.128 s over two runs. Nothing else in the table moves, and no release build changes. `buffer` (2.6 times) and `vector` (2 times) do not change, and they were never paying for an
     index: left out one at a time in the generated C, the tests that cost are the test of `sum += b` in
     `buffer`'s `total` (0.72 s to 0.28 s, the time of C, with that one test gone and every other kept),
     and in `vector` the test of `sum += x` (0.66 s to 0.53 s) and those of `xs[i] * 3 + 1` (to 0.33 s).
     Those tests are real, since a sum can overflow, and they keep the C compiler from adding several
     numbers at once. The "Slower than C" list says so now.
314. **The tests**: one program of fifteen tests, each its own process, since all but the first panic:
     sums that fit, as an index of a list, an array, a slice and a text, as both ends of a range and with
     `+=`; an overflow as the index of a list, with the constant first, as an `int` index of an array and
     an `sbyte` index of a slice (both widened to `nint`), as the index of a text, at each end of a range of
     items and of text, at the start of a range whose end prints, whose end is a sum too and whose end is
     a sum of what prints; a sum that fits and is past the end;
     a negative sum that fits. The interpreter and compiled code print the same, also under the
     sanitizers.

Choices made in the fifty-third slice (the overflow tests of a sum kept in a loop, in a checked build;
compiled code alone, since nothing a program can see changes, and a fix on the way), approved on 14 November 2026:

315. **A sum kept in a loop runs without its test for as many rounds as it has room for.** In a checked
     build, before a loop that adds to a sum (`sum += b`) starts, compiled code works out how many rounds
     cannot overflow the sum from the value it has then: the room to the top of its type divided by the
     most one round can add, and the room to the bottom divided by the most it can take. The loop is
     written twice: the first copy runs that many rounds (or to the end, if that comes first) with the
     sum added without its test, the second takes over at the same position with the test. A sum that
     does come near its end stops the program at the same step, with the same message and line, so the
     interpreter does not change. Java's and Swift's compilers split loops this way for their index
     checks; it is the candidate the "Slower than C" list wrote down after the fifty-second slice.
316. **Which sums**: a variable of an integer type declared before the loop that the body changes only
     with `+=` and `-=`, where every value added is a constant or of a narrower integer type (`long +=
     byte`, `long += int`, `int += 1`, `ulong += uint`). A `byte` added to a `long` that starts at zero
     leaves room for 2^63 / 255 rounds; a `short` adding `byte`s has room for 128, and the rest of a
     longer loop runs in the second copy. Left with their test: a sum that the body assigns in another
     way or lends as `ref`, a `ref` parameter or a field, `sum = sum + x`, a value of the sum's own type
     (`int += int` would have room for one round), and a sum of a `float`, which has no test. A sum read
     in the body (`if sum > highest`) is not changed by being read.
317. **Which loops**: `for i in a..b`, and `for` over a list, an array, a slice, a map or a set, lent or
     handed over: each has a count that goes up by one a round. Not a `while` or a `for` over the parts
     of a text, which have no count before they start (there the decoding of the text costs far more
     than the test), and only a loop with no loop inside, so that a step runs at most once a round and
     no loop is written more than twice. Nor a loop whose body calls anything (a function, a method, a
     lookup in a map, a formatted text): a call keeps the C compiler from doing several rounds at once
     whatever the test does, so the test costs little next to it, and the second copy calls the
     function from a second place, after which the C compiler may stop putting it where it is called.
     Measured before this was added: `classes` (checked) went from 0.30 s to 0.40 s, because `kind`
     stopped being put in place, and `numbers` and `keywords` lost about a tenth.
318. **Compiled code**: the count is `sg_upto` of the position and `sg_steps` of each sum (the least of
     them when there are several), computed once on 128 bits. The first copy is `for (; at < stop &&
     at < end; at++)` with the sums written as a release build writes them; the second is the loop as it
     was. A `break` in the first copy leaves both, with a `goto` to a label after the second. A release
     build is written as before.
319. **The variable of `for i in a..b` is a copy of the count, in compiled code too.** Found while the
     loops were split: the body may change `i`, and in the interpreter that changes only the round it is
     in (`for i in 0..4 { i += 1; print(i) }` prints 1, 2, 3, 4), while compiled code changed the count
     and printed 1 and 3. Compiled code now keeps the count in a variable of its own and makes `i` from it
     at the start of each round, as the interpreter does, as Go's `for i := range n` does and as Rust
     does with `for mut i`. C#'s `for` would print 1 and 3; its `foreach` variable cannot be assigned. The design
     did not say; this writes down what the interpreter did.
320. **Measured**: the checked `buffer` takes 0.290 s, where it took 0.733 s (release 0.280 s, C 0.282
     s), and the checked `vector` 0.529 s, where it took 0.682 s; what is left there are the tests of
     `xs[i] * 3 + 1` ("Slower than C"). With gcc 16 the checked `buffer` goes from 0.90 s to 0.62 s (its
     release build takes 0.50 s). Five of the measured programs have a loop written twice (`bits`,
     `buffer`, `counts`, `vector`, `words`), and no other row moves: each program was built with the
     back end before and after the slice and the two run side by side, since the machine itself drifted
     by up to a tenth between runs of the table (`fields` and `numbers` in release, which this slice does
     not touch). A loop written twice is about twice the code of that loop.
321. **The tests**: one program of eleven tests, each its own process, since five of them panic: sums
     that fit over a slice, a list, a range, a map, several sums in one loop with one taken from and a
     constant counted, a `uint` added to a `ulong`; the first copy handing over to the second in the
     middle of a loop, for a `long` and for a `short`; `break` and `continue`; a sum lent as `ref`; a loop
     inside a loop; the loop's own variable named like a sum; and an overflow after the rounds without
     the test, a count at the top of `int`, a `ulong` taken below zero, a `short` past its rounds and a
     negative sum. The interpreter and compiled code print the same, also under the sanitizers. And
     one program for 319. To try the second copy everywhere, every loop of every program was written
     twice for a run of all the tests (three rounds in the first copy): they passed, under the sanitizers.

Choices made in the fifty-fourth slice (`shared`, `weak` and type aliases, in the checker and the
interpreter; the first of the four parts the work on several owners was split into: `checked` comes
second, compiled code third, threads and `locked` fourth), approved on 14 November 2026:

322. **`shared T` and `weak T` as the design wrote them** ("Shared ownership", "Weak reference", "`share`
     and `null`", "Identity and equality", "Copy of `shared T`"): `new shared x` makes the value and its
     first owner, `share x` one more owner without moving `x`, and a move hands an owner over. `weak x`
     looks without owning, and `share w` gives a `shared T?`: an owner, or `null` once the last owner is
     gone (and for a `null`). `same(a, b)` tells whether two point to one object, `==` compares what they
     point to, and `.clone()` of a `shared` value is one more owner, not a copy. In the interpreter a
     `shared` value is Rust's `Rc` and a `weak` value its `Weak`.
323. **What a `shared` value points to is only read.** Each way of writing through it is a compile error
     whose note says that a value changed through several owners is a `shared checked T` (the second
     part): assigning a field, `ref node.value`, a method that takes `ref this`, a method that changes a
     collection (`node.items.add(1)`), lending it as a `ref T`. So is each way of taking something out of
     it: moving a field (`var name = node.name`, hint `read node.name`) and a method that takes `this`. A
     `shared Node` lends what it points to where a `read Node` goes (`width(read node)`), as a `T*`
     does; `for item in read node.items` borrows the items, and `for word in words` over a `shared
     List<string>` hands that one owner to the loop and lends it the items. This is the design's error
     "Write through `shared`".
324. **Where the words stand in a type**: `shared` and `weak` in front of the name, after `read` or `ref`
     (`read shared Node`). `shared Node?` is a `shared Node` that may be missing: the `?` goes outside, as
     in the design's `shared checked DNode? next`. A `shared` cannot hold a `T*` (it is on the heap
     already; hint `shared Node`), a `T?` (hint `shared Node?`), a `shared` or a `weak` value, a slice, or
     a type that carries a borrow (it lives as long as its last owner, which may be longer than what it
     borrows from). Any other value can be shared: `new shared 5`, `new shared List<string> { ... }`.
325. **`same` looks at its arguments, as `print` does**: it does not move them, with or without `read`
     (the design's cities example writes `same(read cur, goal)`; both forms work). Both are `shared`
     values of one type; a `T?` gets the hint to test it first, a `weak` the hint `share w`, any other
     value the hint `==` if it has one. `==` between two `shared` values whose type is not `Equatable`
     gets the hint `same(a, b)`, as the design asked.
326. **A `shared` value is never a key** of a `HashMap` or an item of a `HashSet`, whatever it points to;
     the hint is a field that tells the values apart (`HashSet<int>`, `city.id`). The design refused
     `HashSet<CityRef>` because `City` is not `Hashable`; this refuses it in every case, since what it
     points to will be changeable through `checked` (the second part), and a key that changes breaks the
     table. It counts for `==` (a struct with a `shared` field can be `Equatable`), not for `Hashable`,
     as a collection does.
327. **A `weak` value has nothing to show by itself**: it cannot be printed, compared or reached into
     (`w.value`), and each of those errors has the hint `share w`. A struct with a `weak` field cannot be
     `Formattable` or `Equatable` field by field (the design's "Identity and equality": the compiler cannot
     make `equals` for `City.roads`); the hint is to write `format` by hand.
328. **`shared`, `weak`, `share` and `using` are words of the language**, not names: `var weak = 1` is an
     error that says so.
329. **A type alias as the design wrote it** ("Type alias"): `using NodeRef = shared Node`, or `public
     using`, at the top level of a file, one on a line. It is the same type: `NodeRef?` is `shared
     Node?`, and a `NodeRef` goes where a `shared Node` goes. It builds what its type builds: `P(1, 2)` and
     `P { ... }` for `using P = Point`, `new NodeRef(args)` and `new NodeRef { ... }` for a `shared` one
     (the design's `new FolderRef(...)`); `NodeRef { ... }` without `new` is an error whose hint adds it.
     It is `public` or not as a type is, and a `public` alias in a module cannot show a type that is not.
330. **What an alias cannot be**: a borrow (`using Text = read string`; the hint writes `read Text` where it
     is used), generic (`using Link<T> = ...`, deferred with generics), the name of another type, an alias
     that comes back to itself ("the aliases `A` and `B` name each other"), or `shared` around an alias
     that is `shared` already. A collection is built by its type, not by its alias (`Names()` gets the
     hint `List<string>()`). Compiled code takes an alias as the type it names, so a program with `using
     P = Point` is compiled; a `shared` or a `weak` value is refused there with an error that names it.
331. **A moved value leaves its place empty in the interpreter**, as it does in compiled code. Until now
     the interpreter kept a copy in the variable or the field a value was moved out of; no program could
     see it, but it would have counted as one more owner of a `shared` value and kept a `weak` one alive.
     Every program and test runs as before.
332. **The leak report waits for the second part.** The design reports the `shared` values still alive when
     a program ends ("`shared` cycle"). Here nothing can be changed through a `shared` value yet, so no two
     values can point to each other and nothing can leak; the report comes with `checked`, where a test
     can make a cycle.
333. **The tests**: three programs (`ok_shared`: owners, `same`, `==`, `.clone()`, a parameter that owns,
     a number and a list shared, an enum tree with one leaf in it twice, lists and maps of `shared`
     values and `share map.get(key)`; `ok_weak`: a `weak` value whose last owner is assigned over, dropped
     in a function, ends with its block, is taken off a list, cleared and taken out of a `T?`, a child
     that looks at its parent, a list of `weak` values gone over with `share`; `ok_alias_shared`), one of
     plain aliases that is compiled too (`ok_alias`), thirty-four tests of errors, and three programs with
     modules (a `public` alias used from another module with and without the module's name, an alias that
     is not `public`, a `public` alias that shows a type that is not).

Choices made in the fifty-fifth slice (`shared checked`, the `as` blocks, `this` with a type and the leak
report, in the checker and the interpreter), approved on 14 November 2026:

334. **`shared checked T` and `weak checked T` as the design wrote them**: `checked` comes after `shared`
     or `weak` (`shared checked DNode? next`, `List<weak checked City>`), `new shared checked x` makes one,
     and an alias of it builds it (`new FolderRef(...)`). `checked` alone (`checked int x`, `new checked
     5`) is an error that says where it goes. `shared T` and `shared checked T` are two types, and the
     error between them says why. `checked` is a word of the language.
335. **How long a borrow lasts.** The design says an access borrows "for the duration of the expression";
     taken word for word, `cur.bytes += amount` in its own folder example reads and changes `cur` in one
     expression and would conflict with itself. Here an access that copies a value out or assigns one is
     checked once and borrows nothing after it (`a.count = a.count + a.total()` works). A place lent to a
     call (an argument, `read x`, `ref x`, the receiver of a method) is borrowed until the call returns,
     or until its line ends when the result borrows from it; the head of a `for` for the whole loop; the
     value a `match` looks into, and one an `is` with names looks into, for the whole `match` or `if`; an
     `as` block for the block. A borrow also covers each `shared checked` value on the way to it (`read
     a.next.items` borrows `a` too), so that what is lent cannot be freed meanwhile.
336. **The receiver of a method that changes it is lent in two steps**, as in Rust: to read while the
     arguments are computed, then to change. `a.items.add(a.items.length())` works, as it does on a plain
     list; an argument that is still a borrow of the same value when the method starts stops the program
     (`stock.counts.addAll(read other.counts)`, where `other` is another owner).
337. **A conflict through the same variable is a compile error**, as the design asked ("`checked`
     conflict"): the same variable and the same fields in front of the value (`folder`, `a.next`), while
     a borrow that does not allow the access is in progress. The message says that the line would stop the
     program every time it ran and shows where the borrow began; the hint depends on what holds it (a loop,
     a block, a `match`, a call). An item of a list (`xs[i]`) is not compared; that is left to run time.
338. **A conflict through another variable stops the program with both lines**: "this `Folder` is being
     read, so it cannot be changed here", the line of the access, and "it is being read from here" with
     the line where the borrow began.
339. **A borrow does not outlive its line** ("a local variable cannot hold a `checked` borrow"): keeping one
     in a variable (`var kids = read folder.children`, hint `read folder as value { ... }`), assigning it to
     a place, returning it, or seeing the names of a condition after its `if` is a compile error. A result
     that borrows (`map.getOrAdd(key, 0)`) is used inside `ref x as m { ... }`.
340. **`read x as n { }` and `ref x as n { }`** open a `shared checked` value for the block; `read ... as`
     also a plain `shared` one, and `ref ... as` on it is an error whose note names `shared checked`. Any
     other value is an error with the hint to use it as it is, a `T?` the hint to test it first, a `weak`
     the hint `share`. The name borrows for the block, so the value is not moved or assigned in it.
341. **`this` with a type**: `read FolderRef this` borrows the owner, `FolderRef this` takes one of its own;
     `ref` is an error. The type is `shared` or `shared checked` around the method's own struct, or an alias
     of it. The method is called only on such a value; on a plain `Folder` the error says which and the
     hint is `new FolderRef(...)`, and a method that takes the owner by value, called on a borrow, has the
     hint `share`. It does not open the value for the call, so code that walks up the tree from it does not
     conflict with itself.
342. **What `checked` changes, as `ref` does**: fields, a method that takes `ref this`, the methods that
     change a collection, `for x in ref`, and `ref a` where a `ref T` goes (`read a` there gets the hint
     `ref a`). It is changed through a `read FolderRef` too: what is borrowed is the owner, not the value,
     as in the design's `child`. A field is not moved out (hint `replace` or `take`), and a method that
     takes `this` by value is not called through it.
343. **The names of `match` and `is` borrow**, of a `Copyable` value too: `if d.shape is Circle(r) {
     d.shape = ... }` is an error. Without names (`is Named(_)`, a `match` on a `Copyable` field, which is
     copied) the value is read once. A `match` on a field that owns needs `read`, as on a plain `shared`
     value. `print(a)`, `$"{a}"` and `a == b` read what it holds for the call or the line.
344. **The leak report** ("`shared` cycle"): the `shared` values, `checked` or not, still alive when a
     program ends (after `main`'s own values are gone) or when a test that passed ends. It says how many,
     where most of them were made (and the next place), the field through which they hold each other, and
     a hint with that field's type written with `weak`, aliases spelled out (`List<weak checked City>`). A
     program that leaks ends with exit code 1, and a test that leaks fails with the report; nothing is
     reported after a panic or `exit`. The design's wording was fitted to the form of the other messages.
     It is in the interpreter only, until compiled code has `shared` values (the third part).
345. **The cities example without a lambda**: `remove` builds the list of the cities to keep with a `for`
     and `same`, since a lambda was taken out of the design (12 November); the rest runs as written, and so
     do the folder tree (with `touchAll` as a program that panics) and the doubly linked list of 02.
346. **The tests**: five programs (`ok_checked`: changes through either owner, a method that changes it,
     a list that grows by its own length, `for x in ref`, a map changed through `ref ... as`, a `match` on
     a `Copyable` field, `read a` and `ref a` lent, `==`, both blocks, `this` with a type, `weak checked`,
     `.clone()`, a plain `shared` value opened for reading; `ok_checked_match`; `ok_folders`, `ok_cities`,
     `ok_dlist` from the design), three that panic (another variable in a loop, an argument still lent when
     the method starts, a receiver read inside its own method), two that leak (`leak_cycle`,
     `leak_cities`, the design's wrong version), a test file where one test leaks (`tests_checked_leak`),
     and twenty-three tests of errors.

Choices made in the fifty-sixth slice (`shared`, `weak` and `shared checked` in compiled code, and the leak
report there), approved on 14 November 2026:

347. **A `shared T` is a pointer to the value, with two counts in front of it**: the owners, and the `weak`
     lookers, where the owners together count as one looker (Rust's `Arc`). `shared T` and `weak T` are the
     size of a pointer, what a `shared T` holds is reached as through a `T*` (fields, methods, `print`,
     `==`, `match`), and `shared T?` and `weak T?` are the same pointer with `NULL` for `null`, as `T*?` is.
     `share x` adds an owner, the end of an owner takes one away, and the last one drops the value; the last
     looker frees the block. `share w` adds an owner only while there is one (a compare-and-swap that
     fails at zero), `same(a, b)` compares the two pointers, `.clone()` is `share`.
348. **The counts are atomic**, as the design decided ("Shared ownership": one name for `Rc` and `Arc`,
     because a count made atomic later is the hard direction): adding is relaxed, letting go releases and
     the last one acquires, as in Rust. In `tree` they are about 0.010 s of the 0.033 s between SG and C;
     a program has one thread so far, so this is a price paid ahead of threads (see "Slower than C").
349. **A `shared checked` value has its borrows in front of its counts** (16 bytes more): the borrow that
     changes it, and the borrows that read it, the last made first, so that a conflict names the line the
     interpreter names. A borrow that lasts is a small record in the frame of the function that lent it:
     declared at the start of the function, made where the checker opened the value (`Open`), let go where
     the checker said it ends (the end of the call, the line, the loop, the `match`, the `as` block) and on
     every way out before that (`return`, `break`, `continue`, `try`). An access that copies a value or
     assigns one is a check and records nothing. A conflict panics with the interpreter's two lines; the
     checks are kept in a release build, as the check of an index is. A plain `shared` value opened for
     reading (`read plain as items`) has nothing to check, and nothing is written for it.
350. **The receiver of a method that changes it is lent in two steps here too** (336): the receiver and then
     each argument are computed into variables, and only then is the receiver lent to change and the
     method called, for the methods you write and for the built-in ones (`add`, `addAll`, `remove`,
     `getOrAdd`).
351. **The leak report is in a debug build and in the tests, not in a release build**, as the design wrote
     ("in test and debug builds"). A debug build keeps every `shared` value that is alive in a list (48
     bytes in front of its counts), in the order they were made, with the place it was made at; when `main`
     returns and its values are gone, or a test that passed ends, the values still in the list are reported
     with the interpreter's text, and the program ends with exit code 1 (the test fails). The field through
     which they hold each other is the first field, in the order the values were made, that owns a `shared`
     value somewhere inside it: a helper for each type that can hold one looks. A release build keeps no list
     and reports nothing; its values cost the counts alone.
352. **What leaks on purpose is not counted as left behind by `cargo test`**: the two programs that leak
     (`leak_cycle`, `leak_cities`) report it themselves, and their blocks are still reached from the list,
     so the system's leak check does not see them; every other program frees all it took.
353. **A `for` over a `shared` collection that owns it takes that owner** (`for word in words`, 323) and lets
     it go when the loop ends, as the interpreter does: a `weak` look after the loop finds it gone.
354. **Measured** (`tree`): the folder tree of 05 with 111,111 folders, built and freed twenty times: 0.095 s
     against 0.062 s for C with plain pointers (1.53), 0.102 s in a checked build. What the 0.033 s are is
     under the table; the counts are the decision of 348, the rest is the price of several owners and a
     check of the borrows, which C does not pay because it checks nothing.
355. **The tests**: every program with `shared`, `weak` and `checked` is compiled now, gives the interpreter's
     output (the panics of a conflict and the leak reports included) and runs clean under the sanitizers;
     the test file where one test leaks is compiled too. One new program, `ok_checked_tables` (the methods
     of a map, a set and a list in a `shared checked` value lent in two steps, and a `for` that takes the
     owner of a `shared` and of a `shared checked` list).

Choices made in the fifty-seventh slice (tasks, `locked` and `lock`, in the checker and the interpreter; the
last of the four parts of the work on several owners, its compiled code to come in the fifty-eighth),
approved on 14 November 2026:

356. **In the interpreter a task runs when it is waited for.** The design lets tasks change hands only where
     one waits ("Tasks in the interpreter"); which task runs then is the one waited for, on a thread of the
     interpreter of its own while the one that waits is blocked, until it ends. `Task.run` only records the
     function and what it is given, so the starter goes on first: in `ok_task_order` "started" comes before
     what the tasks print. A task that starts tasks waits for them the same way, so the waits make a tree.
     The values a task reaches (`Rc`) cross to its thread and back with it; no two threads use them at the
     same time.
357. **An exit with an error lets a task go, and the task is waited for there.** As for a `StreamWriter`
     (`try`, `return Error(...)`): the task runs when the function ends (in the interpreter; compiled code
     waits for it there) and what it gives is dropped, because the caller gets the error that made the
     function stop. Without this the examples' two files could not be written as the design wrote them:
     `try first.wait()` leaves `second` when `first` fails. Every other exit (the end of the block, `return`,
     `break`, `continue`, `exit`, another task given to the variable) is a compile error, as the design says.
358. **A `Task` is kept only as it is, in a local variable, a parameter or a return value**: not in a field,
     a variant, a list, a map, a `T?`, a `T*`, a `shared` value, and not behind `read` or `ref`, where the
     compiler could not see it waited for. A function that takes a `Task` waits for it, and one that
     returns it hands that over to its caller. A list of tasks comes with `Task.whenAll`, which the design
     keeps for later.
359. **`wait` takes the task**, as `close` takes a writer: a task is waited for once, and a borrowed one
     cannot be. What it gives is used or dropped on purpose: `t.wait()` alone on a line is an error when
     the function returns a value (the hint is `var value = t.wait()`, or `_ = t.wait()`), as for the
     built-in methods that only give a value (282); a `Result` left unused is the usual error, and a
     `Task<void>` is waited for with `t.wait()` alone.
360. **`Task.run` checks the function, then what it is given, as a call does.** The first argument is the
     name of a function or a `Func`; `Task.run(work(1))` is an error whose hint is `Task.run(work, 1)`.
     The thread rule is checked on the types of the function's parameters and on what it returns, so the
     error names the parameter and, inside a struct, the field (`Holder` holds a `shared checked Node` in
     `Node.next`).
361. **The thread rule, as written in the checker**: a borrow, a slice, a type that carries a borrow, an
     iterator over a text, a `shared checked` or `weak checked` value and a `Task` do not go to another
     task, also inside the fields of a struct or an enum, the items of a collection, a `T?`, a `T*`, a
     `shared` value. A `Func` goes (it is the address of a function, decided on 12 November), and so do a
     `StreamReader` and a `StreamWriter`, which own their file, and a plain `shared T`, which is only read
     and whose count is atomic.
362. **What a `locked` value holds follows the thread rule** when every type is known, so a struct whose
     field is `shared checked` cannot be locked, and the error names the field; `shared checked locked`
     is an error, as is `locked` alone, after `new` alone, and around a `Task`.
363. **Outside a `lock` block what a `locked` value holds is not reached at all**, not even read: a field,
     a method, `for`, `match`, `print`, `==`, `read x` where a `read T` goes and `read x as` are each an
     error whose hint is `lock x as name { ... }`. `share`, `weak`, `same` and `.clone()` (one more owner)
     work on it, since they do not touch what it holds.
364. **A `lock` block**: each name is a `ref T` for the block, and the values stay where they are while it
     runs. The same variable twice is a compile error, the same value through two variables a panic
     (`x` and `y` are one value, which a `lock` takes once); two names alike are an error. A `weak locked`
     value is locked through `share w` (the hint), a `shared locked T?` after a test; a plain `shared` and
     a `shared checked` value are refused with what fits them.
365. **What waits is found on the whole program after the checker** (`src/waits.rs`): a function waits when
     it takes a lock or waits for a task, or calls a function that does, until nothing more is found. A
     call of a `Func` inside a `lock` block is refused when any function whose address the program takes,
     with as many parameters and the same return type, waits: a little wider than the design's "could be
     such a function", and never narrower. The error shows where the called function waits, through the
     chain of calls (`twice` calls `bump`, which takes a lock here). Starting a task inside a `lock` block
     is allowed: starting does not wait.
366. **A panic inside a task** is the usual message and, under it, the line of `Task.run` that started the
     task (`in the task `divide` started here`): the design's "which task and which line". In a test, what a
     task writes with `printError` shows under the test's `FAIL` line, as what the test writes does.
367. **In the interpreter a lock is never held by another task** when one is taken, because tasks change
     hands only at `wait`, which a `lock` block cannot reach; so nothing is kept for a lock there, and "the
     same value twice" is found by comparing the values. A system that refuses a thread for a task is a
     panic that says so, as the design wrote.
368. **`lock` and `locked` are words of the language**, not names (as `shared` and `checked` are), and
     `spawn` is a name like any other now that there is no `spawn`.
369. **The leak report covers `locked` values**: they are `shared` ones, and its hint is `weak locked T?`.
370. **The tests**: `ok_task_counter` (the syntax draft's counter) and `ok_task_files` (the examples' two
     files, which it writes first, and a third that is missing), `ok_task_kinds` (tasks that start tasks, a
     task given to a function and one given back, a `Func`, a task in a loop, a `Task<void>`),
     `ok_task_order`, `ok_lock_together` (two accounts locked together from two tasks, a list filled by two
     tasks, a `weak locked` look), `panic_task`, `panic_lock_same_value`, `leak_locked_cycle`, a test file
     (`tests_tasks`) and fifty-two tests of errors. Compiled code refused each with "is not in the C back
     end yet" until the fifty-eighth slice.

Choices made in the fifty-eighth slice (tasks, `locked` and `lock` in compiled code), approved on 15 November 2026:

371. **A task is a thread of the system**, started by `Task.run` at once, with a stack of 8 MB on every
     system, as `main` has (the design's "The stack of a task"). `Task.run` takes one allocation: a block
     that holds the thread, what the function will return, the function and its arguments, which the
     task owns from then on. A `Task<R>` is a pointer to that block; `wait` joins the thread, takes what
     the function returned and frees the block. An exit with an error that lets a task go (357) joins it
     there and drops what it returned. For each type of function that is started there is one starter,
     so a named function and a `Func` go the same way. A system that refuses the thread is a panic at the
     line of `Task.run`, with the system's words, as in the interpreter.
372. **A panic names the task it happened in, the innermost one**: in a task started by a task, the line
     that started the inner one, where the line that stopped is. A panic that already shows a second place
     (a conflict of a `shared checked` value names the borrow in progress) keeps it, as the message has room
     for one. The interpreter named the outermost task and replaced such a place; it now does the same as
     compiled code (`panic_task_inner`).
373. **A panic holds the output to the end of the process**: from its first line it holds the standard
     output and the standard error, so a second panic at the same time waits behind it and never mixes into
     it, and no task writes a line after it (the design's "A panic in a task"). A line of `print` is one call
     of `printf` and `printError` one of `fprintf`, which C writes whole, so two tasks never mix their lines
     (the design's "`print` from several tasks"); nothing else was needed for it.
374. **A `locked` value has a mutex of the system in front of its counts** (`pthread_mutex_t`: 64 bytes on
     macOS, 48 on Linux, with the alignment). `lock x as c` takes it and gives it back after the block and
     on every way out of it (`return`, `break`, `continue`, `try`). Two values are taken in the order of
     their addresses, three or more sorted first, so tasks that lock the same values never wait for each
     other in a circle; the same value through two names is tested for each pair before anything is taken,
     and is the interpreter's panic at the second.
375. **The standard streams are safe from several tasks**: a write to `Stdout` or `Stderr` is put together
     in a block of the call's own and written with one call of C, so it comes out whole, and the first
     error is kept atomically; the input is read by one task at a time (a mutex around each `readLine`,
     `readText`, `readBytes` and each line of `lines()`), so each line goes whole to one task. Only in a
     program with tasks or `locked` values: elsewhere `Stdout` writes through its own block as before.
376. **The block where printed values are written (54) is one for each thread** in a program with tasks,
     and a task gives it back when it ends; two tasks that print a struct at once each write it whole.
377. **The list of `shared` values that are alive** (the leak report of a debug build) is changed under a
     lock where several threads run; the counts were atomic already (348). The borrows of a `shared
     checked` value are not locked, since such a value stays in one task (the thread rule).
378. **A program pays for threads only when it has tasks or `locked` values**: then the C has `SG_THREADS`,
     includes the threads of the system and is linked with `-pthread` (which `sg build` passes always); a
     program without them has no lock, no value of its own for each thread and no test of a stream.
379. **A task started through a `Func` finds the name of its function by its address**, among the
     functions of the program, so its panic says `in the task `twice``, as the interpreter does; a named
     function has its name written at the line that starts it.
380. **The tests**: where tasks print in an order of their own, a `.unordered` file next to the program says
     so and the compiled output is compared line by line in any order (`ok_task_order`, `ok_task_streams`);
     the interpreter's output is still compared as it is. `panic_task` waits for the first task before it
     starts the second, so the 5 is printed on every run (the output is the same, a line lower), and
     `ok_lock_together` sorts the list two tasks filled. New: `ok_lock_ways_out` (a `lock` left by
     `return`, `break` and `continue`, three values locked together, a `shared locked T?`),
     `ok_task_streams` (two tasks writing to `Stdout` and `Stderr` and printing a struct, two reading the
     input) and `panic_task_inner`. Every program with tasks compiles, gives the interpreter's output, and
     runs clean under the address, undefined-behaviour and thread sanitizers, and the leak check where it
     does not end in a panic (382).
381. **Measured** (`tasks`): four tasks at a time, ten times, over the numbers below 400,000, adding Collatz
     lengths to a `shared locked` total every 100 numbers: 0.115 s, the time of the same program in C with
     threads of the system and a mutex (1.00). A checked build takes 0.188 s, which is the overflow test of
     `3 * n + 1`, as in `collatz`. On Linux (383) 0.168 s against 0.166 s for C (1.01).
382. **A program that ends in a panic is not looked at for leaks.** A panic does not unwind (the row "Copy
     and cleanup"): what the program owned is given back by the system when the process ends, not freed.
     Whether a leak tool still finds a pointer to it is up to the C compiler: on macOS the pointer of
     `panic_fallback_panics` happened to be kept in a register, so `leaks` passed, while gcc on Linux let
     it go and the sanitizer reported a leak (this was so before the slice too). Such a program still
     runs under the sanitizers and must give the interpreter's output; `cargo test` counts it and says
     how many there were (67 of 217), and every other program must leave nothing behind. This is the
     rule the compiled tests already had (89): a test that fails is not looked at. The runs whose output
     is compared now tell Linux's address sanitizer not to look for leaks at the end, since the leak
     check is a run of its own and its report would be added to the output.
383. **Linux tried** in a container (Debian, gcc 14, aarch64, eight cores): `cargo test --release` passes,
     with the same counts as on macOS (217 of 884 compiled, 11 under the thread sanitizer, 150 left no
     memory behind, compiled tests 12 of 15), the leak check through the sanitizer and its self-test
     included. `-pthread` and `_POSIX_C_SOURCE` (for `flockfile` under glibc) are enough. There is no
     CI; this was run by hand.

Choices made in the fifty-ninth slice (async, the first part: async functions, `await` and `Task.delay`, in
the checker and the interpreter), approved on 15 November 2026:

384. **Async comes in parts.** This one: `async` functions, `await`, `Task.delay` and the rules of the
     design's "Async, the direction" that need nothing more. Next: cancellation (`CancellationSource`, a
     token) with input and output that is async, where the design left open how a cancellation shows in
     a `Result` and what the functions are named. Then compiled code: the state machines, the pool of
     threads and the loop that waits for input and output.
385. **`async` is written in front of a function at the top level**, and the type after it is what the body
     returns: `async int count()` returns an `int`, and its call gives a `Task<int>`, the same `Task<R>` that
     `Task.run` gives. `async void f()` gives a `Task<void>`, which is awaited like any task, so it is not
     C#'s `async void` that nobody can wait for, which the design leaves out. An `async` method is not here
     yet (an error whose hint is a function that takes the value), nor is `async` in front of anything but
     a function.
386. **A call without `await` starts the task at once, and it runs on the caller until it first waits**, as
     in C# (the design's "Starting"); then the caller goes on. A task of `Task.run` still runs when it is
     waited for (356), so a program without async functions runs as it did.
387. **`await f(x)` runs `f` as part of this task**, with no task of its own: the interpreter calls it where
     it stands, as compiled code is to put its state machine inside the caller's (the design's "`await f()`
     takes no allocation").
388. **What a call without `await` is given follows the thread rule; `await f(read xs)` can borrow.** A call
     without `await` is a task that may run on another thread while the caller goes on, as `Task.run`'s is
     (360), so a borrow is an error there, with the hint `await f(read xs)` in an async function. An awaited
     call ends before the caller goes on, so it lends as a call does. This is Rust's line: `tokio::spawn`
     needs what it is given to be owned, an awaited future borrows. What an async function returns comes
     back from a task, so it follows the thread rule wherever it is called.
389. **`await t` takes the task, as `wait` does (359)**, and is written only in an async function; elsewhere
     the hint is `t.wait()`, in a test too. The value it gives is used or dropped with `_ =` on purpose, as
     for `wait`; `await` on what is not a task is an error, and when it is the call of a function that is not
     async the hint is to drop the `await`.
390. **`wait` in an async function is an error** whose hint is `await t` (the design's "Waiting the wrong
     way"). Code that is not async, `main` or a task of `Task.run`, waits for an async task with `wait`: it
     holds a thread of its own while it waits.
391. **What holds the thread is an error in an async function**: `wait`, the functions of `File`, reading
     `Stdin`, reading and writing a file through a stream, and a call of a function that does one of them,
     found on the whole program after the checker as 365 finds what waits, with the chain in the message
     (`greet` calls `ask`, which does `Stdin.readLine` here). The hint is `await Task.run(work, x)`.
     Writing to `Stdout` and `Stderr`, and `print`, are not counted, as C# writes to its console from async
     code; a write to the terminal is short, and the design's promise is about waits that can take long.
392. **`await` in a `lock` block is an error, and an async function that awaits counts as waiting**, so its
     call in a `lock` block is refused too, with the line of its `await` (the design's "Locks").
393. **An async function that awaits itself, directly or through others, is an error** whose hint keeps the
     call's task in a variable, which gives it a task of its own (the design's "Calling itself"). Only
     awaited calls make the circle; `var below = depth(n - 1)` then `await below` is accepted.
394. **An async function is not a value**: `Task.run(answer)` and `var f = answer` are errors whose hint is
     to call it. The size of its state machine is known only where it is called by its name; a `Func` that
     gives a task can come with compiled code if it is needed.
395. **`Task.delay(ms)`** gives a `Task<void>` that ends after that many milliseconds, an `int`; a negative
     number is a panic. Code that is not async can `wait` for it, which sleeps. Its form with a token comes
     with cancellation.
396. **`main` can be async**: `async void main()`, `async Result<void, E> main()`, with the command line
     too; the message of a wrong `main` says so.
397. **In the interpreter every task that runs code is a fiber**: a thread of its own, of which one runs at a
     time; the turn changes hands only where one waits, and the fibers whose wait is over get it in the
     order they became ready (the design's "Tasks in the interpreter"). The clock of the run moves only when
     every fiber waits and a timer is next, so which timer ends first never depends on how fast the machine
     is, and the run still sleeps until the time comes: `ok_async_order` prints the same on every run. Each
     fiber waits on a condition of its own, so only the next one wakes: 2,000 tasks alive at once take
     0.46 s (29 s when every hand-over woke them all), 20,000 one after the other 0.5 s. The output, the
     input and what a test writes to the standard error are shared by the fibers of a run.
398. **A panic in a task stops the run at once**, even while the task that started it waits for something
     else (`panic_async`); before this slice a task ran only when it was waited for, so its panic came at
     the wait. The message names the task, the innermost (372); fibers still waiting end without a word,
     and a writer they hold does not write its buffer, as after any panic.
399. **When nothing could end a wait, the run stops with a panic** ("every task waits, and nothing is left
     that could end a wait"). It cannot happen yet, since a task is not given to another task; it is there
     for channels (the design's "Channels").
400. **Found on the way: a `lock` block that returns on every path ends the function.** The check that a
     function returns did not look into a `lock` block, so `int bump(...) { lock c as x { x.value += 1;
     return x.value } }` was refused (`ok_lock_returns`, compiled too).
401. **The example in the design is corrected**: `countLines("a.txt", source.token)` gives a
     `Task<Result<int, IoError>>`, not a `Task<int>`; `try await first` takes the `int` out.
402. **The tests**: `ok_async_order` (two tasks and their delays, one awaited in place), `ok_async_results`
     (`try await`, an awaited call that borrows a list, `await Task.run`, an error, `wait` from `main`),
     `ok_async_error_exit` (a task left by an error exit is waited for), `ok_async_tasks_inside` (async
     tasks that start tasks, a `Task.run` task that waits for an async one, `Task.delay(0)`), `panic_async`,
     `panic_async_delay_negative`, two test files (`tests_async`, `tests_await_outside_async`),
     `ok_lock_returns`, `ok_async_module` (an async function of a module, awaited with its full name and
     with `use`) and twenty tests of errors. Compiled code refuses an async function and `Task.delay` with
     "is not in the C back end yet"; `cargo test` compiles 218 of 912 programs.

Choices made in the sixtieth slice (async, the second part: cancellation and async input and output, in the
checker and the interpreter), approved on 15 November 2026:

403. **A cancellation is an error of its own type, `Cancelled`**, chosen before the slice: a built-in struct
     with no fields (`Copyable`, `Formattable`, `Equatable`), made with `Cancelled { }`. `token.check()` gives
     a `Result<void, Cancelled>`, and `IoError` has one more variant, `Cancelled(Cancelled cancelled)`, so
     async input and output still gives an `IoError`, and `try token.check()` in a function that returns an
     `IoError` turns one into the other by the rule of error conversion (no `From` written). The cost: a
     `match` on `IoError` that names every variant needs one more arm (`error_io_error_cancelled_arm`); no
     test had one.
404. **The names take C#'s way, with `Async` at the end, and a token that may come last**, also chosen before
     the slice: `File.readTextAsync(read path)` or `File.readTextAsync(read path, read token)`,
     `Stdin.readLineAsync(ref line)`. Only a built-in function leaves an argument out; a function of the
     program takes what it declares.
405. **Methods, not properties**: `source.token()`, `source.isCancelled()`, `token.isCancelled()`. SG has no
     properties (`text.length()`, `xs.length()`), so the design's `source.token` is written with parentheses
     now, in its example too. The token that is never cancelled is a constant, `CancellationToken.None`, as
     `int.MaxValue` is (the names of constants start with a capital), and as C# names it.
406. **A source**: `CancellationSource()`, `CancellationSource.after(ms)` (a negative time is a panic),
     `cancel()`, `token()` and `isCancelled()`. It has one owner and no `clone`; another task that is to
     cancel it is given it, or the source is shared (`new shared CancellationSource()`, then `share source`).
     `cancel` works through a `read` borrow, and so through a `shared` value, which is only read (323):
     cancelling twice is cancelling once, as Rust's `CancellationToken::cancel(&self)` takes no `&mut`.
407. **A token**: not `Copyable`; `token.clone()` gives one more that looks at the same source (Rust's token is
     `Clone` too). It has `isCancelled()` and `check()`; it is not `Formattable` or `Equatable`, it cannot be
     a key, and it goes to other tasks (the thread rule lets it). A token or a source dropped without a word
     cancels nothing, as the design says. What `token()`, `clone()` and `isCancelled()` give is used (282).
408. **A built-in function takes the token as `read CancellationToken`**, as it takes a path as `read
     string`: `File.readTextAsync(read path, read token)`. The function keeps a token of its own for as long
     as it needs one, so the borrow ends with the call; a token given without `read` is an error whose note
     says so. A function of the program takes a token however it declares it (`CancellationToken token` to
     own one, which a task started without `await` needs).
409. **A built-in async call that is given a borrow is awaited where it is written**, by the rule of 388 for
     an async function of the program: `var task = File.readTextAsync(read path)` is an error whose hint is
     `await File.readTextAsync(read path)`. Paths and tokens are borrows, and the async methods of a reader
     lend the reader, so in practice every one of them is awaited in place; two reads at once are two async
     functions of the program that own their paths. Outside an async function the note says that the
     functions without `Async` do the same work.
410. **`Task.delay(ms, read token)` gives a `Task<Result<void, Cancelled>>`**: `Ok()` when the time comes,
     `Error(Cancelled { })` as soon as the token is cancelled, and at once when it already is. Without a
     token it stays a `Task<void>`. A negative time is a panic either way.
411. **A cancel ends the delays that wait on it in the order they were started**: they become ready in that
     order and run when the task that cancelled waits (`ok_cancel_delay`). `CancellationSource.after(ms)` is
     a timer on the clock of the run, as a delay is (397); a source that is gone when its time comes is not
     waited for, and the run does not sleep for it.
412. **Async input and output in the interpreter is done when it is called**: the task has ended when it is
     given, and `await` goes on without giving the turn away. The token is looked at after the arguments,
     before the work, so with a cancelled token nothing is read or written (`ok_async_files`). The cost:
     a read of `Stdin` that waits for a line holds every task until the line comes, and a token cancelled
     meanwhile does not stop it; compiled code, with its loop that waits for input and output, is to. The
     tests give the input from a file, where it makes no difference.
413. **Streams the async way come later**: `File.openTextAsync`, `createTextAsync`, `openAppendAsync` and the
     async methods of a writer are errors that say so, with the hint `await Task.run(work, x)`. A reader has
     no `linesAsync`, since a `for` does not await; the hint reads line by line with `readLineAsync`.
     `Stdout` and `Stderr` are written without waiting, as before (391).
414. **The hint of 391 names the async version where there is one**: `File.readText(read path)` in an async
     function gets `await File.readTextAsync(read path)`, `Stdin.readLine(ref line)` gets `await
     Stdin.readLineAsync(ref line)`; the others keep `await Task.run(work, x)`.
415. **A struct with no fields is written by its name alone**, as Rust writes it: `Marker`, and
     `Cancelled(Cancelled)` for the error. It was `Marker {  }`. Compiled code writes it the same way
     (`ok_struct_empty`, compiled).
416. **The tests**: `ok_cancel_delay` (a source cancelled by the clock, two tasks on one shared source
     cancelled by a third, a token already cancelled, `CancellationToken.None`), `ok_cancel_check`
     (`check`, `try` into `IoError`, `clone`, cancelling twice, a `match` on `Cancelled(_)`),
     `ok_async_files` (each async function of `File`, a missing file, a cancelled token),
     `ok_async_stdin` (lines and the rest of the input, a cancelled token), `ok_struct_empty`,
     `panic_cancel_after_negative`, `panic_cancel_delay_negative` and twenty-eight tests of errors;
     `error_async_file` has the new hint. The design's example of "Async, the direction" runs as it is
     written now. Compiled code refuses cancellation and async input and output with
     "is not in the C back end yet"; `cargo test` compiles 219 of 947 programs.

Choices made in the sixty-first slice (async, the third part: async functions, delays and cancellation in
compiled code), approved on 15 November 2026:

417. **Two slices, and files on threads of their own**, chosen before the slice: this one compiles async
     functions, `Task.delay`, cancellation and an async `main`; the sixty-second compiles input and output
     the async way. A file cannot be waited for with epoll or kqueue (it is always ready), so a read or a
     write will run on threads that may block and wake the task when it is done, as Tokio does it; the loop
     of epoll and kqueue comes with the network. The design's row "One runtime" says so now.
418. **An async function is a machine** (`a_poll_f`): a frame (`F_f`) holds its parameters, every variable
     and temporary it declares, and what it returns; the function gives `true` when it has ended and `false`
     when it has to wait. Every variable goes into the frame, not only those alive across an `await`: it is
     simpler and right by construction, and the cost is a larger frame. The frames of the async functions it
     awaits in place, and the delays it awaits in place, are used one at a time, so they share one room.
419. **It comes back with a jump**: each `await` is a numbered place, and a `switch` at the start of the
     function jumps with `goto` to the place it stopped at; plain C, and a `goto` is not caught by the
     `switch` of a `match` as a `case` would be. The back end writes the function as it writes any other,
     then moves the declarations into the frame in a pass over the C it wrote (`hoisted`); a variable
     declared inside an expression stays where it is, since nothing waits there.
420. **Where `await` stands in compiled code**: wherever its value is computed before the statement, which
     is in an argument, an operand, the condition of an `if` or a `while`, the value of a `match`, `try
     await` and `print(await f())`; what comes before it in the statement is computed before it, as the
     order of evaluation says. On a side computed only sometimes (the right of `&&`, `||` and `??`) it is
     refused with "is not in the C back end yet" and the hint to give the value a name first, as `try` is
     there; the interpreter runs it.
421. **A task is one block**: the `Task<R>` (who waits for it, and what it gives) with the frame after it
     (`J_f`), allocated when an async function is called without `await` and freed by the `await` or the
     `wait` that takes it. It runs on the caller's thread until it first has to wait, as in C#. `await f()`
     allocates nothing: the frame of `f` is in the caller's.
422. **The pool**: one thread for each core, started when a task first needs one, each with the stack of
     8 MB that every task has (375); the tasks that can run wait in one queue, first in first out, under one
     lock. A task woken while it runs runs once more right after (it is waiting, queued, running, woken or
     ended, changed by atomic steps), so no wake is lost. There is no work stealing: no measurement asks for
     it yet.
423. **Who waits for a task**: one pointer, changed by atomic steps: nobody yet, the async task that awaits
     it, or a thread asleep in `wait`. `wait` in code that is not async sleeps on a condition variable of
     its own; a `Task.run` task is still a thread (371), and `await` of one wakes the task that awaits it
     when the thread ends. A thread of the pool never sleeps in `wait`, since `wait` in async code is an
     error (388).
424. **Delays**: one more thread, the clock, keeps them in a heap ordered by when they end, and then by the
     order they were started, on the monotonic clock of the system. `Task.delay(0)`, and a delay whose
     token is already cancelled, end at once: the task goes on without giving the turn away, as
     `Task.Delay(0)` in C# is a task that has already ended. In the interpreter a delay of zero lets the
     other tasks go first (`ok_async_tasks_inside`); only the order of lines could show the difference, and
     compiled code runs its tasks side by side anyway.
425. **Compiled code runs on a real clock**: its tasks run on several threads at once, so the lines tasks
     print come in the order their times decide. The tests that wait compare their lines as they are, with
     times far enough apart: `ok_cancel_delay` waits ten times longer now (steps of 100 ms; its lines
     changed, and the interpreter sleeps 0.6 s more for it). Where the order is open, `.unordered` (380) is
     there. Later four times longer again (steps of 400 ms, at least 120 ms between two lines): 50 ms was
     too little on the loaded machines of the CI of Windows, where a third tick came after the cancel.
426. **A source and its tokens are one block**: an atomic count of owners (the source, each token, and the
     clock while `CancellationSource.after` waits), a flag, and the delays that look at it;
     `CancellationToken.None` is no pointer. `cancel` takes the clock's lock and ends those delays in the
     order they were started (411); a source dropped before its time is held by the clock until then.
427. **An async `main` is a task started on the main thread**, which then waits for it; the error it ends
     with is written as for any `main`. A panic in it names no task, as in the interpreter.
428. **A way out with an error awaits the tasks it lets go**: in an async function that `await` is a place
     to come back to, so the rest of the way out of a `try` is written after the body and the `try` jumps
     there; the way out can then wait for its tasks as any `await` does (`ok_async_error_exit`). In code
     that is not async it sleeps in `wait`, as before.
429. **The names in C** of the pieces of an async function `f` are a family of their own: `F_f`, `J_f`,
     `a_poll_f`, `a_init_f`, `a_start_f` and `a_run_f`, so that a function of the program cannot meet a
     name of the runtime (a function named `run` met `sg_job_run` on the way).
430. **What async adds to a call, measured** (`bench/async`): for each number below a million an async
     function starts a task for one Collatz length and awaits another in place, and nothing has to wait:
     0.244 s against 0.226 s for the same calls in C with one allocation for the task, 1.08 times, and
     0.372 s in a checked build. The difference is the task: its allocation and the atomic steps of
     starting, ending and being awaited, which a task that ends before it first waits would not need
     ("Slower than C").
431. **The tests**: `ok_async_order`, `ok_async_results`, `ok_async_error_exit`, `ok_async_tasks_inside`,
     `ok_cancel_check`, `ok_cancel_delay`, `panic_async`, `panic_async_delay_negative`,
     `panic_cancel_after_negative`, `panic_cancel_delay_negative` and the tests of `tests_async` are
     compiled now, and so is a new `ok_async_places` (`await` in a loop, in the condition of a `while`, in
     an `if` and a `match`, in place with a borrow, an awaited call that awaits another, two thousand
     tasks, a delay kept as a task, `Task.run`). Each runs under the thread sanitizer too; the
     undefined-behaviour sanitizer found on the way that a method of a `shared` source looked at the wrong
     pointer. `ok_async_files` and `ok_async_stdin` wait for the sixty-second slice. `cargo test` compiles
     231 of 948 programs, 24 of them under the thread sanitizer, and 13 of 17 programs of tests; on Linux
     too, in a container, with the same counts, where the clock waits on the monotonic clock in its own way.

Choices made in the sixty-second slice (async, the fourth part: input and output the async way in compiled
code), approved on 15 November 2026:

432. **The work is done on a thread of input and output** (417): `await File.readTextAsync(read path)`
     hands the work of `File.readText` to a thread that may block, which puts what it gives into the
     task's frame, marks it done and wakes the task; the task holds no thread meanwhile. The work, its
     arguments and the room for what it gives are one block in the frame (`sg_io_F`), in the room the other
     awaits share, so nothing is allocated for it. Every async call of input and output is given a borrow
     (a path, a reader), so the checker has it awaited where it is written (409), and that is the only way
     it is compiled.
433. **The token is looked at before the work**, chosen before the slice: a cancelled token gives
     `Cancelled` at once and nothing is read or written, as in the interpreter (412). Work that has started
     goes to its end, so a read of the input that waits for a line waits for it after a cancel too. Ending
     such a wait needs the loop that waits for the network (the input and a pipe to wake it, waited for
     together), which comes with the network.
434. **The threads of input and output**, chosen before the slice: one is started when work is queued and
     none is free, up to 512; above that the work waits in the queue, first in first out. A thread that has
     had nothing to do for 10 seconds ends. These are the numbers of Tokio's threads for blocking work.
     Each has the stack of 8 MB that every thread has (375), of which the system gives only the pages
     used. A system that refuses a thread is a panic only when there is none; otherwise the work waits for
     one that is there.
435. **A read the block can answer is done in place**, chosen before the slice: a reader keeps a block of
     64 KB, and `readLineAsync` when the block holds a whole line, `readBytesAsync` when it holds as many
     bytes as asked, and any read after the end are done on the task's own thread, without waiting; only a
     read that needs the system goes to a thread. When another task is reading the input at that moment,
     the read goes to a thread too, since that task may be waiting for the system: the input is tried, not
     waited for, on a thread of the pool. Tokio's `BufReader` works the same way. Measured in 439.
436. **A task that waits for the input holds no thread of the pool**: here compiled code differs from the
     interpreter on purpose (412). A test of compiled code alone shows it (`compiled_input_waits_alone`):
     sixty-four tasks wait for a line each, more than the pool has threads, and a task that only waits for
     a delay still runs; its line is looked for before any input is given.
437. **`try` and `await` on the right of `&&` and `||`**: the right side, which runs only sometimes, is
     written as an `if` of its own before the statement, after the left side, when it can leave the
     function or wait. `while count < 2 && try await Stdin.readLineAsync(ref line)` in `ok_async_stdin` is
     compiled now, and so is `text.length() > 0 && try int.parse(read text) < 10` in code that is not
     async. Fewer positions are refused (81, 420): the right of `??`, and the right of `&&` and `||` after
     a left side that made a value needing a temporary, where the hint is to give the left side a name.
438. **The names in C** for a function of the glue `F` that an async call goes through: `sg_io_F` (the
     block in the frame), `sg_io_of_F` and `sg_io_of_F_t` (the block made from the arguments, with a token
     the second), `sg_io_go_F` (the token, the read in place, or the thread), `sg_io_run_F` (what the
     thread does) and, for a reader, `F_n` (the read in place, which says whether the block was enough). The runtime is added only to a program that does input and output the async way.
439. **Measured** (`bench/asynclines`): `lines` the async way, two million lines (125 MB) read with
     `await Stdin.readLineAsync`: 0.125 s, against 0.091 s for `lines` without async and 0.090 s for C,
     1.37 times; the checked build takes the same. Handing every read to a thread would take 7.6 s, 84
     times (measured by turning the read in place off), so 435 is what makes reading the input this way
     usable. Of the 0.035 s left, about 0.023 s is the trip to a thread and back for each block of 64 KB
     (about 1,900 of them, measured by filling the block in place), and the rest is the lock of the input
     and the variables kept in the frame; the read in place is one pass over the line (`sg_read_line_at`
     with `now`: a line not whole in the block is left for the thread untouched). In "Slower than C".
440. **The tests**: `ok_async_files` and `ok_async_stdin` are compiled now, and a new `ok_async_io_tasks`
     (thirty-two tasks reading and writing files of their own at once, `try` and `await` on the right of
     `&&` and `||`, the input read in place and on a thread, by lines, by bytes and the rest), with
     `compiled_input_waits_alone` (436). Each runs under the thread sanitizer too, and the read in place
     was checked once more by turning it off (439). `cargo test` compiles 234 of 949 programs, 27 of them
     under the thread sanitizer, and 13 of 17 programs of tests; on Linux too, in a container, with the same
     counts, where the threads of input and output wait on the monotonic clock in their own way.

Choices made in the sixty-third slice (async, the fifth part: a list of tasks, `Task.whenAll` and
`Task.whenAny`), in the checker, the interpreter and compiled code together, approved on 15 November 2026:

441. **A list of tasks is kept where a task is kept** (358): `List<Task<R>>` is a type of a local
     variable, a parameter and a return value, and nothing else: not a field, an item of another
     collection, a `T?`, a `shared` value, and not behind `read` or `ref` in a type. It does not go to
     another task (the thread rule). A task in a `HashMap` or a `List<List<Task<R>>>` is still refused
     (`error_task_in_list` became `error_task_in_map`).
442. **It has `add` and `length`, and nothing else**: an index, `removeLast`, `clear` and the other methods
     are errors whose hint is `Task.whenAny`; `for` over it is an error, since a `break` would leave the
     tasks after it with no one to wait for them, and the hint is a `for` over what `Task.whenAll` gives. A
     list of tasks that has no name (made by a call) cannot be given a method, and one alone on a line is an
     error, as a task is.
443. **Every task in it is waited for**, decided before the slice: the list is given to `Task.whenAll`, or
     emptied by a loop `while tasks.length() > 0` (`!= 0`, `0 <` and `0 !=` too) with no `break`, which ends
     only when no task is left; otherwise it is a compile error, on some paths too, and so is giving it
     another list or dropping it with `_ =`. After that loop the list counts as used up: using it again is
     an error whose hint is a new list. A way out with an error waits for what is left in it, and what
     they give is dropped (357).
444. **`Task.whenAll(tasks)` takes the list** and gives a `Task<List<R>>`, a `Task<void>` when the tasks
     give nothing: what each gave, in the order of the list, not in the order they ended (C#'s order). An
     error is an item of the list, so none is lost; C#'s `await Task.WhenAll` throws only the first. An empty
     list gives an empty list at once.
445. **`Task.whenAny(ref tasks)` gives what the first task to end gave**, decided before the slice, and
     takes that task out of the list, as Rust's `JoinSet::join_next`; the list is lent with `ref` because it
     changes. It cancels nothing, and the tasks left are waited for as any are (443). An empty list is a
     panic: no task could end.
446. **Both are awaited, or waited for, where they are written**: `await Task.whenAll(tasks)` in an async
     function, `Task.whenAll(tasks).wait()` in code that is not async, the same for `whenAny`. The task of
     either kept in a variable is an error whose hint is to await it there: the one of `whenAny` borrows the
     list, and one of `whenAll` kept for later would need a task of its own that waits for the others (Not
     here yet). `wait` and `await` on them are what waits, so a `lock` block refuses them and an async
     function refuses `wait`, as for any task.
447. **No `isCompleted`**, decided before the slice: what C# keeps on a task (`IsFaulted`, `IsCanceled`,
     `Exception`) is in what an SG task gives (`Error(...)`, `IoError.Cancelled`), and a loop that asks
     whether a task has ended without waiting would never end in the interpreter, where tasks change hands
     only where one waits. `Task.whenAny` answers "which has ended".
448. **In the interpreter**, `whenAll` waits for the tasks one after the other in the order of the list,
     so a task of `Task.run` runs when its turn comes. `whenAny` takes the first task in the list that has
     ended; when none has, the fiber waits for all of them at once and, if one is a task of `Task.run` that
     has not run, the first such runs then, as if it were waited for. A fiber woken by one task is taken off
     the others, and off the queue of fibers to run, so a second task that ends does not wake it later. The
     output is the same on every run.
449. **Compiled, a list of tasks has a group** (`sg_group`): the list holds pointers to the tasks, and the
     group is made with the first task added: a mutex, a condition, the tasks that have ended in the order
     they ended, and the async task waiting, if one is. A task added tells the group when it ends (its
     `waiter` with the second bit set), or at once if it has ended already, as the last thing it does with
     the group, under the lock; the group is freed only after every task in it has told it, so a task that
     ends on another thread never meets a freed group. `whenAny` takes the first task the group was told
     of, so compiled code gives the first that ended; `whenAll` waits until as many have ended as the list
     holds and takes them in the order of the list. An async task that waits is woken; code that is not async
     sleeps on the condition. A task block grows by one pointer (`later`).
450. **The names in C**: `sg_group` and its functions, `sg_all_L_TK_R` (what `whenAll` gives),
     `sg_any_L_TK_R` (one task taken out of the list) and `sg_when_any_empty` (the panic).
451. **Measured** (`bench/tasklist`): in each of 200,000 rounds eight async tasks are taken with
     `Task.whenAll` and eight with `Task.whenAny`, and none has to wait: 0.476 s against 0.384 s for C,
     which keeps each result in a block it allocates, 1.24 times; 0.689 s in a checked build. The same
     sixteen tasks kept in sixteen variables and awaited one by one take 0.406 s (1.06 times, the cost of
     a task, 430), so the list adds about 0.07 s, some 22 ns a task: a group for each list (an allocation,
     a mutex and a condition made and unmade), a lock each time a task tells it and each time one is
     taken. In "Slower than C".
452. **The tests**: `ok_task_list` (`whenAll` in the order of the list, `whenAny` in the order the tasks
     end, tasks that give nothing with a delay among them, an empty list, a way out with an error that
     waits for the task left in the list, two hundred tasks that end at about the same time on the
     threads of the pool), `ok_task_list_threads` (tasks of `Task.run` and of async functions in one list,
     from code that is not async, two hundred of them ending on other threads while the list is taken
     from), `panic_when_any_empty` and eighteen programs the checker refuses (`error_task_list_*`,
     `error_when_*`). The two programs run under the thread sanitizer too. `cargo test` compiles 237 of
     970 programs, 30 of them under the thread sanitizer, and 13 of 17 programs of tests; on Linux too, in
     a container, with the same counts.

Choices made in the sixty-fourth slice (async, the sixth part: streams opened, written and closed the
async way), in the checker, the interpreter and compiled code together, approved on 15 November 2026:

453. **A writer dropped in an async function is closed the async way**, decided before the slice: where a
     writer ends without `closeAsync` (a way out with an error, the end of a block, a field, an item or a
     `shared` value that ends, a variable given another value), what its block holds is written and its
     file closed on a thread of input and output, and the function awaits it there; the error of that
     writing is lost, as when a writer ends without `close` in code that is not async. A cancelled token
     does not stop it, and a panic writes nothing, as before. Until this slice the writer was closed on
     the thread of the pool, which held it for as long as the writing took. C# does the same with
     `await using` (`DisposeAsync` awaited on the way out of an exception), Python with `async with`;
     Rust has no async drop, and Tokio's `BufWriter` loses what it holds.
454. **In the order the names end**, decided before the slice: the writers and the tasks left behind on a
     way out with an error end together in the reverse order of their names, one after the other: a task
     declared after a writer is awaited before the writer is closed. The interpreter kept the names of a
     block in no set order, so its writers and tasks ended in another order; its names are now kept in the
     order they are declared and end in the reverse one, as compiled code always did, and a task left
     behind is awaited where its name ends.
455. **Compiled, a dropped writer goes on its task's list**: the drop of a writer in the machine of an
     async task (`sg_job_now` set) moves the writer to the end of a list in the task (`sg_task.closing`,
     one pointer more in every task) instead of writing it. Where the function awaits the closing (after a
     statement that reaches a writer, and after the drop of a variable that owns one on a way out), the
     list is handed to a thread of input and output (`sg_closing_run`), which writes and closes the writers
     in the order they were dropped while the task waits. A function that reaches no writer, through a
     value or a borrow, has no such place and costs nothing more. A writer dropped inside a `lock` block is
     closed after the block, which does not wait. A writer still on the list when the task ends is closed
     there, a net under the compiler that no test reaches.
456. **A function that is not async may drop a writer**, changed from what was decided before the
     slice. It was decided that such a function counts as holding the thread when an async function
     calls it (the rule of choice 391). Written, there is no need: called from an async task, it runs on
     the task's thread with `sg_job_now` set, so the writer it drops goes on the task's list like any other
     and is closed where the caller awaits it, after the line of the call. Nothing is written on the thread
     of the pool, and the rule would have refused functions that only take a writer by value and give it
     back, or end with an error.
457. **`File.openTextAsync`, `File.createTextAsync` and `File.openAppendAsync`** give the stream of the
     function without `Async` as a task, with a token that may come last, and are awaited where they are
     written, since a path is lent (`read string`). The hint for `File.createText` in an async function is
     now `await File.createTextAsync(...)`.
458. **The async methods of a writer**: `writeAsync(x)` and `writeLineAsync(x)` (what `print` takes),
     `writeBytesAsync(bytes)` and `flushAsync()`, each with a token that may come last, and `closeAsync()`,
     which takes the writer, as `close` does, and meets the `Closable` check. They lend the writer, so they
     are awaited where they are written; so is `closeAsync`, which needs no borrow, so that the file is
     closed before the next line. `closeAsync` takes no token: closing is not cancelled. In an async
     function the hint of the `Closable` check is `try await w.closeAsync()`, and that of a writer's method
     without `Async` is the method with it.
459. **What the block can take is written in place**, decided before the slice, as a read the block can
     answer (435): `writeAsync` and `writeLineAsync` write into the block of 64 KB on the task's thread,
     and only a block that is full is written out on a thread of input and output while the task waits.
     `writeBytesAsync` puts fewer than 64 KB into the block the same way; 64 KB or more go to a thread,
     which writes the block and then the bytes straight. `flushAsync` is done in place when the block is
     empty, `closeAsync` always on a thread (closing a file on a network can wait too).
460. **`Stdout` and `Stderr` have the async methods**, decided before the slice, done in place as `print`
     writes, in order with it: a function that takes a `ref StreamWriter` and awaits `writeLineAsync` works
     when it is given `Stdout`. `Stdout.closeAsync()` is an error, as `close` is, with `flushAsync` as the
     hint.
461. **A cancelled token writes nothing**: the method gives `IoError.Cancelled(...)`, what was put into
     the block for this call is taken out again, and the writer goes on. The token is looked at after the
     value is computed, as the interpreter does.
462. **In the interpreter** the async methods and functions do the work of those without `Async` when
     they are called, and give a task that has ended, as the reads do (412); the token is looked at
     first. A dropped writer is written there, on the task's own thread.
463. **The names in C**: `sg_io_writer` and `sg_io_run_writer` (the block, the bytes or the closing on a
     thread), `sg_writer_wrote_n`, `sg_writer_bytes_n` and `sg_writer_flush_n` (the work done in place,
     -1 when the block has to go), `sg_closing`, `sg_closing_put`, `sg_closing_io`, `sg_closing_run` and
     `sg_closing_left`, and `sg_drop_writer_now`, the drop that writes. `SG_CLOSING` is 1 in a program
     with an async function that reaches a writer.
464. **Measured** (`bench/asynccopy`): the log of `lines` (two million lines, 125 MB) copied line by line
     the async way, each line with its number in front, opened with `openTextAsync` and
     `createTextAsync`, read with `readLineAsync` and written with `writeLineAsync` in an async `main`:
     0.301 s against 0.275 s for the C of `copy` (`getline` and `fprintf`), 1.09 times; 0.299 s in a
     checked build. The same program without async takes 0.258 s (0.94 times C). What async adds, about
     0.04 s, is the hand-over of each block of 64 KB to a thread of input and output and back, some four
     thousand of them for the reads and the writes; a line itself is read and written in place. In
     "Slower than C".
465. **The tests**: `ok_async_writer` (every async method and function, more than the block holds, bytes of
     64 KB and more, a cancelled token, a missing folder, `Stdout` and `Stderr` in order with `print`),
     `ok_async_writer_drop` (a writer dropped on a way out with an error, given to a task that ends with
     one, a task declared after a writer awaited first, a writer in a struct given another value, in a list
     that is cleared, dropped by a function that is not async, dropped inside a `lock` block, and left in
     a struct at the end of the function; each file read on the next line is whole) and ten programs the
     checker refuses (`error_async_close_*`, `error_async_writer_*`, `error_async_create_blocking`).
     `error_async_writer` and `error_async_open_text`, which said "not here yet", are gone. The two
     programs that run are compiled and run under the thread sanitizer too, and no file they write is
     left half written. `cargo test` compiles 239 of 980 programs, 32 of them under the thread sanitizer,
     and 13 of 17 programs of tests; on Linux too, in a container, with the same counts.

Choices made in the sixty-fifth slice (the network: `TcpListener`, `TcpStream`, and a cancel that ends a
wait for a socket or for the input), in the checker, the interpreter and compiled code together, approved
on 15 November 2026:

466. **TCP first, in two types**, decided before the slice: `TcpListener` (`bind`, `port`, `accept`) and
     `TcpStream` (`connect`, `receive`, `send`), each call without async and with it (`acceptAsync`,
     `connectAsync`, `receiveAsync`, `sendAsync`, a token that may come last), in the checker, the
     interpreter and compiled code at once. An async call lends what it is called on, so it is awaited
     where it is written (409). A call without `Async` in an async function is an error, followed through
     the calls, with the async version in the hint, as for files; `bind` and `port` do not wait and are
     allowed there. Lines over a socket, a stream read by one task and written by another, and UDP come
     later.
467. **`receive` and `send`, not `read` and `write`**: `read` is a word of the language (a borrow), so
     `stream.read(...)` cannot be written. The names are those of C#'s `Socket`, and they keep the call
     apart from a reader's `readBytes`, which fills the buffer: `receive` gives what has come, at most
     what the buffer holds, and how many; 0 is the end, once the other side has let the connection go. That
     is the read the design left open for the network (04, "A read that gives what is there"). `send`
     sends all the bytes (Rust's `write_all`; C#'s `Send` may send part).
468. **An address is one text, `host:port`**, decided before the slice: `"127.0.0.1:8080"`,
     `"[::1]:8080"`, `"example.com:80"`; port 0 lets the system choose, and `listener.port()` tells which.
     `bind` takes a number, `localhost` (the v4 loopback) or no host (`":8080"`, every v4 address of the
     machine) and looks nothing up, so it never waits; there is no `bindAsync`. `connect` looks the host
     up by name and tries its addresses in turn; the error is the last one met. A listener lets its
     address be taken again at once (`SO_REUSEADDR`, as Rust does).
469. **Four variants of `IoError` for the network**, decided before the slice: `ConnectionRefused`
     (nobody listens), `ConnectionReset` (the other side broke the connection off, a write after it had
     gone included), `AddressInUse` and `HostNotFound` (the lookup found nothing, a name was given to
     `bind`, or the address is not `host:port`). A `match` that names every variant of `IoError` needs four
     more arms (`error_io_error_cancelled_arm` says so now). Writing to a connection the other side closed
     gives `ConnectionReset`, never the signal C would get.
470. **A socket has no buffer and no `close`**: `send` hands its bytes to the system, so nothing is left
     to lose, and a listener or a stream is closed where it ends, as a reader is; it is not `Closable`,
     and `close()` is an error whose note says so. A listener is reached through `read` (several tasks may
     take connections from one `shared` listener); a stream goes on from where it is, so it is received
     from and sent to through `ref`.
471. **A cancel ends a wait for the network at once**, decided before the slice: a token cancelled
     before the call gives `Cancelled` and nothing is done (412); one cancelled while the call waits for its
     socket takes it off and the call gives `Cancelled` at once. Nothing that came is lost: it stays in
     the socket for the next `receive`. A `send` that was cut short may have sent part of its bytes. A
     name being looked up is left to its thread.
472. **A wait for a line of the input ends with a cancel too**, decided before the slice: until the block
     of `Stdin` holds what the read needs, a thread reads once into it while the task waits, and a cancel
     ends the wait; what the thread reads stays in the block for the next read. This is for an input that
     can make the program wait (a pipe, a terminal); a file is always ready and is read as before. macOS
     cannot wait for a terminal with kqueue or `poll`, so the input is not put on the loop. One such
     thread is out at a time and one task waits for it; another task that reads at the same time, and a
     read that needs more than the block holds (a line longer than 64 KB), reads on a thread of its own and
     is not cut short. It closes what 412 and 433 left open for the input, in the interpreter and in
     compiled code.
473. **The network in the interpreter**: a socket does not block; a task whose socket is not ready gives
     the turn away, and when no task can run the scheduler waits for the sockets with `poll` until the
     first timer is due. `poll` is declared to the C library directly, so the interpreter still has no
     dependency. After such a wait the clock of the run goes on to the time that has gone by, since a
     wait outside the run is not the clock's to choose; a run with the network is as ordered as the
     network is. A connect, with its lookup, runs on a thread of its own, which tells the scheduler
     through a pair of sockets; so does the thread that reads the input. The input is read through a block
     of the interpreter's own, so the world of a run now owns its input (`World::input`, with
     `input_waits`).
474. **A task of `Task.run` starts when a task is about to wait outside the run** (in the interpreter):
     otherwise a server started with `Task.run` would never run while `main` waits for its answer, and
     compiled code starts it on a thread at once. The rule that such a task runs when it is waited for (14
     November) still holds for every program that does not wait for the network or the input.
475. **One thread waits for the sockets, in compiled code**, decided before the slice: epoll on Linux,
     kqueue on macOS, each wait put on once (one-shot) when the socket is not ready, and the task woken
     through the pool when it is. A cancel takes the socket off, and the thread of the loop wakes the task
     once it is done with the events it holds, so a wait is never looked at after its task went on: who
     ends a wait sets `settled` last. A listener is waited for through a copy of its socket (`dup`), since
     several tasks may wait on one `shared` listener. The thread starts when the first async call has to
     wait. A host written as a number is not looked up; any other is looked up on a thread of input and
     output, which a cancel leaves (the thread frees what it found). Since the eighty-second slice a
     socket goes on once, edge-triggered, its tasks wait in its slot, and there is no copy (678-683).
476. **A socket blocks until an async call uses it**: a call without `Async` is then one call of the
     system, and an async call makes the socket non-blocking once; every call still waits with `poll` when
     its socket is not ready, so the two kinds of call can share a socket. Sockets that never blocked took
     three calls for each `receive` that had to wait, and the threaded echo 1.5 times C; now 0.99.
477. **What it is in C**: `NET_RUNTIME` (`sg_listener` and `sg_socket`, a socket and its flags; the calls
     without async, `sg_net_bind`, `sg_net_accept`, `sg_net_connect`, `sg_net_recv`, `sg_net_send`) and
     `NET_ASYNC_RUNTIME` (`sg_loop_main`, `sg_wait_start`, `sg_wait_cut`, and `sg_net_op` with
     `sg_net_go` and `sg_net_more`, the call that goes on until it is done); in `ASYNC_IO_RUNTIME`, `sg_wait`
     and `sg_far` (work on a thread that its task may leave), and `sg_in_wait`, which fills the block of
     the input. A program that uses the network defines `_GNU_SOURCE` on Linux (`accept4`).
478. **Measured** (`bench/echo`): an echo server and eight clients in one async program over the
     loopback, each client sending 64 bytes and waiting for them to come back 20,000 times: 2.083 s against
     1.119 s for C, whose sockets block, with a thread for each connection and each client: 1.86 times. A
     wait wakes two threads where C wakes one: the system wakes the thread of the loop, which wakes a
     thread of the pool. In "Slower than C". The same program without async (`bench/echosync`, threads of
     `Task.run`) takes 1.108 s, 0.99 times C.
479. **The tests**: `ok_net_echo` (an async server with a task for each connection in a list, three clients
     at once, one of them connecting by name), `ok_net_sync` (the server on a thread of `Task.run`, a
     buffer smaller than the message, `localhost` bound and connected to), `ok_net_errors` (every new
     variant of `IoError`, addresses that are not `host:port`, without async and with it), `ok_net_cancel`
     (a cancel that ends a wait to accept and one to receive, a token cancelled before the call for each
     async call, and the bytes that came kept for the next `receive`) and twelve programs the checker
     refuses (`error_net_*`). Two tests in `golden.rs`, `compiled_input_cancel` and
     `interpreted_input_cancel`, give the input through a pipe only once the cancel has been seen. The four
     programs run under the thread sanitizer too. `cargo test` compiles 243 of 996 programs, 36 of them
     under the thread sanitizer, and 13 of 17 programs of tests; on Linux too, in a container, with the same counts.

Choices made in the sixty-sixth slice (lines over a connection: the reader of a connection, `send`
through a `read` borrow, the longest line and `readSome`; and a fault of async input and output), in the checker, the interpreter and compiled code
together, approved on 15 November 2026:

480. **The reader of a connection is a `StreamReader`**, decided before the slice: `conn.reader()` reads
     the connection through a block of its own, and every method of a reader works on it (`readLine`,
     `lines()`, `readText`, `readBytes`, `readSome` and the async versions), so a function that takes a
     `ref StreamReader` reads a file, `Stdin` and a connection alike. It borrows the connection (it is no
     copy of the socket, so the other side sees the end once the connection ends) and carries the borrow:
     its type is a `StreamReader` that lends (`Ty::Reader(true)`), lent where a `StreamReader` is lent,
     and refused where one that owns its file goes (a field, a return, a task), with a note that says why.
     Making it does not wait: there is no `readerAsync`, and it is called in an async function as it is.
481. **`send` goes through a `read` borrow, `receive` through a `ref`**, decided before the slice: as
     Rust's `Write for &TcpStream`, a connection sends while its reader holds it; a `receive` while the
     reader is used again is a compile error (`error_net_receive_under_reader`), and so is one inside a
     `for` over `reader.lines()`, so bytes are never taken from under the block. The rule of 470 that a
     stream is sent to through `ref` changes with it.
482. **Not through a `shared` connection**, decided before the slice: a `shared` stream may be owned by
     tasks on several threads, so `send` and `reader()` through one are errors whose note points to the
     stream read by one task and sent on by another, still to come; through `shared checked` (one task)
     and inside a `lock` they are allowed, as before.
483. **The longest line is 64 KB**, decided before the slice: a line (without its `\n` or `\r\n`), or
     the text of `readText`, longer than the limit is `IoError.LineTooLong`, a new variant (11); every read
     after it gives it too, so a connection that sent one is not read on by mistake. `conn.reader(limit)`
     gives another limit, a `nint` of at least 1 (a panic otherwise, `panic_net_reader_limit`). It is
     known as soon as the block holds more than the limit with no line end, not when the line ends. A
     reader of a file or of the input has no limit.
484. **What has come stays in the block until a read can take all it needs**: the reader of a connection
     gives out a line only once all of it is in its block, which grows to twice its size when it is full
     (up to the limit and a line end, or the buffer of a `readBytes`), so a cancel that ends a wait loses
     nothing that came (471 holds for the reader too, `ok_net_lines`). A `readBytes` without async, which
     nothing cancels, takes what the block holds and receives the rest straight into the buffer.
485. **`readSome(ref buffer)` on every reader**, decided before the slice: what the block holds, or what
     one read of the system gives when it is empty, at most what the buffer holds; 0 at the end, and 0
     at once for an empty buffer. It settles the open question of 04 ("A read that gives what is there")
     for every reader, the input included; `readSomeAsync` waits for the input on a thread, as the other
     reads do (472).
486. **In the interpreter**: the reader of a connection keeps what has come in a block of its own
     (`NetReader`), and a read that needs more receives into it, giving the turn away while the socket is
     not ready, as `receive` does (473).
487. **In compiled code**: `sg_reader` has the connection it reads (`sock`), the size of its block, the
     limit and the mark of a line too long; `sg_net_reader` makes it, and the reads of `IO_RUNTIME` go to
     the connection when it is there (`sg_sock_ready`, `sg_sock_wait`, `sg_sock_line`), receiving with
     `poll` as `receive` does. An async read that its block cannot answer receives what has come, and when
     nothing has, waits on the loop of the network (`sg_reader_net`, in the glue of each async read beside
     the fast path), so a program that uses the network and async input and output carries the loop. A
     reader of a connection frees its block and leaves the socket to the connection.
488. **Measured** (`bench/netlines`): the two million lines of the log of `lines`, 125 MB, sent over the
     loopback by one task, which reads the input in pieces of 64 KB, and read line by line by `main` with
     `await reader.readLineAsync(ref line)`: 0.117 s (0.117 s checked) against 0.080 s for C, whose
     sockets block, a thread sending and the main thread reading with `getline` from a `FILE` over the
     socket: 1.46 times. Every receive filled the block (1,916 of 64 KB), so the reader is what is
     measured. The check that each line is UTF-8, which C does not do, is 0.046 s of it: without it the
     program takes 0.071 s, 0.89 times C. In C the file is read by the other thread, so nothing hides the
     check here, as it does in `lines` (1.08); it is in "Slower than C". The same program without async
     (the sender a thread of `Task.run`) takes 0.103 s, and 0.055 s without the check.
489. **A task freed while a thread of input and output still woke it** (a fault since the sixty-second
     slice, found by the thread sanitizer on Linux in `ok_async_io_tasks`): the thread marked the work
     done and then woke the task, and a task that saw the mark in between went on, ended and was freed
     before the wake touched it. Now `done` is 2 while the thread wakes the task and 1 once it has let go
     of it, and a task that sees 2 waits for the 1, a few instructions (`sg_io_done`, `sched_yield`).
490. **The tests**: `ok_net_lines` (an async server that answers each line while it reads, `\r\n` and a
     line sent in two pieces, the limit met and kept, `readSomeAsync` after a header line from the same
     block, and a cancel in the middle of a line that loses nothing), `ok_net_lines_sync` (the server on a
     thread of `Task.run`, a `for` over `reader.lines()` that sends while it goes, one function reading a
     connection and `Stdin` through `ref StreamReader`, `readBytes` then `readText`, and `readText` longer
     than the limit), `panic_net_reader_limit` and seven programs the checker refuses (`error_net_read_line`,
     `error_net_reader_async`, `error_net_reader_blocking_in_async`, `error_net_reader_returned`,
     `error_net_reader_shared`, `error_net_receive_under_reader`, `error_net_send_shared`). The two programs
     that run are compiled and run under the thread sanitizer too. `cargo test` compiles 246 of 1006
     programs, 38 of them under the thread sanitizer, and 13 of 17 programs of tests; on Linux too, in a
     container, with the same counts.

Choices made in the sixty-seventh slice (checking UTF-8 a block at a time, in compiled code), decided
before it on 15 November 2026 after a measurement, approved the same day:

491. **A reader checks its block at once**, decided before the slice: a line is checked with the rest of
     the block, from where the check stands, and the lines inside the part known to be text cost
     nothing; where it stands is kept in the reader (`checked`, counted as `done` counts, so a block that
     is filled again or moved keeps it right). A line starts after a `\n`, which never lies inside a
     character, so what is found is what a check of the line alone finds: the same lines are text, and a
     bad one gives the same place; after it the check goes on from the next line. A line that two blocks
     share (one in 64 KB) is checked whole, as before; a read of bytes (`readBytes`, `readSome`), which
     can stop inside a character, lets go of what is known. It is so for a file, the input and a
     connection. A measurement before the slice found that a check of each line costs about the same
     whatever the way of checking (0.033 s to 0.042 s for the two million lines, the reading itself
     0.019 s): the lines are short (61 bytes), and the cost is in each call and in where the `é` falls.
492. **The check itself**, decided before the slice: sixteen bytes at a time while they are ASCII, then
     eight, the ASCII after a character byte by byte without going back to the top, and a character of two
     bytes looked at first (`sg_utf8_valid`, which gives how many bytes from the first are whole
     characters; `sg_utf8_bad` is made of it). It is the same C on every machine: 6.3 GB/s against 4.3
     before on the whole log. A check with NEON (Keiser and Lemire's, as `simdjson` and `simdutf` do) was
     measured at 8.8 GB/s, some 6 ms less on the log, but only on arm64; x86 needs SSSE3, which a
     program would have to look for when it starts. It is left in "Slower than C". `readText`,
     `string.fromUtf8` and the command line use the new check too.
493. **Measured**, each before and after on the same machine (best of seven): `lines` 0.096 s → 0.075
     s against 0.089 s for C (1.08 → 0.84 times), `copy` 0.253 → 0.226 against 0.271 (0.93 → 0.83),
     `asynclines` 0.134 → 0.112 against 0.088 (1.27 now), `asynccopy` 0.305 → 0.284 against 0.274
     (1.04), `netlines` 0.115 → 0.092 against 0.079 (1.46 → 1.16). The table above has the new times.
494. **The interpreter is not changed**: it checks each line with Rust's `from_utf8`, which is what
     compiled code must agree with.
495. **The tests**: `ok_text_blocks` makes a file of 388 KB with characters of one to four bytes, some cut
     by the end of a block, bytes that are not UTF-8 (an overlong form, a lone continuation, a
     surrogate), lines ended by `\r\n` and a last line cut inside a character, and reads it with
     `readLine` and with a `readBytes` of three bytes every 97 reads, from the file and from a connection;
     the lines, the bad ones and their places agree with the interpreter. Two faults put into the C on
     purpose (no letting go after `readBytes`, a block taken as text without a check) change what it
     prints. Every way of checking was compared with the old check on twenty million random sequences
     and on every sequence of three bytes. `cargo test` compiles 247 of 1007 programs, 39 of them under the
     thread sanitizer, and 13 of 17 programs of tests; on Linux too, in a container, with the same counts.

Choices made in the sixty-eighth slice (printing a fractional number with Dragonbox), decided before it
on 15 November 2026, approved the same day:

496. **The digits of a fractional number are the fewest that read back as it, and of those the nearest; a
     tie between two goes to the even digit**, decided before the slice. Until now (150) the number was
     rounded to 1, 2, ... digits until it read back. The two rules differ only next to a power of two,
     where the number below is twice as near as the one above: the number rounded to sixteen digits may
     not read back where another of sixteen does. 46 of the 2,046 powers of two of a `double` now print
     one digit fewer: 2^-1017 was `7.1202363472230444e-307` and is `7.120236347223045e-307`. The new rule
     is JavaScript's (whose layout SG already has), Python's `repr` and Java's since 19. Rust's `{}` and
     `{:e}` give the same digits but for a tie, where they take the larger: `160057272853489.625` is
     `160057272853489.62` here, in JavaScript and Python, and `...489.63` in Rust.
497. **Compiled code finds the digits with Dragonbox** (Junekey Jeon, 2020), decided before the slice:
     the fastest of the known ways; Ryu (Rust's `ryu`, `serde_json`) and Schubfach (Java) give the same
     digits, so one could take its place later without a program printing anything else. The C is
     written here (`shortest.rs`); its tables, 619 powers of ten in 128 bits for a `double` and 78 in 64
     bits for a `float`, each rounded up, are made by `sg` itself with arithmetic on numbers of any size,
     and are only in a program that writes a fractional number (`SG_FLOATS`): one whose code calls a
     helper that does. The exponent is written by hand too, so printing calls no `snprintf`.
498. **The interpreter takes the count of digits from Rust's `{:e}`**, and the number rounded to that
     many digits with `{:.*e}` (which rounds a tie to the even digit) when it reads back; when it does
     not (next to a power of two), the digits `{:e}` gave. Both print the same digits by the rule, each
     by its own way.
499. **Measured** (`floats`, a new program): 0.251 s against 0.762 s for C's `%.17g` (0.33 times), in
     the overflow-checked build too; with the old search 4.627 s (6.07 times). The entry in "Slower than
     C" is gone. Printing a number in C with the fewest digits needs a library such as Ryu's; with one
     the times would be about the same.
500. **The tests**: `ok_float_digits` prints ties, the powers of two whose digits changed, the largest
     and smallest numbers, `1e+22` and `1e+23`, and `float`s, the same in the interpreter and compiled
     code. Outside `cargo test` (it takes minutes), the C was compared with the rule written in Rust on
     every one of the 2^32 `float`s, on 400 million random `double`s, on every power of two and the
     numbers next to it, the whole numbers below two million and those divided by a thousand, numbers
     like `n e p` for p from -300 to 300, the smallest and the largest million, and with the old search
     on five million random numbers that are not powers of two: no difference. `cargo test` compiles 248 of
     1008 programs, 39 of them under the thread sanitizer, and 13 of 17 programs of tests; on Linux too, in a container, with the same counts.

Choices made in the sixty-ninth slice (raw memory: `raw T`, `unsafe`, `Memory`), the first of the three
slices of calling C. 501 to 510 were decided before it on 15 November 2026; 511 to 516 were chosen while it
was written, and all were approved the same day:

501. **`raw T` is an address that owns nothing, and it points at plain data**, decided before the slice:
     numbers, `bool`, `char`, raw pointers, and arrays and structs made only of them, or `void`. A `string`,
     a `List`, a `T*`, a `shared T` or a struct that holds one is an error that names the part (`it holds a
     string`): such a value owns memory of its own, and raw memory knows nothing of owners. So raw memory is
     read and written by copying bytes, and the `put` and `take` of the design (for items that own) come
     later, with them. Plain data lies as C lays it: the fields in the order they are written, each at the
     next place its alignment allows, so the same bytes can be handed to C in the next slice. `raw raw byte`
     is a pointer to pointers; `raw` stands right before the type, after `shared` if there is one (`raw shared
     int` is an error). A raw pointer is copied like a number, and is not `Formattable`: an address differs
     from one run to the next.
502. **What `unsafe` is written around**, decided before the slice: an item of raw memory, read or written
     (`p[i]`, `p[i].x`, `p[i].values[2]`), `p + n` and `p - n`, a cast between raw pointers (`(raw byte)p`),
     `Memory.free` and `Memory.copy`. Making, keeping, copying and comparing a pointer (`==`, `!=`, with
     `null` too) is not, and neither is `Memory.allocate`: memory that is never given back is a leak, not
     undefined behaviour. Without `unsafe` it is an error that says why the checker cannot follow it (the
     size and life of raw memory, a pointer that may point outside its memory, ...).
503. **`unsafe { ... }` around statements, `unsafe` in front of a value**, decided before the slice. The
     prefix takes what follows it as `try` and `await` do: `unsafe p[0] + 1` reads one item and adds to it,
     and `unsafe p + 1` is `(unsafe p) + 1`, which is the error of 504 with the hint `unsafe (p + n)`. A block
     is not a value (`var x = unsafe { p[0] }` is an error that shows the prefix), and a write is a statement
     in a block: `unsafe p[0] = 1` is an error whose hint is `unsafe { p[0] = ... }`. A block is a scope of
     its own, as every block is.
504. **An `unsafe` that nothing needs is an error, and so is one inside another**, decided before the slice,
     since the language has no warnings: `nothing here needs unsafe`, with the list of 502. Then the word is
     seen only where the checker really stops following.
505. **`Memory.allocate<T>(n)` gives room for `n` items, all zero**, decided before the slice: there is no
     memory that was never written, and zero is a value of every plain type. A count below 0 and more bytes
     than an address can count are panics, compiled too, and so is a system out of memory; `n` may be 0, and
     what it gives is still given back with `Memory.free`. Compiled it is `calloc`.
506. **A raw pointer may be `null`**, decided before the slice, as in C and Rust: `raw int p = null`, `p ==
     null`. `raw int?` is an error (it can be `null` already), and so is `p is null`, whose hint is `p ==
     null`: `is` is for a `T?`, which is opened before it is used, and a raw pointer is not.
507. **`Memory.copy(source, target, count)` copies `count` items, also when the two overlap**, decided
     before the slice (C's `memmove`; the order of the arguments is the order of the words). A count below 0
     is a panic, compiled too. The design's `moveItems` is left for items that own memory.
508. **`raw void` is C's `void*`**, decided before the slice: it is cast to a pointer to some type before
     anything else is done with it; an item of it, `p + n` and `Memory.copy` of it are errors whose hint is
     the cast. `Memory.free` takes it.
509. **The interpreter checks raw memory, compiled code does not**, decided before the slice. A pointer in the
     interpreter is a real address and the block of `Memory.allocate` it came from; an item outside its
     block, one after `Memory.free`, one through `null`, a block given back twice, and `Memory.free` of an
     address inside a block are panics that say so (`raw memory is read outside its block: item 4, where the
     block holds 4 items of int`). A pointer read back out of raw memory finds its block by its address; one
     whose block was given back and whose address a later block took is taken for a pointer into the later
     one. A `char` read out of raw memory must be a character. Compiled code checks none of it and costs
     nothing for it; the tests run it under the sanitizers. So a program with `unsafe` is tried in the
     interpreter first, as Rust's is tried under Miri.
510. **A raw pointer does not go to another task**, decided before the slice: not through `Task.run`, not
     to an async function called without `await` (a task of its own), not inside a `shared` or `locked`
     value that does: the checker does not follow it, and two tasks could reach what it points at at once.
     An async function that is awaited runs as part of the awaiting task and may take one, and a raw
     pointer may be held across an `await`: the task may go on on another thread, but never on two at once.
511. **A place in raw memory is not lent yet**: `read p[i]`, `ref p[i]`, and a method of a struct or an
     array called on it (a method is lent what it is called on) are errors whose hint copies the item out
     (`var item = unsafe p[i]`) or writes it where it is. The checker cannot follow how long such a borrow
     lives, and the interpreter would have to lend bytes; Rust lends them, under `unsafe`. Left in "Not here
     yet".
512. **Reading an item of raw memory copies it, whatever its type**: plain data owns nothing, so a struct
     that is not `Copyable` is copied out too (`var p = unsafe points[0]`), and the copy is a value of its
     own. A write through a `read raw int` writes the memory, not the pointer: `read` keeps the pointer as it
     is, not what it points at, as `&*mut T` does in Rust.
513. **The rest of the notation**: a raw pointer has no fields (`p.x` is an error whose hint is
     `p[0].x`), the pointer is written first in `p + n` (`1 + p` is an error that shows it), the count is an
     integer that fits a `nint`, and `+=` does not move a pointer; pointers have no order (`<`) and no
     difference (`p - q`) yet; two pointers of different types are compared after a cast; `Memory` is a
     built-in name, and `Memory.allocate(4)` without the type is an error that shows it.
514. **Compiled code**: a `raw T` is a `T*` and a `raw void` a `void*`; `p[i]` is `p[i]`, `p + n` is `p + n`,
     a cast is a C cast, `Memory.free` is `free` and `Memory.copy` is `memmove` behind the test of the count.
     The two functions it needs (`sg_raw_alloc`, `sg_raw_copy`) are only in a program that uses them.
515. **Measured** (`ring`, a new program): a ring buffer of its own over raw memory, four hundred million
     numbers in and out, takes 0.328 s, 0.326 s in the overflow-checked build, against 0.328 s for the same
     program in C over `calloc` (1.00 times): an item of raw memory is the item C reaches.
516. **The tests**: `ok_raw_memory` writes and reads items of every plain type, moves pointers both ways,
     copies to the right and to the left over the same memory, reads a struct back byte by byte (so the
     interpreter and C must lay it out alike), keeps pointers in raw memory, and grows a list of its own
     (`Numbers`, the design's `List` written for `int`); `ok_raw_async` holds a pointer across `await`. Nine
     `panic_unsafe_` programs show what the interpreter catches (they are not compiled), three `panic_raw_`
     programs the panics of both, and 35 `error_unsafe_` and `error_raw_` programs the errors of 501 to 513.
     `cargo test` compiles 253 of 1057 programs, 40 of them under the thread sanitizer, and 13 of 17 programs
     of tests; on Linux too, in a container, with the same counts.

Choices made in the seventieth slice (calling C: `extern` blocks, `CString`, `buffer.pointer()`), the
second of the three slices of calling C. 517 to 528 were decided before it on 15 November 2026 (with the
direction in "Calling C, the direction"); 529 and 530 come from the slice, and all were approved the same day:

517. **An `extern` block declares functions of C**, decided before the slice: `extern "z" { ... }` for a
     library, `extern { ... }` for the C library itself, at the top level of a file only, each function on
     a line of its own with no body (a body is an error whose hint is to write it outside). C names are
     written as C writes them (`compressBound`), the one place the camelCase rule does not hold. The
     functions are private to their module; `public` is written on each that the users of the module may
     call (`public extern { }` is an error that shows it), so a module can wrap a library in a safe
     interface and keep every `unsafe` inside: `examples/zlib` does that for zlib.
518. **A C name is a name of the module**, decided before the slice: a function of SG and a function of C
     of one name in a module are "defined twice", a built-in name cannot be taken (`print`), and the same C
     function declared in two modules must be declared the same way in both (`the C function abs is
     declared twice, and the two declarations differ`, with both places): C has one function of each name.
519. **A library is named as the C compiler links it**, decided before the slice: `extern "z"` is `-lz`.
     The name is letters, digits and `_ - . +` and does not start with `-`, so nothing else reaches the C
     compiler through it (`extern "z -o x"` is an error). Only the libraries of the functions a program
     calls are linked. Compiled, they are written on the first line of the C (`/* sg-link: -lz */`), where
     `sg build` and the tests read them; `-lm` is given to every program already. `sg build -L path` for
     a library the C compiler does not find by itself, `pkg-config` and frameworks come later. *(17
     November 2026, the ninety-sixth slice: `-L folder`, and `extern "z", "zlib"` for a library of several
     names, 963-971.)*
520. **What a C function takes and gives**, decided before the slice, with three details from it: the
     integer types, `float`, `double` and `bool` as their C types; `raw T` as `T*`; `ref T` as `T*` and
     `read T` as `const T*` of plain data, for the time of the call; `read CString` as `const char*`. It
     gives one of the first four, or nothing. A parameter has a name (`int abs(int)` is an error whose hint
     shows it), as in any function. `char` is an error whose hint is `byte` or `sbyte`: a `char` of SG is a
     character of 32 bits. `string` is an error whose hint is `read CString`, a list or an array one whose
     hint is the pointer and the count apart (`ref byte[16]` too: C takes the address of the first item,
     and `buffer.pointer()` gives it), a struct given by value one that says it is not here yet, and `...`
     is an error that says so too. A C function is not `async`.
521. **Every call of a C function is written inside `unsafe`**, decided before the slice: the checker
     cannot see what C does. The arguments are written as for any function (`ref size`, `read name`) and
     computed from left to right; what a C function gives may be left unused, as with a function of your
     own. A C function is not a `Func` (`Func<int, int> f = abs` and `Task.run(abs, x)` are errors whose
     hint is a function of your own around the call): a `Func` is the address of a function of SG.
522. **A borrow lent to C lasts for the call**, decided before the slice: C must not keep the address of a
     `ref` or a `read` argument; nothing can check it, and memory C keeps is taken with
     `Memory.allocate`.
523. **`buffer.pointer()` is written only as an argument of a C function**, decided before the slice: on a
     `List<T>`, a `T[N]` or a slice of plain data it gives a `raw T` to the first item, and in the call it
     counts as a `ref` of the buffer, so `memcpy(bytes.pointer(), bytes.pointer(), n)` is "lent twice"; a
     cast may stand around it (`(raw void)names.pointer()`). Anywhere else it is an error whose note says
     why (a pointer kept after the call could point at items that have moved) and whose hint is
     `Memory.allocate`; on a value that is not kept in a variable or borrowed, an error whose hint names
     it; on items that are not plain data, an error that names their type. Compiled it is the pointer to
     the items; the interpreter copies the items into memory of its own before the call and back after it.
524. **`CString` is a text with a zero byte after it, for C**, decided before the slice: owned, not
     `Copyable`, freed where its owner ends, moved like a `string`, not `Formattable`; it has no methods
     and is lent to C as `read CString` (`CString` by value is an error whose hint is that). `CString.from(text)`
     copies the text and gives a `Result<CString, TextError>`: a zero byte inside is the error, its
     `position` where the byte is.
525. **`unsafe string.fromCString(p)` copies the text C ended with a zero byte**, decided before the slice:
     `p` is a `raw byte` (or `raw sbyte`, C's signed `char`), the bytes up to the zero are checked to be UTF-8
     and copied, and the result is a `Result<string, TextError>`. In the interpreter a `null` pointer is a
     panic, and so is a block of `Memory.allocate` that holds no zero byte from the pointer to its end;
     compiled code checks neither, as with any raw memory.
526. **One allocator for SG and C**, decided before the slice: in the interpreter too `Memory.allocate` is
     C's `calloc` and `Memory.free` is C's `free`, so memory C gives with `malloc` (`strdup`) is given back
     with `Memory.free`, and the other way round. An address that no block of `Memory.allocate` starts at,
     and that lies in none, is C's and goes to C's `free`; until now that was the panic "an address that
     `Memory.allocate` did not give" (509).
527. **The interpreter calls C through wrappers it builds**, decided before the slice: before the program
     runs, it writes a C file with a wrapper for each C function the program calls, all of one signature
     (`void w(void **args, void *ret)`), builds it with `cc -shared` with the libraries the blocks name, and
     opens it with `dlopen`; a call writes each argument into a slot of eight bytes as its C type and hands
     over their addresses. No libffi, no new dependency. It is built in a folder of its own that only the
     user can open, made new for each run and removed once the library is open, so no other user can put a
     library there; nothing is kept between runs (a run that calls C takes about a tenth of a second more).
     A library or a function that cannot be found is an error at the first `extern` block, before anything
     runs; on Linux a function missing from the libraries is found when they are linked, as on macOS.
528. **Compiled code calls C directly**, decided before the slice: each C function is declared under a
     name of the runtime (`sg_c_compress`) bound to its C name with a label (`__asm__("compress")`, with the
     `_` macOS puts in front), so it clashes with no header the runtime includes, though its types are
     written as SG has them (`uint8_t*` where `getenv` gives `char*`); an inline adapter (`sg_x_compress`)
     takes the arguments as the program gives them and passes a `read CString` as the text it lends. A
     `CString` is a `uint8_t*` from `malloc`, freed with `free`.
529. **Measured** (`crc`, a new program): sixteen gigabytes through zlib's `crc32`, 64 KB of an array at a
     time (262,144 calls), take 0.418 s, 0.417 s in the overflow-checked build, against 0.421 s for the same
     calls in C through zlib's header (0.99 times): a call of C is a call of C. Apple's zlib computes the
     sum with the CRC instructions of the processor, so the calls, not the sum, would show any cost.
530. **The tests**: `ok_extern_libc` calls the C library and the math library (a `ref int` and a `ref
     double` written by C, a struct of plain data lent to `memcmp` and `memcpy`, what `strdup` gives
     freed with `Memory.free`, the items of a list, an array and a slice handed to `memset`), `ok_extern_zlib`
     goes through `compress` and `uncompress` and back, `ok_extern_cstring` makes, moves and measures C
     text and reads it back (a zero byte inside, bytes that are not UTF-8); two `panic_unsafe_cstring_`
     programs show what the interpreter catches, and 27 `error_extern_`, `error_pointer_`, `error_cstring_`
     and `error_from_cstring_` programs the errors of 517 to 527 (`error_extern_no_library` names a library
     that is not there: the interpreter says so before the program runs, and it is not compiled). `tests/modules/clib` is a module with a
     private and a public C function, with a program that calls them, one that calls the private one (an
     error) and one that declares it the other way (an error); `examples/zlib` is the safe wrapper, with
     its tests. `cargo test` compiles 258 of 1093 programs, 40 of them under the thread sanitizer, and 14 of
     18 programs of tests; on Linux too, in a container that has zlib, with the same counts.

Choices made in the seventy-first slice (`Disposable` and `Closable` on types of your own, opaque C types
and C structs by value), the last of the three slices of calling C. 531 to 540 were decided before it on 15
November 2026 (with the direction in "Calling C, the direction"); 538 was built another way, and 541 to 546
come from the slice, and all were approved the same day:

531. **`Disposable` on a struct of the program**, decided before the slice: `struct T : Disposable` has one
     method `void dispose(ref this)`. Without it, `: Disposable` is an error whose hint is the method; a
     `dispose` on a type that is not `Disposable` is an error (it would never run); one that takes `this`
     or `read this`, takes arguments, gives a value or is `async` is an error that says how it is written.
     An enum cannot be `Disposable` for now: a struct that holds it can.
532. **When `dispose` runs**, decided before the slice: once, where the last owner of the value ends (the
     end of a block or of a function, a value written over, an item taken out of a list, a `T?` set to
     `null`, the last owner of a `shared` value, a way out through `try`), and then what the value holds
     ends, field by field in the order of the fields. Several values that end together end in the reverse
     order of their names, and the items of a list in their order. A panic and `exit` dispose of nothing.
533. **`dispose` is not called by the program**, decided before the slice: `x.dispose()` is an error, whose
     hint is `try x.close()` on a `Closable` type, and otherwise to hand the value to a function that
     takes it by value.
534. **A method that takes `this` ends the value**, decided before the slice: its `dispose` runs where the
     method ends, also after `close`; a type that frees something in such a method keeps a field that says
     so (`ended`), as in C#'s `Dispose` pattern. In the interpreter a receiver handed over now leaves its
     place empty, as an argument does, so it ends where compiled code ends it.
535. **No field is moved out of a `Disposable` value**, decided before the slice (row "`Disposable` and
     moving a field"): an error everywhere, inside its own methods and `dispose` too, and through a field of
     a field (`pair.first.name`); the hint is `take` or `replace`, which leave the value whole.
536. **What a `Disposable` type is not**, decided before the slice: not `Copyable` (both are an error: each
     copy would be disposed of) and not plain data, so it is not in raw memory (`Memory.allocate<T>`, a
     `raw T`) or in a signature of C. It is in a `List`, a `HashMap`, a `T?` and a `shared` value, and goes
     to another task, unless it holds a `raw T` (row "C and tasks"), so a type that wraps a C library does
     not go to another task for now.
537. **`Closable` on a struct of the program**, decided before the slice: it asks for a `close(this)` that
     gives a `Result`, and a local of the type, a parameter it takes by value, a name of a pattern and the
     item of a `for` are closed before they end, or before they are given another value, as a `StreamWriter`
     is (choice 210): the error says that the error its `close` gives would be lost, and the hint is `try
     x.close()`. `this` is exempt inside the type's own methods, which are what close it. A `Closable` type
     need not be `Disposable`.
538. **Compiled `dispose`**, decided before the slice as a C function with a flag only on a local whose
     move depends on a condition; **built another way**: a `Disposable` struct has one byte more
     (`sg_gone`), which a move sets and its drop looks at before `dispose` runs. The back end already
     empties every place a value is moved out of and tests it where it is dropped (a text leaves `NULL`, an
     enum a tag that is no variant); the byte is that rule for a struct with nothing else to empty, and
     costs a byte, a store on a move and a test on a drop. A value is made with its byte at 0, so every way
     of making one gives a live value.
539. **How the interpreter runs `dispose`**: the fields of a value of a `Disposable` type name it, and when
     the last place that holds them lets them go, they wait in a list; the interpreter runs `dispose` on
     them before and after each statement, at the end of each block and of each call, and runs what a value
     held before the next value. That is the order of compiled code, which drops a value field by field. A
     value read from a place (a copy) shares the fields of a `Disposable` one, so it is disposed of once. A
     `for` handed a collection takes each item out as its turn comes, so an item ends at the end of its
     turn, as in compiled code, and not with the collection.
540. **Types of C seen only behind a pointer**, decided before the slice: `extern "sqlite3" { struct sqlite3
     }` declares a type of C without a body, written only as `raw sqlite3`; `ref raw sqlite3` is C's
     `sqlite3**`. An item of it, `p + n`, a field, `Memory.allocate<sqlite3>`, a value, a list of it and a
     struct built of it are errors, and so are `:` and a body after its name. Private to its module unless
     `public`. Compiled, it is a C struct that is declared and never defined.
541. **Structs of C by value**, decided before the slice: a struct of plain data goes to C and comes back by
     value (`Div div(int numer, int denom)`); a struct that holds something else is an error that names
     what it holds. Its fields have names of SG, and only their order and types count. `ref raw T` and
     `read raw T` are now in a signature (`strtol`'s `char**`). The interpreter writes the struct into the
     C file of its wrappers, those it holds first, and hands its address in a slot (a struct C gives back is
     written into room after the arguments); compiled code passes the struct itself, and writes `read T` as
     `T const*`, so that a `read raw T` is a pointer to a pointer that C does not change.
542. **zlib's stream behind a safe type** (`examples/zlib`, decided before the slice): `Deflater : Disposable,
     Closable` holds a `z_stream` in raw memory, because zlib keeps its address and a value of SG moves.
     `Deflater(level)` gives a `Result` (a level outside 0 to 9 is the new `ZlibError.BadLevel`), `write`
     compresses a piece, `take` hands over what is compressed so far, `close` ends the stream and gives the
     rest, and `dispose` gives back what zlib holds and the two blocks of 64 KB, closed or not. What is
     written is copied into raw memory a byte at a time: `buffer.pointer()` lasts only for a call (523),
     and zlib reads its input across calls.
543. **Measured** (`deflate`, a new program): a gigabyte at level 1 through the `Deflater`, 64 KB at a time,
     takes 1.001 s, 1.001 s in the overflow-checked build, against 0.962 s for the same calls in C through
     zlib's header (1.04 times). The difference is the copy of each piece into raw memory (542), noted in
     "Slower than C".
544. **The tests**: `ok_disposable` goes through the ways a value ends (a block, a call, a value written
     over, `removeLast`, `null`, a `shared` value with two owners, a move on one path, an enum, a `match`,
     a way out through `try`, a `for` handed a list, a map, a task, an async function), and
     `ok_disposable_closable` a `Closable` type whose `close` fails; their output is the same in the
     interpreter and in compiled code. `ok_extern_struct` gets `div_t` and `ldiv_t` back from C, gives
     `inet_ntoa` a `struct in_addr`, writes and reads a `FILE` that `tmpfile` makes, and lets `strtol` write
     a `char*` (`ref raw byte`). 18 `error_dispos` and `error_closable_` programs show the errors of 531 to 537,
     9 `error_opaque_` programs those of 540, and 2 `error_extern_struct_` programs those of 541;
     `error_extern_struct_value` is gone, since a struct by value is no longer an error. `examples/zlib` has
     three new tests (a stream given in pieces comes back whole, a level outside 0 to 9, a deflater that is
     never closed gives back what zlib holds, which the leak check of the sanitizers sees). A struct that
     holds a struct and an array, given by value and given back, was checked by hand against a small C
     library in both; the tests have only the C library and zlib to call. `cargo test` compiles 261 of 1124
     programs, 41 of them under the thread sanitizer, and 14 of 18 programs of tests; on Linux too, in a container, with the
     same counts.

Choices made in the seventy-second slice (the clock). 546 to 548, 550, 551, 553 and 554 were decided before
it on 15 November 2026, with the parts of 548 and 551 marked as coming from the slice; 545, 549, 552 and 555
come from the slice, and all were approved the same day:

545. **The clock is written in SG**, in the language's own module `std` (`src/std/time.sg`), which a program
     gets when one of its files names `Duration`, `Instant`, `DateTime`, `DayOfWeek` or `DateError`. Its
     types are seen from every module with nothing written, as `TextError` is; `use std` is an error whose
     hint takes the line away, a folder named `std` is not a module, and a program cannot define a type of
     these names. Its fields are its own: a field of it (`span.ns`) and a value of it built field by field
     are errors whose hints name its methods and functions. The two readings of the clock,
     `Clock.monotonic()` and `Clock.wall()`, are the compiler's, and only this module can call them. So
     equality, hashing, printing in a collection, sorting and compiled code are what every struct of a
     program has; nothing of the clock needed a new case in the interpreter or the back end besides those
     two readings. A message names the types as a program writes them, `Duration`, not `std.Duration`.
546. **`Duration`**: a span of time in a `long` of nanoseconds, which may be negative, `Copyable`,
     `Equatable`, `Hashable`, `Comparable` and `Formattable`. `Duration.fromNanoseconds`,
     `fromMicroseconds`, `fromMilliseconds` and `fromSeconds` take a `long`; `nanoseconds()`,
     `microseconds()`, `milliseconds()` and `seconds()` give the whole count cut toward zero, and
     `totalSeconds()` a `double`; `plus`, `minus` and `times(long)` are methods, since the language defines
     no operators (`span + span` is an error whose hint is `span.plus(span)`). The arithmetic follows the
     rule of the numbers: an overflow is a panic in a debug build and in a test, and wraps in a release
     build. Printed in the unit its size calls for (`ns`, `µs`, `ms`, `s`) with the fewest digits after the
     point: `1.5s`, `250ms`, `3µs`, `-2ms`, `0s`; seconds stay seconds (`90s`).
547. **`Instant`**: a reading of the monotonic clock, which never goes back and which nothing sets;
     `Copyable`, `Equatable`, `Hashable` and `Comparable`, and not `Formattable`: printing one is an error
     whose note says that it means something only next to another, and whose hint is `start.elapsed()`.
     `Instant.now()`, `start.elapsed()`, `later.since(earlier)` (negative when `earlier` is later, never a
     panic), `plus` and `minus` of a `Duration`. An `Instant` given where a `DateTime` goes, or the other
     way, is an error whose note says why they are two types. Read with `clock_gettime(CLOCK_MONOTONIC)`, in
     the interpreter too; on Linux it stops while the machine sleeps, on macOS it does not.
548. **`DateTime`**: a moment of the calendar in UTC, a `long` of nanoseconds since 1970 as the clock of the
     system counts them (no leap seconds), from 1677 to 2262; the Gregorian calendar, also before it was in
     use. `DateTime.now()` (`CLOCK_REALTIME`, which can be set and go back), `DateTime.fromUnixSeconds(n)`,
     `DateTime.fromUnixMilliseconds(n)`, and `DateTime.utc(year, month, day, hour, minute, second)`, which
     gives a `Result<DateTime, DateError>`: `InvalidMonth`, `InvalidDay` (29 February of a year that is not
     a leap year too), `InvalidHour`, `InvalidMinute`, `InvalidSecond`, or `OutOfRange` outside what a
     `DateTime` holds. `year()`, `month()`, `day()`, `hour()`, `minute()`, `second()`, `nanosecond()` and
     `dayOfWeek()` (a `DayOfWeek`, `Monday` to `Sunday`) take it apart; `unixSeconds()` and
     `unixMilliseconds()` count down to the second it lies in, so before 1970 a part of a second counts the
     earlier second. `since`, `plus` and, from the slice, `minus`. Printed in ISO 8601,
     `2026-11-15T09:30:05Z`, with the fraction in 3, 6 or 9 digits when there is one. The calendar is
     Howard Hinnant's arithmetic of eras of 400 years, in SG, not the C library's `gmtime`, which knows
     time zones and takes a lock. From the slice: the names of the errors, the order of their checks (the
     month first, so that the day is checked against it), and `DayOfWeek` without an order (`<` on it is an
     error whose note says it is a built-in type without one).
549. **A panic inside the language's own module points at its line there**: `Duration.fromSeconds` of a
     count too large stops at the multiplication in `<built-in>/std/time.sg`, as the same code would in a
     module of the program, and its hint names `tryMultiply`, which the caller cannot reach. Pointing at the
     call is left for later (Not here yet). *Done in the seventy-fifth slice (582): the panic is shown at the
     call, without the hint.*
550. **A delay waits its time in the interpreter too.** A delay was put on the clock of the run, which moves
     only when every task waits; after a program computed for a second, `Task.delay(100)` ended at once. Now
     the clock of the run still gives the order of the timers, so the output is the same on every run, and
     each timer also waits until its time has really gone by since it was made, as in compiled code. The
     same holds for `CancellationSource.after`.
551. **`Task.delay` keeps its `int` of milliseconds**, with no form that takes a `Duration` (the language has
     no overloads); given a `Duration`, the hint is `Task.delay((int)pause.milliseconds())`. There is no
     `sleep`: code that is not async waits for a delay kept in a variable (`var pause = Task.delay(100)`,
     `pause.wait()`). Found in the slice: `Task.delay(100).wait()` in one line is an error, since `wait`
     takes only a task kept in a variable, and its message says the task is borrowed; left in Not here yet.
     *Done in the seventy-fifth slice (581).*
552. **Compiled code** compiles the module like the program's own code; the two readings are `sg_clock`, one
     call of `clock_gettime`, in the runtime of a program that reads the clock, which asks glibc for POSIX
     as a program with threads does. The interpreter reads the same clocks with `clock_gettime`, declared to
     the C library directly, so it has no dependency.
553. **Measured** (`clock`, a new program): ten million times `start.elapsed()`, each compared with the one
     before, take 0.191 s, 0.180 s in the overflow-checked build, against 0.189 s for `clock_gettime` in C
     (1.01 times).
554. **Measured** (`dates`, a new program): ten million moments between 1900 and 2200 taken apart into their
     six parts and their day of the week take 0.121 s, 0.295 s in the overflow-checked build, against 0.413 s
     for `gmtime_r` in C (0.29 times): the C library looks at the time zone and takes a lock on each call,
     and the C compiler computes the date once for the three calls that ask for its parts.
555. **The tests**: `ok_time_duration` makes, reads, compares, sorts and prints spans, the largest and the
     smallest among them; `ok_time_date` takes dates apart, prints them with and without a fraction, goes
     through every error of `DateTime.utc` and the edges of the range, before 1970 too, and keeps a
     `DateTime` as a key; `ok_time_clock` measures delays, one after a computation among them. Their output is
     the same in the interpreter and in compiled code. `panic_time_overflow` and `tests/release/duration_wraps`
     show the rule of the numbers in a debug and a release build, `tests/modules/ok_time_module` a module that
     shows the types in what it makes public, and ten `error_time_` programs the errors of 545 to 551.
     `cargo test` compiles 267 of 1140 programs, 42 of them under the thread sanitizer, and 14 of 18
     programs of tests; on Linux too, in a container, with the same counts.

Choices made in the seventy-third slice (random numbers). 557, 558 and 561 to 563 were decided before it on
15 November 2026, with the parts marked as coming from the slice; 556, 559, 560 and 564 to 566 come from the
slice, and all were approved the same day:

556. **Random numbers are written in SG**, in the language's own module `std`, in a second file
     (`src/std/random.sg`): a program gets each file of the module when one of its files names a type of
     that file, `Random` or `SecureRandom` for this one, so a program that measures time does not carry the
     generator. The random bytes of the system, `Entropy.fill(ref bytes)`, are the compiler's, and only this
     module can call it; elsewhere `Entropy` is a name that is not there. A program cannot define a type
     named `Random` or `SecureRandom`. A value of either built field by field or with `Random(...)` is an
     error whose hint names the functions that make one.
557. **`Random`** is xoshiro256\*\*, its state of four `ulong`s set from the seed with SplitMix64 (the
     generator of C#'s `System.Random` since .NET 6). The same seed gives the same numbers in the
     interpreter, in compiled code and on every system; the algorithm is named in the documents, and a
     change of it would be a change of the language, written down with the version. Whoever sees some of its
     numbers can work out the rest, so its comment and the documents send keys and tokens to `SecureRandom`.
     It is not `Copyable`: a copy would give the same numbers again, without a word. Using a generator after
     it was moved is an error whose note says so and whose hint is `ref r`. It is not `Formattable`, its
     fields are its own, and it goes to a task like any value that owns what it holds.
558. **No generator the program does not make.** The language has no globals, so there is no `Random.Shared`
     as in C# and no `thread_rng()` as in Rust: `Random.fromSeed(long seed)` gives the numbers of that seed,
     `Random.fromSystem()` takes 256 bits of the system (drawn again in the case, once in 2^256, of all
     zeros, where the generator would stand still), and the program lends the generator with `ref`.
559. **The methods**, all `ref this`: `nextInt(int low, int high)` and `nextLong(long low, long high)` give a
     number from `low` up to `high`, without `high`, as C#'s `Next` and Rust's `gen_range(low..high)` do,
     every number as likely as the others. `nextInt` takes Lemire's way: one multiplication of 32 random bits
     by the size of the range, and a second draw only when the first would favour some numbers. `nextLong`
     takes the bits of a draw that the range needs and draws again until they fall inside it, since the
     language has no multiplication of 128 bits; on average fewer than two draws. Neither overflows, also
     from `long.MinValue` to `long.MaxValue`. `low >= high` is a panic in every build, with both numbers.
     `nextDouble()` gives one of the 2^53 evenly spaced numbers from 0 up to 1, `nextBool()` the highest bit
     of a draw, `fill(ref bytes)` eight bytes from each draw, the lowest first (a last part takes a whole
     draw).
560. **A change from what was decided before the slice: `nextUlong()`, not `nextLong()`.** The language has
     no overloads, so `nextLong()` for 64 random bits and `nextLong(low, high)` for a range cannot both be
     there. The range keeps the name, as `nextInt` has it, and the 64 bits are `nextUlong()`, unsigned like
     the bits they are (`r.nextUlong().wrappingToLong()` for a `long`). `nextLong()` with nothing is an error
     whose hint names both. `SecureRandom` has the same names.
561. **`SecureRandom`**: `SecureRandom.fill(ref bytes)`, `nextUlong()`, `nextInt(low, high)`,
     `nextLong(low, high)` (the same ways to a range) and, added before the slice,
     `hexString(int length)`: that many hex digits, each of four random bits (`32` is 128 bits), as .NET 8's
     `GetHexString`; from the slice, in lower case, an odd length allowed, a length below 0 a panic. The
     bytes are `arc4random_buf` on macOS and the BSDs and `getrandom` on Linux, which waits only until the
     system has gathered enough at boot; the interpreter calls the same functions. It never gives an error:
     if the system ever gave no bytes, the program stops with the panic "the system gave no random bytes",
     as Go's `crypto/rand` since 1.24 and Rust's `OsRng` do, rather than hand a `Result` that invites
     `?? 0`. It is a struct without fields that is never made: its functions are called on the type.
562. **The key of a hash table is drawn the same way** (`sg_entropy`, from the slice), in place of reading
     `/dev/urandom` on Linux; a system without random bytes now stops with the same panic instead of
     `abort()`.
563. **Shuffling and picking wait** (`xs.shuffle(ref r)`, `r.choose(xs)`): without generics of your own they
     cannot be written in SG, and a built-in method is a case in the checker, the interpreter and compiled
     code. Fisher-Yates is four lines, and `ok_random_seeded` has it. `nextFloat`, a normal distribution and
     `split` are left out (Not here yet).
564. **Measured** (`rng`, a new program): a hundred million numbers from 0 up to 1000 of a generator with a
     fixed seed, added up, take 0.112 s, 0.112 s in the overflow-checked build, against 0.112 s for the same
     algorithm written in C (1.00 times); both print the same sum, so the way to a range is the reference's
     too.
565. **Measured** (`secure`, a new program): a gigabyte of the system's bytes, 64 KB at a time, takes
     0.241 s, 0.242 s in the overflow-checked build, against 0.242 s for `arc4random_buf` in C on macOS
     (1.00 times).
566. **The tests**: `ok_random_seeded` prints the first numbers of three seeds, which are those of the
     reference C code of xoshiro256\*\* with SplitMix64, rolls a die sixty thousand times and tests the counts
     (chi-square), goes to the ends of the ranges (`nextInt(-5, -4)`, `long.MinValue` to `long.MaxValue`),
     fills an array and a range of a list, shuffles a list, lends the generator with `ref` and moves one into
     a task; `ok_random_secure` tests what is true of every run (the lengths and digits of `hexString`, two
     draws that differ, numbers inside their range). Their output is the same in the interpreter and in
     compiled code. Three `panic_random_` programs show the panics of 559 and 561, and eight `error_random_`
     programs the errors of 556, 557 and 560. `cargo test` compiles 272 of 1153 programs, 43 of them under the
     thread sanitizer, and 14 of 18 programs of tests; on Linux too, in a container, with the same counts.

Choices made in the seventy-fourth slice (folders, paths and the environment). 568 to 570 and 572 to 575
were decided before it on 15 November 2026, with the parts marked as coming from the slice; 567, 571 and
576 to 580 come from the slice, and all were approved the same day:

567. **Folders, paths and the environment are written in SG**, in three more files of the module `std`
     (`files.sg`, `path.sg`, `environment.sg`), over calls of the system that only that module makes
     (`System.createDirectory`, `removeDirectory`, `removeAll`, `list`, `status`, `current`, `makeTemp`,
     `variable`); elsewhere `System` is a name that is not there. `File.info(path)` and `File.exists(path)`
     are done by two functions of `FileInfo` that are not `public`, so a program that names `File` and one of
     the two gets the file of `FileInfo`. A file of the module that names a type of another file brings that
     file along: a `FileInfo` holds a `DateTime`, and `Directory.temp` reads `TMPDIR`. A program cannot define
     a type named `Directory`, `DirectoryEntry`, `FileInfo`, `FileKind`, `Path`, `Environment` or
     `VariableError`; two tests that had a `struct Path` of their own now call it `Route`. `Directory`,
     `Path` and `Environment` are structs without fields whose functions are called on the type; a value of
     one built field by field is an error whose hint says so.
568. **`File.delete`, `File.move`, `File.replace` and `File.copy`** are built into `File`, each a
     `Result<void, IoError>`. `delete` is `unlink`: a missing file is `NotFound`, and a folder `IsDirectory`
     on every system (macOS says `EPERM`; from the slice). `move` never takes the place of a file: the
     target is tested in the same step of the system as the rename (`renameat2` with `RENAME_NOREPLACE` on
     Linux, `renamex_np` with `RENAME_EXCL` on macOS), and a target that is there is `AlreadyExists`. From
     the slice: a file moved onto itself, or onto another name of the same file, is `AlreadyExists` too,
     tested first, since macOS called it done and Linux `EEXIST`. `replace` is `rename`, which takes the
     place of the target in one step, for writing a file whole next to it and putting it in its place.
     `copy` makes a new file (`AlreadyExists` when the target is there, `IsDirectory` for a folder), copies
     what the file holds and its permissions without the bits of setuid, setgid and sticky, and removes the
     target if it could not be written whole. A path with a zero byte in it is `NotFound`, as for the other
     functions of `File`. They have no `Async` form: `File.deleteAsync` is an error whose note says that the
     calls are made as they are, also in an async function.
569. **`IoError` has three more variants**: `AlreadyExists` (`EEXIST`), `NotDirectory` (`ENOTDIR`) and
     `DirectoryNotEmpty` (`ENOTEMPTY`), after the others, so the numbers of the variants before them stay.
     A file on the way of a path (`notes.txt/inside`) was `NotFound` and is now `NotDirectory`, in every
     function of `File`; `ok_file_text` shows the change. A `match` on an `IoError` that names each variant
     needs the three new arms.
570. **`Directory`**: `create` is `mkdir` (the folder it is in must be there, `NotFound` otherwise, and a
     folder or a file already there is `AlreadyExists`); `createAll` makes the folders on the way, written in
     SG over `create` as Rust's `create_dir_all` is: a folder already there is no error, a file in the way is
     (`AlreadyExists`, or `NotDirectory` for a path through it). `delete` is `rmdir`, of an empty folder
     (`DirectoryNotEmpty`; a file is `NotDirectory`). `deleteAll` removes the folder and all it holds
     without following a link: each folder inside is opened through the one it is in with `O_NOFOLLOW`, so a
     link that takes the place of a folder while it works is removed, not followed, which is the hole of
     CVE-2022-21658 in Rust. A path that is itself a link, or a file, is `NotDirectory`; what another program
     removed first is no error. The interpreter tests the path with `lstat` and then calls Rust's
     `remove_dir_all`, which works the same way since Rust 1.58.1; compiled code has its own (`sg_fs_clear`).
571. **`Directory.list(path)`** gives a `List<DirectoryEntry>`: `name` (the name alone, without `.` and
     `..`) and `kind`, read from the listing where the system gives it (`d_type`) and with `lstat` where it
     does not. The entries are sorted by the bytes of the names, so the order is the same on every system and
     in both ways of running. A name that is not UTF-8 is an `InvalidText` at its first byte that is not, the
     first such name in that order. From the slice: that case has no test, since macOS does not let a program
     make such a name (APFS answers `EILSEQ`); Linux does.
572. **`File.info(path)`** gives a `FileInfo` (`Copyable`, `Equatable`, `Formattable`) with three fields
     that are read and not written: `kind`, `size` (a `long` of bytes) and `modified` (a `DateTime`, the time
     of the last write). A link is followed, so `kind` is never `Link` there; `FileKind` is `File`,
     `Directory`, `Link` or `Other`, and a listing tells a link. `File.exists(path)` is `true` for a file
     and `Directory.exists(path)` for a folder, a link followed, and both are `false` when the path cannot be
     looked at, as C#'s are; their comments and the documents say that what they tell can be different the
     next moment, and that the way to use a file is to open it and look at the error.
573. **The folder the program works in**: `Directory.current()` gives a `Result<string, IoError>`, and there
     is no way to change it, since every thread would see it change. `Directory.temp()` is `TMPDIR` as it is
     written, or `/tmp` when it is not set or empty. `Directory.createTemp()` makes a new folder with a name
     nobody else has inside it, that only the user of the program can go into (`mkdtemp`, `sg-XXXXXX`), and
     gives its path; removing it is the program's work.
574. **`Path`** looks only at the `/` in a path and never asks the system. `Path.join(a, b)` gives a new
     `string` with one `/` between the two (`logs/` and `today.txt` give `logs/today.txt`); a second path that
     starts with `/` is put after the first, as C#'s `Path.Join` does and Rust's `join` does not; an empty
     part gives the other. (The ninety-fifth slice: a `..` in the second part still leads out of the
     folder, so a name from outside goes through `Path.joinInside`, 950.) `fileName`,
     `extension` and `parent` give a `read string` of the path, nothing copied (`from path`): `fileName` what
     comes after the last `/` (empty for `logs/`), `extension` the end of the name from its last `.` with the
     `.`, as C#'s, and none for a name with a `.` only at its start (`.profile`) or, from the slice, only at
     its end (`file.`), and `parent` what comes before the last `/`: `/` for `/report.txt` and for `/`, and
     the empty text for a name alone. Only `/` is a separator.
575. **`Environment.get(name)`** gives a `Result<string, VariableError>`: `Missing`, or `InvalidText(long
     position)` for a value that is not UTF-8, as Rust's `VarError`; `Environment.get("HOME") ?? "/"`
     reads it in one line. A name that is empty or has `=` or a zero byte in it is `Missing`. There is no
     way to set or remove a variable: setting one while another thread reads one is not safe in C, which is
     why Rust 2024 made `set_var` unsafe, and `Environment.set` is an error whose note says so. A program that
     starts another will give it an environment of its own. Reading all of them waits.
576. **The interpreter asks the system the same questions as compiled code.** It calls Rust's `std::fs`
     where that is one call of the system (`unlink`, `rename`, `mkdir`, `rmdir`, `stat`, `opendir` and
     `readdir`, `getcwd`, `getenv`), and the C library directly for `renamex_np` or `renameat2` and
     `mkdtemp`, so the answers are the system's in both; a copy goes through Rust's `io::copy`, which may use
     `copy_file_range`, where compiled code reads and writes 64 KB at a time: what the file holds is the same.
     A change from what was decided before the slice, which said the C library directly in both.
577. **Compiled code** has the calls in a runtime of their own (`FILES_RUNTIME`), only in a program that
     makes one; on Linux it asks glibc for the GNU names (`renameat2`, `d_type`, `mkdtemp` are not in strict
     C11), as a program with the network does. The module `std` and the functions over the calls are compiled
     like the program's own code. Found in the slice: the type `IoError` written in a module other than the
     root was not found by the back end; it is now seen from every module, as `TextError` is.
578. **Measured** (`listing`, a new program): a folder of ten thousand files listed a hundred times, each name
     with its kind, take 0.407 s, 0.436 s in the overflow-checked build, against 0.412 s for `readdir`,
     a copy of each name and `qsort` in C (0.99 times).
579. **Measured** (`status`, a new program): a million `File.info` of one file take 0.618 s, 0.627 s in the
     overflow-checked build, against 0.619 s for `stat` in C (1.00 times). From the slice: the first
     measurement was 1.03 times, the path copied into a block of `malloc` on each call to end it with a zero
     byte; a path shorter than 256 bytes is now ended on the stack.
580. **The tests**: `ok_files` moves, replaces, copies and deletes files and goes through each error, the
     permission a copy keeps among them (a script that runs, set and tested through C); `ok_directory` makes,
     lists, looks at and removes folders, with a link inside a folder that `deleteAll` removes without what it
     points at, and the folder the program works in and folders made by `createTemp`; `ok_path` takes paths
     apart and joins them; `ok_environment` reads variables, two of them set through C, one of which is not
     UTF-8. Their output is the same in the interpreter and in compiled code. Nine `error_` programs show the
     errors of 567, 568, 572, 574 and 575. `cargo test` compiles 276 of 1166 programs, 43 of them under the
     thread sanitizer, and 14 of 18 programs of tests; on Linux too, in a container, with the same counts.

Choices made in the seventy-fifth slice (three small leftovers of the slices before it, in the checker, the
interpreter and compiled code, and a warning of the C compiler found on the way, which led to holding all
compiled C to the C compiler's warnings), and all were approved the same day:

581. **`wait` takes the task a call gives, in one line**: `Task.delay(100).wait()` and
     `Task.run(f, x).wait()` are allowed, as `await Task.delay(100)` already was. The task has no other owner,
     so it is waited for where it is made, and nothing is left to wait for at the end of the function. The
     message that called such a task borrowed is gone: a borrowed task (`lent.wait()` through a `ref`) keeps
     it, and a task that is a part of a value with no name (none can be written today, since a task is kept
     only in a variable or a list) is told to take a name first. The rest of the rules stay: an async
     function does not `wait`, a value the task gives is used or dropped with `_ =`, and `Task.run` still
     takes no borrow.
582. **A panic inside the language's own module is shown at the call of the program** that led there, as
     `#[track_caller]` does in Rust for its standard library: `r.nextInt(5, 5)` stops with the line of the
     program that called `nextInt`, not the line of `panic` in `<built-in>/std/random.sg`, and
     `Duration.fromSeconds` of a count too large with the line that called it, not its multiplication. A
     call from one function of the module to another keeps the place of the program's call, so the place is
     always the program's. The message stays as it was (the overflow still says which numbers did not fit);
     the hint is left out, because it named code the caller cannot see (`tryMultiply` on a multiplication
     inside the module, 549). The functions of the program, and their modules, are shown where they panic,
     as before; there is no way to ask for this in code of your own.
583. **How the place reaches the panic**: the interpreter turns the place of a panic that comes out of the
     module into the place of the call that went in, at that call, and drops the hint there; nothing is done
     on a call that does not panic. In compiled code every function of the module takes the place of its call
     as one more parameter, a pointer to the constant text of that place; every place inside such a function
     is that parameter, and a call to another function of the module hands it on. The constant text of a
     place has one byte more, after its terminating zero, that tells a call of the module from any other
     place, and the three panics of the runtime that write a hint (overflow, a value that does not fit in a
     type, a character code) look at it. The methods the runtime calls on its own, with no place to give
     (`format` when a value is written, `equals`, `compare` and `hash` in a collection, `dispose` when a
     value ends), take no such parameter and show their own line, as before; none of the module's can panic
     today. The measured programs of the clock and of random numbers keep their times: `clock` 0.191 s
     against 0.190 s for C (0.191 before), `dates` 0.121 s against 0.411 s, `rng` 0.114 s against 0.114 s,
     `secure` 0.244 s against 0.242 s; the one more parameter costs nothing that shows.
584. **`for` over what a `try` opens is compiled**: `for name in try names() { ... }`, and over a map, a
     set, an array or a `T?` alike, with `try await` in an async function. The collection is computed once,
     before the first round, so the `try` can leave the function there as it does at the start of any other
     statement; what the function made before the loop is freed on that way out. The interpreter already ran
     it. `??` in front of the body (`for n in xs ?? return`) stays a syntax error, as before: the parser reads
     the `{` after a `for` as the body. The tests: `ok_for_try` (a list of strings, a map, a set, an array, a
     `T?`, a loop inside a loop, and the way out of each with a list made before it) and `ok_for_try_await`;
     `ok_task_wait_unnamed` and `error_wait_borrowed_task` for 581; the four `panic_random_` and
     `panic_time_` programs now show the line of the program, the same in the interpreter and in compiled
     code, and `tests_std_module` shows it under the test command, compiled too (the first program of tests
     that uses the module: a slip that left the place of the module's functions on in the code after them
     was found there, not by the programs that are run).
585. **`assert` on `<` between values of a struct with its own `compare` compiles without a warning**: the
     two sides are kept behind `const` pointers, so that they can be written when the test fails, and
     were handed to `compare` as they were, while `compare` takes `read this`, a pointer that is not
     `const` in C; the C compiler warned that the `const` was dropped. They are now cast, as the sort
     already did; `compare` only reads them. The program worked before as well; no test had compiled this
     path. `panic_assert_compare` does, with the four comparisons and one that fails.
586. **The C of every program compiles without a warning, and the tests hold it to that.** `cargo test`
     builds each compiled program, and each compiled program of tests, with `-Wall -Wextra -Werror`, with
     clang on macOS and gcc on Linux (`warnings` in `tests/golden.rs`). One warning is left on as a warning:
     a function nobody calls. The runtime is a library every program carries, the language's own module
     comes whole, SG lets a program have a function it does not call, and the C compiler leaves out what is
     not called. What turning them on found, and how each was fixed rather than silenced:
     - A failed call of the system returned `errno` as its error, and an `errno` left at 0 would have read as
       success, with the socket or the file it was to give unset (gcc: "may be used uninitialized"). Every
       such place now goes through `sg_os_error`, which gives `EIO` for a 0, as one place already did; the
       end of a folder, where `readdir` leaves 0 on purpose, keeps it.
     - The port of an address was written with `"%ld"` into six bytes; it is known to be at most 65535 by
       then, and is now written as the `uint16_t` it is.
     - A fixed-size array made at its zero values was `{ 0 }`, not the shape of the struct that holds it
       (`{ { 0 } }`, and `{ { { 0 } } }` for an array of `T?` kept as a flag and a value).
     - An async function's way back after a wait was a one-line `if` with a statement after it on the same
       line, which reads as if guarded; the `if` has braces now.
     - A parameter of a list's drop that a list of numbers does not need, the names of a function value
       when the program has none, the two sides of `==` on a struct with no fields, and the label after the
       arms of a `match`, written even when no arm jumped to it (now only when one does).
     - Three of gcc's analyses that follow values through the optimiser warn on code that is right: a
       constant text's bytes behind the test that frees only text with a capacity, the guard of a borrow of a
       `shared checked` value that is taken off its list before the block ends, and a read straight into the
       buffer that the length of a 3-byte array never reaches. Each is listed with the one program it is
       in, and is a warning only there (`GCC_SEES_WHAT_IS_NOT_THERE`); anywhere else it fails the test.
     The programs the tests do not compile keep their warnings, which are true: those that recurse forever
     or free raw memory twice on purpose (`panic_stack_overflow`, `panic_unsafe_`).
587. **What SG lets a program leave unused is used in C too**: a parameter, the variable of a `for`, a name
     of a `match` arm or of an `is`, and a variable given values that are never read (`var p =
     rect.corner; p.x = 0`, which `ok_compiled_struct` and `ok_list` do to show that a `Copyable` struct is
     copied) each get a `(void)` in the C, which makes no code. The last may be a mistake in a program, and
     whether SG should refuse it with a hint is written down as an open question in the decisions file,
     for a slice of its own. `cargo test` compiles 280 of 1171 programs, 45 of them under the thread
     sanitizer, and 15 of 19 programs of tests, all without a warning; on Linux too, in a container, with gcc and the same counts. Taking the fix of 585 back out
     makes the test fail on `panic_assert_compare`, as it should.

Choices made in the seventy-sixth slice (a variable nothing reads, a compile error; in the checker, with the C
of compiled code following it), and all were approved the same day:

588. **A local variable whose value nothing reads is a compile error**: one never used (`var total = 10`
     alone) and one only written (`best = x` in a loop, `count += 1`, `p.x = 0`). The error points at the
     name, "`total` is never used" or "`best` is given values that are never read", the second with the first
     write. A value written over before it is read, or given after the last read, is not looked at yet (in
     "Not here yet").
589. **What a read is**: writing the whole variable, `x op= v`, and a write to one of its fields or items are
     not reads. A write whose way goes through a borrow (`var p = ref rect.corner`, the variable of `for x in
     ref xs`, a `ref` name of a pattern), a `shared` or `weak` value, raw memory or a slice lands in a place
     that outlives the variable, and is one. Through a `T*` it is not: the variable owns what it points to.
     Every other use is a read: a method called on the variable, `ref x` given to a call, a call of a
     variable that holds a function, `try x.close()`, `t.wait()`. So a list only added to is not seen.
590. **What is not looked at**: a parameter (a `Func` it is given as, `dispose(ref this)` and `main(args)` may
     need one they do not use), and a `var` of a `Disposable` type, whose use is its end. The decision said a
     `Disposable` type; the slice found two more that are kept for their end alike (in `ok_disposable`), and
     takes them too: a struct with a `Disposable` in its fields, and a `shared` owner of one, whose end may be
     the last. A change from what was decided before the slice.
591. **The names of `for`, of a pattern and of a block**: `for i in 0..3` with `i` unused is the error, with
     the hint `for _ in 0..3`; a name of a `match` arm or of an `is`, with the pattern written again with `_`
     in its place (`Rect(w, _)`, `Ok(_)`). The names of an `as` and a `lock` block are held to it as well, with
     a note that the block does nothing with what it is lent; the decision named `for` and patterns only.
592. **A write to a copy**: when the variable was given a `Copyable` struct copied out of a field or an item
     (`var p = rect.corner`) and is only written in its parts, a note says it is a copy whose writes do not
     reach `rect.corner`, and the hint is the borrow, `var p = ref rect.corner`. Otherwise the hint is to read
     it where it is needed or to take it away with the lines that write it; a variable never used has
     "take the line away", or `_ = ...` to run what it is given.
593. **How it is built**: the checker notes, for each name it looks up, which variable it found, and every
     local variable it declares (`Seen`). A pass of its own, `unread.rs`, then goes over the checked tree of
     each function and test and tells reads from writes. It runs after every other check of the program, so
     this error, the least of them, never hides another (an `error_` program keeps its own error). A use the
     checker found another way counts for every variable of that name declared before it: the rule may miss
     an unread variable there, but never calls one unread that is read. No cost at run time.
594. **Compiled code follows**: the `(void)` that told the C compiler a variable, the variable of a `for`, or a
     name of an arm or of an `is` is used (587) is gone, since SG now reads each of them; only a `for _`
     keeps one. The C compiler's `-Wunused-variable` and `-Wunused-but-set-variable`, under `-Werror` in the
     tests, are a second check of the rule. A parameter keeps its `(void)`.
595. **What it found in the code already written**: 51 names of a `for` or of a pattern that nothing used,
     now `_`: 26 in the measured programs, 4 in the examples, 21 in the tests. By hand: `ok_list` and
     `ok_compiled_struct`, which write to a copy to show it is one, now print the copy too; `ok_weak` ended a
     value with `taken = null`, and now with `_ = taken`; `ok_option_exits` drops what `take` gives with
     `_ = list.take()`; three `panic_` programs and four tests of `tests_loop_sum`, whose writes end in the
     panic they test, read the variable after it. The language's own module had none.
596. **The tests**: eleven `error_unread_` programs (a variable never used, one written in a loop, `+=`, a
     write to a copy, to items, through a `T*`, the variable of a `for` over a collection and over a range, a
     name of a `match` arm and of an `is`, and of a `lock` block), `tests_error_unread` for a `test` block,
     and `ok_unread` with what passes: a write through `ref` and through `shared checked`, `ref x` given to a
     call, a list only added to, a `Disposable` and a struct that holds one, `_` in a loop and in a pattern,
     a value given after its last read, and an unused parameter. `cargo test` compiles 281 of 1183
     programs, 45 of them under the thread sanitizer, and 15 of 20 programs of tests, all without a warning
     of the C compiler now that the names are not marked used; on Linux too, in a container, with gcc and the
     same counts.

Choices made in the seventy-seventh slice (running another program and waiting for it; in the checker, the
interpreter and compiled code). 598 to 600, 602 to 609 and 611 to 613 were decided before it on 16 November
2026, with the parts marked as coming from the slice; 597, 601 and 610 come from the slice, and 614 to 616
from a gap the slice found (the value of a function of `std` that is not used), closed on the same day for
every call. All were approved the same day:

597. **Running another program is written in SG**, in one more file of the module `std` (`process.sg`), over
     two calls of the system that only that module makes: `System.run`, which starts a program and waits for
     it, and `System.executable`, which tells whether a file may be run. A program cannot define a type named
     `Command`, `ExitStatus` or `ProcessError`. The fields of a `Command` are its own; a program reaches it
     through its methods only.
598. **No shell reads the words.** `Command("git")` names the program, and `arg(word)` and `args(read words)`
     add arguments, each reaching the program as it is written: `$HOME`, `*`, `; ls` and a back quote are
     plain text (`ok_process_run`). A shell is asked for by name: `Command("sh")` with `-c`. As Rust and Go;
     C#'s `Arguments` is one text the program splits again.
599. **A command is built with methods, one per line**: `arg`, `args`, `directory(path)`, `setVariable(name,
     value)`, `removeVariable(name)`, `clearVariables()` and `input(text)`, each through `ref this`, giving
     nothing, so they are not chained as Rust's builder is. `Command(name)` is `Command.create(name)`.
600. **`run()` gives a `Result<void, ProcessError>`, `output()` a `Result<string, ProcessError>` and `status()`
     a `Result<ExitStatus, IoError>`.** `run` and `output` take an end with anything but 0 for an error,
     `Failed(status)`, as Go's `Run` and `Output`: a failure let go in silence is the best known mistake of
     shell scripts. `run` hands the program the input, the output and the standard error of this program.
     `output` keeps what it writes to its output, which must be UTF-8, gives it nothing to read, and lets its
     standard error go to this program's, where it is seen. `status` takes no end for an error, for a program
     whose code says something (`grep` ends with 1 when it found nothing).
601. **A change from what was decided before the slice: `ProcessError.Io(IoError cause)`, not `Start`.** The
     variant also carries what is not about the start: output that is not UTF-8 (`InvalidText`). And `try`
     turns an `IoError` into a `ProcessError` on its own through this variant, as it does for any enum with a
     variant that carries the error, so a function that gives a `ProcessError` can `try Directory.create(...)`
     (`ok_process_environment`). `ProcessError` prints as the `IoError` does, or as `the program exited with
     code 3`.
602. **`ExitStatus`** is `Code(byte code)` (`Code(int code)` since the ninety-fifth slice, for the codes
     of 32 bits of Windows, 956), the code the program ended itself with (a `byte`, as `exit`
     takes), or `Signal(int number)`, the signal that stopped it; `Copyable`, `Equatable`, `Hashable`,
     `Formattable`, with `success()` and the texts `exited with code 3` and `was stopped by signal 9`. Rust's
     `ExitStatus`, which gives `code()` and `signal()`.
603. **How the program is found**: a name without `/` is looked for in the folders of this program's `PATH`
     that start with `/`; an empty folder, `.` and any relative one are left out, so a file of the folder the
     program works in is never run by its name alone (Go's `ErrDot`, since 1.19). A path with `/` is taken as
     it is, from the folder this program works in, also when `directory` names another. Nothing found is
     `NotFound`; from the slice: a `PATH` that is not set or not UTF-8 is `NotFound` too, and a file that is
     there and may not be run is `PermissionDenied` only when no folder has one that may (as `execvp`). A
     folder is `PermissionDenied`, and a file without `#!` that is not a program is `Other(8)` (`ENOEXEC`):
     no shell is tried in its place, as `execvp` would. The search is written in SG, so the interpreter and
     compiled code find the same file; a test of its own runs the program as a process with `.`, an empty part
     and a relative folder in `PATH` (`process_path_lookup`), since the golden tests cannot change `PATH`.
604. **The environment of the program is a table of its own**: this program's, with the changes made to the
     command, the last change of a name counting; `clearVariables()` starts it empty and forgets the changes
     made before it. This program's environment never changes, as decided in the seventy-fourth slice. A name
     that is empty or holds `=` or a zero byte, and a zero byte in a value or an argument, is the new
     `IoError.InvalidInput` (Rust's name), after the others, so the numbers of the variants before it stay;
     from the slice: it is told when the program is run, since the methods that change the command give
     nothing. A zero byte in the name of the program or in the folder is `NotFound`, as in every path.
605. **What was printed goes out first**: the output and the standard error of this program are written out
     before the program starts, so what both print comes out in order. Bytes the program read ahead of its
     input (`Stdin`) are not seen by a program that takes the same input, as in Rust and C#.
606. **`input(text)` gives the program what it reads**: the text is written into a pipe while its output is
     read, so a program that reads all of it before it writes, or writes more than a pipe holds before it
     reads, does not stop either side (four megabytes through `cat` in `ok_process_output`). A program that
     ends before it read all of it is no error, as in Go: the write gives `EPIPE`, not the signal, which macOS
     leaves out for that pipe (`F_SETNOSIGPIPE`) and Linux holds back on the thread for the time of the
     writing and then takes away if it came.
607. **Only the input, the output and the standard error reach the program.** It is started with
     `posix_spawn` (not `fork`, which is not safe in a program with threads), with no signal held back and
     `SIGPIPE` as the system has it. Every file, socket and pipe the runtime opens is close-on-exec. Found on
     the way: the four `open` calls behind reading and writing a file whole (`File.readText`, `File.writeText`
     and the others) and behind `File.openText`, `File.createText` and `File.openAppend` were not; now they
     are. On macOS the sockets and the pipe of the loop of the network were
     made close-on-exec a step after they were opened, which another thread starting a program could come
     between; there the spawn now closes every descriptor it is not told to keep
     (`POSIX_SPAWN_CLOEXEC_DEFAULT`). From the slice: what this program was itself handed open without
     close-on-exec does not reach the program either (the first run of the tests found the pipe of the test
     runner in it): on macOS through the same flag, with glibc 2.34 and later through
     `posix_spawn_file_actions_addclosefrom_np`, and in the interpreter by marking every descriptor from 3
     up close-on-exec in the child before the program runs (one `close_range` on Linux); an older glibc
     still hands such a descriptor on. `ok_process_descriptors` keeps a writer, a reader, an appending writer and
     a listener open and has the program try each descriptor from 3 up. The folder is given with
     `posix_spawn_file_actions_addchdir`, the name POSIX took in 2024, where macOS 26 is the target, and with
     the older `_np` name elsewhere.
608. **These calls wait for the program, so an async function does not make them**: `try cmd.run()` there is
     the error of any call that holds the thread, with the hint `await Task.run(work, x)`. From the slice: the
     message says "`run` waits for another program to end" at the call the program wrote, rather than show a
     line inside the language's own module, for every function of the module. A `Command` goes to another
     task (it holds only text and lists). Starting a program and talking to it while it runs, stopping it,
     and the async versions are the next slice.
609. **The interpreter runs the program with Rust's `std::process::Command`**, the path the search in SG found
     and the name as it was given in front of the arguments. The output and the standard error of the run need
     not be those of the process (the tests keep them in memory), so the program writes into two pipes whose
     bytes the interpreter copies to them as they come; a program run by the interpreter sees a pipe where a
     compiled one may see a terminal. A program that reads this program's input reads the input of the
     process. The marking of 607 makes Rust start the program with `fork` and `exec` rather than
     `posix_spawn`; between the two only `fcntl` and `close_range` run.
610. **Compiled code** has the calls in a runtime of its own (`PROCESS_RUNTIME`), only in a program that runs
     another: the words and the table of the environment are made for the call (the texts of this program's
     environment are not copied), the input and the output go through pipes only when `input` or `output` asks
     for them, and one `poll` loop writes the one and reads the other straight into the `string` that
     `output` gives, checked to be UTF-8 where it is. A first version read into a list of bytes and copied it
     into the text; it took the same time and held the output twice. A program that reads or writes a file
     now asks glibc for POSIX 2008 (`_POSIX_C_SOURCE`), as one with tasks or the clock did: in strict C11,
     glibc keeps `O_CLOEXEC` out otherwise (found by the first run on Linux).
611. **Measured** (`spawn`, a new program): five thousand runs of `true`, each looked for in `PATH`, started
     and waited for, take 3.90 s, 4.06 s in the checked build, against 5.60 s for `posix_spawnp` and
     `waitpid` in C (0.70), on macOS with twelve folders before `/usr/bin` in `PATH`: `posix_spawnp` tries
     to run the program in each folder in turn, where the search in SG asks `stat` and `access`, which cost
     less than a failed start. What is left is the start itself, about 0.8 ms.
612. **Measured** (`capture`, a new program): `cat` of the log of two million lines (125 MB) kept with
     `output` five times takes 0.333 s, 0.338 s in the checked build, against 0.223 s for the same pipe read
     in C (1.49). All of the difference is the check that the output is UTF-8, which the C does not make:
     without it the same program takes 0.23 s. A new entry in "Slower than C", with the other checks of
     text.
613. **The tests**: `ok_process_run` (the words as written, `run` and `status` on an end with a code and with
     a signal, a program that is not there, one that may not be run, a script made runnable, a folder),
     `ok_process_output` (`input`, four megabytes through `cat`, a program that reads none of a large input and
     one that stops early, a failed end whose standard error is ours, output that is not UTF-8),
     `ok_process_environment` (a name set twice and removed, the table emptied, ours unchanged, each
     `InvalidInput`, `directory` and a folder that is not there), `ok_process_descriptors`, three `error_`
     programs (a `run` in an async function, its `Result` not used, a field of a `Command`), and
     `process_path_lookup`. `cargo test` compiles 285 of 1190 programs, 45 of them under the thread
     sanitizer, and 15 of 20 programs of tests, without a warning of the C compiler; on Linux too, in a
     container with glibc 2.41 and gcc, with the same counts.
614. **Every call that gives a value is a compile error when the value is not used**, with `_ =` to drop
     one on purpose: a function or a method of your own, one of the language's own module, a built-in method
     and a function of C alike, and the value after `try` and `await` as well as the one before.
     `Path.join(a, b)`, `try cmd.status()`, `random.nextInt(1, 6)`, `xs.removeLast()`, `seen.add(x)`,
     `noisy(4)` and `unsafe memset(...)` alone are errors; `_ = try cmd.status()` runs the program and drops how
     it ended. It takes the place of 283 and 284: the slice found a status of a program that nobody looked at
     passing silently, because a function of `std` was held to the rule only as one of your own was, and
     the line that decides which calls are free (only gives a value, changes and gives, may print) is one
     the reader would have to know call by call. Now there is no list: what a call gives is read, kept or
     dropped where it can be seen. Zig refuses every value thrown away the same way; Rust warns only where a
     function is marked `#[must_use]`, Swift where it is not marked `@discardableResult`.
615. **The words of the error**: "the `ExitStatus` that `status` gives is not used", with the hint "keep it:
     `var value = try cmd.status()`; to drop it on purpose, `_ = try cmd.status()`", as an `await` whose value
     is not used says it. A built-in method that only gives a value keeps the words of 285 ("it leaves
     `distance` as it was"), and a `Result` not used keeps its own.
616. **No cost, and compiled code as it was**: the rule is in the checker alone. Eighteen programs of the tests
     and one example dropped a value: `_ =` went in front of the `add` of a `HashSet` (five), `removeLast`
     (two), `try reader.readLine(ref line)` and its async form (six), `try` of a function of your own (two,
     and three in the calculator), `receive` and two functions of C. The benchmarks and `std` dropped none.
     `ok_unused_changers` now drops each value with `_ =`. The tests: `error_std_value_unused` (`Path.join`),
     `error_std_value_unused_try` (`try cmd.status()`), `error_std_value_unused_changing` (`nextInt`),
     `error_unused_own_function`, `error_unused_own_method`, `error_unused_changer` (`removeLast`),
     `error_unused_set_add`, `error_unused_extern` and `ok_std_value_dropped`. `cargo test` compiles
     286 of 1199 programs, 45 of them under the thread sanitizer, and 15 of 20 programs of tests.


Choices made in the seventy-eighth slice (a program started and talked to while it runs, stopped, and waited
for the async way; in the checker, the interpreter and compiled code). 617 to 625, 628 and 630 were decided before
it on 16 November 2026, with the parts marked as coming from the slice; 626 and 627 were chosen by the user
during it, 628 was confirmed by the user during it, and 629 comes from the slice. All were approved the same day:

617. **`cmd.start()` gives a `Result<Process, IoError>`** and goes on while the program runs, as Rust's
     `spawn`: a program can only fail to start, so the error is an `IoError` (`NotFound`, `PermissionDenied`,
     `InvalidInput`), as that of `status`. What was printed goes out first (605).
618. **`pipeInput()`, `pipeOutput()` and `pipeError()` ask for pipes**, and only `start` looks at them; a
     stream not asked for is this program's, so a pipe nobody reads never makes a program wait. A text of
     `input` is written into the input of a started program by a thread while it runs (Go's way), and
     `input` and `pipeInput` take each other's place, the last one written counting. `run`, `output` and
     `status` keep what they did.
619. **The streams are taken out of the `Process` once**: `takeInput()` gives a `StreamWriter?`, `takeOutput()`
     and `takeError()` a `StreamReader?`, each `null` once taken or when it was not asked for (Rust's
     `child.stdout.take()`); `process.takeOutput() ?? panic("...")` is the usual way to write it. What is
     taken out is owned, every method of a reader and a writer works on it, and it goes to another task.
620. **The input is a `StreamWriter`**, `Closable` as any writer: closing it is how the program learns that its
     input has ended. Writing to a program that no longer reads gives `IoError.ConnectionReset`, as for a
     connection (469), never the signal: macOS leaves the signal out for the pipe (`F_SETNOSIGPIPE`), Linux
     has the writing thread hold it back and take it away (`sg_write_pipe`). No variant of `IoError` is new.
621. **`wait()` takes the `Process`** and gives a `Result<ExitStatus, IoError>`. It first closes the input the
     `Process` still holds (as Rust's `wait`) and lets go of the readers it still holds, so that a program
     that reads to the end of its input, or writes more than a pipe holds, does not wait for this one
     forever. A reader taken out before reads what is left after the wait (`ok_process_start`).
622. **A `Process` is waited for**: one in a local variable that is not waited for on every path is a compile
     error, as a task is ("the program in `p` is not waited for before its block ends", with the hint `var
     status = try p.wait()`; giving the variable another program too). One that ends without being waited
     for (a way out with an error, a field or an item that ends) stops its program with the signal 9 and
     waits for it there, so no program is left running or as a process that ended and was never asked
     how (Rust leaves both; tokio's `kill_on_drop(true)` and Go's `CommandContext` stop it). From the slice:
     a name of `is Ok(process)` is lent, so the rule does not see it, and the program stops where the
     value it is lent from ends.
623. **`kill()` sends the signal 9 and `terminate()` the signal 15**, which a program may catch; both give
     nothing. A program is taken away only by a wait, so its number names no other program until then,
     and a signal to a program that has ended does nothing (`ok_process_kill`). `id()` gives the number,
     an `int`. A killed program is still waited for, and ends with `ExitStatus.Signal(9)`.
624. **The async versions**: `runAsync()`, `outputAsync()` and `statusAsync()` of a `Command` and
     `waitAsync()` of a `Process`, each with a token that may come last (404). A `Command` is lent, so
     they are awaited where they are written (409). In an async function `cmd.run()` and the others are
     still errors, and the hint now names the async version (`await list.runAsync()`).
625. **A cancel stops the program**, with the signal 9, and the call gives `IoError.Cancelled(...)` (or
     `ProcessError.Io(Cancelled(...))`): `waitAsync` takes the `Process`, so after a cancel nothing would
     be left to stop it with, as with Go's `CommandContext`; C# only stops waiting. From the slice: with a
     token cancelled before the call, the program does not start.
626. **A method can be async**, chosen by the user during the slice, which needed it to write the async
     versions in SG: `public async int total(read this)`, on a struct or an enum, and an async function of a
     type (`await Store.load(4)`). One that lends `this` is awaited where it is written, as an async
     function given a borrow is (388); one that takes `this` by value may start as a task of its own, under
     the thread rule. `create`, `format`, `equals`, `hash` and `compare` are called by the language without
     `await`, so they are not async (`error_async_method_format`, `error_async_method_create`); a method that
     awaits itself around is the error it is for functions. In compiled code an async method is an async
     function with `this` in front of its parameters (`a_init_`, `a_start_` and the frame `F_` of
     `Owner.name`).
627. **A function or a method of the language's own module may leave out a last `read CancellationToken`**,
     chosen by the user with 626: `CancellationToken.None` is put there, so `await cmd.runAsync()` and
     `await cmd.runAsync(read token)` are one method, as the built-in async functions are (404). A function
     of the program still takes what it declares.
628. **The end of a program is waited for on the loop of the network**, as decided before the slice and
     confirmed by the user during it, after a first version that waited on a thread of input and output
     held for each program (Go's way, which a server waiting for many long programs would have filled the
     512 threads of input and output with). Linux puts the pidfd of the program on epoll (`pidfd_open`,
     5.3 and later), macOS puts `EVFILT_PROC` with `NOTE_EXIT` on kqueue (as tokio does on Linux); neither
     takes the program away, so it is taken away where the task goes on, and its number names no other
     program before. A program that has ended already is ready at once (macOS gives `ESRCH` for it). A cancel
     takes the wait off the loop. Where Linux has no pidfd, a thread of input and output waits with
     `waitid(..., WEXITED | WNOWAIT)` instead. The readers of the pipes are read on the loop as the reader
     of a connection is (480 to 487, without a longest line), so a cancel ends a wait for a line and loses
     nothing; the receive of the loop is now `read`, which reads a socket and a pipe alike.
629. **In the interpreter** a program is started with Rust's `std::process::Command`, kept in a table of the
     run until it is waited for. An output it shares with this program is a pipe read by a thread and
     written out to the run's output when the program is waited for, since the run's output need not be
     the stream of the process (609): what a started program writes there comes out at its wait, in
     compiled code as it writes. `waitAsync` has a thread wait with `waitid` and tell the scheduler through
     its pair of sockets, as a connect does (473). The reads of a pipe are done when they are called (412):
     an async read waits there, and a cancel does not end it.
630. **A `Process` goes to another task**: it owns the number of the program and its pipes.
631. **Measured** (`pipelines`, a new program): the log of `lines` (two million lines, 125 MB) read line by
     line from the output of `cat`, started with `start`: 0.078 s (0.081 s checked) against 0.092 s
     for C, `popen` of `cat` and `getline`, which does not check that a line is text: 0.85 times. The reader
     takes 64 KB of the pipe at a time; `getline` goes through the 4 KB buffer of a `FILE`.
632. **Measured** (`pipesasync`, a new program): the same, read with `readLineAsync` and waited for with
     `waitAsync`: 0.093 s against the same 0.090 s of C, which
     has no async: 1.03 times. Almost every line is in the block already, so a read is done in place;
     when it is empty, the task waits for the pipe on the loop.
633. **Measured** (`spawnasync`, a new program): five thousand runs of `true` with `statusAsync`:
     3.778 s against 5.312 s for C's
     `posix_spawnp` and `waitpid`: 0.71 times, as `spawn` (0.72 on the same run). The end of each program is
     waited for on the loop (pidfd, `EVFILT_PROC`), so no thread is held for it.
634. **The tests**: `ok_process_start` (the pipes, each taken once, a stream not asked for, the input closed
     by `wait`, a text of `input` and the output read whole, the standard error through a pipe, a reader read
     after the wait, a program that does not start), `ok_process_kill` (9, 15, a program that had ended, a
     shell that catches 15), `ok_process_talk` (a task writes the input of `cat` while `main` reads its
     output, the async way), `ok_process_async` (every async version, an end with a code, output that is not
     UTF-8, a program not found), `ok_process_cancel` (a cancel of each async wait, a token cancelled before
     the call), `ok_process_drop` (a program stopped on a way out with an error and in a field) and
     `ok_async_method`; the checker refuses `error_process_not_waited`, `error_process_not_waited_block`,
     `error_process_wait_moved`, `error_process_wait_async`, `error_process_output_async`,
     `error_process_given_another` and `error_async_method_borrowed`, `_format`, `_create` and `_itself`
     (`error_async_method`, which said an async method was not here yet, is gone). `cargo test` compiles
     293 of 1215 programs, 57 of them under the thread sanitizer, and 15 of 20 programs of tests; on Linux
     too, in a container, with the same counts, where the sanitizer found a field of the wait that was read
     before it was written (the direction of a wait on epoll), now set.

Choices made in the seventy-ninth slice (the Unicode tables: what a character is, the case of a character and of a
text, and comparing without case; in the checker, the interpreter and compiled code). 635 to 644 were decided
before it on 16 November 2026, with the parts marked as coming from the slice; 645 to 647 come from the slice.
All were approved the same day:

635. **What is here**: what a character is (a letter, a number, an upper or a lower case letter, a control
     character), the case of a character and of a text, and comparing and folding without case. Not here, as
     in Rust's standard library: normalization (`"é"` written as one character and as `e` with a combining
     accent are not equal), grapheme clusters (`👍🏽` is two `char`s), sorting by a language, the width of a
     character on the screen, words, scripts, and names in Unicode in the source.
636. **One set of tables, Unicode 17.0, for both sides**: `src/unicode_tables.rs` is made by
     `tools/unicode_tables.py` from four files of the Unicode Character Database (`UnicodeData.txt`,
     `SpecialCasing.txt`, `CaseFolding.txt`, `DerivedCoreProperties.txt`), each checked against its SHA-256;
     the files are not kept in the repository, the tables made from them are (Rust's way). The interpreter
     does not use Rust's own `char` methods, whose version of Unicode is Rust's: it reads the same tables as
     compiled code, so the two give the same answers. Each code point has a record of what it is and what
     its simple upper, lower and folded case add to its code (201 different records); it is found in three
     steps (by the code shifted right by 9, the next 5 bits, the last 4), 20.7 KB in all. A mapping to more
     than one character is in a short sorted table (102 upper case, 1 lower case, 104 folded). From the
     slice: the generator checks every code point against Python's tables when Python knows the same
     version (3.15 knows 17.0): the category, and `upper()`, `lower()` and `casefold()` of the character.
637. **What a character is, on `char` only**: `isLetter()` is the general category L (Lu, Ll, Lt, Lm, Lo), as
     C#, Go and Java have it (Rust's `is_alphabetic` is the wider Alphabetic: `'Ⅻ'` and the vowel signs of
     Indian scripts); `isNumeric()` is N (Nd, Nl, No: `'٣'`, `'½'`, `'²'`), Rust's `is_numeric` and C#'s
     `IsNumber`; `isAlphanumeric()` is either; `isUpper()` and `isLower()` are Lu and Ll, so an upper case
     letter is a letter (Rust's `is_uppercase` takes the derived property, with `'Ⓐ'`); `isControl()` is Cc
     (65 characters). `isDigit` stays out (172): `'٣'` would be a digit that `parse` does not read. A `byte`
     keeps the ASCII methods.
638. **`c.toUpper()` and `c.toLower()` give a `char`**: the simple mapping, one character for one, as C#'s
     `ToUpperInvariant` and Go's `unicode.ToUpper`: `'ß'.toUpper()` is `'ß'`, `'ǅ'.toUpper()` is `'Ǆ'`.
     Rust's `to_uppercase` gives an iterator (`'ß'` gives `S`, `S`), whose type cannot be written here; the
     text methods give the full mapping.
639. **`text.toUpper()` and `text.toLower()` give a new `string`** by the full mappings, Rust's
     `to_uppercase` and `to_lowercase`: `"straße".toUpper()` is `"STRASSE"`, `"ﬃ"` gives `"FFI"`, `"İ"`
     lower case is `i` with a combining dot above (U+0307). `Σ` lower case is `ς` at the end of a word
     (Final_Sigma: after a cased letter, case-ignorable characters such as `'` skipped, and not before one),
     `σ` elsewhere: `"ΟΔΟΣ"` gives `"οδος"`. The length in bytes may change (`"ﬁ"` is 3 bytes, `"FI"` 2).
640. **No language is taken into account**: the answer is the same on every machine, as in Rust and Go and in
     C#'s invariant culture: `"istanbul".toUpper()` is `"ISTANBUL"`, not `"İSTANBUL"`, and `"I".toLower()` is
     `"i"`. C#'s `ToUpper` follows the culture of the machine, the source of the well-known "Turkish i"
     mistakes. The Turkish and Azeri mappings (Go's `unicode.TurkishCase`) are left for later.
641. **`a.equalsIgnoreCase(b)` folds both by the full folding** of CaseFolding.txt (C and F, not the Turkic T):
     `"Straße".equalsIgnoreCase("STRASSE")` and `"ΣΑΣ".equalsIgnoreCase("σας")` are true, `"İ"` and `"i"` are not
     the same. It allocates nothing: both texts are folded one character after the other and compared as they
     go. C#'s `OrdinalIgnoreCase` and Go's `EqualFold` fold one character to one (`ß` is not `ss`); Rust's
     standard library has only the ASCII one. `equalsIgnoreAsciiCase` stays, for the names of protocols (178).
642. **`text.foldCase()` gives the folded text**, Python's `casefold`: the key of a `HashMap` that is not to
     care about case. `toLower` would not do it: `"STRASSE"` and `"Straße"` lower case differ, folded they are
     both `"strasse"`.
643. **In compiled code** the tables are only in a program that calls one of these methods; one that does grows
     by about 16 KB (macOS). An ASCII character is changed without the tables, and a text of ASCII is copied
     byte by byte. From the slice: a new text takes room for the length of the old one first, and room for
     three characters before each character past ASCII, which maps to at most three.
644. **The hints**: `toUpper` and the Unicode tests on a `byte` are errors with the ASCII method as a hint
     (`toAsciiUpper()`; for `isControl`, `b < 32 || b == 127`) and the note that they are methods of `char`.
     `isDigit` gets `isAsciiDigit()` and the note on `isNumeric`; `isAlphabetic` gets `isLetter()`,
     `toUpperCase` and `to_uppercase` `toUpper()`, `casefold` `foldCase()`. The notes that said the Unicode
     tables were not here are gone.
645. **Measured** (`case`, a new program): a text of 1.3 MB in many scripts, fifty times in upper case and back
     in lower case: 0.220 s against 0.962 s for C's `mbrtowc`, `towupper` and `wcrtomb` in a UTF-8 locale: 0.23
     times on macOS (0.221 s checked); on Linux in a container, glibc 2.41, 0.162 s against 1.855 s, 0.09
     times. The C functions map one character to one and know nothing of the end of
     a word, so they do less; SG reaches a record in three steps and changes an ASCII byte without the tables.
646. **Measured** (`letters`, a new program): the characters of the same kind of text tested with `isLetter`,
     `isUpper` and `isNumeric`, two hundred times: 0.290 s against 2.969 s for `mbrtowc` with `iswalpha`, `iswupper` and
     `iswdigit`: 0.10 times on macOS (0.387 s checked, where the three counters are checked for overflow);
     on Linux 0.263 s against 2.493 s, 0.11 times.
     `iswdigit` knows the ASCII digits only, so C counts no `'٣'`.
647. **The tests**: `ok_unicode_char` (each test and the simple mappings on letters of several scripts, `'ß'`,
     `'ǅ'`, `'Σ'`, `'Ⅻ'`, `'ª'`, Cherokee, control and format characters), `ok_unicode_text` (the full
     mappings, Final_Sigma with and without an apostrophe, Turkish letters, folding, `equalsIgnoreCase` both
     ways, a `HashMap` keyed by the folded text, a trimmed look at a text), `ok_unicode_sample` (every 17th code
     point, which reaches each block of 16 of the tables: counts and sums over every method), and
     `tests/release/unicode_every` (every code point, compiled), whose counts and sums of the full mappings are
     those Python gives; a test of `src/unicode.rs` goes over every code point in the interpreter's code. The
     checker refuses `error_unicode_on_byte`, `error_unicode_control_on_byte`, `error_unicode_is_digit` and
     `error_unicode_upper_case_name`; `error_text_to_upper`, which said the case of a text was not here, is
     gone. `cargo test` passes on macOS and on Linux in a container, where it compiles 297 of 1222 programs, 57
     of them under the thread sanitizer, and 15 of 20 programs of tests.
Choices made in the eightieth slice (datagrams: `UdpSocket`, `SocketAddress`, and the addresses of a
`TcpStream`; in the checker, the interpreter and compiled code). 648 to 660 were decided before it on 16
November 2026, with the parts marked as coming from the slice; 661 to 666 come from the slice.
All were approved the same day:

648. **What is here**: `UdpSocket` (`bind`, `sendTo`, `receiveFrom`, `connect`, `send`, `receive`, `port`,
     `localAddress`, `remoteAddress`, and the async versions with a token that may come last),
     `SocketAddress` (`parse`, `lookup`, `lookupAsync`, `host`, `port`) and `Received`, and the two addresses
     of a `TcpStream`. Not here, as in Rust's `UdpSocket` without a crate: multicast, the options of a socket
     (the time to live, the size of its buffers, `SO_REUSEADDR`), a look at a datagram that leaves it in the
     socket (`peek`), and sending to every host of a network (broadcast), which comes later. From the slice:
     the zone of a v6 address (`fe80::1%en0`) is not kept, so a link-local address cannot be answered.
649. **Written in SG, in the language's own module** (`src/std/net.sg`), decided before the slice: an address
     is read and written in SG, so the interpreter and compiled code read the same texts and print the same
     way; only the calls of the system under the three types are the compiler's own (`System.udpBind`,
     `System.udpSend`, `System.udpReceive`, `System.lookup` and the others), and only this module can call
     them. From the slice: a `SocketAddress` is two `ulong` (the 128 bits of the host; a v4 host is the last
     32), whether it is v6, and the port: an array is not `Hashable`, two numbers are, so `Copyable`,
     `Equatable` and `Hashable` come field by field. A v4 address inside a v6 one (`[::ffff:1.2.3.4]:53`) is
     not equal to the v4 address, as in Rust.
650. **`SocketAddress.parse(text)` reads a number, never a name**, decided before the slice: `1.2.3.4:53` and
     `[::1]:53`, anything else `InvalidInput`. From the slice: a part of a v4 address is a number up to 255
     without a 0 in front (`01.2.3.4` is not read, as in Rust), a v6 address has groups of
     up to four hex digits, one `::`, and may end in a v4 address (`[::ffff:10.0.0.1]:1`). It is printed as RFC
     5952 asks: small hex digits, the longest run of two groups of 0 or more as `::` (the first of the
     longest), never one group alone, and `::ffff:` followed by the v4 address. `host()` gives the host
     alone, without the brackets, and `port()` the port.
651. **`SocketAddress.lookup(text)` gives a `List<SocketAddress>`**, decided before the slice: the addresses
     of `host:port` in the order the system gives them, one for each (as Rust's `to_socket_addrs`); a host
     written as a number is the one address, found without asking. A name that has no address, and a text
     that is not `host:port`, are `HostNotFound`, as for `TcpStream.connect`. `lookupAsync` looks the name up
     on a thread of its own, which a cancel leaves to finish, as `connectAsync` does.
652. **`receiveFrom` gives a `Received`**, decided before the slice (the first of two ways offered; the other
     was a `ref SocketAddress` to fill, C#'s way): a struct of two read-only fields, `count` and `from`, so
     `got.from` answers where the datagram came from. It is `Copyable`, `Equatable` and `Formattable`.
653. **`UdpSocket.bind(address)` takes what `TcpListener.bind` takes**, decided before the slice: a number,
     `localhost` (the v4 loopback), or no host for every v4 address; port 0 lets the system choose, and
     `port()` tells which. From the slice: a name, or a text that is not `host:port`, is `HostNotFound` here
     as it is for a listener, though `SocketAddress.parse` says `InvalidInput` for the same text: `bind` keeps
     the word of `TcpListener.bind`, `parse` the word of a text it was given to read. `SO_REUSEADDR` is not
     set (Rust does not set it for a datagram socket), so a port in use is `AddressInUse`.
654. **`connect(address)` gives the socket a default address**, decided before the slice: `send` and
     `receive` need none, and the system lets go of a datagram from anyone else. When nobody listens there,
     a `send` or a `receive` after it may say `ConnectionRefused`: the system heard so from the other
     machine. `remoteAddress()` gives that address, or `null` before a `connect`.
655. **A datagram that does not fit is an error, `MessageTooLong`**, decided before the slice: a new variant of
     `IoError` (the seventeenth, last), both for a send the system cannot make as one message and for a
     receive into a buffer shorter than the datagram, which is then gone. Rust and Go cut such a datagram
     short without a word. A `match` that names every variant of `IoError` needs one more arm.
656. **Every method goes through a `read` borrow**, decided before the slice: a `shared` socket is used by
     several tasks at once, which UDP allows, since each message is whole. From the slice: an async call
     waits for the socket on the loop through a copy of its own (`dup`), as a listener is waited for, since
     several tasks may wait for one socket (since the eighty-second slice, in the slot of the socket, with
     no copy, 681, and the socket stops blocking on its first async call, 689).
657. **No `close`**, decided before the slice: a datagram is handed to the system whole, so nothing is left to
     lose; the socket is closed where it ends (it is `Disposable` inside the module), and `close()` is an
     error whose note says so, as for a `TcpStream`.
658. **The async versions**, decided before the slice: `sendToAsync`, `receiveFromAsync`, `sendAsync`,
     `receiveAsync` and `lookupAsync`, each awaited where it is written (they lend the socket), with a token
     that may come last. A token cancelled before the call gives `Cancelled` and nothing is done; a cancel
     while a receive waits ends the wait at once, and a datagram that comes later is taken by the next
     receive. A wait has a time limit through `CancellationSource.after(ms)`. A call without `Async` in an
     async function is an error with the async version in the hint, followed through the calls.
659. **The interpreter and compiled code**, decided before the slice: Rust's `UdpSocket` in the interpreter,
     the calls of the system in compiled code, through the loop of the network for async calls. From the
     slice, in the interpreter: a socket is the number of a socket of the system, which does not block, as a
     `TcpStream`; a datagram is received into a block one byte longer than the buffer, so one that fills it
     did not fit. In compiled code: a socket blocks, so a call without async is one call of the system, and
     an async call asks the system not to wait (`MSG_DONTWAIT`) and waits on the loop instead, so the socket
     has no state to keep and the two kinds of call share it; a receive is `recvmsg`, whose `MSG_TRUNC` says
     the datagram did not fit. `UDP_RUNTIME` and `UDP_ASYNC_RUNTIME` are added only to a program that uses
     them; `sg_net_op` has two more kinds.
660. **The two addresses of a `TcpStream`**, decided before the slice: `stream.localAddress()` gives a
     `SocketAddress`, `stream.remoteAddress()` a `Result<SocketAddress, IoError>`, since the system no
     longer knows the other side once it has broken the connection off (`ConnectionReset`). From the slice:
     the checker hands both to the module (`SocketAddress.remoteOf(read stream)`), which a program that names
     either method gets; `ENOTCONN` is `ConnectionReset` for every call.
661. **A blocking call in an async function names what it waits for**: the note says `receiveFrom` "waits for
     the network" and `lookup` "waits for a name to be looked up", and the hint writes the async call with
     its arguments. The hint of a `TcpStream` got its arguments too: `await conn.receiveAsync(ref buffer)`
     where it said `(...)` (`error_net_blocking_in_async`).
662. **A user type may have the name of a type of the module**: `struct Received` in a program is its own,
     as `Command` is; the module's file is read only when the program names one of its types or one of the
     two methods of a `TcpStream`.
663. **Found on the way**: a struct literal of `Command` or `Process` said how to make a `DateTime`; it now
     says `Command("ls")` and `cmd.start()`.
664. **Measured** (`udpecho`, a new program): an echo server and eight clients in one async program over the
     loopback, each client sending 64 bytes and waiting for them to come back 20,000 times, the server one
     task with one socket: 1.261 s against 1.062 s for C, whose sockets block, with a thread for the server
     and for each client: 1.19 times on macOS (1.267 s checked). A wait wakes two threads where C wakes one,
     as for `echo` (478), and the socket is waited for through a copy of its own (656), made and closed for
     each wait. On Linux in a container, 1.668 s against 0.691 s: 2.41 times. Waiting through the socket
     itself instead of a copy took that to 1.67 s, so the copy is not what costs: the two wakes are. In
     "Slower than C".
665. **Measured** (`udpechosync`, a new program): the same without async, the server and each client on a
     thread of `Task.run`: 1.058 s against the same C, 1.055 s: 1.00 times on macOS (1.070 s checked), each
     call one call of the system (659); on Linux 0.700 s against 0.707 s, 0.99 times.
666. **The tests**: `ok_udp_address` (texts read and printed, v4 inside v6, `::` rules, sixteen texts that are
     not addresses, equality and a `HashSet`, a lookup of a number), `ok_udp_sync` (a datagram whole, one too
     long for the buffer and one too long to send, an empty one, `connect`, `send` and `receive`, the v6
     loopback, `bind` given a name and a port in use), `ok_udp_async` (a `shared` socket answered by two
     tasks at once, a cancel that ends a receive, a token cancelled before each async call, `lookupAsync`,
     and `ConnectionRefused` on a connected socket), `ok_net_address` (the two ends of a TCP connection,
     through a borrow too), and six programs the checker refuses (`error_udp_*`).
     `error_io_error_cancelled_arm` names one more variant. `cargo test` passes on macOS and on Linux in a
     container, where it compiles 301 of 1232 programs, 61 of them under the thread sanitizer, and 15 of 20
     programs of tests.

Choices made in the eighty-first slice (the pool of async tasks waits on the loop of the network itself;
compiled code alone). 667 to 675 were decided before it on 16 November 2026, after a measurement, with the
numbers from the slice; 676 and 677 come from the slice. All were approved the same day:

667. **What changes**: the pool of async tasks and the loop of the network in compiled code, nothing else.
     The interpreter runs every task on one thread and waits with `poll`, as before, and nothing a program
     writes or sees is different. It is the first of three slices toward the scheduler of Tokio, Rust's
     async runtime, written in the C of the runtime with no library: this one, then a socket put on the loop
     once instead of for each wait, then a queue for each thread of the pool from which the others steal.
668. **No thread of its own for the loop**: a thread of the pool that finds nothing to do waits on the loop
     itself, one at a time (Tokio calls it the driver), and the tasks it wakes go on the queue without waking
     anyone; it takes them when its round is over. The thread the system wakes for a socket is the one that
     runs the task, where the loop's own thread used to wake a thread of the pool for it: two wakes for each
     wait became one. When the loop is opened and the pool has no thread yet, one is started.
669. **One thread at a time looks for work** (Tokio's rule): while tasks are queued, at most one idle thread
     is woken, or one started, to take them, and the one that takes a task wakes the next if more are left.
     Every task used to wake an idle thread: on eight threads they fought over the lock of the queue, and an
     echo over eight connections took 2.30 s on Linux, 0.93 s with the pool cut to one thread.
670. **A task woken from outside the pool while every thread is busy**: a thread of `Task.run`, the thread of
     the clock or a thread of input and output that wakes an async task, when the pool can have no more
     threads and none is idle, writes a byte into the pipe of the loop, the one a cancel already writes, so
     the thread waiting there wakes and takes the task.
671. **What it costs**: when every thread of the pool is busy with long tasks, a socket that is ready is seen
     only when one of them is done, as in Tokio. It was so before too in effect: the loop's thread saw it,
     but no thread was there to run the task.
672. **The other threads stay**: the clock's thread for `Task.delay` and the threads of input and output for
     files, the input and lookups. The end of a program waited for the async way (628) is waited for on the
     same loop, now by the pool.
673. **Measured** (`echowork`, a new program): `echo` with work in it, the server going through 300,000 steps
     of arithmetic for each message before it sends it back, and each of the eight clients sending 300:
     0.376 s against 0.369 s for C with a thread for each connection, 1.02 times on macOS (0.372 s checked;
     0.380 s before the slice); on Linux in a container 0.332 s against 0.331 s, 1.00 times (1.06 before).
     The tasks the loop wakes run on several threads at once. A first try in which the thread that waited on
     the loop ran every task it woke took 5 times as long here, so it was not taken. The other async
     programs take the time they took before, on both systems: `tasks`, `async`, `asynclines`, `tasklist`,
     `asynccopy`, `echosync`, `netlines`, `pipesasync`, `spawnasync` and `udpechosync`, within the noise.
674. **Measured** (`echo`, and Tokio beside it): 1.373 s against 1.135 s for C, 1.21 times on macOS (1.80
     before the slice); on Linux 1.372 s against 0.079 s, 17 times (27.6 before). The same program in Rust
     with Tokio 1.40's runtime on as many threads, measured outside the repository since it needs a
     library: 0.42 s on Linux, 0.97 s on macOS, so Tokio is 5.5 times C on Linux and 0.86 times on macOS.
     Most of the distance on Linux is the C's: with eight connections the system wakes each blocked thread
     directly, which on Linux is cheap, and every async runtime pays for its scheduler there; async wins
     where a server has thousands of connections. `udpecho` went from 1.18 to 1.11 times C on macOS (1.175
     s against 1.087 s, 1.217 s checked) and from 2.36 to 1.49 on Linux. The rest is in "Slower than C".
675. **The tests**: `ok_net_wakes_from_thread` (a server waits on the loop before the pool has a thread, and
     a thread of `Task.run` wakes `main` while it waits), and `compiled_small_pools` in `cargo test`, which
     builds it and six other async programs on the network, on processes and on threads with a pool of one
     thread and of two (677), under the thread sanitizer: the thread waiting on the loop is then soon the
     only one, so the pipe of 670 has to wake it. Without that write the first program never ends. No test
     makes a pool of as many threads as the machine has cores all busy, since that number differs from one
     machine to another. `cargo test` passes on macOS and on Linux in a container, where it compiles 302 of
     1233 programs, 62 of them under the thread sanitizer, and 15 of 20 programs of tests.
676. **A thread that leaves the loop hands it on** (from the slice): a thread that takes a task when no other
     thread waits on the loop and none looks for work wakes one, or starts one, which finding nothing waits
     on the loop. Without it, the thread that waited there and took a long task would leave the loop to
     nobody while idle threads slept, and the other connections would wait for that task, where the loop's
     own thread used to wake them at once. It costs a wake for each round of the loop: `echo` took 1.31 s on
     macOS without it and 1.37 s with it, the same on Linux.
677. **A pool of a size the tests choose** (from the slice): the C of the runtime takes the most threads of
     the pool from `SG_POOL_MOST` when the C compiler is given it (`-DSG_POOL_MOST=1`), which only
     `compiled_small_pools` does; a program has as many as the machine has cores, as before. Rust's Tokio
     reads the number from a variable of the environment (`TOKIO_WORKER_THREADS`) and Go from `GOMAXPROCS`;
     whether SG gets one is a question for later.

Choices made in the eighty-second slice (a socket goes on the loop of the network once; compiled code
alone). 678 to 688 were decided before it on 16 November 2026, with the numbers from the slice; 689 comes
from the slice. All were approved the same day:

678. **Where a socket is kept on the loop**: in a table by the number the system gave it, in blocks of 1,024
     that never move, made as they are needed, so the loop reads a slot without a lock on the table. A
     record of its own for each socket was the other way, as Tokio has: a `TcpStream` is a value that moves,
     so its place in memory changes, a `UdpSocket` is a number in SG, and several tasks may hold one
     listener; the number is the same for all three.
679. **When it goes on**: the first time a task waits for it, for reading and writing at once, edge-triggered
     (`EPOLLET` on Linux, `EV_CLEAR` with kqueue, which takes the two filters one at a time, since a pipe may
     not have both). It stays on until it is closed. A socket only code without async uses never goes on, as
     before.
680. **A wait is no call of the system**: the loop writes into the slot what is ready and counts its events
     (Tokio's tick), and wakes the tasks waiting there. A task tries the call first; when it finds the socket
     not ready, it waits in the slot, unless the count changed while it tried, in which case it tries again.
     Before, a socket was put on the loop again for each wait: one `epoll_ctl` or `kevent` each time, four
     calls for a listener and a datagram socket (a copy made, put on, taken off, closed).
681. **Several tasks may wait for one socket** (a `shared` listener or `UdpSocket`): readers and writers wait
     on two lists in the slot, and an event wakes all of the list, as in Tokio; the one that finds something
     takes it, the others wait again. The copy of the socket that each wait made (656) is gone.
682. **A cancel**: the task is taken out of the slot under its lock and goes on at once. Whoever takes it
     out, the loop or the cancel, wakes it, so it is woken once. The loop no longer has to be woken for a
     cancel, nor hold the waits it ended until its round is over; the end of a program waited for the async
     way (628) still is, since its event names the wait.
683. **Closing**: the slot of a socket is let go of before the socket is closed, and its generation goes
     up; an event carries the number and the generation, so one that comes for a closed socket after its
     number went to another is dropped. Every way a socket is closed goes through that: a connection, a
     listener, `UdpSocket.dispose` and the pipe of a program read the async way.
684. **The locks**: 256 locks shared by the slots, chosen by the number, instead of one for each slot; a slot
     keeps two numbers, two bits and two lists.
685. **What stays**: the end of a program is waited for once, as before (`EPOLLONESHOT`, `EV_ONESHOT`), and
     input and output on threads and the `poll` of code without async do not change.
686. **Compiled code alone**: nothing a program writes or sees changes, nor the checker; the interpreter
     runs every task on one thread and waits with `poll`, as before.
687. **The tests**: `ok_net_slots`: three tasks wait to accept on one `shared` listener and four to receive
     on one `shared` `UdpSocket`, one of which gives up and is cancelled while the other three wait on, and
     twenty rounds close a connection and two datagram sockets and open new ones, which get the same numbers.
     Without letting the slot go when a socket is closed (683) the rounds never end: the new socket is
     taken to be on the loop already. It is in `compiled_small_pools` too, with a pool of one thread and of
     two under the thread sanitizer. `cargo test` passes on macOS and on Linux in a container, where it
     compiles 303 of 1234 programs, 63 of them under the thread sanitizer, and 15 of 20 programs of tests.
688. **Measured** (`echo`): 1.142 s against 1.108 s for C, 1.03 times on macOS (1.174 s checked; 1.373 s,
     1.23 times, before the slice, measured beside it); on Linux in a container 1.382 s against 0.078 s, 17.7 times,
     as before (17.2). The calls of `epoll_ctl` went from 320,019 to 18 (counted with `strace`), but the
     time on Linux is in the queue of the pool: the calls of `futex` the threads wake each other with,
     about a million. That is the next slice. `udpecho`: 1.09 times C on macOS (1.158 s against 1.063 s), as
     before, and from 1.54 to 1.16 on Linux (0.811 s against 0.697 s, after 689). The others take the time they took: `echowork`, `netlines`,
     `pipesasync`, `asynccopy`, `echosync` and `udpechosync`, within the noise. Not measured: a
     server with thousands of connections, where a wait with no call of the system counts most.
689. **A datagram socket stops blocking on its first async call** (from the slice): an async call of a
     `UdpSocket` used to try the socket with `MSG_DONTWAIT`, which leaves the socket blocking for the calls
     without async. On macOS that flag gives `EAGAIN` while another thread is receiving from the socket,
     though a datagram is there: the call does not wait for the other thread. With a socket put on the
     loop once, nothing comes after to tell of that datagram, and in `ok_net_slots` two tasks waited for
     ever with two datagrams in the socket, about one run in four. The same in C, without SG, does it too,
     while one thread alone, a socket that does not block, or Linux never do. So the socket now stops
     blocking the first time an async call uses it, as a `TcpStream` does (476), and the async calls go
     without the flag. A call without async on such a socket still waits with `poll` when it is not ready,
     which then costs a call of the system more; a socket only code without async uses blocks, as before.

Choices made in the eighty-third slice (a queue for each thread of the pool, from which the others steal;
compiled code alone). 690 to 701 were decided before it on 16 November 2026, with the numbers from the
slice; 702 comes from the slice. All were approved the same day:

690. **A queue for each thread**: each thread of the pool has a ring of 256 tasks, which it alone puts
     on and takes from first in first out, and from which the other threads take half when they have
     nothing (they steal), without a lock: the thread moves the end it puts at, and whoever takes moves
     the other end by exchange (Tokio's queue). A list under a lock for each thread was the other way,
     simpler, but a thread would take that lock for each of its own tasks too.
691. **Where a woken task goes**: a task woken on a thread of the pool goes on that thread's queue; one
     woken outside it (the main thread, a thread of `Task.run`, the clock, a thread of input and output)
     goes on a queue the threads share, under a lock, as every task did before. The tasks that a round of
     the loop of the network wakes go on the queue of the thread waiting on it, and when there is more
     than one, another thread is woken to take some (Tokio's rule).
692. **The slot**: a task that a task woke on a thread of the pool, such as the one that awaited a task
     that ended, goes in the thread's slot and runs next, while what the two share is still in the cache;
     the task that was in the slot goes on the queue. Three in a row at most come from the slot, then a
     task woken goes on the queue, so that two tasks that wake each other leave the others a turn (Tokio's
     limit). Unlike Tokio's, a thread that looks for work takes from the slots of the others too, so that a
     task there need not wait for the end of a long task on its thread: it costs an exchange each time the
     slot is used. A thread looks there when it has nothing to do or is woken for work; when all the
     others sleep and nothing wakes them, the task in the slot still waits for the task before it, as in
     Tokio.
693. **A full queue**: half of it, with the task put, goes on the shared queue under one lock.
694. **Taking turns**: every 61 tasks a thread takes from the shared queue first, and looks at the loop of
     the network without waiting if no thread waits on it (Tokio's numbers; Tokio has since made the first
     one change with the time a task takes, which is left for later). 676 stays: a thread that takes a task
     while no thread waits on the loop wakes one, which finding nothing waits on it.
695. **Stealing**: a thread with nothing to do looks at the others from one chosen at random (a xorshift of
     its own, not the generator of `Random`), takes half the queue of the first that has tasks, its slot if
     the queue is empty, and the shared queue last. Half the threads at most look for work at once.
696. **Sleeping and waking**: the number of threads looking for work and of threads awake are kept in one
     word, so that a thread that puts a task and sees one looking, or every thread awake, wakes nobody
     without a lock. The threads asleep are on a list under the lock, each with a condition of its own, so
     one of them is woken, the one waiting on the loop last, since it would leave it; it is woken through
     the pipe of the loop (670). The last thread to stop looking looks at every queue once more before it
     sleeps, since a task put meanwhile woke nobody. The count of wakes not yet taken (669) is gone.
697. **What stays**: as many threads as the machine has cores, started as they are needed, and
     `SG_POOL_MOST` (677) for the tests.
698. **Compiled code alone**: nothing a program writes or sees changes, nor the checker; the interpreter
     runs every task on one thread, as before. The order in which tasks that can run do run was never
     promised; no test relied on it.
699. **The tests**: `ok_pool_queues`: a cancel on a thread of the pool wakes a thousand tasks waiting on one
     source, more than one queue holds, so half of it goes to the shared queue, and the other threads steal
     them; then four chains of 500 tasks, each awaited by the one above it, run through the slots three at
     a time. Counted in the C while it was written: the queue filled 4 to 6 times a run, threads stole 78 to
     93 times, from a slot 0 to 8 times, and the limit of the slot cut 500 chains. It is in
     `compiled_small_pools` too, with a pool of one thread and of two under the thread sanitizer. That two
     tasks waking each other leave the others a turn is not tested: SG has nothing yet with which two tasks
     wake each other without a thread outside the pool. The thread sanitizer on Linux found that a
     task's place in the source was read after it was put on a queue, where another thread could take it,
     run it to its end and free it; it is read before now. `cargo test` passes on macOS and on Linux in a
     container, where it compiles 304 of 1235 programs, 64 of them under the thread sanitizer, and 15 of 20
     programs of tests.
700. **Measured** (`echo`, and Tokio beside it): 1.004 s against 1.129 s for C, 0.89 times on macOS (1.009 s
     checked; 1.208 s, 1.07 times, before the slice, measured beside it); on Linux in a container 0.549 s
     against 0.074 s, 7.4 times (1.382 s, 18.7 times, before). The calls of `futex` went from 1,096,384 to
     452,656 (counted with `strace`). Tokio's echo (674) takes 0.42 s on Linux and 0.97 s on macOS, so SG
     is 1.3 times Tokio on Linux and 1.04 times on macOS, where it was 3.3 and 1.2. What is left on Linux
     is mostly wakes: a round of the loop that wakes more than one task wakes a thread to take some, and
     676 wakes one to wait on the loop, which alone costs 0.03 s here (0.52 s without it). `udpecho`: 1.06
     times C on macOS (1.161 s against 1.091 s, 1.155 s checked; 1.11 before), and 0.57 times on Linux
     (0.400 s against 0.702 s; 1.16 before), where the C's threads, each blocked on its own socket, are
     slower than the pool.
701. **Measured** (the other async programs): they take the time they took, on both systems, within the
     noise: `echowork`, `tasks`, `tasklist`, `async`, `asynclines`, `asynccopy`, `pipesasync`, `spawnasync`,
     `echosync` and `udpechosync`. `netlines` took 0.092 s to 0.097 s on Linux in three runs, against 0.090 s
     to 0.094 s before: 3 per cent slower at most, if it is not the noise; on macOS the same as before.
702. **The thread that finds work wakes the next only if work is left** (from the slice): the last of the
     threads searching that finds a task wakes another when a queue still has a task, its own or the shared
     one or another thread's, as 669 had it; Tokio wakes one whatever is left. With Tokio's rule an async
     read of the input woke a second thread for nothing each time a thread of input and output woke the
     task: `asynclines` took 0.150 s on Linux against 0.133 s before the slice, with half as many calls of
     `futex` again (18,903 against 12,507), and now 0.135 s with 12,323. Looking costs two numbers read for
     each thread, only when the last thread stops searching.

Choices made in the eighty-fourth slice (a C file beside the program, the variables of C, `errno`, and a
pointer as a number), the first of the slices that bring SG to what C reaches (04, "Reaching what C
reaches, the direction"). 719 to 729 were decided before it on 16 November 2026; 730 comes from the slice.
All were approved the same day:

719. **A C file is named in place of a library**, decided before the slice: `extern "./native.c" { ... }`.
     The path starts with `./`, ends in `.c` and stays in the module's folder or below it (no `..`, no empty
     part, letters, digits and `_ - . +`); the file is looked for beside the file of SG that names it, and
     one that is not there is an error at the block. A name that ends in `.c` without `./` is an error whose
     hint adds it. The block declares what a library block declares (functions, types of C seen only
     behind a pointer, and now variables), with the same rules: C types only, each call inside `unsafe`,
     `public` on each line, and a C name is a name of the module.
720. **Where the file goes**, decided before the slice: compiled code takes it in with `#include` at the
     end of the unit, after the program, so that its macros touch nothing of the program; the program
     reaches each of its functions through an adapter declared before and written after the file, which
     calls the function by its name, so a `static inline` one is inlined at `-O2`. The interpreter puts the
     file at the top of the C of its wrappers (at the end since 748). Both define `_GNU_SOURCE` on Linux when a C file is used, so
     the file sees every function of the system's headers, and both build with `-std=c11`, so assembly is
     written `__asm__`. The wrappers of the interpreter now declare a function of a library under a name of
     their own bound to its C name, as compiled code does, so that no header a C file includes clashes with
     the declaration.
721. **The libraries a C file needs**, decided before the slice: the library blocks of a module are linked
     when anything of C in the module is used, an empty one too (`extern "z" { }`); before, only the library
     of a function called was linked.
722. **A C file is compiled when its block is used**, decided before the slice: when a function it declares
     is called or a variable it declares is read or written; otherwise it is left out, as a library no
     function of which is called. What the C compiler says about the file is shown under the error of the
     interpreter.
723. **A wrong declaration**, decided before the slice: the adapter calls the function of the file with the
     types SG declared, so the C compiler sees a call whose arguments do not fit; the tests build with
     `-Werror`, where a pointer of another sign is an error too.
724. **The variables of C**, decided before the slice: `int opterr` in an `extern` block, a line without
     parentheses. It holds plain data (numbers, `bool`, raw pointers, arrays and structs of them); a `char`,
     a `string`, a type of C seen only behind a pointer and a borrow are errors that say what to write, and
     `int count = 1` is one too. A use of the name is an item of raw memory at the variable's address
     (`unsafe opterr`, `unsafe { opterr = 0 }`, `unsafe { origin.y = 40 }`), so it reads and writes what raw
     memory reads and writes, and the error outside `unsafe` says that C may change it at any time and from
     any thread. A local variable of the same name hides it. It is not lent (`ref opterr`), and its
     address is not taken yet. The same variable declared in two modules is declared the same way. Compiled
     code reaches it through a function of its own that gives its address (`sg_cv_opterr()`), declared
     under a name of the runtime bound to the C name for a library, written after the file for a C file;
     the interpreter asks the wrappers for the address once.
725. **`errno`**, decided before the slice: `C.errno()` and `C.setErrno(n)`, without `unsafe`, since a number
     read or written touches no memory; `C` is a built-in name, as `Memory` is. Compiled code reads and
     writes the `errno` of C, as it is. The interpreter keeps an `errno` for each fiber: a wrapper puts it in
     place before the call and keeps what the call left, so that the interpreter's own work between two calls
     of C changes nothing. It is read right after the call it belongs to; `ok_c_errno` frees two texts
     between the call and the read, in both. (The ninety-fifth slice adds `C.lastError()`, the system's
     own last error, 958.)
726. **`IoError.fromCode(n)`**, decided before the slice: the variants a call of the system gives (`NotFound`,
     `PermissionDenied`, `AlreadyExists`, ...), the same mapping the files and the network use, and
     `Other(code)` for the rest; a number below 0, which the runtime uses for codes of its own, is
     `Other(n)` as it is. No new variant. (The ninety-fifth slice: `fromCode` takes the system's code,
     `C.lastError()`, and `IoError.fromErrno` a number of `errno`, 959.)
727. **An address as a number and back**, decided before the slice: `(nuint)p` and `(nint)p` without
     `unsafe`, any other integer type an error whose hint is these two; `unsafe (raw T)n` from a `nint` or a
     `nuint`, a number written there taken as a `nint` (`(raw void)(nint)-1`, `(raw byte)0x1000`), and any
     other integer type an error whose hint is the cast. The interpreter finds the block of a pointer made
     from a number by its address, so a pointer that went through a number is still checked.
728. **The difference and the order of two pointers**, decided before the slice: `p - q` of two pointers of
     one type gives the count of items between them as a `nint`; on a `raw void` it is an error whose hint
     counts bytes (`unsafe ((raw byte)p - (raw byte)q)`), on a type of C seen only behind a pointer an
     error. `p < q`, `<=`, `>`, `>=` of two pointers of one type give their order. Neither needs `unsafe`.
     Compiled code takes both on the numbers of the addresses (`uintptr_t`), so two pointers into two
     blocks are no undefined behaviour of C; the interpreter panics on the difference of two pointers into
     two blocks it knows, which is always a mistake, and not on their order.
729. **The tests and the measurement**, decided before the slice: `ok_c_errno` (`open` of a file that is not
     there, `strtol` that says through `errno` that a number does not fit, `C.setErrno`), `ok_c_getopt`
     (`getopt` of the C library and its variables `optarg`, `optind`, `opterr`, with the words built in raw
     memory, since a `CString` kept by C is not here yet), `ok_raw_addresses`, `panic_unsafe_two_blocks`, the
     program `tests/modules/cfile` (a C file with a macro, two `static inline` functions, a builtin and two
     variables, one a struct; a variable of the C library; and a module with a C file of its own that calls
     zlib, linked by an empty block), and 14 `error_` programs for 719 to 728. `error_raw_order` is gone,
     since an order of two pointers is no longer an error. `cargo test` passes on macOS and on Linux in a
     container, where it compiles 308 of 1253 programs, 64 of them under the thread sanitizer, and 16 of 21
     programs of tests.
     Measured (`inline`, a function of a C file called five hundred million times in a loop): 1.072 s
     against 1.044 s for the same loop in C, 1.03 times on macOS (1.059 s checked), and 1.007 s against
     1.125 s, 0.90 times, on Linux in a container (1.029 s checked); no call of `mix` is left in the
     program, it is inlined.
730. **The C files of one program share one unit** (from the slice): a `static` function of one name in
     two C files, also of two modules, is an error of the C compiler, which the interpreter shows; the
     names of the C files of a program are one space, as those of the headers of a program in C are. The
     other way would be a unit for each C file, its functions reached across units: a `static inline`
     function is then inlined only with link-time optimization (`-flto`), which every build of a program
     that uses a C file would need. The test that found it is `tests/modules/cfile`, whose module first
     had a function of the name its main file has.

Choices made in the eighty-fifth slice (functions of SG that C calls, pointers to functions of C, the
state a callback reaches), the second of the slices that bring SG to what C reaches. 731 to 746 were
decided before it on 16 November 2026; 747 to 749 come from the slice. All were approved the same day:

731. **`export` in front of a function**, decided before the slice: `export int compareInts(raw void a, raw
     void b) { ... }`, only on a function at the top level of a module. A method is an error whose hint
     reaches the value through the pointer C gives; `async` is an error (C cannot wait for a task), and so
     is `main`. `export` is not `public`: whether other modules of SG see the function is said apart.
732. **What it takes and gives**, decided before the slice: the values C has, integers, `float`, `double`,
     `bool`, `raw T`, `CFunc` and structs of plain data by value, and `void`; a `read`, a `ref` and a
     `CString` are errors, since a pointer C gives is a `raw T`, read inside `unsafe`, and calling it a
     borrow would tell the checker something nobody checked.
733. **The name C sees**, decided before the slice: the name written, without the module, so it is one in
     the whole program; two `export` functions of one name in two modules are an error that shows both,
     and so is a name of a function or a variable of C, a name that starts with `sg_` (the code SG writes)
     and a word of C (`register`). SG calls an `export` function by its name, as any other, with no
     `unsafe`.
734. **A C file of the program calls it by its name**, decided before the slice: SG declares every `export`
     function before the C files, so a file calls it without declaring it, and a declaration of its own
     that differs is an error of the C compiler.
735. **A panic inside ends the process**, decided before the slice: with its message and the code of a
     panic, also under `sg test` in the interpreter, where the tests after it do not run. Nothing unwinds
     through C, and C is not handed a made-up value to go on with. Compiled code ends at a panic anyway;
     the interpreter writes out what was printed, then the message, and ends.
736. **The thread C calls it on**, decided before the slice: compiled code runs it on any thread, one that C
     started too. The interpreter runs it only on a thread that is in a call of C, through the interpreter
     of that call; on another thread the process ends with an error that says compiled code takes such a
     call. To know it in compiled code each call of C would have to mark its thread, and the calls that
     are inlined (720) would not stay free.
737. **`CFunc<A, R>`**, decided before the slice: written as `Func` is, the return type last, `void` for
     none, and only the values of 732; a borrow among them is an error. It is plain data: `Copyable`, a
     field of a struct C gets by value, an item of raw memory; the rule of 71 that a pointer to a function
     is a `raw void` is gone (04, "Structs of C").
738. **What is a `CFunc`**, decided before the slice: the name of an `export` function, and the name of a
     function of C (`free`, for sqlite to call), where a `CFunc` of its types is expected; a function of
     SG without `export` is an error whose hint adds it, and types that differ are an error that shows
     both. Without a `CFunc` expected, a name is a `Func` as before, and the name of a function of C is
     the error it was.
739. **A call through a `CFunc` is a call of C**, decided before the slice: inside `unsafe`, whatever it
     points at, since its type does not say where it came from; `buffer.pointer()` is an argument of it
     as of a function of C. An `export` function called by its name needs no `unsafe`.
740. **`null` and the casts**, decided before the slice: a `CFunc` may be `null`, as a `raw T` may, and
     `CFunc<...>?` is an error whose hint says so; `f == null`, `f != null` and `f == g` need no
     `unsafe`. Calling a `null` one is a panic in both, where compiled code tests it before the call.
     `unsafe (CFunc<int, int>)p` takes one from a `raw void` (what `dlsym` gives; a number is made a
     `raw void` first), and `(raw void)f` gives one back, with no `unsafe`.
741. **`unsafe Memory.addressOf(ref x)`**, decided before the slice: the address of a variable or a field,
     as a `raw void`, for C to keep and give back; a value made in the call, an index and a `read` are
     errors. C does not read through it: what C reads goes to it as `ref T`, or lies in memory of
     `Memory.allocate`.
742. **`unsafe (ref State)data` and `unsafe (read State)data`**, decided before the slice: what lies at the
     address, lent again; kept in a variable (`var state = unsafe (ref State)data`), it is a local borrow.
     That the value still lives and has not moved is the program's promise; from anything but a `raw void`
     the cast is an error.
743. **The interpreter checks the address**, decided before the slice: it keeps the addresses
     `Memory.addressOf` gave, each with the cell of its value and its type, and panics on a type that
     differs, a value that has ended, a value moved away, and an address it did not give.
744. **How the interpreter is called back**, decided before the slice: its library of wrappers gets a
     function for each `export` function, of its name and its C types, that hands the addresses of the
     arguments to an entry of the interpreter; a wrapper for each set of C types a `CFunc` is called with,
     which calls through the address; and a function that gives the address of a function of C named as
     a `CFunc`. No libffi. A call of C marks the interpreter that makes it for its thread, and touches
     nothing of it until the call returns, so a function C calls in between runs on the same interpreter.
745. **The tests**, decided before the slice: `ok_c_callbacks` (`qsort` and `bsearch` of the C library with
     a comparator of SG, a struct of plain data sorted, `abs` and the same function found by `dlsym` as
     `CFunc` values and compared, a `CFunc` in a field, one that is `null`, a state reached through
     `void*`, and an `export` function called from SG), the program `tests/modules/callbacks` (a C file
     that keeps a callback and its state between two calls, calls an `export` function by its name, and is
     given the address of a function of its own; an `export` function of a module), `panic_cfunc_null`, four
     `panic_unsafe_lend_` programs for 743, 19 `error_` programs, and `callbacks_end_the_process`, which
     runs the interpreter and compiled code apart: a panic inside a comparator, and a function of SG
     called from a thread a C file starts. `cargo test`
     passes on macOS and on Linux in a container, where it compiles 311 of 1279 programs, 64 of them under
     the thread sanitizer, and 17 of 22 programs of tests.
746. **Measured** (`callback`), decided before the slice: ten million numbers sorted by `qsort` of the C
     library with the comparator `compareInts` of SG take 0.904 s against 0.902 s with a comparator of C,
     1.00 times on macOS (0.907 s checked), and 0.873 s against 0.873 s, 1.00 times, on Linux in a
     container (0.874 s checked). The comparator is the `export` function C calls, which calls the function
     of SG; the C compiler joins the two.
747. **A struct by value keeps an `export` function from the C files** (from the slice): a C file does not
     call such a function by its name. The declaration before the C files (734) would name the struct as
     SG's C names it, and the names differ between compiled code (`S_Point`) and the interpreter's wrappers,
     so a C file could not call it the same way in both; the interpreter leaves the declaration out, and
     compiled code's, which it needs for itself, is no promise. Such a function is reached through a
     `CFunc`. Calling it by name would need a name for each struct of plain data that both write, which
     the library built from SG (91) needs too.
748. **The interpreter takes the C files in after its declarations** (from the slice): as compiled code does
     (720), at the end of the C of its wrappers, after the declarations of the functions of C and of the
     `export` functions and before what reaches into the files; the files came first before, which left
     no place for the declarations of 734. `_GNU_SOURCE` stays defined on Linux before every header.
749. **A `lock` block does not call C when an `export` function waits** (from the slice): a call of C may
     call any `export` function back, so in a `lock` block a call of C, or one through a `CFunc`, is an
     error when an `export` function of the program waits, with the place where it does; the same holds
     for what holds the thread in an async function. Otherwise a callback could wait while its caller
     holds a lock, the deadlock the rule of 364 keeps out.

Choices made in the eighty-sixth slice (functions of C that take a varying count of arguments, and SQLite
behind a safe module), the third of the slices that bring SG to what C reaches. 750 to 758 were decided
before it on 16 November 2026; 759 and 760 come from the slice. All were approved the same day:

750. **`...` after the parameters**, decided before the slice: only in an `extern` block, written last, and
     after one named parameter at least, as C11 asks (`int sum(...)` is an error); a function of SG and an
     `export` one do not take it, and there is no `va_list`, so `vprintf` and its kin are not declared.
751. **What goes after `...`**, decided before the slice: the type of each value is the type C reads, so
     the types C widens without a word are errors with the cast as the hint, as in Rust: `sbyte`, `byte`,
     `short` and `ushort` (`(int)b`), `float` (`(double)f`), and `bool` (the hint asks for an `int`, since
     SG has no cast from `bool`); `char` too (`(uint)c`), which C has no type of. A struct by value, a
     `string`, an owned `CString` and a `Func` are errors with their hints. `int`, `uint`, `long`, `ulong`,
     `nint`, `nuint`, `double`, `raw T`, `CFunc`, `ref T` and `read T` of plain data (C's `T*`, written back
     after the call) and `read CString` go.
752. **A number without a type is an error there**, decided before the slice: `30`, `-1.5` and `null` after
     `...` are errors whose hint is `(int)30` or `(long)30`, `(double)1.5`, and a variable of the pointer
     type. C would take `30` as an `int`; libcurl's `CURLOPT_TIMEOUT` reads a `long`, and on Apple's arm64,
     where each argument after `...` lies in eight bytes of the stack, the half C reads beyond the `int` is
     whatever lay there.
753. **A function that takes `...` is no `CFunc`**, decided before the slice: its name where a `CFunc` is
     expected is an error whose hint is an `export` function of one's own that calls it, since C calls such
     a function by a type that says `...`, and `CFunc` has none.
754. **How a call is made**, decided before the slice: the interpreter writes a wrapper into its library for
     each list of C types a function that takes `...` is called with, which calls it with its declaration
     of `...`; compiled code writes an adapter for each list the same way, a `static inline` function that
     the C compiler joins with the call.
755. **The format is not looked at**, decided before the slice: what a `printf` reads after `...` is the
     program's to match, inside the `unsafe` the call is written in; the C compiler's `-Wformat` does not
     see it either, since SG declares the function itself.
756. **`examples/sqlite`**, decided before the slice: a module whose users write no `unsafe`. `Database.open`
     (`sqlite3_open_v2`, then `sqlite3_db_config` with `SQLITE_DBCONFIG_DEFENSIVE` after `...`, so SQL cannot
     write the schema SQLite keeps), `execute`, `rows` (`sqlite3_exec`, whose callback is an `export`
     function that adds each row to a list it reaches through the address C gives back), `prepare`,
     `changes` and `lastInsertId`; a `Statement` with `bindLong`, `bindDouble`, `bindText` (copied by SQLite:
     `SQLITE_TRANSIENT` is made in SG from the number -1), `bindNull`, `step`, `reset`, `isNull` and the
     columns, a text column copied with its length; `quote` through `sqlite3_mprintf("%Q", ...)`; and
     `SqliteError` with SQLite's code and message. Both types are `Disposable`.
757. **The tests**, decided before the slice: `ok_c_varargs` (`snprintf` with no value after the format, with
     an `int`, a `long`, a `double`, a text, a `uint` and a `nuint`, with a `byte` and a `float` cast, with a
     `null` pointer, and `sscanf` writing three places lent with `ref`), the program `tests/modules/varargs`
     (functions of a C file that read their arguments with `va_arg`: `long`s, `double`s, pairs of a function
     and a number where the function is an `export` function or a `CFunc`, places of `int` and of a struct
     lent with `ref`, texts), 16 `error_varargs_` programs, and `examples/sqlite` with eight tests, run in
     the interpreter and compiled, one of which fails when the connection is not defensive. `cargo test` passes
     on macOS and on Linux in a container, where it compiles 314 of 1297 programs, 64 of them under the thread
     sanitizer, and 19 of 24 programs of tests; `error_extern_variadic` is gone. Every compiled program is checked for leaks as before, and the
     interpreter itself leaks nothing under `leaks` on the new programs.
758. **Measured** (`sqlite`), decided before the slice: a million rows put into a database in memory with one
     prepared statement, each text made by `snprintf`, and read back, take 0.320 s against 0.321 s in C,
     1.00 times on macOS (0.321 s checked), and 0.295 to 0.311 s against 0.304 to 0.312 s, 0.97 to 1.00 times, on Linux in a container (0.296 to 0.318 s checked). Nearly all of it is SQLite's.
759. **The name of an `export` function after `...` is its `CFunc`** (from the slice): there is no type
     expected there, where a name of a function would otherwise be a `Func`, which does not go to C; so
     `call_each(2, triple, (int)4)` hands C the address of `triple`. libcurl's write callback is given this
     way (`CURLOPT_WRITEFUNCTION`).
760. **A `Statement` may outlive its `Database`** (from the slice): `Database` closes its connection with
     `sqlite3_close_v2`, which SQLite keeps alive until its last statement is finalized, so a statement
     holds no borrow of the database, and one returned from a function whose database ended still steps.
     A borrow would forbid what SQLite allows, and a statement could not be kept in a struct beside its
     database.

Choices made in the eighty-seventh slice (libcurl behind a safe module), the fourth of the slices that
bring SG to what C reaches. 761 to 773 were decided before it on 16 November 2026, and 774 during it; 775
to 778 come from the slice. All were approved the same day:

761. **`examples/curl`**, decided before the slice: a module `curl` (`Client`, `Response`, `Header`,
     `CurlError`) whose users write no `unsafe`, and `main.sg`, which fetches the URL it is given and
     prints the status, the type, the count of headers and bytes, and the first lines of the body. libcurl's
     multi interface, async requests, cookies, a proxy, uploads and a choice of HTTP/2 are left out.
762. **The start of libcurl**, decided before the slice: SG has no global variable, so `Client.open()` calls
     `curl_global_init`, which libcurl counts (and which may be called from any thread since 7.84), then
     `curl_easy_init`; a `Client` is `Disposable`, and its end calls `curl_easy_cleanup` and
     `curl_global_cleanup`. A request takes `ref this`: one handle is not used by two tasks at once, which
     the borrow rules see. The connections of a client are kept from one request to the next.
763. **`unsafe Memory.view(p, count)`**, decided before the slice: the `count` items at `p` as a `read T[]`,
     Rust's `slice::from_raw_parts`, so a piece of a body is added to a list in one copy
     (`body.addAll(unsafe Memory.view(data, n))`) instead of a loop over its bytes. `p` points at plain
     data, not `raw void`; a count below 0, and `null` with a count above 0, are panics in both. The
     interpreter checks the items against their block, as it checks `p[i]`, and reads them once; compiled
     code is the pointer and the count.
764. **A limit on what a response brings**, decided before the slice: `maxBytes`, 16 MB unless changed. A body
     whose length the server says is larger is refused before it comes (`CURLOPT_MAXFILESIZE_LARGE`); one
     that grows past it is stopped by the callback, which takes no more. Either is the code 63, and
     `isTooLarge()` says so. A compressed body is not unpacked.
765. **Safe defaults**, decided before the slice: only `http` and `https`, on the first request and on
     every redirect (`CURLOPT_PROTOCOLS_STR`, `CURLOPT_REDIR_PROTOCOLS_STR`), so a redirect to `file://` is an
     error; at most 5 redirects (`maxRedirects`); certificates always checked, with no way to turn that off,
     and `caFile` for authorities of one's own; 30 s for a request (`timeoutMs`), 10 s to connect
     (`connectTimeoutMs`); `CURLOPT_NOSIGNAL`, since a program has threads.
766. **Errors**, decided before the slice: `CurlError { code, message }`, the message being what libcurl
     wrote into its error buffer (it names the host, the protocol, the certificate), else its message for
     the code; `isTimeout()` for the code 28. A 404 or a 500 is a `Response`, not an error, as in reqwest and
     C#'s `HttpClient`.
767. **`Response`**, decided before the slice: `status`, `contentType` (`string?`), `headers` (a list of
     `Header { name, value }`, gathered by a second `export` callback), `body` (`List<byte>`), `text()`,
     which checks UTF-8, and `header(name)`, which finds a header whatever the case of its letters.
768. **`get` and `post`**, decided before the slice: `post(url, body, contentType)` sends the type as a
     header in a `curl_slist`, held by a `Disposable` of the module; a line break in the type is an error
     (code 43) before anything is sent, so it cannot become a header of its own.
769. **Numbers in place of constants**, decided before the slice: libcurl's options are written as numbers,
     each with its name in a comment (`// 10002: CURLOPT_URL`), as `examples/sqlite` does; they do not change
     between versions of libcurl. A number option is a `long` cast where it is given (752).
770. **No async**, decided before the slice: `curl_easy_perform` waits on the thread that calls it; an async
     function gives the request to `Task.run`.
771. **The tests**, decided before the slice: `examples/curl` with ten tests against servers of their own on
     the loopback, nothing going out to the network: a page with its status, type, headers and body; a 404;
     a redirect followed, with the headers of the last response; a redirect to `file://`, a loop of
     redirects and `ftp://` refused; a body past `maxBytes`, said and not said; a server that does not
     answer, a timeout; a post with its body and type, and a type with a line break refused; a body that is
     not UTF-8; a zero byte in a URL and a closed port. The server is a task of the test, written in SG. The
     test of HTTPS makes a certificate with `openssl req` and serves with `openssl s_server`, both started
     with `Command`; the certificate is refused until it is given as `caFile`. Without `openssl` that test
     fails. `main.sg` is run by `cargo test` without a URL, so it prints how it is used and ends with 2 (a
     program of several files is now held to its standard error and its exit code, as one of one file
     was); fetching
     `https://example.com` was tried by hand on macOS and on Linux, interpreted and compiled. `cargo test`
     passes on macOS and on Linux in a container, where it compiles 320 of 1310 programs, 66 of them under
     the thread sanitizer, and 20 of 25 programs of tests, the tests of `examples/curl` among them; every
     compiled program is checked for leaks as before. gcc on Linux warns of a read past the 256 bytes of
     the server's buffer, which the check of the window rules out before it; it is listed with the program
     as `ok_text_blocks`' is.
772. **Measured** (`curl`), decided before the slice: a thousand fetches of a megabyte over the loopback, on
     one kept connection, from a server that is a task of the program, take 0.160 to 0.165 s against 0.160 to
     0.163 s in C, 1.01 to 1.02 times on macOS, and 0.118 to 0.142 s against 0.118 to 0.136 s, 0.99 to 1.12
     times, on Linux in a container, where the runs vary more. The copy of each piece through `Memory.view`
     takes the time of `memcpy`; the rest is libcurl's and the loopback's.
773. **The documents**, decided before the slice: this list, the table at the top, "Not here yet", the
     paragraph on the back end and the row of the measurement; 04, 02 and 06 and the README of the
     repository.
774. **A call of C lets the other tasks run in the interpreter** (decided during the slice): a call of C
     that waits holds up only its own thread in compiled code, but in the interpreter, where one fiber runs at
     a time, it held up every task, so a server task never answered the request its program was making. Now
     a call of C made while there are other tasks gives the turn away for its time, as a wait for the
     network does: the tasks of `Task.run` not started yet start, the others go on, and when nobody is
     ready a thread of its own waits for the first that is. A function C calls back during the call
     takes the turn while it runs, and gives it away again. A program with one task pays nothing. One that
     comes back while the run ends after a panic of another task still runs, and its task unwinds when
     the call of C returns. `ok_c_call_tasks` shows it: a task runs while `usleep` waits, `qsort` calls
     back beside another task, and a task waits in C while the one beside it runs. Chosen by the user
     among three ways (the other two were a server in C on a thread of its own, and a server in another
     process), on 16 November 2026.
775. **An adapter of `...` for each list of the types of SG** (from the slice; a mistake of 754 put right):
     compiled code made one adapter for each list of C types, but a `read CString` and a `raw byte` are both
     a pointer of C and go over differently, the text of the one and the address the other holds. A
     `curl_easy_setopt` with a text and another with a `raw void` shared one adapter, and the URL went to
     libcurl as the address of the `CString`. The interpreter was right, since its wrappers write each value
     as C reads it. `ok_c_varargs` gives `snprintf` a `read CString` and a `raw byte` for one `%s`.
776. **A window borrows its pointer** (from the slice): `Memory.view(p, n)` borrows the variable or the field
     that holds `p`, so it is not returned from where `p` ends, not kept past a change of `p`, and given
     back from a function only with `from p` on a `read raw T` parameter. A pointer that is not a variable
     or a field (`p + 1`) is an error whose hint names it first.
777. **What libcurl keeps past a call** (from the slice): an option that points at memory is read by libcurl
     later, during the request, and the interpreter lends `buffer.pointer()` only for the time of a call. So
     the error buffer is room of `Memory.allocate` that the `Client` holds as long as it lives, and the body of
     a post is copied by libcurl when it is given (`CURLOPT_COPYPOSTFIELDS`). Each request starts with
     `curl_easy_reset`, so the options of the last one (a body, its headers, the address of its state) never
     reach the next; the connections stay.
778. **Headers count toward `maxBytes`** (from the slice): a server that sends headers without end is cut
     off as one that sends a body without end. A line of the headers that is not UTF-8 is left out of
     `headers`, and each status line starts the headers again, so they are those of the last response.

Choices made in the eighty-eighth slice (signals), the fifth of the slices that bring SG to what C
reaches. 779 to 793 were decided before it on 16 November 2026; 794 to 799 come from the slice. All were
approved the same day:

779. **`Signal`**, decided before the slice: a built-in enum of the signals a program may listen for,
     `Interrupt`, `Terminate`, `Hangup`, `Quit`, `User1`, `User2` and `WindowChange`; `Copyable`,
     `Formattable`, `Equatable`, `Hashable`, and written in SG in the module `std`. No numbers are given:
     `User1` is 30 on macOS and 10 on Linux. Left out: the signals of faults (SIGSEGV, SIGBUS, SIGFPE,
     SIGILL), those that cannot be caught (SIGKILL, SIGSTOP), SIGPIPE, which the runtime deals with,
     SIGCHLD, which `Process` uses, and SIGALRM and the signals of job control, for later.
     `ExitStatus.Signal(int)` keeps its number, since a program may end by any signal.
780. **`Signals.listen(read Signal[] signals)`**, decided before the slice: gives a `SignalListener`, which
     is not `Copyable` and may be given to another task. No signal at all is a panic; a signal written
     twice is one. While a listener of a signal lives, the signal does not end the program; once its last
     listener ends, it does what it did before (Go's `signal.Stop`; Tokio keeps its handler for good).
781. **`listener.next()`**, decided before the slice: takes `ref this` and gives a `Signal`. A signal that
     came after `listen` and before the call is kept; several of one kind before a call are one (the
     system does not count them either, and Tokio folds them too); kinds come in the order they came;
     every listener of a kind gets it. In the interpreter the other tasks run while it waits.
782. **`listener.nextAsync(read token)`**, decided before the slice: a `Task<Result<Signal, Cancelled>>`,
     awaited where it is written; the token may be left out (627). A cancel ends the
     wait, and a signal that comes later stays for the next call. `next()` in an async function is an
     error whose hint is `nextAsync`, as for the calls of 608.
783. **`Signals.raise(signal)`**, decided before the slice: the signal to this program, for tests and for a
     program that cleans up and then ends the way the signal asks. Without a listener the signal does
     what it does by default, so the program ends at once, as in C. It is C's `raise` (794).
784. **`process.signal(signal)`**, decided before the slice: a `Process` started with `Command` is sent a
     signal by its name; it gives nothing, as `kill()` does, since a program not waited for keeps its
     number. There is no way to send a signal to any number.
785. **How it works in C**, decided before the slice: the handler, set with `sigaction` and `SA_RESTART`,
     only writes the kind into a pipe that does not block and is closed on `exec`, and keeps `errno`. The
     first `listen` starts a thread that waits for the pipe (`poll`) and hands each signal, under a lock, to
     the listeners of its kind; the pipe is read only under that lock (794). `next` waits on a condition of
     its listener, and an awaiting task is woken as one that waits for a thread of input and output is. A
     program that never listens pays nothing.
     `SA_RESTART` keeps a read of `Stdin` or an `accept` from failing with `EINTR`. `sigwait` was left out:
     the mask would have to be set before any thread starts, and a program started with `Command` gets it.
786. **The interpreter**, decided before the slice: the same way (`signals.rs`), the handler set with
     `signal` through `extern "C"`, no library added. A wait is a piece of work the relay tells the
     scheduler of, so a fiber that waits gives the turn away, with `next` as with `nextAsync`.
787. **A handler of C**, decided before the slice: one can still be set inside `unsafe`, through `export`
     and `CFunc`. When it and a listener are set for one signal, the last one set wins; this is written
     here, not checked.
788. **Programs started with `Command`**, decided before the slice: `exec` gives back to them what a
     signal does by default, so a listener of this program is not theirs. Ctrl-C at a terminal reaches
     every program of the group, those started by this one too.
789. **A listener nothing reads**, decided before the slice: `var quiet = Signals.listen(...)`, kept only
     so that Ctrl-C does not end the program, is not the error of a variable nothing reads; a
     `SignalListener` is `Disposable`, and those are left out of that rule (590).
790. **`examples/shutdown`**, decided before the slice: an echo server over TCP, async, that ends cleanly:
     the first `Interrupt` or `Terminate` cancels the source of its accepts, so it takes no more
     connections and waits for those that are open; a second cancels the source of the open ones. It then
     says how many it served and ends with 0. Its module `server` has two tests, which send `User2`
     with `Signals.raise`; the test `shutdown_on_signals` of `cargo test` starts the program interpreted,
     compiled and compiled under the thread sanitizer, and sends it real `SIGINT` and `SIGTERM` from
     outside, with no connection, with one served to its end, and with one cut by a second signal.
791. **The tests**, decided before the slice: `ok_signals_next` (several of one kind are one, the order of
     kinds, every listener, a listener given to a task), `ok_signals_async` (a cancel, a cancelled token,
     two tasks each with a listener), `ok_signals_default` (the program ends by the signal once its last
     listener has ended), `ok_signals_kept` (789), `ok_signals_process` (`Interrupt` and `Terminate` to
     `sleep`, `Hangup` to a shell that catches it), `panic_signals_listen_nothing`,
     `error_signals_next_in_async` and `error_signals_listen_one`; `examples/shutdown` with its two tests
     and `shutdown_on_signals` (790). `cargo test` passes on macOS, where it compiles 327 of 1319 programs,
     73 of them under the thread sanitizer, and 21 of 26 programs of tests, the tests of
     `examples/shutdown` among them, and on Linux in a container with the same counts. Every compiled program is checked for leaks as before,
     except `ok_signals_default`, which a signal ends (797).
792. **Measured** (`signals`), decided before the slice: a hundred thousand rounds of `Signals.raise` and
     `next()` take 0.318 to 0.326 s against 0.250 to 0.254 s for C's self-pipe, whose program reads the pipe
     itself, 1.27 to 1.28 times on macOS (0.313 to 0.323 s checked), and 0.265 to 0.268 s against 0.112 to
     0.115 s, 2.36 times, on Linux in a container (0.255 to 0.267 s checked). The relay wakes for each
     signal although `raise` has handed it on: without that wake the program takes 0.253 s on macOS, the
     time of C, and 0.131 s on Linux, where waking a thread costs more. It is in "Slower than C".
793. **The documents**, decided before the slice: this list, the table at the top, "Not here yet", the
     paragraph on the back end and the row of the measurement; 04, 02 and 06 and the README of the
     repository.
794. **`raise` gives the listeners the signal before it returns** (from the slice; 783 said
     `kill(getpid())`): C's `raise` sends the signal to the thread that calls it, so its handler has written
     the kind into the pipe when the call returns; sent to the process, it may be handled by another thread
     later. Then `raise` reads the pipe itself, under the lock the relay reads it under, and hands on what
     it holds, so a kind the relay took is handed on too by the time the lock is free. Before this, a
     thread under the sanitizers took the first of three `User1` before the relay had handed on the other
     two, so three were two and the order of kinds came out wrong; and a `raise` that waited for the relay
     took 3.0 times C in `signals`, against 1.27 now (792). What was printed goes out before, as it does
     before `Command` runs a program (605), so a signal that ends the program does not lose it.
795. **What a signal did before comes back** (from the slice; 780 said the default): the first listener of
     a kind keeps what the kind did, and its last listener sets that again. A program started with
     `SIGHUP` ignored (`nohup`) ignores it again once it stops listening, as Go does.
796. **A signal that came as its last listener ended** (from the slice): its kind may still be in the pipe
     when a new listener of it starts, and that listener then gets it. Telling them apart would need a
     count shared with the handler; the case is written here, not dealt with.
797. **The tests that send signals run in a process of their own** (from the slice): a signal reaches the
     whole process, and one that no listener is left for ends it, so the interpreter runs a program of
     `tests/cases` with `signals` in its name as `sg file.sg`, not inside `cargo test`; the tests of
     `examples/shutdown`, which run inside it, send `User2` only while it is listened for. A compiled
     program that a signal ends is not looked at for leaks, as one that ends in a panic is not (382), and
     a compiled program that awaits a signal runs under the thread sanitizer, since the relay is a thread.
798. **A server says what it does at once** (from the slice): the output of a program into a pipe is
     written out a block at a time, so a line that says the server took a signal could wait in the block
     while the test waits for it. `examples/shutdown` writes each line with `say`, which prints and then
     calls `Stdout.flush()`.
799. **The type of `listen` stays a list** (from the slice): `Signals.listen(Signal.Interrupt)` is the
     error `expected read Signal[], found Signal`, with no hint of its own; a list of one is written
     `read List<Signal> { Signal.Interrupt }`.

Choices made in the eighty-ninth slice (the layout of C), the sixth of the slices that bring SG to what C
reaches. 800 to 816 were decided before it on 16 November 2026, with what Windows asks of them (805, 808,
810 and 816 were changed for it the same day: the language is to run on Windows, Linux and macOS, Linux
and macOS first, and nothing is to be built in a way Windows cannot take later); 817 to 829 come from the
slice. All were approved the same day:

800. **`union Bits { uint word` / `float number }`**, decided before the slice: a type whose fields all
     start where it starts, written as a struct is, one field to a line. It holds plain data only (numbers,
     `bool`, `char`, raw pointers, `CFunc`, and arrays, structs and unions of them), and has the size and
     the alignment C gives it. `public`, `: Copyable` and methods are as for a struct.
801. **A union is built with one field**, decided before the slice: `Bits { number = 1.5 }`; the bytes that
     field does not cover are zero. None, or two, is an error.
802. **A field of a union is read inside `unsafe`**, decided before the slice (and in "The layout of C"
     before that): it is read as its own type, whichever field was written last. Writing one whole needs
     no `unsafe` (`bits.word = 3`), as in Rust. A field of a union is not lent (`read bits.word` is an
     error whose hint copies it out), as a place in raw memory is not (511).
803. **A union is not compared, hashed, ordered or written out by its fields**, decided before the slice:
     `: Equatable`, `: Hashable` and `: Comparable` are errors, and so is `: Formattable` without a
     `format` of its own; there is no `match` on one. The hint reads the field it holds inside `unsafe`.
804. **`uint version : 4`**, decided before the slice: a bit field of a struct, of an integer type (signed
     ones too: `int delta : 3` holds -4 to 3) or of `bool` (1 bit). It has at least 1 bit and at most as
     many as its type. `uint _ : 4`, without a name, leaves bits empty, and `uint _ : 0` starts the next
     field at a new unit of its type. A struct with bit fields holds plain data; a union has none.
805. **Laid out as C lays it out on the machine**, decided before the slice and changed the same day for
     Windows: the back end writes C's bit fields and C places them. The interpreter places them itself
     (`raw.rs`), by one of three rules: macOS and Linux on x86-64 put a field where the bits before it end,
     unless it would cross a unit of its type; Linux on 64-bit Arm does the same, but bits left empty count
     for the alignment of the struct (AAPCS64); Windows, with every compiler there (MSVC, clang-cl and
     MinGW), starts a new unit of its type unless the one before has the same size and room left. The
     test `layouts` of `cargo test` compares eighteen structs and unions with what C gives (829).
806. **A value that does not fit its bits**, decided before the slice: the rule of overflow (a panic in
     the interpreter and in a debug build, the lowest bits in a release build, as C keeps them), for `=`,
     for `+=` and the others, and where the struct is built. A number written in the program that does not
     fit is an error of the checker: `16 does not fit in a bit field of 4 bit(s), which holds 0 to 15`.
807. **A bit field has no address**, decided before the slice: `read flags.version`, `ref flags.version`,
     `Memory.addressOf` of one, and one lent to C are errors; reading one copies it.
808. **`packed struct Header { ... }`**, decided before the slice and changed the same day for Windows: no
     padding, an alignment of 1, plain data only. It has no bit fields, since GCC and Clang pack them bit
     by bit and Windows by units of their type, and it holds no type written with `align(N)`, whose
     alignment Windows keeps inside it and the others do not. The C is written between
     `#pragma pack(push, 1)` and `#pragma pack(pop)`, which every compiler of C reads. `packed(2)` waits.
809. **A field of a `packed` struct is not lent**, decided before the slice: one whose type has an
     alignment above 1 may lie where that type cannot be reached through a pointer, so `read h.size`, a
     method of a struct field called on it, and one lent to C are errors whose hint copies it out (Rust's
     error E0793). The struct itself is lent as any other.
810. **`align(64) struct Slot { ... }`**, decided before the slice and changed the same day for Windows:
     the alignment of the type is at least that (a power of two up to 4096, not below the one its fields
     ask for), and its size a multiple of it. Any struct may have it. Wherever a value of it is kept it lies
     at a multiple of it: on the stack and inside other values as C places it, and on the heap through one
     function of the runtime (`sg_alloc`, 821). `packed` and `align` are not written on one type. An
     alignment written on one field waits.
811. **`Memory.sizeOf<T>()` and `Memory.alignOf<T>()`**, decided before the slice: the size and the
     alignment C has for a type of plain data, as a `nint`, with no `unsafe` (`qsort(..., Memory.sizeOf<int>(), ...)`).
     Another type is an error. `offsetOf` waits.
812. **An item of raw memory lies where its type may**, decided before the slice (and on 16 November, after
     the eighty-seventh slice): `p[i]`, `Memory.view`, `Memory.viewRef` and `Memory.copy` at an address
     that is not a multiple of the alignment of their type are a panic in the interpreter and in a debug
     build: `raw memory is reached 2 byte(s) past a multiple of 4, the alignment of uint: Memory.readUnaligned reads it there`.
     A cast does not panic, as in Rust; a release build tests nothing, so the access costs what it costs
     in C (the rule of overflow, and Rust's checks of a debug build).
813. **`Memory.readUnaligned(p)` and `Memory.writeUnaligned(p, value)`**, decided before the slice: an
     item at any address, inside `unsafe`, copied byte by byte (Rust's `read_unaligned`).
814. **`Memory.viewRef(p, count)`**, decided before the slice (and on 16 November, after the
     eighty-seventh slice): a window that writes, a `ref T[]`, for a buffer C wants filled. It borrows the
     variable or the field that holds the pointer as `ref`, so while it is used the pointer is not read
     and no second window is made from it; two pointers to the same memory are the program's to keep apart,
     inside `unsafe`. The rules of a count below 0 and of `null` are those of `Memory.view`.
815. **`Header.fromBytes(bytes)`**, decided before the slice: a struct or a union of plain data out of
     exactly as many bytes as it takes, as they lie in memory, with no `unsafe`. Only a type that every
     pattern of bits is a value of has it: integers, `float`, `double`, bit fields (a `bool` one too), and
     arrays, structs and unions of them; a `bool`, a `char`, a raw pointer or a `CFunc` is an error. A
     window of another length is a panic, `Pair.fromBytes reads 8 bytes, and it was given 9`, and a
     length the checker sees is an error (the rule of `int.fromLittleEndian`). A struct of the program with
     a function `fromBytes` keeps its own. `toBytes` waits: the padding of a struct has no value.
816. **`examples/headers`**, decided before the slice: a BMP file header (`packed`, its `uint` fields at
     bytes 2 and 10) and its info header, and an IPv4 header (the length and the version in four bits each,
     the addresses a union of a `uint` and four bytes) read out of their bytes with `fromBytes`; the C file
     beside it declares the same headers as C does (with `#pragma pack`, and the bit fields of Linux's
     `struct iphdr`) and reads what SG wrote into raw memory. Three tests. It is the program of the
     threshold that reads a binary header with a union and bit fields.
817. **A field of a union is written whole** (from the slice): `word.halves.low = 1` and `bits.word += 1`
     would read the field first, so they are errors whose hints read it, change the copy and write it
     back. Only the place an assignment writes is free of `unsafe`: `xs[bits.word] = 5` reads it.
818. **A union in the interpreter is its bytes** (from the slice): a value of one keeps the bytes C would
     keep, and a field is read out of them as that field and written over them. A bit field is a field of
     its own; the bits are placed only where a struct is written into raw memory or read out of it.
819. **A window that writes, in the interpreter** (from the slice): its items are cells of their own, kept
     in step with the memory: what the program wrote into the window goes to the memory before raw memory
     is reached again, a call of C among it, and what was written into the memory, by C or through a
     pointer, comes back into the window after; each side writes only the items it changed. Over memory C
     gave (not `Memory.allocate`'s), what C writes there afterwards is not seen through the window, since
     C may have given that memory back; a new window sees it.
820. **What is tested first** (from the slice): a pointer is tested against its block before its
     alignment, as in the interpreter's other panics; `Memory.copy` tests where it copies from before where
     it copies to; `Memory.view` and `Memory.viewRef` test `null` and the count before the alignment, and a
     window of no items tests nothing.
821. **Memory aligned beyond 16 bytes on the heap** (from the slice): `new`, a `List`, a `HashMap`, a
     `shared` value (whose counts in front are moved so that the value lies at a multiple), the frame of an
     async function and of a task, the buffer of `sort` and `Memory.allocate` all take it from `sg_alloc`,
     which is `malloc` up to 16 bytes and C11's `aligned_alloc` above; on Windows it will be `_aligned_malloc`.
     A `List` of such items that grows is copied to a new block, since `realloc` keeps only what `malloc`
     gives. On Linux and macOS `free` gives every block back; on Windows the blocks of `_aligned_malloc`
     need `_aligned_free`, which the slice of Windows will call where the back end frees them.
822. **A block of `Memory.allocate` is given back through a pointer to its type** (from the slice):
     `Memory.free` gives one aligned beyond 16 bytes back with `sg_aligned_free`, by the type of the
     pointer it is given. The interpreter remembers the alignment of each block and panics when it is given
     back through a pointer to a type of another (`panic_unsafe_free_aligned`), so a program that would go
     wrong on Windows goes wrong on macOS and Linux too. Memory C gave with `malloc` is given back through a
     pointer to a type aligned to 16 bytes or less.
823. **The helpers of the back end copy a field they cannot point at** (from the slice): writing a struct
     out, `==` and the hash of a key are made of one function for each type, given the address of each
     field. A bit field has no address and a field of a `packed` struct may lie where its type is not
     reached through a pointer (C warns of it, and the build turns warnings into errors), so such a field
     is copied first.
824. **A struct by value to a function of the program's own C file** (from the slice): C sees the struct
     the C file declares and SG the one it declares, two types of one layout, and the C compiler takes
     them as different. `examples/headers` hands its headers to C through raw memory (`raw void`), which
     is what a test of the layout wants anyway. A function of a library takes a struct by value as before.
     In "Not here yet".
825. **The keywords stay names** (from the slice): `union`, `packed` and `align` are words only in front of
     a declaration of a type (`union Bits {`, `packed struct`, `align(8) struct`), so a variable or a
     function may still be named `align`.
826. **The C of Windows** (from the slice): the C the back end writes uses statement expressions and
     `__builtin_` functions, which Clang and GCC read and Microsoft's `cl` does not; on Windows the
     program will be compiled by Clang (`clang-cl` or MinGW), whose layout is Windows' (805). `SG_ALIGNED`
     is `__declspec(align(n))` for `cl` and `__attribute__((aligned(n)))` for the others.
827. **The tests**: `ok_union`, `ok_bit_fields`, `ok_packed`, `ok_align` (a value of `align(64)` behind
     `new`, in a list, in a `shared checked` value, in a struct in a list, given to a task and given back,
     and in raw memory), `ok_raw_windows`; `panic_bitfield_overflow`, `panic_bitfield_signed`,
     `panic_raw_misaligned`, `panic_view_ref_misaligned`, `panic_from_bytes_length`,
     `panic_unsafe_free_aligned` (the interpreter only), and `bitfield_wraps` in `tests/release`; 32
     errors (`error_union_*`, `error_bitfield_*`, `error_packed_*`, `error_align_*`, `error_sizeof_*`,
     `error_from_bytes_*`, `error_view_ref_twice`, `error_read_unaligned_void`); `examples/headers` with its
     three tests. `cargo test` passes on macOS, where it compiles 339 of 1364 programs, 74 of them under
     the thread sanitizer, and 22 of 27 programs of tests, and on Linux in a container with the same
     counts. A first run on Linux found that glibc does not declare `posix_memalign` under `-std=c11`;
     the runtime takes C11's `aligned_alloc` instead (821).
828. **Measured** (`bitfields`): four million pixels of 16 bits in bit fields of 5, 6 and 5 made brighter
     field by field five hundred times, then four thousand IPv4 headers read with `fromBytes` twenty
     thousand times, take 0.326 to 0.330 s against 0.358 to 0.361 s for C with the same bit fields and
     `memcpy`, 0.91 times, and 0.323 to 0.326 s in a debug build optimised as the others: the tests that a
     value fits its bits cost nothing that shows here. A bit field is C's own, and
     `fromBytes` is one test of the length and a `memcpy`. Why SG is a little faster was not looked into.
829. **The test of layouts** (from the slice): `tests/layouts.rs` builds eighteen structs and unions (the
     bit fields of every kind, bits left empty with and without a size, a union of a struct, a `packed`
     struct holding a struct, `align(64)` and `align(32)` on a union) with the C the back end writes for
     them, and compares where each field lies, the size and the alignment with what C gives: on this
     machine from a program built with `cc` that prints them, and, where `clang` is, for macOS on Arm and
     x86-64, Linux on x86-64, 64-bit Arm and RISC-V, and Windows with MSVC on x86-64 and Arm and with
     MinGW, from the layouts `clang` dumps. Linux in its container has no `clang`, and tests its own
     layout there. Given the wrong rule for a system, it fails with what differs.

Choices made in the ninetieth slice (atomics and `volatile`), the seventh of the slices that bring SG to what
C reaches. 830 to 847 were decided before it on 16 November 2026, with C's and Rust's names kept over C#'s
(`fetchAdd` gives the value before, as C and Rust do; C#'s `Interlocked.Add` gives the one after); 848 to 859
come from the slice. All were approved the same day:

830. **`atomic T`**, decided before the slice: a word in front of the type, as C's `_Atomic` and SG's own
     `locked`, made with `atomic e` (`atomic 0`, `new shared atomic 0`). Unlike `locked`, it is written
     anywhere a value is (a local, a field, an item of a list or an array, raw memory, behind `new` and
     `shared`), since an atomic value is plain data the size of its type, not a value with a lock in front.
831. **What may be atomic**, decided before the slice: every integer type, `bool`, `float`, `double` and a
     raw pointer (`atomic raw Node`). Nothing of 128 bits, which would need `libatomic` and a lock.
832. **The methods**, decided before the slice: `load`, `store`, `exchange`, `compareExchange` and
     `compareExchangeWeak` on all; `fetchAdd` and `fetchSub` on numbers; `fetchAnd`, `fetchOr` and `fetchXor`
     on integers and `bool`. `fetchMax`, `fetchMin` and `fetchNand` wait. There is no `+=` on an atomic value
     and no reading of it without a method; a call that gives a value is used or written `_ = ...`, as every
     call is (614).
833. **`compareExchange(expected, desired)` gives a `Result<T, T>`**, decided before the slice: `Ok` with the
     value it wrote over, `Error` with the value it found, as Rust's. A `float` and a `double` are compared by
     their bits, as C compares them: `-0.0` is not `0.0`.
834. **`fetchAdd` and `fetchSub` follow the rule of overflow**, decided before the slice: a panic in the
     interpreter and in a debug build, a wrap in a release build. The test needs no loop: it looks at the
     value the operation gave back. The new value is written by then, and the panic ends the program.
     `wrappingFetchAdd` and `wrappingFetchSub` wrap in every build, as `wrappingAdd` does.
835. **`MemoryOrder`**, decided before the slice: `Relaxed`, `Acquire`, `Release`, `AcquireRelease` and
     `SequentiallyConsistent`, C11's orders without `consume`; not named `Order`, which a program may well
     have. An operation is sequentially consistent unless an order is written after its values, the way a
     `CancellationToken` may be left out (627), with no second function of the same name. The order is
     written as a variant, not a variable, and one the operation cannot keep is a compile error: a `load`
     that is `Release` or `AcquireRelease`, a `store` that is `Acquire` or `AcquireRelease`, a
     `compareExchange` whose order on failure is `Release` or `AcquireRelease`. Given one order,
     `compareExchange` takes its order on failure from it (`AcquireRelease` gives `Acquire`, `Release`
     gives `Relaxed`); given two, it keeps both.
836. **Through `read` and `shared`**, decided before the slice: an atomic operation changes the value
     through a `read` borrow and inside a `shared` value, as Rust's through `&self`; tasks share one as a
     `shared atomic T` or in a `shared` struct of them. The thread rule is the same. A field written
     `readonly atomic` is an error, since another module could still `store` into it; the hint keeps it to
     its module and gives a method that reads it.
837. **Not `Copyable`, `Equatable`, `Hashable`, `Comparable` or `Formattable`**, decided before the slice: a
     struct with an atomic field cannot say `: Copyable`. A value is moved as any other, and one held alone
     (a local, or through `ref`) is written whole with `count = atomic 2`.
838. **Its layout**, decided before the slice: the size of its type and an alignment of that size, also
     where the type has less (a `long` on 32-bit x86), as C11 and Rust lay it out. It is not a field of a
     union, of a `packed` struct or a bit field. It is plain data, so `Memory.allocate<atomic long>(n)` and
     `raw atomic int` are written, and memory shared with C is read as atomic with a cast inside `unsafe`,
     `(raw atomic int)p`; an operation on an item of raw memory is inside `unsafe` (`p[i].fetchAdd(1)`).
839. **`Atomic.fence(order)`, `Atomic.compilerFence(order)` and `Atomic.spinLoop()`**, decided before the
     slice: C's `atomic_thread_fence`, `atomic_signal_fence` (an order for the compiler alone) and the hint
     that a thread spins (`pause` on x86, `yield` on Arm).
840. **Compiled through the `__atomic` builtins**, decided before the slice: those of GCC and Clang, which
     the runtime's own counts of `shared` already use and which Clang has on Windows; no `stdatomic.h` and
     no `libatomic`. Every atomic value is free of locks on the systems SG runs on.
841. **In the interpreter**, decided before the slice: one task runs at a time, so every operation is one
     step and every order acts as `SequentiallyConsistent`. An atomic operation is a place where the turn may
     go to another task that is ready, in the same order on every run, so that a loop that spins on a `load`
     ends; not inside a `lock` block, where no other task may take a lock held. How often the turn goes was
     changed by the slice (849). A bug of a weaker order than needed is not found by the interpreter;
     compiled tests run under the thread sanitizer.
842. **`struct Pool : unsafe Sendable`**, decided before the slice: a value that holds a raw pointer does not
     go to another task ("C and tasks"); this is the writer's promise that what its raw pointers reach is safe
     for several tasks, and with it the value goes. Swift's `@unchecked Sendable`, Rust's `unsafe impl Send`.
843. **`unsafe Memory.readVolatile(p[i].status)` and `unsafe Memory.writeVolatile(p[i].status, v)`**,
     decided before the slice, a little apart from "Reaching what C reaches", which wrote `readVolatile(p)`:
     the argument is a place in raw memory written as it is read anywhere else, so a field of a block of
     registers is reached as C's `regs->status`. `volatile` is not a type.
844. **What `volatile` takes**, decided before the slice: a place of plain data; a bit field is an error
     whose hint reads the struct whole; the alignment of a debug build is checked as for any item of raw
     memory (812). A volatile access orders nothing between tasks: C's `volatile`, for devices, not C#'s,
     which is an acquire and a release (`load(MemoryOrder.Acquire)` and `store(v, MemoryOrder.Release)` in
     SG).
845. **`examples/queue`**, decided before the slice: Dmitry Vyukov's bounded queue for many producers and
     many consumers, of `long` values since generics wait. Each cell has an atomic number that says whose
     turn it is and an atomic value, read and written `Relaxed`, which costs what a plain read and write
     cost; the head and the tail are `align(64)` so that the producers and the consumers do not share a line
     of the cache. Neither the module nor its users write `unsafe`. `main` sends ten thousand numbers through
     1024 places with four producers and four consumers and checks the sum; its four tests run in the
     interpreter, compiled and under the thread sanitizer. It is the program of the threshold that is a
     queue without locks built on atomics.
846. **The measurements**, decided before the slice: `queue`, against the same queue in C with pthreads and
     `__atomic`, and `atomics`, four tasks adding to two counters (859).
847. **The documents**, decided before the slice: this list, the table at the top, "Not here yet", the
     paragraph on the back end and the rows of the measurements; 04, 02 and 06 (with C#'s `Interlocked` and
     `Volatile` beside Rust's types) and the README of the repository.
848. **An array of atomic values** (from the slice): `atomic uint[3] { atomic 1, atomic 2, atomic 3 }`:
     `atomic` in front of an array built in place makes its items atomic, since no array is. An atomic
     pointer with `*`, `?` or `[...]` after it is named with `using` first (`using NodeRef = raw Node`, then
     `atomic NodeRef`), and `atomic null` with no type known is an error whose hint writes the type.
849. **The turn goes every 64 operations in the interpreter** (from the slice; 841 said at each): a
     hand-over wakes the thread of another task, and `examples/queue` took 10.8 s with one at each operation
     against 0.35 s with one every 64. `Atomic.spinLoop()` gives the turn away at once, and a loop that spins
     without it gives it after 64 rounds at most. The tasks of `Task.run` not started yet start there, so a
     task that spins waiting for one still to start is not left waiting.
850. **An atomic value in raw memory, in the interpreter** (from the slice): read and written as the memory
     it is, after the checks of the block and of the alignment. One task runs at a time, so it is one step
     for the program, but a thread of C that changes it at the same time is not seen in one step ("Not here
     yet").
851. **The panic of `fetchAdd`** (from the slice): `overflow: 2147483645 + 5 does not fit in int`, as for
     `+`, with the hint ``wrappingFetchAdd` wraps around``; compiled, one `__builtin_add_overflow` on the
     value `__atomic_fetch_add` gave back.
852. **What is said instead of a method** (from the slice): `count += 1` is an error that says the read and
     the write would be two steps, with the hint `_ = count.fetchAdd(1)` (`fetchSub`, `fetchAnd`, `fetchOr`,
     `fetchXor` for the others); `count = 2` has the hint `count.store(2)`; `count + 1`, `count == other` and
     `print(count)` have the hint `count.load()`; `Counter { hits = 0 }` has the hint `atomic 0`.
853. **The order on failure is no stronger than on success** (from the slice): C asks it of
     `compare_exchange`, and GCC warns otherwise, so `compareExchange(a, b, MemoryOrder.Release,
     MemoryOrder.Acquire)` is an error. A fence that is `Relaxed` orders nothing, and is an error too.
854. **How the back end writes them** (from the slice): an atomic value is its type in C, written in a
     struct with `_Alignas(sizeof(T))`; a `bool` is worked on as its byte, which the builtins that change
     bits take; a `float` and a `double` go through the forms of the builtins that take addresses, and their
     `fetchAdd` is a loop of `compareExchangeWeak`, since C has no `fetch_add` for them.
855. **`MemoryOrder` is written in SG** (from the slice): an enum of the language's own module
     (`std/atomic.sg`), brought along when a program names it, as `Signal` is.
856. **`: unsafe Sendable`, the details** (from the slice): it lets go the raw pointers of the type and of
     the structs it holds; a borrow, a `shared checked` value and a `Task` still stay. `Sendable` without
     `unsafe` is an error, and so is `unsafe` in front of another interface. The message of the thread rule
     on a raw pointer names it.
857. **`volatile`, the details** (from the slice): a field of a union is read and written as any other
     place (the program is inside `unsafe` already); a field of a `packed` struct that may lie where its
     type is not reached is an error, as for a borrow (809); an atomic place is an error whose hint is `load`
     and `store`; a whole struct is read at once (`Memory.readVolatile(regs[0])`). The value written is
     computed before the place, as for any write (270).
858. **The tests**: `ok_atomics` (every operation on each kind, with and without orders, in a struct,
     behind `new`, in a list and in an array), `ok_atomic_tasks` (counters shared by tasks, a value
     published with `Release` and read with `Acquire`, a loop that spins without `spinLoop`), `ok_atomic_raw`
     (atomic values in raw memory and through a cast, `volatile` on registers and a union, a struct
     `: unsafe Sendable` given to tasks), `panic_atomic_overflow`, `panic_atomic_sub`, and `atomic_wraps` in
     `tests/release`; 38 errors (`error_atomic_*`, `error_fence_*`, `error_volatile_*`,
     `error_sendable_without_unsafe`, `error_unsafe_copyable`, `error_raw_to_task_hint`); `examples/queue`
     with its four tests. `cargo test` passes on macOS, where it compiles 346 of 1409 programs, 77 of them
     under the thread sanitizer, and 23 of 28 programs of tests, and on Linux in a container with the same
     counts. Three messages of earlier tests changed with the slice: the thread rule on a raw pointer names
     `unsafe Sendable`, and the functions of `Memory` listed include the two of `volatile`.
859. **Measured** (`atomics` and `queue`): four tasks adding to two counters five million times each take
     0.274 s against 0.268 s for C, 1.02 times, and 0.256 s in a debug build optimised as the others, so
     the test of overflow after `fetchAdd` costs nothing that shows; on Linux in a container of eight cores
     0.265 s against 0.270 s, 0.98 times. Four producers and four consumers of `examples/queue`'s queue take
     0.475 s against 0.341 s for C, 1.39 times (0.456 s in a debug build), and 1.42 times on Linux, where C
     itself goes from 0.28 to 0.44 s from one run to the next; with two of each it is 1.10 times. The loop
     of a producer compiles to the instructions of C's, one more for the bound of the list, so what is left
     is in "Slower than C".

Choices made in the ninety-first slice (a library built from SG for C to call), the eighth of the slices
that bring SG to what C reaches. 860 to 875 were decided before it on 16 November 2026, with the address of
a value of SG given to C as Rust gives a `Box<T>` (A) rather than as a `void*` taken back with `unsafe`
(B); 876 to 885 come from the slice. All were approved on 17 November 2026:

860. **`sg build folder --library` and `--shared`**, decided before the slice: a static library
     (`libfolder.a`) or a shared one (`libfolder.so`, `libfolder.dylib` on macOS), as Go's `c-archive` and
     `c-shared`; each with `folder.h` beside it. `-o` names the file, `--release` works as for a program,
     and `sg emit-c folder --library` prints the C.
861. **No `main`, at least one `export` function**, decided before the slice: a library with `main` is an
     error whose hint builds it without `--library`; one that exports nothing is an error that shows how one
     is written. Its `test` blocks run with `sg test` as any module's.
862. **Only the `export` functions are seen**, decided before the slice: the runtime of compiled code is one
     unit whose names are all `static`, so the static library shows its `export` functions alone; the shared
     one is built with `-fvisibility=hidden` and they are marked to be seen. Two libraries of SG are linked
     into one program, each with its runtime. A library is built with `-fPIC`, so the static one goes into a
     program that is position independent, and into a shared library. A function of a C file of the
     library (`extern "./native.c"`) is C's to hide: `static`, or seen from the static library.
863. **The header**, decided before the slice: a guard, `stdbool.h`, `stddef.h` and `stdint.h`,
     `extern "C"` for C++, the C types compiled code has (`int` is `int32_t`, `nint` is `int64_t`), each
     plain struct written out with its fields under their names (unions, bit fields, `packed` and `align`
     as in the eighty-ninth slice), each type C holds by its address declared without fields, and each
     pointer to a function written out where it stands (a `typedef` named after the struct and the field
     for one in a plain struct). A type is named as SG names it, without its module: two of one name, a
     type and a function of one name, and a name or a field that is a word of C are errors. A macro
     `FOLDER_API` is `__declspec(dllimport)` for a program that uses the DLL on Windows, and nothing
     elsewhere (`FOLDER_STATIC` for the static library there).
864. **What a program links**, decided before the slice: `sg build` says it (`libcalclib.a -lm -pthread`
     and the libraries of the `extern` blocks, or `-Lbuild -lcalclib`), and so does the comment at the top
     of the header. No `pkg-config` file yet.
865. **Values of SG that C holds**, decided before the slice: an `export` function takes and gives `T*`,
     `T*?`, `read T` and `ref T` of a struct of SG that is not plain data, or of an enum. Giving a `T*` gives
     C the value to keep; taking one back ends it at the end of the call, its `dispose` included; `T*?` takes
     `NULL` too, as C's `free` does; `read T` and `ref T` lend it for the call. No `unsafe` is written:
     what C gives is C's promise, as Rust's `extern "C" fn` takes `Box<T>` and `&T`. C sees
     `typedef struct Calculator Calculator;` and never its fields. `NULL` where none is taken is a panic, in
     the interpreter and in compiled code; the interpreter keeps every address it gave, with its type, and an
     address it did not give, one of another type and one given back already are panics there. The same
     holds for the `export` functions of a program, which a C file of it calls.
866. **A panic aborts in a library**, decided before the slice: its message, then C's `abort`. In a
     program, `exit(1)` stays. Threads of the program that loaded the library may be running, and `exit`
     would run its `atexit` functions under them; Rust aborts at the edge of an `extern "C"` function too.
     `exit(n)` written in SG is still C's `exit`.
867. **Nothing to call first**, decided before the slice: the pool of tasks, the clock, the threads of
     input and output and the signals start on first use under their locks, so C calls an `export`
     function from any thread, several at once. A key of the hash tables is drawn by a constructor of C when
     the library is loaded. `Signals.listen` in a library takes the signals of the program that loaded it.
868. **A shared library is not unloaded**, decided before the slice: the threads of its tasks would run on
     in code that is gone. Linux links it with `-z nodelete`; macOS does not unload a library with variables
     of its threads; Windows will pin it (`GET_MODULE_HANDLE_EX_FLAG_PIN`). Go's `c-shared` is not unloaded
     either.
869. **The report of `shared` values left alive**, decided before the slice: in a debug build of a library
     that makes them, it is written when the process ends (`atexit`), as "the library ended with ..."; the
     exit code is the program's.
870. **`examples/calclib`**, decided before the slice: the calculator of the examples grown into a library,
     with C's program `main.c`. Its module `calc` knows nothing of C: tokens that borrow the text
     (`from source`), a tree of `Expr*`, errors in one enum that is `Formattable`, `Result` and `try`, `match`
     with `or`, ranges and `when`, a `HashMap` of `Func<double, double>`, and four tests. The root file
     `api.sg` is the boundary: a `Calculator : Disposable` and a compiled `Expr` held by C, `read CString`
     texts, a value written to `double *out` through a `raw double`, the message of the last error copied
     into a buffer of C through `Memory.viewRef` and cut as `snprintf` cuts it, a plain struct by value, a
     callback per variable whose name lies in memory from `Memory.allocate`, a hook of C called by
     `dispose`, and a sum worked out by four tasks of `Task.run` and `Task.whenAll`. It is the program of
     the threshold that is a library written in SG and called from C.
871. **The names C sees are C's** (decided before the slice): an `export` function of a library may be named
     as C names functions (`calc_new`); the rule of camelCase gives way here, as for the functions of an
     `extern` block.
872. **The tests**, decided before the slice: `libraries` in `tests/golden.rs` builds `examples/calclib`
     as a static library (debug and release) and as a shared one, compiles `main.c` with `-Wall -Wextra
     -Werror -pedantic` against each and compares what it prints with `main.out`; checks with `nm` that only
     the functions of the header are seen; runs it under the address and undefined sanitizers and looks at
     it for leaks; links a second library of SG with a map and a task into the same program; calls the
     library from four threads of C under the thread sanitizer; sees a panic abort; and gives the checker
     six libraries it must refuse. `handles_misused` gives the interpreter every wrong address, and compiled
     code `NULL`. `tests/modules/handles` runs the same C against the interpreter and compiled code. All of
     it on Linux too.
873. **Measured** (`library`), decided before the slice: a program of C linked against the library built
     from SG and against the same library written in C (885).
874. **Windows**, decided before the slice: not built yet, and kept within reach: the macro of the header,
     a `.dll` with its `.lib`, the pin of 868, the constructor of 867 (Clang's on Windows too), and `SG_EXPORT`
     already `__declspec(dllexport)` there. Added to the list of "Three systems" in 04.
875. **The documents**, decided before the slice: this list, the table at the top, "Not here yet", the
     paragraph on the back end and the row of the measurements; 04, 02 and 06 (with Rust's `staticlib`,
     `cdylib` and cbindgen), the README of the repository and the README of the example.
876. **`read CString` from C** (from the slice): a text C gives an `export` function is a `read CString`,
     C's `const char*`, the same type a function of C is given a text as; `string.fromCString(text)` copies
     it, checked to be UTF-8, with no `unsafe`, since the zero byte at its end is C's promise. `NULL` is a
     panic. Without it, a text came as `raw byte`, which a header writes `uint8_t *`, and a program of C
     could not give it a string constant without a cast.
877. **A plain struct is not held by its address** (from the slice): `T*` of a struct of plain data is an
     error whose hint gives it by value or as a `raw T`. C sees its fields, could make one of its own, and
     give that back to be freed.
878. **A C file of a program sees `void*`** (from the slice): the declarations SG writes before a C file of
     the program take and give a value of SG as `void*` (`const void*` for `read T`), so the C file holds it
     under a type of its own; the header of a library names the types.
879. **`nint` and `nuint` are `int64_t` and `uint64_t` in the wrappers of the interpreter** (from the slice;
     they were `intptr_t` and `uintptr_t`): what a C file of the program is declared is the same in the
     interpreter and in compiled code. On macOS `intptr_t` is `long` and `int64_t` `long long`, of one size
     but two types, so a pointer to a function of C that gives an `int64_t` was refused by one of the two.
880. **The rules of borrowing for what C lends** (from the slice): the interpreter counts the calls in
     progress that a value is lent to. Several `read` borrows at once are allowed (C lends the value again to
     a call it makes inside one); a `ref` borrow and a `T*` taken back while any other is in progress are
     panics that name the call that has it ("a `Tally` that a call of `tally_each` in progress changes").
881. **The name of a library** (from the slice): its folder, or its file without `.sg`. The header is named
     after it and written beside the library, whatever `-o` says; the static library holds one object,
     `name.o`, and is made new each time.
882. **A program run on a library** (from the slice): `sg folder` on a folder with `export` functions and no
     `main` says how a library is built.
883. **The constructor** (from the slice): what `main` did first in a program (drawing the key of the hash
     tables) is done by a function marked `constructor`, with the `atexit` of 869; nothing else needed it.
884. **The tests passed**: `libraries`, `handles_misused`, `tests/modules/handles` (its C file calls the
     functions with values of SG, and one lends a value again from inside a call it is lent to), and
     `examples/calclib` with its four tests in the interpreter and compiled; `error_export_borrow` now hints
     at `read CString`. `cargo test` passes on macOS and on Linux in a container, where it compiles 347 of
     1410 programs, 77 of them under the thread sanitizer, and 25 of 30 programs of tests. The header was
     also compiled by a C++ compiler with `-pedantic`.
885. **Measured** (`library`): fifty million calls of `stats_add(ref Stats, double)` through the address C
     holds, then FNV-1a over 256 MB of C's bytes through `Memory.view`, take 0.506 s against 0.503 s for
     the same program linked against the same library in C, 1.01 times (0.507 s with overflow checked);
     on Linux in a container 0.528 s against 0.530 s, 1.00 times. The test for `NULL` at each call does
     not show. On Linux the run of every measurement stops at `case`, whose output differs from that of C
     there, before it reaches `library`, which was run on its own.

Choices made in the ninety-second slice (a name that hides another), a small slice of the checker alone,
decided on 17 November 2026 during the ninety-first, with C#'s rule (CS0136) rather than Rust's shadowing.
886 to 894 were decided before it and approved on 17 November 2026, with a name free again after its block
has ended (A) rather than refused in the whole block as C# does (B):

886. **Where the rule holds**: every new name of a function or a test, a local declared with `var` or a type,
     the name of a `for`, of a `match` arm and of an `is`, the names of an `is not` seen after the `if` or
     `while` too, may not take the name of a local variable or a parameter (`this` among them) of a block
     around it that is still open. A local of the function's body with the name of a parameter is one: the
     parameters are the scope around the body.
887. **Side by side**: two loops `for i` one after the other, `Ok(value)` in two arms of one `match`, a
     `var label` in both branches of an `if` are not errors; C# allows them too.
888. **After a block has ended** (A): `var total = 0` after a loop whose body had a `total` is not an error,
     since in SG a name's scope starts where it is declared and no line sees both. C# refuses it, its scope
     being the whole block whatever comes first (B).
889. **`_` is not a name**: `for _` inside `for _`, and `_` in patterns, take nothing.
890. **Other names stay as they were**: a local may still have the name of a function, a type, a module or a
     variable of C (which it hides, 724); `Stdin`, `Stdout`, `Stderr` and `this` stay names no local takes.
891. **The error**: ``error: `text` is already a parameter of `echo`, and a new name cannot hide it`` (or "a
     local variable of a block around this one"), pointing at both, with the note that a name means one
     thing in a function, as in C#, and the hint to give the new variable another name. It is checked where
     the name is declared, so it is found in its order among the others.
892. **What it found in the repository**: the `bind` of the language's `UdpSocket` named the address it
     found `address`, the name of its parameter (now `found`); `examples/curl` had the `contentType` of the
     response hide the `contentType` of the request it was given (now `responseType`); a test of
     `examples/shutdown` read the answer into `text`, the name of the text it sent (now `echoed`). Four
     tests that showed the old shadowing (`ok_is_after`, `ok_match`, `ok_range_variable`, `tests_loop_sum`)
     now show a name taken again after its block or side by side, with the same output.
893. **The tests passed**: `error_hides_local`, `error_hides_parameter`, `error_hides_with_for`,
     `error_hides_with_match_arm`, `error_hides_with_is`, `error_hides_after_is`, `tests_error_hides`, and
     `ok_names_side_by_side` for 887 to 889, in the interpreter and compiled.
     `cargo test` passes on macOS: 348 of 1417 programs compile through C, 77 of them under the thread
     sanitizer, and 25 of 31 programs of tests. Linux was not run, since no compiled code changed.
894. **Documents**: the row "A name that hides another" of the decisions, a part of the syntax draft, and a
     row of the Rust mapping (`let x = x.trim()` becomes `var trimmed = x.trim()`). Nothing is measured: no
     compiled code changes.

Choices made in the ninety-third slice (building without an operating system), the ninth of the slices
that reach what C reaches (04, "Without an operating system"). It was written on 17 November 2026 while
the author was away, on their word to go on with the next work, so 895 to 913 were not put to them
before the slice as the others were. All were approved afterwards, the same day:

895. **`sg build folder --freestanding`** gives an object, `folder.o` (`-o` names it otherwise), not a
     program: the code that starts the machine, its linker script and the C compiler's own library are the
     machine's, and are linked with it. `--release` as for a program; `sg emit-c folder --freestanding`
     prints the C. Rust's `#![no_std]` crate is a library for the same reason.
896. **The machine**: `--target riscv64-unknown-elf` (or `aarch64-none-elf`, `x86_64-none-elf`, any triple)
     compiles it with Clang for that machine, which knows them all with one binary; without it, `cc` for the
     processor of this computer. What follows `--` goes to the C compiler as it is written: `-- -mcmodel=medany
     -march=rv64gc`, `-mno-red-zone`, `-mgeneral-regs-only`. A target or `--` without `--freestanding` is
     an error of the command line.
897. **What C promises without a system**: the C is compiled with `-ffreestanding`, and includes only the
     headers C has there (`stdint.h`, `stddef.h`, `stdbool.h`, `stdarg.h`, `limits.h`, `float.h`); nothing of
     C's library is taken for granted. `-fno-stack-protector`, since the guard of the stack calls into C's
     library when it fails.
898. **No `main`, and `export` functions**: `main` is an error whose hint is an `export` function; one at
     least is asked for, as of a library (861). The code that starts the machine calls them, with the
     signatures of C an `export` function has (732).
899. **`onPanic`**: `void onPanic(read string message)` in the root module is asked for. A panic gathers the
     message a program with a system writes to its standard error (the panic, the place, the line, the
     hint) into a buffer of 1024 bytes, cut before a character it would cut in half, and gives it to
     `onPanic` once; the program writes it where the machine shows it. When `onPanic` returns, the
     processor waits for ever (`for (;;) { }`); a panic inside `onPanic`, and one on another core while the
     first is told, wait without being told. Cores that have not panicked go on running: stopping them, and putting
     the processor to sleep instead of spinning (`wfi`, `hlt`), differ from one machine to the next, so they are
     `onPanic`'s, as in Rust. Rust's `#[panic_handler]`, which cannot return; SG has no type for a function
     that never returns, so the waiting is the runtime's.
900. **`allocate` and `release`**: `raw byte allocate(nuint size, nuint align)` and `void release(raw byte
     block, nuint size, nuint align)` in the root module, both or neither, are where every block comes from
     and goes back to: texts, lists, `new`, `shared`, `Memory.allocate`. A block keeps its size and its
     alignment in the 16 bytes in front of it (as many as its alignment when that is more), so `release` is
     told what `allocate` was asked, as Rust's `GlobalAlloc` is told the layout; nothing else is kept. A
     `null` from `allocate` is the panic "there is no memory left: `allocate` gave `null`".
901. **A program without them allocates nothing**: a statement that would is an error there ("this allocates
     memory, and this program has no `allocate`"), with the hint of the two signatures; a call of a function
     of the language's own module that allocates is named. What allocates is read from the C the back end
     wrote: the functions of the runtime and the helpers that call `malloc` or its kin, directly or through
     each other, and the statements that call one. An `assert` on values that are written when it fails
     allocates, and is refused too.
902. **`allocate` and `release` allocate nothing**: a statement in them that allocates, or that calls a
     function of the program that does, is an error; it would call `allocate` again.
903. **What needs a system is an error where the program writes it**: `print`, `printError` and `exit`;
     files, streams and `Stdin`; the network; tasks, `async`, `await`, `Task.delay` and cancellation;
     `locked` values and `lock`; the clock (`Instant.now`, `DateTime.now`); the system's random bytes
     (`SecureRandom`, `Random.fromSystem`); `HashMap` and `HashSet`, whose key is drawn from them; the
     environment, folders, other programs and signals; `errno`; the functions of `<math.h>` (`sqrt`,
     `floor`, `round`, `sin`, `pow` and the others) and `double.parse`, which is C's `strtod`. A type that
     holds one of them in a signature or a field is an error too. A function of the language's own module
     that needs a system is an error at the call the program wrote, saying why ("`Instant.now` needs an
     operating system: it reads the clock of the system"); those of them nothing calls are left out of the
     C. Everything else is there: numbers (with `abs`, `min` and `max` of fractional ones), text and its
     methods, `List`, structs and enums, `match`, `Result`, `T*`, `shared`, `weak`, atomics, raw memory and
     `volatile`, `extern` functions and C files, `export` and `CFunc`, `Duration`, `Random.fromSeed`, `Path`.
904. **The little of C's library the runtime uses is written into the program**: a `vsnprintf` for the
     conversions the runtime and the program write (a test holds it to the C library's for each of them),
     `strlen`, `memchr`, and `frexp` and `ldexp` by the bits of the number; `isinf`, `signbit`, `fabs` and the
     others are the compiler's builtins. The one way left to C's library, `%f` for more than 18 digits after
     the point or a number too large or too small for the exact way (`{x:F20}`, `{1e30:F2}`), is a panic;
     so is `strtod`, which only the refused `double.parse` reaches, but which GCC compiles in a debug build
     even when nothing calls it.
905. **`memcpy`, `memmove`, `memset` and `memcmp`** are defined in the object as weak functions, so that it
     links alone and the machine's own take their place: a C compiler calls them for a copy of a struct
     even without a system, as GCC documents, and Rust's `compiler_builtins` gives them too. They are loops
     of bytes; on macOS `bzero` is one more. The C compiler's own library (`__udivti3` for the division of
     128 bits the runtime does, and on 64-bit ARM GCC's atomics, `__aarch64_swp4_acq_rel`) comes from the
     link, `-lgcc` or compiler-rt's builtins, which `sg build` says.
906. **Machines of 64 bits**: the runtime holds a value of any integer type in `__int128`, which Clang and
     GCC have only for 64-bit targets. A device of 32 bits (Cortex-M, RV32) waits ("Not here yet").
907. **No report of `shared` values left alive**: a program without a system does not end.
908. **The interpreter**: it does not run such a program (`sg folder` says it is built with
     `--freestanding`), and cannot link a root whose `extern` block names functions of the machine; the
     modules that need no machine are tested as any others (`sg test examples/bare/log`).
909. **`examples/bare`**: a program for a machine without a system, run here by a machine of this
     computer. `heap.sg` is an
     allocator over the memory the machine gives, with a `Heap` at its start and the blocks given back in a
     list that a request of the same size takes again; `onPanic` writes to the machine's console and stops
     it. The module `log` keeps a ring of readings in a `List`, tells each against its limits with an enum
     that is `Formattable`, and is tested in the interpreter and compiled. `start` writes with `$"..."` and
     widths, keeps a `new` value, and `readingAt(7)` panics past the end of a list, told through `onPanic`,
     after which the machine stops with 101. `report` counts the blocks: all are given back but one, the
     room the runtime writes values into text with, which it keeps. `machine.c` is its machine for the
     tests: a program of this computer that gives the memory, shows what is written and calls the
     functions, as a kernel's start code would.
910. **The tests**: `tests/freestanding.rs`. `refused`: seventeen programs the build refuses, each with its
     message (`error_*.out`). `built_and_run`: `ok_runtime` (texts and their searches, a sort, a struct and
     an enum written, fractional numbers, `{x:F3}`, `{n:X4}`, widths, the case of Unicode, `shared`, an
     atomic, raw memory, `CString`, `Duration`), whose output is that of the interpreter for the same program
     with `print`, and `ok_panic_stops` (an overflow told with its hint), each debug and release, compiled
     with `-Wall -Wextra -Werror` for this computer and for `aarch64-none-elf`, `x86_64-none-elf` and
     `riscv64-unknown-elf` where Clang knows them, `nm -u` seeing only the functions of the machine and those
     of the compiler's library, then run with `board.c`. `waits_after_a_panic`: an `onPanic` that returns
     and panics again leaves the program waiting. `bare`: `examples/bare` built by `sg build`, debug,
     release and for each target, linked with `machine.c` under `-Wall -Wextra -Werror -pedantic`, and run.
     `formats_as_printf_does`: the `vsnprintf` of 904 against the C library's. `cargo test` passes on
     macOS and on Linux in a container (with GCC there), where 348 of 1417 programs compile through C, 77 of
     them under the thread sanitizer, and 26 of 32 programs of tests, `examples/bare/log` among them. Clang
     19 on Linux also compiled the C of `examples/bare` and of both programs for `riscv64-unknown-elf`,
     `aarch64-none-elf` and `x86_64-none-elf` under `-Werror`, each taking only the machine's functions
     from outside; GCC, for 64-bit ARM, takes its atomics from its library too (905).
911. **Nothing is measured**: the code is the code a program with a system runs; only the loops of 905 are
     new, and a machine gives its own.
912. **Windows**: a program without an operating system has none of Windows either; Clang builds it the same
     way there (826).
913. **The documents**: this list, the table at the top, "Not here yet", the paragraph on the back end; 04,
     02 and 06 (with Rust's `#![no_std]`, `#[panic_handler]` and `#[global_allocator]`), the README of the
     repository and the comments of the example.

Choices made in the ninety-fourth slice (a kernel in QEMU), the tenth and last of the slices that reach what
C reaches (04, "The kernel shown"). 914 to 926 were put to the author before the slice and approved on 17
November 2026; 927 came up while it was written, and was approved afterwards, the same day:

914. **The machine**: QEMU's `virt`, started with no firmware (`-bios none`), so that the kernel runs in
     machine mode from 0x8000_0000 on one core: the start is the simplest, and the serial port and the
     timer are plain. Supervisor mode under OpenSBI, Linux's way, would put calls of SBI between the kernel
     and the timer; it waits ("Not here yet").
915. **`examples/kernel`**: `main.sg` (`kernelMain`, `onTrap`, `onPanic`), `devices.sg` (the serial port,
     the timer and the device that powers off), `heap.sg` (the allocator of `examples/bare` over the memory
     after the kernel), `machine.c`, `kernel.ld` (where each part lies, the stack, the heap and the last page,
     where the tests leave word), `kernel.out` and `run.sh`, which builds, links and boots it.
916. **Only what SG has no words for is C**: `_start` (the cores but the first wait; a stack; the variables
     of C that start at zero cleared; every trap sent to the vector; the unit of fractional numbers turned
     on where the core has one; `kernelMain` called), the vector of traps (the registers a call may change
     kept on the stack, the fractional ones with their state, `onTrap` called, `mret`), and the registers
     of the core and `wfi` as `static inline` functions. It is `extern "./machine.c"`, compiled into the
     same object (84's one door), so a file of assembly is not needed. The devices are SG: their registers
     read and written with `Memory.readVolatile` and `writeVolatile` at addresses made pointers inside
     `unsafe` (`(raw byte)0x1000_0000`).
917. **The state an interrupt reaches**: there are no globals, so `kernelMain` keeps a `Kernel`, with an
     `atomic ulong ticks`, and writes its address into the core's `mscratch` (`Memory.addressOf`); the
     vector hands it to `onTrap(raw void state, ulong cause, ulong pc)`, which lends it again with
     `unsafe (read Kernel)state` and changes only the atomic value. It is 85's way for a callback; Rust's is
     a `static AtomicU64`. A pointer kept by each core fits more cores later, as xv6 keeps its own. Interrupts
     are turned off before the `Kernel` ends.
918. **What it does**: writes a greeting and the size of its heap with `$"..."`, sets the timer for 10 ms
     (`mtime` counts at 10 MHz), turns its interrupt on and sleeps in `wfi`; the loop, not the handler,
     writes `tick 1` to `tick 5` as the count grows, so the serial port is written from one place. Then
     interrupts go off, the blocks of the heap are counted (all given back) and the test device of the
     machine powers it off: QEMU ends with 0. Any other trap is a panic, "unexpected trap: a fault of a load
     at 0x…", which `onPanic` writes to the serial port before it powers the machine off with 101.
919. **The panic is tried too**: QEMU's `loader` device writes 1 into the last page of memory before the
     machine starts; the kernel then reads where no memory and no device is, the core answers with a
     fault, and the panic is told.
920. **The link**: `sg build` gives an object as before (895), and `run.sh` and the test link it with
     `clang -nostdlib -fuse-ld=lld -T kernel.ld`. A debug build divides numbers of 128 bits (the check of a
     loop's sum, `sg_steps`), for which a C compiler calls `__divti3` in its own library, and Clang has
     that library for RISC-V without a system almost nowhere (not in Homebrew's LLVM, not in Debian's
     Clang). So the object has `__udivti3`, `__umodti3`, `__divti3` and `__modti3` itself, weak like
     `memcpy` (905), a bit at a time by shifts of one place; the compiler's own take their place where
     there is one, as with Rust's `compiler_builtins`. `sg build` no longer tells the link to add `-lgcc`
     but for GCC's atomics on 64-bit ARM.
921. **The C compiler for a target**: with `--target`, `sg build` runs the compiler `CC` names, as C's
     builds have it (and Rust's `cc` crate), and `clang` without it. A compiler that fails there is told
     it may not know the machine ("Apple's Clang knows only its own"), not that sg made a mistake.
922. **The tests**: `tests/kernel.rs` builds the kernel debug and release under `-Wall -Wextra -Werror`,
     links it with nothing but its object and boots it in `qemu-system-riscv64` for at most a minute: what
     the serial port shows is `kernel.out`, the same for both, and QEMU ends with 0; asked for a fault, it
     ends with 101 after the panic's lines. A Clang that does not know RISC-V, no LLD or no QEMU fails the
     test with what to install, never a quiet pass. `tests/freestanding.rs` compiles for the targets the
     Clang `CC` names knows, and `divides_as_the_compiler_does` holds the division of 920 to the C
     compiler's for 200,000 pairs of every size and sign and the edges. `cargo test` passes on macOS
     and on Linux, where 348 of 1417 programs compile through C, 77 of them under the thread sanitizer,
     and 26 of 32 programs of tests; on both, `riscv64-unknown-elf` is now among the targets of
     `tests/freestanding.rs`, and the kernel boots.
923. **The machines**: on macOS QEMU came from Homebrew, beside its LLVM and LLD. On Linux the tests run in
     an image made from `tests/Containerfile` (the Rust image with Clang, LLD and QEMU), built once with the
     network and run without it; there the kernel is built by Clang 19 and LLD 19 and booted by QEMU 10.0,
     on macOS by Homebrew's LLVM 23, LLD 22 and QEMU 11.1.
924. **Measured by size**: QEMU's time says nothing of the kernel's, so `bench/run.sh` builds the same kernel
     written in C (`bench/kernel.c`: the same `machine.c`, the devices through `volatile`, numbers written by
     hand, no heap, no checks), both with `-O2` for `rv64gc`, and compares what is loaded. SG's is 5168
     bytes, C's 2236: 2.31 times. The difference is a fixed part of the runtime, not of the kernel: the
     `vsnprintf` that writes `$"..."` (about a kilobyte), the text of a panic with its place and line, the
     check that it is UTF-8, and the heap. It is in "Slower than C".
925. **The interpreter** does not change: it does not run a program built with `--freestanding` (908), and
     every module of the kernel touches the machine.
926. **The threshold is reached**: with the kernel, each of the seven programs of 04's threshold runs and
     is tested (sqlite, libcurl, a server that ends on Ctrl-C, a library of SG for C, the kernel, a queue
     without locks, a binary header read with a union and bit fields). The documents: this list, the table
     at the top, "Not here yet", "Next", "Slower than C"; 04 ("The kernel shown, as built", the threshold
     reached), 02 and 06 (Rust's `riscv-rt` entry, its `#[interrupt]` and `static AtomicU64`), the README of
     the repository and the comments of the example. What comes next is Windows, talked through (718).
927. **A section of its own for each function**: an object built with `--freestanding` is compiled with
     `-ffunction-sections -fdata-sections`, as Rust's are by default, so that a link with `--gc-sections`
     leaves out what nothing calls: the division of 920 in a release build, the weak `memcpy` a machine
     does not call. The kernel's links use it; it took the kernel from 5866 bytes to 5168 (C's from 2252 to
     2236).

Choices made in the ninety-fifth slice (what Windows needs of the language, written on Linux and macOS),
the first of the slices of 04's "Windows, the direction" (930, 932, 934, 940). 943 to 962 were put to the
author before the slice and approved on 17 November 2026, 944 with them:

943. **`CLong` and `CULong` are types like the other integers**: a local, a field, a parameter, a result,
     an item of a list or an array, a key of a map, `raw CLong`. Nothing restricts where they are written;
     with no arithmetic, nobody reaches for them but at the edge of C.
944. **Crossing on their own, by the rule of 02**: a type becomes another without a cast only when the
     other holds every one of its values on every system (`nint` counts as 32 bits). C's `long` has 32 to
     64 bits, so an `int` (or a `short`, a `byte`) becomes a `CLong` on its own, and a `CLong` a `long`; a
     `long` becomes a `CLong`, and a `CLong` an `int`, with a cast. `CULong` the same with `uint` and
     `ulong`. This refines 930, which said a value crosses with a cast, so that no rule is added to the one
     the language has. A sum of a `CLong` and a `long` is a `long`; a `CLong` and an `int` give a
     `CLong`, which has no `+`.
945. **A cast checks this system's size**: `(CLong)n` panics where the value does not fit here (32 bits on
     Windows, 64 elsewhere), `n.tryToCLong()` gives `null` and `n.wrappingToCLong()` cuts it to size; on a
     value that always fits, `tryTo` is an error, as `byte.tryToInt()` is.
946. **A number written for one fits in 32 bits**, on every system, also as the value of a cast or an arm
     of a `match`: `CLong r = 3000000000` is an error on Linux too, with the note that C's `long` is 32
     bits on Windows and the hint to convert a `long` where the program runs. A `match` on one is looked
     at as if it had 64 bits, so that one written to cover it does so everywhere.
947. **What they do**: `==`, `<`, a `match` on numbers, printing with `{r,5}` and `{r:X}` (whose hex has 8
     or 16 digits after the system), `toString`, `compare`, `equals`, `Hashable` and zero in an array.
     `CLong.MinValue` and `CLong.MaxValue` are this system's bounds. No arithmetic (`+`, `-`, `*`, `/`, `%`,
     the bit operators, shifts, `-r`, `~r`, `+=`), no range of them (`for i in 0..r`), no other method
     (`abs`, `max`, `toLittleEndian`), no `atomic CLong` and no bit field of one: each is an error that
     says C's `long` is 32 bits on Windows and 64 elsewhere, with the hint to compute in `long` (`(long)r +
     1`, `r = (CLong)((long)r + 1)`, `((long)r).abs()`, `atomic long`).
948. **What C sees**: `long` and `unsigned long`, in compiled code, in the wrappers of the interpreter, in
     a struct of plain data (8 bytes on Linux and macOS, 4 on Windows, as `raw.rs` lays it out for each
     system) and in the header of a library of SG. `long` in an `extern` block stays `int64_t`.
949. **What was put right**: `examples/curl` gives libcurl its numbers as `CLong` (`curl_global_init`,
     `CURLOPT_FOLLOWLOCATION`, `MAXREDIRS`, `TIMEOUT_MS`, `CONNECTTIMEOUT_MS`, `NOSIGNAL`) and reads the
     status into a `CLong`; the options from 30000 on take a `curl_off_t`, 64 bits everywhere, and stay
     `long`. `examples/zlib` takes zlib's `uLong` as `CULong` everywhere (`compressBound`, `compress2`,
     `uncompress`, `crc32`, the counts of `z_stream`), and tells `deflateInit_` the size of `z_stream` with
     `Memory.sizeOf` (112 bytes on Linux and macOS, 88 on Windows). `ok_extern_libc` (`labs`) and
     `ok_c_errno` (`strtol`) take `CLong`.
950. **`Path.joinInside(folder, name)`** gives a `Result<string, IoError>`: the two joined as `join` joins
     them, or `InvalidInput` for a name that could lead out of the folder on some system.
951. **A name of several parts** (`2026/report.txt`) is taken, each part checked; a `\` is refused on every
     system, since Windows separates with it.
952. **Refused, the same on every system**: an empty name or part (`a//b`, `a/`), a root (`/etc`), a `:`
     (`C:x`, a stream of NTFS), a part that ends with `.` or a space (`.` and `..` among them, and `x.`,
     which Windows would take for `x`), a control character, and a part that Windows takes for a device:
     `CON`, `PRN`, `AUX`, `NUL`, `COM0` to `COM9`, `LPT0` to `LPT9`, the same with `¹`, `²`, `³`, `CONIN$`,
     `CONOUT$`, in any case and with an extension (`nul.txt`, `nul .txt`). Go's `filepath.IsLocal` has the
     same list; `a/../b`, which Go takes, is refused.
953. **What is left to the system**: `<`, `>`, `"`, `|`, `?` and `*` lead nowhere, and Windows refuses them
     when the path is opened.
954. **What it does not see**: a link inside the folder that points out of it is followed when the path
     is opened. Opening by the handle of the folder (Go's `os.Root`, `openat`) is in "Not here yet".
955. **`Path.join` stays as it is**, `..` with it, for the paths a program makes itself; its comment no
     longer says a name from outside stays inside, and points at `joinInside`.
956. **`ExitStatus.Code(int)`** on every system. Linux and macOS give 0 to 255; Windows gives 32 bits, kept
     with `wrappingToInt`, so a code with its highest bit set is negative and printed in hex, as Windows
     writes it: `exited with code 0xC0000005`.
957. **`exit(code)` keeps a `byte`**: the code a program gives itself is the one every system keeps whole.
958. **`C.lastError()` and `C.setLastError(n)`**, without `unsafe`, as `errno`: the system's last error, which
     on Linux and macOS is `errno` itself and on Windows `GetLastError` (with the slice of C on Windows).
     With `--freestanding` both are errors where they are written, as `errno` is.
959. **`IoError.fromErrno(n)`** takes a number of `errno`, and `IoError.fromCode(n)` a code of the system;
     on Linux and macOS they give the same. Rust's `from_raw_os_error` has one for both, since on Windows
     its standard library does not read `errno`.
960. **The one taken for the other is an error**: `IoError.fromCode(C.errno())` and
     `IoError.fromErrno(C.lastError())` would be right on Linux and wrong on Windows, so the checker refuses
     them where the call is written inside the other, with the hint of the right one.
961. **The tests**: `ok_c_long`, `error_c_long_arithmetic`, `_assign_op`, `_literal`, `_cast_literal`,
     `_from_long`, `_method`, `_range`, `_atomic`, `_try_fits`, `_bit_field`, `_match`; `ok_path_join_inside`
     (every refusal of 952 and the names taken); `ok_exit_status`; `error_errno_as_code`,
     `error_last_error_as_errno`, `ok_c_errno` with `C.lastError()`; `tests/freestanding/error_last_error`;
     `tests/modules/cfile` hands a struct with a `CLong` and a `CULong` to its C file, which checks it with
     C's own `sizeof(long)`; `layouts` lays out a struct of them for every system against Clang (4 bytes on
     Windows); `libraries` exports a function of `CLong` and finds `long words_next(long at);` in the header.
     `cargo test` passes on macOS and on Linux in a container, where 351 of 1433 programs compile
     through C, 78 of them under the thread sanitizer, and 26 of 32 programs of tests.
962. **Documents**: this list, the tables at the top, the notes on 574, 602, 725 and 726; 04 ("C's `long`",
     "Paths and files", "Other programs" and "The interpreter and C" as built), 02, 06 (`c_long`,
     `last_os_error`, `from_raw_os_error`, `ExitStatus::code`, a name from outside), the README of the
     repository. Nothing is measured: no arithmetic is added, and a cast is the one every integer has.

Choices made in the ninety-sixth slice (where libraries lie, the names of a library, tests without `fork`,
the count of blocks), the second of the slices of 04's "Windows, the direction" (938, 939, 941), written on
Linux and macOS. 963 to 979 were put to the author before the slice and approved on 17 November 2026:

963. **`-L folder`** is taken by `sg build`, by `sg test` (interpreted and `--compiled`) and by a program the
     interpreter runs, where it comes before the file (`sg -L lib main.sg words...`), since the words after
     the file are the program's. `-L folder` and `-Lfolder` are both taken, as C's compilers take them, and
     several are looked in in the order they are written. `sg emit-c` takes none: it links nothing. In
     `sg build ... -- flags`, what follows `--` is C's, a `-L` too.
964. **A folder that is not there is an error** (`error: -L lib: no such folder`, exit code 2), as is a file.
     A relative folder is read from the working folder and given to the C compiler absolute.
965. **The program looks there when it starts too**: on Linux and macOS each folder is written into the
     program as an `rpath`, absolute, so it runs without `LD_LIBRARY_PATH`; the interpreter's wrappers the
     same. Zig does it; C's compilers and Rust do not. A path relative to the program (`$ORIGIN`) is not
     written. Windows has no `rpath`: where a DLL is looked for is the slice of C on Windows (101).
966. **A folder others may write into is an error**: a library put there would run inside the program that
     looks there when it starts. "Others" are the group and everyone (`/tmp` among them); the hint is
     `chmod go-w`. A path with `$` or `:` is an error too, since the loader reads them as words of its own.
967. **What a program that links a library of SG is told** names the folders: `sg build --library` says
     `libapp.a -L/opt/z/lib -lz -lm -pthread`, and `--freestanding` the same after "with".
968. **`extern "z", "zlib" { }`** names a library by the names it has on the systems, with commas. A block
     names one C file or the names of one library: `extern "./native.c", "z"` is an error, and so is a name
     written twice in a block, or none after a comma. Each name is checked as 519 checks one.
969. **The first one the C compiler finds is linked**, the names asked in the order they are written: an
     empty program is linked with the folders of `-L` and `-l` of the name, and the first that links is the
     one. SG does not look on the disk itself, since only the compiler knows where it looks, and on macOS the
     libraries of the system are stubs in its SDK, not files. The question is asked only for a block with
     several names, once for each name in a run; the interpreter asks it the same way. Without an operating
     system nothing is linked by `sg`, so there the first name is the one said.
970. **When none is found**, the error is at the block: `none of the libraries this extern block names was
     found: z, zlib`, a note with the flags tried and the folders looked in (those of `-L`, then where the C
     compiler looks), and the hint `-L folder`.
971. **A library of one name that is not found** is said the same way, at its block (`the library sgdemo
     was not found`), where the message was "is every library installed where the C compiler looks?". The
     libraries are asked one by one only once the link has failed, so a build that links pays nothing.
972. **`sg test --compiled` without `fork`**: the program of tests starts itself again for each test with
     `--only N` (87), through `posix_spawn` on Linux and macOS (`CreateProcessW` on Windows). The path it
     starts is the one the system gives (`/proc/self/exe`, `_NSGetExecutablePath`), not `argv[0]`, which
     is only the name it was started by, perhaps one found on `PATH`.
973. **What the test sees is what it saw**: its output and its input are the program's, its standard error
     goes to a pipe and is written under its `FAIL` line, and the two ends of the pipe are closed in it as
     it starts, so it holds nothing else of the program's.
974. **The tests still run one after the other.** Each test now starts a program where a `fork` copied one:
     on macOS 200 tests take 0.25 s where they took 0.14 s, on Linux 0.039 s where they took 0.023 s: about
     half a millisecond more for each test on macOS, a tenth of one on Linux. Running them at once is in
     "Not here yet".
975. **A debug build counts the blocks of its values**, on every system: each block the runtime takes for a
     text, a list, a map, a `T*`, a `shared` value or a task adds one to a count, each it gives back takes
     one away, atomically. Not counted: raw memory (`Memory.allocate` and `Memory.free`, since C's blocks
     go back that way too), what C and the program's C files take, the blocks the runtime keeps while a
     thread or the process lives (the block a thread prints values in, the blocks of `Stdout` and
     `Stderr`, the pool of threads, the slots of the network, the clock of delays, a source of
     cancellation the clock holds), and a program without an operating system. A release build has no
     count. Only the number of blocks, not their bytes, which would need a header on each block.
976. **It is looked at where the `shared` values are**: when `main` returns, when a test ends (in its own
     process, against the count when it began), and when a library is unloaded. Not after `exit(code)` or a
     panic, which end the process with its values.
977. **One report**: when `shared` values hold each other up, their report alone, since their blocks are in
     the count too. Otherwise `leak: the program ended with 3 blocks of memory never given back` and the
     note `safe SG gives back every block it takes, so this is a mistake in sg`; a count below zero is
     `gave back 2 blocks of memory more than it took`. The program ends with 1, the test fails.
978. **The tests**: every program `cargo test` compiles in a debug build is run with the count, the
     sanitizers' leak check (LSan, `leaks` on macOS) still after it, and none reports a leak.
     `block_count_sees_a_leak` makes three programs leak on purpose in their C (a block taken, one given
     back that was not taken, a block a test leaves) and fails if the count does not see them, and checks
     that a release build has none. `library_folders` builds a shared library of C in a folder of its own
     and runs a block of two names against it through the interpreter, a built program started from
     another folder without `LD_LIBRARY_PATH`, the tests interpreted and compiled, and the program of tests
     started by a name found on `PATH`; with the errors of a folder that is not there, one others may write
     into, and none found. `ok_extern_names` links C's math library by the second of its names;
     `error_extern_none_found`, `_name_twice`, `_file_and_library`, `_name_after_comma`, and
     `error_extern_no_library` with its new message. `examples/zlib` names zlib `"z", "zlib"`. On Linux no
     check is missing; the line that names a check a system does not have comes with Windows.
     `cargo test` passes on macOS and on Linux in a container, where 352 of 1438 programs compile through
     C, 78 of them under the thread sanitizer, and 26 of 32 programs of tests.
979. **Documents**: this list, the tables at the top, the notes on 82, 84 and 519, "Not here yet", "Next";
     04 ("Tests" and "Libraries" as built), 02 (the names of a library, `-L`), 06 (`#[link(name)]`,
     `rustc-link-search`, `rpath`, the test harness), the README of the repository. Only 974 is measured:
     the speed of a program does not change, the count is in debug builds alone.

Choices made in the ninety-seventh slice (the base of Windows), the first of the slices of Windows in 04's
"Windows, the direction" (929, 931, 932, 935, 937, 938, 939), written on the branch `windows` and tried on a
machine with Windows on 64-bit Arm. 980 to 998 were put to the author before the slice and approved on 17
November 2026; at 980 the author chose that every program may ask, not only the language's own module:

980. **Which system**: `OperatingSystem.isWindows()`, `isLinux()` and `isMacOS()` give a `bool` known when the
     program is built (C#'s `OperatingSystem.IsWindows()`, Rust's `cfg!(windows)`). Both branches of an `if`
     on one are checked on every system; the C of only the branch of this system is written, so the other
     may call what this system has not. A program built `--freestanding` is on none. `Path` and
     `Directory.temp()` are written with them.
981. **What Windows does not run yet is a compile error** where it is written, when the program is built,
     run or tested on Windows: `the network is not on Windows yet: the network and async input and output
     come to Windows in the 99th slice`, with a note that names what this slice brings. A call of a
     function of the language's own module that needs it is the error at the call (`Command.output` needs
     the hundredth slice), found as a program without an operating system finds them (`freestanding.rs`
     holds both rules now, `pending.rs` the one of Windows), the bodies of `test` blocks too. Tasks,
     `locked`, cancellation and async functions wait for the 98th; the network, UDP and the async reads
     and writes for the 99th; `Command`, `Process` and `Signals` for the 100th; `extern`, `export`, `CFunc`,
     `Memory.addressOf` and the variables of C for the 101st. Each slice takes its lines out.
982. **One runtime**: Windows is in the same pieces of C as Linux and macOS, behind `#if defined(_WIN32)` in
     the functions at their bottom (`sg_sys_read`, `sg_sys_open`, `sg_fs_delete`...); `windows.h` comes
     with `WIN32_LEAN_AND_MEAN`. A handle is kept where a descriptor is: Windows gives handles that keep
     their value in 32 bits.
983. **The compiler on Windows**: `src/cc.rs` finds it (`CC`, `clang` on the `PATH`, LLVM's folder, else an
     error that says to install LLVM and the Build Tools) and gives it `-fuse-ld=lld -fms-runtime-lib=static`
     (C's library inside the program), `-g` in a debug build (`x.pdb` beside it), and when it links: 8 MB
     of stack for the main thread (`/STACK`), a manifest (UTF-8 as C's code page, long paths, Windows 10
     and 11) embedded by LLVM's linker, `/MANIFESTUAC:NO`, and `/DEPENDENTLOADFLAG:0x1000`. A program
     is `x.exe` when `-o` names no extension. The tests build with the same.
984. **The console**: what goes to the output or the standard error is written with `WriteConsoleW` when
     that is a console, the UTF-8 turned into UTF-16, a character cut between two writes kept until it
     is whole and a byte that is not UTF-8 shown as U+FFFD; the code page of the console is not changed.
     C's `printf`, `fputs`, `fwrite` and their kin reach `stdout` and `stderr` through functions of the
     runtime (macros on Windows only), so nothing the back end writes changes. Into a file or a pipe the
     bytes go as they are. The interpreter does the same over Rust's stream, which would refuse the
     byte.
985. **The console's input**: read with `ReadConsoleW` and given as UTF-8, a pair of UTF-16 cut between two
     reads kept; Ctrl-Z at the start of a line ends it. Only the reads that wait; one a token can cancel
     comes with the 99th.
986. **The command line and the environment**: the words come from `GetCommandLineW` and
     `CommandLineToArgvW` as UTF-8 (on 1809 as on 11, whatever the manifest says), and a variable from
     `GetEnvironmentVariableW`, its name matched without case. A half of a pair of UTF-16 that stands alone
     becomes the bytes it would have (WTF-8), so it is the error of a text that is not UTF-8, at its place.
     The interpreter reads Rust's `OsStr` the same way (`as_encoded_bytes`).
987. **Files and paths** (932 built): every call is a `W` function; a path of 248 characters or more goes to
     Windows made whole with `\\?\` in front, as Rust does it. A file is opened so that others may read,
     write, rename and delete it; `File.delete` takes the name away at once (POSIX semantics) and deletes a
     file that may only be read, as Linux does; `File.move` is `MoveFileExW` without replacing, `File.copy`
     `CopyFileExW` that fails on a file there; `Directory.deleteAll` holds the handle of each folder it is in,
     so none can be swapped for a link, and removes a link or a junction without following it;
     `Directory.list` gives names in the order of their UTF-8, a junction as a link; `createTemp` makes a
     folder whose list of access has one entry, its owner. The codes of Windows are told apart as Rust tells
     them (`ERROR_SHARING_VIOLATION` stays `Other(32)`); a folder opened as a file is `IsDirectory` and a
     file on the way `NotDirectory`, as on Linux. `Path` takes `/` and `\` and the roots `C:\`, `C:`,
     `\\server\share\`, `\\?\C:\`; `join` keeps a separator the first path ends with and writes `\`
     otherwise, and `Directory.temp()` is the folder Windows gives (`GetTempPath2W`).
988. **The clock and random bytes**: `QueryPerformanceCounter`, `GetSystemTimePreciseAsFileTime`, and
     `ProcessPrng` loaded from the folder of the system once, as Rust loads it.
989. **Memory**: a block aligned beyond 16 bytes comes from `_aligned_malloc` and goes back through
     `_aligned_free`, wherever the back end gives one back (`sg_free_as`: a list, a `T*`, a map, the sort);
     before this slice the sort took the alignment from the size and freed it with `free`, which Windows
     ends the program for. The count of blocks counts them too.
990. **A panic** ends with code 1 as elsewhere; a library's `abort` shows no window and sends no report
     (`_set_abort_behavior`), and the streams are binary from the start (`sg_start`).
991. **`sg test --compiled`** starts itself with `CreateProcessW` and `--only N`, from the path Windows
     gives; the child gets this program's input and output and a pipe for its standard error, and no
     other handle (`PROC_THREAD_ATTRIBUTE_HANDLE_LIST`). A test that ends with no message says its code in
     hex.
992. **Line ends**: `.gitattributes` keeps `\n` in every text file on checkout (`* text=auto eol=lf`), the
     inputs of the tests byte for byte as before; a file of SG with `\r\n` is read as it was.
993. **The tests on Windows** build with Clang as `sg build` does. The address and undefined-behaviour
     sanitizers are tried once on a few lines: where one is missing the output says so (the check of
     undefined behaviour then runs without its runtime, a fault stopping the program; on Windows 11 on
     64-bit Arm with LLVM 23 the address sanitizer is not there, its libraries are not in LLVM's folder, so
     only that check runs: corrected in the ninety-eighth slice, which first wrote both), and leaks are
     looked at by the count of blocks of each debug build, since LeakSanitizer is not on Windows; the line
     says that and that the thread sanitizer is not there either. On every system the programs the tests
     start get the default action of `Interrupt` and `Quit`: a shell that starts `cargo test` in the
     background has both ignored, and an ignored signal stays ignored in every program started after it,
     so `ok_signals_process` failed there (the language's `Command` hands an ignored signal on, as Rust's
     and Go's do; the author kept it so).
994. **An output of its own on Windows**: `name.windows.out` beside `name.out` where Windows prints
     otherwise: `ok_path`, `ok_path_join_inside` and `ok_path_roots` (the separator) and `ok_bit_fields`
     (Microsoft's layout of bit fields). `UPDATE_GOLDEN=1` on Windows writes one only where the output is
     not that of `name.out`. The place of an error is compared with `/` (`--> stock\\items.sg` is shown
     with `\\` on Windows).
995. **`tests/windows-pending.txt`**: the programs and the tests that wait for a later slice, each with the
     slice; a program listed must be refused on Windows with "is not on Windows yet", and one that runs is
     an error until it is taken off. Every slice shortens it; after the last it is empty.
996. **The CI**: `.github/workflows/windows.yml` runs `cargo test --release` on `windows-latest` and
     `windows-11-arm` for every push to `windows`, with `contents: read`, GitHub's `actions/checkout` pinned
     to its commit (`persist-credentials: false`) as the only action, and a time limit; the repository
     already allows only GitHub's own actions, pinned. Decided at the end of the slice: nobody waits for
     it, and the tests on Windows are run whole when a release is near, not after every slice (a run on
     the machine of Windows takes most of an hour); while a slice of Windows is written, a few programs
     are tried there.
997. **The interpreter on Windows**: what it asks the system is in `src/sys/unix.rs` and `src/sys/windows.rs`,
     Windows' functions declared there, with no crate; signals wait in `src/sys/signals_windows.rs`, the
     network and the scheduler's wait behind `cfg(unix)` in `net.rs`, and what a later slice brings is
     unreachable, since the checker refuses it first.
998. **Documents**: this list, the tables at the top, "Not here yet" (821), "Next"; 04 ("Windows, the
     direction" as built, a row for `OperatingSystem`), 02 ("Which system", paths on Windows), 06
     (`cfg!(windows)`, `Path::join` on Windows), the README of the repository. Nothing is measured.
     `cargo test` passes on macOS, on Linux in a container (353 of 1439 programs compile through C, 78
     of them under the thread sanitizer, 26 of 32 programs of tests) and on Windows 11 on 64-bit Arm,
     where 254 programs compile through C and run under the check of undefined behaviour without its
     runtime, the address sanitizer not being there (this said both until the ninety-eighth slice), 12
     programs of tests, and the 124 lines of `tests/windows-pending.txt` wait
     for their slices. New: `ok_path_roots`.

Choices made in the ninety-eighth slice (threads, tasks, locks, atomics and the async scheduler on Windows),
the second of the slices of Windows (935, 937), on the branch `windows`. 999 to 1014 were put to the author
before the slice and approved on 17 November 2026:

999. **One layer for threads**: every lock, condition and thread of the runtime goes through `sg_mtx`,
     `sg_cnd` and `sg_thread_start` (`SG_SYNC`, in a program with threads of its own or the runtime's, for the
     input of another program or for signals); on Linux and macOS each is the pthread call it was, on Windows
     Win32's. The parts of the network and of other programs are written so too, so the next slices only
     add what is theirs. No header of Windows takes pthread's names: a library of C linked in the 101st
     slice may bring its own pthread (winpthreads), and the names would clash.
1000. **A lock is an `SRWLOCK`**, taken only exclusively, as Rust's `Mutex` takes it on Windows: it needs no
     call to be made or freed and is the size of a pointer. It is given back by the thread that took it,
     which a `lock` block, that never waits, makes sure of. `try` is `TryAcquireSRWLockExclusive`.
1001. **A condition is a `CONDITION_VARIABLE`**: its wait with a time (a thread of input and output that has
     had nothing to do for 10 seconds ends) is counted in milliseconds, rounded up. The two ways of Linux and
     macOS to wait on the monotonic clock are one function now (`sg_cnd_wait_ns`).
1002. **A thread** comes from `_beginthreadex`, with 8 MB of stack reserved and taken as it is used
     (`STACK_SIZE_PARAM_IS_A_RESERVATION`), as Linux maps it; a thread nobody waits for has its handle closed
     at once, and `wait` waits for the handle and closes it. When the system refuses one, the panic is the
     one of Linux and macOS, with the error of C's library.
1003. **The pool has a thread for each core** the system has on, in every group of them
     (`GetActiveProcessorCount(ALL_PROCESSOR_GROUPS)`), as `_SC_NPROCESSORS_ONLN` counts them on Linux.
1004. **Smaller things**: a thread that yields calls `SwitchToThread`; the threads of the pool are kept in a
     block of `sg_alloc`, since Windows' C has no `aligned_alloc`; `_Thread_local` is Clang's on Windows too.
1005. **The clock of delays** sleeps on Windows on a timer of high resolution
     (`CREATE_WAITABLE_TIMER_HIGH_RESOLUTION`, from Windows 10 1803) and an event that tells it of a sooner
     delay, both waited for at once; elsewhere on its condition, as before. A condition variable on Windows
     wakes only at a tick of the system's timer, about 16 ms, and `timeBeginPeriod` would change that for
     the whole system. A system that refuses the timer is a panic, as one that refuses a thread. The clock
     reads `Instant`'s clock (`sg_clock`).
1006. **A write to the console holds its stream's lock** (`_lock_file`) on Windows, as C's own writes do: the
     bytes kept of a character cut between two writes belong to one stream, and the line of one task is not
     cut by another's; a panic takes the same locks (`flockfile` elsewhere) and is written alone. Tried by
     hand on Windows 11 on 64-bit Arm through a console (`ssh -tt`, which gives a pseudo console): eight
     tasks wrote 300 lines each with letters of two, three and four bytes, and all 2400 came out whole. The
     same program built without the lock came out whole too: a line of `print` is one write, so that try
     does not reach the bytes the lock guards, and no test sees them; the lock is there by reading the code.
1007. **Atomics and `volatile`** are what they were: an atomic value is at most 8 bytes, which both machines
     change in one instruction, and Clang reads `volatile` as C does, not as Microsoft's compiler can.
1008. **The scheduler** of the eighty-first to eighty-third slices is the same on Windows: the queues of the
     threads, the shared queue, the threads asleep. Its wait in the loop of the network comes in the 99th.
1009. **On Windows now**: `Task.run` and `wait`, `locked` and `lock`, `atomic`, async functions and `await`,
     `Task.delay`, cancellation, `Task.whenAll` and `Task.whenAny`, lists of tasks. A cancel that stops a
     wait for the console, the network or another program comes with those.
1010. **`tests/windows-pending.txt`** has no line of the 98th: 43 programs of its block run on Windows; nine
     wait for the 99th (`File.readAsync` and the async streams, as 04 orders them) and two for the 100th, each
     moved to the slice the first error names.
1011. **No thread sanitizer on Windows** (935): there a program on several threads runs under the sanitizers
     the machine has (993), and the tests say how many ran without the thread sanitizer
     (`thread sanitizer: not on Windows; N program(s) on several threads ran without it`).
     `compiled_small_pools` runs on every system now, with the compiler `sg build` uses.
1012. **The interpreter** needed nothing: its tasks are Rust's threads and its clock its own.
1013. **Tests**: `short_delays_are_short` (every system): fifty delays of 1 ms end within half a second and
     each waits its millisecond. On Windows 11 on 64-bit Arm they take about 97 ms; with a timer of the
     usual resolution about 875 ms, which the test does not let pass. The programs that came off the list
     found two things on Windows. The block of a task whose arguments are aligned beyond 16 bytes
     (`Task.run(bump, slot)` with an `align(64)` struct, `ok_align`) came from `_aligned_malloc` and went
     back through `free`, which Windows ends the program for: the type the task is freed as does not hold
     its arguments, so a task now keeps how its block was taken (`sg_task_free`). And the names of
     functions a task can be started through (`sg_func_name`) listed functions of the language's own
     module that are left out of the C on Windows (`ok_exit_status`, `ok_std_value_dropped`); they are left
     out there too.
1014. **Documents**: this list, "Not here yet", "Next"; 04 ("The runtime" and "Tests" as built), 06 (tasks and
     delays on Windows), the README of the repository. Nothing is measured. `cargo test` passes on macOS,
     on Linux in a container (353 of 1439 programs compile through C, 78 of them under the thread
     sanitizer, 26 of 32 programs of tests) and on Windows 11 on 64-bit Arm, where 293 programs compile
     through C and run under the check of undefined behaviour (the address sanitizer is not there), 41 of
     them on several threads without the thread sanitizer, and 80 lines of `tests/windows-pending.txt`
     wait for their slices. New: `short_delays_are_short`.

Choices made in the ninety-ninth slice (the network and async input and output on Windows), the third of the
slices of Windows (936), on the branch `windows`. 1015 to 1031 were put to the author before the slice and
approved on 17 November 2026:

1015. **The loop asks AFD**, the driver under Windows' sockets, as mio and Tokio do: a question
     (`IOCTL_AFD_POLL`) on a handle of `\Device\Afd`, answered on an I/O completion port, which is the loop
     a thread of the pool with nothing to do waits on (`GetQueuedCompletionStatusEx`). The slots of the
     sockets are those of Linux and macOS: a task tries its call, and waits in the slot when the socket is
     not ready. The driver answers a question once, where epoll and kqueue keep a socket on: a question is
     asked when a task waits for a way the question out does not ask, and asked again, of what tasks still
     wait for, when its answer has come; one that asks more calls the one out back. It is answered at once
     when the socket is ready now, so nothing that came before the question is missed. The functions of
     the driver are ntdll's, taken by name when the loop opens (`GetProcAddress`), so nothing more is
     linked than `ws2_32`.
1016. **A question is a block of its own**, let go of when its answer comes, so an answer that comes after
     its socket was closed, and its number given to another, is let go of and reaches no one (on Linux and
     macOS `gen` keeps them apart). A question's block is taken outside the count of blocks (975): one may
     still be out when the program ends. Each 32 numbers of sockets share a handle of the driver, as mio
     shares one, so a call back does not cost more as sockets are added.
1017. **The loop is woken** by a packet posted to the port (`PostQueuedCompletionStatus`), where Linux and
     macOS write into a pipe.
1018. **A socket is a handle**, kept in 32 bits as a file is (97); its slot is its number over 4, since a
     handle is a multiple of 4. It is asked of through its base handle (`SIO_BASE_HANDLE`), under any layer
     a program put over Winsock.
1019. **A socket** comes from `WSASocketW`, overlapped and not handed to programs this one starts
     (`WSA_FLAG_NO_HANDLE_INHERIT`); one that `accept` gives has its handle set so too, and blocks or not as
     the call asked. Winsock is started by the first call that needs it, as Rust starts it, and never
     stopped.
1020. **A listener and a datagram socket hold their port alone** (`SO_EXCLUSIVEADDRUSE`), never
     `SO_REUSEADDR`: without it, while one has the port on every address of the machine (`":8080"`), a
     socket of another program of the same user that asks to share the port binds it on `127.0.0.1` and
     receives what is sent there. Measured on Windows 11 on 64-bit Arm, with each pair of addresses: that
     is the one case it changes; one bound to `127.0.0.1` is not taken on `127.0.0.1` either way. Rust's
     `TcpListener` and `UdpSocket` set neither.
1021. **A datagram that came back** ("port unreachable") is told as Linux and macOS tell it: a socket that is
     not connected is not told (`SIO_UDP_CONNRESET` and `SIO_UDP_NETRESET` off when it is bound; without
     that, its next receive fails, whoever the datagram was for), and a connected one is told, its reset a
     `ConnectionRefused`. Rust leaves Windows' way; libuv and Go turn it off.
1022. **A call that is not async** is one call of a blocking socket, as elsewhere; a socket that an async call
     made non-blocking is waited for with `WSAPoll`. Its fault, a failed connect it does not tell of, is not
     met: a connect that is not async is made on a blocking socket.
1023. **A name is looked up with `GetAddrInfoW`**, its UTF-8 turned into UTF-16, where Rust's `getaddrinfo`
     goes through the code page of the machine; on a thread of input and output when async, as elsewhere.
     The interpreter looks up with it too.
1024. **The codes of Winsock** are the variants of `IoError` they are on Linux and macOS, through the table
     of 988; `ok_net_errors` prints the same on every system.
1025. **Windows tells of a refused connection after about two seconds**: it tries again twice after the
     other side refused, as it does for Rust's and Go's programs. Measured on Windows 11 on 64-bit Arm:
     2.02 s to a port nobody listens on, and 2.05 s to `localhost`, whose `::1` is tried first while the
     listener is on `127.0.0.1`. It is left as Windows has it; a test that connects to `localhost` is two
     seconds slower there.
1026. **Files and the async streams** are read and written on the threads of input and output, as on Linux
     and macOS (Tokio does not take files to the completion port either); only the calls under them are
     Windows' (`sg_sys_read`, `GetFileType`).
1027. **A cancel of a wait for the input** ends the wait at once, as elsewhere: the thread that reads goes on
     reading (`ReadConsoleW` or `ReadFile`), and what it reads stays for the next read. No
     `CancelSynchronousIo`, which could lose a character the console has given. The program ends with the
     thread still reading.
1028. **The interpreter**: std's sockets, waited for with `WSAPoll`; its scheduler is woken through a datagram
     socket of `127.0.0.1` connected to itself, where Linux and macOS have a pair of sockets of the machine;
     a listener and a datagram socket are made with `WSASocketW` and handed to std, holding their port
     alone (1020) and told as 1021 says. Winsock is declared in `src/sys/net_windows.rs`, no crate.
1029. **`tests/windows-pending.txt`** has no line of the 99th: 19 programs and three tests of the harness
     (`compiled_input_waits_alone`, `compiled_input_cancel`, `interpreted_input_cancel`) run on Windows, and
     four that also start other programs or listen for signals (`ok_process_async`,
     `ok_process_descriptors`, `ok_process_talk`, `examples/shutdown`) moved to the 100th. A field of the
     runtime named `far` is `away` now: Windows' headers take `far` as a word of their own.
1030. **Tests**: `ok_udp_unreachable` (every system): an unconnected socket that sent to a port nobody listens
     on receives the next datagram, and a connected one is refused; without 1021 Windows gives the first a
     reset (`WSAECONNRESET`, tried in C on the machine). `ports_are_held_alone` (Windows): a socket that asks
     to share the port, on `127.0.0.1`, of a listener or a datagram socket bound to every address,
     interpreted and compiled, is refused, while it takes the port of such a socket of std. Its first
     version bound both to `127.0.0.1`, where Windows refuses the second either way, and its check of std
     failed: so the case was measured (1020).
1031. **Documents**: this list, the network in the table above, "Not here yet", "Next"; 04 ("The runtime" as
     built), 02 (the network), 06 (the sockets on Windows), the README of the repository. Nothing is measured but
     1020 and 1025. `cargo test` passes on macOS, on Linux in a container (354 of 1440 programs compile
     through C, 79 of them under the thread sanitizer, 26 of 32 programs of tests) and on Windows 11 on 64-bit
     Arm, where 313 programs compile through C and run under the check of undefined behaviour, 60 of them
     on several threads without the thread sanitizer, 16 programs of tests, and 58 lines of
     `tests/windows-pending.txt` wait for their slices. New: `ok_udp_unreachable`, `ports_are_held_alone`.

Choices made in the hundredth slice (other programs and signals on Windows), the fourth of the slices of
Windows (933, 934), on the branch `windows`. 1032 to 1049 were put to the author before the slice and
approved on 17 November 2026:

1032. **A program is looked for** as on Linux and macOS, with what Windows adds: a name without `/`, `\` or
     a drive is looked for in the folders of `PATH`, apart at `;` (a folder in quotes is taken whole, as
     Rust and Go read it), as `name.exe` when it has no extension (a path without one gets `.exe` too,
     as Rust gives it, since `CreateProcessW` adds none); only absolute folders are looked in
     (`C:\tools`, `\\server\share`, not `\tools` or `C:tools`), and never the folder the program works
     in or the folder of the system unless `PATH` names it. Rust also looks in the folder of the program
     and in the folders of the system; Go tries every extension of `PATHEXT`. Written in SG
     (`Command.found`, `pathFolders`, `isAbsolute`).
1033. **The command line** is written so that C's library reads the words back as they were given, by
     Rust's rules: a word is quoted when it is empty or holds a space or a tab, a quote after `n`
     backslashes is written after `2n+1`, and the backslashes before a closing quote are doubled. The name
     is quoted and not escaped; one with a quote in it is an `InvalidInput`.
1034. **A batch file** (`.bat`, `.cmd`, in any case, the path made whole first so that a dot or a space
     at its end does not hide the extension) runs only when named with its extension, through `cmd.exe`
     from the folder of the system (`cmd.exe /e:ON /v:OFF /d /c ""script" args"`), each argument escaped as
     Rust escapes it since CVE-2024-24576: quoted unless every character is plain, a quote doubled, `%`
     written `%%cd:~,%` so that no variable takes its place. A `\r` or a `\n` in an argument, which would
     end the command, is an `InvalidInput`, as a script name with a quote or a `\` at its end is.
1035. **Pipes** are named pipes, as Rust makes them: a new name of this program each time, made as the
     first instance of that name, so that one made by someone else is never joined, and refused to other
     machines. The end of the other program is synchronous, as programs expect; the end here is
     overlapped, kept with its lowest bit set (no handle has it), and every read and write of it waits for
     its own call. An async read of it is made on a thread of input and output into the free end of the
     reader's block; a cancel ends the wait at once and cancels the call (`CancelIoEx`), and what came
     before stays in the block for the next read: so the call is never left to read on, and nothing is
     lost. This is where the slice left the direction of 934, which named `CancelSynchronousIo`: on a
     synchronous handle its cancel can race the call it is meant for. `run` with an input and its output
     both kept writes and reads at once, two overlapped calls waited for together.
1036. **What the program inherits** is its three streams and nothing else: each is a copy made inheritable
     for the call and listed for it (`PROC_THREAD_ATTRIBUTE_HANDLE_LIST`), closed after it; an input of
     nothing is `NUL`. The program shares this program's console, and is not put in a group of its own,
     so Ctrl-C at the console reaches both, as at a terminal of Linux and macOS.
1037. **The environment** goes as a block of UTF-16, sorted as Windows wants it; names are matched without
     case (`Path` and `PATH` are one). A cleared environment gets `SystemRoot` back when no change gave
     it, as Go gives it (Rust does not): without it Winsock does not start in the program.
1038. **A program is named by its handle**, which `Process` keeps beside its number (`handle`; on Linux
     and macOS the number again), so no other program can take its name before it is waited for. `wait`
     waits for the handle, takes the code (32 bits) and closes it; `ExitStatus.Signal` never comes on
     Windows. A folder to work in that is not there is `NotFound` and a file `NotDirectory`, as elsewhere,
     where Windows says the same of both.
1039. **`kill`, `terminate` and `signal`** end the program with `TerminateProcess` and the code 1 (933);
     `Signal.WindowChange` does nothing. Once the program has ended, nothing.
1040. **`waitAsync`** waits on the pool of threads of the system (`RegisterWaitForSingleObject`, once, on a
     handle of its own), as Tokio and libuv wait; its call back wakes the task, so no thread is held for
     each program. A cancel wakes the task at once and leaves the wait to its end, when it lets itself go.
     The interpreter waits on a thread, as on Linux and macOS.
1041. **`System.executable`** is 1 for a file and 0 for none or a folder: Windows has no right to run a
     file apart from the right to read it, and starting it says the rest (`PermissionDenied`, or
     `Other(193)` for a file that is not a program).
1042. **Signals** come through the handler of the console (`SetConsoleCtrlHandler`), set by the first
     listener: Ctrl-C and Ctrl-Break are `Interrupt`; the console closed, the user logging off and the
     system shutting down are `Terminate`. Windows runs the handler on a thread of its own, so it hands the
     signal on itself, with no pipe and no relay. With no listener of the kind it lets Windows do what it
     does by default. After a `Terminate` it does not return, since Windows ends the program once it does:
     the program ends in its own way, within the few seconds Windows gives it.
1043. **The command line is read by the runtime**, by the rules of 1033 read the other way (Rust's reader),
     not by `CommandLineToArgvW`: its library, `shell32`, brings `user32` along, and a console program
     that has `user32` is not told of the user logging off or of the system shutting down. `shell32` is no
     longer linked. `ok_process_words` sends seventeen words through a command line and reads them back.
1044. **`Signals.raise` on Windows** hands the signal to the listeners of its kind, as elsewhere; with none,
     the program ends as Ctrl-C ends it (`0xC000013A`, `STATUS_CONTROL_C_EXIT`), what was printed written
     out first, and `WindowChange` does nothing.
1045. **`sg-test-tool`** (`tests/tool/main.sg`) is the program the tests of other programs start, on every
     system, in place of `sh`, `cat`, `sort` and their kin, which Windows does not have: each word of its
     command line is a step (`out=TEXT`, `exit=N`, `cat`, `sort`, `head=N`, `sleep=MS`, `env=NAME`, `pwd`,
     `args`, `listen` and more). The harness builds it once with `sg build` and puts its folder first on
     `PATH`; a program that starts other programs or listens for signals runs in a process of its own,
     interpreted too. The tests of other programs and signals were written again on it; what differs on
     Windows is in `.windows.out` (a program stopped ends with the code 1; a cleared environment has
     `SystemRoot`; a signal raised with no listener ends with `0xC000013A`). `ok_process_descriptors` also
     checks, on every system, that a program started later does not hold the end of a pipe of another
     one (else `cat` would not see the end of its input); its shell that tries each descriptor from 3 up
     runs on Linux and macOS only.
1046. **Tests of Windows** in the harness: `process_path_lookup` on Windows (`.`, an empty part, a relative
     folder and `\folder` not looked in, a quoted folder with `;` looked in, `.exe` added, a batch file with
     `%PATH%`, quotes and an empty word, one named without its extension not found, a line break refused, a
     code `0xC0000005` in hex), interpreted and compiled; `shutdown_on_signals` on Windows: the server runs
     with a console of its own, and a small program of C (`tests/sender.c`) attaches to it and raises
     Ctrl-C (twice Ctrl-Break to cut a connection), interpreted and compiled; for a closed console it
     runs the server in a pseudo console of its own and closes that, as Windows Terminal closes a tab: a
     closed console is `Terminate`, and the server ends cleanly with 0 within the time Windows gives.
     The close was put to be tried by hand; it runs in the harness instead. It first posted `WM_CLOSE`
     to the window of a console made with `CREATE_NEW_CONSOLE`, which on GitHub's machine of x86-64 is
     only a stand-in (`PseudoConsoleWindow`, the console going to a terminal of its own) that closes
     nothing; a pseudo console is closed the same way on every machine, with the same
     `CTRL_CLOSE_EVENT` (18 November 2026). On the machine the tests ran over `ssh`, whose
     programs start with Ctrl-C ignored, which Windows hands on: a listener of `Interrupt` turns that off
     (`SetConsoleCtrlHandler(NULL, FALSE)`), as a listener on Linux and macOS takes the place of an
     ignored signal. Unlike there, the ignoring does not come back when the last listener ends: Windows
     does not tell whether it was on.
1047. **The interpreter** starts programs through std's `Command` with the path SG found: std writes the
     command line and escapes a batch file as 1033 and 1034 do (its name is the path, not the name given),
     makes no handle inheritable, and reads an overlapped pipe through `File`, waiting for its call. A
     cleared environment gets `SystemRoot` there too. Signals are `src/sys/signals_windows.rs`, the
     functions of Windows declared there, no crate. A read of a pipe there is not cut short by a cancel,
     on Linux and macOS either: `outputAsync` cancelled waits for the program to end its output.
1048. **`tests/windows-pending.txt`** has no line of the 100th: sixteen programs and two tests of the harness
     run on Windows.
1049. **Documents**: this list, other programs and signals in the table above, "Not here yet", "Next"; 04
     ("Signals", "Other programs" and "The runtime" as built), 02, 06, the README of the repository.
     Nothing is measured (102). `cargo test` passes on macOS and on Linux in a container (355 of 1441
     programs compile through C, 80 of them under the thread sanitizer, 26 of 32 programs of tests). On
     Windows 11 on 64-bit Arm the programs of this slice ran interpreted and compiled, and `golden`,
     `modules`, `compiled_tests`, `compiled_small_pools`, `process_path_lookup` and `shutdown_on_signals`
     pass; the whole of `cargo test` on Windows is the CI's from this slice on, so that a slice does not
     wait for the machine. `tests/windows-pending.txt` keeps 41 lines, of C's slice and the last. New:
     `tests/tool`, `ok_process_words`, five `.windows.out`.

Choices made in the hundred-and-first slice (C on Windows), the fifth of the slices of Windows (940, 941),
on the branch `windows`. 1050 to 1067 were put to the author before the slice and approved on 17 November
2026:

1050. **The interpreter's wrappers on Windows** are a DLL, built by the C compiler of compiled code (Clang
     with LLVM's linker), with every function the interpreter looks up exported (`__declspec(dllexport)`).
     The DLL takes C's library as a DLL (`/MD`, the universal C runtime), the one the interpreter itself
     is linked with: memory C gives with `malloc` goes back with `Memory.free`, and the other way round, and
     `errno` is one. A program built by `sg build` keeps C's library inside it (`/MT`, 929): each DLL it
     loads has a heap and an `errno` of its own.
1051. **Where a DLL is looked for** by the interpreter: in the folder of the program's first file, in the
     folders of `-L` (`AddDllDirectory`) and in `System32`, the library of wrappers loaded by its full path
     (`LoadLibraryExW` with `LOAD_LIBRARY_SEARCH_USER_DIRS | LOAD_LIBRARY_SEARCH_SYSTEM32`); never in the
     folder it works in or on `PATH` (940).
1052. **The folder of the wrappers**, which only its user may open, cannot be removed on Windows while its
     DLL is loaded. It is named after the process (`sg-ffi-<pid>-...`), stays until the process ends, and
     the next interpreter that opens a library removes the folders of processes that have ended; one of a
     process that runs is left alone, since its DLL may be about to be loaded (the tests run many at once).
1053. **The libraries of the examples on Windows** come from vcpkg as DLLs with their import libraries (its
     triplets `x64-windows` and `arm64-windows`). Built into the program instead (`x64-windows-static`),
     they would take C's library of `/MT` into the interpreter's DLL of `/MD`, and libcurl would need the
     libraries of the system it calls named by hand, which Windows, without `pkg-config`, does not tell. A
     program of `/MT` uses a DLL of `/MD` as 929 says: memory goes back through a function of the one that
     gave it.
1054. **A program finds its DLLs beside itself**: Windows has no `rpath`, and a program looks in its own folder
     and `System32` (929). So `sg build` (a program, `--test`, `--shared`) copies beside what it builds each
     DLL it linked from a folder of `-L`, and each DLL that one needs from those folders (libcurl needs
     zlib); the name of a DLL is read from its import library (`zlib.lib` names `zlib1.dll`) and from the
     table of imports of each DLL copied, in Rust, with no tool. A DLL already there and the same is not
     copied again; one an import library names that no folder holds is a note. vcpkg keeps the import
     libraries in `lib` and the DLLs in `bin`: both are given with `-L`. `sg test --compiled` builds in a
     folder of its own on Windows, so that the DLLs land beside its program and go with it.
1055. **`-L` on Windows** takes a path with `:`, and is not checked for whether others may write into the
     folder: nothing of it is written into the program, which is why Linux and macOS check it (their
     `rpath`). The path is made absolute without `\\?\`, which C's compiler and the messages would carry.
1056. **`extern "m"` and `extern "c"`** name C's library and its mathematics, which on Windows are C's library
     itself: nothing is linked for them, and `extern "nosuchlib", "m"` takes `m`. A message that says where a
     library was looked for names the C compiler that looked (`cc`, or `clang` on Windows).
1057. **The libraries of the system** the runtime calls (`kernel32`, `advapi32`, `ws2_32`, `bcrypt`, `ntdll`)
     are named by the C itself (`#pragma comment(lib, ...)`), in a program, in the wrappers and in a static
     library built from SG, which so brings them to the program that links it; an `extern` block without
     a library finds a function of them (`inet_ntoa`) as one of C's library. With them goes
     `legacy_stdio_definitions`: Microsoft's C library has `printf`, `snprintf`, `sscanf` and their kin only
     in its headers, inline, and a declaration in an `extern` block needs them as functions.
1058. **The system's last error** is kept by every call of C as `errno` is, on Windows apart from it: a wrapper
     of the interpreter puts both in place before the call and takes both after it, and `C.lastError()` and
     `C.setLastError(n)` are `GetLastError` and `SetLastError` in compiled code. A DLL with C's library of
     its own (`/MD`) sets an `errno` that a program of `/MT` does not see in `C.errno()`; the code of
     `GetLastError` is the system's, and seen by all.
1059. **An `if` on the operating system** (`OperatingSystem.isWindows()`) leaves the functions of C of the other
     branch out of the interpreter's wrappers, as compiled code leaves their C out, so that one file calls
     `symlink` on Linux and macOS and `CreateSymbolicLinkA` on Windows. The checker still reads both.
1060. **Tests that call what only POSIX has** take a branch for Windows: `ok_directory` makes its link with
     `CreateSymbolicLinkA` (with the flag that needs no administrator where Windows allows it), `ok_environment`
     sets its variables in UTF-16 (a lone surrogate is what is not text there, `InvalidText(1)` as on Linux),
     `ok_c_call_tasks` waits in `Sleep`, `ok_c_callbacks` takes `abs` from `ucrtbase.dll` with
     `GetProcAddress` (another copy than the program's, so `f == g` is `false` there), `ok_files` sets the
     only mode Windows has, read-only, and `ok_process_run` runs a batch file. `ok_process_run` starts
     `sg-test-tool` in place of `sh`, `echo`, `true` and `false` on every system; a program stopped by a
     signal and a file that may not run until its mode says so stay on Linux and macOS. `getopt` is not in
     Windows' C library: `ok_c_getopt` says so there, and so does `tests/modules/cfile` of `opterr`, the
     variable of the C library it uses, in an `if` on `OperatingSystem.isWindows()` so that it is not linked
     there (left out at first, which did not link on Windows); the variables of C are read and written on
     every system by `tests/modules/cvariables`, from a C file of its own.
1061. **Types of C's `long` in the tests** made right: `ldiv` and `strtol` take and give `CLong`, zlib's `crc32`
     `CULong`; `ok_c_varargs` writes a `long` with `%lld`, C's `long long`, and takes the way Windows prints
     `NULL` with `%p`. zlib is named `extern "z", "zlib"` and libcurl `extern "curl", "libcurl"`, as vcpkg
     names them.
1062. **A library built from SG on Windows**: `--library` gives `x.lib`, made by LLVM's `llvm-lib` (beside the C
     compiler, or on `PATH`), and a program that links it defines `X_STATIC` before the header;
     `--shared` gives `x.dll` with `x.dll.lib`, its import library, beside it, and the DLL holds itself
     loaded until the process ends (`GET_MODULE_HANDLE_EX_FLAG_PIN`, 874). The tests of the harness run on
     Windows too (`libraries`, `library_folders`, `handles_misused`, `callbacks_end_the_process`), their C
     made with threads of Windows where it had pthread's. A program of C prints with `\r\n` there, its
     standard output being text, and so does the message of a panic inside a library, which goes through
     C's `stderr` of the program that loaded it; a library does not change how that program writes. A panic
     in a library ends the process with `0xC0000409`, C's library ending it at once (`__fastfail`), as a
     panic of Rust that aborts does. Microsoft's headers define `printf` and its kin inline, so a static
     library that calls one carries it besides its `export` functions.
1063. **The interpreter carries the manifest** of a compiled program (`src/sg.manifest`, embedded by
     `build.rs` with the linker's own flags, no crate): C it calls takes a path as UTF-8, as compiled code
     does, and a path may be long.
1064. **The CI of Windows** installs zlib, SQLite and curl with vcpkg (the one of GitHub's image where it is
     there, else vcpkg at a fixed commit) and keeps what it built in GitHub's cache (`actions/cache`,
     pinned to its commit, the second action the repository allows); the tests take the folders from
     `SG_TEST_LIB_DIRS` and give them to the interpreter as `-L` and to the C compiler, copying the DLLs
     beside each program as `sg build` does. A library missing there fails its tests.
1065. **`examples/bare` and the kernel** are tested on Windows in the last slice, with the rest of what is
     built without an operating system (`tests/freestanding.rs`, `tests/kernel.rs`, which the list of what
     waits holds whole); nothing of this slice changes them.
1066. **What Windows did not run yet** is no longer a part of the checker (`src/pending.rs` is gone):
     `tests/windows-pending.txt` keeps only the three files of tests of the last slice.
1067. **Documents**: this list, "Calling C", "A library of SG for C" and "Not here yet" above and below; 04
     ("The machines and the compiler", "The interpreter and C" and "Libraries" as built), 06, the READMEs
     of the examples, the README of the repository. Nothing is measured (102). `cargo test` passes on macOS
     and on Linux in a container (356 of 1442 programs compile through C, 80 of them under the thread
     sanitizer, 27 of 33 programs of tests). On Windows 11 on 64-bit Arm `golden`, `modules`, `libraries`,
     `library_folders`, `handles_misused` and `callbacks_end_the_process` pass, the programs of zlib,
     SQLite and curl left to the CI, which has their libraries; the whole of `cargo test` on Windows is the
     CI's. New: `tests/modules/cvariables`, five `.windows.out`.

Choices made in the hundred-and-second slice (everything tested on Windows, measured against C), the last of
the slices of Windows (942), on the branch `windows`. 1068 to 1079 were put to the author before the slice
and approved on 17 November 2026; what the slice found besides is said with the choice it belongs to:

1068. **Without an operating system on Windows**: `sg build --freestanding` without `--target` gives an object
     of this machine (COFF), as elsewhere, for a machine of the tests (`board.c`). Clang writes no library of
     C into such an object unless it is told which C library a program has, which `sg build` does not tell
     it there, so no flag is needed (`-fms-omit-default-lib` was tried and is unused by Clang's own driver);
     the test holds every object of Windows to having no `/DEFAULTLIB`. The runtime's own
     `#pragma comment(lib)` lines and `__declspec(dllexport)` are for programs and libraries with a system
     and are left out of such an object. Clang 23's `<float.h>` defines `NAN` and `INFINITY`: the runtime
     without a system defines its own after taking those away. With `--target` on Windows, the Clang is the
     one `sg build` finds (`CC`, `clang`, LLVM's folder).
1069. **What Clang asks for on its own** on Windows: `__chkstk` (a frame of more than a page) and `_fltused`
     (a program with fractional numbers) are names of the C compiler, not of C's library; the test of what
     an object takes from outside allows them, as it allows every name that starts with `__`.
1070. **Division of 128 bits**: the test of the runtime's own `__divti3` and its kin compares them on
     Windows with Clang's library of its own (`clang_rt.builtins`), which Windows does not link by itself.
     Found by it: a **debug** build on Windows did not link when a loop added up a sum (`sg_steps`) or a
     number was written with digits after the point (`{x:F2}`, `sg_fixed`), since without optimisation the
     C compiler calls its library for a division of 128 bits, and on Windows that library is not linked; the
     tests compile with `-O1`, where the division by a constant is gone. Both now divide in 64 bits (the
     numbers fit there; `sg_tenth` takes a tenth of 128 bits in steps of 32). A new test,
     `needs_nothing_of_128_bits`, compiles every program of one file without optimisation into an object and
     fails on any name of that library (`__udivti3`, `__multi3`, `__muloti4` and their kin), on every system,
     so that what would break Windows is seen where the code is written; it found ten programs before the
     change.
1071. **The tests' tools on Windows**: the C compiler `sg build` finds (`sg::cc::compiler`) instead of `cc`,
     LLVM's `llvm-nm` and `llvm-readobj` beside Clang instead of `nm`, a file instead of `/dev/null`, and the
     line ends and the `\` of a path made the same before an output is compared. Three tests of
     `golden.rs` were written for Linux and macOS only and did nothing on Windows, silently
     (`compiled_input_waits_alone`, `compiled_input_cancel`, `interpreted_input_cancel`): they run on every
     system now, the last over pipes of Rust's library (`std::io::pipe`) instead of sockets of Unix.
1072. **The kernel on Windows** is built, linked with LLD and booted in QEMU as on Linux and macOS
     (`tests/kernel.rs`). QEMU is looked for on `PATH`, then where its installer puts it
     (`C:\Program Files\qemu`), and a missing one fails the test with what to install. The CI installs the
     build qemu.org points to (qemu.weilnetz.de), the one of each machine, at a fixed version (11.1.0 of
     11 August 2026), its installer checked against its SHA-256 and kept by GitHub's cache.
1073. **`tests/windows-pending.txt` is gone**, and with it `tests/pending`: nothing waits.
1074. **One runner of the benchmarks**: `bench/run.py`, on the three systems, in place of `bench/run.sh`.
     The C goes through the compiler and the flags `sg build` uses there (`/MT` and LLD on Windows); a
     library of C is named on the first line of the C (`/* link: -lz */`) and, where its name differs on
     Windows, on the second (`/* link on windows: -llibcurl */`); `-L` gives folders of libraries to SG
     and C alike. Before it is timed, each build runs once and its output is compared with the others',
     the checked build's included (it was not before); `--runs N` takes the best of `N` instead of five.
     Found by it: `sg emit-c` refused `-L`, though which name of an `extern "z", "zlib"` it writes on its
     first line depends on where the library is; it takes `-L` now. The benchmarks `crc`, `deflate` and
     `curl` still wrote C's `long` as `long` and named one library each, which 101 had changed in the
     examples they come from: they take `CULong`, `CLong`, `Memory.sizeOf<ZStream>()` and both names now.
1075. **The C of the benchmarks on Windows**: what POSIX has and Windows has not is taken from Windows
     (`bench/windows/posix.h`, the branches of each program), every program measured; `cat` and `true` for
     the programs that start them are built from `bench/windows`. Said with the measurement above.
1076. **Where Windows is measured**: Windows 11 on 64-bit Arm in a virtual machine, in pieces of less than
     ten minutes; x86-64 is not measured.
1077. **What was found slow** goes into "Slower than C": `dates`, `listing`, `echo` and `echosync`, on
     Windows only. Nothing was changed for them in this slice.
1078. **The CI** runs on every push to `main`; the branch `windows` is merged and deleted.
1079. **Documents**: the measurement on Windows and "Slower than C" above, "Not here yet", this list; 04
     ("Testing on Windows", "The machines and the compiler", "Without an operating system, as built", "The
     order"), the README of the repository. 06 has nothing new: nothing of the language changed. `cargo
     test` passes on macOS and on Linux in a container (356 of 1442 programs compile through C, 80 of them
     under the thread sanitizer, 27 of 33 programs of tests). On Windows 11 on 64-bit Arm `freestanding`,
     `layouts`, `kernel` and the four new and opened tests of `golden` pass; the whole of `cargo test` on
     Windows is the CI's. New: `needs_nothing_of_128_bits`, `bench/run.py`, `bench/windows`.

A fix between the hundred-and-second and the hundred-and-third slices: a task freed while it was woken.
Found by the tests of the hundred-and-third on Linux, where the thread sanitizer saw the clock of delays wake
a task that was freed on another thread (`ok_pool_queues`, a pool of two). It was a real use of freed memory,
not a report of the sanitizer alone: with a pause put where a thread can be taken off the processor, the
address sanitizer sees it every time. 1120 to 1123 were put to the author and approved on 17 November 2026:

1120. **A wait of its own** (a socket, a program, a signal, work on a thread of its own, on every system): the
     thread that ends it sets `settled` to 2, wakes the task and sets it to 1 last. A task that sees 2 waits
     for the 1; until then the wait and the task may still be touched. Before, `settled` was 1 before the
     wake, and the task could go on, end and be freed before the wake wrote into it.
1121. **A delay**: the clock ends it under its lock, wakes the task and lets the lock go; the task that sees
     the delay over takes the lock once, so it goes on only after the clock is done with it. One lock is
     taken when a delay is over.
1122. **A task that ends**: whoever awaits it is woken while the task says `SG_ENDING`, and `SG_ENDED` comes
     after the wake; `await`, `wait` and a list of tasks that see `SG_ENDING` wait the moment until
     `SG_ENDED`. Input and output did so already (`done` 2, then 1), and a list of tasks is told under its
     lock.
1123. **The test**: `-DSG_PAUSE_WAKES` builds the runtime with a pause of 2 ms where the waker has told the
     end and not yet woken the task, and where a task has just begun to wait, after which it looks again.
     `compiled_small_pools` builds its programs so a second time, under the thread sanitizer and under the
     address sanitizer, and with them `ok_wake_races` (twelve thousand times a task that ends at once after a
     delay, freed by the one that awaits it). Without the fix of 1121 the address sanitizer sees a use of
     freed memory and the thread sanitizer a race; without that of 1122 the thread sanitizer a race.

Choices made in the hundred-and-third slice (consts). 1080 to 1087, 1098 and 1099 were put to the author
before the slice and approved on 17 November 2026; they are in the decisions file ("Consts"). What building
them asked for besides, from 1109 (1088 to 1097 are the statics of the hundred-and-fifth slice, 1100 to 1108
the calls in a const of the hundred-and-fourth). Approved on 17 November 2026:

1109. **A const array, struct or enum is a `read` borrow** where it is used, of the one value, which lasts
     for the run: `Masks[i]` reads it where it lies, `sum(Masks)` lends it as a slice, a function with no
     borrowed parameter may return it (`read uint[] masks() { return Masks }`), and `var copy = Masks` copies
     it as a `read` variable is copied. So writing into it, `ref Masks` and a method that changes it are
     refused by the rules of a `read` borrow; writing and `ref` say that it is a const. A const of a number,
     a `bool`, a `char` or a text is a value, as a number written out is; `read Size` on one is an error
     whose hint is `Size`. A text is the text itself where it is used, as if it were written there.
1110. **No text inside an array, a struct or an enum** yet, which 1082 had let in: an item of `string[N]` is
     a `string`, which owns its bytes and is not `Copyable`, and the language has no type of an array of
     `read string`. A const of one text is in; a table of texts waits for a program that needs it.
1111. **The value of a const is found by the interpreter**, in the checker, once every initializer is checked
     (the calls of 1100 to 1108 will take the same way). A panic on the way is a compile error that names the
     const (`the value of \`Width\` is found when the program is compiled, and this stops it: overflow: ...`),
     and so is a const that needs itself (`` `First` -> `Second` -> `First` ``). An overflow there is an
     error in a release build too.
1112. **The count of an array is found before the types**, since a field of a struct can be `byte[Size]`:
     from the text of an integer const, made of numbers, other such consts, operators, casts to an integer
     type, `MinValue` and `MaxValue`. A const of another kind there is an error that says why
     (`` `Width` cannot give the count of an array: it is a `double` ``). The checker finds every const again
     later, and the two must agree.
1113. **A name in a pattern that names a const stands for its value**, in a `match` and after `is`, inside
     the parentheses of a variant too; a variant of the enum looked at, of that name, comes first. Only a
     const of a number, a `char`, a text or a `bool` is a pattern; another is an error whose hint is `when`.
1114. **A local or a parameter does not take the name of a const it sees**, of its module or of a block
     around it, since a pattern would read the name as the const: the rule of 92 for consts.
1115. **`embed`** reads the file when the program is compiled, from the folder of the source file that names
     it; a missing file, an empty one (an array has an item) and one of more than 64 MB are errors. It is
     written only where a const is given its value (an error of the parser elsewhere), with its path written
     out.
1116. **In compiled code** a const of a number, a `bool` or a `char` is written where it is used; one of an
     array, a struct or an enum is one `static const` outside the functions, defined only if the code reads
     it, and read where it lies (`((sg_A4_uint*)&sg_k_Masks)->items[i]`). A `byte` array is kept as its bytes.
1117. **A const nothing reads** is not an error, as a function nothing calls is not.
1118. **A `public const`** of a module shows its type: a type the module keeps to itself is an error there, as
     for a `public` function.
1119. **Documents and tests**: this list, the table above, "Not here yet", "Next"; 02 ("Consts"), 04 ("Consts"
     and "Consts, as built"), 06. Tests: `ok_consts`, 30 `error_const_*`, the program `tests/modules/consts`
     (a const of another module, the count of an array from one, `embed`, a test) and
     `error_const_exposes` with the module `hidden`. `cargo test` passes on macOS and on Linux in a container (359 of 1476 programs compile through C, 81 of them under the thread sanitizer, 28 of 34 programs of tests). Found by them: an array read only for its length (`buffer.length()`) left its variable unused in C, which GCC refuses; the array is now named all the same, with no cost.

Choices made in the hundred-and-fourth slice (calls in a const). 1100 to 1108 were put to the author before
the slice and approved on 17 November 2026; they are in the decisions file ("Calls in a const"). What building
them asked for besides, from 1124. Approved on 18 November 2026:

1124. **Two times to find a const.** One that calls no function is found before the bodies are checked, as
     in the hundred-and-third slice; one that calls (or names a const that calls) once the bodies are checked.
     A body that needs what such a const holds before it is found (a text, which is written where it is
     used; a pattern; `byte[Count]`) is left as written and checked again once it is. So the order of the
     consts and of the functions in the file does not matter.
1125. **The count of an array in a type** (`int[Count] items` in a field, in a signature) cannot come from a
     const that calls: the types come before the bodies, and the error says so (`` `Count` cannot give the
     count of an array: its value is found by a call, which runs once the types are known ``). In an
     expression (`byte[Count]`) it can. Rust lets a `const fn` give the length of an array type; we would have
     to check bodies before types.
1126. **What is written is checked, not what runs**: every function a call of the initializer reaches, and
     the `format`, `equals`, `hash`, `compare` and `dispose` of each struct and enum whose value is there,
     which run with no call written. A barred call on a line that never runs is still an error, as in Rust's
     `const fn`; a const that reaches itself through a function is an error the same way (`` `A` -> `B` ->
     `A` ``). The message names the first function the initializer calls and shows the place where the
     barred thing is written (`` `size` calls `load`, which reads or writes a file here ``); inside the
     language's own module it stops at the call the program wrote (`` `draw` calls `Random.fromSystem`, which
     draws random bytes of the system ``).
1127. **What is barred, written out for the language**: besides 1102, `exit`, calling an `async` function (it
     starts a task), cancelling, `Memory.allocate` and every value of a `raw T`, which covers what `unsafe`
     does with raw memory, and a variable of C. An `atomic` value is in: no other task can see it there.
1128. **Of the fractions**, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`, `exp`, `log`, `log2`, `log10`
     and `pow` are barred (1106 named four; the others are rounded by the same C library). `sqrt`, `floor`,
     `ceil`, `round`, `truncate`, `abs`, `min`, `max`, `fusedMultiplyAdd`, reading a number out of a text and
     printing one are in: their result is exact or correctly rounded everywhere, and printing is our own.
     *Since the hundred-and-tenth slice (1235)* `sinh`, `cosh`, `tanh`, `asinh`, `acosh`, `atanh`, `cbrt`, `exp2`,
     `expMinusOne`, `logOnePlus`, `erf`, `erfc`, `gamma`, `logGamma` and `hypot` are barred too; the exact ones
     of that slice are in.
1129. **A step** is a block run or a function called: a loop with an empty body still counts. The hundred
     million steps take about 14 seconds in the interpreter on the author's Mac; the error is `` finding the
     value of `Spun` takes more than 100 000 000 steps ``. The note says such a value is found when the program
     runs; with the statics of the next slice it will name `static`.
1130. **The 64 MB are known from the type**, before anything runs, for every const (`const long[10000000]
     Zeros = long[10000000]` is an error now too): `` a `Huge[9]` takes 72 MB ``. A text is measured once found.
1131. **`const int[] Squares = squares()`** takes the count from the array the function returns
     (`int[5] squares()`), as 1098 takes it from `{ ... }` and 1099 from `embed`. A direct call of a function
     only.
1132. **A hash table at compile time is the table of the run**: its entries in the order they came, so a const
     built from what a `HashMap` holds is the same on every compile, and the same as the function gives when
     the program runs.
1133. **What an initializer may be**: the same as before (numbers, texts, consts, operators, casts, arrays,
     structs, variants), now with calls of functions, of a type's function (`Size.square(3)`) and of methods on
     a const (`Square.area()`), whose arguments are of the same; the name of a function is one too. A `match`,
     a `$"..."` or a loop goes in the function.
1134. **Documents and tests**: this list, the table above, "Not here yet", "Next"; 02 ("Consts"), 04 ("Calls in
     a const" and "Calls in a const, as built", and "comptime" in "Deliberately deferred"), 06; the root
     README. Tests: `ok_comptime` (a table, recursion, a text, a struct and its methods, an enum method, a
     list and a set, `Result`, `Random.fromSeed`, a const of a block and of a test, as a value, a text, a
     pattern and the count of an array, and the same functions called when the program runs),
     `error_const_counted` changed, `error_const_call` removed (it is allowed now), 16 `error_comptime_*`,
     `tests/modules/consts` with two consts found by functions another module does not see, and
     `const_steps_run_out` in `golden.rs` (apart from the cases: it takes seconds). `cargo test` passes on macOS and on Linux in a container (360 of 1492
     programs compile through C, 81 of them under the thread sanitizer, 28 of 34 programs of tests).

Choices made in the hundred-and-fifth slice (statics). 1088 to 1097 were put to the author before the slice
and approved on 17 November 2026; they are in the decisions file ("Statics", "Globals, as decided"). What
building them asked for besides, from 1135. Approved on 18 November 2026:

1135. **The value is a function of its own**: what a static is given becomes the body of a function,
     `Name#init`, which no program can name, checked, run and compiled as any other; so the rules of a body
     (moves, borrows, the values never read) hold there. A static of a block sees the consts and statics of
     the blocks around it and no local, as a const of a block does.
1136. **Found when the program is compiled** only by the rule of a const that calls nothing (103): numbers,
     consts, operators, casts, `MinValue` and the others, arrays, structs and variants of them, and `atomic`
     around them, of a type a const can hold. A static given a call is found where it is first used, even when
     the function could run when the program is compiled: running it there could take seconds and turn a
     panic into a compile error. For a table found when the program is compiled, write a `const` and give the
     static that const, or use the const.
1137. **Where it is used**: a number, a `bool`, a `char` or a function is read as a value; anything else is a
     `read` borrow of the one value, as a const array is (1109), so writing into it, `ref` and a method that
     changes it are refused. Assigning to a static and `ref` of one are errors that say a static is never
     assigned and point at `static atomic` and `static locked`; `read Limit` on a number is an error whose
     hint is `Limit`.
1138. **`static locked`** is written only as the value of a `lock` (`lock Settings as s`, with others in the
     same `lock`); anywhere else, `share` and a call included, it is an error whose hint is the `lock`. One
     `lock` takes it once, as any value.
1139. **A local does not take the name of a static** it sees, as it does not take a const's (1114): a name
     means one thing in a function.
1140. **A static that holds a function is not called by its name** (`Double(21)` is an error whose hint is `var
     f = Double`, then `f(21)`): a call by a name reads a local; this waits for a program that needs it.
1141. **The circle** is followed through the calls of what finds the value, the `format`, `equals`, `hash`,
     `compare` and `dispose` that run unwritten, and the statics read on the way, whose own values are
     followed in turn. The error names the path and shows where the next static is read (`` the value of
     `First` needs itself: `First` -> `Second` -> `First` ``).
1142. **A circle through a function value** panics when the program runs, in the interpreter and in compiled
     code with the same words: a task that finds a static being found by itself (`` the value of `Total` needs
     itself: it is read while it is being found ``), or by a task that waits for a static this task is
     finding. Compiled, every task that waits for a static sleeps on one condition of the runtime, under one
     lock taken only around the state, never while the value is found. Without an operating system a core
     spins until the value is found, as 1095 says: a circle there, or an interrupt that reads a static the
     kernel is finding, spins for ever; the kernel's count of ticks is known from the start, so it never
     waits.
1143. **At the end**: a debug build and the interpreter let go of what the statics hold when the program or a
     test ends, before the leaks are counted, so a static's `List` is not a leak while `shared` values that
     hold each other inside one still are; a release build does nothing (1092). Nothing in a static is
     `Disposable`, so letting go of it runs no code of the program.
1144. **Closed or disposed of** is looked for through the fields, the items and the variants: a struct
     holding a `StreamWriter` is refused as the writer is, naming the field; the hint gives the value as a
     parameter.
1145. **In compiled code** a static known from the start that holds no atomic value is a `static const`, in the
     data the program only reads; one with atomic values, or `locked`, is a writable global. A `static locked`
     value lies after a mutex and the counts of a `shared` value, so a `lock` finds it as it finds one made
     with `new shared locked`; a value aligned to more than 16 bytes there is refused by the C compiler for
     now. A library always waits for a static on the lock of the runtime, since C may call it from several
     threads; a program without tasks takes a static being found as a circle.
1146. **A const reads no static**: by its name it is an error (`` a const cannot read it ``), and in a
     function a const calls, an error of what may run when the program is compiled (`` what it runs reads the
     static `Base` ``). The notes that said a value is found when the program runs "by a function it calls"
     now say it is a `static`.
1147. **Documents and tests**: this list, the table above, "Not here yet", "Next"; 02 ("Constants and
     statics"), 04 ("Globals, as decided", "Statics", "Statics, as built", and the rows of C's variables and
     callbacks), 06; the root README; the kernel counts its ticks in `static atomic ulong Ticks`. Tests:
     `ok_statics`, `tests_statics`, `panic_static_itself` and `panic_static_itself_tasks`, 18
     `error_static_*`, the program `tests/modules/statics`, and the benchmark `statics` (0.99 times C on
     macOS). `cargo test` passes on macOS and on Linux in a container (364 of 1514 programs compile through C,
     84 of them under the thread sanitizer), the kernel in QEMU included. Found by them: the C type of an
     atomic value is the type it holds, so whether a static holds one is told from the type as written, or a
     counter known from the start would have been put where the program only reads.

Choices made in the hundred-and-sixth slice (loops with a name, counting down, raw text, `const assert`).
1148 to 1156 were put to the author before the slice, from the table of what the language lacks beside C23 and
the draft of C2y, and approved on 18 November 2026; they are in the decisions file ("Loops with a name",
"Counting down", "Raw text", "`const assert`"). What building them asked for besides, from 1157. Approved
on 18 November 2026:

1157. **What a loop moved** is followed at every way out of it, and a hole is closed. Each `continue` starts a
     round, as the end of the body does, so what is moved before one is checked the same way; after the loop a
     value is gone if it was gone at a `break`, before the first round (unless the loop is `while true`) or at
     the end of a round. Before, a value moved before a `continue` or a `break`, or moved before a loop that gives
     it back inside (`take(xs)`, then `for ... { xs = List<int>() }`, then `take(xs)`), was taken as there: the
     interpreter stopped on it with an internal error and compiled code read an emptied list, which for a
     pointer would have been a null one. Found while `break outer` was being built, since it leaves through
     the same ways. No program of the repository was refused by it.
1158. **The size of a type is measured only in a `const assert`**: `Memory.sizeOf` and `Memory.alignOf` in a
     const are an error whose hint is the assert, and in a function a const calls an error of what may run when
     the program is compiled (`` what it runs measures the size of a type ``); before, a function could measure
     a type for a const, with the layout of the machine that compiled it.
1159. **An assert that measures a type calls no function**, so that C can check it as well: a call there is an
     error whose hint is an assert of its own. A limit of `nint` or `nuint` counts as measuring.
1160. **The message is a text written out**, raw text included; `$"..."` is an error that says the values of a
     comparison are shown beside it. A failed assert says the message, or the condition as written.
1161. **Another machine**: `sg build --freestanding --target` leaves an assert that measures a type to C's
     `_Static_assert`, whose message names the file and line of SG; on this machine the checker refuses it
     first, and compiled code hands it to C either way. Only 64-bit machines are built for today (C's `__int128`
     is used by the runtime), so the two have not differed yet.
1162. **An assert is a const of its own**, named `assert#` and where it is written, which no program can name;
     it is found with the other consts, by the same steps and limits, and nothing of it is in compiled code but
     the `_Static_assert` of 1161.
1163. **Raw text, the edges**: the opening quotes followed by text on the same line make a raw text of one line,
     which cannot start or end with `"`; more quotes in a row than opened it are an error whose hint is to open
     it with more; a line with nothing on it may be indented less; a `\r` before a line end is dropped, so a file
     saved with Windows' line ends gives the same text.
1164. **A range in brackets** at the head of a `for` is only `(a..b).reversed()`: without it, or with another
     method, it is an error whose hint gives both ways; `..=` is refused there as at any `for`.
1165. **Names of loops, the edges**: `break outer` that names the loop it is in is a `break`; the name of a loop
     that is not around the `break` gives an error listing the loops around it that have a name, or the hint to
     name one; a name and `:` in front of anything but `for` and `while` is an error.
1166. **Documents and tests**: this list, the table above, "Next"; 02 ("Raw text", "Loops with a name, and
     counting down", "Constants and statics"), 04 (the rows of 1148 to 1157), 06; the root README. Tests:
     `ok_labeled_loops`, `ok_reversed_range`, `ok_raw_text`, `ok_const_assert`, 19 `error_*` (the names of
     loops, the moves of 1157, raw text, `const assert`, `Memory.sizeOf` in a const), and
     `measured_for_the_target` in the tests of programs without an operating system. `cargo test` passes on macOS and on Linux in a
     container (368 of 1537 programs compile through C, 84 of them under the thread sanitizer), the kernel in
     QEMU included.

Choices made in the hundred-and-seventh slice (an enum with numbers, the bits of an integer). 1167 to 1183 were
put to the author before the slice and approved on 18 November 2026; they are in the decisions file ("Enums with
numbers", "The bits of an integer"). What building them asked for besides, from 1184. Approved on
18 November 2026:

1184. **The number of a variant is a const of its own**, named by the variant and where it is written (`Image#`),
     which no program can name, checked against the enum's type and found with the consts that call nothing,
     before any function is checked: so it calls no function, even through a const (an error that says so).
     It names consts and, through a cast, the other variants (`Trailer = (byte)Block.Image + 0x0F`); a circle
     between them is the error of a const that needs itself. A variant named alone there is not seen (`Image +
     1` is the error of a name that is no const).
1185. **The cast reads the number where the value is**: `(byte)block` moves nothing, even when the enum is not
     `Copyable` (Rust's `as` moves an enum that is not `Copy`). It works through a borrow (`read Block`), and in
     a const.
1186. **`: Comparable` without a `compare`**, on an enum with numbers, is given one that compares the numbers:
     the order 1174 asks for, through the path every `compare` takes (`<`, `sort`, `x.compare(read y)`). A
     `compare` written in the enum is used instead; a struct and an enum without numbers still write theirs.
1187. **`(Block)code`**: a name in capitals alone in parentheses, before a name or a number, is an error of the
     parser that says a cast goes to a number type or `char`, with the hint `Block.fromValue(n)`; before, it was
     a line that did not end where expected.
1188. **The type of the numbers** is any integer type but C's `long` and `unsigned long`, whose size differs
     from system to system (hint `int` or `long`); `nint` and `nuint` are taken, as Rust's `isize` and `usize`.
1189. **`fromValue` is the language's**: a function of that name written in an enum with numbers is an error. In
     an enum without numbers it is an ordinary name.
1190. **The bits, the edges**: `reverseBytes` on a `byte` or an `sbyte` is the value itself (where `toLittleEndian`
     is an error: there it would be an array of one byte); a count of 0 is the width of the type; the next power
     of two of 0 is 1, as in Rust; the count of a rotation is any integer, as the count of a shift. The panics
     say `overflow: the next power of two of 129 does not fit in byte` and `log2 of 0: no power of two is 0`.
     GCC, the C compiler of Linux, has no builtin that turns the bits around, so there it is three swaps and
     the swap of the bytes; Clang has one.
1191. **Found on the way**: a method of an enum called on a value with no name, with an argument after it
     (`Level.High.compare(read Level.Low)`), was compiled with the C type of a struct of that name, which the C
     compiler refused. The receiver is now typed as the struct or the enum it is written in.
1192. **Documents and tests**: this list, the table above, "Not here yet", "Next"; 02 ("Enum and match", "Numbers
     and bit operators"), 04 ("Enums with numbers", "Enums with numbers, as built", "The bits of an integer",
     and a note on "`Comparable` is written"), 06; the root README. Tests: `ok_enum_numbers`, `ok_bit_methods`,
     `panic_next_power_overflow`, `panic_log2_zero`, `tests/release/next_power_wraps`, the program
     `tests/modules/enumnumbers`, and 17 `error_*` (the numbers of the variants, the casts, C, the
     powers of two on a signed type, the count of a rotation). `cargo test` passes on macOS and on Linux in a
     container (374 of 1560 programs compile through C, 84 of them under the thread sanitizer), the kernel in
     QEMU included. Found by them: GCC, which builds the programs without an operating system with warnings
     as errors, took a test and a statement after it on one line of the new helpers for a misleading indent;
     they are written in braces.

Choices made in the hundred-and-eighth slice (the default value of a field, `with`, `?.`). 1193 to 1201 were put
to the author before the slice and approved on 18 November 2026; they are in the decisions file ("The default
value of a field", "`with`", "`?.`"). What building them asked for besides, from 1202. Approved on
18 November 2026:

1202. **A default is a const of its own**, as the number of a variant is (1184): `port#` and where it is written,
     checked by the rules of a const and found with the others, so a literal that leaves the field out reads one
     value. The messages speak of the field (`the default value of a field cannot be a List<int>`, with what a
     default can be). A field `int? limit = 5` holds a const of `int`. `null` and an empty collection are no
     const; they are checked where they are written (`int count = null` is an error there, not at a literal).
1203. **A field that borrows has no default**, nor does one of a union: what a field borrows is given when the
     value is built, and a union is built with the one field it holds.
1204. **Where `with` stands among the operators**: tighter than any operator between two values, looser than one
     in front, as in C#: `try load() with { ... }` changes what `try` gives, `a + b with { ... }` changes `b`. In a
     condition it needs parentheses, as a literal does, with an error that says so; `with { }` is an error.
1205. **The order of `with`**: the value it starts from first, then the values written in their order, and only
     then are the old values of those fields dropped (compiled, one expression of C; the interpreter drops each
     where it is replaced, which shows only as the order of two `dispose`). A value lent is taken only when it is
     `Copyable`; the hint is `p.clone() with { ... }` when the struct has `clone`.
1206. **`?.` is a `match`** the parser writes (`match a { null => null, var held => held.b }`, `held` a name no
     program can write), so its borrows, its moves and its C are those of `match`. A place is lent: `read`, or
     `ref` when the first method of the chain, reached through fields and items, changes what it is called on
     (`cache?.items.add(1)`); a method of the program that takes `this`, as the first step, uses the place up,
     as `.` does (`ticket?.spend()`). From a value with no name a field is taken, as `make().name` takes it
     (`find(3)?.name` is a `string?`).
1207. **The type of `?.`** is that of the chain made a `T?`: a place that is `Copyable` is copied (`a?.age` is an
     `int?`), and a `T?` stays one. Where a value that is not a `T?` is expected (`int x = a?.age`), the error
     is that of an `int?` there, with the hint `??`.
1208. **Found on the way**: a `match` that gave a borrow of what it looked at (`var text = match read answer {
     Text(t) => t ... }`) did not keep it lent, so `answer` could be changed while `text` was still used, and
     compiled code would read what was freed. What a `match` gives now borrows from what it looks at, as the
     names of its arms do. A program that changed the value while the borrow lived is now a compile error.
1209. **Found on the way**: `null` where a `read T*?` or a `read shared T?` goes (`missing(null)`) was not in
     the C back end; it is a pointer to a pointer that is `NULL`, one for each type, which nothing writes.
1210. **Documents and tests**: this list, the table above, "Not here yet", "Next"; 02 ("Construction", "`?.`"),
     04 ("The default value of a field", "`with`", "`?.`", each with its row "as built"), 06; the root README.
     Tests: `ok_field_defaults`, `ok_with`, `ok_null_conditional`, the module `tests/modules/defaults` with
     `ok_defaults_module`, and 20 `error_*` (what a default can be, `with` on what it cannot copy or another
     module cannot build, `?.` on what is never `null` and on a `Result`, a borrow of `?.` and of `match` kept,
     a value used up). `cargo test` passes on macOS and on Linux in a container (378 of 1584 programs compile
     through C, 84 of them under the thread sanitizer), the kernel in QEMU included.

Choices made in the hundred-and-ninth slice (seeking in a file, `File.openWrite`, `File.createTemp`). 1211 to
1214 were put to the author before the slice and approved on 18 November 2026; they are in the decisions file
("Seeking in a file", "A temporary file"). What building them asked for besides, from 1215. Approved on
18 November 2026:

1215. **`seek` gives the new place**, counted from the start of the file, as a `Result<long, IoError>`, as
     Rust's `seek` gives it; there is no `position()` or `length()` beside it: `seek(SeekFrom.Current(0))` tells
     where the stream stands, `seek(SeekFrom.End(0))` how long the file is (and moves there). The place is a
     value like any other, so a `seek` whose place is not wanted is written `_ = try writer.seek(...)`, as
     `_ = try reader.readLine(ref line)` is. Whether such a value could be dropped without `_ =` (a list of
     built-in calls whose value is a by-product, a mark as Swift's `@discardableResult`, or Rust's `must_use`
     the other way round) was asked as 1224, and the rule stays: every value is used or dropped on purpose.
1216. **`SeekFrom` is written in SG** in the module `std`, next to `FileKind`: `Start(long offset)`,
     `End(long offset)`, `Current(long offset)`, `Copyable`, `Equatable` and `Formattable`, seen from every
     module. `Start` holds a `long` too (Rust's holds a `u64`): one type for the three, and a negative start is
     an `InvalidInput` as a place before the start is from the end or from where the stream stands.
1217. **What cannot seek is told where it can be**: `Stdin.seek`, `Stdout.seek` and `Stderr.seek` are compile
     errors, as `Stdout.close()` is, and so is `seek` on the reader of a connection, which the type tells; the
     note names the call that opens a file that can. Lent on as `ref StreamReader` or `ref StreamWriter`, the
     program's streams are `NotSeekable` when the program runs, as are the pipes of another program and a writer
     of `File.openAppend`, whose every write goes to the end (`O_APPEND`), so a seek there would move nothing a
     write sees. `NotSeekable` is the last variant of `IoError`, so the numbers of the others stay; `ESPIPE`
     (a pipe opened by its path) is `NotSeekable`, and on Windows what is not a file on a disk.
1218. **In an async function** a reader seeks as it is: it writes and reads nothing, it only asks the system
     where the file stands or how long it is, as quick as `File.delete`, which is made as it is there too; a
     reader has no `seekAsync` (an error that says so). A writer writes what it holds before it moves, which
     holds the thread, so `writer.seek` in an async function is an error with the hint `await
     writer.seekAsync(...)`, which takes a token that may come last, writes on a thread of input and output as
     `flushAsync` does, and then moves (Tokio's `AsyncSeek` of a `BufWriter` flushes the same way).
1219. **A reader moves inside its block without the system**: the block holds the bytes of the file from where
     the reader stands, less what it has given out of the block, so a `Start` or a `Current` that lands in it
     moves in it, forward or back (Rust's `BufReader` keeps its buffer only for `seek_relative`). `End` asks the
     size of the file (`fstat`); a place outside the block is one `lseek`, and the block is let go. After a seek
     the check of UTF-8 starts again from the place, and an `InvalidText` is still counted from the start of the
     file. A writer asks where it stands (`lseek`) only for `Current`, and the size only for `End`.
1220. **A place past the end is no error**, as in C, Rust and C#: a read there finds the end, and a write there
     leaves zero bytes between the end and what it writes. A place before the start, or one past what a `long`
     holds, is `InvalidInput`, and the stream stays where it stood.
1221. **`File.openWrite(path)`** opens for writing only, from the start, keeping what the file holds; a missing
     file is made as `createText` makes one (0666 less the umask), and a folder is `IsDirectory`. It has an async
     form, `File.openWriteAsync`, as `createTextAsync`. On Windows `GENERIC_WRITE` with `OPEN_ALWAYS`.
1222. **`File.createTemp()`** is written in SG in the module `std`, as `File.info` is, on a call of the system
     only it makes (`mkstemp`, the file closed again; on Windows `CreateFileW` with `CREATE_NEW` and a list of
     access that gives only its owner anything, as `Directory.createTemp` gives a folder). It has no async form,
     as `File.delete` has none. A name that is taken is tried again with other letters, up to a hundred times,
     on Windows; `mkstemp` does that by itself.
1223. **Documents and tests**: this list, the table above, "Not here yet", "Next"; 02 ("Reading and writing
     files", "Folders, paths and the environment"), 04 ("Seeking in a file", "A temporary file", each with its
     row "as built"), 06; the root README. Tests: `ok_file_seek`, `ok_async_seek`, and 8 `error_*` (the
     program's streams, the reader of a connection, a number where a `SeekFrom` goes, a `read` borrow, `seek`
     of a writer and `seekAsync` of a reader in an async function, `File.createTempAsync`). `cargo test` passes on
     macOS and on Linux in a container (380 of 1594 programs compile through C, 85 of them under the thread
     sanitizer), the kernel in QEMU included; on Windows the two programs give the same output interpreted and
     compiled, and a temporary file gives its owner alone any access. Found by them: GCC, reading into a buffer
     of 4 bytes, took the read of 64 KB or more that goes straight into the buffer to overflow it, since it could
     not see that the count read so far is never below zero; the test now says so.

Choices made in the hundred-and-tenth slice (the rest of `<math.h>`, `abs`, `clamp`, the saturating operations,
`Memory.zeroSecurely`). 1225 to 1240 were put to the author before the slice and approved on 18 November 2026;
they are in the decisions file ("The rest of `<math.h>`", "Integers: the absolute value, `clamp`, saturating",
"Zeroing a secret"). What building them asked for besides, from 1241. Approved on 18 November 2026:

1241. **`copySign` is the C compiler's own** (`__builtin_copysign`), which moves the bit of the sign without the
     library, so a program without an operating system has it, with `nextUp`, `nextDown`, `exponent`, `isFinite`
     and `isSignNegative`, which read the bits. `fraction`, `roundToEven`, `scaleByPowerOfTwo` and
     `ieeeRemainder` are exact but come from the library (`modf`, `rint`, `scalbn`, `remainder`): errors there,
     as `floor` is, and allowed in a const.
1242. **A fractional number is cut to an integer without `trunc`**: the test that its whole part fits is made
     on the number itself (above the low end less one, below the high end; at 64 bits the low end less one is
     the low end, and no number lies between them), and C's conversion cuts toward zero by itself. On x86-64
     `trunc` is a call of the library, and the ends of the type were found with `ldexp`, which a debug build
     calls too; so a program without an operating system could not turn a fractional number into an integer.
     The new test found it. The cast of a debug and of a release build, `tryToX` and `saturatingToX` all take
     the new test, and the ends are now made by a shift.
1243. **The panics**: `overflow: the absolute value of -9223372036854775808 does not fit in long`, with the
     hint `` `tryAbs` gives `null` instead, `wrappingAbs` wraps around, and `unsignedAbs` gives a `ulong` ``;
     `-x` now hints `tryNegate` and `wrappingNegate` (161 said it named none); `` `clamp`: the low end 4 is
     above the high end 2 `` and `` `clamp`: the low end is NaN ``. In a release build `abs` wraps as `-x`
     does; `clamp` checks its ends in every build, since they come from the data, as an index does.
1244. **The ends of `clamp` are a compile error** only when both are numbers written in the program, a minus
     in front included (`level.clamp(10, -5)`); with a variable at either end they are checked when it runs.
1245. **`scaleByPowerOfTwo` takes an `int`**, as C's `ldexp` does; a narrower integer widens, a `long` is an
     error with the usual hint. A power past what the type holds gives `Infinity` or 0, as in C.
1246. **`Memory.zeroSecurely` takes `ref`** of a list, an array or a window (`ref words[1..]`); without it the
     hint adds it. Plain data only: a `string`, or a `List` inside a struct, would lose what it owns. The
     interpreter writes into each item the value zero bytes are for its type (`false`, `'\0'`, a `null` raw
     pointer, a struct of zeros).
1247. **The names of C, Rust and C# get a hint**: `fmod` (`` `a % b` is C's `fmod` ``), `remainder`, `rint`,
     `nearbyint`, `roundeven`, `tgamma`, `lgamma`, `copysign`, `nextafter`, `nexttoward`, `modf`, `fract`,
     `frexp`, `ldexp`, `scalbn`, `scaleB`, `ilogb`, `logb`, `expm1`, `log1p`, `fabs`, `fmin`, `fmax`, `trunc`,
     `isfinite`, `signbit` and `isNegative`; a fractional method called on an integer hints the cast to
     `double`, the new ones too.
1248. **Documents and tests**: this list, the table above, "Not here yet", "Next"; 02 ("Integer overflow",
     the math under it, a secret wiped), 04 (the three rows, the first with its row "as built"), 06; the root
     README. Tests: `ok_math_more` (every new method on both types, the ends of each), `ok_zero_securely`,
     8 `error_*` (`abs` on a `uint`, the ends of `clamp`, `fmod` and `frexp`, a saturating conversion that
     cannot fail, `zeroSecurely` without `ref` and over text, `gamma` in a const), 5 `panic_*` (`abs`, the two
     kinds of ends of `clamp`, `-x`), `release/abs_wraps`, and without an operating system `error_math_exact`
     and new lines in `ok_runtime` (the methods of the bits, `clamp`, `abs`, a saturating conversion, a cast of
     a fractional number and `zeroSecurely`). `cargo test` passes on macOS and on Linux in a container (388 of
     1610 programs compile through C, 85 of them under the thread sanitizer); on Windows the new programs give
     the same output interpreted and compiled, and the names of its library (`_hypot`) link. Found by them:
     besides 1242, GCC refused a test that a value below zero is negative on an unsigned type (always false), so
     the helpers read the top bit instead; and `erf` differs in the last digit on Windows, as 164 says the
     library may, so `ok_math_more` prints what the library rounds with 12 digits after the point.

Choices made in the hundred-and-eleventh slice (`binarySearch`, `lowerBound`, `upperBound`). 1249 to 1254 were
put to the author before the slice and approved on 18 November 2026; they are in the decisions file
("Searching items in order"). What building them asked for besides, from 1255. Approved on 18 November 2026:

1255. **The names of other languages get a hint**: `binary_search`, `BinarySearch`, `bsearch` and `search`
     hint `binarySearch`; `lower_bound`, `partition_point`, `partitionPoint`, `bisect_left` and `bisectLeft`
     hint `lowerBound`; `upper_bound`, `bisect_right`, `bisectRight` and `bisect` hint `upperBound`; on a list,
     an array and a slice. On items with no order the error is the one of `sort`
     (`` `binarySearch` compares the items, and `bool` is not `Comparable` ``).
1256. **Compiled code**: one helper for each type of item halves the window with one test of the order at
     each step (`(a > b) - (a < b)` on integers and `char`, `sg_fcmp` on fractional numbers, `sg_cmp` on text,
     the `compare` the type wrote), and the three methods are small helpers that fix its two flags, which the
     C compiler folds. A list and an array are searched through the window of their items (the pointer and
     the count), so they have no helper of their own. An item that is not a number, a `char` or a text is
     given by its address, as to `indexOf`. The interpreter takes the same steps, so a `compare` that the type
     wrote is called as many times, with the same items, in both: the place found and then one more test of
     it for `binarySearch`.
1257. **Documents and tests**: this list, the table above, "Not here yet", "Next"; 02 (the methods of a list),
     04 (the row and its row "as built"), 06; the root README. Tests: `ok_binary_search` (equal items, every
     value from 0 to 21 against a search that looks at each item, an empty list, a range, an array of
     `double` with `NaN`, `char`, `ulong`, text through a borrow, a struct that wrote its `compare`), 5
     `error_*` (items with no order, on a list and through a slice, a value of another type, two names of
     other languages), and a line of `ok_runtime` without an operating system. `cargo test` passes on macOS and on Linux in a
     container (389 of 1616 programs compile through C, 85 of them under the thread sanitizer); on Windows on
     Arm `ok_binary_search` gives the same output interpreted and compiled.
1258. **zlib is named `extern "z", "zlib", "libz"`** in `examples/zlib` and the tests that link it (a fix after
     the slice). vcpkg's zlib 1.4 (13 November 2026) is built by zlib's own CMake, which names the library
     `libz.lib` on Windows, where 1.3 gave `z.lib`; the CI of Windows on Arm took the new vcpkg of its image and
     found neither name. The DLL comes from the table of imports of the library, whatever its name (1061), so
     nothing else changes. Tried on Windows on Arm with only a `libz.lib`, interpreted and compiled.

Choices made in the hundred-and-twelfth slice (`lock ... when`). 1259 to 1265 were put to the author before
the slice and approved on 18 November 2026; they are in the decisions file ("Waiting under a lock"). What
building them asked for besides, from 1266. 1266 to 1268 were put to the author while the slice was built and
approved on 18 November 2026:

1266. **The condition does not read what changes while no `lock` ends**: an `atomic` (a field, a static, its
     methods), the clock, random bytes of the system, files, streams, the network, other programs and `print`,
     written in the condition, are a compile error (`` the condition of `when` reads an `atomic`, and a task
     waiting for it is woken only when a `lock` block on the same value ends ``), whose hint is to keep it inside
     the locked value. The functions of the language's own module are followed (`` calls `Instant.now`, which
     reads the clock ``); those of the program are not.
1267. **`await lock` without `when` is an error**: an ordinary `lock` does not wait, so there is nothing to
     await; the hint drops the `await`. One way to write each thing.
1268. **Only a program with `when` pays for it**: the list of waiters is a pointer in the block in front of a
     `locked` value (`SG_WHEN`), which takes 80 bytes on macOS instead of 64 and stays 48 on Linux and 16 on
     Windows; a program without `when` has neither the pointer nor the reads of it.

Approved on 18 November 2026:

1269. **The list is guarded by its lowest bit, not by the mutex.** An owner that lets go of a `locked` value
     wakes those that wait (one of them may be left the only owner), and it may do so inside a `lock` block of
     the same value, through another owner (`other = null`), where taking the mutex would wait for itself. So
     the list has a lock of its own in its lowest bit, held only inside the runtime and never while code of
     the program runs; an owner that lets go, and one that looks with `weak`, take it, and so does the end of a
     `lock` block, after one read of the list under the mutex finds a waiter. The waiter checks the counts and
     puts itself in the list under the same bit, so either it sees the owner gone, or the owner sees it.
1270. **A waiter is a link in the list of each value**: a thread sleeps on a condition of its own, made on its
     stack for the wait, and an async task returns from its machine and is woken through the pool (`sg_wake`),
     coming back to take the locks again where it stopped. A wake takes the whole list out, and each waiter
     takes its other links out once it holds the locks again. The values are taken again in the order of
     their addresses, as `lock` takes them.
1271. **The interpreter**: a fiber that waits is in a list of the scheduler with the cells of its values; a
     `lock` block that ends on one of them makes it ready, and at each change of turn one that is left the only
     owner is made ready to look again and panic. The tasks of `Task.run` not started yet start before a fiber
     waits, since one of them may make the condition hold. Waiters that wait for each other, or several owners
     that all wait, stop the run with the panic of 399, exactly; compiled code waits there for now (see "Not
     here yet").
1272. **What is written**: `when` is no reserved word, read only after the last `as name`; `await lock` starts a
     statement. In the condition each name is lent to read, and a change there is the error of a `read` borrow
     with the hint `the condition only looks at what is locked; change it in the block`; the names an `is`
     gives there are seen in the condition only. The condition follows the rules of the block (no `lock`, no
     wait, followed through the calls) and is computed again after each wake, so it may run many times.
1273. **A `lock ... when` that is not awaited holds its thread**: a function that has one, called from an async
     function, is the error of 654 (`` `take` does `lock ... when` here ``); written in an async function, the
     error's hint is `await lock ... when ...`. `await lock` in code that is not async says that there it waits
     without `await`.
1274. **The panic of the only owner** is the same in the interpreter and compiled code: `` this task waits until
     `<condition>`, and no other task could make it hold: it is the only owner of `jobs` ``, at the condition,
     with the task that waits and the hint `` a task with no more to give says so inside the value before it lets
     it go, `lock jobs as q { q.done = true }`, and the condition looks at it: `when ... || q.done` ``. A value
     that is a `static` is never alone: every task reaches it.
1275. **Documents and tests**: this list, the table above, "Not here yet", "Next"; 02, 04 (the rows of the
     decision and "as built", the promise at the end), 06; the root README. Tests: `ok_lock_when` (four takers
     and a giver on threads, two values waited for together, a `static locked`, an owner let go inside a `lock`
     of the same value and a `weak` look), `ok_lock_when_async` (async takers and givers woken from tasks and
     from a thread, also among the small pools with the pauses of `SG_PAUSE_WAKES`), `panic_lock_when_alone`,
     and 8 `error_*` (an `atomic`, the clock through `Instant.now`, a change in the condition, a lock in it, a
     `when` not awaited, `await` without `when`, `await lock` not in async code, a function with `when` called
     from async code). `ok_lock_when` also waits on names an `is` gives in the condition. `cargo test`
     passes on macOS and on Linux in a container (392 of 1627 programs compile through C, 88 of them under the
     thread sanitizer); on Windows on Arm `ok_lock_when`, `ok_lock_when_async` and `panic_lock_when_alone` give
     the same output interpreted and compiled.

Choices made in the hundred-and-thirteenth slice (channels). 1276 to 1289 were put to the author before the
slice and approved on 18 November 2026; they are in the decisions file ("Channels"). What building them asked
for besides, from 1290; 1290 to 1299 were put to the author once the slice was built and approved on 18 November
2026.

1276. **What is written**: `var jobs = Channel<int>(16)` is the end that receives; `jobs.sender()` gives a
     `Sender<int>`; both are made more with `.clone()`. The methods are `send` and `receive`, the names of a
     `TcpStream`. Rust gives a pair `(tx, rx)`, which needs a tuple; Go has one `chan`; C# has
     `Channel.CreateBounded<T>(n)` with its `.Writer` and `.Reader`.
1277. **The end**: once the last `Sender` has ended, the receivers take what is left and then `receive` gives
     `null`. There is no `close()`: C#'s `Writer.Complete()` is forgotten and its reader waits for ever, and
     Go's `close` twice is a panic. Rust ends it the same way, with `drop`.
1278. **A send with no end to receive** gives `false`, and the item is dropped; the rule of the unused value
     makes the program look: `if !tx.send(x) { return }` or `_ = tx.send(x)`. Rust gives the item back in an
     error, Go waits for ever, C#'s `TryWrite` gives `false`.
1279. **Room**: always given, at least 1; the ring is taken once, when the channel is made (Go). No channel
     without a limit, whose memory could grow without end (the reason of the 64 KB line), and none with no room
     (Go's unbuffered channel): a task waited for does that. Below 1 is a panic, and a compile error when it is
     written as a number.
1280. **Several receivers**: `jobs.clone()` is one more end that receives from the same channel; each item goes
     to one of them, in the order it was sent, so a pool of workers is written with it. Go, C# and crossbeam do
     this; Rust's `mpsc` does not (`Arc<Mutex<Receiver>>`).
1281. **Async**: `sendAsync` and `receiveAsync`. A token may come last on these and on `send` and `receive`; then
     they give `Result<bool, Cancelled>` and `Result<T?, Cancelled>`. A time limit is
     `CancellationSource.after(ms)`. `receive` in async code is the error of a call that holds the thread, whose
     hint is `await jobs.receiveAsync()`.
1282. **The loop**: `for job in jobs` in code that is not async; in async code `while` and `receiveAsync`. A `for`
     that awaits stays with `linesAsync` in "Not here yet".
1283. **Not taken**: Go's `select`; `tryReceive`, `trySend` and `length`, since a loop that asks without waiting
     holds its thread (the reason there is no `isCompleted`); a `close` by hand.
1284. **Rules**: a send and a receive wait, so they are not in a `lock` block or a condition of `when`. An item
     goes to another task, so its type follows the thread rule. `Channel<int?>` is an error: a `null` item would
     read as the end; the hint is an enum.
1285. **A sender that keeps the loop from ending**: `for ... in jobs` while a `Sender` this function made of
     `jobs` (`jobs.sender()`, or a `.clone()` of one) has not been given away on every path is a compile error,
     whose hint is to give the last one away (`Task.run(produce, tx)`). Rust's best known mistake with channels.
     Only `for` is looked at: one `receive` may find an item.
1286. **Tasks that wait for each other**: the interpreter stops with the panic of 399; compiled code waits for
     ever for now, and the count of waiting tasks comes with the hardening, as for 1263.
1287. **What is left** is dropped when the last end ends, `dispose` too. When the last receiver ends, the
     senders that wait for room wake and get `false`.
1288. **Compiled code**: one block, a header (a mutex, the counts of the two kinds of end, the two lists of
     those that wait) and the ring; a thread sleeps on a condition of its own, an async task is woken through
     the pool. The items are copied as bytes, and a C function of each type of item drops them. A program
     without channels carries none of it.
1289. **Documents and tests**: below in 1299.

Approved on 18 November 2026:

1290. **The async versions are awaited where they are written**: they lend the end, as the async methods of the
     network do, so `var t = jobs.receiveAsync()` is an error whose hint is `await`.
1291. **A `for` over a channel in a variable receives from it, and does not take it**: after a `break` the
     channel is still there to receive from (`for x in read jobs` is the same); a channel with no name ends with
     the loop. A `for` over a channel is an error in async code, whose hint is the `while` loop, and in a `lock`
     block.
1292. **A wait the channel lets go on is not cancelled**: an item that is there is taken, and room that is
     there takes the item, also with a token already cancelled; a cancel ends only a wait. A send that is
     cancelled drops its item, as a send with no end to receive does (Rust's `send_timeout` gives it back in
     the error, which needs a generic error type).
1293. **A channel of a `StreamWriter`, or of a value that holds one, is not here yet**: a writer left in a
     channel would be closed where the channel ends, which async code has to await ("A writer dropped in
     async code").
1294. **What 1285 follows**: a local of the same function made by `.sender()` of the channel's variable, or by
     `.clone()` of such a local. A sender in a field, given as a parameter or made by another function is not
     followed; there the interpreter's panic of 1286 says it.
1295. **An item that is not taken ends inside the call**, and its `dispose` runs before `send` gives its answer,
     in the interpreter as in compiled code.
1296. **Who is woken**: in compiled code an item wakes the first receiver that waits, and room the first sender,
     in the order they came; the last end of a kind wakes all of the other kind. One woken looks again, since
     another may have taken what woke it. The interpreter looks at those that wait at each change of turn, in
     the order they began, so a run goes the same way every time. Making and ending an end takes the mutex.
1297. **The room is an `nint`**. A room whose ring does not fit in memory is a panic of compiled code (`a channel
     with room for ... items of ... bytes does not fit in memory`); the interpreter grows its ring as items come,
     and does not see it.
1298. **A token on the calls that block**: no other call that blocks takes one yet, and 1281 gives one to
     `send` and `receive`; a thread asleep on a channel is woken by the cancel. The cancel only marks the wait
     and wakes it, under the lock of the clock, and never takes the channel's mutex; whoever wakes a waiter
     takes it out of the list under the mutex, so a wait in the frame of a task is never touched after it has
     gone on.
1299. **Documents and tests**: this list, the table above, "Not here yet", "Next"; 02, 04 (the rows of the
     decision and "as built", the promise at the end), 06; the root README. Tests: `ok_channel` (two producers
     and three receivers, the order of one sender, items that own memory, a send with no receiver, what is
     left dropped, a borrowed channel left with `break`, tokens on the calls that block), `ok_channel_async`
     (senders and receivers that await on a channel with room for one, a sender on a thread, tokens; also among
     the small pools with the pauses of `SG_PAUSE_WAKES`), `panic_channel_deadlock` (interpreted only), and 5
     `error_*` (a send in a `lock` block, a `T?` item, a sender alive at the loop, `receive` in async code, no
     room). `cargo test` passes on macOS and on Linux in a container (394 of 1635 programs compile through C, 90 of
     them under the thread sanitizer); on Windows on Arm `ok_channel` and `ok_channel_async` give the same output
     interpreted and compiled.

Choices made in the hundred-and-fourteenth slice (the local time zone, writing and reading dates). 1300 to 1310
were put to the author before the slice and approved on 18 November 2026; they are in the decisions file ("Time
zones and dates in text"). What building them asked for besides, from 1311; 1311 to 1317 were put to the author
once the slice was built and approved on 18 November 2026.

1300. **The local time comes from the system**: one more call of the compiler that only `std` makes,
     `Clock.offsetAt(seconds)`, the seconds the system's time zone is ahead of UTC at that moment. On Linux and
     macOS `localtime_r`, which reads `TZ`, `/etc/localtime` and the system's files of zones; the offset is the
     local date and time it gives counted by the calendar of `std`, less the moment, since `tm_gmtoff` is not
     POSIX. On Windows `SystemTimeToTzSpecificLocalTimeEx` with the zone of `GetDynamicTimeZoneInformation`, which
     keeps the rules of each year, where the C library's `localtime_s` takes this year's rules for every year;
     `TZ` is not read there. The interpreter makes the same calls. Rust's library has no time zones (chrono reads
     the files of zones itself on Unix and asks Windows the same way); Go reads the files itself; C# has
     `TimeZoneInfo.Local`. Reading the files in SG was not taken: more code, and Windows apart all the same.
1301. **A type of its own, `DateTimeOffset`** (C#'s name): a moment and its offset from UTC. `DateTime` stays in
     UTC and in 8 bytes, and a function that takes one is not given a local time by mistake. Go keeps the place
     in its one `time.Time`, where `==` compares the place too; chrono has `DateTime<Tz>`; Java `OffsetDateTime`.
1302. **How one is made**: `d.toLocal()` at the offset of the system's zone then; `d.withOffset(offset)`, a
     `Result<DateTimeOffset, DateError>` that is `InvalidOffset` for a day or more either way;
     `DateTimeOffset.local(year, month, day, hour, minute, second)` from what the clocks of the place show, a
     `Result`; `t.toUtc()` back. No `DateTimeOffset.now()`: `DateTime.now().toLocal()`, one way to do it.
1303. **The hours summer time skips and repeats**: in `local(...)` a time the clocks skip (02:30 when summer
     time starts) is `Error(DateError.SkippedLocalTime)`; a time they show twice (when it ends) gives the earlier
     moment. Java moves a skipped time on and takes the earlier offset; chrono gives a result with three cases; C#
     throws for a skipped time and takes the later one; Go does not say which.
1304. **Its methods**: the parts as the clocks of the place show them, `year()` to `nanosecond()` and
     `dayOfWeek()`; `offset()` a `Duration`; `since`, `plus` and `minus`. `plus` keeps the offset, as Java's
     `OffsetDateTime` does: after the clocks change, `t.toUtc().plus(span).toLocal()` gives the new one. Equal
     and hashed by the moment and the offset, as every struct is; in order by the moment, then the offset (Java);
     `a.toUtc() == b.toUtc()` compares only the moments.
1305. **Printed** in RFC 3339, `2026-11-18T12:30:05+03:00`, the fraction as `DateTime` writes it; an offset of
     0 is `+00:00` (`Z` is for a `DateTime`), and an offset with seconds, as the local mean times of old had,
     `+01:56:56` (Java's way).
1306. **No pattern of your own** (`yyyy-MM-dd`, `%Y-%m-%d`): `$"{t.day():D2}.{t.month():D2}.{t.year()}"` does it,
     checked when the program is compiled and with nothing to run; no names of months either, since there is
     no locale. C and chrono have `strftime`, C# `"dd.MM.yyyy"`, Go `"02.01.2006"`.
1307. **Reading**: `DateTime.parse(text)` and `DateTimeOffset.parse(text)` read RFC 3339,
     `2026-11-18T09:30:05Z`, a fraction of 1 to 9 digits, an offset `Z`, `+hh:mm` or `-hh:mm`, and `:ss` after
     it to read back what 1305 writes. `DateTime.parse` takes the moment to UTC; `DateTimeOffset.parse` keeps
     the offset. Strict: only `T` and `Z`, nothing before or after, no date alone, and a second of 60 is
     `InvalidSecond` (there are no leap seconds). chrono's `parse_from_rfc3339` and Go's
     `time.Parse(time.RFC3339, ...)` read the same form.
1308. **Three errors more in `DateError`**: `InvalidText(nint position)`, as `NumberError.Invalid(position)`,
     `InvalidOffset` and `SkippedLocalTime`. A text of the right form with a wrong value gives the error
     `DateTime.utc` gives (`2026-02-30` is `InvalidDay`).
1309. **Zones by name** (`Europe/Istanbul`) are not in this slice ("Not here yet"); Windows has no files of
     the IANA database either.
1310. **Tests**: on Linux and macOS a program with an exact output runs with `TZ` set to the rules written out,
     `CET-1CEST,M3.5.0,M10.5.0/3`, which needs no files of zones in a container; Windows does not read `TZ`,
     so there a program checks the zone of the machine against itself, and the exact one is not run.

Approved on 18 November 2026:

1311. **A `DateTime` where a `DateTimeOffset` goes**, or the other way, is the error of two types, with a hint:
     `seen.toUtc()` for the moment in UTC, `d.toLocal()` or `d.withOffset(offset)` for a moment seen somewhere.
1312. **An offset with a part of a second is `InvalidOffset`** in `withOffset`, as no clock of a place shows
     one; `-00:00` is read as `+00:00`.
1313. **The order of the checks of `parse`**: the whole text first, so a text not of the form is
     `InvalidText` whatever its numbers say; then the parts in the order of `DateTime.utc` (the month first,
     the year's range last), then the offset, then the moment against the range of a `DateTime`, its fraction
     counted (`2262-04-11T23:47:16.854775808Z` is `OutOfRange`).
1314. **How `local` finds the moment**: it reads the offsets in force a day before, at and a day after the
     date and time counted as UTC, and keeps each moment, the date and time less one of them, at which that
     offset is in force; none is a skipped time, two a repeated one, of which it takes the earlier. Three
     readings, which hold while the clocks of a place change at most once in two days.
1315. **The rules of `TZ` before 1970**: macOS takes the rules of a `TZ` written out (`CET-1CEST,...`) only
     from 1970 on, and before that only the first offset; glibc takes them for every year. It is the C
     library's; a test gives such a year only a time in winter.
1316. **The field of the offset is `ahead`**, in seconds, so that `offset()` is the method, since a field and a
     method do not share a name; a program does not see it, as with every field of `std`.
1317. **Documents and tests**: this list, the table above, "Not here yet", "Next"; 02, 04 (the rows of the
     decision and "as built"), 06; the root README. Tests: `ok_time_offset` (offsets of your own, the parts
     of a place east and west of the date line, every form and every error of `parse`, the edges of the
     range, equality, order, a set, `plus` and `minus`, and the zone of the machine checked against itself),
     `local_time_zone` in `tests/golden.rs` (Linux and macOS: moments on both sides of both changes of the
     clocks, the skipped and the repeated hour, `plus` against the way through UTC, interpreted and compiled),
     and 3 `error_*` (a `DateTimeOffset` where a `DateTime` goes and the other way, `Clock.offsetAt` called by
     a program). `cargo test` passes on macOS and on Linux in a container (395 of 1639 programs compile
     through C, 90 of them under the thread sanitizer); on Windows on Arm `ok_time_offset` gives the same output
     interpreted and compiled, and the zone of the machine, Istanbul's, takes the rules of each year (2010 in
     summer `+03:00`). `dates` takes 0.126 s against 0.425 s for C, as before.

Choices made in the hundred-and-fifteenth slice (making it sturdier: a review of its safety, fuzzing, mutants,
and tests of the `sg` command). 1318 to 1325 were put to the author before the slice and approved on 18
November 2026:

1318. **The order**: this slice writes the tests of the command (1322) while the review (1319) runs beside
     it; the next one fuzzes (1323, 1324) and tries mutants (1325). What the review finds comes as choices of
     their own.
1319. **The review** is read-only, by four reviewers at once, each over one part: the soundness of the
     checker (programs it takes whose C is undefined), the generated C and its runtime, the boundary of
     `unsafe` and C, and the Rust side of the tool (its `unsafe`, signals, waits, and the files the command
     writes). Rust has Miri, cargo-geiger and RustSec for this; Go has gosec.
1320. **A finding counts only with a proof**: a program the checker takes, with a report of a sanitizer
     (address, undefined behaviour, threads) on its C or an output that differs between the interpreter and
     the compiled code, or a panic or crash of the tool. What could not be shown is listed apart and read,
     not counted.
1321. **Each finding is a choice**, with the fix put forward; once it is approved a test that shows it comes
     first, then the fix. Its details are not written anywhere public before the fix is.
1322. **Tests of the command** (`tests/cli.rs`): each command and flag on a way that works, what it writes
     and where, and its exit code; and the ways it fails. The codes are written down below.
1323. **Fuzzing the front end**: a crate of its own, `fuzz/` at the root, so `sg` keeps no dependencies,
     with cargo-fuzz (libFuzzer, Rust nightly) in a Linux container: the lexer, the parser and the checker
     never panic or hang on any input, starting from the programs of `tests/cases`.
1324. **Fuzzing for differences**: a generator of our own, as Csmith is for C, of small programs that are
     typed, end and have no undefined behaviour, its reach growing step by step; the interpreter's output
     must be that of the compiled program under the address and undefined-behaviour sanitizers, a panic's
     message and code with it. Runs of about two hours in a container.
1325. **Mutants** (cargo-mutants, a tool installed, not a dependency) only of the functions that make the
     rules of safety (moves, borrows, lifetimes, bounds in the checker; the checks of bounds and overflow the
     C gets), named in a list, then `--in-diff`; each mutant runs the quick part of the golden tests. A mutant
     that lives is a test that is missing.

What writing the tests of the command found, from 1326. Approved on 18 November 2026:

1326. **A second program, or a flag of `sg` given twice, is not a usage**: `sg build a.sg b.sg c.sg` built
     `c.sg` (the third word took the place the second had emptied), and `sg test --compiled --compiled` looked
     for a file named `--compiled`. Both now give the usage, code 2, as two programs already did; so do `-o`,
     `--target`, `--release` and the kind of build (`--test`, `--library`, ...) given twice. rustc refuses `-o`
     given twice; Go's flags keep the last.
1327. **`-o` and `--target` with nothing after them are the usage**: `sg build a.sg -o` built `a`, and
     `--target` at the end built for this machine, saying nothing.
1328. **`emit-c` with `-o` is the usage**: the `-o` was passed over and the C went to the output. `> file`
     writes it into a file; a second way to do that is left out.
1329. **An output into a folder that is not there is told before the C compiler runs**: `-o /no/such/x` gave
     the linker's message and then "this is a mistake in sg". Now `error: cannot write /no/such/x: there is no
     folder /no/such`, code 2, as a program that cannot be read has: both are paths of the command line.
1330. **An output closed early ends a program the same way interpreted and compiled**: on Linux and macOS a
     compiled program is ended by `SIGPIPE`, as C and Go programs are (a shell shows 141, which `set -o
     pipefail` sees), while the interpreter, which Rust starts with that signal ignored, ended with 0. The
     interpreter now sets the signal back and raises it when its output is closed; on Windows, which has no
     such signal, both end with 0. Nothing changes in compiled code.
1331. **Documents and tests**: this list, the table of exit codes under "How it is built", "Next"; 04 (the row of
     `print` and "The `sg` command, as built"), 06, the root README. `tests/cli.rs` has 10 tests: running a
     program with its words and `-L`, the codes of a run, an output closed early (interpreted and compiled),
     the test command interpreted and compiled, building a program, a library (static and shared) and an
     object for a machine without a system, the command lines refused, an output into a folder that is not
     there, and `emit-c`. `cargo test` passes on macOS and on Linux in a container (395 of 1639 programs compile
     through C, 90 of them under the thread sanitizer); on Windows on Arm `tests/cli.rs` passes.

Choices made in the hundred-and-sixteenth slice (fixing what the review of 1319 found). The review found 36
faults, each shown by a program; the fixes were put to the author as choices and approved on 18 November 2026.
Each is written here with its fix, and each fix came after a test that showed the fault:

1332. **The order**: the fixes come first, fuzzing and mutants (1323-1325) after them. This slice fixes the
     small faults, each in one place; the next two the rules of borrowing, whose faults share one cause.
1333. **The files a build makes on the way are its own**: `sg build` wrote its C at `sg-<number of the
     process>.c` in the temporary folder, put the object of a static library in `sg-<number>`, and `sg test
     --compiled` built and ran its program at `sg-test-<number>`. Where the temporary folder is shared
     (`/tmp`), another user who guessed the number could put a link there, so the C was written over a file
     of the user's, or a pipe, so the C compiler read the other user's C and the tests ran it. Now each
     build makes a folder of its own there, with a name no one can guess, that only its user opens (`0700`;
     on Windows a list of access that gives only its owner anything), made new and never one that was there
     already, and removes it at the end: the C, the object, the manifest of Windows and the program of the
     tests are in it. The wrappers of `extern` blocks the interpreter builds were in such a folder already.
     Rust's `tempfile` and Go's `os.MkdirTemp` make folders the same way. Tested in `tests/cli.rs`
     (`keeps_its_temporary_files_to_itself`: nothing is left in the temporary folder, and on Linux and macOS
     links and a file planted at the old names are not written through).
1334. **An `export` function is called from any thread, and its name is one C does not have already**.
     A program or a library that started no task of its own was built for one thread (`SG_THREADS 0`): the
     runtime's lists of values and its scratch had no locks, so a program of C calling a library from two
     threads, or a library of C calling back into a program from a thread of its own, broke the heap. Now
     every program and library with an `export` function is built for several threads, as one with tasks
     is. A lock is taken where the runtime shares something; a library that writes a list into a text, called
     two million times from one thread, takes the same time either way (1.86 s on macOS). And an `export` function
     took the name of a function of C the runtime calls (`export raw void realloc(...)` replaced C's
     `realloc` in the whole program, under the runtime's texts) or a header it includes declares, or a
     prefix of the names the code SG writes (`f_main` is the `main` of SG): these are errors now, beside
     `sg_` and the words of C. The names the runtime calls are read from its C, so the list keeps up with
     it; the standard functions of the headers of C it includes are listed. Rust lets `#[no_mangle]`
     replace `malloc` (written `#[unsafe(no_mangle)]` since Rust 2024); in Go an exported name C has
     already fails when the C is compiled or linked. Tested by
     3 `error_*` (a name of C's library the runtime calls, one a header declares, a prefix) and in
     `libraries` in `tests/golden.rs` (a library with no task called from four threads of C under the
     thread sanitizer, and the C of a library and of a program with an `export` function built for several
     threads). `cargo test` passes on macOS and on Linux in a container (395 of 1642 programs compile through C,
     96 of them under the thread sanitizer: the 6 with an `export` function are new there); on Windows on Arm
     `libraries` passes.
1335. **A struct with no fields holds one byte**, as in C++, in compiled code, in the header of a library
     and in the interpreter: `Memory.sizeOf<Marker>()` is 1. C has no struct without fields: GCC and Clang
     gave it the size 0, so a list of them that grew from an array (`addAll`) divided by 0, which C leaves
     undefined (on macOS on Arm the program ended with no message), and a block grown to 0 bytes may be `NULL`
     (glibc), which the runtime takes for memory that ran out; Microsoft's compiler gives it 4, so a struct
     that held one was laid out otherwise there than in the interpreter; and a program of C++ that read the
     header laid out a struct holding one otherwise than the library. Now C gets `char sg_empty;` in it.
     Rust gives it the size 0, and handles a list of such values apart; one byte keeps every list and
     every layout of SG as they are. Tested by `ok_empty_struct` (a list of them grown, reversed and
     emptied, and a struct that holds them, with their sizes) and in `tests/layouts.rs` (the layout of a
     struct with no fields and of one that holds it, against the C compilers of every system; without the fix
     it fails). `cargo test` passes on macOS and on Linux in a container (396 of 1643 programs compile through
     C); on Windows on Arm `ok_empty_struct` gives the same output interpreted and compiled, and `layouts` passes.
1336. **A line is written with the lengths of its texts**: `print`, `printError`, `panic`, `$"...".toString()`
     and `f.write($"...")` gave their formats to C's `printf` and `vsnprintf`, a text as `%.*s` with its
     length cast to `int`. A text with the byte 0 in it was cut there in compiled code (the interpreter
     writes all of it, and a text from a file or a socket may hold one), a text of 2 GB or more had a
     negative length, which C reads as "up to the byte 0", and a line of 2 GB or more gave `vsnprintf` -1,
     which `f.write` took for a length and wrote in front of its block. Now the runtime writes those lines
     itself (`sg_vlines`): only the conversions the code SG writes uses (`%lld`, `%llu`, `%llx`, `%llX`
     with the digits they have at least, `%s`, `%.*s` with an `int64_t` length, a width counted in
     characters, `%%`), each piece given with its length to where the line goes; a `print` gathers its line
     on the stack and writes it with one `fwrite`, holding the stream where several threads run, so a line
     is never cut by another. The texts the runtime writes into a panic of `assert`, a key of a map and the
     error of a test go the same way. On Windows a text of more than a gigabyte becomes UTF-16 in pieces
     (`MultiByteToWideChar` counts in `int`), and a name of a variable longer than any Windows keeps is no
     variable. It is faster too, since a format is no longer read by `printf` for every line: on macOS `show`
     takes 0.47 times C (0.57 before), `floats` 0.23 (0.33), `copy` 0.54 (0.83), `table` 0.52 (0.89); on
     Windows on Arm 0.34, 0.25, 0.18 and 0.76 (0.37, 0.35, 0.28, 0.94); the others do not change. Rust's
     `format!` and Go's `fmt` write a text with its length too; a text of C# holds at most about a billion
     characters.
     Tested by `ok_text_with_zero` (a text with the byte 0 printed, with widths, written into a text, made
     with `toString`, written by a `format`, used as a key, and sent to the standard error, with hex and
     digits beside it). The texts of 2 GB and more were checked by hand (`"a".repeat(2147483648)` made into a
     text with `$"{s}"`, and three texts of a gigabyte written into one with `f.write`): each gives its
     length; they need more than 4 GB of memory, too much for `cargo test`. `cargo test` passes on macOS and on
     Linux in a container (397 of 1644 programs compile through C); on Windows on Arm fourteen programs that
     print, format, and name files, folders, variables and programs give the same output interpreted and compiled.
1337. **The tool does not stop on its own faults**: eight places where a program made `sg` itself panic,
     overflow its stack or wait for ever, each now an error, a panic of the program, or what was meant.
     (1) A mark under a character of more than one byte: an unknown escape (`"a\éb"`) and a line of a raw
     text that starts with one marked a single byte, and writing the message cut the character, so `sg`
     panicked instead of saying what was wrong; the lexer now marks the whole character, and the message
     rounds any mark to whole characters. (2) A struct that holds itself directly (`struct Point { Point x
     }`), or two that hold each other, overflowed the stack of the checker, whose rules of layout walked
     the fields before the check that finds a type of no finite size; that check now comes first, and the
     error is the one an enum already had. (3) `p + int.MaxValue` and `p + (nint)Level.High` on a raw pointer
     checked the count twice, the second time after the checker had rewritten it, and `sg` panicked; it is
     checked once, as an `nint`. (4) The header of a library whose plain struct points at itself (`raw Node
     next`) overflowed the stack of `sg build --library`; the header now names first, with a `typedef`, a
     plain struct that one written before it points at, and writes each after those it holds by value. (5)
     A `compare` that does not put its values in one order made the interpreter panic in Rust's sort
     (compiled code sorted them); the interpreter now sorts as compiled code does, the same merge sort step
     for step, so both leave the same items in the same places and neither panics. (6) A thread the system
     refused for reading the input, connecting, looking up a host, waiting for a program or copying its
     output made the interpreter panic in Rust and left the run waiting for the thread; it is now a panic of
     the program at the call (`the system refused a thread for connecting`), as for a task, and a program
     started is stopped first. (7) `Path.joinInside` cut a name of four or five bytes whose first
     characters take more than one (`ğğ`, `😀`) to look for `COM1` and `LPT1`, and panicked; the first three
     bytes are looked at one by one first. (8) The hint of a conversion to `CLong` or `CULong` that does not
     fit named `tryTo#ULong()` in compiled code; it names `tryToCULong()`, as the interpreter did. Rust's
     `sort_by` may panic on such a `compare` since Rust 1.81; C's `qsort` leaves it undefined; Go's
     `sort.Slice` gives some order and does not panic. Tested by `error_escape_wide_char`, `error_raw_text_indent_wide`,
     `error_struct_contains_itself_directly`, `_in_array` and `_packed`, `ok_raw_move_by_named`,
     `ok_sort_no_order` (the same order interpreted and compiled), `panic_cast_c_ulong`, names of several
     bytes in `ok_path_join_inside`, and in `libraries` in `tests/golden.rs` (plain structs that point at
     themselves and at each other, and a union, in a header C compiles with `-pedantic`); each of them fails
     without its fix. The refused thread was checked by hand, since the number of threads a system gives
     depends on the machine: on macOS 12 286 tasks that wait leave no thread for a connect, which is now
     that panic, and the run ends. `cargo test` passes on macOS and on Linux in a container (400 of 1652
     programs compile through C, 96 of them under the thread sanitizer); on Windows on Arm the new programs give
     the same output (interpreted and compiled where they run), and `libraries` passes.
1338. **Limits, each said where it is met**: what had no bound made `sg` panic, overflow its stack, wait for
     ever, or take minutes and gigabytes. (1) A value too big for any machine (`byte[9223372036854775807]`,
     or a struct of two fields that each fit) made the interpreter panic in Rust, and wait for ever when a
     task was running; the C compiler refused it, and `Memory.sizeOf` wrapped around to 0. It is an error
     now (1341). An array the checker allows but the memory does not hold is a panic of the program in the
     interpreter (`there is no memory for an array of ... items`), as a text repeated too often already was.
     (2) A fault of the interpreter itself, while a task waited, left the run waiting for the task's thread;
     the tasks now end first, and the fault goes on as it is. No program is known to reach one now; this was
     checked with a fault put in by hand. (3) A source nested 100 000 deep overflowed the stack of the
     parser or the checker, and 10 000 nested `if` blocks took 2.6 GB; a program nests at most 1000 deep
     (1342), and a chain of 100 000 names (`a.b.b...`) took 10 GB before it was read, since every start of
     the chain was tried as a module: those are taken only as long as a program may nest. (4) Writing or
     comparing a value nested a million deep (a list of links) overflowed the interpreter's stack; it is a
     panic of the program now (1343). (5) A const grew to what the memory held before its limit of 64 MB was
     looked at: `"ab".repeat(1 << 33)` took 25 GB and 17 seconds before its error. Now a text repeated, a text
     made while a const is found and a list grown by `add` and `addAll` stop when they pass 64 MB, with the
     error the checker gives a value found too big. (6) A function whose values took more than the stack of
     its thread wrote past the stack, into memory that is not its own (1344, 1345). Tested by
     `error_value_too_big`, `error_struct_too_big`, `error_field_too_big`, `panic_array_no_memory`,
     `error_nested_too_deep`, `error_const_too_big_repeat`, `error_const_too_big_text` and
     `panic_deep_value_written` (the two panics are the interpreter's: compiled code makes the value on its
     stack, which ends at the guard page, as in the open stack depth question), and in `tests/golden.rs`
     `deep_sources_are_errors` (seven shapes nested 100 000 deep, each refused in under ten seconds) and
     `large_frames_touch_their_stack` (it fails on Linux without the touching). `cargo test` passes on macOS and on
     Linux in a container (400 of 1660 programs compile through C, 96 of them under the thread sanitizer); on
     Windows on Arm the new programs give the same output, and the two new tests pass.
1341. **A value takes at most 2^61 - 1 bytes**, as in Rust: the largest object whose addresses pointer
     arithmetic counts right is `isize::MAX`, and Clang lets an array have at most 2^61 bytes on a machine
     of 64 bits, so that is the bound. A type whose values would take more (an array, a struct, an enum, a
     `T?`) is an error where it is written, a struct's at the field too big on its own or at its name, with
     the hint of a `List<T>`. The sizes are counted as C lays the values out, padding included, a field of
     bits as its whole type and a tag as eight bytes, so the bound is never reached by a value smaller than
     it. A machine of 32 bits would have 2^31 - 1; SG has none yet (906). Rust says *values of the type ...
     are too big for the target architecture*; GCC's bound is `PTRDIFF_MAX`; Go has a bound of its own too.
1342. **A program nests at most 1000 deep**: blocks, expressions, the operators of a chain (`a + b + c` is
     `(a + b) + c`, one deeper with each), the fields, calls and indexes read one after another, unary
     operators, types and patterns, counted together. Beyond that the error is *this is nested more than
     1000 deep*, with the hint to give a part a name or a function of its own. A program written by hand
     does not come near it, and one a tool writes does not either but for a long sum, which a variable or a
     loop splits. Rust and Go have no such bound, and grow the stack of the compiler instead; C# gives
     *an expression is too long or complex to compile*; Clang allows 256 nested brackets.
1343. **The interpreter watches its stack**: each thread of it notes where its stack starts, and writing,
     comparing and ordering a value look at how much of it is used before they go one level deeper. When
     less than 64 MB of its gigabyte is left, it is a panic of the program at the line (*stack overflow:
     the value is nested too deep to be written here*), noting that a loop of the program walks such a value
     link by link. The count of 5000 nested calls stays. Comparing a list of three million links still
     works.
1344. **A large frame is touched before it is used**: a function whose values took more than the stack of
     its thread had its whole frame made at once, below the page the system leaves free under a stack, so
     what the function called ran in memory that is not its own and wrote there: on Linux, where the
     stacks of threads lie next to each other, a task whose function held a `byte[10000000]` in a branch
     that never ran wrote 64 KB of another thread's stack and the program went on (with GCC and Clang, in
     debug and release builds). GCC and Clang touch such a frame page by page on Linux with
     `-fstack-clash-protection`, which `sg build` gives them now; the program then ends at that page, as
     deep recursion does. Apple's Clang does it on macOS on its own, and so does the compiler on Windows
     (`__chkstk`).
1345. **And SG touches it itself**, since a Clang that is not Apple's does not on macOS: a function whose
     values may take a page (4096 bytes) of its stack or more (its parameters, its variables, and each
     struct, array or enum it makes, counted as 1341 counts them) is called through a small one that takes
     that room and 64 KB more below itself and touches it a page at a time from the top
     (`sg_probe_stack`), then calls the body, which C is told not to inline into it. It costs a call and a
     touch per page in those functions only; the others are as they were. Rust's stack probes do the same
     on every machine.

1339. **Paths and the linker, each taken as written**: five faults where a name or a path from outside went
     further than it should. (1) `embed("../../secret.txt")` and `embed("/etc/passwd")` read any file of the
     machine that builds the program into it, though a C file (`extern "./native.c"`) had a rule against `..`
     already: a module of someone else's could carry out a key. Now a path of `embed` is written from the
     folder of its file with `/`, without `..`, `.`, an empty part, a `\` or a `:` (as Go's `//go:embed` takes
     it), and a file of `embed` or a C file must lie in that folder or below it also where a link leads;
     otherwise *`token.txt` lies outside the folder of this file*, with where it leads. (2) `sg build
     --shared` gave the linker the name of the library after `-Wl,`, where a comma starts another option: a
     library named `v,-map,PLANTED` made the linker of macOS write a file `PLANTED`. The names, of the library
     on macOS and of the import library and the manifest on Windows, go to the linker whole with `-Xlinker`
     now. (3) On Windows the names of the DLLs read out of an import library or a DLL were joined to folders
     as they were, so a name with `..` in it was copied from and written to a place outside them; only a name
     of a file is taken now, and a name that points past the end of the file no longer panics. (4) A task of
     the interpreter let go of its task after the turn had gone on, while the task that awaited it could let
     the task itself go on its own thread; the counts of the value are not counted across threads, so they
     could be wrong. It lets go while it has the turn now, on every way a task ends. (5) The interpreter lent
     C a struct of `align(64)` (by `read`, by `ref`, as the items of a buffer) at a multiple of 16, though C
     is told the type lies at a multiple of 64 and may count on it; it lends it at the alignment of its type
     now, as compiled code does. Tested by `error_const_embed_up`, `_absolute` and `_drive`,
     `ok_async_many_ends` (the thread sanitizer saw the fault in each of three runs of a longer one, and in
     none of six after the fix), the module `aligned` (each address C is given is a multiple of 64; 752 bytes
     off without the fix), in `tests/cli.rs` `a_comma_in_a_name_goes_to_the_linker_whole` and
     `takes_in_no_file_from_outside_its_folder` (links to a file outside, from the folder and from one below
     it, by `embed` and as a C file), and in `src/link.rs` `copies_only_dlls_by_name`; each fails without its
     fix but `ok_async_many_ends`, which only the thread sanitizer tells apart. `cargo test` passes on macOS
     and on Linux in a container (402 of 1665 programs compile through C, 97 of them under the thread
     sanitizer); on Windows on Arm the new tests pass, the linker taking the names given with `-Xlinker`.
1346. **A frame no stack holds is an error of `sg build`**, found when 1338 ran on Windows on Arm: there the C
     compiler fails (*Stack size cannot exceed 256MB*) on a function whose frame passes 256 MB, such as the
     `byte[1000000000000000]` of `panic_array_no_memory`, and `sg build` called that a mistake of its own.
     Such a function could run nowhere, since a thread of SG has 8 MB of stack. Now a function whose values
     may take more than 256 MB of its stack, counted as 1345 counts them, is an error on every machine, at its
     name, with the hint of a `List<T>`; the interpreter, which keeps them on its heap, is as it was. Rust and
     C build it and it ends at the guard page when it runs; Go moves a large value to the heap itself. Tested
     in `tests/cli.rs` (`refuses_a_frame_no_stack_holds`: two arrays of 150 MB are refused together, one of
     100 MB is built); on Windows on Arm `needs_nothing_of_128_bits`, which failed there, passes again.

Choices made in the hundred-and-seventeenth slice (the rules of borrowing, first part). 1340 was approved with
the other fixes of the review on 18 November 2026; each of its parts is written here when it is fixed, with
the choices it raised:

1340. **The rules of borrowing, where the review found them weak**: eleven faults of the checker, each a
     program it took that read freed memory in compiled code or made the interpreter panic. They share one
     cause: a borrow was followed through the variables that hold it, and lost where no variable held it.
     Fixed so far:
     (1) **What a part of a call lends lasts until the call runs.** `f(read xs[0], g(ref xs))` lent the first
     item, `g` emptied the list, and `f` read the freed text; `consume(read p, p)` gave `p` away while lending
     it, `p.eat(p.a)` moved a field out of the value `eat` changes, `both(ref r, ref xs)` lent `xs` twice as
     `ref`, once through `var r = ref xs`, and `s[change(ref s)..]` cut a text that its bound had replaced.
     The parts of a call (the value a method is called on, then the arguments) and of an index or a range
     (the value looked into, then the bounds) are computed from left to right as before, and what an earlier
     part lends (a `read` or `ref` borrow, a variable that holds one, a value that carries one, the value a
     method looks at) cannot be changed or moved by a later part: *`xs` cannot be changed here: `read xs[0]`
     is given to this call before it, and the call still uses it*, with the place where it was lent and the
     hint to compute the later part first. A later part may read it (`xs.add(xs.length())`, `addTwo(ref xs,
     xs.length())`), and a borrow that a part makes ends with that part. This is Rust's rule, with its
     two-phase borrows. Tested by `error_call_lent_then_changed`, `_lent_then_moved`, `_receiver_then_changed`,
     `_receiver_field_moved`, `_borrow_var_then_changed`, `_ref_alias`, `error_range_bound_changes_text` and
     `ok_call_parts_in_order`; the checker took each of the errors before.
     (2) **An item a loop lends borrows the collection.** `for x in read local { return x }` returned a
     borrow of a list that was gone after the return, `r = read x` kept an item past the loop while the list
     was emptied, and `for x in s`, where `var s = xs[0..2]`, let the body empty `xs`: the item borrowed
     nothing. Now it borrows what the collection is (the variable, or what a borrow of it borrowed), so the
     returned borrow, the kept item and the emptied list are the usual errors; the list a borrow of it
     reaches is kept from changing while the loop runs, as the variable gone over is (*`xs` cannot be
     changed or moved while a `for` goes over `s`*); and an owned item carries what the items borrowed.
     Through a `shared` value each access is checked as before. (3) **Borrows join where ways join.** After
     an `if`, a `match` or a loop a variable borrows what it borrowed on any way that reaches there, as a
     value is moved if any way moved it; before, the last branch read decided (`var r = read a`, `if ... { r
     = read b }`, then `a` changed and `r` read). A borrow that a round of a loop leaves in a variable from
     outside is held when the next round starts, so what that round does to the place is an error when the
     variable is used again, with a note that the loop comes round. Like the rest of the check, a variable
     given another borrow still counts as holding the first until its last use. (4) **Writing through a
     `ref` variable uses it**: `r = value`, with `var r = ref s`, did not count as a use, so `s` could be lent
     to another before it (`var v = read s`), or tested for `null` and then emptied through `r`. Tested by
     `error_for_item_returned`, `_item_kept_after_loop`, `_item_of_unnamed`, `_over_range_changed`,
     `_ref_item_list_read`, `_ref_item_nested_read`, `error_branch_borrows_joined`, `_carrier_joined`,
     `error_loop_borrow_next_round`, `error_ref_var_written_later`, `_written_after_test` and
     `ok_for_item_borrows`; the checker took each of the errors before. (5) **What `lock` and `read ... as`
     open is lent for the block only.** `lock cell as c { r = read c }` let `r`, from outside, keep looking at
     the locked text after the lock was let go, when another `lock` gave it a new text and freed the old one;
     a function could return `c`, and `read a as c` let a borrow of a `shared checked` value outlive the
     block while another owner changed it. The name now counts as a value that is gone at the end of its
     block: *`r` borrows from `c` here, but `c` is closed at the end of its block and `r` is used after
     that*, with the hint to copy what is needed there; returning it is an error with the hint `.clone()`.
     Tested by `error_lock_name_kept`, `_returned`, `error_as_name_kept` and `ok_lock_name_copied`. `cargo test` passes on
     macOS (405 of 1691 programs compile through C, 98 of them under the thread sanitizer); only the checker
     changed, so it was not run again on Linux and Windows.
     (6) **A `ref` field reached through `read` only reads, and a change through it changes what it
     borrowed.** `void clear(read this) { this.items.clear() }`, where `items` is a `ref List<string>`,
     changed the list through a struct that was only looked at (a field, an item, a method that changes,
     a range); now that is the error a `read` borrow gives, as `&&mut T` only reads in Rust. And `var v =
     read w.items[0]` followed by `w.items.clear()` freed the text `v` looked at, since the change
     touched `w` and not the list `w.items` borrowed; it touches that list now. (7) **Every value that
     carries a borrow says so**: the items of a list being built (`List<Word> { Word { text = read text }
     }`) carry what they borrow, as a struct being built does; what a value borrows is written out for
     each kind of expression, with no case left to a default. (8) **What a collection keeps, it keeps
     all of**: `add` of a list put only the last item's borrow in the list, `getOrAdd`, the key of
     `HashMap.add` and `HashSet.add` none, and writing an item (`xs[i] = w`) forgot what the other items
     borrowed; each now adds to what the collection borrows. (9) **A `Disposable` value that borrows
     uses the borrow where its block ends**, since its `dispose` runs there: the owner cannot be changed
     before that, and an owner of the same block declared after it, which is gone first, is an error
     (*declare `text` before `g`*). (10) **A range of an array in a `packed` struct is not taken**: it
     would be a slice whose items lie where their type cannot be reached, as a field there is not lent.
     (11) **What a `shared checked` value holds, looked at, is lent until its line ends.** A text, a list
     or a struct in it that a line looks at where it lies (`$"{a.name} {grow(read b)}"`, `a.name ==
     ...`, `a.name[grow(read b)]`) was opened only for a look, so a call later in the line that changed
     it through another owner freed what was being looked at; now the line holds it, and the change
     stops the program with the usual panic, compiled and interpreted alike. A number read out is still
     only looked at, and so is the value an item is read out of once the item is read. And a borrow of it
     put into a collection (`all.add(Word { text = read a.name })`), which outlives the line, is an error
     with the hint `.clone()`. Tested by `error_ref_field_through_read`, `_item_through_read`,
     `_change_clashes`, `ok_ref_field_changes`, `error_collection_literal_outlives`, `_changed`,
     `error_list_add_keeps_every_borrow`, `error_set_add_keeps_borrow`, `error_map_get_or_add_keeps_borrow`,
     `error_dispose_uses_borrow`, `_owner_declared_after`, `error_packed_field_range`,
     `error_checked_borrow_kept_by_list`, `panic_checked_text_looked_at`, `_text_indexed` and
     `_item_looked_at`; each was taken before, or ran into freed memory compiled.
     `cargo test` passes on macOS and on Linux in a container (409 of 1707 programs compile through C, 98
     of them under the thread sanitizer); on Linux `ok_task_list` once came out of order under the thread
     sanitizer while the macOS tests ran beside it, and passed again alone.

1347. **No exception for plain data**, approved on 19 November 2026: a later part that changes what an earlier
     one lends is an error also where the change frees nothing, as with a local struct of numbers in
     `counter.plus(counter.next(), counter.next())`, where compiled code gave `plus` the value after the
     arguments. `ok_evaluation_order` had that line; it shows the receiver computed first with a receiver made
     by a call now. Rust refuses it too (E0502). One rule also covers the texts: a `read string` is given as
     where its bytes are, so `show(read s, change(ref s))` read bytes that `change` had freed. Tested by
     `error_call_receiver_changed_by_argument` and `error_call_text_lent_then_changed`. `cargo test` passes on
     macOS (403 of 1675 programs compile through C, 97 of them under the thread sanitizer); only the checker
     changed, so it was not run again on Linux and Windows.

1348. **The receiver of a built-in method is computed before its arguments in compiled code too**, approved
     on 19 November 2026. Found while testing (11): an argument whose value is kept in a temporary until the
     statement ends (`a.name.startsWith(read mk(read b))`, where `mk` returns a new text) was computed on a
     line of its own before the statement, so before the receiver was found and lent: the interpreter
     stopped at the change `mk` makes through `b`, another owner of `a`, and the compiled program did not (it
     read the new text; nothing was read after it was freed). The same for the other methods of a text
     that take a text, for `write` (`a.name.write($"{b.name}")` named the other access), for the methods
     of a map and a set, for the searches of a list, an array or a window for an item compared where it
     is, for `send` and for the atomic operations, and also where no `shared checked` value is in play: a
     receiver given by a call (`holder.name().contains(read word("b"))`) ran after the argument, and the
     second argument of `replaceAll` before the first. Now the receiver goes into a temporary before such an
     argument, as `in_order` does for the arguments of a call, and the arguments of these methods are
     computed in order; it costs a copy of a pointer or of a look at text, and only there. Tested by
     `ok_method_receiver_first`, `panic_method_receiver_lent` and `panic_text_write_lent`. `cargo test`
     passes on macOS and on Linux in a container (412 of 1710 programs compile through C, 99 of them under
     the thread sanitizer).

Choices made in the hundred-and-eighteenth slice (fuzzing the front end, 1323, and mutants of the rules
of safety, 1325). Approved on 19 November 2026:

1349. **The order**: this slice fuzzes the front end (1323) and then tries mutants (1325); fuzzing for
     differences (1324), with a generator of programs of our own, is the largest piece and is the next slice.
1350. **Two targets** (`fuzz/`): `parse`, the lexer and the parser; and `check`, all `sg build` does before
     the C compiler runs (the parser, the checker, the consts it finds and the C written), on a stack as
     large as the one `sg` gives them. The interpreter does not run there; that is 1324's. An input that is
     not UTF-8 is passed over, as `sg` does not read such a file.
1351. **What counts as a finding**: a panic of Rust, a stack run out, more than 2 GB of memory, or an input
     that takes more than 10 seconds. The targets are built with debug assertions, so an overflow a release
     build of `sg` would wrap without a word is a panic there. A message about the program is the answer
     expected.
1352. **A const takes at most 1 000 000 steps while it is fuzzed** (`cfg(fuzzing)`, which cargo-fuzz sets):
     a hundred million take about 9 seconds, and the fuzzer would count each such input as one that hangs.
     `sg` itself is built as before. The steps stay counted for each const, as Rust counts them for each
     const (`long_running_const_eval`) and Zig for each block run when it compiles (`@setEvalBranchQuota`).
1353. **Where the fuzzer starts**: from the programs of `tests/cases` and `examples`, with the words and
     marks of the language in `fuzz/sg.dict`. The corpus it grows is kept in a volume of the container, not
     in the repository. An input that fails is made smaller (`cargo fuzz tmin`) and becomes a test before
     the fix (1321).
1354. **How it is set up**: `fuzz/` is a crate of its own, not in a workspace with `sg`; its one dependency,
     libfuzzer-sys, has its version pinned and its `Cargo.lock` kept, and Dependabot watches it. The image of
     `fuzz/Containerfile` pins Rust nightly to a day and cargo-fuzz to a version and fetches the crates; only
     building it uses the network, and the fuzzing runs without it (`fuzz/run.sh`). It is not in the CI,
     which runs on Windows and with stable Rust.
1355. **How long**: three hours of `check` and one of `parse`, six at once; a short run again after a fix.
1356. **Which functions mutants are tried on**, named in `.cargo/mutants.toml`: those of the checker that
     follow moves and borrows and what a borrow outlives, all of `src/opening.rs`, all of `src/sums.rs`,
     and the functions of the back end that leave a check of bounds or of overflow out or lend a value; 677
     mutants in all. `Emitter::value` (533 mutants alone) and the rules of types are left out.
     cargo-mutants changes the Rust of `sg`, not the C it writes, so a check written as C is tried through
     the Rust that decides when it is left out.
1357. **Which tests run on each mutant**: on those of the checker and of `opening`, the golden tests in the
     interpreter (`--exact golden`, 34 seconds), where every `error_*` program is; on those of the back end,
     the compiled golden tests without the sanitizers (`--exact compiled`, about two minutes).
1358. **Where**: on macOS, four at once; the fuzzing runs beside it in the container.
1359. **A mutant that lives** is read: a missing test gets written; a mutant that changes nothing a
     program can see is left out in `.cargo/mutants.toml` with the reason; a way of the code that is not
     needed comes as a choice.
1360. **After this slice**, a change to these functions is tried only where it touched them: `git diff |
     cargo mutants --in-diff`. It is not in the CI.

What fuzzing the front end found, from 1377. The first run gave `check` three hours (15 845 031 inputs)
and `parse` one (30 086 188), six at once: no input hung or took more than 2 GB, the parser never panicked,
and `check` found one fault, approved on 19 November 2026 (after the fixes of this slice, half an hour more of
`check`, 2 747 192 inputs, found nothing):

1377. **An end of a channel behind a pointer** (`new jobs.sender()` is a `Sender<int>*`): the checker took
     `tx.send(1)`, `await tx.sendAsync(1)` and `jobs.receive()` through the pointer and the interpreter ran
     them, but `sg build` stopped with a fault of its own (the back end looked at the type without going
     through the pointer, which the address it computes already did). Fixing it showed two more of the same:
     `tx.clone()` counted the copy as an end that receives, so compiled code never let the channel go (a
     leak its debug build reported), and the temporary that holds the end of a send had the C type of one
     pointer too many, which the C compiler took with a warning. The back end now looks through the
     pointers in all four places, as it does for a `List<T>*`; Rust's `Box<Sender<T>>` reaches the methods
     the same way. Tested by `ok_channel_behind_pointer` and `ok_channel_behind_pointer_async`, with which `sg
     emit-c` stopped before the fix; compiled, under the sanitizers, they print what the interpreter prints.

What trying mutants found, from 1378. Approved on 19 November 2026:

1378. **How far the living mutants are followed**: read in four groups, each by a reviewer of its own
     (borrows of the parts of a call, the other borrows, moves and scopes, `opening.rs`), a test written for each
     one a program can tell, and the rest listed as equivalent or out of reach with the reason, once. After this,
     only `--in-diff` (1360). Most of what lives is equivalent, and following each costs more than it finds.

The first runs: in the checker and `opening.rs`, 250 mutants were caught, 108 lived, 3 hung and 161 did not
compile; in the back end, 23 were caught, 62 lived and 70 did not compile. The tests written for them, 75 in
all, are what the slice adds to `tests/cases`:

- **The sums that cannot overflow** (`sums.rs`) had no test of a loop whose sum does overflow, so a mutant that
  took the test out of a loop where it is needed lived: compiled code would have wrapped where the interpreter
  panics. Nine `panic_sum_loop_*` programs overflow at the narrow step that a wrong count would leave without its
  test: a narrower value added or taken twice a round, a number, a negative number, a sum the loop also assigns,
  a step in an arm of a `match`, a sum lent as `ref`, a loop inside the body, and steps whose sum over a round
  does not fit in a `long`. The body holds no call and no formatted text, so the loop is split (a first version printed in the loop, which
  `sums.rs` does not split, and caught nothing). With three more (a negative `sbyte` taken, a sum with a step of no
  small bound, steps below a `long`), 39 of the 61 mutants of `sums.rs` are caught, where none was; the 20 that live
  keep a test that could go, so they make a loop slower and not wrong.
- **A static that would never be closed** was tested only for a writer of its own: `error_static_disposable`,
  `error_static_writer_inside` (a field, a list, `shared`) and `error_static_writer_in_enum`.
- **The checker**: 57 tests from the four groups, among them a field moved out through a `read` or a `ref`
  parameter, a part moved in one branch of a `match` and not the other, a `Copyable` receiver changed by an
  argument, a pointer given to a call that a later part changes, a borrow lent whole and through a field in one
  call, the next round of a loop that changes what a borrow of the round before holds, and the methods of a
  `shared checked` value inside `read ... as` and `ref ... as`. With them, 291 mutants of the checker and
  `opening.rs` are caught, 3 hang and 27 live, from 108.

Four faults of the checker came out of reading them, approved on 19 November 2026:

1379. **`ref a`, `a` a `shared checked` value, lent it to be read only.** Where a `ref T` goes, the checker
     wraps the borrow in the node that reaches what the value holds, and left that node without a type, so
     `opening.rs` took the borrow for a read. Through the same variable, `read a as v { reset(ref a) }` was
     taken; through another owner the change did not stop the program, and compiled code then read a text the
     change had freed (`var head = seen.name[0..9]`, then `rename(ref other)`, then `print(head)`, found with
     the address sanitizer). The node now has the type it stands for, in the three places it is made. Tested
     by `error_checked_ref_whole_in_block` and `panic_checked_ref_whole_through_other`.
1380. **`<`, `<=`, `>` and `>=` on two `shared checked` values** compare what they hold, as `==` and `!=` do,
     so they look through the check too: `a < b` read `a` while another owner changed it. Only the interpreter
     had the fault; compiled code does not take the comparison yet. Tested by `panic_checked_order_while_changed`.
1381. **A borrow of the round before** is named so also where the borrow and the change start at the same place
     (`later(Window { part = buffer.all() }, view)` in a loop), as `error_loop_borrow_same_call_next_round` shows.
1382. **What a part of an index lends lasts until the index runs, also where its value is only looked at**:
     `print(s[cut(ref s)])`, where `cut` gives `s` a new text, was taken (`var c = s[cut(ref s)]` was refused), and
     compiled code read the old text after it was freed. The rule of 1340 and 1347 now holds on that way as well.
     And the note says "the parts of an index". Tested by `error_index_part_changes_text_looked_at`.

And one of the back end, found when the tests above were built under `-Werror`, approved on 19 November 2026:

1391. **A method that takes `this` by value, called on a `shared` or a `shared checked` value**, was given the
     address of the box where C wants the value, which the C compiler refuses: `sg build` could not build a
     program the checker takes. The value is now copied out of its box, as it is out of a `T*`. Tested by
     `panic_checked_value_method_while_changed`, which panics where the interpreter does.

Equivalent or out of reach, read and left: the arms of `held` for `shared` around `shared`, a slice or `void`,
which the parser and the rules of aliases refuse first; the guards whose arm gives what the next one gives
(`part_loans` and `unnamed_owner` on a borrow, `lent_by` on a place); the count of loops in `loop_body`, which only
a `break` outside a loop would see and which is refused before; the order of points of the same `seq`; and in the
back end the mutants that only keep a test that could go (`sums` giving nothing, a step whose bound is not
found), and the indentation of the copies of a loop. `opening.rs`'s guard of a comparison became equivalent with
1380, and the arms of `loans_of` for a formatted value change only a refusal of a safe program
(`s.write($"{r}".toString())` with `r` a borrow of `s`). Not read: the two of `not_lent`, and the arm of `part_loans`
for a number given as the receiver, which lives although `ok_call_parts_copied` was written for it.

Choices made in the hundred-and-nineteenth slice (fuzzing for differences, 1324). Approved on 19 November 2026:

1361. **A generator of our own** (`fuzz/difference/`), a crate with no dependencies on stable Rust, as Csmith
     and YARPGen are for C and rustlantis for Rust (which compares Miri with what LLVM makes): a seed gives one
     program, the same on every machine, so a finding is its seed. libFuzzer is not used here: each input is a
     build of C, and what guides it, the coverage of `sg`, does not see the C runtime.
1362. **A program that the checker takes and that ends**: the generator keeps the type of every value it writes;
     a loop runs a number of rounds written in it, a `while` has a counter that no `continue` steps over, a
     function calls only those written before it, and one that calls itself is given its depth; the steps a
     function may take are counted, so a program ends in a fraction of a second. Its only output is `print`: no
     clock, randomness, files, `unsafe` or C. A program the checker refuses is kept apart and read: either the
     generator wrote what it should not, or the checker is stricter than it should be.
1363. **Panics are allowed and compared**: safe SG has no undefined behaviour, and an overflow, an index out of
     range or a division by zero is a panic, whose message and code must be the same both ways. Most operations
     are guarded (`i & 7` as an index, `d | 1` as a divisor, the wrapping methods), about one in ten not, so a
     program does not stop at its first line.
1364. **What is compared**: the interpreter against the debug build under the address and undefined-behaviour
     sanitizers (`-O1`), output, errors and code; and when the program does not panic, the release build
     (`-O2`, under the undefined-behaviour sanitizer) against the debug build, so the way that wraps never
     reaches undefined behaviour of C. A report of a sanitizer, a run that does not end and a fault of `sg` are
     findings too.
1365. **Step by step**: stage 1, the integer types, `bool`, arithmetic, the bit operators, conversions,
     comparisons, `if`, `match`, `for` and `while`, functions, recursion, arrays, consts and `print` with a
     format; stage 2, `Copyable` structs, enums with what they carry and `match` on them, `T?` with `??` and
     narrowing, lists, text and functions that give a `Result` with `??`, `match` and `try`. Borrows and moves
     (stage 3) and tasks with channels (stage 4, compared in any order) come later.
1366. **A finding is made smaller** by a reducer of our own: the generator writes a tree of statements, and
     runs of them are taken away, halves first, then blocks replaced by what they hold, while the checker takes
     the program and the finding stays. C-Reduce does not know SG. The program then becomes a test (1321).
1367. **Where**: in the Linux image of the tests, without the network, six at once for two hours; what is found
     and its seed stay in the volume of the fuzzing. On macOS only to see a finding again.
1368. **The interpreter is the `sg` program**, run in a process of its own with a time limit, so a fault or a
     hang of it does not stop the run, and the way a user runs a program is the one tried.

The first run, two hours in the Linux container, six at once: 135 496 programs, of which 14 885 ended in a panic,
gave the same output, message and code interpreted and compiled, and none drew a report of a sanitizer or a
difference between the debug and the release build. The 60 programs the checker refused were all the
generator's: it took a list written through an index that is not cut (`l[5] = x`) for one that was read, and the
checker rightly said the list is given values that are never read; after the fix, 1 703 programs in three minutes
on macOS, none refused. Writing the generator taught it the rules a program must keep, each a rule of the
language as it is meant (`b.max(300)` on a `byte` is an `int`, 163; a `T?` has no text; `x is null` narrows `x`
where it holds, so a `??` there is an error; a program nests at most 1000 deep). The reducer, tried on a difference
made on purpose (the interpreter given one more line when the program has a `while`), took a program of 47 lines
to 10 in 40 tries, nine seconds.

Choices made in the hundred-and-twentieth slice (support for editors). Approved on 19 November 2026:

1369. **The order**: (1) colours and the settings of the editor (a TextMate grammar, its brackets, comments and
     indentation, and an extension for VS Code in `editors/vscode`), and (2) `sg lsp`, the errors as you write and
     a hint applied with one click, in this slice; (3) the type under the pointer, going to a definition and the
     names of a file, and (4) completion, which needs a parser that goes on after an error, later.
1370. **The language server is in `sg`** (`sg lsp`), with JSON read and written by `sg` itself and no dependency,
     as Deno and Gleam have it: one program, so the server and the compiler are always the same version.
     rust-analyzer, zls and gopls are programs of their own. Publishing the extension is a step outward and is
     asked for with the preview.
1371. **A hint applied with one click** is an edit the error carries beside its text (`Diag::fix`: a place and
     what is written there), given only where the code is certain; rustc marks such suggestions
     `MachineApplicable`. The first four: a conversion (`(int)big`, or `(int)(a + b)`), `.clone()` of a text
     that is only borrowed, `_ = ` before a value dropped on purpose, and the missing `use` line, put before
     the first line of the file that is not a comment. An unused `Result` has none: its two ways are `try`,
     which needs a function that gives a `Result`, and `_ =`, which drops the error, the risky way a hint does
     not take (the first rule of hints in 04). Each fix is applied in `tests/lsp.rs` and the program then checks.
1372. **When and what is checked**: 300 ms after the last change, on a thread of its own with the stack the
     checker needs; the program is the one `sg` would read the file in (the nearest folder above it with a
     `main.sg`, or the file alone with its modules beside it), and an open file is read from the editor, saved
     or not. The program is only checked, never run; `test` blocks are checked too.
1373. **What is shown**: the first error, as the checker stops there, in the file it is in (an error in a module
     is shown in the module's file), with its notes and hint, and the second place it names as a link. An error in
     the language's own module is shown at the start of the file. Several errors at once wait for a parser that
     goes on after one (part 4).
1374. **A fault of the checker does not stop the server**: a panic is caught and shown as one error, *the
     checker stopped: this is a mistake in sg*, and the next change is checked again.
1375. **Tested from outside** (`tests/lsp.rs`): a `sg lsp` process is talked to as an editor talks to it; the
     first error comes, at the column the protocol counts (UTF-16, a character of two units before it), and goes
     once the text is right; what the editor has not saved is read, an error in a module is shown there; each of
     the four fixes is applied and the program then checks; a request it does not serve is answered with an error,
     and the exit codes are as the protocol has them. `json.rs` and the reading of `file:` URIs have tests of
     their own.
1376. **The client for VS Code** is plain JavaScript (`extension.js`), with nothing to build, and one
     dependency, vscode-languageclient 10.1.2, pinned in `package-lock.json` and watched by Dependabot. `sg.path`
     names the `sg` program (`sg` on the `PATH` without it); a workspace that is not trusted cannot set it, so
     a folder cannot choose the program the editor starts. Tried by hand in VS Code 1.146 on macOS: the colours,
     the error under `int small = big` and its fix `(int)big`, after which the error went.

Choices made in the hundred-and-twenty-first slice (the web site, `website/`). Approved on 19 November 2026:

1383. **In the repository**: `website/`, built with Docusaurus 3.10.2 on Node 22, its packages pinned and
     locked and watched by Dependabot, so a slice's one commit changes the code and the page that describes it.
1384. **Laid out as Rust's site is**: a front page (what SG promises, three pillars, a program that runs, what was
     built with it, where it stands), then Learn, Reference, Library and Status.
1385. **A thin start**: the front page, how to install, a tour, SG for Rust programmers and the status; the
     reference and the library point into the repository until the preview (942), and pages of the library
     made from its source, as source.dot.net shows .NET's, come with a `sg doc` of their own later. The pages are
     written for the reader, not copied from the record of decisions, in English. `learn/for-rust.md` is the
     exception: `scripts/sync-docs.mjs` makes it from `design/06-rust-to-sg.md` before each build, which keeps it
     as the slice left it.
1386. **Search built into the site** (a plugin, no service outside it); Algolia's DocSearch, which asks for an
     application and runs its own crawler, is for after the preview.
1387. **The look**: the nazar bead (drawn simply for now, `static/img/logo.svg`) and Pasha; cobalt, sky blue,
     white and navy; light or dark as the system is set.
1388. **SG coloured on the site** with the words and marks of the VS Code grammar (`src/theme/prism-include-languages.js`).
1389. **Published by Cloudflare Pages** from the repository on GitHub, no secret in it, with headers of its own
     (`static/_headers`): a CSP that runs no inline script but the colour mode's, by its hash
     (`scripts/check-csp.mjs` fails the build otherwise; Docusaurus's banner about a wrong base URL, another
     inline script, is turned off), HSTS, `nosniff`, no framing. No analytics.
1390. **Publishing is the author's step**: the account, the connection of Pages and the name servers of
     sg-lang.org. Tried before it on macOS with the headers applied: the front page, the tour, and the search,
     with no script refused. Every program on the pages runs (`npm run check`, 9 programs).
1392. **The pages of the library are made from its code**, as Rust's `std` documentation and Go's pkg.go.dev
     are: a page for each type, with its methods, what each does (from comments `///` written in the source),
     examples that run, and a link from each item to its source on GitHub. A browser of the source itself, as
     source.dot.net is for .NET, is for reading how a thing is built rather than how to use it, and is not what
     these pages are; one may come later from what `sg lsp` knows. The pages come with the preview, from a
     `sg doc` of their own and the `///` comments that were put off until then.

## Slower than C, and could be better

Places where a measurement found SG slower than the same program in C and a fix is in sight. They are kept
here to be talked over when their time comes; none of them breaks a program, and none is fixed by turning a
check off.

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
- **Starting a task that ends at once** (`async`, 1.08 times C, choice 430): a task that never waits
  still takes its allocation and the atomic steps of a task that might be woken from another thread.
  Candidates: a task that ends before it first waits could skip those steps, and its block could come
  from a list of freed ones.
- **Reading the input the async way** (`asynclines`, 1.28 times C, choice 439; 1.37 before choice 491): a line the block holds
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
- **Handing a buffer to C across calls** (`deflate`, 1.04 times C, choice 543): zlib reads its input
  between calls, and `buffer.pointer()` lasts only for one (523), so the `Deflater` copies each piece into
  raw memory a byte at a time, and copies what zlib wrote back the same way. Candidates: `Memory.copy`
  between a slice and raw memory, which compiles to `memcpy`, and a buffer lent to a value for as long as
  the value lives (a `raw` view that carries a borrow).
- **Checking that a line is UTF-8** (`netlines`, 1.14 times `getline`, which checks nothing, 0.89 without
  the check, choice 493; `lines` takes 0.82 times with it): the reader checks its block at once (491), with
  sixteen bytes at a time while they are ASCII and one byte at a time inside a character (492), about
  6 GB/s; the check is still some 0.02 s of the 0.09 s. The output a program keeps of another (`capture`,
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

## Not here yet

- **Of fuzzing for differences (1361-1368)**: the generator writes stages 1 and 2; borrows and moves (stage 3) and
  tasks with channels, compared in any order (stage 4), are still to come. The reducer takes statements and blocks
  away, not parts of an expression.
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
- **Of the hundred-and-thirteenth slice**: in compiled code, tasks that wait on channels for each other wait
  for ever, where the interpreter stops with a panic (1286): the count of waiting tasks comes with the hardening.
  Waiting on several channels at once (Go's `select`), a `for` that awaits (`for await job in jobs`, with
  `linesAsync`), a channel of a `StreamWriter` (1293), and a `Sender` this function did not make by name followed
  by the rule of 1285 (1294).
- **Of the hundred-and-twelfth slice**: a time limit and a cancellation of `lock ... when` (`cnd_timedwait`; 1264);
  in compiled code, waiters that wait for each other in a circle, or several owners that all wait, are not
  seen and wait for ever, where the interpreter stops with a panic (1263, 1271): a count of the tasks that
  wait, as Go keeps, comes with the hardening before the preview.
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
- **Of `const assert`**: one that measures a type calls no function (1159).
- **Of consts**: a text inside an array, a struct or an enum (1110); an array of arrays, as anywhere; the
  count of an array in a type from a const that calls a function (1125).
- **Of statics**: a static that holds a function is copied to a local to be called (`var f = Double`, 1140); a
  value given by a call is found when the program runs even when a const could find it (1136); a `static
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
- **Of other programs**: two programs joined by a pipe (`a | b`), an output sent straight into a file, a
  look at whether a program has ended without waiting (`try_wait`; a wait with a time limit is
  `waitAsync` with a token of `CancellationSource.after(ms)`), a program left to go on after this one
  (`detach`), groups of processes, a signal to a program that is not one's own, a terminal of its own
  (pty). A program
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
  `where r from name`); a `List<read T>`, and a borrow as the error of a `Result` (a list of a type that
  carries a borrow works); a `match` used as a value that gives a borrow is not followed.
- `swap(ref a, ref b)`.
- A string literal has the type `string` here; in the design it is `read string` that becomes `string` without
  allocating.
- The names of an `is` inside a `!` around a whole chain (`if !(r is Ok(v) && v > 0) { return }` and then
  `v`, 295), and a note that names the `break` which keeps a variable a `T?` after a `while` (289). An `is`
  with names on a value that owns memory, made by a call after the first `&&` or `||`, in compiled code
  (268, 299).
- A name for a value whose pattern is tested at the same time (Rust's `n @ Number(_)`), a text tested
  for how it starts (choices 243, 248), a constant as an end of a range (`int.MaxValue`), a range open at
  one end (`5..=`), `..=` outside a pattern (254, 255). A pattern of an `or` inside the parentheses that the
  ones before it already take is not caught (256).
- Interfaces of your own (`interface`, `where`) and the other built-in one (`Clonable`). `Disposable` and
  `Closable` on an enum. A `Disposable` value that holds a `raw T` going to another task. `.toString()` on a `Formattable` struct or enum.
- Generics other than the built-in `Result<T, E>`, `List<T>`, `HashMap<K, V>` and `HashSet<T>` (generics of your own are deferred
  in the design), and so an alias with type parameters (`using Link<T> = ...`).
- The task of `Task.whenAll` kept in a variable to be awaited later (`var all =
  Task.whenAll(tasks)`), and a `for` over a list of tasks (a `break` would leave tasks); a method or a function of a struct
  started as a task (a `Func` is a function written at the top level). Of async: `linesAsync` (a `for`
  that awaits), an async function or method kept as a `Func`. A token cancelled while a file is
  read or written does not stop the work (433); one cancelled while a task waits for the network or for a
  line of the input that can make the program wait does (471, 472). Of the network: a stream received from
  by one task and sent to by another (and so `send` through a `shared` stream), the reader of a connection
  kept in a field or given back (it carries a borrow; `StreamReader<from conn>`), a writer of a connection,
  closing one direction (`shutdown`), the
  options of a socket (`TCP_NODELAY`), a
  listener on every v6 address. `checked` on its own, in a value with one
  owner (`checked int hits` in a plain struct, the design's `RefCell` outside `Rc`). `List<shared T>.clone()`,
  since a list has no `clone` yet, so the design's hint "loop over a copy" is not given. A conflict through two
  items of one list (`xs[i]` and `xs[j]`) is left to run time even when `i` and `j` are the same name.
- An alias of a collection used to build it (`Names()`, 330). Running the tests of one module only.
- A collection changed inside its own `for` through another `ref` to it is not caught (it is part of the
  borrow check); the direct forms are. Here a list is gone over by position and a map or a set as it was when
  the loop started.
- Lists of borrows (`List<read T>`); `reserve`, `clone`; a `Hashable`
  type with its own `equals` or `hash`.
- What Unicode has beyond the tables of 635: normalization, grapheme clusters, sorting by a language, the width
  of a character, the title case, the Turkish and Azeri case mappings (640), and names in Unicode in the
  source. Replacing only the first places
  (`replacen`), replacing in place without a new text; a `padLeft` is a width: `$"{name,10}"` (229); the positions
  of the characters (`charIndices`); splitting at most n times. Writing the type of an
  iterator, and a `for` over a type of your own that has `next` (with generics). `text.capacity()` and
  `text.reserve(n)`; a `ref string` given to a `format` method in place of its `Formatter`.
- A number is written in decimal, hex and binary (`{n:x}`, `{n:b}`) and read in decimal and hex
  (`parse`, `parseHex`), not in binary. C#'s other letters (`E`, `G`, `N`, `C`, `P`) and formats made of
  characters (`{x:0.00}`), a fill other than a space, centring, and a width that comes from a variable
  (choices 224, 229). A fractional number is not read out of bytes or written into them
  (`double.fromLittleEndian`, the bits of a `double` as a `ulong`). Of `<math.h>`: `nextafter` toward a
  number (`nextUp` and `nextDown` do it), the sign of Γ beside `logGamma`, `lround` and the other roundings
  to an integer (`round()` and then a conversion do it), `fdim`, `fenv` and the rounding modes (1225-1236).
  `saturatingDivide`, `saturatingNegate`, `saturatingAbs` and a saturating shift.
- Arrays of arrays, `read T[]?`, an array as a key, looking for a run of items in a slice. `addAll` of
  items that are not `Copyable`, moving every item of one list into another, adding a range of a list to
  the same list (`extend_from_within`), a sort that does not allocate (choices 235, 236, 239). A sort or
  a search in order with a function that compares, or by a key (`sortBy`, `binarySearchBy`; 1254).
- A panic of a shift names no way out: there is no `tryShiftLeft` yet.
- The command line as bytes (a word that is not UTF-8 stops the program). `IoError` does not carry the path
  (choice 195). `PermissionDenied` and `NoSpace` are mapped but have no test: the tests cannot make a file
  they may not read or fill a disk.
- **Of folders and the environment**: making a link and reading where one points, changing permissions and
  owners, watching a folder, patterns of names (`*.txt`), a listing of a whole tree, the names of a folder
  or the variables of the environment as bytes, all the variables at once, the path of the program itself,
  the `Async` forms (the calls are made as they are, also in an async function). A name in a folder that is not UTF-8 is an `InvalidText`, but has no test: macOS does
  not let a program make such a name (571). A path opened by the handle of a folder (Go's `os.Root`,
  `openat`), so that a link inside the folder cannot lead out of it as it can after `Path.joinInside`
  (954).
- **Of seeking in a file**: making a file shorter or longer (`setLength`, POSIX's `ftruncate`), reading and
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
- Freeing a long linked list recurses, the open stack-depth question from the design; here it runs on a 1 GB stack.
  Compiled code frees a chain of `T*` in a loop, but a chain of `shared` values recursively, one call for each.

Words from the design that are not implemented produce "is not in the interpreter yet" instead of a confusing
error.

## Next

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

A value can have several owners: `new shared x`, `share x` and `weak x`, and a type has a second name
with `using`. What a `shared checked` value points to is changed through any owner, each access checked
against the borrows in progress: a conflict through the same variable is a compile error, one through
another a panic with both lines. `read x as n { }` and `ref x as n { }` lend it for a block, a method takes
the owner with `read FolderRef this`, and the `shared` values that hold each other up when a program or a
test ends are reported. The design's folder tree and its cities and roads run as it wrote them (the
cities without the lambda). All of it is compiled too: a `shared` value is a pointer with the counts in
front of it, the borrows of a `shared checked` one are checked where the checker opened it, and a debug
build reports the leaks; the folder tree takes 1.53 times the same tree in C with plain pointers.

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
Windows' C library several times faster (1068-1079). Before a preview of the language (942), what the
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
and read in RFC 3339 (1300-1317). The hundred-and-fifteenth begins the hardening, before the preview: the
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
