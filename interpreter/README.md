# Interpreter

A tree-walking interpreter for SG, written in Rust with no dependencies. Its job is to test the design by
running real programs, not to be fast. Source files end in `.sg`; the crate and the command are named `sg`.

```
cargo run -- examples/stack.sg                  # runs the program
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

## What runs today (13 November 2026)

| Area | Supported |
|---|---|
| Functions | Top-level functions, type-first signature, recursion, any order. `void main()` is the entry point |
| Modules | A folder is a module: the files in it see each other with nothing written. `use stock` at the top of a file brings in the `public` names of the folder `stock`; `use shop.stock` for a folder inside a folder. The full name works without `use`: `billing.Line { ... }`, `stock.total(read xs)`, `stock.Color.Red`, `List<stock.Product>`. `public` on a struct, an enum, a function, a field or a method; without it the name stays inside its module. `readonly` on a field. The program is `sg main.sg` (the file is the root module, the folders next to it are the modules) or `sg folder` (every file of the folder is the root module). `sg build main.sg` and `sg build folder` compile the same program |
| Types | `sbyte`, `byte`, `short`, `ushort`, `int` (32 bit), `uint`, `long`, `ulong`, `nint`, `nuint`, `float`, `double`, `bool`, `char`, `string`, `void` as return type, structs, enums, `T*`, `T?`, `Result<T, E>`, `List<T>`, `HashMap<K, V>`, `HashSet<T>`, arrays `T[N]`, slices `read T[]` and `ref T[]`, `read T` and `ref T` in signatures |
| Types that carry a borrow | `struct Word<from source> { read string text` / `int start }`, `enum Token<from source> { Name(read string text) ... }`: a field can be a borrow when the type names its source. In a method `from source` says "from what this value borrowed, not from the value itself"; outside the type it is `from words.source` |
| Structs | `struct Node { int value` / `Node*? next }`, `Node { value = 1, next = null }`, field access and assignment through pointers and borrows |
| Methods | Written in the struct body with `this`, `read this` or `ref this` as the first parameter; fields are reached as `this.x`. Called as `stack.push(1)`, also through pointers and borrows. A function without `this` is called on the type: `Stack.create()`. `Stack(args)` is short for `Stack.create(args)` |
| Enums | `enum Shape { Circle(int radius)` / `Rect(int width, int height)` / `Empty }`, one variant per line. Built with the full name: `Shape.Circle(2)`, `Shape.Empty`, `new Expr.Number(1)` |
| `match` | `match x { Circle(r) => ... }` as an expression or a statement, on an enum, also behind pointers and borrows. Patterns: a variant with a name or `_` for each value, `_`, `var other`; a condition with `when`. On an integer the patterns are numbers and characters, on a `char` characters: `match c { '+' => ... _ => ... }`. An arm is an expression, a statement (`r += 1`, `return x`, `break`) or a block whose last line is the value |
| Interfaces | Five built-in ones, written after the name: `struct Point : Copyable`, `enum AppError : Formattable`. A `Copyable` value is copied where another would be moved. A `Formattable` value is accepted by `print`, `$"..."` and `f.write`; a struct can write its own `void format(read this, ref Formatter f)`, otherwise the text is made from the fields. `Equatable` gives `==` and `!=` (field by field, or the struct's own `bool equals(read this, read T other)`), `Hashable` lets the type be a key of a `HashMap` or an item of a `HashSet`, `Comparable` gives `<` `<=` `>` `>=` and `sort` through the struct's `int compare(read this, read T other)` |
| `List<T>` | Built in. `List<int>()`, `List<int> { 1, 2 }`; `xs.add(v)`, `xs.length()`, `xs[i]` to read and to write (`xs[i] = v`, `xs[i] += 1`, `xs[i].name = ...`, `ref xs[i]`), `xs.get(i)` gives `null` where `xs[i]` would panic, `xs.removeLast()`, `xs.reverse()`, `xs.clear()`. `xs.indexOf(v)` and `xs.contains(v)` on `Equatable` items, `xs.sort()` on `Comparable` ones (numbers and text are both). `string.join(", ", read names)` |
| `HashMap<K, V>` | Built in; a key is an integer, a `bool`, a `string` or a type written `: Equatable, Hashable`. `HashMap<string, int>()`, `HashMap<string, int> { {"x", 10} }`; `map.add(key, value)`, `map[key]` to read and to write what is under a key that is there, `map.get(key)` gives `null` for a missing key, `map.getOrAdd(key, initial)` gives a `ref V`, `map.contains(key)`, `map.remove(key)`, `map.length()`, `map.clear()` |
| Text of a collection | `print(xs)`, `$"{xs}"`: `[1, 2, 3]`, `{"ada": 36}`, `{1, 2, 3}`, when the items can be written as text |
| `HashSet<T>` | Built in; an item is what a key of a `HashMap` can be. `HashSet<int>()`, `HashSet<int> { 1, 2 }`; `set.add(v)` gives `true` if the item was not there, `set.contains(v)`, `set.remove(v)` gives `true` if it was there, `set.length()`, `set.clear()` |
| Arrays | `int[8] values`: a fixed number of items, kept where the value is (in the variable, inside the struct), with the length in the type. Built as `int[3] { 1, 2, 3 }` (the items), `byte[16](255)` (one value in every item) or `byte[16]` (zeros: 0, `false`, `null`). `xs[i]` to read and to write, `xs.length()`, `xs.get(i)`, `xs.indexOf(v)`, `xs.contains(v)`, `==`, `print`. An array of items that can be copied is copied like a number; any other is moved |
| Slices | `read T[]` looks at items that something else owns, `ref T[]` can also write them. `read xs` and `ref xs` give one where a slice is expected, for a `List<T>` and a `T[N]`; a range gives one too: `xs[a..b]`, `xs[..b]`, `xs[a..]`, `xs[..]`, and `ref xs[a..b]` for a range that is written through. `s[i]`, `s.length()`, `s.get(i)`, `s.indexOf(v)`, `s.contains(v)`, `==`, `print`, `for`. `text.asBytes()` looks at a text as a `read byte[]`; `string.fromUtf8(bytes)` looks at bytes as text and gives a `Result<read string, TextError>` |
| `for` over a collection | `for x in xs` hands the collection over and `x` owns each item; `for x in read xs` and `for x in ref xs` lend, and so does a collection that is already borrowed. Over a `HashMap` the variable has a `key` and a `value`; a `HashSet` is gone over by hand-over or `read`. An array is gone over like a list; a slice lends its items |
| `is` | `if s is Circle(r) { ... }`, `while next() is not End { ... }` |
| Errors | `Result<T, E>` is built in: `return Ok(value)`, `return Ok()` for `Result<void, E>`, `return Error(cause)`. `try x` gives the value or returns the error to the caller, and on a `T?` returns `null`; on a `read T?` it gives a `read T`. `x ?? fallback` drops the error; the right side can be `return ...` or `panic(...)`. `match` and `is` see the variants `Ok` and `Error`. `_ = f()` drops a value on purpose. `Result<void, E> main()` prints the error it ends with and exits with code 1 |
| Ownership | `new x` gives `T*`; `read x` and `ref x` lend; passing or assigning a non-copyable value moves it. `slot.take()` on a `T?` gives the value and leaves `null`; `slot.replace(value)` gives the old value and leaves the new one |
| Null | `null`, `x is null`, `x is not null`, `a ?? b`, `a ?? panic("...")`, narrowing of local variables and parameters |
| Variables | `var x = 1`, `int x = 1`, assignment, `+=` `-=` `*=` `/=` `%=`, `&=` `\|=` `^=` `<<=` `>>=`. A variable keeps its type |
| Control flow | `if` / `else if` / `else`, `while`, `for i in a..b`, `for x in xs`, `break`, `continue`, `return` |
| Operators | `+ - * / %`, `== != < <= > >=`, `&& \|\| !` (short circuit), unary `-`, cast `(int)x`. On integers `& \| ^ ~ << >>`; they bind tighter than a comparison, so `flags & mask == 0` tests the masked value |
| Numbers | `42`, `0xde`, `0b1010`, and `_` between digits: `1_000_000`, `0xdead_beef` |
| Text | `"..."` with `\n \t \r \0 \\ \" \'` and `\u{1F600}`, `$"{a}: {b}"` with `{{` and `}}`, `.toString()`, `text.length()`, `text.clone()`. `text[i]` reads one byte. `text[a..b]`, `text[..b]`, `text[a..]` and `text[..]` give a `read string`. `trim()`, `trimStart()`, `trimEnd()` give a `read string`; `indexOf(x)` and `lastIndexOf(x)` a `nint?` in bytes; `contains(x)`, `startsWith(x)`, `endsWith(x)`, where `x` is a `char` or a `read string` (or a `byte` for the two positions); `equalsIgnoreAsciiCase(other)` |
| Building text | A `string` grows at its end: `line.write(x)` adds whatever `print` takes (`$"{name}: {count}\n"`, a number, a `char`, a text, a `Formattable` value), `line.clear()` empties it and keeps its memory, `string.withCapacity(n)` is an empty text with room for `n` bytes |
| Going over text | `for c in text.chars()` gives each `char`; `text.split(',')` (a `char` or a `read string`), `text.splitWhitespace()` and `text.lines()` give each part as a `read string`, nothing copied. Kept in a `var`, the iterator gives the next part with `parts.next()`, `null` at the end |
| Characters | `char`: one Unicode character. `'+'`, `'é'`, `'\n'`, `'\u{e9}'` is a `char`, and a `byte` where an integer goes (`byte b = '+'`, `text[i] == ':'`). `(int)c` is its code, `(char)n` the character of a code, `n.tryToChar()` gives `null` where `(char)n` would panic. `==`, `<`, `match`, keys, `sort`, `print`. `isAsciiDigit()`, `isAsciiLetter()`, `isAsciiAlphanumeric()`, `isAsciiUpper()`, `isAsciiLower()`, `isAsciiWhitespace()`, `isAscii()`, `toAsciiUpper()`, `toAsciiLower()` on `char` and on `byte`; `isWhitespace()` and `utf8Length()` on `char` |
| Function values | `Func<int, bool>`: the address of a named function, the last type is what it returns (`Func<void>` takes and returns nothing). The name of a function is the value (`apply(double, 5)`, `Func<int, int> f = double`); it is copied freely, kept in variables, fields, variants and lists, and called like a function: `f(4)`, `this.onClick(read this.label)` |
| Built-ins | `print(x)`, `panic("...")`, `assert(condition)` |
| Tests | `test "name" { ... }` at the top level of a file, run by `sg test file.sg`: one line per test (`ok` or `FAIL` with the reason), exit code 1 if any failed. In a test `try` passes on an error of any type, and the test fails with it. A normal run leaves the tests out |
| Comparing by name | `a.compare(b)` on numbers, `char` and text (negative, 0, positive), `a.equals(b)` on those and `bool`: what `<` and `==` are short for |
| Integer methods | `wrappingAdd`, `wrappingSubtract`, `wrappingMultiply`; `tryAdd`, `trySubtract`, `tryMultiply`, `tryDivide` give `null` where the operator would panic. `tryToInt()` and the others give `null` where a cast would panic, `wrappingToInt()` and the others keep the bits that fit; `min`, `max` |
| Fractional methods | `abs`, `sqrt`, `floor`, `ceil`, `round`, `truncate`, `isNaN`, `isInfinite`; `exp`, `log`, `log2`, `log10`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`, `pow`, `fusedMultiplyAdd`, `min`, `max`; `tryToInt()` and the others |
| Numbers from text | `int.parse(text)`, and the same on every number type, gives a `Result<int, NumberError>`; the built-in `enum NumberError { Invalid(nint position)` / `OutOfRange }` says where the text stops being a number, or that the type does not hold it |
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
- `write` and `clear` change the text, so they need it where it may change: not through `read` (the hint is
  `ref string`) and not in a `readonly` field. Writing a text into itself, or anything that looks at it
  (`s.write(s[..3])`, `s.write(s.trim())`), is an error whose hint is `.clone()`; a number computed from
  it is fine (`s.write($"{s.length()}")`). `text += "x"` is an error whose hint is `write`.
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

## The C back end

Since the sixteenth slice (12 November 2026) a program can be compiled: `sg build file.sg` writes it as C and
hands that to the C compiler (`sg build folder` for a program that is a folder; the modules it names are
compiled with it). The interpreter stays as the reference: `cargo test` compiles every program the
back end accepts and requires the same output as the interpreter, panic messages included, then runs the
same C once more under the address and undefined-behaviour sanitizers, and once more to see that it left no
memory behind (154 of 535 programs for now; the others are programs the checker rejects, on purpose).

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
`string.withCapacity`, the same way.
What is still missing is an error that names it (not silence): the few positions listed with the choices
below (81, 111).

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
| `buffer`: 32 bytes on the stack written as hex into a buffer of 64 through a `ref byte[]`, added up through a `read byte[]` and a range of it, fifteen million times | 0.280 s | 0.733 s | 0.283 s | 0.99 |
| `bits`: a sieve below sixty-four million with one bit for each number, then thirty million numbers written seven bits at a time into a buffer on the stack and read back | 0.322 s | 0.321 s | 0.407 s | 0.79 |
| `hash`: XXH64, the 64-bit xxHash, written with `ulong`, over one megabyte, two thousand times | 0.134 s | 0.488 s | 0.133 s | 1.01 |
| `nbody`: the sun and the four giant planets moved twenty million steps in `double`, a square root for each pair at each step | 0.661 s | 0.662 s | 0.679 s | 0.97 |
| `numbers`: a million lines of whole and fractional numbers read ten times with `long.parse` and `double.parse` and added up | 0.186 s | 0.191 s | 0.196 s | 0.95 |
| `fields`: a million lines like ` Ada17 , 120, -45 ,6789` gone over twenty times with `lines`, `split`, `trim`, `long.parse` and `chars` | 1.228 s | 1.259 s | 1.202 s | 1.02 |

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
`buffer` is the first program with arrays and slices: an id of 32 bytes on the stack is written as hex
into a buffer of 64 bytes through a `ref byte[]`, and the hex is added up through a `read byte[]` and
through a range of it, fifteen million times. The C it is compared with passes a pointer and a count by
hand and checks nothing. The two take the same time (0.99, the same over three runs): the arrays are
C arrays and a slice is the pointer and the count the C program passes by hand. Every index and the
range are checked in the C that is generated; here the C compiler, once it has put the small functions
into `main`, sees both lengths and removes every one of those checks, so the built program holds none.
That is this program, not a promise: where a length is not known, the check stays, one comparison that
is never taken. The overflow-checked build takes 2.6 times as long: the checks on `i * 2`, on the sum
and on the cast to `byte` keep the C compiler from handling several bytes at a time.
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
eight bytes at a time with `memcpy`, as xxHash does. SG puts the number together from the bytes, after
cutting the eight out of the data (`data[at..at + 8]`): one check, and the C compiler joins the eight
reads into one. Written without the cut, with each of `data[at]` to `data[at + 7]` checked by itself, the
same program took 3.5 times the time of C (choice 136). The overflow-checked build takes 3.6 times: the
additions `at + 8`, `at + 16` and `at + 24` that give the place of each read are checked, and those tests
again stand between the reads. With them written as wrapping additions it takes the time of C; the tests of
the casts `(ulong)b[0]` cost nothing (the C compiler sees that a `byte` always fits).
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
   (Since the thirty-sixth slice a character is a `char` where no integer is expected: `print('+')` writes
   `+`; where an integer goes it is still a `byte`, 167.)
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
    `cargo test`. The path on Linux has not been run yet.

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
    of items (`data.indexOf(other)`) are not here yet.
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
     `binary.LittleEndian.Uint64`) is a candidate, not part of this slice.
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

## Slower than C, and could be better

Places where a measurement found SG slower than the same program in C and a fix is in sight. They are kept
here to be talked over when their time comes; none of them breaks a program, and none is fixed by turning a
check off.

- **Bytes read one index at a time** (`hash`, choice 136): `data[at] | data[at + 1] << 8 | ...` checks
  each index, the C compiler cannot join the reads, and the program takes 3.5 times the time of C. Cutting
  the window first (`data[at..at + 8]`) already takes the time of C. Candidates: a built-in that reads a
  number out of bytes (`ulong.fromLittleEndian(...)`), or a hint from the checker where it sees the pattern.
- **Overflow checks on the arithmetic of an index in a checked build** (`hash`, 3.6 times): `at + 8` is
  tested for overflow before it is used as a place, and those tests stand between reads that could be
  joined; with them written as wrapping additions the checked build takes the time of C. A release build
  has none. The checked builds of `vector` (2 times) and `buffer` (2.6 times) may have the same cause;
  that is not looked at yet. Candidates: leave out the
  overflow test of an addition whose result goes straight into an index or a range that is checked
  anyway (an overflow there could only give a place the bounds check refuses), or let the checker prove
  the range of a loop counter.
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

## Not here yet

- **What the borrow check does not do yet.** A type with more than one source (`struct Match<from text, from
  pattern>`); storing a borrow from outside into a value reached through `ref` (the design's
  `where r from name`); a `List<read T>`, and a borrow as the error of a `Result` (a list of a type that
  carries a borrow works); a `match` used as a value that gives a borrow is not followed.
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
- What text needs Unicode tables for: changing the case of a text (`toUpper`), letters and digits beyond
  ASCII (`isLetter`), comparing without case beyond ASCII. `replace`, `repeat`, `padLeft`; the positions
  of the characters (`charIndices`); splitting at most n times; `match` on text. Writing the type of an
  iterator, and a `for` over a type of your own that has `next` (with generics). `text.capacity()` and
  `text.reserve(n)`; a `ref string` given to a `format` method in place of its `Formatter`.
- A number is written in decimal only (`print`, `$"..."`) and read in decimal only
  (`parse`); there is no hex output or input. No way to read a number out of bytes in one step
  (`ulong.fromLittleEndian(bytes)`): it is put together from the bytes by hand (see 136). `abs` and
  `clamp` on integers, `sinh` and the other functions of `<math.h>` that are not listed in 162.
- Arrays of arrays, `read T[]?`, an array as a key, `sort` and
  `reverse` through a `ref T[]`, `list.addAll(slice)`, looking for a run of items in a slice.
- A panic of `-x` and of a shift names no way out: there is no `tryNegate` or `tryShiftLeft` yet.
- Freeing a long linked list recurses, the open stack-depth question from the design; here it runs on a 1 GB stack.

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

Candidates for what comes next: `list.addAll(slice)` and `sort` through a `ref T[]`, type aliases
(`using`), threads, the places that are slower than C.
