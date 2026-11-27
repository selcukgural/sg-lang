---
name: float
kind: type
summary: A fraction of 32 bits (IEEE 754), about 7 digits; half the size of a double, for large arrays and for C.
see: double, int, short, extern
learn: ../docs/learn/book/02-values.md#float-and-double
library: ../docs/library/numbers.md
---

## Forms

| Form | What it is |
|---|---|
| `float x = 0.1` | a fraction written in the program, a `float` where the place says so; there is no suffix |
| `float n = 3` | a whole number where a `float` goes, if a `float` holds it exactly |
| `(float)d`, `(float)n` | a `double`, an `int` or a `long` as a `float`, rounded |
| `+ - * / %` | the hardware's arithmetic in 32 bits: it never panics, and past the largest gives `Infinity` |
| `float.MaxValue`, `float.NaN`, `float.Infinity`, `float.Pi` | constants, as a `float` holds them |
| `x.toBits()`, `float.fromBits(bits)` | the 32 bits as a `uint`, and back |
| `{x:F2}` | printed with two digits after the point |
| `float.parse(text)` | the number a text writes, as a `Result<float, NumberError>` |

## What it means

A `float` is a floating-point number of 32 bits, as IEEE 754 sets it out: about 7 digits, up to about 3.4e38
(`float.MaxValue`). It takes half the room of a [`double`](double.md), which is what a list of a million values,
a file format or a function of C may ask for. Where none of those asks, a `double` is the better choice.

A number with a point or an exponent takes the type it is used as, so `float x = 0.1` is a `float`, and in a `var`
it is a `double`. No letter at its end makes one a `float`: `0.5f` is refused. A whole number goes where a `float`
goes only if a `float` holds it exactly, which every whole number up to 2^24 (16 777 216) is. `(float)d` rounds
a `double` to the nearest `float`, a value past the largest to `Infinity`, and never stops the program.

A `float` becomes a `double` on its own, exactly; the other way is `(float)x`. `sbyte`, `byte`, `short` and
`ushort` become a `float` on their own; an `int`, a `uint`, a `long` and the others need `(float)n`, which rounds
past 2^24. A `float` beside a `double` makes the operation a `double`'s; a number written beside a `float` is a
`float`. A `float` is computed as a `float`: each operation is rounded to 32 bits on its own, in the interpreter
and in compiled code alike, so both give the same digits.

As for a `double`, the arithmetic and `==` are the hardware's: a division by zero gives `Infinity` or `NaN`, and
`NaN` equals nothing. `(int)x` drops the fraction and stops the program for a value that does not fit, `NaN`
included, in the interpreter, in tests and in a debug build; a release build gives the nearest value, and 0 for
`NaN`.

