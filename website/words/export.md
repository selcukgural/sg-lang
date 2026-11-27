---
name: export
kind: keyword
summary: Makes a function of SG that C calls by its name, or through a CFunc, a pointer to a function as C has it.
see: extern, unsafe, raw, struct, T*, sg build
learn: ../docs/learn/book/11-c.md#being-called-by-c
---

## Forms

| Form | What it is |
|---|---|
| `export R name(T a, U b) { ... }` | a function at the top level that C calls by the name written |
| `CFunc<T, U, R>` | a pointer to a function as C calls it: the parameters, then what it gives |
| `CFunc<T, U, R> f = name` | the name of an `export` function, or of a function of C, as a `CFunc` |
| `unsafe f(a, b)` | a call through a `CFunc` |
| `(raw void)f`, `unsafe (CFunc<T, R>)p` | the address as a `raw void`, and back |
| `export Calc* make()`, `ref Calc`, `read Calc` | in a library, a value of SG that C holds by its address |
| `sg build folder --library` | a static library of the `export` functions, and the header C includes |

## What it means

`export` in front of a function at the top level makes it a function of C. C sees it under the name written,
without its module, so the name is one in the whole program, is not a word of C, does not start as the names
of the code SG writes do (`sg_`, `f_` and the like), and is not a function of C's library that the runtime uses
(`strlen`, `qsort`, `atoi`). It is not a method, not `async` and not `main`.

It takes and gives what C has: integers, `float`, `double`, `bool`, raw pointers, `CFunc` values and structs
of plain data by value. A text from C is a `read CString`, its `const char*`, which `string.fromCString(text)`
copies with no `unsafe`. A `string`, a `char`, a collection, a `T?` and a borrow of plain data are refused; a
struct that owns memory goes by its address, as a library below shows. SG calls an
`export` function by its name with no `unsafe`, as any function; a C file of the program calls it by its name
too, since SG declares it before the file.

C has no other way to call SG. `qsort` is given a comparator, a library of callbacks is given a function to
call later, and the state a callback needs comes through C's `void*`: `unsafe Memory.addressOf(ref state)`
gives the address, and inside the callback `unsafe (ref State)data` lends it again. The interpreter keeps the
addresses it gave, and stops the program on a wrong type, a value that has ended or moved, and an address it
did not give. Compiled code checks none of this.

A `CFunc<A, B, R>` is written as a `Func` is, with the types C has. It is plain data: it is copied, kept in a
field and compared, and it may be `null` as a pointer of C may, so `CFunc<...>?` is not written. Where one is
expected, the name of an `export` function or of a function of C is one. Each call through it is inside
`unsafe`, since what it points at may be any function of C, and a call through a `null` one stops the program.

A panic inside an `export` function ends the process with its message: nothing unwinds through the frames of
C between it and `main`. A program with an `export` function is built for several threads, so a thread C
starts may call it.

`sg build folder --library` builds a static library of a folder with no `main` (`--shared` a shared one), and
writes the header C includes. Only the `export` functions are seen from outside; each library carries its own
runtime. A struct of SG that is not plain data goes to C by its address: an `export` function gives it as
`T*`, which C keeps, takes it back as `T*`, where it ends, or is lent it as `read T` or `ref T` for the call.
C sees only `typedef struct Name Name;`, so it cannot reach inside. In a library a panic aborts the program,
and so does a `NULL` given where no `null` is taken.

## Examples

### A comparator for `qsort`

```sg
extern {
    void qsort(raw void base, nuint count, nuint size, CFunc<raw void, raw void, int> compare)
}

struct Runner {
    int bib
    int seconds
}

// C calls this with the addresses of two items: fastest first, then by bib.
export int fastestFirst(raw void a, raw void b) {
    var x = unsafe ((raw Runner)a)[0]
    var y = unsafe ((raw Runner)b)[0]
    if x.seconds != y.seconds {
        return x.seconds < y.seconds ? -1 : 1
    }
    return x.bib < y.bib ? -1 : x.bib > y.bib ? 1 : 0
}

void main() {
    var runners = List<Runner> {
        Runner { bib = 7, seconds = 3121 },
        Runner { bib = 3, seconds = 2990 },
        Runner { bib = 12, seconds = 3121 },
        Runner { bib = 1, seconds = 3305 },
    }
    var size = (nuint)Memory.sizeOf<Runner>()
    unsafe qsort((raw void)runners.pointer(), (nuint)runners.length(), size, fastestFirst)
    for r in runners {
        print($"{r.bib} {r.seconds}")
    }
}
```

```text
3 2990
7 3121
12 3121
1 3305
```

