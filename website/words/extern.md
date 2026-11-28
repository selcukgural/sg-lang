---
name: extern
kind: keyword
summary: Declares the functions of a C library, or of a C file beside the program, to call inside unsafe.
see: unsafe, raw, export, struct, public, use
learn: ../docs/learn/book/11-c.md#calling-a-function-of-c
---

## Forms

| Form | What it is |
|---|---|
| `extern { ... }` | functions of the C library itself |
| `extern "m" { ... }` | functions of a library, linked as `-lm` |
| `extern "z", "zlib" { ... }` | the names one library has on different systems; the first one found is linked |
| `extern "./native.c" { ... }` | functions of a C file in the module's folder, compiled with the program |
| `R name(T a, U b)` | one function on each line, with no body |
| `R name(T a, ...)` | a function that takes a varying count of arguments after the named ones |
| `struct FILE` | a type of C that SG only points at: `raw FILE` |
| `int opterr` | a variable of C, read and written inside `unsafe` |
| `public R name(...)` | a function of C that other modules may call too |

## What it means

An `extern` block, written at the top level of a file, declares functions of C with SG's types. Nothing is
written by hand between the two: compiled code calls the function as C does, and the interpreter builds a small
library of wrappers with the C compiler and opens it, so `sg file.sg` and a built program call the same code.
Only the libraries of functions the program calls are linked, and a library that is not there is an error
before the program runs.

The checker cannot see what a function of C does, so every call is written inside `unsafe`. The value it gives
is used, or dropped on purpose with `_ =`, as any other. A function of C is a name of its module, private to it
unless `public` is written in front of it. The usual way is to keep the block inside a module that checks what
it is given, turns C's codes into SG's types and gives the rest of the program functions with no `unsafe`.

C takes integers, `float`, `double`, `bool`, raw pointers, structs of plain data by value, and `ref T` and
`read T` of plain data that is not an array, which are C's `T*` and `const T*` for the time of the call. A text
goes as a `read CString`, C's `const char*`, made with `CString.from(text)`. C gives back a number, a `bool`, a
raw pointer, a struct of plain data, or nothing. `buffer.pointer()`, written only as an argument, gives C the
items of a list, an array or a slice of plain data for the call. C's `long` is `CLong` in SG, since its size
differs between systems. A `string`, a `char`, a collection and a `T?` are refused where they are declared,
most with a hint of what to write instead.

After `...` each value must have a type C reads there: `int`, `uint`, `long`, `ulong`, `nint`, `nuint`,
`double`, a raw pointer, a `CFunc`, `ref T` or `read T` of plain data, or a `read CString`. C reads the small
types as `int` and a `float` as a `double`, so those are cast first, and a number or `null` written bare is
refused, since C would read it as whatever type it looks for.

`extern "./native.c"` names a C file in the module's folder or below it. It is compiled with the program, so
what cannot be declared, a macro, a `static inline` function, a builtin of the C compiler, is reached through a
function written there. All the C files of a program share one unit of C.

The compiler cannot check a declaration against the library: it trusts the types written. A function declared
with the wrong types is called the wrong way, as in C, and gives whatever the machine leaves.

## Examples

### Functions of the C library

```sg
extern {
    nuint strlen(read CString text)
    int toupper(int c)
    int atoi(read CString text)
}

void main() {
    var name = CString.from("pasha") ?? panic("a zero byte inside")
    print(unsafe strlen(read name))
    print((char)unsafe toupper((int)'p'))
    var count = CString.from("42 cats") ?? panic("a zero byte inside")
    unsafe {
        var n = atoi(read count)
        print(n + 1)
    }
}
```

```text
5
P
43
```

`extern { }` names no library: these are functions of the C library itself. A `char` of SG is 32 bits, so the
character goes to `toupper` as an `int` and comes back the same way. An `unsafe` block covers several
statements.

### A library by its name

```sg
extern "m" {
    double cbrt(double value)
    double hypot(double x, double y)
    double nextafter(double from, double toward)
}

void main() {
    print(unsafe cbrt(27.0))
    print(unsafe cbrt(-8.0))
    print(unsafe hypot(3.0, 4.0))
    print(unsafe hypot(1e308, 1e308))
    print(unsafe nextafter(1.0, 2.0) - 1.0)
}
```

```text
3
-2
5
1.4142135623730951e+308
2.220446049250313e-16
```

`"m"` is linked as `-lm`, the math library of C. `hypot` gives the length without squaring first, so it does
not overflow where `x * x` would.

### What C takes and gives

```sg
extern {
    Div div(int numerator, int denominator)
    double modf(double value, ref double whole)
    raw byte strdup(read CString text)
}

// C's `div_t`: plain data, laid out as C lays it out.
struct Div {
    int quot
    int rem
}

void main() {
    var d = unsafe div(-17, 5)
    print($"{d.quot} remainder {d.rem}")

    double whole = 0.0
    var part = unsafe modf(-3.75, ref whole)
    print($"{whole} and {part}")

    var name = CString.from("Pasha") ?? panic("a zero byte inside")
    var copy = unsafe strdup(read name)
    unsafe {
        copy[0] = (byte)'p'
    }
    print(unsafe string.fromCString(copy) ?? "not UTF-8")
    unsafe Memory.free(copy)
}
```

