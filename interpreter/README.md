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

## What runs today (21 November 2026)

| Area | Supported |
|---|---|
| Functions | Top-level functions, type-first signature, recursion, any order. `void main()` is the entry point; `void main(List<string> args)` gets the words of the command line. The last parameters may have default values, which a call that leaves them out gets as if it wrote them: `void connect(read string host, int port = 80, int retries = 3)`, then `connect("a")`, `connect("a", 8080)`; a default is what a const can be, `null`, an empty collection, a literal that gives no field (`Options {}`) or `CancellationToken.None` |
| Modules | A folder is a module: the files in it see each other with nothing written. `use stock` at the top of a file brings in the `public` names of the folder `stock`; `use shop.stock` for a folder inside a folder. The full name works without `use`: `billing.Line { ... }`, `stock.total(read xs)`, `stock.Color.Red`, `List<stock.Product>`. `public` on a struct, an enum, a function, a field or a method; without it the name stays inside its module. `readonly` on a field. The program is `sg main.sg` (the file is the root module, the folders next to it are the modules) or `sg folder` (every file of the folder is the root module). `sg build main.sg` and `sg build folder` compile the same program. Packages: an `sg.project` beside the program names them, `require xml example.com/ada/xml 1.4.0`; `sg add`, `sg update`, `sg remove` and `sg lock` fetch them with `git` into a cache and pin each by the SHA-256 of its files in `sg.lock`, and `local xml ../xml` takes one from a folder; `use xml` and `use xml.reader` name its modules, and inside it `use reader` names its own. `unsafe` in a package is allowed only by the program's `require` line. A folder `internal` is named only from inside the folder that holds it |
| Types | `sbyte`, `byte`, `short`, `ushort`, `int` (32 bit), `uint`, `long`, `ulong`, `nint`, `nuint`, `CLong` and `CULong` (C's `long` and `unsigned long`), `float`, `double`, `bool`, `char`, `string`, `void` as return type, structs, enums, `T*`, `T?`, `shared T`, `weak T`, `shared checked T`, `weak checked T`, `shared locked T`, `weak locked T`, `Task<R>`, `CancellationSource`, `CancellationToken`, `TcpListener`, `TcpStream`, `UdpSocket`, `SocketAddress`, `CString`, `Duration`, `Instant`, `DateTime`, `Random`, `Result<T, E>`, `List<T>`, `HashMap<K, V>`, `HashSet<T>`, arrays `T[N]`, slices `read T[]` and `ref T[]`, raw pointers `raw T`, pointers to functions of C `CFunc<A, R>`, types of C seen only behind a raw pointer (`struct sqlite3` in an `extern` block), `read T` and `ref T` in signatures |
| Type aliases | `using NodeRef = shared Node`, `public using P = Point`: a second name for the same type, which also builds it (`new NodeRef(1, null)`, `P(1, 2)`, `P { x = 1, y = 2 }`) |
| Types that carry a borrow | `struct Word<from source> { read string text` / `int start }`, `enum Token<from source> { Name(read string text) ... }`: a field can be a borrow when the type names its source. In a method `from source` says "from what this value borrowed, not from the value itself"; outside the type it is `from words.source` |
| Structs | `struct Node { int value` / `Node*? next }`, `Node { value = 1, next = null }`, field access and assignment through pointers and borrows. A field may have a default value, which a literal that leaves it out gives it (`struct Config { int port = 80 }`, `Config {}`): what a const can be, found when the program is compiled, `null` or an empty collection. `p with { x = 3 }` is a new value with the fields written changed: `p` is moved, or copied when it is `Copyable`, and what those fields held is dropped there |
| Methods | Written in the body of a struct or an enum with `this`, `read this` or `ref this` as the first parameter; fields are reached as `this.x`, and an enum looks at itself with `match this`. Called as `stack.push(1)`, `shape.area()`, also through pointers and borrows. With a type in front of `this` (`read FolderRef this`, `FolderRef this`) a method is called on the `shared` value that holds the struct. A function without `this` is called on the type: `Stack.create()`, `Shape.unit()`. `Stack(args)` is short for `Stack.create(args)` |
| Enums | `enum Shape { Circle(int radius)` / `Rect(int width, int height)` / `Empty }`, one variant per line, and methods after or between them. Built with the full name: `Shape.Circle(2)`, `Shape.Empty`, `new Expr.Number(1)`. An enum with numbers names their integer type first: `enum Block : byte, Copyable { Image = 0x2C` / `Trailer = 0x3B }`, its variants carry nothing, and one without a number has one more than the variant above; `(byte)block` gives the number, `Block.fromValue(b)` the variant or `null`, and `: Comparable` orders by the numbers without a `compare` written |
| `match` | `match x { Circle(r) => ... }` as an expression or a statement, on an enum, also behind pointers and borrows. Patterns: a variant with a pattern for each value it carries (a name, `_`, or a pattern inside: `Add(Number(0), right)`, `Word("let")`, `Paint(Red)`), `_`, `var other`; a condition with `when`. On an integer the patterns are numbers and characters, on a `char` characters (`match c { '+' => ... _ => ... }`), on a text texts (`match word { "if" => ... _ => ... }`), on a `bool` `true` and `false`. Patterns are joined with `or` (`Add(l, _) or Mul(l, _)`, `'+' or '-'`), and each names the same values; numbers and characters come in ranges that take both ends (`'a'..='z'`, `0..=9`). On a `T?` the patterns are `null` and those of the `T`, also inside a variant (`Node(null, _)`); a name below an arm `null` takes a value that is not `null`. A value no arm takes, or an arm that can never run, is a compile error that names the value: `does not handle Add(Name(_), _)`, `no arm takes 128..=255`. An arm is an expression, a statement (`r += 1`, `return x`, `break`) or a block whose last line is the value |
| Interfaces | Seven built-in ones, written after the name: `struct Point : Copyable`, `enum AppError : Formattable`. A `Copyable` value is copied where another would be moved. A `Formattable` value is accepted by `print`, `$"..."` and `f.write`; a struct or an enum can write its own `void format(read this, ref Formatter f)`, otherwise the text is made from the fields. `Equatable` gives `==` and `!=` (field by field, or the type's own `bool equals(read this, read T other)`), `Hashable` lets the type be a key of a `HashMap` or an item of a `HashSet`, `Comparable` gives `<` `<=` `>` `>=` and `sort` through the type's `int compare(read this, read T other)`. A `Disposable` struct has a `void dispose(ref this)` that runs by itself, once, where the last owner of the value ends, before its fields end (not on a panic or `exit`; `x.dispose()` is an error, and no field is moved out of it). A `Closable` struct has a `close(this)` that gives a `Result`, and a local of it is closed before it ends, as a `StreamWriter` is |
| `List<T>` | Built in. `List<int>()`, `List<int> { 1, 2 }`, `List<byte>.repeat(0, n)` (`n` copies of a `Copyable` item in one block, zeros from `calloc`), `List<T>.withCapacity(n)` (empty, with room for `n`; 1648, 1652); `xs.add(v)`, `xs.length()`, `xs[i]` to read and to write (`xs[i] = v`, `xs[i] += 1`, `xs[i].name = ...`, `ref xs[i]`), `xs.get(i)` gives `null` where `xs[i]` would panic, `xs.removeLast()`, `xs.reverse()`, `xs.clear()`, `xs.addAll(items)` copies the items of a slice to the end (`buffer.addAll(text.asBytes())`, `xs.addAll(read ys)`, `xs.addAll(ys[2..])`) when they are `Copyable`, and `xs.copyFrom(s)` copies them over as many. `xs.indexOf(v)` and `xs.contains(v)` on `Equatable` items, `xs.sort()` on `Comparable` ones (numbers and text are both); on items in order, `xs.binarySearch(v)` gives the place of the first equal one or `null` by halving, and `xs.lowerBound(v)` and `xs.upperBound(v)` the first place whose item is not less than `v`, or greater than it (`upperBound(b) - lowerBound(a)` counts the items from `a` up to `b`). `string.join(", ", read names)` |
| `HashMap<K, V>` | Built in; a key is an integer, a `bool`, a `string` or a type written `: Equatable, Hashable`. `HashMap<string, int>()`, `HashMap<string, int> { {"x", 10} }`; `map.add(key, value)`, `map[key]` to read and to write what is under a key that is there, `map.get(key)` gives `null` for a missing key, `map.getOrAdd(key, initial)` gives a `ref V`, `map.contains(key)`, `map.remove(key)`, `map.length()`, `map.clear()` |
| Text of a collection | `print(xs)`, `$"{xs}"`: `[1, 2, 3]`, `{"ada": 36}`, `{1, 2, 3}`, when the items can be written as text |
| `HashSet<T>` | Built in; an item is what a key of a `HashMap` can be. `HashSet<int>()`, `HashSet<int> { 1, 2 }`; `set.add(v)` gives `true` if the item was not there, `set.contains(v)`, `set.remove(v)` gives `true` if it was there, `set.length()`, `set.clear()` |
| Arrays | `int[8] values`: a fixed number of items, kept where the value is (in the variable, inside the struct), with the length in the type. Built as `int[3] { 1, 2, 3 }` (the items), `byte[16](255)` (one value in every item) or `byte[16]` (zeros: 0, `false`, `null`). `xs[i]` to read and to write, `xs.length()`, `xs.get(i)`, `xs.indexOf(v)`, `xs.contains(v)`, `xs.binarySearch(v)`, `xs.lowerBound(v)`, `xs.upperBound(v)`, `xs.sort()`, `xs.reverse()`, `xs.copyFrom(s)`, `==`, `print`. An array of items that can be copied is copied like a number; any other is moved |
| Consts | `const int BlockSize = 4096`, at the top of a file (`public const` for other modules) or in a block, where it is seen to the end of the block. A number, `bool`, `char`, a text, or an array, a struct or an enum of numbers, `bool` and `char` (`: Copyable`). `const int[] Primes = { 2, 3, 5, 7 }` counts the items, `const byte[] Logo = embed("logo.gif")` holds the bytes of a file beside the source. The value is found when the program is compiled: numbers, texts, other consts, operators, casts, `MinValue` and the others, arrays, structs and variants built of them, `.length()` and an item or a field of a const; an overflow, a division by zero or an index outside there is a compile error. A const calls no function (`a const calls no function`, with the hint of a `static`): the compiler runs none of the program's code. An integer const gives the count of an array (`byte[BlockSize]`, `int[N](0)`), and a const of a number, a `char`, a text or a `bool` is a pattern (`match tag { Trailer => ... }`). An array, a struct or an enum is lent where it is used, for the whole run, and never changed (`var copy = Masks` copies it); in compiled code it is one `static const`, which nothing copies. `const assert(cond)` and `const assert(cond, "message")` check a condition made as a const is, at the top of a file or in a block; a failed one says both sides of a comparison. `Memory.sizeOf<T>()` and `Memory.alignOf<T>()` are written only there: compiled code hands such an assert to C's `_Static_assert` too, for the machine it is compiled for |
| Statics | `static uint[256] Crc = crcTable()`, `static atomic long Requests = atomic 0`, `static locked Config Settings = Config.defaults()`: one value for the whole program, never assigned, at the top of a file (`public static` for other modules) or in a function, where its name is seen to the end of the block. A value made as a const's is (numbers, consts, operators, no call; inside `atomic` and `locked` too) lies in the program from its start; any other is found the first time the static is used, once, whichever task comes first, and the others wait. What finds it cannot `lock`, `wait` or `await` through the calls, and one that needs itself is a compile error, or a panic through a `Func`. A number, a `bool`, a `char` or a function is read as a value, anything else lent as `read`; `static atomic` changes by its methods, `static locked` only inside `lock`. What a static holds follows the thread rule and is never closed or disposed of, so no file, stream or socket; each test starts with the statics not used yet, and what they hold is not a leak. In compiled code a static is a global of C, a `static const` when it is known from the start and holds no atomic value; a read of one found later is a load and a branch |
| Slices | `read T[]` looks at items that something else owns, `ref T[]` can also write them. `read xs` and `ref xs` give one where a slice is expected, for a `List<T>` and a `T[N]`; a range gives one too: `xs[a..b]`, `xs[..b]`, `xs[a..]`, `xs[..]`, and `ref xs[a..b]` for a range that is written through. `s[i]`, `s.length()`, `s.get(i)`, `s.indexOf(v)`, `s.contains(v)`, `s.binarySearch(v)`, `s.lowerBound(v)`, `s.upperBound(v)` (on a range the place is counted from its start), `==`, `print`, `for`; `s.sort()`, `s.reverse()` and `s.copyFrom(t)` through a `ref T[]`, and on a range in front of them: `xs[2..5].sort()` puts those items of `xs` in order where they are, `buf[0..n].copyFrom(buf[k..k + n])` copies items as `memmove` does (a panic if the lengths differ). `text.asBytes()` looks at a text as a `read byte[]`; `string.fromUtf8(bytes)` looks at bytes as text and gives a `Result<read string, TextError>` |
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
| Bytes as text, and checksums | `Hex.encode(read data)` and `Hex.decode(text)`, `Base64.encode` and `decode` (padded) and `encodeUrl` and `decodeUrl` (unpadded), read strictly: a decoding gives `Result<List<byte>, DecodeError>`, with the place of the first byte that does not belong. `Crc32.of(read data)` is the CRC-32 of gzip, PNG and ZIP; `var c = Crc32()`, `c.update(read piece)`, `c.value()` the same piece by piece; `Adler32` is zlib's checksum, used the same way. `Sha256.of(read data)` is SHA-256 as a `byte[32]`, `var h = Sha256()`, `h.update(read piece)`, `h.finish()` piece by piece; `Hmac.sha256(read key, read data)` is HMAC-SHA-256, and `Bytes.equalInConstantTime(read a, read b)` compares a tag in a time that does not tell where it differs. Written in SG in the module `std`; compiled, CRC-32 and SHA-256 go through the processor's own instructions where it has them (ARMv8's CRC, PMULL and SHA2, x86-64's PCLMULQDQ and SHA-NI), the same bytes either way |
| The command line | `var cl = CommandLine("gifcrop", "Crops GIF images.")`, then `cl.flag("verbose", 'v', "Print each step")`, `cl.option("output", 'o', "FILE", "Where to write")` (`null` for no short letter), `cl.positional("input", "The GIF to read")`, `cl.rest("files", "More GIFs")` and `cl.version("1.2.0")`; `cl.parseOrExit(read args)` gives an `Arguments` or ends the program: the help (`-h`, `--help`) or the version printed with 0, an error to the standard error with 2 and `Try 'gifcrop --help'.` `cl.parse(read args)` gives a `Result<Arguments, CommandLineError>` instead. GNU's way of writing (`--output x`, `--output=x`, `-vq`, `-ox`, options after the words, `--`, a lone `-` a word), no long name shortened, and an unknown one told the declared name nearest to it. `a.flag(name)`, `a.option(name)` (`read string?`, the last given), `a.options(name)` (all), `a.positional(name)`, `a.rest()`; a name not declared, or declared as something else, is a panic. Written in SG in the module `std` |
| JSON | `Json.parse(text)` gives a `Result<JsonValue, JsonError>`; a `JsonValue` is `Null`, `Bool`, `Integer` (a whole number that fits a `long`), `Number` (every other), `String`, `Array` or `Object` (a `JsonObject`, its members in order). `v.get(name)`, `v.at(i)`, `asString()`, `asLong()`, `asDouble()`, `asBool()`, `asArray()`, `asObject()` give `null` for what is not there, so `doc.get("user")?.get("name")?.asString() ?? "-"` never panics; `o.add`, `o.set`, `o.remove`, `o.get`, `o.names()` and `o.values()` in the same order on a `JsonObject`. `v.toJson()` writes it on one line (`$"{v}"` too), `v.toJson(JsonOptions { indent = 2 })` on lines. Read strictly (RFC 8259): no comments, trailing commas, `NaN` or byte order mark, a name twice an error, nesting to 128, written to 128 too, `JsonOptions { maxDepth = 512 }` given to `toJson` and `Json.parse` for more; a `JsonError` gives the line, the column and a `JsonErrorKind`. `==` compares objects in any order, `clone()` copies. A struct of the program is written and read by hand. Written in SG in the module `std` |
| Log | `Log.trace`, `debug`, `info`, `warn` and `error` write a line to the standard error: `2026-11-19T20:15:03.123Z INFO  listening on 8080`, the time in UTC. `Log.setLevel(LogLevel.Debug)`, `Log.level()`, `Log.isOn(level)`; a program starts at `Info` or at what `SG_LOG` names. A level that is off builds no text: `Log.debug($"...")` is compiled inside `if Log.isOn(...)`, and a call in the text does not run. A message is never read for anything in it (no `${...}` lookups), and its control characters, marks of direction and backslashes are escaped, so one line is always one call. Lines of tasks never mix. The level is coloured on a terminal, not with `NO_COLOR`. Written in SG in the module `std` |
| Regular expressions | `Regex.of("""(\d+)-(\d+)""")` for a pattern written in the program, checked when the program is compiled (one that is not a regular expression is a compile error), and kept in a `static` to be compiled once; `Regex.compile(pattern)` for one made when the program runs, a `Result<Regex, RegexError>` (`column 3: missing closing )`). `re.isMatch(text)`; `re.find(text)` gives a `Match?` (`start` and `end` in bytes, `m.of(text)` the text); `re.findAll(text)` a `List<Match>`; `re.captures(text)` a `Captures?` (`caps.get(1)`, `caps.named("year")`); `re.replaceAll(text, "$2-$1")` (`${name}`, `$$`) and `re.split(text)`. The meaning of RE2, as in Go and Rust: the time of a match grows with the text and no more, so there are no back references and no look-around. Go's syntax: `[...]`, `\d \w \s \b` (ASCII), `\p{Lu}` `\pL` `\PL` by the general category of Unicode, `[[:alpha:]]`, `(?<name>...)`, `{n,m}`, `*?`, `(?i)` `(?m)` `(?s)` `(?U)`; `(?i)` compares by Unicode's simple case folding. Searched by lazy DFAs, forwards for whether and where a match ends and backwards for where it starts, with Pike's machine only for the groups; their states are kept in the `Regex` behind a lock, so one in a `static` serves every task (1535-1539). Written in SG in the module `std` |
| Compression | `Gzip.compress(read data, 6)` and `Gzip.decompress(read data, limit)`, a `Result<List<byte>, CompressError>`; `Zlib` and `Deflate` the same (RFC 1950 and 1951). The bytes are zlib's at every level, 0 to 9: the compressor is zlib 1.3.1's, written again in SG. Opening always takes a limit, and data that opens into more is `TooLarge`, so a bomb is refused; `Corrupt(position)`, `Truncated`, `ChecksumMismatch`, `TrailingData`. `Gzip.writer(target, level)` gives a `Compressor` (`writeBytes`, `close`) over a `StreamWriter`, `Gzip.reader(source, limit)` a `Decompressor` (`readSome`, `readBytes`, `readLine`, `readText`) over a `StreamReader`. Several members of gzip open as one. A level written in the program outside 0 to 9 is a compile error. Written in SG in the module `std` |
| Zip archives | `var zip = try ZipReader.open(read path)` (or `ZipReader.of(reader)` over a file opened another way) reads the central directory and checks it before anything is given: a name that could lead out of a folder (`..`, a root, `\`, `:`, NUL, a name Windows keeps for a device, empty) is `InsecurePath`, two entries of one name `DuplicateName`, entries whose data lie over each other `Overlap` (the bomb of one block of data shared by every entry), more entries than the directory holds `Corrupt`. `zip.entries()` lends the reader's list of `ZipEntry` (`name`, `size`, `compressedSize`, `modified` in UTC, `isDirectory`, `method`), and `entry.clone()` copies one to keep while the reader reads (1650); `zip.readBytes(read entry, limit)` gives the bytes and `zip.copyTo(read entry, ref writer, limit)` writes them a block at a time: an entry larger than the limit is `TooLarge`, one that opens into another size than the one written `SizeMismatch`, a wrong CRC-32 `ChecksumMismatch`, encryption `Encrypted`, a method other than stored and deflated `UnsupportedMethod`. `var writer = ZipWriter.create(target)`, then `writer.add(name, read bytes, ZipMethod.Deflated, modified)`, `writer.addFrom(name, ref reader, method, modified)` (a block at a time, with a data descriptor), `writer.addDirectory(name, modified)` and `writer.close()`; zip64 where it is needed, the flag of UTF-8 for a name that is not ASCII, the time of DOS in UTC and the field 0x5455, so every machine writes the same bytes; the writer refuses the names a reader refuses. No call unpacks a whole archive to the disk. Written in SG in the module `std` |
| Folders and the environment | `File.delete(path)`, `File.move(from, to)` (never takes the place of a file: `AlreadyExists`, told in the same step as the rename), `File.replace(from, to)` (takes its place in one step) and `File.copy(from, to)` (a new file, with the permissions) give a `Result<void, IoError>`. `File.info(path)` gives a `FileInfo` with `kind` (`FileKind.File`, `Directory`, `Link`, `Other`), `size` and `modified` (a `DateTime`); `File.exists(path)` and `Directory.exists(path)` a `bool`. `Directory.create`, `createAll`, `delete` (an empty folder) and `deleteAll` (all it holds, links not followed); `Directory.list(path)` gives a `List<DirectoryEntry>` (`name`, `kind`) in the order of the bytes of the names. `Directory.current()`, `Directory.temp()`, `Directory.createTemp()` (a new folder only the user can go into), `File.createTemp()` (a new empty file only the user can read and write, its path given). `Path.join(a, b)` gives a new path; `Path.joinInside(folder, name)` a `Result<string, IoError>` that is an `InvalidInput` for a name that could lead out of the folder on any system (`..`, a root, a `\`, a `:`, a device of Windows); `Path.fileName`, `Path.extension` and `Path.parent` a view of it. `Environment.get(name)` gives a `Result<string, VariableError>` (`Missing`, `InvalidText(position)`); the variables are only read. Written in SG in the module `std`; only the calls of the system are the compiler's |
| Other programs | `var git = Command("git")`, then `git.arg(word)`, `git.args(read words)`, `git.directory(path)`, `git.setVariable(name, value)`, `git.removeVariable(name)`, `git.clearVariables()` and `git.input(text)`; no shell reads the words. `git.run()` gives a `Result<void, ProcessError>` (our input, output and standard error), `git.output()` a `Result<string, ProcessError>` (what it wrote, as text), each taking an end with anything but 0 for `Failed(status)`; `git.status()` a `Result<ExitStatus, IoError>` (`Code(int code)`, `Signal(int number)`). A name is looked for in the folders of `PATH` that start with `/`. Written in SG in the module `std`. On Windows the same through `CreateProcessW`: a name without an extension is looked for as `name.exe` in the absolute folders of `PATH` (apart at `;`, a quoted folder taken whole) and never in the folder the program works in; the words go on one command line, quoted so that the program reads them back as written; a `.bat` or `.cmd` runs only when named with its extension, through `cmd.exe`, its arguments escaped as Rust escapes them, a line break in one an `InvalidInput`; a cleared environment keeps `SystemRoot`; a code with its highest bit set is written in hex (`exited with code 0xC0000005`); `kill`, `terminate` and `signal` end the program with the code 1, since Windows has no polite stop to send (`Signal.WindowChange` does nothing). `git.start()` gives a `Result<Process, IoError>` and goes on: the streams asked for with `pipeInput()`, `pipeOutput()` and `pipeError()` are taken out once (`process.takeInput()` a `StreamWriter?`, `takeOutput()` and `takeError()` a `StreamReader?`), `process.wait()` takes it and gives how it ended, `kill()` and `terminate()` stop it, `signal(Signal.Interrupt)` sends it a signal by its name, `id()` gives its number. A `Process` in a local variable is waited for on every path; one that ends without that stops its program with the signal 9. `runAsync`, `outputAsync`, `statusAsync` and `waitAsync` take a token that may come last, and a cancel stops the program |
| Signals | `var signals = Signals.listen(read List<Signal> { Signal.Interrupt, Signal.Terminate })`: while the listener lives, those signals do not end the program; `signals.next()` waits for the next and gives it, `await signals.nextAsync(read token)` without holding the thread. Several of one kind before a call are one; every listener gets each signal it listens for. `Signal` is `Interrupt`, `Terminate`, `Hangup`, `Quit`, `User1`, `User2` or `WindowChange`. `Signals.raise(signal)` sends one to this program. Written in SG in the module `std`; the handler only writes into a pipe, and a thread hands each signal on. On Windows `Interrupt` is Ctrl-C and Ctrl-Break, `Terminate` the console closed, the user logging off and the system shutting down (the program is then held until it ends, within the few seconds Windows gives); the others never come there, and `raise` hands them to listeners all the same; with no listener a raised signal ends the program as Ctrl-C does (`0xC000013A`), and `WindowChange` does nothing |
| Atomic values | `atomic T` for an integer, `bool`, `float`, `double` or `raw T`, made with `atomic 0` and written anywhere a value is (a field, a local, a list, an array `atomic uint[4] { ... }`, raw memory as `Memory.allocate<atomic long>(n)` or `(raw atomic int)p`); tasks share one as `new shared atomic 0` or in a `shared` struct, since its methods work through `read` and `shared`. `count.load()`, `count.store(v)`, `count.exchange(v)`, `count.compareExchange(expected, desired)` and `compareExchangeWeak` (a `Result<T, T>`: `Ok` with the value it wrote over, `Error` with the one it found), `count.fetchAdd(n)` and `fetchSub` on numbers (the value before; overflow is a panic in a debug build and wraps in a release build, `wrappingFetchAdd` wraps in both), `fetchAnd`, `fetchOr`, `fetchXor` on integers and `bool`. Each is sequentially consistent unless an order follows the values, written as a variant: `count.load(MemoryOrder.Acquire)`, `count.compareExchange(a, b, MemoryOrder.AcquireRelease, MemoryOrder.Relaxed)`; an order the operation cannot keep is a compile error. `Atomic.fence(order)`, `Atomic.compilerFence(order)`, `Atomic.spinLoop()`. `struct Pool : unsafe Sendable` goes to another task with its raw pointers, as its writer promises. `examples/queue` is a queue without locks for many producers and many consumers, with no `unsafe` |
| `volatile` | `unsafe Memory.readVolatile(regs[0].status)` and `unsafe Memory.writeVolatile(regs[0].control, 1)`: a place in raw memory read or written once, where it is written, for the registers of a device; it orders nothing between tasks |
| Null | `null`, `x is null`, `x is not null`, `int? first = xs.get(0)` (a `read T?` copied into a `T?` when `T` is copyable), `a ?? b`, `a ?? panic("...")`, narrowing of local variables and parameters, also along `&&` and `\|\|` (`if name is not null && name.length() > 2`, `if text is null \|\| text.length() == 0 { return }`), under `!` (`if !(x is null)`) and after `while x is null { ... }`. `a?.b.c`, `a?.m()`, `a?.b?.c`: `null` when `a` is, and the rest of the chain does not run; a field that is not `Copyable` is lent (`a?.name` is a `read string?`), and `a?.b is not null` narrows `a` |
| Variables | `var x = 1`, `int x = 1`, assignment, `+=` `-=` `*=` `/=` `%=`, `&=` `\|=` `^=` `<<=` `>>=`. A variable keeps its type. An assignment computes its right side first, then finds the place. A name never hides another: a new local, the name of a `for` or of a pattern with the name of a local or a parameter of a block around it is a compile error, as in C#; blocks side by side, and code after a block has ended, may use one name |
| Control flow | `if` / `else if` / `else`, `while`, `for i in a..b`, `for i in (a..b).reversed()` (from `b - 1` down to `a`), `for x in xs`, `break`, `continue`, `return`. A loop may have a name, `outer: for ...` or `outer: while ...`, which `break outer` and `continue outer` use from a loop inside it; a name not used is an error. What a loop moved is gone after it as it was at each `break`, before its first round and at the end of each round |
| Operators | `+ - * / %`, `== != < <= > >=`, `&& \|\| !` (short circuit), unary `-`, cast `(int)x`. On integers `& \| ^ ~ << >>`; they bind tighter than a comparison, so `flags & mask == 0` tests the masked value |
| Numbers | `42`, `0xde`, `0b1010`, and `_` between digits: `1_000_000`, `0xdead_beef` |
| Text | `"..."` with `\n \t \r \0 \\ \" \'` and `\u{1F600}`, `$"{a}: {b}"` with `{{` and `}}`, raw text `"""a "quoted" C:\path"""` with no escapes, on lines with the indentation of its closing quotes taken off, and `$"""...{a}..."""`, `.toString()`, `text.length()`, `text.clone()`. `text[i]` reads one byte. `text[a..b]`, `text[..b]`, `text[a..]` and `text[..]` give a `read string`. `trim()`, `trimStart()`, `trimEnd()` give a `read string`; `indexOf(x)` and `lastIndexOf(x)` a `nint?` in bytes; `contains(x)`, `startsWith(x)`, `endsWith(x)`, where `x` is a `char` or a `read string` (or a `byte` for the two positions); `equalsIgnoreAsciiCase(other)`. `replaceAll(old, new)` (each a `char` or a `read string`) and `repeat(n)` give a new `string`; a call of them whose result is not used is an error |
| Building text | A `string` grows at its end: `line.write(x)` adds whatever `print` takes (`$"{name}: {count}\n"`, a number, a `char`, a text, a `Formattable` value), `line.clear()` empties it and keeps its memory, `string.withCapacity(n)` is an empty text with room for `n` bytes |
| Writing a number | Inside `$"..."` a width and a letter may follow the value, as in C#: `{n,5}` takes at least five characters with spaces in front, `{name,-10}` with spaces after (counted in characters, for every value `print` takes); `{b:X2}` and `{crc:x8}` write hex with at least so many digits, `{mask:b8}` binary, `{n:D3}` decimal with at least three digits, `{price:F2}` a fractional number with two digits after the point; `{price,8:F2}` both |
| Going over text | `for c in text.chars()` gives each `char`; `text.split(',')` (a `char` or a `read string`), `text.splitWhitespace()` and `text.lines()` give each part as a `read string`, nothing copied. Kept in a `var`, the iterator gives the next part with `parts.next()`, `null` at the end |
| Characters | `char`: one Unicode character. `'+'`, `'é'`, `'\n'`, `'\u{e9}'` is a `char`, and a `byte` where an integer goes (`byte b = '+'`, `text[i] == ':'`). `(int)c` is its code, `(char)n` the character of a code, `n.tryToChar()` gives `null` where `(char)n` would panic. `==`, `<`, `match`, keys, `sort`, `print`. `isAsciiDigit()`, `isAsciiLetter()`, `isAsciiAlphanumeric()`, `isAsciiUpper()`, `isAsciiLower()`, `isAsciiWhitespace()`, `isAscii()`, `toAsciiUpper()`, `toAsciiLower()` on `char` and on `byte`; `isWhitespace()` and `utf8Length()` on `char` |
| Unicode | On `char`, from the tables of Unicode 17.0: `isLetter()` (category L), `isNumeric()` (N: `'٣'`, `'½'`), `isAlphanumeric()`, `isUpper()` (Lu), `isLower()` (Ll), `isControl()` (Cc), and `toUpper()`, `toLower()`, one character for one (`'ß'.toUpper()` is `'ß'`). On a text: `toUpper()` and `toLower()` give a new text by the full mappings (`"straße".toUpper()` is `"STRASSE"`, `Σ` at the end of a word is `ς`), `foldCase()` the text as it is compared without case, `a.equalsIgnoreCase(b)` compares so without allocating. No language is taken into account: `"i".toUpper()` is `"I"`. `c.category()` gives the general category, a `UnicodeCategory` (`UppercaseLetter`, `DecimalNumber`, `CurrencySymbol`, ... `Unassigned`), and `k.abbreviation()` its short name (`Lu`). `text.normalize(form)` gives the text in a normal form of Unicode, `text.isNormalized(form)` says whether it is in one: `NormalizationForm.C`, `D`, `KC` or `KD` (`"e\u{301}".normalize(NormalizationForm.C)` is `"é"`) |
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

How fast the compiled programs are, against the same programs written in C by hand, is in
[SG against C](../benchmarks/against-c.md) and [on Windows](../benchmarks/against-c-windows.md); the rules
that keep those numbers fair are in [How it is measured](../benchmarks/method.md).

The choices made for the back end, from the sixteenth slice on, are in
[the record of 12 November](../design/decisions/2026-11-12.md#slice-16) and the days after it.

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
| `src/unicode.rs`, `src/unicode_tables.rs` | What a character is, its general category and its case: the tables of Unicode 17.0, one record for each code point found in three steps, read by the interpreter and written into compiled code as C when a program calls a method that needs them. `tools/unicode_tables.py` makes the tables from the Unicode Character Database (each file checked against its SHA-256) and checks every code point against Python's tables of the same version |
| `src/diag.rs` | Error and panic messages |
| `src/source.rs` | The files of a program: which folders are modules and which of them are read. All files are kept as one text, so a position says which file it is in; the language's own module comes last, when a file names one of its types |
| `src/std/time.sg` | The language's own module `std`, written in SG: `Duration`, `Instant`, `DateTime`, `DateTimeOffset`, the calendar and the reading of RFC 3339. Only it calls the readings of the clock and of the time zone, `Clock.monotonic()`, `Clock.wall()` and `Clock.offsetAt(seconds)` |
| `src/std/random.sg` | The same module's random numbers: `Random` (xoshiro256\*\* seeded with SplitMix64) and `SecureRandom`. Only it calls `Entropy.fill`, the random bytes of the system. A program gets each file of `std` when it names one of its types. A panic inside the module is shown at the program's call that led there (`Interp::call`; in compiled code `Emitter::tracked`) |
| `src/std/files.sg`, `src/std/path.sg`, `src/std/environment.sg` | The same module's folders (`Directory`, `DirectoryEntry`, `FileInfo`, `FileKind`, and the work of `File.info` and `File.exists`), paths (`Path`) and variables of the environment (`Environment`, `VariableError`). Only they call `System.list`, `System.status` and the other calls of the system; a file of `std` that names a type of another brings it along (a `FileInfo` holds a `DateTime`) |
| `src/std/process.sg` | The same module's other programs (`Command`, `Process`, `ExitStatus`, `ProcessError`): the search in `PATH`, the checks of what the system cannot take, the pipes taken out of a running program, its stop when it is not waited for, and the reading of how a program ended, over `System.run`, `System.start`, `System.wait`, `System.waitAsync`, `System.signal`, `System.reader`, `System.writer` and `System.executable` |
| `src/std/net.sg` | The same module's datagrams and addresses (`UdpSocket`, `SocketAddress`, `Received`): reading and printing a v4 and a v6 address, and the two ends of a `TcpStream`. Only it calls the sockets of the system (`System.udpBind`, `System.udpSend`, `System.udpReceive`, `System.lookup` and the others) |
| `src/std/signals.sg` | The same module's signals (`Signal`, `Signals`, `SignalListener`). Only it calls `System.signalListen`, `System.signalNext` and the other calls of the signals |
| `src/std/regex.sg`, `src/std/unicode.sg` | The same module's regular expressions (`Regex`, `Match`, `Captures`, `RegexError`: a tree, a program of Thompson's construction and its reverse, lazy DFAs and Pike's machine) and the general category of a character (`UnicodeCategory`, the work of `c.category()`). Only they call `System.charCategory` and `System.charFold`, which read the Unicode tables; `Regex.problem` is run by the checker on each pattern written in the program (`patterns_checked` in `src/check.rs`) |
| `src/std/compression.sg` | The same module's compression (`Deflate`, `Zlib`, `Gzip`, `Compressor`, `Decompressor`, `CompressError`): zlib's `deflate.c` and `trees.c` written again in SG, with zlib's licence at its top, and an inflate of tables of 9 and 6 bits. The checker looks at a level written in the program (`level_written` in `src/check.rs`) |
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
| 2 | A command line that is not one of the usages (a second program, a flag given twice, `-o` or `--target` with nothing after it, `-o` with `emit-c`), a folder of `-L` that is not there, a program that cannot be read (no such file, not UTF-8, a folder with no `.sg` file), an output into a folder that is not there, an output that is a folder (1398) |
| 0 or 1, `sg lsp` | The language server ends with 0 when the editor asked it to (`shutdown`, then `exit`), and with 1 when its input ends or it is told `exit` without `shutdown`, as the protocol has it |
| `SIGPIPE` | On Linux and macOS, an output closed before the program was through (`sg x.sg \| head`), interpreted and compiled, as C's and Go's programs end (1330), and before the tool was through with what it writes itself (`sg emit-c x.sg \| head`, 1399); on Windows that is 0 |

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

## Choices

Every choice made while building it, slice by slice and day by day, with what each slice found and how it
was tested, is in [`design/decisions/`](../design/decisions/README.md). The numbers that the code, the tests
and this page cite (`choice 886`, `1340`) are the numbers of those choices.

## Measurements

How fast compiled programs are is in [`benchmarks/`](../benchmarks/README.md): against C
([macOS](../benchmarks/against-c.md), [Windows](../benchmarks/against-c-windows.md)), the modules of `std`
against Rust and Zig ([the library](../benchmarks/library.md)), and every place where SG is still slower
([Slower than C](../benchmarks/slower-than-c.md)).

## Not here yet

- **Of default values of parameters (1620-1628)**: named arguments (`connect("a", retries: 5)`), so a call
  can leave out a parameter that is not last; a default found when the program runs (Kotlin and C++ run
  any expression at each call); a default carried by a `Func` value.
- **Two `ref` windows of one place at once (1610)**: a call is lent one range of a list or an array as `ref`,
  and nothing else of it; Rust's `split_at_mut`, which gives two that do not overlap, has no partner yet.

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
- A text constant has the type `string` here, not the design's `read string` (04, "String constant"). What the
  design wants of it is here all the same: it is not allocated or freed, has no capacity, and its first write or
  growth copies it, so a constant goes where a `string` or a `read string` goes with no `.clone()`. What differs:
  `var s = "x"` is a `string`, and nothing else: a constant goes where a `read string?` goes as it is too
  (`greet("")`, 1403).
- **Found writing the site (1402), in compiled code**: three things the interpreter runs and `sg build`
  refuses with "not in the C back end yet" and a hint to name the value first: `try` inside a larger
  expression (`print((try command.output()).trim())`), a text and then a value that lends with `ref` in one
  `$"..."` (`$"{pause} {path} {random.nextInt(1, 7)}"`), and a `$"..."` inside another.
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
- What Unicode has beyond the tables of 635 and the normal forms (1599): grapheme clusters, sorting by a language, the width
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
1501-1524). Real programs are now written in SG, Rust and Zig and measured against each other, each
difference a choice (1418, `trials/`): `grep` and `wc` first, which brought a lazy DFA to `Regex`, the C
compiler `CC` names and an `IoError` written as a sentence (1525-1539). What they found is made right before
the next trial (1543): `print` first, which writes into a block of the program's own, its lines put
together where they are printed (1544-1546), and ends compiled code where it cannot write, as it ends the
interpreter (1547); then reading lines and a search for a plain text, found by its two rarest bytes, and
UTF-8 checked faster (1549, 1550). Packages are here (1552, 1555-1583): a program's `sg.project` names
them, `sg add` fetches them by `git`, with no registry, pinned by the SHA-256 of their files in `sg.lock`
and kept in a cache, the versions chosen by minimal version selection, the `unsafe` of each allowed only
by the program, and a folder `internal` keeps a library's own modules to itself. A second review of safety
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
`JsonOptions` changes for both (1600, 1617-1619). Every module of `std` is now measured against Rust and
Zig, and a new one comes in with its measurement (1553, 1629-1637): JSON first, where a `JsonValue` takes 32
bytes and not 96 and an object gives its values in order (1638-1640), then compression and zip, where a
match is compared and copied eight bytes at a time, a list is made at its size (`List<T>.repeat`,
`List<T>.withCapacity`) and a zip reader lends its entries and holds 20 MB for 100,000 of them
(1642-1652), then SHA-256, HMAC, CRC-32 and Adler-32, which go through the processor's own instructions
where it has them and lose to Rust and Zig by at most 1.3 times, from six to nine (1653-1658); then, a slice
for each, hex, Base64 and Unicode; time, random numbers and the log; the network; and the command line,
the environment and paths. Then the checks of the DFA's reads and a search in any case
with vector instructions; then a `jq` of JSON, a key-value store with a write-ahead log, a pool of workers,
an HTTP/1.1 file server, a GIF tool and an interpreter of Lox. XML is not in `std` (1551, in place of 1413)
and will be a package of the language's own. After them come the reference of the language on the site
(1394), the rest of the third stage of the generator of programs, which moves, and the preview (942) with
`sg doc` (1392).
