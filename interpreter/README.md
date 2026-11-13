# Interpreter

A tree-walking interpreter for SG, written in Rust with no dependencies. Its job is to test the design by
running real programs, not to be fast. Source files end in `.sg`; the crate and the command are named `sg`.

```
cargo run -- examples/stack.sg                  # runs the program
cargo run -- test examples/tests_stock.sg       # runs its tests
cargo run -- examples/shop/main.sg              # a program with modules: the folders next to the file
cargo run -- test examples/shop                 # the tests of every module of the program in that folder
cargo run -- build examples/primes.sg -o primes # compiles it through C (needs `cc`)
cargo run -- emit-c examples/primes.sg          # prints the C
cargo test
cargo run -- build examples/primes.sg --release # the same as a release build: optimised, overflow wraps
sh bench/run.sh                                 # compiled SG against C written by hand
```

## What runs today (13 November 2026)

| Area | Supported |
|---|---|
| Functions | Top-level functions, type-first signature, recursion, any order. `void main()` is the entry point |
| Modules | A folder is a module: the files in it see each other with nothing written. `use stock` at the top of a file brings in the `public` names of the folder `stock`; `use shop.stock` for a folder inside a folder. The full name works without `use`: `billing.Line { ... }`, `stock.total(read xs)`, `stock.Color.Red`, `List<stock.Product>`. `public` on a struct, an enum, a function, a field or a method; without it the name stays inside its module. `readonly` on a field. The program is `sg main.sg` (the file is the root module, the folders next to it are the modules) or `sg folder` (every file of the folder is the root module). `sg build main.sg` and `sg build folder` compile the same program |
| Types | `byte`, `int` (32 bit), `long`, `nint`, `bool`, `string`, `void` as return type, structs, enums, `T*`, `T?`, `Result<T, E>`, `List<T>`, `HashMap<K, V>`, `HashSet<T>`, `read T` and `ref T` in signatures |
| Types that carry a borrow | `struct Word<from source> { read string text` / `int start }`, `enum Token<from source> { Name(read string text) ... }`: a field can be a borrow when the type names its source. In a method `from source` says "from what this value borrowed, not from the value itself"; outside the type it is `from words.source` |
| Structs | `struct Node { int value` / `Node*? next }`, `Node { value = 1, next = null }`, field access and assignment through pointers and borrows |
| Methods | Written in the struct body with `this`, `read this` or `ref this` as the first parameter; fields are reached as `this.x`. Called as `stack.push(1)`, also through pointers and borrows. A function without `this` is called on the type: `Stack.create()`. `Stack(args)` is short for `Stack.create(args)` |
| Enums | `enum Shape { Circle(int radius)` / `Rect(int width, int height)` / `Empty }`, one variant per line. Built with the full name: `Shape.Circle(2)`, `Shape.Empty`, `new Expr.Number(1)` |
| `match` | `match x { Circle(r) => ... }` as an expression or a statement, on an enum, also behind pointers and borrows. Patterns: a variant with a name or `_` for each value, `_`, `var other`; a condition with `when`. On an integer the patterns are numbers and characters: `match c { '+' => ... _ => ... }`. An arm is an expression, a statement (`r += 1`, `return x`, `break`) or a block whose last line is the value |
| Interfaces | Five built-in ones, written after the name: `struct Point : Copyable`, `enum AppError : Formattable`. A `Copyable` value is copied where another would be moved. A `Formattable` value is accepted by `print`, `$"..."` and `f.write`; a struct can write its own `void format(read this, ref Formatter f)`, otherwise the text is made from the fields. `Equatable` gives `==` and `!=` (field by field, or the struct's own `bool equals(read this, read T other)`), `Hashable` lets the type be a key of a `HashMap` or an item of a `HashSet`, `Comparable` gives `<` `<=` `>` `>=` and `sort` through the struct's `int compare(read this, read T other)` |
| `List<T>` | Built in. `List<int>()`, `List<int> { 1, 2 }`; `xs.add(v)`, `xs.length()`, `xs[i]` to read and to write (`xs[i] = v`, `xs[i] += 1`, `xs[i].name = ...`, `ref xs[i]`), `xs.get(i)` gives `null` where `xs[i]` would panic, `xs.removeLast()`, `xs.reverse()`, `xs.clear()`. `xs.indexOf(v)` and `xs.contains(v)` on `Equatable` items, `xs.sort()` on `Comparable` ones (numbers and text are both). `string.join(", ", read names)` |
| `HashMap<K, V>` | Built in; a key is an integer, a `bool`, a `string` or a type written `: Equatable, Hashable`. `HashMap<string, int>()`, `HashMap<string, int> { {"x", 10} }`; `map.add(key, value)`, `map[key]` to read and to write what is under a key that is there, `map.get(key)` gives `null` for a missing key, `map.getOrAdd(key, initial)` gives a `ref V`, `map.contains(key)`, `map.remove(key)`, `map.length()`, `map.clear()` |
| Text of a collection | `print(xs)`, `$"{xs}"`: `[1, 2, 3]`, `{"ada": 36}`, `{1, 2, 3}`, when the items can be written as text |
| `HashSet<T>` | Built in; an item is what a key of a `HashMap` can be. `HashSet<int>()`, `HashSet<int> { 1, 2 }`; `set.add(v)` gives `true` if the item was not there, `set.contains(v)`, `set.remove(v)` gives `true` if it was there, `set.length()`, `set.clear()` |
| `for` over a collection | `for x in xs` hands the collection over and `x` owns each item; `for x in read xs` and `for x in ref xs` lend, and so does a collection that is already borrowed. Over a `HashMap` the variable has a `key` and a `value`; a `HashSet` is gone over by hand-over or `read` |
| `is` | `if s is Circle(r) { ... }`, `while next() is not End { ... }` |
| Errors | `Result<T, E>` is built in: `return Ok(value)`, `return Ok()` for `Result<void, E>`, `return Error(cause)`. `try x` gives the value or returns the error to the caller, and on a `T?` returns `null`. `x ?? fallback` drops the error; the right side can be `return ...` or `panic(...)`. `match` and `is` see the variants `Ok` and `Error`. `_ = f()` drops a value on purpose. `Result<void, E> main()` prints the error it ends with and exits with code 1 |
| Ownership | `new x` gives `T*`; `read x` and `ref x` lend; passing or assigning a non-copyable value moves it. `slot.take()` on a `T?` gives the value and leaves `null`; `slot.replace(value)` gives the old value and leaves the new one |
| Null | `null`, `x is null`, `x is not null`, `a ?? b`, `a ?? panic("...")`, narrowing of local variables and parameters |
| Variables | `var x = 1`, `int x = 1`, assignment, `+=` `-=` `*=` `/=` `%=`. A variable keeps its type |
| Control flow | `if` / `else if` / `else`, `while`, `for i in a..b`, `for x in xs`, `break`, `continue`, `return` |
| Operators | `+ - * / %`, `== != < <= > >=`, `&& \|\| !` (short circuit), unary `-`, cast `(int)x` |
| Text | `"..."` with `\n \t \r \0 \\ \" \'`, `$"{a}: {b}"` with `{{` and `}}`, `.toString()`, `text.length()`, `text.clone()`, `text.indexOf(':')`. `text[i]` reads one byte. `text[a..b]`, `text[..b]`, `text[a..]` and `text[..]` give a `read string` |
| Characters | `'+'`, `'\n'`: a `byte` |
| Function values | `Func<int, bool>`: the address of a named function, the last type is what it returns (`Func<void>` takes and returns nothing). The name of a function is the value (`apply(double, 5)`, `Func<int, int> f = double`); it is copied freely, kept in variables, fields, variants and lists, and called like a function: `f(4)`, `this.onClick(read this.label)` |
| Built-ins | `print(x)`, `panic("...")`, `assert(condition)` |
| Tests | `test "name" { ... }` at the top level of a file, run by `sg test file.sg`: one line per test (`ok` or `FAIL` with the reason), exit code 1 if any failed. In a test `try` passes on an error of any type, and the test fails with it. A normal run leaves the tests out |
| Comparing by name | `a.compare(b)` on integers and text (negative, 0, positive), `a.equals(b)` on those and `bool`: what `<` and `==` are short for |
| Integer methods | `wrappingAdd`, `wrappingSubtract`, `wrappingMultiply`; `tryAdd`, `trySubtract`, `tryMultiply`, `tryDivide` give `null` where the operator would panic |

Design rules that are already enforced:

- Integer overflow panics in the interpreter, in tests and in a debug build; a release build wraps (Rust's
  way). `wrappingAdd` wraps in every build, `tryAdd` is checked in every build.
- `int` widens to `long` or `nint` on its own. Narrowing is written `(int)x` and panics if the value does not fit.
- `$"..."` is not a `string`; `string text = $"..."` is an error whose hint is `.toString()`.
- `assert` prints the expression text and, for a comparison, the value of both sides.
- Deep recursion ends in a readable panic (limit: 5000 nested calls), not a crash.
- Errors come with the source line, a marker and, where the fix is known, a hint.
- Types are checked before the program runs. An integer literal takes the type it is used as
  (`long x = 5`); one that does not fit is an error.
- `T` is never `null`; only `T?` is. Reaching into a `T?` without a test is an error. After
  `if x is not null { ... }`, `while x is not null { ... }` or `if x is null { return }` the variable counts as
  `T` until it is assigned again. Fields are not narrowed: copy to a local first.
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

## The C back end

Since the sixteenth slice (12 November 2026) a program can be compiled: `sg build file.sg` writes it as C and
hands that to the C compiler (`sg build folder` for a program that is a folder; the modules it names are
compiled with it). The interpreter stays as the reference: `cargo test` compiles every program the
back end accepts and requires the same output as the interpreter, panic messages included, then runs the
same C once more under the address and undefined-behaviour sanitizers, and once more to see that it left no
memory behind (100 of 342 programs for now; the others are programs the checker rejects, on purpose).

What is compiled so far: `byte`, `int`, `long`, `nint`, `bool`; text (`string`, `read string`, `ref string`:
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
What is still missing is an error that names it (not silence): the few positions listed with the choices
below (81).

Who frees memory: the checker has already decided who owns each value, and the back end follows it. An owned
value (a `string`, a `T*`, a struct, an enum or a `T?` that holds one) is freed where its variable ends, on every way
out of the block (`return`, `break`, `continue`, and the `return` inside `try` and `?? return`). A use that moves the value empties the place it came from, so the later free
finds nothing to do; a text constant has no capacity and is never allocated or freed. There is no garbage
collector and no reference count.

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
| `vector`: a list of a million numbers filled with `add`, changed through `xs[i]`, added up with `for`, turned around and freed, six hundred times | 0.331 s | 0.682 s | 0.324 s | 1.02 |
| `counts`: a million numbers counted in a `HashMap` of fifty thousand keys, read back and a third removed, twenty times | 0.382 s | 0.376 s | 0.319 s | 1.20 |
| `show`: two million structs and half a million lists of numbers printed, one on each line | 0.214 s | 0.191 s | 0.374 s | 0.57 |
| `lookup`: three hundred million lookups in a list of records of 128 bytes, through a function that gives back a `read Record?` | 0.498 s | 0.488 s | 0.491 s | 1.01 |

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
The numbers move by a few percent from run to run, and ten small programs are not a benchmark suite.

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
    compiled test programs give the same output when built with gcc 16 instead of clang.

## How it is built

| File | Role |
|---|---|
| `src/lexer.rs` | Tokens, the line-end rule, `$"..."` pieces |
| `src/parser.rs` | Hand-written recursive descent parser |
| `src/types.rs` | Types as the checker sees them |
| `src/check.rs` | Static checks: names, types, `null` safety, use after move (per variable and per field), `match` covering every variant, `return` on every path, unused `Result`, what `try` may pass on, what `Copyable` and `Formattable` promise. Writes resolved details (literal types, field indexes, implicit conversions, what each call runs) into the tree |
| `src/interp.rs` | Runs the checked tree; only data-dependent failures (panics) are left to it |
| `src/emit_c.rs` | The back end: writes the checked tree as C, with the same panics |
| `src/diag.rs` | Error and panic messages |
| `src/source.rs` | The files of a program: which folders are modules and which of them are read. All files are kept as one text, so a position says which file it is in |
| `src/lib.rs`, `src/main.rs` | Library entry point and the command line |

It is a library with a thin command line on top, so that a language server can later reuse the parser and the
checks.

Tests are golden files: every `.sg` file in `examples/` and `tests/cases/` is run and its output (what it
printed, then the error or panic message) is compared with the `.out` file next to it. After an intended change,
regenerate them with `UPDATE_GOLDEN=1 cargo test` and review the diff.

Programs with modules are in `tests/modules/`: the folders there (`stock`, `billing`, `shop/parts`, `leaky`) are
modules, and each `.sg` file next to them is a program of one file that uses them. A folder with a `main.sg`
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
3. **`-b` on a `byte` is an error**, since the result could only be 0 (Rust). The hint is `-(int)b`.
4. **An index and the ends of a range take any integer that widens to `nint`**: a `byte`, an `int`, a `nint`.
   A `long` is converted with `(nint)`.
5. **A range that ends before it starts panics** (`text[3..1]`), it is not an empty text (Rust).
6. **In a `match` on a number the patterns are numbers and characters, `_` and `var name`.** Listing all 256
   values of a `byte` still needs `_`: the check does not count. The same number twice, or a number that does
   not fit the type, is an error.
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
`HashMap` filled with `getOrAdd`, and a `for` over both.

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
   only `equals` and `hash`). An enum cannot be `Comparable` until enums have methods.
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
    measurement). To remember: the block is one for the program; with threads it has to be one for each
    thread. Approved on 13 November, with the 64 KB limit added that day.
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
    `$"..."`, on the right of `&&` and `||`, and inside `assert`; a `match` that gives anything but a
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
    `cargo test`. The path on Linux has not been run yet.

