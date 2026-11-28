---
name: sg build
kind: command
summary: Compiles through C: a program, the program of its tests, a library C calls, or an object without an operating system.
see: sg, sg test, sg emit-c, extern, export, test
learn: ../docs/learn/book/14-building.md#debug-and-release-builds
---

## Forms

| Form | What it is |
|---|---|
| `sg build file.sg`, `sg build folder` | a debug build of the program, named after the file without `.sg`, or after the folder |
| `sg build path -o out` | the same, written to `out` |
| `sg build path --release` | a release build: optimised, overflow wraps, no checks of a debug build |
| `sg build path --test` | the program that runs the tests, each in a process of its own; `./tests --only N` runs one |
| `sg build path --library` | a static library C links, `libname.a` (`name.lib` on Windows), and its header `name.h` |
| `sg build path --shared` | a shared library, `libname.so` (`.dylib` on macOS, `name.dll` on Windows), and its header |
| `sg build path --freestanding` | an object, `name.o`, for a machine without an operating system |
| `sg build path --freestanding --target triple` | the same for another machine, compiled by Clang |
| `sg build path -L folder`, `-Lfolder` | looks for the libraries of `extern` blocks in that folder first; given once for each folder |
| `sg build path -- flags` | gives the C compiler these flags too, after sg's own |
| `CC=compiler sg build path` | builds with that C compiler |
| `sg build --help` | prints this page |

## What it means

`sg build` checks the program as `sg` does, writes it as C, and has a C compiler make it. The C compiler is the
one `CC` names, else `cc` on Linux and macOS and Clang on Windows. What the build makes on the way, the C and
the objects, is in a folder of the temporary folder made new for it, which only its user opens, and removed
at the end. `sg emit-c` prints that C instead.

A **debug build**, the default, is compiled without optimisation (`-O0`) and checks what the interpreter checks:
integer overflow, a cast that does not fit, a shift past the bits of its type, raw memory read at an address not
aligned for its type, and, when `main` returns, the `shared` values still alive. It carries what the system's
debugger reads: inside the program on Linux, in `name.dSYM` beside it on macOS, in `name.pdb` on Windows. A
**release build** (`--release`) is optimised (`-O2`), lets overflow wrap and a cast keep the bits that fit,
and counts nothing. What keeps memory safe is checked in both: an index out of range,
division by zero, a conflict of borrows, `assert` and `panic`.

The output goes to the current folder, whatever folder the source is in, or to the path `-o` names, a file
there being written over, unless it is a source: the program's own file, a file of SG or of C, or a header
`sg build` did not write is never replaced. A program is named after its file without `.sg`, or after its folder; `sg build .`
names it `a.out`, so it is given `-o`. On Windows a program gets `.exe`. `--test` builds the `test` blocks
instead of `main`, as `sg test --compiled` does; with `--release` too, the tests wrap and count nothing.

A library is the `export` functions of the program, which must have no `main`. It is named after its folder or
file: `sg build temps --library` writes `libtemps.a` and `temps.h`, and says how a program of C links it. A
shared library shows only its `export` functions and is never unloaded. `--freestanding` writes an object with
no `main` and nothing of C's library, which the code that starts the machine links; with `--target` the
machine is another one, and the build uses Clang: the one `CC` names, else the first `clang` of the path.

A program and its tests write nothing when they are built. A library and an object write a line, `built ...`,
on the standard output.

`-L folder` names a folder where the libraries of `extern` blocks are looked for first, and it is written into
the program (its `rpath` on Linux and macOS), so the program finds them when it runs. On Windows the DLLs
linked from those folders are copied beside the program.

The flags after `--` go to the C compiler on every kind of build, after sg's own, so the last one written
wins: `-O3` over `-O2`, `-march=native`, `-flto`, `-g`, `-fsanitize=address`, `-l`, `-I`, `-Wl,`. Most go on
quietly, unknown ones too. Some go on with a warning, written on every build: what changes fractional results
(`-ffast-math`, `-Ofast`, `-ffp-contract=fast` or `on`, `-funsafe-math-optimizations`, `-ffinite-math-only`,
`-fno-signed-zeros`, `-freciprocal-math`, `-fassociative-math`, `-mfpmath=387`), what takes a guard away
(`-fno-stack-clash-protection`, `execstack` or `norelro` for the linker), and a setting of SG's runtime
(`-DSG_...`, `-USG_...`). Some are refused before anything is compiled: what sg decides itself (`-o`, `-c`,
`-S`, `-E`, `-x`, `-std=`, `-shared`, `-static`, `--target`) and what changes the size or the layout of C's
types (`-m16`, `-m32`, `-mx32`, `-fshort-enums`, `-fpack-struct`, `-fshort-wchar`, `-funsigned-char`,
`-fsigned-char`). A flag of C turns off no check of SG: only `--release` leaves out the checks of a debug build.

