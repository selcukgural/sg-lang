# The C back end

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
`weak` and type aliases, then written `using`) and the fifty-fifth (`shared checked`, the `as` blocks, `this` with a type and the leak
report) were written in the checker and the interpreter; since the fifty-sixth they are compiled too: a
`shared` value is a pointer with the counts of its owners and of its `weak` lookers in front of it, a
`shared checked` one has the borrows of it in progress there as well, and a debug build keeps the values
that are alive in a list for the leak report (347-355). The fifty-seventh slice (tasks, `locked` and
`lock`) was written in the checker and the interpreter, and since the fifty-eighth it is compiled: a task
is a thread of the system with a stack of 1 GB, taken as it is used (choice 1821), a `locked` value has a mutex in front of its counts, and a
`lock` block takes its values in the order of their addresses and gives them back on every way out
(371-383). The fifty-ninth slice (async functions, `await` and `Task.delay`, 384-402) and the sixtieth
(cancellation and async input and output, 403-416) were written in the checker and the interpreter; since
the sixty-first, async functions, delays and cancellation are compiled too: an async function is a machine
whose variables live in a frame, a task is one block with its frame inside, and a pool of threads, one
for each core (on Linux each core the program may run on, since the hundred-and-fifty-sixth slice), runs the
tasks that can go on (417-431). Since the sixty-second, async input and output is
compiled too: the work is done on threads of their own, started as they are needed, while the task waits,
and a read the block of a reader can answer is done in place (432-440). The sixty-third slice (a list of
tasks, `Task.whenAll` and `Task.whenAny`) was written in the checker, the interpreter and compiled code
together: a list of tasks has a group that its tasks tell as they end (441-452). So was the sixty-fourth
(streams opened, written and closed the async way): a writer an async task drops is put on the task's
list and closed on a thread of input and output where the task awaits it (453-465). So was the
sixty-fifth (the network: `TcpListener` and `TcpStream`, and a cancel that ends a wait for a socket or for
the input): one thread waits for every socket a task waits for, with epoll or kqueue (466-479; since the
eighty-first slice that thread is one of the pool, 667-677, since the eighty-second a socket goes on
the loop once, 678-688, and since the hundred-and-fifty-seventh each thread of the pool has a loop of its
own, which a socket stays on, and the clock's thread looks at the loop of a thread busy with one task,
1708-1714). So was the
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
[SG against C](../../benchmarks/against-c.md) and [on Windows](../../benchmarks/against-c-windows.md); the rules
that keep those numbers fair are in [How it is measured](../../benchmarks/method.md).

The choices made for the back end, from the sixteenth slice on, are in
[the record of 12 November](../../design/decisions/2026-11-12.md#slice-16) and the days after it.