`qsort` moves the items of the list in place and calls `fastestFirst` for each pair it compares. A `Runner` is
plain data, laid out as C lays it out, so the comparator reads one through the address C gives.

### Called from SG, and kept as a `CFunc`

```sg
extern {
    int abs(int value)
}

export int twice(int value) {
    return value * 2
}

export int negate(int value) {
    return -value
}

struct Step {
    string name
    CFunc<int, int> apply
}

void main() {
    print(twice(21))
    var steps = List<Step> {
        Step { name = "twice", apply = twice },
        Step { name = "negate", apply = negate },
        Step { name = "abs", apply = abs },
        Step { name = "none", apply = null },
    }
    var value = 5
    for step in steps {
        if step.apply == null {
            print($"{step.name}: skipped")
            continue
        }
        value = unsafe step.apply(value)
        print($"{step.name}: {value}")
    }
    CFunc<int, int> f = twice
    var address = (raw void)f
    var back = unsafe (CFunc<int, int>)address
    CFunc<int, int> g = negate
    print(back == f)
    print(f == g)
}
```

```text
42
twice: 10
negate: -10
abs: 10
none: skipped
true
false
```

`twice(21)` is an ordinary call. Through a `CFunc` the call is inside `unsafe`, whether it points at a function
of SG or at `abs` of C, and a `null` one is tested first. A `raw void` is how C hands a pointer to a function
around (`dlsym` gives one); the cast back is inside `unsafe`, since nothing checks what lies there.

### A C file calls it by name

```c title="retry/retry.c"
#include <stdint.h>

// Calls tryOnce, a function of SG, until it gives 0; the attempts it took, or -1.
int32_t with_retries(int32_t limit) {
    for (int32_t attempt = 1; attempt <= limit; attempt++) {
        if (tryOnce(attempt) == 0) {
            return attempt;
        }
    }
    return -1;
}
```

```sg file title="retry/main.sg"
extern "./retry.c" {
    int with_retries(int limit)
}

export int tryOnce(int attempt) {
    print($"attempt {attempt}")
    return attempt < 3 ? 1 : 0
}

void main() {
    print(unsafe with_retries(5))
    print(unsafe with_retries(2))
}
```

```text
$ cd retry
$ sg main.sg
attempt 1
attempt 2
attempt 3
3
attempt 1
attempt 2
-1
$ sg build main.sg -o retry
$ ./retry
attempt 1
attempt 2
attempt 3
3
attempt 1
attempt 2
-1
```

SG declares `int32_t tryOnce(int32_t)` before the C file, so the file calls it without a declaration of its own;
one that differs (`int64_t tryOnce(int64_t)`) is an error of the C compiler. Calls go from SG to C and back
again within one line of `main`.

### State through C's `void*`

```c title="lines/lines.c"
#include <stdint.h>
#include <string.h>

typedef void (*visit_fn)(void *start, uint64_t length, void *state);

// Calls visit for each line of the text, without its '\n'.
void each_line(const char *text, visit_fn visit, void *state) {
    const char *start = text;
    for (;;) {
        const char *end = strchr(start, '\n');
        if (end == NULL) {
            if (*start != '\0') visit((void *)start, strlen(start), state);
            return;
        }
        visit((void *)start, (uint64_t)(end - start), state);
        start = end + 1;
    }
}
```

```sg file title="lines/main.sg"
extern "./lines.c" {
    void each_line(read CString text, CFunc<raw void, nuint, raw void, void> visit, raw void state)
}

struct Stats {
    int lines
    int blank
    nint longest
}

export void countLine(raw void start, nuint length, raw void data) {
    var stats = unsafe (ref Stats)data
    var bytes = unsafe (raw byte)start
    var line = unsafe Memory.view(bytes, (nint)length)
    stats.lines += 1
    if line.length() == 0 {
        stats.blank += 1
    }
    if line.length() > stats.longest {
        stats.longest = line.length()
    }
}

void main() {
    var poem = CString.from("The cat sat\n\non the warm mat\nand slept") ?? panic("a zero byte inside")
    var stats = Stats { lines = 0, blank = 0, longest = 0 }
    unsafe each_line(read poem, countLine, Memory.addressOf(ref stats))
    print($"{stats.lines} lines, {stats.blank} blank, the longest {stats.longest} bytes")
}
```

```text
$ cd lines
$ sg main.sg
4 lines, 1 blank, the longest 15 bytes
$ sg build main.sg -o lines
$ ./lines
4 lines, 1 blank, the longest 15 bytes
```

