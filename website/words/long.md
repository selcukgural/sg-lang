---
name: long
kind: type
summary: An integer of 64 bits, with a sign; for sums, sizes of files and time in milliseconds.
see: int, ulong, nint, double
learn: ../docs/learn/book/02-values.md#conversions
library: ../docs/library/numbers.md
---

## Forms

| Form | What it is |
|---|---|
| `long` | an integer of 64 bits, from -9 223 372 036 854 775 808 to 9 223 372 036 854 775 807 |
| `3_000_000_000` | a number written in the program that does not fit in an `int` |
| `long total = 0` | a `long` from a number that would be an `int` alone |
| `long.MinValue`, `long.MaxValue` | the two ends |
| `(long)n * m` | an `int` made a `long` first, so the arithmetic is done in 64 bits |
| `(int)x`, `x.tryToInt()`, `x.saturatingToInt()` | a `long` back to an `int` |
| `(double)x`, `(nint)x` | a `long` as a fraction, rounded, or as an index |
| `long.parse(text)` | the number a text writes, as a `Result<long, NumberError>` |

## What it means

A `long` is a signed integer of 64 bits, on every machine. It is the type for what can pass two billion: a sum of
many values, the size of a file in bytes, a time in milliseconds or nanoseconds. `File.info` gives a size as a
`long`, `DateTime` gives its milliseconds as one, and `Duration` is made from one.

A number written in the program is a `long` where it goes into a `long`, or where it does not fit in an `int`:
`var big = 3_000_000_000` is a `long`, and `var small = 5` an `int`. To have a `long` from a small number, the
type is written: `long total = 0`.

An `int` becomes a `long` on its own, in an assignment, as an argument and next to a `long` in arithmetic:
`count * size` is a `long` when either is. Two `int`s still give an `int`, and the `long` comes only after:
`long bytes = megabytes * 1048576` multiplies in 32 bits and can overflow. `(long)megabytes * 1048576` makes the
`long` first.

Nothing becomes smaller on its own. A `long` goes to an `int` with `(int)`, which stops the program when the
value does not fit (a release build keeps the low 32 bits instead), or with `tryToInt()`, `saturatingToInt()` or
`wrappingToInt()`, which do the same in every build. A `long` does not mix with a `ulong`, since neither holds every value of the other, and goes to a
`double` only with `(double)`, since a `double` holds whole numbers exactly only up to 2^53. An index is an
`nint`, so a `long` used as one is written `(nint)x`.

Overflow follows the rule of every integer: a panic in the interpreter, in tests and in a debug build, wrapping
in a release build; `tryAdd`, `wrappingAdd` and `saturatingAdd` say one behaviour for every build. The methods
are in [Numbers](../docs/library/numbers.md).

## Examples

### When a number is a `long`

```sg
void main() {
    var small = 5
    var big = 3_000_000_000
    long total = 0
    total += small
    total += big
    print($"{total} {big * 4}")
    print($"{long.MinValue} {long.MaxValue}")
}
```

```text
3000000005 12000000000
-9223372036854775808 9223372036854775807
```

`small` is an `int` and widens as it is added. `big` is a `long` because it does not fit in an `int`, so `big * 4`
is computed in 64 bits.

### An `int` meets a `long`

```sg
long square(long x) {
    return x * x
}

void main() {
    int side = 100_000
    print(square(side))
    int count = 3
    long size = 4_000_000_000
    var total = count * size
    print(total)
    long sum = 0
    for i in 1..100_001 {
        sum += (long)i * i
    }
    print(sum)
}
```

```text
10000000000
12000000000
333338333350000
```

`side` becomes a `long` on its way into `square`. `count * size` is a `long` because `size` is one. In the loop
`i` is an `int`, and `(long)i` makes the square a `long` before it is taken: `i * i` alone would overflow from
46 341 on.

### The type reaches the numbers, not the variables

```sg
void main() {
    long month = 30 * 24 * 60 * 60 * 1000
    int days = 30
    long same = (long)days * 24 * 60 * 60 * 1000
    print($"{month} {same}")
}
```

```text
2592000000 2592000000
```

In `month` every number is written in the program, so each takes the type of the place, `long`. With `days`, an
`int`, the product would be computed in `int` and overflow; `(long)days` makes every step a `long`.

### Sizes of files

```sg
string human(long bytes) {
    if bytes < 1024 {
        return $"{bytes} B".toString()
    }
    if bytes < 1 << 20 {
        return $"{(double)bytes / 1024:F1} KB".toString()
    }
    if bytes < 1 << 30 {
        return $"{(double)bytes / (1 << 20):F1} MB".toString()
    }
    return $"{(double)bytes / (1 << 30):F2} GB".toString()
}

void main() {
    var sizes = List<long> { 512, 48_213, 7_340_032, 5_368_709_120 }
    long total = 0
    for size in read sizes {
        total += size
        print(human(size))
    }
    print($"{total} bytes, {human(total)}")
}
```

```text
512 B
47.1 KB
7.0 MB
5.00 GB
5376097877 bytes, 5.01 GB
```

A file of 5 GB does not fit in an `int`. `(double)bytes` is written out, since a `long` becomes a `double` only
with a cast. In `bytes < 1 << 30` the shift takes the type of `bytes`, so `1 << 40` would be a `long` too.

### Time in milliseconds