```text
-3 remainder -2
-3 and -0.75
pasha
```

`div` gives a struct by value, which SG declares with the fields C has, in C's order. `modf` writes the whole
part through a `ref double`, its `double*`. `strdup` gives memory of C's `malloc`, which the program changes
through the `raw byte`, copies back into a `string` and gives back with `Memory.free`.

### A varying count of arguments

```sg
extern {
    int snprintf(raw byte target, nuint size, read CString format, ...)
}

void main() {
    var line = List<byte>.repeat(0, 64)
    var format = CString.from("%-6s|%5d|%08.3f|%lx") ?? panic("a zero byte inside")
    var name = CString.from("tea") ?? panic("a zero byte inside")
    var written = unsafe snprintf(line.pointer(), (nuint)line.length(), read format, read name, (int)42, (double)3.14159, (long)255)
    print(written)
    print(string.fromUtf8(line[0..written]) ?? "not UTF-8")

    var small = List<byte>.repeat(0, 8)
    var needed = unsafe snprintf(small.pointer(), (nuint)small.length(), read format, read name, (int)42, (double)3.14159, (long)255)
    print(needed)
    print(string.fromUtf8(small[0..7]) ?? "not UTF-8")
}
```

```text
24
tea   |   42|0003.142|ff
24
tea   |
```

Each value after the format says its type: `%d` reads an `int`, `%f` a `double` and `%lx` a `long`. The format
itself is not looked at, so it is the program's to match. `line.pointer()` lends the list's bytes for the call;
into a buffer of 8 bytes `snprintf` writes 7 and the zero, and still gives the length the whole line needs.

### A C file beside the program

```c title="bits/native.c"
#include <stdint.h>

#define SQUARE(x) ((x) * (x))

static inline int32_t clamp(int32_t value, int32_t low, int32_t high) {
    return value < low ? low : value > high ? high : value;
}

int32_t square_clamped(int32_t value) {
    return clamp(SQUARE(value), 0, 100);
}

uint32_t count_bits(uint64_t value) {
    return (uint32_t)__builtin_popcountll(value);
}
```

```sg file title="bits/main.sg"
extern "./native.c" {
    int square_clamped(int value)
    uint count_bits(ulong value)
}

void main() {
    print(unsafe square_clamped(7))
    print(unsafe square_clamped(12))
    print(unsafe count_bits(0xff))
    print(unsafe count_bits(ulong.MaxValue))
}
```

```text
$ cd bits
$ sg main.sg
49
100
8
64
$ sg build main.sg -o bits
$ ./bits
49
100
8
64
```

A macro, a `static inline` function and a builtin of the compiler cannot be declared in an `extern` block; a
function of the C file reaches all three, and is compiled with the program, so the `static inline` one is
inlined. The path starts with `./` and is looked for beside the file of SG.

### Behind a safe module

```sg file title="calc/number/parse.sg"
extern {
    long strtoll(read CString text, ref raw byte end, int base)
}

// The number `text` writes in `base`, or `null` when it is not one number that fits.
public long? parseIn(read string text, int base) {
    if base < 2 || base > 36 || text.length() == 0 {
        return null
    }
    var c = CString.from(text) ?? return null
    raw byte end = null
    C.setErrno(0)
    var value = unsafe strtoll(read c, ref end, base)
    if C.errno() != 0 || unsafe end[0] != 0 {
        return null
    }
    return value
}
```

```sg file title="calc/main.sg"
use number

void show(read string text, int base) {
    var n = parseIn(text, base)
    if n is null {
        print($"{text} (base {base}): not a number")
        return
    }
    print($"{text} (base {base}): {n}")
}

void main() {
    show("ff", 16)
    show("-777", 8)
    show("zz", 36)
    show("12 apples", 10)
    show("9223372036854775807", 10)
    show("9223372036854775808", 10)
    show("101", 1)
}
```

```text
$ cd calc
$ sg main.sg
ff (base 16): 255
-777 (base 8): -511
zz (base 36): 1295
12 apples (base 10): not a number
9223372036854775807 (base 10): 9223372036854775807
9223372036854775808 (base 10): not a number
101 (base 1): not a number
```

`strtoll` is private to `number`, which checks the base before C sees it, and reads what C left: `errno` says
the number did not fit, and the address `strtoll` wrote into `end` says where it stopped. `main.sg` writes no
`unsafe`; naming `strtoll` there gets `` `strtoll` exists in the `number` module but is not public ``.

## What the compiler refuses

### A text given back

```sg error
extern {
    string getenv(read CString name)
}

void main() {
    var name = CString.from("HOME") ?? panic("a zero byte inside")
    print(unsafe getenv(read name))
}
```

```text
error: the C function `getenv` cannot give a `string`
  --> example.sg:2:5
 2 |     string getenv(read CString name)
   |     ^^^^^^
  hint: C gives a text as `raw byte`; `unsafe string.fromCString(p)` copies it
```