C knows nothing of `Stats`: it carries its address as a `void*` and gives it back with each line.
`countLine` lends it again as `ref Stats`, and reads the line where it lies through `Memory.view`, a `read
byte[]` with the checks of a slice. The `CFunc` takes the parameter types the C file's `visit_fn` has.

### A library for C

```sg file title="weather/temps/temps.sg"
// Readings in tenths of a degree, kept by SG; C holds them by their address.
struct Readings {
    List<int> values
}

export Readings* temps_new() {
    return new Readings { values = List<int>() }
}

export void temps_add(ref Readings r, int tenths) {
    r.values.add(tenths)
}

export void temps_add_all(ref Readings r, raw int items, nuint count) {
    for value in unsafe Memory.view(items, (nint)count) {
        r.values.add(value)
    }
}

// The mean, rounded down; 0 for none.
export int temps_mean(read Readings r) {
    if r.values.length() == 0 {
        return 0
    }
    long sum = 0
    for v in read r.values {
        sum += v
    }
    return (int)(sum / r.values.length())
}

export void temps_free(Readings* r) {
}
```

```c title="weather/main.c"
#include <stdio.h>
#include "temps.h"

int main(void) {
    Readings *week = temps_new();
    int32_t first[] = { 182, 175, 201 };
    temps_add_all(week, first, 3);
    temps_add(week, 214);
    temps_add(week, -40);
    printf("mean %d\n", temps_mean(week));
    temps_free(week);
    return 0;
}
```

```text
$ cd weather
$ sg build temps --library
built libtemps.a and temps.h; a program links it with `libtemps.a -lm -pthread`
$ cc -std=c11 -o weather main.c libtemps.a -lm -pthread
$ ./weather
mean 146
```

`temps.h` declares `typedef struct Readings Readings;` and the five functions with the types of C
(`int32_t`, `uint64_t`, `const Readings *` for the `read` one), so C holds a `Readings` without seeing its list.
`temps_new` gives it to C; `temps_free` takes it back, and it ends there, its list with it, at the end of the
empty body. `temps_add(NULL, 1)` would stop the program with `` panic: C gave `null` to `temps_add`, which
takes a `ref Readings` ``.

## What stops the program

### A panic inside a comparator

```sg panic
extern {
    void qsort(raw void base, nuint count, nuint size, CFunc<raw void, raw void, int> compare)
}

export int strictly(raw void a, raw void b) {
    var x = unsafe ((raw int)a)[0]
    var y = unsafe ((raw int)b)[0]
    if x == y {
        panic($"{x} is there twice")
    }
    return x < y ? -1 : 1
}

void main() {
    var unique = List<int> { 42, -7, 19, 3 }
    unsafe qsort((raw void)unique.pointer(), (nuint)unique.length(), 4, strictly)
    print(unique)
    var twice = List<int> { 42, -7, 19, 42, 3 }
    unsafe qsort((raw void)twice.pointer(), (nuint)twice.length(), 4, strictly)
    print(twice)
}
```

```text
[-7, 3, 19, 42]
panic: 42 is there twice
  --> example.sg:9:9
 9 |         panic($"{x} is there twice")
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

`qsort` is in the middle, and C has no way to be left halfway, so the panic ends the process there, in the
interpreter and in compiled code alike. An error a callback meets is kept in its state, and the caller looks
at it when C returns.

### The address of another type

```sg panic
struct Tally {
    int count
}

struct Other {
    long total
}

export void bump(raw void data) {
    var tally = unsafe (ref Tally)data
    tally.count += 1
}

void main() {
    var right = Tally { count = 0 }
    CFunc<raw void, void> f = bump
    unsafe f(Memory.addressOf(ref right))
    print(right.count)
    var wrong = Other { total = 0 }
    unsafe f(Memory.addressOf(ref wrong))
    print(wrong.total)
}
```

```text
1
panic: `(ref Tally)` is given the address of an `Other`
  --> example.sg:10:17
 10 |     var tally = unsafe (ref Tally)data
    |                 ^^^^^^^^^^^^^^^^^^^^^^
```

The interpreter remembers the type behind each address `Memory.addressOf` gave. A compiled build does not: it
writes the `int` into the first bytes of `wrong` and prints `1`, as a program of C would.

### A `CFunc` that is `null`

```sg panic
struct Step {
    CFunc<int, int> apply
}

void main() {
    var step = Step { apply = null }
    print(step.apply == null)
    print(unsafe step.apply(3))
}
```

```text
true
panic: `apply` is called, and it is a `CFunc` that is `null`
  --> example.sg:8:11
 8 |     print(unsafe step.apply(3))
   |           ^^^^^^^^^^^^^^^^^^^^
