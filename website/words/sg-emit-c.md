---
name: sg emit-c
kind: command
summary: Prints the C that sg build compiles, for reading, for a sanitizer, or for a build of your own.
see: sg build, sg, sg test, extern, export, test
learn: ../docs/learn/book/14-building.md#the-c-that-sg-writes
---

## Forms

| Form | What it is |
|---|---|
| `sg emit-c file.sg`, `sg emit-c folder` | prints the C of a debug build of the program |
| `sg emit-c path --release` | the C of a release build: overflow wraps, and no check of a debug build is in it |
| `sg emit-c path --test` | the C of the program that runs the tests, in place of `main` |
| `sg emit-c path --library` | the C of a library: the program's `export` functions, with no `main` |
| `sg emit-c path --freestanding` | the C of an object for a machine without an operating system |
| `sg emit-c path -L folder`, `-Lfolder` | looks for the libraries of `extern` blocks in that folder too; given once for each folder |
| `sg emit-c --help` | prints this page |

## What it means

`sg emit-c` checks the program as `sg` does and prints, on the standard output, the C that `sg build` would
compile with the same flags. Nothing is compiled and no file is written: the C goes where the command line
sends it, `> main.c` or a pipe. A reader that closes early, such as `head`, ends it quietly.

The C is plain C11 and the whole program: the runtime of SG first, then the program's types and functions. The
lines at its top (`#define SG_THREADS 0`) choose the parts of the runtime the program uses. A function of SG
keeps its name with `f_` before it, a variable with `v_`, so the C can be read beside the source. Before each
function and statement a `#line` names its line of the `.sg` file, by the file's absolute path, so a debugger
and the C compiler's own messages point at SG; a C file named by an `extern "./native.c"` block is included
by its absolute path too. The C is therefore for this machine and these files.

It is the input of a build by hand: with flags of your own, a sanitizer, or another C compiler. A program
needs `-std=c11` and `-ffp-contract=off`, which keeps a multiply and an add apart so fractional results stay
the interpreter's, and on Linux and macOS `-lm` and `-pthread`. `sg build` also gives a debug build `-g`, and
on Linux `-fstack-clash-protection`. When an `extern` block names a library, the first line of the C says
what to link: `/* sg-link: -lz */`, the name found first among those the block gives. To find it, the C
compiler is asked where it looks, with the `-L` folders before.

`--release`, `--test`, `--library` and `--freestanding` choose what is printed as they choose what `sg build`
makes. `--release` goes with any of them; the other three exclude each other. `--library` prints the C of
the library alone: its header is written by `sg build --library`.

The exit code is 0 when the C was printed, 1 when the checker refuses the program, and 2 for a command line
that is not a usage and for a path that does not lead anywhere.

## Examples

### The C of a program

```sg file title="twice.sg"
int twice(int x) {
    return x * 2
}

void main() {
    print(twice(21))
}
```

```text
$ sg emit-c twice.sg > twice.c
$ grep -E '^static .* f_[a-z]+\(.*\);$' twice.c
static int32_t f_twice(int32_t v_x);
static void f_main(void);
$ cc -std=c11 -ffp-contract=off twice.c -o twice -lm -pthread
$ ./twice
42
```

The two lines `grep` finds are the program's own functions, declared before the rest; everything else in
`twice.c` is the runtime. Compiled by hand with those flags, it is the same program `sg build` makes.

### What a debug build checks

```text
$ sg emit-c twice.sg | grep 'v_x,'
    return sg_mul_int(v_x, ((int32_t)2LL), sg_loc_0);
$ sg emit-c twice.sg | grep '^static const char sg_loc_0'
static const char sg_loc_0[] = "  --> twice.sg:2:12\n 2 |     return x * 2\n   |            ^^^^^\n" "\0\0";
$ sg emit-c twice.sg --release | grep 'v_x,'
    return sg_wmul_int(v_x, ((int32_t)2LL));
$ sg emit-c twice.sg --release > fast.c
$ cc -std=c11 -O3 -ffp-contract=off fast.c -o fast -lm -pthread
$ ./fast
42
```

In a debug build `x * 2` is a call that checks for overflow, and is given the place to name when it panics:
the line, written out as the message shows it. In a release build the same multiplication wraps, and the
place is not there. `-O3` is a flag of your own; `sg build --release` compiles with `-O2`, or takes it after
`--`.

### The C of the tests

```sg file title="fines.sg"
int fineCents(int daysLate) {
    return daysLate * 25
}

test "a day late costs 25 cents" {
    assert(fineCents(1) == 25)
}

test "on time costs nothing" {
    assert(fineCents(0) == 0)
}
```

