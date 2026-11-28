# Rules already enforced

The design rules the checker, the interpreter and compiled code already hold a program to. Each choice
behind them is in [`design/decisions/`](../../design/decisions/README.md).

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
- Deep recursion ends in a readable panic when the stack of 1 GB is nearly full (about 380,000 calls in
  the interpreter, choice 1823), not a crash.
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
- Two windows or items of one list, array or window, one of them `ref`, go together only when they share no
  item (`var a = ref xs[..k]` beside `var b = ref xs[k..]`, `swap(ref xs[i], ref xs[j])`): ends that are
  numbers are told apart at compile time, the others when the second is taken, with a panic that names the
  items shared and the variable or the part of the call that holds the first (2011-2015).
- A variable declared with a borrow (`var all = ref names`) lends what it borrowed: `f(ref all, read names[0])`
  lends `names` twice in one call, and the error names the variable. An item of a list whose items carry a
  borrow, lent or written into, is a borrow of the list (195).
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
- A `readonly` field cannot be assigned, lent as `ref`, or changed through (`account.history.add(1)`); a
  `readonly` `shared locked` or `shared checked` field is only read in a `lock` block and not opened with
  `ref ... as` (2151).
- An index that is known and lies outside an array is an error, not a panic: `buf[16]` with `byte[16] buf`.
  An index that depends on data panics with the index and the length, as for a list; so does a range
  outside a list, an array or a slice.
- An index counted from the end (`xs[^n]`) is `^1` or more: `xs[^0]` and `xs[^-1]` are errors, as is `a[^4]`
  on an `int[3]`. `^` is written only first between `[ ]` (`var i = ^1` is an error), counts one operand
  (`xs[^k + 1]` is an error whose hint is `xs[^(k + 1)]`), and has no end to count from in a `HashMap` or a
  `raw T`. A count that depends on data panics naming `^n` and the length.
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