```sg
Result<void, DateError> main() {
    var start = try DateTime.parse("2026-11-27T09:15:00Z")
    var end = try DateTime.parse("2026-11-27T11:47:30.250Z")
    long ms = end.unixMilliseconds() - start.unixMilliseconds()
    long seconds = ms / 1000
    print($"{ms} ms")
    print($"{seconds / 3600}:{seconds / 60 % 60:D2}:{seconds % 60:D2}.{ms % 1000:D3}")
    print(Duration.fromMilliseconds(ms))
    print(start.unixMilliseconds())
    return Ok()
}
```

```text
9150250 ms
2:32:30.250
9150.25s
1795770900000
```

`unixMilliseconds()` counts from 1970 and is past two billion, so it is a `long`; so is the difference. `/` and
`%` split it into hours, minutes, seconds and milliseconds.

### Back to an `int`

```sg
void main() {
    long size = 4_000_000_000
    long kilobytes = size / 1000
    print((int)kilobytes)
    print(size.tryToInt() ?? -1)
    print(size.saturatingToInt())
    print(size.wrappingToInt())
    var names = List<string> { "Ada", "Grace", "Linus" }
    long last = 2
    print(names[(nint)last])
}
```

```text
4000000
-1
2147483647
-294967296
Linus
```

`(int)` is for a value that fits; `tryToInt` gives `null` for one that does not, `saturatingToInt` the nearest
end and `wrappingToInt` the low 32 bits. An index is an `nint`, so a `long` becomes one with `(nint)`.

### Read from text

```sg
void main() {
    print(long.parse("9007199254740993") ?? 0)
    print(long.parse("-9223372036854775808") ?? 0)
    match long.parse("9223372036854775808") {
        Ok(n) => print(n)
        Error(e) => print(e)
    }
    print(int.parse("3000000000") is Error(OutOfRange))
}
```

```text
9007199254740993
-9223372036854775808
OutOfRange
true
```

A number too large for an `int` is read with `long.parse`; `OutOfRange` says it does not fit in a `long` either.

## What stops the program

### An `int` product on its way into a `long`

```sg panic
long bytesOf(int megabytes) {
    return megabytes * 1024 * 1024
}

void main() {
    print(bytesOf(4096))
}
```

```text
panic: overflow: 4194304 * 1024 does not fit in int
  --> example.sg:2:12
 2 |     return megabytes * 1024 * 1024
   |            ^^^^^^^^^^^^^^^^^^^^^^^
  hint: `tryMultiply` gives `null` instead, and `wrappingMultiply` wraps around
```

`megabytes` is an `int`, so the product is one, and becomes a `long` only when it is returned.
`(long)megabytes * 1024 * 1024` is computed in 64 bits.

### An overflow

```sg panic
long next(long n) {
    return n + 1
}

void main() {
    print(next(long.MaxValue))
}
```

```text
panic: overflow: 9223372036854775807 + 1 does not fit in long
  --> example.sg:2:12
 2 |     return n + 1
   |            ^^^^^
  hint: `tryAdd` gives `null` instead, and `wrappingAdd` wraps around
```

In a release build it wraps to `long.MinValue`.

### A cast to `int` that does not fit

```sg panic
int toInt(long value) {
    return (int)value
}

void main() {
    print(toInt(4_000_000_000))
}
```

```text
panic: value 4000000000 does not fit in int
  --> example.sg:2:12
 2 |     return (int)value
   |            ^^^^^^^^^^
  hint: `tryToInt()` gives `null` when the value does not fit, and `wrappingToInt()` cuts it to size
```

## What the compiler refuses

### A `long` where an `int` goes

```sg error
int half(int n) {
    return n / 2
}

void main() {
    long size = 2048
    print(half(size))
}
```

```text
error: expected `int`, found `long`
  --> example.sg:7:16
 7 |     print(half(size))
   |                ^^^^
  hint: convert with `(int)`; it panics if the value does not fit
```

### A `long` and a `ulong`

```sg error
void main() {
    long a = 1
    ulong b = 2
    print(a + b)
}
```

```text
error: `long` and `ulong` do not mix: neither holds every value of the other
  --> example.sg:4:11
 4 |     print(a + b)
   |           ^^^^^
  hint: convert one side to the type of the other: `(long)b`; it panics if the value does not fit
```

### A `long` where a `double` goes

```sg error
void main() {
    long total = 10
    double average = total / 4
    print(average)
}
```

```text
error: expected `double`, found `long`
  --> example.sg:3:22
 3 |     double average = total / 4
   |                      ^^^^^^^^^
  hint: a `double` holds whole numbers exactly up to 2^53, and `long` goes past that; convert with `(double)`, which rounds
```

`(double)total / 4` divides as fractions and gives `2.5`.

### A `long` as an index

```sg error
void main() {
    long at = 1
    var names = List<string> { "Ada", "Grace" }
    print(names[at])
}
```

```text
error: expected `nint`, found `long`
  --> example.sg:4:17
 4 |     print(names[at])
   |                 ^^
  hint: convert with `(nint)`; it panics if the value does not fit
```

### A number past every `long`

```sg error
void main() {
    print(18_446_744_073_709_551_615)
}
```

```text
error: 18_446_744_073_709_551_615 does not fit in long
  --> example.sg:2:11
 2 |     print(18_446_744_073_709_551_615)
   |           ^^^^^^^^^^^^^^^^^^^^^^^^^^
  hint: write the type where it goes: `ulong x = 18_446_744_073_709_551_615`
```

A number is never unsigned on its own; a `ulong` is written.