A `string` of SG is not ended by a zero byte and owns its memory; what C gives is an address.

### A `char`

```sg error
extern {
    int toupper(char c)
}

void main() {
    print(unsafe toupper('q'))
}
```

```text
error: the C function `toupper` cannot take a `char`
  a `char` of SG is a Unicode character of 32 bits, and a `char` of C is a byte
  --> example.sg:2:17
 2 |     int toupper(char c)
   |                 ^^^^
  hint: `byte`, or `sbyte` for a C `char` that is signed
```

### A bare number after `...`

```sg error
extern {
    int printf(read CString format, ...)
}

void main() {
    var format = CString.from("%ld seconds\n") ?? panic("a zero byte inside")
    _ = unsafe printf(read format, 30)
}
```

```text
error: this number has no type of its own, and C reads it after `...` as whatever it looks for
  C reads each value after `...` as the type the function looks for, and only the call says what it gives
  --> example.sg:7:36
 7 |     _ = unsafe printf(read format, 30)
   |                                    ^^
  hint: say what C reads: `(int)30`, or `(long)30` where C takes a `long`
```

`%ld` reads a `long`; given an `int`, C would read half of it and whatever lay beside it.

### `...` on a function of SG

```sg error
int total(int first, ...) {
    return first
}

void main() {
    print(total(1, 2, 3))
}
```

```text
error: a function of SG takes a fixed count of arguments: only a function of C, declared in an `extern` block, takes `...`
  --> example.sg:1:22
 1 | int total(int first, ...) {
   |                      ^^^
  hint: several values of one type are given as a slice: `read int[] values`
```

### A body in the block

```sg error
extern {
    int abs(int value) {
        return value < 0 ? -value : value
    }
}

void main() {
    print(unsafe abs(-3))
}
```

```text
error: a C function has no body: it is declared here, and C has it
  --> example.sg:2:24
 2 |     int abs(int value) {
   |                        ^
  hint: a function of your own is written outside the `extern` block
```

### A pointer to a buffer kept

```sg error
extern {
    int snprintf(raw byte target, nuint size, read CString format, ...)
}

void main() {
    var line = List<byte>.repeat(0, 32)
    var at = line.pointer()
    var format = CString.from("%d") ?? panic("a zero byte inside")
    _ = unsafe snprintf(at, 32, read format, (int)7)
}
```

```text
error: `pointer()` is written only as an argument of a C function
  C is given the address of the items for the time of the call; a pointer kept after it could point at items that have moved
  --> example.sg:7:14
 7 |     var at = line.pointer()
   |              ^^^^^^^^^^^^^^
  hint: memory that C keeps is taken with `Memory.allocate<T>(count)`
```

### A function of C as a value

```sg error
extern {
    int abs(int value)
}

void main() {
    var f = abs
    print(unsafe f(-3))
}
```

```text
error: `abs` is a function of C, and a `Func` holds the address of a function of SG
  --> example.sg:6:13
 6 |     var f = abs
   |             ^^^
  hint: wrap the call in a function of your own and use that
```

Where a `CFunc<int, int>` is expected, a pointer to a function as C calls it, the name of a function of C is
one: `CFunc<int, int> f = abs`.

### A name of C taken twice

```sg error
extern {
    int abs(int value)
}

int abs(int value) {
    return value < 0 ? -value : value
}

void main() {
    print(abs(-3))
}
```

```text
error: function `abs` is defined twice: here, and as a function of C in an `extern` block
  --> example.sg:5:5
 5 | int abs(int value) {
   |     ^^^
```

A function of C is a name of its module, as a function of SG is.

### One function of C, declared two ways

```sg file title="clash/a/a.sg"
extern {
    int abs(int value)
}

public int distanceA(int x) {
    return unsafe abs(x)
}
```

```sg file title="clash/b/b.sg"
extern {
    long abs(long value)
}

public long distanceB(long x) {
    return unsafe abs(x)
}
```

```sg file title="clash/main.sg"
use a
use b

void main() {
    print(distanceA(-3))
    print(distanceB(-4))
}
```

```text
$ cd clash
$ sg main.sg
error: the C function `abs` is declared twice, and the two declarations differ
  C has one function of this name, so it is declared the same way everywhere
  --> b/b.sg:2:10
 2 |     long abs(long value)
   |          ^^^
  the other declaration:
  --> a/a.sg:2:9
 2 |     int abs(int value)
   |         ^^^
```

Two modules may declare the same function of C, each for itself, but the same way. C's `labs` is the one that
takes a `long` (`CLong` in SG).

### A C file outside the module

```sg error
extern "./../shared.c" {
    int twice(int value)
}

void main() {
    print(unsafe twice(4))
}
```

```text
error: `./../shared.c` is not a C file of this module
  a C file beside the program lies in the module's folder or below it, ends in `.c`, and its path is letters, digits and `_ - . +`, without `..`
  --> example.sg:1:1
 1 | extern "./../shared.c" {
   | ^^^^^^^^^^^^^^^^^^^^^^
  hint: `extern "./native.c" { ... }`
```
