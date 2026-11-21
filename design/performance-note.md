# Performance note

Notation does not affect the machine code: `read`, `ref`, `from` and type names disappear at compile time. Decisions about meaning do.
None of this has been measured, and what follows are expectations.

| Effect | Decision | Compared to Rust |
|---|---|---|
| Same | `T*`, `read`, `ref`, `T?`, `enum`, `match`, `Result`, `try`, monomorphization, bounds checks, `locked` | The same rules, the same optimisation information |
| Better | Panic does not unwind | Like Rust's `panic=abort` setting: no cleanup path is generated, and the binary gets smaller |
| Same | Overflow is checked in debug and wraps in release | Changed on 12 November from "checked in release too". One difference in the C that is generated: a wrapping operation is written with unsigned arithmetic, so the C compiler cannot assume "this never overflows" as it does for C's own `int` |
| Worse | `shared` is always atomic | More expensive than `Rc`, only on `share` and when an owner lets go |

This goal holds only with a back end at the level of LLVM. A tree-walking interpreter is for testing the language and gives no performance.

*First measurement, 12 November 2026.* The first slice of a back end writes C and lets the C compiler optimise
(integers, functions, structs, `read`/`ref`). Against the same programs written by hand in C, with the same `-O2`, and
with overflow checked in release as the design then said: trial division 0.97 times the time of C, recursive Fibonacci
1.38, a Collatz loop 1.59. The check cost nothing where arithmetic is rare and up to about 60% in a loop that is nothing
but arithmetic. On this measurement the overflow rule was changed to Rust's (checked in debug, wrapping in release); after the
change a release build runs the three programs in 0.96, 0.98 and 1.00 times the time of C
([SG against C](../benchmarks/against-c.md)). The first program with memory (text read byte by byte, compared, copied to the heap and freed) runs in 1.05 times the
time of C: that is the cost of the checks that stay, on every index and every range. The first program that follows
pointers (a linked list of a million nodes built with `new`, walked and freed) runs in 1.01 times the time of C: a
`Node*?` is one pointer and `is not null` one comparison. The first program with an enum (an expression tree
built, evaluated with `match`, rewritten and freed) runs in 1.02 times the time of C written as a tagged union and a
`switch`. The first program with errors as values (numbers read out of text, an error found three calls deep and
passed up with `try`) runs in the time of C that returns error codes (0.99 to 1.02): a `Result` is a tag and a value,
and `try` is one test. The first program with a list (a million numbers added, changed through their index, added
up and freed) runs in 1.02 times the time of C that keeps a growing array by hand. One thing it showed about the
overflow rule: in a loop that is nothing but arithmetic on its counter, wrapping can cost what C's undefined signed
overflow saves (1.28 on such a loop, until the arithmetic was taken out of the measurement). Rust pays the same.
The first program with a hash table (a million numbers counted under fifty thousand keys) runs in 1.20 times the time
of C; all of it is the keyed hash (SipHash-1-3, as in Rust), and with the plain hash of the C program the two take the
same time. Printing two million structs first took 1.22 times the time of C's `printf`, because each printed value was
allocated; written into one block that the program keeps, it takes 0.57 times (a `printf` reads its format on every call).
