# Interpreter

A tree-walking interpreter for SG, written in Rust with no dependencies. Its job is to test the design by
running real programs, not to be fast. Source files end in `.sg`; the crate and the command are named `sg`.

```
cargo run -- examples/stack.sg
cargo test
```

## What runs today (12 November 2026)

| Area | Supported |
|---|---|
| Functions | Top-level functions, type-first signature, recursion, any order. `void main()` is the entry point. `public` is accepted and has no effect yet |
| Types | `int` (32 bit), `long`, `nint`, `bool`, `string`, `void` as return type, structs, enums, `T*`, `T?`, `read T` and `ref T` in signatures |
| Structs | `struct Node { int value` / `Node*? next }`, `Node { value = 1, next = null }`, field access and assignment through pointers and borrows |
| Methods | Written in the struct body with `this`, `read this` or `ref this` as the first parameter; fields are reached as `this.x`. Called as `stack.push(1)`, also through pointers and borrows. A function without `this` is called on the type: `Stack.create()`. `Stack(args)` is short for `Stack.create(args)` |
| Enums | `enum Shape { Circle(int radius)` / `Rect(int width, int height)` / `Empty }`, one variant per line. Built with the full name: `Shape.Circle(2)`, `Shape.Empty`, `new Expr.Number(1)` |
| `match` | `match x { Circle(r) => ... }` as an expression or a statement, on an enum, also behind pointers and borrows. Patterns: a variant with a name or `_` for each value, `_`, `var other`; a condition with `when`. An arm is an expression, a statement (`r += 1`, `return x`, `break`) or a block whose last line is the value |
| `is` | `if s is Circle(r) { ... }`, `while next() is not End { ... }` |
| Ownership | `new x` gives `T*`; `read x` and `ref x` lend; passing or assigning a non-copyable value moves it. `slot.take()` on a `T?` gives the value and leaves `null`; `slot.replace(value)` gives the old value and leaves the new one |
| Null | `null`, `x is null`, `x is not null`, `a ?? b`, `a ?? panic("...")`, narrowing of local variables and parameters |
| Variables | `var x = 1`, `int x = 1`, assignment, `+=` `-=` `*=` `/=` `%=`. A variable keeps its type |
| Control flow | `if` / `else if` / `else`, `while`, `for i in a..b`, `break`, `continue`, `return` |
| Operators | `+ - * / %`, `== != < <= > >=`, `&& \|\| !` (short circuit), unary `-`, cast `(int)x` |
| Text | `"..."` with `\n \t \r \0 \\ \"`, `$"{a}: {b}"` with `{{` and `}}`, `.toString()`, `text.length()` |
| Built-ins | `print(x)`, `panic("...")`, `assert(condition)` |
| Integer methods | `wrappingAdd`, `wrappingSubtract`, `wrappingMultiply` |

Design rules that are already enforced:

- Integer overflow panics, also in a release build. Wrapping is written on purpose with `wrappingAdd`.
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

## How it is built

| File | Role |
|---|---|
| `src/lexer.rs` | Tokens, the line-end rule, `$"..."` pieces |
| `src/parser.rs` | Hand-written recursive descent parser |
| `src/types.rs` | Types as the checker sees them |
| `src/check.rs` | Static checks: names, types, `null` safety, use after move (per variable and per field), `match` covering every variant, `return` on every path. Writes resolved details (literal types, field indexes, implicit conversions, what each call runs) into the tree |
| `src/interp.rs` | Runs the checked tree; only data-dependent failures (panics) are left to it |
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

The stack example differs from the design text in one line: `pop` tests `if node is null { return null }`
where the design writes `try this.top.take()`, because `try` is not here yet.

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

## Not here yet

- **Borrow checking.** Moves are checked, borrows are not: nothing stops a `read` borrow from outliving its
  owner or a `ref` from overlapping another borrow, and `from` is not parsed. `ref` values can be copied freely.
- `swap(ref a, ref b)`, and `this` with a type (`read FolderRef this`).
- A string literal has the type `string` here; in the design it is `read string` that becomes `string` without
  allocating.
- Narrowing only follows a direct `x is null` / `x is not null` test, not `&&` chains. The same holds for
  the names of `x is Circle(r)`: they exist when the test is the whole condition of an `if` or `while`.
- Patterns inside patterns (`Negate(Number(v))`), patterns for numbers and text (`match c { '+' => ... }`),
  methods and interfaces on enums. An enum cannot be copied, compared with `==` or printed yet.
- `Result` and `try`, generics, interfaces (`Copyable`,
  `Formattable`: a struct cannot be copied or printed yet), lambdas, collections, modules, `test` blocks, threads,
  `shared` / `weak` / `checked` / `locked`.
- `byte`, `char`, `double` and the other number types; character literals; indexing and slices.
- The `try...` methods (`tryAdd`, `tryToInt`). Until then overflow and cast panics print the values but not the
  safe alternative.
- Freeing a long linked list recurses, the open stack-depth question from the design; here it runs on a 1 GB stack.

Words from the design that are not implemented produce "is not in the interpreter yet" instead of a confusing
error.

## Next

`Result` and `try` (with `??` on a `Result` and `return` on its right side), which reach the calculator. `Result` will be a built-in type; user-written generics are not planned for now.
