# Interpreter

A tree-walking interpreter for SG, written in Rust with no dependencies. Its job is to test the design by
running real programs, not to be fast. Source files end in `.sg`; the crate and the command are named `sg`.

```
cargo run -- examples/stack.sg                  # runs the program
cargo run -- test examples/tests_stock.sg       # runs its tests
cargo run -- build examples/primes.sg -o primes # compiles it through C (needs `cc`)
cargo run -- emit-c examples/primes.sg          # prints the C
cargo test
cargo run -- build examples/primes.sg --release # the same as a release build: optimised, overflow wraps
sh bench/run.sh                                 # compiled SG against C written by hand
```

## What runs today (12 November 2026)

| Area | Supported |
|---|---|
| Functions | Top-level functions, type-first signature, recursion, any order. `void main()` is the entry point. `public` is accepted and has no effect yet |
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

## The C back end

Since the sixteenth slice (12 November 2026) a program can be compiled: `sg build file.sg` writes it as C and
hands that to the C compiler. The interpreter stays as the reference: `cargo test` compiles every program the
back end accepts and requires the same output as the interpreter, panic messages included, and then runs the
same C once more under the address and undefined-behaviour sanitizers (56 of 304 programs for now).

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
Anything else (`HashMap`, `HashSet`, types that carry a borrow, function values, writing an enum, a
struct or a list with `print`, `==` on them) is an error that names what is missing: nothing is compiled
wrongly in silence.

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
The numbers move by a few percent from run to run, and eight small programs are not a benchmark suite.

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
    the back end for now, not a rule of the language; the interpreter runs both forms.
15. **`??`, `try` and `replace` are written as GNU statement expressions**, so they run where they stand and
    the right side of `??` runs only when it is needed. clang and gcc both have them; a C compiler without
    them (MSVC) cannot build the output for now.
16. **A `read int` lent where a `read int?` is expected is not compiled yet** (a pointer to a number is not a
    pointer to a number and a flag; it needs a temporary). The same with a `T*?` works, since both are one
    pointer.
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
    for the same reason; the interpreter runs both forms.
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
    owns something (`if make() is Say(text)`) the back end stops and asks for a name. A limit for now.

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
| `src/lib.rs`, `src/main.rs` | Library entry point and the command line |

It is a library with a thin command line on top, so that a language server can later reuse the parser and the
checks.

Tests are golden files: every `.sg` file in `examples/` and `tests/cases/` is run and its output (what it
printed, then the error or panic message) is compared with the `.out` file next to it. After an intended change,
regenerate them with `UPDATE_GOLDEN=1 cargo test` and review the diff.

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
    `assert`. A limit of the back end, not a rule of the language.
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
    cannot stand on the right side of `&&`, `||` and `??` yet (the back end asks for a name).
37. **`indexOf`, `contains` and `sort` are compiled for integers, `bool` and text.** For structs and enums
    they wait for `==` and `compare` in the back end. `sort` is the C library's `qsort`; it does not keep
    equal items in their order, which cannot be seen for these types.
38. **`print(xs)` and `xs == ys` are not compiled yet**; they come with `print` and `==` on structs.

One thing in the checker was found on the way: the node it inserts when a borrowed `T?` is opened with
`??` or `try` (`xs.get(1) ?? -1`) did not carry its type, which only the back end reads. It does now.

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
  in the design); modules, threads, `shared` / `weak` / `checked` / `locked`.
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
sanitizers and measured against C: next `HashMap` and `HashSet`, then types that carry a
borrow (the calculator), function values, and `print` and `==` on structs, enums and collections. After it the question is the
back end: the promise "as fast as C" can only be shown by compiled code, not by this interpreter.
