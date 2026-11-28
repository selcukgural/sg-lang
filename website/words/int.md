---
name: int
kind: type
summary: An integer of 32 bits, with a sign; a whole number is one when nothing else is asked for.
see: long, nint, uint, byte, double
learn: ../docs/learn/book/02-values.md#the-integer-types
library: ../docs/library/numbers.md
---

## Forms

| Form | What it is |
|---|---|
| `int` | an integer of 32 bits, from -2 147 483 648 to 2 147 483 647 |
| `42`, `0xff`, `0b1010`, `1_000_000` | a number written in the program: an `int` where nothing else is asked for |
| `int.MinValue`, `int.MaxValue` | the two ends |
| `a + b`, `a - b`, `a * b`, `a / b`, `a % b` | arithmetic in `int`; `/` cuts toward zero |
| `(int)x` | a value of another number type, as an `int` |
| `x.tryAdd(y)`, `x.wrappingAdd(y)`, `x.saturatingAdd(y)` | arithmetic that says what an overflow does |
| `x.tryToInt()`, `x.wrappingToInt()`, `x.saturatingToInt()` | a conversion to `int` that never panics |
| `int.parse(text)`, `int.parseHex(text)` | the number a text writes, as a `Result<int, NumberError>` |
| `$"{n:x8}"`, `$"{n:D3}"`, `$"{n,6}"` | an `int` written in hex, with zeros in front, in a width |

## What it means

An `int` is a signed integer of 32 bits, on every machine. It is the usual type for a count, a code or a small
sum. A number written in the program takes the type of the place it goes to (`byte b = 200`, `long total =
0`); where no place says a type, as in `var n = 5`, it is an `int` if it fits and a `long` if it does not.

An operation on two `int`s gives an `int`. An `int` becomes a wider type on its own where that type holds every
one of its values: `long`, `nint` and `double`. Every other way is written as a cast, `(byte)n` or `(uint)n`,
and so is the way back, `(int)total`. A number never becomes an `int` on its own from a `long`, a `double` or a
length.

Going past an end is an overflow. In the interpreter, in tests and in a debug build it stops the program, with
the values and the methods that do not stop it; a release build (`sg build --release`) wraps around instead, as
the machine does. A cast whose value does not fit follows the same rule. A division by zero, and
`int.MinValue / -1`, stop the program in every build. Where the program wants one behaviour in every build, a
method says it: `tryAdd` gives `null`, `wrappingAdd` wraps, `saturatingAdd` stops at the end; the same for
`Subtract` and `Multiply`, and `tryDivide`.

Lengths and positions are `nint`, not `int`: `list.length()` gives an `nint`. An `int` is an index as it is,
since it widens; a length becomes an `int` with `(int)`.

The methods are in [Numbers](../docs/library/numbers.md).

## Examples

### Numbers as written

```sg
void main() {
    var count = 42
    var mask = 0xff
    var flags = 0b1010
    var million = 1_000_000
    var big = 3_000_000_000
    print($"{count} {mask} {flags} {million} {big}")
    print($"{int.MinValue} {int.MaxValue}")
}
```

```text
42 255 10 1000000 3000000000
-2147483648 2147483647
```

The first four are `int`s. `big` does not fit in one, so it is a `long`.

### Division and remainder

```sg
void main() {
    int items = 17
    int perBox = 5
    print($"{items / perBox} boxes, {items % perBox} left")
    print($"{-17 / 5} {-17 % 5}")
    double half = (double)(items / 2)
    double exact = items / 2.0
    print($"{half} {exact}")
}
```

```text
3 boxes, 2 left
-3 -2
8 8.5
```

`/` drops the fraction toward zero, and `%` takes the sign of the left side. `items / 2` is divided as two
`int`s; where a `double` goes, the cast around it says the fraction is dropped on purpose (without it the line
is refused). With `2.0` the division is a `double`'s.

### Wider on its own, narrower with a cast