The exit code is 0 when it is built; 1 when the checker refuses the program, or the C compiler cannot be run
or does not build it; 2 when the command line is not one of the forms (a second path, a flag given twice, `-o`
or `--target` with nothing after it, `--target` without `--freestanding`), a path or a folder of `-L` or `-o` is
not there, the output is a folder or a source, or a flag of C is refused.

## Examples

### A debug build and a release build

```sg file title="level/level.sg"
void main() {
    byte level = 250
    for _ in 0..10 {
        level += 1
    }
    print(level)
}
```

```text
$ cd level
$ sg build level.sg
$ ./level
panic: overflow: 255 + 1 does not fit in byte
  --> level.sg:4:9
 4 |         level += 1
   |         ^^^^^^^^^^
  hint: `tryAdd` gives `null` instead, and `wrappingAdd` wraps around
$ sg build level.sg --release -o level-fast
$ ./level-fast
4
```

The debug build stops where the interpreter stops, with the same message; the release build wraps, 250 and
ten more being 4 in a `byte`. Neither build writes anything when it works.

### A folder, named with `-o`

```sg file title="greet/words/caps.sg"
public string capitalized(read string word) {
    if word.length() == 0 {
        return ""
    }
    return $"{word[0..1].toUpper()}{word[1..]}".toString()
}
```

```sg file title="greet/main.sg"
use words

void main(List<string> args) {
    if args.length() == 0 {
        print("Hello, nobody.")
        return
    }
    for name in read args {
        print($"Hello, {capitalized(name)}.")
    }
}
```

```text
$ sg build greet -o greeter
$ ./greeter ada grace
Hello, Ada.
Hello, Grace.
$ ./greeter
Hello, nobody.
```

A folder is a program, with the folders under it as its modules. Built from the folder above, it would be
named `greet`, which is the folder itself, so it is named with `-o`.

### The program of tests

```sg file title="tax/tax.sg"
long withTax(long cents, int percent) {
    return cents + cents * percent / 100
}

test "twenty percent" {
    assert(withTax(1000, 20) == 1200)
}

test "a third" {
    assert(withTax(999, 33) == 1329)
}
```

```text
$ cd tax
$ sg build tax.sg --test -o tests
$ ./tests
ok    twenty percent
FAIL  a third
panic: assert failed: withTax(999, 33) == 1329
  left: 1328
  right: 1329
  --> tax.sg:10:5
 10 |     assert(withTax(999, 33) == 1329)
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
1 of 2 test(s) failed
$ ./tests --only 0; echo $?
0
$ ./tests --only 1; echo $?
panic: assert failed: withTax(999, 33) == 1329
  left: 1328
  right: 1329
  --> tax.sg:10:5
 10 |     assert(withTax(999, 33) == 1329)
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
1
```

The program starts itself again for each test, with `--only` and its number, counted from 0 in the order of the
report. Run so by hand, one test runs in the program's own process, writes no line of its own and ends with 0
when it passes: that is the way to look at one test in a debugger, `gdb --args ./tests --only 1`.

### Flags of C

```text
$ cd level
$ sg build level.sg -o level-o3 -- -O3
$ ./level-o3
panic: overflow: 255 + 1 does not fit in byte
  --> level.sg:4:9
 4 |         level += 1
   |         ^^^^^^^^^^
  hint: `tryAdd` gives `null` instead, and `wrappingAdd` wraps around
$ sg build level.sg --release -o level-small -- -Os -DSG_MAIN_STACK=67108864
warning: -DSG_MAIN_STACK=67108864: it changes a setting of SG's runtime, which is not a promise and may change from one version to the next
$ ./level-small
4
```

With `level.sg` of the first example. `-O3` optimises the debug build harder and keeps its checks. The second
build gives `main` a stack of 64 MB instead of 1 GB: it is built, and told on every build, since the names
of SG's runtime may change.

### A library of C, with `-L`

```c title="scale/grams.c"
#include <stdint.h>

int64_t ounces_to_centigrams(int64_t ounces) {
    return ounces * 2835;
}
```

```sg file title="scale/main.sg"
extern "grams" {
    long ounces_to_centigrams(long ounces)
}

void main(List<string> args) {
    for word in read args {
        var ounces = long.parse(word) ?? panic($"not a number: {word}")
        print($"{ounces} oz = {unsafe ounces_to_centigrams(ounces) / 100} g")
    }
}
```

```text
$ cd scale
$ mkdir -p vendor && cc -c -fPIC -o grams.o grams.c && ar rcs vendor/libgrams.a grams.o
$ sg build main.sg -L vendor -o scale
$ ./scale 1 16
1 oz = 28 g
16 oz = 453 g
$ sg -L vendor main.sg 8
8 oz = 226 g
```

