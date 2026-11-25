# lox

`lox` is an interpreter of Lox, the language of Robert Nystrom's *Crafting Interpreters*, written in SG
after clox, the bytecode interpreter of the book's second half (choices 1883-1894). It is measured against
clox itself, the C program it mirrors, and nothing else: from slice 180 on a trial is written in SG alone
(the rule of `trials/README.md`). clox is MIT; it and the book's tests are fetched at a commit of their
own and built in `trials/target`, never kept in the repository (`tests/clox.py`).

```
lox [--stress-gc] [--ip-local] [PATH]
```

With a path it runs the script, and ends with 65 for an error of compiling, 70 for an error at run time and
74 for a file it cannot read; more than one path is a wrong use, 64. Without one it is the REPL: a prompt
`> `, a line read and run, the globals kept from line to line, until the input ends. What it writes, on the
output and on the standard error, is what clox writes, byte for byte, and so are the codes it ends with.
`--stress-gc` runs the collector at every allocation, as clox built with `DEBUG_STRESS_GC`; `--ip-local`
runs the loop that keeps its place in the code in a local variable (below).

## What is the same as clox

Everything a program can see, and the algorithm under it (choice 1883):

- The scanner, token by token as the compiler asks; the source ends at its first NUL byte, as clox's text
  does, and is bytes, not checked to be UTF-8: a text with any bytes is printed as it is.
- The compiler, in one pass: Pratt's parsing of expressions by precedence, locals resolved at compile time,
  upvalues found through the functions around, `this` and `super` as hidden locals, and its errors with
  clox's words, `[line N] Error at 'x': ...` with the panic mode and its synchronization. The limits are
  clox's: 256 constants, 256 locals and 256 upvalues a function, 255 arguments and parameters, a jump of
  65,535 bytes.
- The machine: a stack of 16,384 values and 64 frames ("Stack overflow." at the 65th, with the trace of
  the 64), the 37 instructions of clox, closures with open upvalues closed when their variable's frame ends,
  classes whose methods are copied down into the subclass (`OP_INHERIT`), bound methods, initializers,
  and the errors at run time with clox's words and its trace, innermost first.
- Values in 64 bits with NaN boxing, as clox builds by default (choice 1885): a number is its own bits,
  `nil`, `false` and `true` a quiet NaN with a tag, an object a quiet NaN with the sign bit and the object's
  handle in its low bits, where clox keeps the address. `double.toBits` and `double.fromBits` (choice
  1891) move the bits; nothing needs `unsafe`.
- Numbers printed as C's `%g` prints them (`$"{d:g}"`, choice 1892); a NaN as `nan`, or `-nan` on Linux
  where glibc writes the sign; the infinities as `inf` and `-inf`.
- A mark-and-sweep collector: the roots marked (the stack, the frames, the open upvalues, the globals,
  `init`), the gray objects blackened, the strings no longer reached taken out of the table of strings,
  the rest swept; it runs when the bytes taken pass the mark, twice what was left after the last one,
  first at 1 MB, the bytes counted as clox counts them on a machine of 64 bits.

## What is SG's own

- **The heap** (choice 1884). Lox's objects refer to each other in cycles (a class holds its methods, a
  closure itself), which owners and borrows do not hold. They are the items of one `List<Obj>`, an enum
  with a variant for each kind of object, and a value holds an item's place, its handle, where clox holds
  its address. A freed object leaves a free place that the next object takes; the handles in use are a
  list of their own, which the sweep goes over as clox goes over its list of objects, so that a sweep
  costs the objects there are and not every place the heap ever had (with every place, `instantiation`
  took 7.07 s, clox 0.39).
- **The program apart from the heap.** The code of every function lies in one list, with its lines and
  its constants beside it (`Program`); a function's code is put there when the compiler ends it, and a
  jump, which is relative, needs no change. The machine reads the program while the heap changes, so the
  program is lent to it to read and the machine to change. The functions are made only by the compiler and
  live as long as the program; their constants are roots, where clox reaches them through the closures. The
  collector does not run while the compiler does, whose lists it does not see; clox's may, marking the
  functions being compiled.