```sg
long square(long x) {
    return x * x
}

void main() {
    int side = 100000
    print(square(side))
    var scores = List<int> { 70, 85, 92 }
    int total = 0
    for s in read scores {
        total += s
    }
    int count = (int)scores.length()
    print($"{total / count} {scores[count - 1]}")
    print($"{(int)7.9} {(int)-7.9} {(int)'A'} {(byte)count}")
}
```

```text
10000000000
82 92
7 -7 65 3
```

`side` becomes a `long` on its way into `square`, so the product does not overflow. `length()` gives an `nint`,
which becomes an `int` with a cast; `count - 1` is an index as it is. A cast from a fraction drops what is after
the point.

### One behaviour in every build

```sg
int? total(read List<int> parts) {
    int sum = 0
    for p in parts {
        sum = sum.tryAdd(p) ?? return null
    }
    return sum
}

void main() {
    int top = int.MaxValue
    print($"{top.wrappingAdd(1)} {top.saturatingAdd(1)} {top.tryAdd(1) ?? 0}")
    print($"{(-5).saturatingMultiply(top)} {10.tryDivide(0) ?? -1}")
    var small = List<int> { 1, 2, 3 }
    var large = List<int> { top, 1 }
    print(total(read small) ?? -1)
    print(total(read large) ?? -1)
}
```

```text
-2147483648 2147483647 0
-2147483648 -1
6
-1
```

`tryAdd` gives `null` where `+` would overflow, and `?? return null` hands that on. These methods do the same in
the interpreter, in a debug build and in a release build.

### Into an `int`, without a panic

```sg
void main() {
    long small = 1234
    long big = 5_000_000_000
    print($"{small.tryToInt() ?? -1} {big.tryToInt() ?? -1}")
    print($"{big.wrappingToInt()} {big.saturatingToInt()}")
    double huge = 1e12
    print($"{huge.tryToInt() ?? -1} {huge.saturatingToInt()}")
}
```

```text
1234 -1
705032704 2147483647
-1 2147483647
```

`tryToInt` gives `null` where `(int)` would not fit, `wrappingToInt` keeps the low 32 bits, and
`saturatingToInt` gives the nearest end.

### Read from text

```sg
void main() {
    var line = "12, 7, x9, 30, 99999999999"
    int sum = 0
    for field in line.split(',') {
        var text = field.trim()
        match int.parse(text) {
            Ok(n) => sum += n
            Error(Invalid(at)) => print($"{text}: not a number from byte {at}")
            Error(OutOfRange) => print($"{text}: too large for an int")
        }
    }
    print(sum)
    print(int.parseHex("7fffffff") ?? 0)
}
```

```text
x9: not a number from byte 0
99999999999: too large for an int
49
2147483647
```

`parse` reads a sign and digits and nothing else, so the spaces are trimmed first. `parseHex` reads hex
digits without `0x`.

### Written into text

```sg
void main() {
    int code = 48879
    int day = 7
    print($"{code:x} {code:X8} {code:b}")
    print($"{day:D3} [{day,4}] [{day,-4}]")
    print($"{-1:x}")
    print(code.toString().length())
}
```

```text
beef 0000BEEF 1011111011101111
007 [   7] [7   ]
ffffffff
5
```

`x` and `X` write hex, `b` binary and `D` decimal, each with at least so many digits. A width after a comma pads
with spaces, in front or, when it is negative, after. A negative number in hex is written as its 32 bits.

### A release build wraps

```sg file title="wrap/main.sg"
int add(int a, int b) {
    return a + b
}

void main() {
    print(add(int.MaxValue, 1))
}
```

```text
$ cd wrap
$ sg main.sg
panic: overflow: 2147483647 + 1 does not fit in int
  --> main.sg:2:12
 2 |     return a + b
   |            ^^^^^
  hint: `tryAdd` gives `null` instead, and `wrappingAdd` wraps around
$ sg build --release main.sg -o add
$ ./add
-2147483648
```

A release build leaves the check out of every `+`, `-` and `*`, and the number wraps around. The wrapping is
defined, two's complement, and an overflow cannot make memory unsafe, since every index is still checked. Where
an overflow is a real case, `tryAdd` says so in every build.

