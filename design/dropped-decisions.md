# Dropped decisions

These no longer hold and are here for the record. Apart from the data race and recoverable error items, they were dropped on 5 November
together with the generation model and the arena:

- Run-time generation checks for use-after-free (generational references).
- An 8-byte pointer whose top 16 bits are the generation. The decision on generation width (16 bits, mask, no TBI) had been made
  the same day, and it lost its subject when the model was dropped.
- Retiring a slot that overflows.
- A slab allocator and a standard arena API as part of the language.
- `ref` being a second-class parameter mode that is only passed downwards.
- Data races being out of scope in the first version. With threads present, a non-atomic counter leads to a double free,
  so the subject touched memory safety; the thread rule is in [the decisions by topic](rules/README.md).
- Settling for escape analysis and avoiding lifetime notation altogether.
- Recoverable errors being left out because they need unwinding. The reasoning was wrong: an error returned as a value
  needs no unwinding (6 November).

Removed on 11 November to make the language smaller. All of them can be added back later, and it is hard to remove something once it has been added:

| Removed | Instead | Cost |
|---|---|---|
| The heavy version of `Closable`: tracking the value everywhere, spreading to structs, the ban on generics and `shared`, `IgnoreClose`, the deferred `try using var` | The version that looks only at local variables, ["Close error"](rules/input-output.md#close-error) | If a value moved into a field or a collection is dropped without being closed, the compiler does not see it |
| Automatic `Ok` | `return Ok(value)`, `return Ok()` | A few characters on every success return |
| The "assign back on every exit" check when moving a field through a `ref` | Always `take` or `replace` | Negligible, Rust is the same |
| A block that produces a value on its own, `var x = { ... }` | The variable is declared and assigned inside `if/else`. The block in a `match` arm stays | A few lines |
| `ThreadFunc`, `shared Func` | Thread work starts with `spawn f(args)` *(14 November: with `Task.run(f, args)`)* | A work queue that takes lambdas (a thread pool) cannot be written in user code |
| `string.fromBytes` | `string.fromUtf8(bytes)`, `.clone()` for a copy | None |

Removed on 12 November. The measure, in the author's words: performance and simplicity are not given up. Lambdas lost on both:

| Removed | Instead | Cost |
|---|---|---|
| Lambda notation (`o => ...`) and capture: borrowing without `new`, moving with `new`, the hidden `checked` on what a lambda writes to, `Func<...>*` as an owner, the rules for writing after a copy and for moving out of a lambda | A function value is the address of a named function (["Function value"](rules/types.md#function-value)). Data a rule needs is passed as a parameter or carried in an `enum` variant | A few more lines where a one-line lambda stood; in exchange no run-time check, no heap, and about ten rules fewer |
| The list methods that take a lambda: `filter`, `map`, `any`, `forEach`, `removeAll`, `sortBy` | A `for` loop; `sort` with `Comparable` | The loop is written out. The machine code is the same or better |
