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
cargo run -- build examples/primes.sg -o primes # compiles it through C (needs `cc`)
cargo run -- emit-c examples/primes.sg          # prints the C
cargo test
cargo run -- build examples/primes.sg --release # the same as a release build: optimised, overflow wraps
sh bench/run.sh                                 # compiled SG against C written by hand
```

## What runs today (15 November 2026)

| Area | Supported |
|---|---|
| Functions | Top-level functions, type-first signature, recursion, any order. `void main()` is the entry point; `void main(List<string> args)` gets the words of the command line |
| Modules | A folder is a module: the files in it see each other with nothing written. `use stock` at the top of a file brings in the `public` names of the folder `stock`; `use shop.stock` for a folder inside a folder. The full name works without `use`: `billing.Line { ... }`, `stock.total(read xs)`, `stock.Color.Red`, `List<stock.Product>`. `public` on a struct, an enum, a function, a field or a method; without it the name stays inside its module. `readonly` on a field. The program is `sg main.sg` (the file is the root module, the folders next to it are the modules) or `sg folder` (every file of the folder is the root module). `sg build main.sg` and `sg build folder` compile the same program |
| Types | `sbyte`, `byte`, `short`, `ushort`, `int` (32 bit), `uint`, `long`, `ulong`, `nint`, `nuint`, `float`, `double`, `bool`, `char`, `string`, `void` as return type, structs, enums, `T*`, `T?`, `shared T`, `weak T`, `shared checked T`, `weak checked T`, `shared locked T`, `weak locked T`, `Task<R>`, `CancellationSource`, `CancellationToken`, `TcpListener`, `TcpStream`, `Result<T, E>`, `List<T>`, `HashMap<K, V>`, `HashSet<T>`, arrays `T[N]`, slices `read T[]` and `ref T[]`, `read T` and `ref T` in signatures |
| Type aliases | `using NodeRef = shared Node`, `public using P = Point`: a second name for the same type, which also builds it (`new NodeRef(1, null)`, `P(1, 2)`, `P { x = 1, y = 2 }`) |
| Types that carry a borrow | `struct Word<from source> { read string text` / `int start }`, `enum Token<from source> { Name(read string text) ... }`: a field can be a borrow when the type names its source. In a method `from source` says "from what this value borrowed, not from the value itself"; outside the type it is `from words.source` |
| Structs | `struct Node { int value` / `Node*? next }`, `Node { value = 1, next = null }`, field access and assignment through pointers and borrows |
| Methods | Written in the body of a struct or an enum with `this`, `read this` or `ref this` as the first parameter; fields are reached as `this.x`, and an enum looks at itself with `match this`. Called as `stack.push(1)`, `shape.area()`, also through pointers and borrows. With a type in front of `this` (`read FolderRef this`, `FolderRef this`) a method is called on the `shared` value that holds the struct. A function without `this` is called on the type: `Stack.create()`, `Shape.unit()`. `Stack(args)` is short for `Stack.create(args)` |
| Enums | `enum Shape { Circle(int radius)` / `Rect(int width, int height)` / `Empty }`, one variant per line, and methods after or between them. Built with the full name: `Shape.Circle(2)`, `Shape.Empty`, `new Expr.Number(1)` |
| `match` | `match x { Circle(r) => ... }` as an expression or a statement, on an enum, also behind pointers and borrows. Patterns: a variant with a pattern for each value it carries (a name, `_`, or a pattern inside: `Add(Number(0), right)`, `Word("let")`, `Paint(Red)`), `_`, `var other`; a condition with `when`. On an integer the patterns are numbers and characters, on a `char` characters (`match c { '+' => ... _ => ... }`), on a text texts (`match word { "if" => ... _ => ... }`), on a `bool` `true` and `false`. Patterns are joined with `or` (`Add(l, _) or Mul(l, _)`, `'+' or '-'`), and each names the same values; numbers and characters come in ranges that take both ends (`'a'..='z'`, `0..=9`). On a `T?` the patterns are `null` and those of the `T`, also inside a variant (`Node(null, _)`); a name below an arm `null` takes a value that is not `null`. A value no arm takes, or an arm that can never run, is a compile error that names the value: `does not handle Add(Name(_), _)`, `no arm takes 128..=255`. An arm is an expression, a statement (`r += 1`, `return x`, `break`) or a block whose last line is the value |
| Interfaces | Five built-in ones, written after the name: `struct Point : Copyable`, `enum AppError : Formattable`. A `Copyable` value is copied where another would be moved. A `Formattable` value is accepted by `print`, `$"..."` and `f.write`; a struct or an enum can write its own `void format(read this, ref Formatter f)`, otherwise the text is made from the fields. `Equatable` gives `==` and `!=` (field by field, or the type's own `bool equals(read this, read T other)`), `Hashable` lets the type be a key of a `HashMap` or an item of a `HashSet`, `Comparable` gives `<` `<=` `>` `>=` and `sort` through the type's `int compare(read this, read T other)` |
| `List<T>` | Built in. `List<int>()`, `List<int> { 1, 2 }`; `xs.add(v)`, `xs.length()`, `xs[i]` to read and to write (`xs[i] = v`, `xs[i] += 1`, `xs[i].name = ...`, `ref xs[i]`), `xs.get(i)` gives `null` where `xs[i]` would panic, `xs.removeLast()`, `xs.reverse()`, `xs.clear()`, `xs.addAll(items)` copies the items of a slice to the end (`buffer.addAll(text.asBytes())`, `xs.addAll(read ys)`, `xs.addAll(ys[2..])`) when they are `Copyable`. `xs.indexOf(v)` and `xs.contains(v)` on `Equatable` items, `xs.sort()` on `Comparable` ones (numbers and text are both). `string.join(", ", read names)` |
| `HashMap<K, V>` | Built in; a key is an integer, a `bool`, a `string` or a type written `: Equatable, Hashable`. `HashMap<string, int>()`, `HashMap<string, int> { {"x", 10} }`; `map.add(key, value)`, `map[key]` to read and to write what is under a key that is there, `map.get(key)` gives `null` for a missing key, `map.getOrAdd(key, initial)` gives a `ref V`, `map.contains(key)`, `map.remove(key)`, `map.length()`, `map.clear()` |
| Text of a collection | `print(xs)`, `$"{xs}"`: `[1, 2, 3]`, `{"ada": 36}`, `{1, 2, 3}`, when the items can be written as text |
| `HashSet<T>` | Built in; an item is what a key of a `HashMap` can be. `HashSet<int>()`, `HashSet<int> { 1, 2 }`; `set.add(v)` gives `true` if the item was not there, `set.contains(v)`, `set.remove(v)` gives `true` if it was there, `set.length()`, `set.clear()` |
| Arrays | `int[8] values`: a fixed number of items, kept where the value is (in the variable, inside the struct), with the length in the type. Built as `int[3] { 1, 2, 3 }` (the items), `byte[16](255)` (one value in every item) or `byte[16]` (zeros: 0, `false`, `null`). `xs[i]` to read and to write, `xs.length()`, `xs.get(i)`, `xs.indexOf(v)`, `xs.contains(v)`, `xs.sort()`, `xs.reverse()`, `==`, `print`. An array of items that can be copied is copied like a number; any other is moved |
| Slices | `read T[]` looks at items that something else owns, `ref T[]` can also write them. `read xs` and `ref xs` give one where a slice is expected, for a `List<T>` and a `T[N]`; a range gives one too: `xs[a..b]`, `xs[..b]`, `xs[a..]`, `xs[..]`, and `ref xs[a..b]` for a range that is written through. `s[i]`, `s.length()`, `s.get(i)`, `s.indexOf(v)`, `s.contains(v)`, `==`, `print`, `for`; `s.sort()` and `s.reverse()` through a `ref T[]`, and on a range in front of them: `xs[2..5].sort()` puts those items of `xs` in order where they are. `text.asBytes()` looks at a text as a `read byte[]`; `string.fromUtf8(bytes)` looks at bytes as text and gives a `Result<read string, TextError>` |
| `for` over a collection | `for x in xs` hands the collection over and `x` owns each item; `for x in read xs` and `for x in ref xs` lend, and so does a collection that is already borrowed. Over a `HashMap` the variable has a `key` and a `value`; a `HashSet` is gone over by hand-over or `read`. An array is gone over like a list; a slice lends its items |
| `is` | `if s is Circle(r) { ... }`, `if e is Add(Number(0), right) { ... }`, `if e is Add(l, _) or Mul(l, _) { ... }`, `while next() is not End { ... }`. The names reach along `&&`: `if int.parse(read text) is Ok(n) && n > 0 { ... }`, `return s is Circle(r) && r > 10`. The names of `is not` are seen where it fails: `if int.parse(read text) is not Ok(n) { return }` and `n` after it, `if r is not Ok(n) \|\| n < 0 { return }`, `else`, and after `while r is not Ok(n) { ... }` |
| Errors | `Result<T, E>` is built in: `return Ok(value)`, `return Ok()` for `Result<void, E>`, `return Error(cause)`. `try x` gives the value or returns the error to the caller, and on a `T?` returns `null`; on a `read T?` it gives a `read T`. `x ?? fallback` drops the error; the right side can be `return ...` or `panic(...)`. `match` and `is` see the variants `Ok` and `Error`. `_ = f()` drops a value on purpose. `Result<void, E> main()` prints the error it ends with and exits with code 1 |
| Ownership | `new x` gives `T*`; `read x` and `ref x` lend; passing or assigning a non-copyable value moves it. `slot.take()` on a `T?` gives the value and leaves `null`; `slot.replace(value)` gives the old value and leaves the new one |
| Several owners | `new shared x` gives a `shared T`, one of the owners of a value on the heap; `share x` makes one more without moving `x` (so does `x.clone()`), and a move hands one over. What a `shared T` points to is read (`node.value`, a method that takes `read this`, `match`, `for item in read node.items`, `read node` where a `read T` goes) and never written or taken out; what a `shared checked T` points to is changed too (the next row). `weak x` gives a `weak T`, which looks without owning: `share w` gives a `shared T?`, `null` once the last owner is gone. `same(a, b)` tells whether two point to one object, `==` compares what they point to |
| Changed through several owners | `new shared checked x` gives a `shared checked T`: what it points to is changed through any of its owners (`a.count += 1`, `a.items.add(x)`, a method that takes `ref this`, `ref a` where a `ref T` goes), and each access is checked against the borrows of it in progress, one that changes it or any number that read it. A read or a write borrows nothing after it; a value lent to a call is borrowed until the call returns, the head of a `for` for the loop, a `match` and an `is` with names for the statement. `read x as n { ... }` and `ref x as n { ... }` lend it for a block. A conflict through the same variable is a compile error, one through another variable a panic that shows both lines. A method can take the owner itself, `read FolderRef this`, so that it can write `share this` and `weak this`. The `shared` values still alive when a program or a test ends hold each other up and are reported (`leak: ...`; in compiled code by a debug build) |
| Tasks | `Task.run(work, x, share total)` starts `work(x, share total)` as a task and gives a `Task<R>`, `R` being what `work` returns; a `Func` can be started too. `t.wait()` waits for it and gives that value (`try t.wait()` passes its error on). A task that is not waited for on every path is a compile error, and so is `Task.run(...)` alone or dropped with `_ =`. What a task is given and gives back owns all it holds: no borrow, no `shared checked` value, no `Task` (the thread rule, also inside fields). In the interpreter a task runs on a thread of its own when it is waited for, so the output is the same on every run; compiled code starts a thread of the system for it at once. A panic inside one names the line that started it |
| Changed by several tasks | `new shared locked x` gives a `shared locked T`, which several tasks own; what it holds follows the thread rule and is reached only inside `lock total as c { c += 1 }`, where `c` is a `ref T` for the block. `lock a as x, b as y { }` takes two together; the same value twice is a panic. A `lock` block does not wait: no other `lock`, no `wait`, and no call of a function that does one, followed through the calls (a call of a `Func` there is refused when it could be one), so locks and waits cannot deadlock |
| Async | `async int count(string path) { ... }` is an async function: a call gives a `Task<int>` that has started and runs until it first waits, then the caller goes on, as in C#. `await t` waits for a task without holding the thread and gives what it returns (`try await f()` passes its error on); `await f(read xs)` runs `f` as part of this task, so it can borrow, while a call without `await` follows the thread rule. `Task.delay(ms)` is a task that ends after so many milliseconds. `async void main()` and `async Result<void, E> main()` await too. In an async function `wait` is an error (`await` instead), and so is anything that holds the thread: input and output that is not async, and a function that does it, followed through the calls (`await Task.run(work, x)` runs such work apart). `await` in a `lock` block is an error, and an async function that awaits itself, directly or around. Compiled too: a machine with its variables in a frame, run by a pool of threads |
| Cancellation | `CancellationSource()` or `CancellationSource.after(ms)` makes a source; `source.token()` gives a `CancellationToken` (`token.clone()` one more, `CancellationToken.None` one never cancelled), `source.cancel()` cancels, through a `read` borrow too. A task looks at its token where it chooses: `token.isCancelled()`, or `try token.check()`, which gives `Error(Cancelled { })`; `try` turns it into `IoError.Cancelled(...)` in a function that returns an `IoError`. `await Task.delay(ms, read token)` ends early with that error. Compiled too |
| A list of tasks | `var tasks = List<Task<int>>()`, `tasks.add(count(path))`: tasks kept in a list, which has only `add` and `length`. `await Task.whenAll(tasks)` takes the list and gives what every task gave, in the order of the list; `await Task.whenAny(ref tasks)` gives what the first task to end gave and takes it out of the list. Both are awaited, or waited for with `.wait()` in code that is not async, where they are written. A list of tasks is given to `Task.whenAll` or emptied by a loop `while tasks.length() > 0` without a `break`, or it is a compile error; a way out with an error waits for what is left in it. It is kept where a task is kept, and it does not go to another task. In the interpreter the tasks are waited for in the order of the list; compiled, each list has a group that its tasks tell as they end |
| Async input and output | `File.readTextAsync(read path)`, `readBytesAsync`, `writeTextAsync`, `writeBytesAsync`, `appendTextAsync`, `openTextAsync`, `createTextAsync`, `openAppendAsync`, on a reader (`Stdin` too) `readLineAsync(ref line)`, `readTextAsync()`, `readBytesAsync(ref buffer)`, `readSomeAsync(ref buffer)`, and on a writer (`Stdout` and `Stderr` too) `writeAsync(x)`, `writeLineAsync(x)`, `writeBytesAsync(bytes)`, `flushAsync()` and `closeAsync()`: the work of the function without `Async`, as a task, with a token that may come last (`read token`; not for `closeAsync`). A call given a borrow, and `closeAsync`, is awaited where it is written. A writer an async function drops without `closeAsync` is written and closed the async way, awaited where it is dropped. In the interpreter the work is done when it is called; the token is looked at first. Compiled too: the work is done on a thread of input and output while the task waits, and what the block of a reader or a writer can do is done in place. A read of an input that can make the program wait (a pipe, a terminal) waits on a thread in both, and a cancel ends the wait; what comes later stays for the next read |
| Network | `TcpListener.bind("127.0.0.1:8080")` (a number, `localhost`, or `":8080"` for every address of the machine; port 0 lets the system choose, `listener.port()` tells which), `listener.accept()`, `TcpStream.connect("example.com:80")` (the host looked up by name, each of its addresses tried in turn), `stream.receive(ref buffer)` (what has come, at most what the buffer holds; how many, 0 once the other side has let go) and `stream.send(bytes)` (all of them, through a `read` borrow): each gives a `Result` with an `IoError` (`ConnectionRefused`, `ConnectionReset`, `AddressInUse`, `HostNotFound`, `LineTooLong` are the network's). `stream.reader()` gives a `StreamReader` that borrows the stream and reads it through a block of its own (`readLine`, `lines()`, `readText`, `readBytes`, `readSome` and their async versions), while the stream goes on sending; a line or a text longer than 64 KB (`stream.reader(limit)` for another limit) is `LineTooLong`, and so is every read after it. A `receive` while the reader is used again is a compile error, and neither `send` nor `reader()` goes through a `shared` stream. `acceptAsync`, `connectAsync`, `receiveAsync` and `sendAsync` do the same work as a task, awaited where they are written, with a token that may come last; a cancel ends a wait at once and loses nothing that came. A socket sends without a buffer and is closed when it ends. In an async function the calls without `Async` are an error, as for files. In the interpreter a socket that is not ready gives the turn away, and the scheduler waits for the sockets with `poll` when no task can run. Compiled too: a call that has to wait waits with `poll`, and an async one on the loop of the network (epoll, kqueue), on a thread of its own |
| Null | `null`, `x is null`, `x is not null`, `a ?? b`, `a ?? panic("...")`, narrowing of local variables and parameters, also along `&&` and `\|\|` (`if name is not null && name.length() > 2`, `if text is null \|\| text.length() == 0 { return }`), under `!` (`if !(x is null)`) and after `while x is null { ... }` |
| Variables | `var x = 1`, `int x = 1`, assignment, `+=` `-=` `*=` `/=` `%=`, `&=` `\|=` `^=` `<<=` `>>=`. A variable keeps its type. An assignment computes its right side first, then finds the place |
| Control flow | `if` / `else if` / `else`, `while`, `for i in a..b`, `for x in xs`, `break`, `continue`, `return` |
| Operators | `+ - * / %`, `== != < <= > >=`, `&& \|\| !` (short circuit), unary `-`, cast `(int)x`. On integers `& \| ^ ~ << >>`; they bind tighter than a comparison, so `flags & mask == 0` tests the masked value |
| Numbers | `42`, `0xde`, `0b1010`, and `_` between digits: `1_000_000`, `0xdead_beef` |
| Text | `"..."` with `\n \t \r \0 \\ \" \'` and `\u{1F600}`, `$"{a}: {b}"` with `{{` and `}}`, `.toString()`, `text.length()`, `text.clone()`. `text[i]` reads one byte. `text[a..b]`, `text[..b]`, `text[a..]` and `text[..]` give a `read string`. `trim()`, `trimStart()`, `trimEnd()` give a `read string`; `indexOf(x)` and `lastIndexOf(x)` a `nint?` in bytes; `contains(x)`, `startsWith(x)`, `endsWith(x)`, where `x` is a `char` or a `read string` (or a `byte` for the two positions); `equalsIgnoreAsciiCase(other)`. `replaceAll(old, new)` (each a `char` or a `read string`) and `repeat(n)` give a new `string`; a call of them whose result is not used is an error |
| Building text | A `string` grows at its end: `line.write(x)` adds whatever `print` takes (`$"{name}: {count}\n"`, a number, a `char`, a text, a `Formattable` value), `line.clear()` empties it and keeps its memory, `string.withCapacity(n)` is an empty text with room for `n` bytes |
| Writing a number | Inside `$"..."` a width and a letter may follow the value, as in C#: `{n,5}` takes at least five characters with spaces in front, `{name,-10}` with spaces after (counted in characters, for every value `print` takes); `{b:X2}` and `{crc:x8}` write hex with at least so many digits, `{mask:b8}` binary, `{n:D3}` decimal with at least three digits, `{price:F2}` a fractional number with two digits after the point; `{price,8:F2}` both |
| Going over text | `for c in text.chars()` gives each `char`; `text.split(',')` (a `char` or a `read string`), `text.splitWhitespace()` and `text.lines()` give each part as a `read string`, nothing copied. Kept in a `var`, the iterator gives the next part with `parts.next()`, `null` at the end |
| Characters | `char`: one Unicode character. `'+'`, `'é'`, `'\n'`, `'\u{e9}'` is a `char`, and a `byte` where an integer goes (`byte b = '+'`, `text[i] == ':'`). `(int)c` is its code, `(char)n` the character of a code, `n.tryToChar()` gives `null` where `(char)n` would panic. `==`, `<`, `match`, keys, `sort`, `print`. `isAsciiDigit()`, `isAsciiLetter()`, `isAsciiAlphanumeric()`, `isAsciiUpper()`, `isAsciiLower()`, `isAsciiWhitespace()`, `isAscii()`, `toAsciiUpper()`, `toAsciiLower()` on `char` and on `byte`; `isWhitespace()` and `utf8Length()` on `char` |
| Function values | `Func<int, bool>`: the address of a named function, the last type is what it returns (`Func<void>` takes and returns nothing). The name of a function is the value (`apply(double, 5)`, `Func<int, int> f = double`); it is copied freely, kept in variables, fields, variants and lists, and called like a function: `f(4)`, `this.onClick(read this.label)` |
| Built-ins | `print(x)`, `printError(x)` (the same, to the standard error), `panic("...")`, `assert(condition)`, `exit(code)` (ends the program with that code), `same(a, b)` (two `shared` values point to one object) |
| Files and the input | `File.readText(path)` gives a `Result<string, IoError>`, `File.readBytes(path)` a `Result<List<byte>, IoError>`; `File.writeText(path, text)`, `File.writeBytes(path, bytes)` and `File.appendText(path, text)` a `Result<void, IoError>`. `File.openText(path)` gives a `Result<StreamReader, IoError>`, `File.createText(path)` a `Result<StreamWriter, IoError>`, and `File.openAppend(path)` one that writes after what the file holds. A reader has `readLine(ref line)` (the next line into `line`, `false` at the end), `readText()` (what is left), `lines()` (each line as a `Result<string, IoError>`) `readBytes(ref buffer)` (bytes into the buffer, as many as it holds unless the end comes first, and how many: a `Result<nint, IoError>`) and `readSome(ref buffer)` (what the block holds, or what one read of the system gives when it is empty; how many, 0 at the end); a writer has `write(x)` and `writeLine(x)` (what `print` takes), `writeBytes(bytes)` (a `read byte[]` as it is), `flush()` and `close()`, each with a `Result<void, IoError>`. `Stdin` is a `StreamReader`, `Stdout` and `Stderr` are `StreamWriter`s, lent on with `ref Stdin`. The built-in `enum IoError { NotFound` / `PermissionDenied` / `IsDirectory` / `NoSpace` / `InvalidText(long position)` / `Other(int code) }` |
| Tests | `test "name" { ... }` at the top level of a file, run by `sg test file.sg`: one line per test (`ok` or `FAIL` with the reason), exit code 1 if any failed. In a test `try` passes on an error of any type, and the test fails with it. A test that passes and leaves `shared` values alive fails with the leak report. A normal run leaves the tests out |
| Comparing by name | `a.compare(b)` on numbers, `char` and text (negative, 0, positive), `a.equals(b)` on those and `bool`: what `<` and `==` are short for |
| Integer methods | `wrappingAdd`, `wrappingSubtract`, `wrappingMultiply`; `tryAdd`, `trySubtract`, `tryMultiply`, `tryDivide` give `null` where the operator would panic. `tryToInt()` and the others give `null` where a cast would panic, `wrappingToInt()` and the others keep the bits that fit; `min`, `max` |
| Fractional methods | `abs`, `sqrt`, `floor`, `ceil`, `round`, `truncate`, `isNaN`, `isInfinite`; `exp`, `log`, `log2`, `log10`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`, `pow`, `fusedMultiplyAdd`, `min`, `max`; `tryToInt()` and the others |
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
  number is printed with the fewest digits that read back as it (`0.1`, `2`, `1e+21`).
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
- Every field of a struct literal gets a value. A struct or an enum cannot contain itself by value; the hint
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
  when one of the two is `ref`.
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
memory behind (246 of 1006 programs for now; the others are almost all programs the checker rejects, on purpose; of the programs that run, one uses what it does not compile yet, 268). A
program that ends in a panic is not looked at for leaks: a panic does not unwind, and the system takes back
what the program owned (382). A
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
the input): one thread waits for every socket a task waits for, with epoll or kqueue (466-479). So was the
sixty-sixth (lines over a connection): `conn.reader()` is a `StreamReader` that borrows the connection
and keeps what has come in its block until a read can take all it needs, `send` goes through a `read`
borrow, and `readSome` gives what is there (480-490). What is still missing is an error that names it (not silence): the few positions listed with the
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

Measured (`sh bench/run.sh`, Apple clang 21 on arm64, best of five, `-O2` for all three; the C is written by
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
| `show`: two million structs and half a million lists of numbers printed, one on each line | 0.214 s | 0.191 s | 0.374 s | 0.57 |
| `lookup`: three hundred million lookups in a list of records of 128 bytes, through a function that gives back a `read Record?` | 0.498 s | 0.488 s | 0.491 s | 1.01 |
| `buffer`: 32 bytes on the stack written as hex into a buffer of 64 through a `ref byte[]`, added up through a `read byte[]` and a range of it, fifteen million times | 0.280 s | 0.290 s | 0.283 s | 0.99 |
| `bits`: a sieve below sixty-four million with one bit for each number, then thirty million numbers written seven bits at a time into a buffer on the stack and read back | 0.322 s | 0.321 s | 0.407 s | 0.79 |
| `hash`: XXH64, the 64-bit xxHash, written with `ulong`, over one megabyte, two thousand times, the bytes read eight at a time with `ulong.fromLittleEndian` | 0.128 s | 0.129 s | 0.128 s | 1.00 |
| `nbody`: the sun and the four giant planets moved twenty million steps in `double`, a square root for each pair at each step | 0.661 s | 0.662 s | 0.679 s | 0.97 |
| `numbers`: a million lines of whole and fractional numbers read ten times with `long.parse` and `double.parse` and added up | 0.186 s | 0.191 s | 0.196 s | 0.95 |
| `fields`: a million lines like ` Ada17 , 120, -45 ,6789` gone over twenty times with `lines`, `split`, `trim`, `long.parse` and `chars` | 1.228 s | 1.259 s | 1.202 s | 1.02 |
| `lines`: two million lines of a log (125 MB) read from the input with `Stdin.readLine`, each checked to be UTF-8, and counted | 0.096 s | 0.097 s | 0.089 s | 1.08 |
| `copy`: the same log read from a file with `File.openText` and written to another, each line with its number in front, through `File.createText` | 0.252 s | 0.253 s | 0.270 s | 0.93 |
| `bytes`: the same log read in pieces of 4 KB with `readBytes` and written with `writeBytes` through a writer from `File.openAppend` | 0.030 s | 0.030 s | 0.032 s | 0.94 |
| `table`: two million lines printed with widths, hex, binary and two digits after the point (`{i:x8} {a,10}  {name,-8}\|{x,10:F3}  {mask:b16} {code:X6}`) | 0.471 s | 0.471 s | 0.533 s | 0.89 |
| `blocks`: eight million pieces of 1 to 255 bytes (about a gigabyte) gathered with `addAll` into a `List<byte>` that is emptied at 64 MB | 0.035 s | 0.035 s | 0.035 s | 1.00 |
| `keywords`: a million words sorted forty times into twelve keywords and the rest by a `match` on text | 0.199 s | 0.200 s | 0.194 s | 1.03 |
| `classes`: ten million bytes sorted twenty times into the classes of a lexer by a `match` of ranges and `or` (`'a'..='z' or 'A'..='Z' or '_'`) | 0.239 s | 0.299 s | 0.274 s | 0.87 |
| `rewrite`: two hundred thousand lines gone over twenty times, the tabs replaced by four spaces and a word by another with `replaceAll`, and an indent made with `repeat` | 0.588 s | 0.597 s | 0.625 s | 0.94 |
| `tree`: the design's folder tree, 111,111 `shared checked` folders that look at their parent with `weak`, each adding a size to every folder above it, built and freed twenty times | 0.095 s | 0.102 s | 0.062 s | 1.53 |
| `tasks`: four tasks at a time over the numbers below 400,000, ten times; each adds up Collatz lengths and adds them to a `shared locked` total every 100 numbers | 0.115 s | 0.188 s | 0.115 s | 1.00 |
| `async`: for each number below a million, an async function starts a task for one Collatz length and awaits another in place; nothing has to wait | 0.244 s | 0.372 s | 0.226 s | 1.08 |
| `asynclines`: `lines` the async way, the two million lines read with `await Stdin.readLineAsync` in an async `main` (the C is that of `lines`) | 0.125 s | 0.125 s | 0.090 s | 1.37 |
| `tasklist`: in each of 200,000 rounds, eight async tasks for Collatz lengths taken with `Task.whenAll` and eight with `Task.whenAny`; none has to wait (the C keeps each result in a block it allocates, in an array for the list) | 0.476 s | 0.689 s | 0.384 s | 1.24 |
| `asynccopy`: `copy` the async way, the log read with `await reader.readLineAsync` and written with `await writer.writeLineAsync` in an async `main` (the C is that of `copy`) | 0.301 s | 0.299 s | 0.275 s | 1.09 |
| `echo`: an echo server and eight clients in one async program over the loopback, each client sending 64 bytes and waiting for them to come back 20,000 times (the C blocks, with a thread for each connection and each client) | 2.083 s | 2.082 s | 1.119 s | 1.86 |
| `echosync`: `echo` without async, the server's connections and the clients on threads of `Task.run` (the C is that of `echo`) | 1.108 s | 1.118 s | 1.115 s | 0.99 |
| `netlines`: the two million lines of `lines` sent over the loopback by one task and read line by line by another with `await reader.readLineAsync(ref line)` (the C blocks, with `getline` from a `FILE` over the socket, which checks nothing) | 0.117 s | 0.117 s | 0.080 s | 1.46 |

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
`double` tries up to seventeen lengths ("Slower than C" below); every text made by `toString` was written
twice, once to find its length (now once, choice 166); and every fractional number went to `strtod`
through a copy, where most are short enough to be one correctly rounded division (choice 164). The
overflow-checked build takes the same time: the digits are added up by the runtime on 128 bits, where
nothing can overflow, and the one checked addition for each line is a test that is never taken.

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
`getline` into one buffer, as one does. SG takes 1.08 times its time, and all of that is the check that
every line is UTF-8, which `getline` does not make: with the check taken out of the generated C the SG
program takes 0.058 s against 0.090 s, since it finds a line with `memchr` in a block of its own where
`getline` goes through `FILE*`. The check first took the program to about 1.15 times; looking at eight
bytes at a time while they are ASCII brought it to 1.08 (choice 202). The rest is in "Slower than C".

`copy` reads the same log from a file, opened with `File.openText` and read with `readLine` into one
text, and writes each line with its number in front (`17: INFO 350 ...`) with `writeLine` through a
writer from `File.createText`, 140 MB in all. The C program reads with `getline` and writes with
`fprintf` into a `FILE*` with a buffer of 64 KB. SG takes 0.93 times its time (0.25 s against 0.27 s,
over three runs of five): it checks every line and still reads faster, as `lines` showed, and the
writing is a `vsnprintf` into the writer's block where C's is an `fprintf`.

`bytes` copies the same log as bytes: `readBytes` into a buffer of 4 KB on the stack, and `writeBytes` of
each piece into a writer from `File.openAppend`, after the file is emptied. The C program does the same
with `fread` and `fwrite` on `FILE*`s with buffers of 64 KB. SG takes 0.94 times its time (0.030 s against
0.032 s, the best of fifteen runs: the program is short, and one run of five gave a cold start of 0.09 s).
Both are copies between blocks; neither looks at the bytes.
`table` prints two million lines, each one `print` of a formatted text with a width on two numbers and on a
name, hex with eight and with six digits, sixteen binary digits and a fractional number with three digits
after the point. The C program is one `printf` a line (`%08llx %10lld  %-8s|%10.3f  %s %06X`), with the
binary digits put together by a small function first, since `printf` has no conversion for them. SG takes
0.89 times its time. Its first version took 1.30 times: the fractional number was written by `snprintf`
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
| `src/interp.rs` | Runs the checked tree; only data-dependent failures (panics) are left to it. Every task that runs code is a fiber, a thread of its own of which one runs at a time |
| `src/emit_c.rs` | The back end: writes the checked tree as C, with the same panics; tasks are threads of the system |
| `src/opening.rs` | After the checker: where a `shared checked` value is reached and for how long it is borrowed (the `Open` and `Within` nodes the interpreter follows), the conflicts through the same variable that are certain before the program runs, and the borrows that would outlive their line |
| `src/waits.rs` | After the checker, on the whole program: which functions wait (take a lock, wait for a task or await one, themselves or through the functions they call), and what waits inside a `lock` block; which functions hold the thread, and what does in an async function; an async function that awaits itself |
| `src/sums.rs` | For the back end: the sums in a loop that cannot overflow for a number of rounds, so that a checked build runs those rounds without their test |
| `src/diag.rs` | Error and panic messages |
| `src/source.rs` | The files of a program: which folders are modules and which of them are read. All files are kept as one text, so a position says which file it is in |
| `src/lib.rs`, `src/main.rs` | Library entry point and the command line |

It is a library with a thin command line on top, so that a language server can later reuse the parser and the
checks.

Tests are golden files: every `.sg` file in `examples/` and `tests/cases/` is run and its output (what it
printed, then the error or panic message) is compared with the `.out` file next to it. After an intended change,
regenerate them with `UPDATE_GOLDEN=1 cargo test` and review the diff. What the program wrote to the standard
error comes after what it printed, and an exit code other than 0 and 1 is a last line (`exit code 3`). A
`.args` file next to a program gives its command line, one word on each line, where `{dir}` is an empty folder
of its own to write files into (it is written back as `{dir}` in the output); a `.in` file gives its input.

Programs with modules are in `tests/modules/`: the folders there (`stock`, `billing`, `shop/parts`, `leaky`,
`links`, `aliasleak`) are modules, and each `.sg` file next to them is a program of one file that uses them. A folder with a `main.sg`
(`tests/modules/rootdir`, `examples/shop`) is a program run as a folder; its output is in `main.out` and what
the test command prints for it in `tests.out`.

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
the same block.

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
    `cargo test`. The path on Linux was first run in the fifty-eighth slice (382, 383).

Choices made in the twenty-ninth slice (the test command on compiled code), approved on 13 November 2026:

83. **`sg test <file | folder> --compiled` runs the tests as compiled code.** Without the word the
    interpreter runs them, as before: it is the reference and needs no C compiler. `sg build <path> --test`
    writes the program that runs the tests and keeps it, `sg emit-c <path> --test` prints its C. A debug
    build, so overflow is checked as it is by the interpreter.
84. **Each `test` block runs in a process of its own**, as the design decided: a panic ends that test and
    the ones after it still run. The program starts a copy of itself for each test (`fork`) and waits for
    it. For now that is POSIX only; it does not build on Windows.
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
     `equalsIgnoreCase` gets the hint. The design's HTTP example changes in that one word.
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
283. **A built-in method that changes its value and gives one too may stand alone**: `xs.removeLast()`, the
     `add` and `remove` of a `HashSet` (they give whether something changed), the `remove` and `getOrAdd`
     of a `HashMap`, and `next` on an iterator. The change is what they are called for. The ones that give
     nothing (`add` on a list, `sort`, `write`, `clear`) are statements as before.
284. **Functions and methods you write are left as they are**: `noisy(4)` and `stack.push(5)` alone are
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
     one, as a `var` does. They stay borrows of the tested value, so changing or moving that value while
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
     there.
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
     output, which a cancel leaves (the thread frees what it found).
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

## Slower than C, and could be better

Places where a measurement found SG slower than the same program in C and a fix is in sight. They are kept
here to be talked over when their time comes; none of them breaks a program, and none is fixed by turning a
check off.

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
- **Printing a fractional number** (`numbers`' first version, 2.7 times with the text built by
  `toString`): the fewest digits are found by writing the number with 1, 2, ... 17 digits and reading each
  back (choice 150), up to seventeen `snprintf` and `strtod` for one number, where C's `printf("%g")` is
  one. The interpreter finds them the same way. Candidates: a shortest-digits algorithm (Ryu, as Rust
  uses), in both, so that they still agree by construction.
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
- **Reading the input the async way** (`asynclines`, 1.37 times C, choice 439): a line the block holds
  is read in place, but each block of 64 KB is filled on a thread of input and output, and the task is
  woken through the pool after it: about 12 µs a block, 0.023 s of the 0.035 s. Candidates: the thread
  fills the next block while the task reads this one (reading ahead), a larger block for a reader read
  the async way, and a task that goes on on the thread that woke it.
- **Copying a file the async way** (`asynccopy`, 1.09 times C, choice 464; 0.94 without async): the
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
- **Waiting for the network the async way** (`echo`, 1.86 times C, choice 478; 0.99 without async): a
  task whose socket is not ready is woken by the thread of the loop, which the system woke, through the
  pool: two threads woken for each wait where C's blocking call wakes one, and the socket put on the loop
  again each time. Candidates: the threads of the pool wait for the loop themselves when they have nothing
  to do, so the one the system wakes runs the task (as Tokio's workers do), and a socket that stays on the
  loop, woken by its edges, instead of being put on again for each wait.
- **Checking that a line is UTF-8** (`lines`, 1.08 times `getline`, which checks nothing; without the
  check the SG program takes 0.058 s against 0.090 s; `netlines`, 1.46 times, 0.89 without it, choice
  488, where the C reads the lines while another thread reads the file, so nothing hides the check): the check looks at eight bytes at a time while they
  are ASCII and at one byte at a time from the first that is not (choice 202). Candidates: a check with
  vector instructions (as `simdutf` does, and Rust's `from_utf8` in part), used by every place that
  checks: `readText`, `readLine`, the command line, `string.fromUtf8`.

## Not here yet

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
- Interfaces of your own (`interface`, `where`) and the other built-in ones (`Clonable`, `Disposable`). `.toString()` on a `Formattable` struct or enum.
- Generics other than the built-in `Result<T, E>`, `List<T>`, `HashMap<K, V>` and `HashSet<T>` (generics of your own are deferred
  in the design), and so an alias with type parameters (`using Link<T> = ...`).
- Channels; the task of `Task.whenAll` kept in a variable to be awaited later (`var all =
  Task.whenAll(tasks)`), and a `for` over a list of tasks (a `break` would leave tasks); a method or a function of a struct
  started as a task (a `Func` is a function written at the top level). Of async: `linesAsync` (a `for`
  that awaits), an `async` method, an async function kept as a `Func`. A token cancelled while a file is
  read or written does not stop the work (433); one cancelled while a task waits for the network or for a
  line of the input that can make the program wait does (471, 472). Of the network: a stream received from
  by one task and sent to by another (and so `send` through a `shared` stream), the reader of a connection
  kept in a field or given back (it carries a borrow; `StreamReader<from conn>`), a writer of a connection,
  closing one direction (`shutdown`), the
  address of the other side, the options of a socket (`TCP_NODELAY`), looking a name up alone, UDP, a
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
- What text needs Unicode tables for: changing the case of a text (`toUpper`), letters and digits beyond
  ASCII (`isLetter`), comparing without case beyond ASCII. Replacing only the first places
  (`replacen`), replacing in place without a new text; a `padLeft` is a width: `$"{name,10}"` (229); the positions
  of the characters (`charIndices`); splitting at most n times. Writing the type of an
  iterator, and a `for` over a type of your own that has `next` (with generics). `text.capacity()` and
  `text.reserve(n)`; a `ref string` given to a `format` method in place of its `Formatter`.
- A number is written in decimal, hex and binary (`{n:x}`, `{n:b}`) and read in decimal and hex
  (`parse`, `parseHex`), not in binary. C#'s other letters (`E`, `G`, `N`, `C`, `P`) and formats made of
  characters (`{x:0.00}`), a fill other than a space, centring, and a width that comes from a variable
  (choices 224, 229). A fractional number is not read out of bytes or written into them
  (`double.fromLittleEndian`, the bits of a `double` as a `ulong`), and the order of the bytes is not
  turned around inside a number (`n.reverseBytes()`). `abs` and
  `clamp` on integers, `sinh` and the other functions of `<math.h>` that are not listed in 162.
- Arrays of arrays, `read T[]?`, an array as a key, looking for a run of items in a slice. `addAll` of
  items that are not `Copyable`, moving every item of one list into another, adding a range of a list to
  the same list (`extend_from_within`), a sort that does not allocate (choices 235, 236, 239).
- A panic of `-x` and of a shift names no way out: there is no `tryNegate` or `tryShiftLeft` yet.
- `Closable` and `Disposable` on a type of your own. Folders, removing or renaming a file, asking whether one is there; the
  variables of the environment; the command line as bytes (a word that is not UTF-8 stops the program).
  `IoError` does not carry the path (choice 195). `PermissionDenied` and `NoSpace` are mapped but have no
  test: the tests cannot make a file they may not read or fill a disk.
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
its choice with `exit`. Reading two million lines takes 1.08 times `getline` in C, which does not check
that they are text. A file is read and written piece by piece through a `StreamReader` and a
`StreamWriter`, `Stdin`, `Stdout` and `Stderr` are such streams, and a writer left open in a local
variable is a compile error; copying the log line by line into another file takes 0.93 times the time
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
So is every other built-in method that only gives a value (`line.trim()`, `xs.contains(3)` alone), while
the ones that change their value (`removeLast`) and the functions you write may stand alone. `!` swaps what
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
the async way takes 1.37 times the same program in C (without async, about the time of C). Tasks are kept
in a list (`List<Task<R>>`) and waited for all at once (`await Task.whenAll(tasks)`, what each gave in the
order of the list) or as they end (`await Task.whenAny(ref tasks)`), and a list whose tasks are not all
waited for is a compile error; compiled, each list has a group that its tasks tell as they end. Streams
are opened, written and closed the async way (`File.createTextAsync`, `await writer.writeLineAsync(x)`,
`await writer.closeAsync()`), and a writer an async function drops on its way out is written and closed
the async way too, awaited where it is dropped, so a thread of the pool never writes a file; copying the
log line by line the async way takes 1.09 times the same program in C. The network is there too: a
`TcpListener` takes connections and a `TcpStream` receives and sends bytes, without async and with it;
compiled, one thread waits for every socket a task waits for, with epoll or kqueue, and a cancel ends a
wait for a socket, or for a line of the input, at once. An echo server with eight clients takes 1.86 times
the same program in C with blocking threads the async way, and the time of C without async. A connection
is read line by line through a `StreamReader` of its own (`conn.reader()`), which borrows it while it
goes on sending, and a line longer than 64 KB is an error; reading two million lines sent over the
loopback takes 1.46 times `getline` in C, which does not check that they are text (0.89 times without the
check). Next is open: a stream shared by a reading and a writing task, a faster check of UTF-8, the
threads of the pool waiting for the loop themselves, `linesAsync`, and an `async` method.