## What stops the program

### An overflow

```sg panic
int area(int width, int height) {
    return width * height
}

void main() {
    print(area(70000, 70000))
}
```

```text
panic: overflow: 70000 * 70000 does not fit in int
  --> example.sg:2:12
 2 |     return width * height
   |            ^^^^^^^^^^^^^^
  hint: `tryMultiply` gives `null` instead, and `wrappingMultiply` wraps around
```

In a release build the product wraps. A `long` holds it: `(long)width * height`.

### A division by zero

```sg panic
int perHour(int count, int hours) {
    return count / hours
}

void main() {
    print(perHour(120, 0))
}
```

```text
panic: division by zero: 120 / 0
  --> example.sg:2:12
 2 |     return count / hours
   |            ^^^^^^^^^^^^^
```

This stops the program in every build, release too; `tryDivide` gives `null` instead. `%` by zero stops it the
same way.

### A cast that does not fit

```sg panic
int toInt(long value) {
    return (int)value
}

void main() {
    print(toInt(5_000_000_000))
}
```

```text
panic: value 5000000000 does not fit in int
  --> example.sg:2:12
 2 |     return (int)value
   |            ^^^^^^^^^^
  hint: `tryToInt()` gives `null` when the value does not fit, and `wrappingToInt()` cuts it to size
```

In a release build the cast keeps the low 32 bits, as `wrappingToInt()` does in every build.

## What the compiler refuses

### A number too large

```sg error
void main() {
    int population = 8_100_000_000
    print(population)
}
```

```text
error: `8_100_000_000` does not fit in int
  --> example.sg:2:22
 2 |     int population = 8_100_000_000
   |                      ^^^^^^^^^^^^^
  hint: a `long` holds it: `long population = 8_100_000_000`
```

`long population` holds it.

### A `long` where an `int` goes

```sg error
void main() {
    long total = 10
    int count = total
    print(count)
}
```

```text
error: expected `int`, found `long`
  --> example.sg:3:17
 3 |     int count = total
   |                 ^^^^^
  hint: convert with `(int)total`; a value that does not fit panics, or is cut to size in a release build
```

### A length as an `int`

```sg error
void main() {
    var names = List<string> { "Ada", "Grace" }
    int count = names.length()
    print(count)
}
```

```text
error: expected `int`, found `nint`
  --> example.sg:3:17
 3 |     int count = names.length()
   |                 ^^^^^^^^^^^^^^
  hint: convert with `(int)names.length()`; a value that does not fit panics, or is cut to size in a release build
```

`var count = names.length()` keeps it an `nint`.

### An `int` and a `uint`

```sg error
void main() {
    int a = 5
    uint b = 7
    print(a + b)
}
```

```text
error: `int` and `uint` do not mix: neither holds every value of the other
  --> example.sg:4:11
 4 |     print(a + b)
   |           ^^^^^
  hint: `long` holds both: `(long)b`
```

### A number as a condition

```sg error
void main() {
    int count = 3
    if count {
        print(count)
    }
}
```

```text
error: expected `bool`, found `int`
  --> example.sg:3:8
 3 |     if count {
   |        ^^^^^
  hint: a number is not a condition; compare it: `count != 0`
```

### A shift past 31

```sg error
void main() {
    int bit = 1 << 32
    print(bit)
}
```

```text
error: a shift by 32 is out of range: `int` has 32 bits
  --> example.sg:2:20
 2 |     int bit = 1 << 32
   |                    ^^
  hint: the count goes from 0 to 31
```

`long bit = 1 << 32` shifts a `long`.

### A conversion that cannot fail

```sg error
void main() {
    int n = 5
    print(n.tryToLong() ?? 0)
}
```

```text
error: every `int` fits in `long`: there is nothing to convert
  --> example.sg:3:13
 3 |     print(n.tryToLong() ?? 0)
   |             ^^^^^^^^^
  hint: it becomes one on its own: `long x = value`
```