## Not here yet

- **What the borrow check does not do yet.** A type with more than one source (`struct Match<from text, from
  pattern>`); storing a borrow from outside into a value reached through `ref` (the design's
  `where r from name`); a `List<read T>` or a `Result<read T, E>` (a list of a type that carries a borrow
  works); a `match` used as a value that gives a borrow is not followed.
- `swap(ref a, ref b)`, and `this` with a type (`read FolderRef this`).
- A string literal has the type `string` here; in the design it is `read string` that becomes `string` without
  allocating.
- Narrowing only follows a direct `x is null` / `x is not null` test, not `&&` chains. The same holds for
  the names of `x is Circle(r)`: they exist when the test is the whole condition of an `if` or `while`.
- Patterns inside patterns (`Negate(Number(v))`), patterns for text (`match word { "if" => ... }`),
  methods on enums.
- Interfaces of your own (`interface`, `where`) and the other built-in ones (`Clonable`, `Disposable`). `.toString()` on a `Formattable` struct or enum.
- Generics other than the built-in `Result<T, E>`, `List<T>`, `HashMap<K, V>` and `HashSet<T>` (generics of your own are deferred
  in the design); threads, `shared` / `weak` / `checked` / `locked`.
- Type aliases (`using`). Running the tests of one module only.
- A collection changed inside its own `for` through another `ref` to it is not caught (it is part of the
  borrow check); the direct forms are. Here a list is gone over by position and a map or a set as it was when
  the loop started.
- Lists of borrows (`List<read T>`); `reserve`, `clone`; a `Hashable`
  type with its own `equals` or `hash`.
- `char` and `text.chars()`, `double` and the other number types.
- Slices of anything but text (`read byte[]`, `xs[a..b]`, `text.asBytes()`, `string.fromUtf8`), `T[N]`.
  A slice of a `string` is a copy here, not a window; without a borrow check the difference cannot be seen.
- `tryToInt` and the other `tryTo...` conversions. Overflow and cast panics print the values but not yet the
  safe alternative.
- Freeing a long linked list recurses, the open stack-depth question from the design; here it runs on a 1 GB stack.

Words from the design that are not implemented produce "is not in the interpreter yet" instead of a confusing
error.

## Next

The back end, one piece of the language at a time, each compared with the interpreter, run under the
sanitizers and measured against C. Every example that runs in the interpreter and has no `test` block is
now compiled, and printing a value allocates nothing. Modules (`use`, `public`, `readonly`) are compiled
too, and with them the stock and invoice example. A `match` gives its value inside a larger expression, and
`try` and `?? return` stand after a value that owns memory and has no name; the few positions still refused are
listed in 81. After it the question is the
back end: the promise "as fast as C" can only be shown by compiled code, not by this interpreter.
