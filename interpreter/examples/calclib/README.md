# calclib: a library in SG, called from C

A calculator written in SG and built into a library that a program of C links. It is the example to start
from when SG is to give C a library: what goes over the boundary, how, and what each side promises.

```
calclib/
  api.sg          the boundary: every `export` function, the only file with `unsafe`
  calc/           the calculator, which knows nothing of C
    lexer.sg      tokens that borrow the text they were read from (`from source`)
    parser.sg     a tree of `Expr*`, and the grammar
    scope.sg      variables and functions (`Func<double, double>`), and the evaluation
    errors.sg     what can go wrong, as one `Formattable` enum, and the number C sees for it
    tests.sg      `sg test examples/calclib` runs these, in the interpreter or compiled
  main.c          the program of C that uses the library
  main.out        what it prints
```

## Building it

From the `interpreter` folder:

```
sg build examples/calclib --library -o build/libcalclib.a      # also writes build/calclib.h
cc -std=c11 -Ibuild -o calc examples/calclib/main.c build/libcalclib.a -lm -pthread
./calc
```

or as a shared library, which a program finds when it starts:

```
sg build examples/calclib --shared -o build/libcalclib.so      # libcalclib.dylib on macOS
cc -std=c11 -Ibuild -o calc examples/calclib/main.c -Lbuild -lcalclib -Wl,-rpath,build
```

`sg build` says what a program links. `--release` builds it optimised, with integer overflow wrapping
instead of being checked.

## What crosses the boundary

| In SG | In `calclib.h` | What it means |
|---|---|---|
| `export Calculator* calc_new()` | `Calculator *calc_new(void);` | C is given a value of SG to keep. It sees `typedef struct Calculator Calculator;`, never the fields |
| `ref Calculator calc` | `Calculator *calc` | C lends it for the call; SG may change it |
| `read Expr expr` | `const Expr *expr` | C lends it for the call; SG only reads it |
| `export void calc_free(Calculator*? calc)` | `void calc_free(Calculator *calc);` | C gives it back and it ends there, its `dispose` with it; `NULL` is nothing to end, as with `free` |
| `export Expr*? calc_compile(...)` | `Expr *calc_compile(...);` | `NULL` when there is nothing to give |
| `read CString text` | `const char *text` | a text of C, copied and checked to be UTF-8 by `string.fromCString(text)` |
| `raw double out` | `double *out` | memory of C, written inside `unsafe` |
| `raw byte buffer, nuint capacity` | `uint8_t *buffer, uint64_t capacity` | a buffer of C, written through `Memory.viewRef` |
| `CalcStats` (plain data) | `typedef struct CalcStats { ... } CalcStats;` | given by value, laid out as C lays it out |
| `CFunc<raw void, raw byte, double, void> visit, raw void data` | `void (*visit)(void *, uint8_t *, double), void *data` | a function of C called back with its own `data` |

No `unsafe` is written for a value of SG that C gives back: that it is one SG gave is C's promise, as a
library of C trusts the pointers it is given. The interpreter checks the promise: an address it did not give,
one of another type, one freed already, or one lent against the rules of borrowing (freed while a call it is
lent to still runs) is a panic. Compiled code checks only `NULL`, where a parameter takes none.

## What the library does with it

- `calc_eval` reads a statement (`2 * x`, `y = x + 1`) and works it out; the value goes to `*out`, and the
  function gives 0 or the number of the error, whose message `calc_error` copies into a buffer of C, cut to
  its size and ended with a zero byte, as `snprintf` does.
- `calc_compile` reads a statement once, and `calc_run` runs it again and again.
- `calc_sum` sums a statement over a variable, worked out by four tasks of the library, each with a copy of
  the variables; the parts are added in their order, so the sum is the same on every run.
- `calc_each_variable` calls a function of C for each variable, in the order of the names, each name in
  memory of its own that lives for the call.
- `calc_on_close` gives a function of C that `dispose` calls when the calculator ends.

## What the program of C must know

- Every function may be called from any thread, and from several at once, each on its own calculator. A
  calculator is not to be used by two threads at once, as with any library of C.
- A panic of SG (a bug of the library, or `NULL` where none is taken) writes its message and aborts the
  program. Nothing unwinds through C.
- A shared library is never unloaded: `dlclose` leaves it where it is, since its tasks' threads may run on.
- The numbers the functions give are in `main.c` (`CALC_OK` and the others); the header has no constants yet.