- **The tables** (choice 1886) are the language's `HashMap`: the strings interned by their bytes,
  `HashMap<List<byte>, nint>`, looked up through a window onto the bytes without copying them; the
  globals, the fields of an instance and the methods of a class by the handle of the interned name,
  `HashMap<nint, ulong>`, which clox finds by the hash it keeps in the string. A string's bytes are held
  twice, in its object and as the key of the table of strings.
- **`clock()`** is the seconds since the program started; clox's is the processor's time (`clock()` of C).
  Only the benchmarks print it.
- **Deeper nesting than clox.** clox's compiler takes nested expressions on the C stack and ends on a
  signal (`SIGSEGV`) at about 200,000 levels; SG's runs on a thread with 1 GB of stack, as every SG program's
  main does, prints the answer at a million levels, and at five million ends with its panic, `stack overflow`, never a signal.

## The place in the code, in a local (choice 1894)

clox's loop reads each byte through `frame->ip`, a pointer it keeps in a register. SG has no pointer local
in safe code: the frame is an index into the list of frames, and each byte read loads the list, then the
frame, then its `ip`. Measured on the simplest instructions, a constant and a pop, SG took 2.2 ns where clox
took 0.9. The first challenge of the book's chapter "Calls and Functions" keeps `ip` in a local variable,
written back to the frame before a call, a return or an error. `--ip-local` is that loop in SG
(`sg/local.sg`), and `clox-ip-local` the same change made to clox's `vm.c` by `tests/clox.py`; both are
measured beside the book's, so that the change is the same in both, as the rule of a design idea has it.

## What the trial found

Three gaps of the language, each made a numbered choice and built in this slice, the rule being that a
trial does not work around what SG lacks (choice 1890):

- **1891**: the bits of a `double` (`toBits`, `fromBits`), which NaN boxing needs.
- **1892**: `%e` and `%g` in `$"..."` (`{d:e3}`, `{d:g}`), which clox prints with.
- **1893**: a hash of a key of a fixed size as hashbrown's foldhash, a folded multiplication by the secret key,
  in place of SipHash-1-3: the fields and methods looked up took 40% of their time in SipHash
  (`properties` 0.75 s, then 0.38; clox 0.22).

What is left is in [Slower than C](../../benchmarks/slower-than-c.md) and the tables in
[Programs](../../benchmarks/programs.md#lox).

## Tests

`tests/check.py` runs the book's tests that clox runs (all of `test/` but `scanning/`, `expressions/` and
`benchmark/`, 246 programs), and the programs of `tests/programs/` (deep recursion, a stack overflow in a
method, numbers at the edges of a double, texts of a million characters, a million objects, closures made in
loops, a NUL byte, bytes that are not UTF-8, an unterminated string, 2,000 levels of parentheses and blocks,
255 arguments), by clox and by each build: each must do what the comments of a test ask, as the book's runner
checks it, and write what clox writes, byte for byte, on the output and the standard error, with its exit
code. Each build runs as it is, with `--stress-gc`, with `--ip-local` and with both, and interpreted by
`sg` (`--interpreted`); `objects.lox` only compiled and without the stress (a million objects collected at
every allocation would take hours, in clox too). A compiled build must also print an expression nested
200,000 levels, where clox ends on `SIGSEGV`, and end with its panic at five million, and end as clox does,
by `SIGPIPE`, when its output is closed after the first bytes. Last run: 265 programs, 2,114 runs, no difference.

`tests/fuzz.py` makes Lox programs from a seed, globals, functions with closures, classes with fields,
initializers, inheritance and `super`, loops bounded by a counter, expressions that mix numbers, texts,
`nil` and objects so that many end in an error at run time, and one in eight cut short or with a token
changed, so that the compiler reports errors; clox and each build, in each of its ways, must write the same
bytes. Last run: 5,000 programs (250 ended well, 1,125 with errors of compiling, 3,575 with an error at
run time, 50 left out as clox took more than ten seconds), no difference.

`the_trial_of_lox` in `interpreter/tests/cli.rs` builds it and runs a program, the REPL, a missing file and
a wrong use, compiled in each way and interpreted, held to what clox writes.