```text
$ sg emit-c fines.sg --test | grep '", t_'
    { "a day late costs 25 cents", t_0 },
    { "on time costs nothing", t_1 },
$ sg emit-c fines.sg
error: there is no `main` function
  --> fines.sg:11:2
 11 | }
    |  ^
  hint: this file holds tests; they are run by the test command: `sg test <file>`
```

With `--test` the C holds each test as a function and a table of their names, which its `main` runs as
`sg test --compiled` does. Without it the tests are left out, as `sg build` leaves them out, and this file has
nothing else to run.

### A library and an object without an operating system

```sg file title="temps/temps.sg"
export double toFahrenheit(double celsius) {
    return celsius * 9.0 / 5.0 + 32.0
}

export bool isFreezing(double celsius) {
    return celsius <= 0.0
}
```

```sg file title="device/device.sg"
void onPanic(read string message) {
}

export ulong sumTo(ulong n) {
    ulong total = 0
    for i in 1..n + 1 {
        total += i
    }
    return total
}
```

```text
$ sg emit-c temps --library | grep '^SG_EXPORT'
SG_EXPORT double toFahrenheit(double a0);
SG_EXPORT bool isFreezing(double a0);
$ sg emit-c device --freestanding | grep -m1 '^#define SG_FREESTANDING'
#define SG_FREESTANDING 1
$ sg emit-c device --freestanding | grep '^SG_EXPORT'
SG_EXPORT uint64_t sumTo(uint64_t a0);
```

An `export` function keeps its own name in the C, the one C calls, beside the `f_` function of SG it calls.
`SG_FREESTANDING` leaves out of the runtime what needs a system, so the object takes nothing of C's library.

### A library of C, and `-L`

```c title="maths/pasha.c"
#include <stdint.h>

int32_t pasha_twice(int32_t value) {
    return value * 2;
}
```

```sg file title="maths/main.sg"
extern "pashacalc", "pasha" {
    int pasha_twice(int value)
}

void main() {
    print(unsafe pasha_twice(21))
}
```

```text
$ cd maths
$ mkdir -p vendor
$ cc -c pasha.c -o pasha.o
$ ar rcs vendor/libpasha.a pasha.o
$ sg emit-c main.sg 2>&1 | head -1
error: none of the libraries this `extern` block names was found: `pashacalc`, `pasha`
$ sg emit-c main.sg -L vendor > main.c
$ head -1 main.c
/* sg-link: -lpasha */
$ cc -std=c11 -ffp-contract=off main.c -o main -Lvendor -lpasha -lm -pthread
$ ./main
42
```

The block names a library by two names; `libpasha.a` in `vendor` is the one found, and the first line of the
C says to link it. Without `-L` the library is not where the C compiler looks, and the C is not printed: the
message goes on to say where it looked, and gives the hint of `-L`.

## Mistakes it tells

### `-o` and flags of C

```text
$ sg emit-c twice.sg -o twice.c 2> usage.txt; echo $?
2
$ grep 'sg emit-c' usage.txt
       sg emit-c <file.sg | folder> [--release] [--test | --library | --freestanding] [-L folder]...
$ sg emit-c twice.sg -- -O3 > /dev/null 2>&1; echo $?
2
```

`sg emit-c` writes to its output, so `-o` is not one of its flags, and it compiles nothing, so neither are flags
of C after `--`. Both are a usage error: the usage of every command is printed, and nothing else. The same goes
for a flag given twice, a second program, and two of `--test`, `--library` and `--freestanding`.

### A library without `--library`

```text
$ sg emit-c temps
error: there is no `main` function
  --> temps/temps.sg:7:2
 7 | }
   |  ^
  hint: a library that C calls through its `export` functions is built with `sg build <folder> --library`, or `--shared`
```

Without `--library` the folder is read as a program, which starts at `main`. `sg emit-c temps --library` prints
its C.

### A program the checker refuses

```sg file title="late.sg"
void main() {
    int days = 3
    days = "three"
    print(days)
}
```

```text
$ sg emit-c late.sg; echo $?
error: expected `int`, found `string`
  --> late.sg:3:12
 3 |     days = "three"
   |            ^^^^^^^
1
```

The error is the one `sg late.sg` gives, and no C is printed, not even a part of it.

### A path that leads nowhere

```text
$ sg emit-c nothere.sg; echo $?
error: cannot read nothere.sg: No such file or directory (os error 2)
2
$ sg emit-c twice.sg -L nowhere; echo $?
error: -L nowhere: no such folder
2
```