```

Compiled code tests the pointer before the call too, so a `null` one stops the program with this message in
every build.

## What the compiler refuses

### A `string` from C

```sg error
export void greet(string name) {
    print(name)
}

void main() {
    greet("Ada")
}
```

```text
error: C calls `greet`, and cannot give it a `string`
  --> example.sg:1:19
 1 | export void greet(string name) {
   |                   ^^^^^^
  hint: C gives a text as `read CString`, its `const char*`, which `string.fromCString(text)` copies
```

### A function of SG given to C

```sg error
extern {
    void qsort(raw void base, nuint count, nuint size, CFunc<raw void, raw void, int> compare)
}

int byValue(raw void a, raw void b) {
    return 0
}

void main() {
    var numbers = List<int> { 3, 1, 2 }
    unsafe qsort((raw void)numbers.pointer(), (nuint)numbers.length(), 4, byValue)
    print(numbers)
}
```

```text
error: C cannot call `byValue`: it is a function of SG
  --> example.sg:11:75
 11 |     unsafe qsort((raw void)numbers.pointer(), (nuint)numbers.length(), 4, byValue)
    |                                                                           ^^^^^^^
  hint: a function C calls is written with `export`: `export int byValue(...)`
```

### A comparator of the wrong type

```sg error
extern {
    void qsort(raw void base, nuint count, nuint size, CFunc<raw void, raw void, int> compare)
}

export long byValue(raw void a, raw void b) {
    return 0
}

void main() {
    var numbers = List<int> { 3, 1, 2 }
    unsafe qsort((raw void)numbers.pointer(), (nuint)numbers.length(), 4, byValue)
    print(numbers)
}
```

```text
error: expected `CFunc<raw void, raw void, int>`, found `CFunc<raw void, raw void, long>`
  --> example.sg:11:75
 11 |     unsafe qsort((raw void)numbers.pointer(), (nuint)numbers.length(), 4, byValue)
    |                                                                           ^^^^^^^
```

C would read half of the `long` as the `int` it looks for.

### A call through a `CFunc` without `unsafe`

```sg error
export int twice(int value) {
    return value * 2
}

void main() {
    CFunc<int, int> f = twice
    print(f(4))
}
```

```text
error: `f` points at a function of C, and the checker cannot see what it does, so it is written inside `unsafe`
  --> example.sg:7:11
 7 |     print(f(4))
   |           ^^^^
  hint: `unsafe f(4)`, or `unsafe { ... }` around several statements
```

`twice(4)` needs no `unsafe`: by its name the compiler knows which function it is.

### A name of C's library

```sg error
export int strlen(raw byte text) {
    return 0
}

void main() {
    print(strlen(null))
}
```

```text
error: C cannot see a function of SG named `strlen`
  `strlen` is a function of C's library or of the system, which the code SG writes uses
  --> example.sg:1:12
 1 | export int strlen(raw byte text) {
   |            ^^^^^^
```

### A method

```sg error
struct Counter {
    int n

    export int bump(ref this) {
        this.n += 1
        return this.n
    }
}

void main() {
    var c = Counter { n = 0 }
    print(c.bump())
}
```

```text
error: a method is not called by C: `export` is written on a function at the top level
  --> example.sg:4:5
 4 |     export int bump(ref this) {
   |     ^^^^^^
  hint: C reaches a value through the pointer it is given: `export void step(raw void data) { var s = unsafe (ref State)data ... }`
```

### An async function

```sg error
export async int later() {
    return 1
}

async void main() {
    print(await later())
}
```

```text
error: a function that C calls is not async: C cannot wait for a task
  --> example.sg:1:1
 1 | export async int later() {
   | ^^^^^^
  hint: start the work with `Task.run` inside it, or call it from SG
```

### One name in two modules

```sg file title="scores/a/a.sg"
public export int score(int x) {
    return x
}
```

```sg file title="scores/b/b.sg"
export int score(int x) {
    return x * 2
}

public int scoreB(int x) {
    return score(x)
}
```

```sg file title="scores/main.sg"
use a
use b

void main() {
    print(score(3))
    print(scoreB(3))
}
```

```text
$ cd scores
$ sg main.sg
error: C would see two functions named `score`
  C sees the name of an `export` function as it is written, without its module
  --> b/b.sg:1:12
 1 | export int score(int x) {
   |            ^^^^^
  the other one:
  --> a/a.sg:1:19
 1 | public export int score(int x) {
   |                   ^^^^^
```

Two functions of SG of one name in two modules are fine; C has one space of names for the whole program, so
an `export` function takes a name no other one has.