`extern "grams"` is linked as `-lgrams`, found in `vendor`. The interpreter takes `-L` before the file, since
the words after the file are the program's.

### A library for C

```sg file title="tally/tally/tally.sg"
// The sum of `count` integers at `items`; 0 for none.
export long tally_sum(raw int items, nuint count) {
    long total = 0
    for value in unsafe Memory.view(items, (nint)count) {
        total += value
    }
    return total
}

export int tally_max(int a, int b) {
    return a.max(b)
}
```

```c title="tally/main.c"
#include <stdio.h>
#include "tally.h"

int main(void) {
    int32_t scores[] = { 7, 12, 5 };
    printf("%lld %d\n", (long long)tally_sum(scores, 3), tally_max(-4, 9));
    return 0;
}
```

```text
$ cd tally
$ sg build tally --library
built libtally.a and tally.h; a program links it with `libtally.a -lm -pthread`
$ cc -std=c11 -o scores main.c libtally.a -lm -pthread
$ ./scores
24 9
$ grep tally_ tally.h
TALLY_API int64_t tally_sum(int32_t *items, uint64_t count);
TALLY_API int32_t tally_max(int32_t a, int32_t b);
```

`tally.h` declares the `export` functions with the types of C. `--shared` builds `libtally.so` (`libtally.dylib`
on macOS) from the same folder, which a program links with `-L. -ltally`. A library is a debug build too unless
`--release` is given, and a panic in it stops the program of C with the message.

### Without an operating system

```sg file title="board/device.sg"
// A checksum the code that starts the board calls.
export uint checksum(raw byte data, nuint length) {
    uint sum = 0
    for b in unsafe Memory.view(data, (nint)length) {
        sum = sum.wrappingMultiply(31).wrappingAdd(b)
    }
    return sum
}

void onPanic(read string message) {
}
```

```c title="board/start.c"
#include <stdint.h>
#include <stdio.h>

uint32_t checksum(const uint8_t *data, uint64_t length);

int main(void) {
    printf("%u\n", checksum((const uint8_t *)"board", 5));
    return 0;
}
```

```text
$ cd board
$ sg build device.sg --freestanding
built device.o for a machine without an operating system; link it with the code that starts the machine; it takes nothing of C's library, and of the C compiler's own only what that compiler calls on its own (GCC's atomics on 64-bit Arm, `-lgcc`)
$ cc -o start start.c device.o
$ ./start
93908710
```

`device.o` has no `main` and takes nothing of C's library, so the C program here stands in for a machine. A
program built so has an `onPanic` function, which a panic goes to. For a real device the machine is named:
`sg build kernel --freestanding --target riscv64-unknown-elf -- -march=rv64gc -mabi=lp64d`, compiled by a Clang
that knows that machine (Apple's knows only its own).

## Mistakes it tells

### An output in a folder that is not there

```sg file title="hello/hello.sg"
void main() {
    print("hello")
}
```

```text
$ sg build hello/hello.sg -o bin/hello; echo $?
error: cannot write bin/hello: there is no folder bin
2
```

It is told before anything is compiled. An output that is a folder is told too: `sg build greet` from the
folder above `greet` gives `` error: `greet` is a folder; name the program with -o ``.

### An output that would replace a source

```text
$ sg build hello/hello.sg -o hello/hello.sg; echo $?
error: cannot write hello/hello.sg: it is the program itself, which the build would replace; name the output with -o
2
```

A C compiler refuses an output that is one of its inputs, and so does `sg build`, before anything is compiled.
A library's header is kept the same way when a header of one's own lies where it would go.

### A flag of C that sg decides

```text
$ sg build hello/hello.sg -- -o hello/out; echo $?
error: -o in the C flags: the output is sg's to name: write -o before `--`
2
$ sg build hello/hello.sg -- -m32; echo $?
error: -m32 in the C flags: it changes the size or the layout of C's types, which the C that sg writes, and the libraries it calls, are written for
2
```

### A C compiler that cannot be run

```text
$ CC=nosuchcc sg build hello/hello.sg -o hi; echo $?
error: cannot run the C compiler `nosuchcc`: No such file or directory (os error 2)
1
```

`CC` names one program; a C compiler that is there but cannot build what sg writes is told by its own messages,
then by sg's.

### A library with a `main`

```text
$ sg build hello/hello.sg --library; echo $?
error: a library has no `main`: C calls its `export` functions
  --> hello/hello.sg:1:6
 1 | void main() {
   |      ^^^^
  hint: a program that starts at `main` is built without `--library` and `--shared`
1
```

A library that has no `export` function is refused too, with the hint of one.