A `float` is printed with the fewest digits that read back as the same `float`: `0.1`, `3.4028235e+38`. `{x:F2}`
rounds the exact value. A function of C takes and gives a `float` as it is, but after the `...` of a function
like `printf`, where C would make it a `double` without a word, a `float` is refused, and the hint is
`(double)x`. The methods are in [Numbers](../docs/library/numbers.md#math-on-float-and-double); in compiled code
a `float` is C's `float`.

## Examples

### Written and printed

```sg
void main() {
    float small = 0.1
    var wide = 0.1
    float rounded = (float)wide
    float count = 3
    print($"{small} {wide} {rounded} {count}")
    float third = 1.0 / 3.0
    print($"{third} {(double)third}")
    print($"{float.MaxValue} {float.Pi} {float.E}")
    print($"{third:F3} {float.Pi:F10}")
}
```

```text
0.1 0.1 0.1 3
0.33333334 0.3333333432674408
3.4028235e+38 3.1415927 2.7182817
0.333 3.1415927410
```

`wide` is a `double`, since nothing says otherwise. A `float` is printed with the digits a `float` needs, so
`third` shows eight; as a `double` the same value shows what it holds. `F10` writes more digits than the `float`
has, and they come from its exact value, not from π.

### Seven digits

```sg
void main() {
    float fsum = 0
    double dsum = 0
    for _ in 0..1_000_000 {
        fsum += 0.1
        dsum += 0.1
    }
    print($"{fsum} {dsum}")
    float count = 16_777_216
    count += 1
    print(count)
    print($"{(float)16_777_217.0} {(float)16_777_218.0}")
    float price = 19.99
    print($"{price} {(double)price} {price:F2}")
}
```

```text
100958.34 100000.00000133288
16777216
16777216 16777218
19.99 19.989999771118164 19.99
```

A million additions of 0.1 in a `float` miss by almost 1 %, since each sum is rounded to 7 digits; a `double`
misses in the eleventh. Past 2^24 a `float` has no room for the ones: 16 777 216 plus 1 is 16 777 216 again.

### A `float` meets a `double` and an integer

```sg
void main() {
    float f = 0.1
    double d = 0.1
    print($"{f == d} {f == (float)d} {f + d}")
    var scaled = f * 2.5
    float back = scaled
    print(back)
    short width = 640
    float wide = width
    int height = 480
    var ratio = (float)height / wide
    print($"{wide * 0.5} {ratio}")
}
```

```text
false true 0.20000000149011612
0.25
320 0.75
```

`f == d` makes `f` a `double` first, and the `double` nearest 0.1 is not the `float` nearest it; `(float)d`
compares as `float`s. `f * 2.5` stays a `float`, since the number takes the type of `f`. A `short` becomes a
`float` on its own; an `int` is written `(float)height`.

### Infinity and `NaN`

```sg
void main() {
    float top = float.MaxValue
    print($"{top * 2} {-top * 2}")
    float big = 1e30
    print($"{big * big} {(double)big * big}")
    float zero = 0
    var none = zero / zero
    print($"{none} {none == none} {none.isNaN()}")
    print($"{(float)1e300} {(float)1e-300}")
    var values = List<float> { 2.5, none, -1, float.Infinity }
    values.sort()
    print(values)
}
```

```text
Infinity -Infinity
Infinity 1.0000000300949327e+60
NaN false true
Infinity 0
[NaN, -1, 2.5, Infinity]
```

Nothing stops the program. `big * big` passes the largest `float` and is `Infinity`; done as `double`s the same
product fits. `(float)` of a `double` too large or too small gives `Infinity` or 0. `sort` puts `NaN` first.

### The bits of a `float`

```sg
void main() {
    float one = 1
    print($"{one.toBits():x8} {(-one).toBits():x8} {float.fromBits(0x40490fdb)}")
    float zero = 0
    var minus = -zero
    print($"{minus} {minus == zero} {minus.toBits():x8}")
    var smallest = float.fromBits(1)
    print($"{smallest} {smallest > 0} {float.fromBits(0x7f7fffff)}")
    var tagged = float.fromBits(0x7fc0_0042)
    print($"{tagged.isNaN()} {tagged.toBits() & 0xff:x2}")
    print($"{one.nextUp() - one} {one.nextUp().toBits() - one.toBits()}")
}
```

```text
3f800000 bf800000 3.1415927
-0 true 80000000
1e-45 true 3.4028235e+38
true 42
1.1920929e-7 1
```

`toBits()` gives the 32 bits as a `uint` and `float.fromBits` reads them back; every pattern is a `float`.
`-0` equals 0 and has a bit of its own. The smallest `float` above 0 is about 1.4e-45, printed with the one digit
that reads back. A `NaN` keeps the bits it is given. The step after 1 is one bit, about 1.2e-7.

### A `float` to C

```sg
extern "m" {
    float sqrtf(float x)
    float hypotf(float x, float y)
}

extern {
    int printf(read CString format, ...)
}

void main() {
    float side = 2
    float root = unsafe sqrtf(side)
    print($"{root} {root == side.sqrt()} {unsafe hypotf(3, 4)}")
    var format = CString.from("%.9f\n") ?? panic("a zero byte inside")
    _ = unsafe printf(read format, (double)root)
}
```

```text
1.4142135 true 5
1.414213538
```

`sqrtf` of C's `<math.h>` takes and gives a `float`; `sqrt` on a `float` gives the same value. After the `...` of
`printf` the value is written `(double)root`, which is what C reads there.

### Read from text

```sg
void show(read string text) {
    match float.parse(text) {
        Ok(v) => print($"{text} -> {v}")
        Error(e) => print($"{text} -> {e}")
    }
}

void main() {
    show("0.1")
    show("3.4028235e38")
    show("1e39")
    show("16777217")
    show("1e-50")
    show("1,5")
}
```

```text
0.1 -> 0.1
3.4028235e38 -> 3.4028235e+38
1e39 -> Infinity
16777217 -> 16777216
1e-50 -> 0
1,5 -> Invalid(1)
```

The value is the nearest `float`: past the largest it is `Infinity`, below the smallest 0. A whole number past
2^24 is rounded too, as a number written in the program is not.

## What stops the program

### A `float` that does not fit the integer

```sg panic
int pixels(float size) {
    return (int)size
}

void main() {
    print(pixels(640.5))
    print(pixels(3e9))
}
```

```text
640
panic: value 3000000000 does not fit in int
  --> example.sg:2:12
 2 |     return (int)size
   |            ^^^^^^^^^
  hint: `tryToInt()` gives `null` when the value does not fit
```

In a release build `(int)3e9` is 2147483647, the nearest `int`; `size.saturatingToInt()` gives it in every build.

### `NaN` as an integer

```sg panic
int percent(float done, float total) {
    return (int)(done / total * 100)
}

void main() {
    print(percent(0, 0))
}
```

```text
panic: value NaN does not fit in int
  --> example.sg:2:12
 2 |     return (int)(done / total * 100)
   |            ^^^^^^^^^^^^^^^^^^^^^^^^^
  hint: `tryToInt()` gives `null` when the value does not fit
```

A release build gives 0.

## What the compiler refuses

### A `double` where a `float` goes

```sg error
float half(float x) {
    return x / 2
}

void main() {
    double size = 3.5
    print(half(size))
}
```

```text
error: expected `float`, found `double`
  --> example.sg:7:16
 7 |     print(half(size))
   |                ^^^^
  hint: convert with `(float)`, which rounds
```

### An `int` and a `float`

```sg error
void main() {
    float scale = 0.5
    int width = 640
    print(width * scale)
}
```

```text
error: `int` and `float` do not mix: a `float` does not hold every `int` exactly
  --> example.sg:4:11
 4 |     print(width * scale)
   |           ^^^^^
  hint: convert it: `(float)width`, which rounds past 2^24
```

### A whole number a `float` does not hold

```sg error
void main() {
    float count = 16_777_217
    print(count)
}
```

```text
error: 16777217 is not held exactly by a `float`
  a `float` holds every whole number up to 2^24
  --> example.sg:2:19
 2 |     float count = 16_777_217
   |                   ^^^^^^^^^^
```

`16_777_217.0`, a fraction, rounds to the nearest `float`, as does `(float)16_777_217`.

### A number past the largest

```sg error
void main() {
    float big = 1e39
    print(big)
}
```

```text
error: 1e39 does not fit in float
  --> example.sg:2:17
 2 |     float big = 1e39
   |                 ^^^^
```

A number that would be 0 in a `float`, as `1e-46`, is refused too; `double` holds both.

### A suffix

```sg error
void main() {
    var half = 0.5f
    print(half)
}
```

```text
error: a number has no `f` at its end; it takes the type it is used as
  --> example.sg:2:16
 2 |     var half = 0.5f
   |                ^^^^
  hint: `float x = 0.5`
```

### A `float` after `...`

```sg error
extern {
    int printf(read CString format, ...)
}

void main() {
    var format = CString.from("%f\n") ?? panic("a zero byte inside")
    float x = 1.5
    _ = unsafe printf(read format, x)
}
```

```text
error: a `float` does not go to C after `...`
  C reads it there as a `double`, made wider without a word
  --> example.sg:8:36
 8 |     _ = unsafe printf(read format, x)
   |                                    ^
  hint: `(double)x`
```

### Bits of a `float`

```sg error
void main() {
    float x = 1.5
    print(x & 1)
}
```

```text
error: operator `&` is not defined for `float` and `float`
  the bit operators work on integers
  --> example.sg:3:11
 3 |     print(x & 1)
   |           ^^^^^
  hint: `x.toBits()` gives its bits as a `uint`
```
