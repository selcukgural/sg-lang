---
title: Numbers
description: The integer and fractional types, their constants, conversions, arithmetic that cannot panic, bits, math and parsing.
sidebar_position: 2
---

# Numbers

SG has ten integer types and two fractional ones, all built into the language. In the signatures below `T`
is the type of the number the method is called on: `x.tryAdd(1)` on an `int` takes an `int` and gives an
`int?`.

| Type | Size | Type | Size |
|---|---|---|---|
| `sbyte`, `byte` | 8 bits | `long`, `ulong` | 64 bits |
| `short`, `ushort` | 16 bits | `nint`, `nuint` | the size of an address; lengths and places are `nint` |
| `int`, `uint` | 32 bits | `float`, `double` | 32 and 64 bits, IEEE 754 |

`CLong` and `CULong` are C's `long` and `unsigned long`, for calling C; they are compared and printed but have
no arithmetic.

## The rules every method follows

An integer becomes a wider type on its own only when that type holds every one of its values (`int` to `long`,
`byte` to `ushort`, never a signed type to an unsigned one); anything else is written as a cast, `(int)x`,
which panics when the value does not fit and, like an overflow, is cut to size in a release build. Overflow of
`+`, `-` and `*` panics in the interpreter, in tests and in a debug build, and wraps in a release build, as in
Rust. The methods below say what happens everywhere, whatever the build: `try...` gives `null`, `wrapping...`
wraps, `saturating...` stops at the end of the type.

```sg error
void main() {
    int a = 1
    uint b = 2
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

A panic of an overflow names the method that would not panic:
`panic: overflow: 2147483647 + 1 does not fit in int`, with the hint `` `tryAdd` gives `null` instead, and
`wrappingAdd` wraps around ``. A value that is known when the program is compiled and does not fit is a
compile error instead: `(byte)300` gives `300 does not fit in byte`.

`float` and `double` never panic: their arithmetic is the hardware's, and nothing is fused. A number is
printed with the fewest digits that read back as it.

## Constants

| Constant | On |
|---|---|
| `T.MinValue`, `T.MaxValue` | every number type |
| `T.NaN`, `T.Infinity`, `T.Pi`, `T.E` | `float` and `double` |

A constant is a value of its type: `uint u = int.MaxValue` needs `(uint)`.

## Conversions

| Signature | What it gives |
|---|---|
| `(U)x` | `x` as the type `U`, a fraction cut toward zero; when it does not fit, a panic, and in a release build the low bits (from an integer) or the nearest value, 0 for `NaN` (from a fraction); `(char)n` panics in every build |
| `U? tryToU()` | `tryToInt()`, `tryToByte()` and the others: `null` where the cast would panic |
| `U wrappingToU()` | the bits that fit: `wrappingToInt()`, on integers only |
| `U saturatingToU()` | the nearest end of `U` where the value does not fit; on fractional numbers too |
| `char? tryToChar()` | the character of a code, or `null` |

A `tryTo...` or `wrappingTo...` that can never fail (`n.tryToLong()` on an `int`) is a compile error whose hint
is to let the value widen on its own.

```sg
void main() {
    long big = 5000000000
    print(big.tryToInt() ?? -1)
    print(big.wrappingToInt())
    print(big.saturatingToInt())
    print((-5).saturatingToUint())
    print((int)3.7)
    print((int)-3.7)
    print(3.7.tryToInt() ?? 0)
    print(1e20.saturatingToLong())
}
```

It prints:

```text
-1
705032704
2147483647
0
3
-3
3
9223372036854775807
```

## Arithmetic

On every integer type:

| Signature | What it gives |
|---|---|
| `T? tryAdd(T other)`, `trySubtract`, `tryMultiply`, `tryDivide` | `null` where the operator would panic (overflow, or division by 0) |
| `T wrappingAdd(T other)`, `wrappingSubtract`, `wrappingMultiply` | the result wrapped around |
| `T saturatingAdd(T other)`, `saturatingSubtract`, `saturatingMultiply` | the nearest end of the type |
| `T min(T other)`, `T max(T other)`, `T clamp(T low, T high)` | the smaller, the larger, the value kept inside `low..=high` |
| `int compare(T other)`, `bool equals(T other)` | what `<` and `==` are short for |

On the signed types:

| Signature | What it gives |
|---|---|
| `T abs()` | the size; `int.MinValue.abs()` overflows, as `-x` does |
| `T? tryAbs()`, `T wrappingAbs()` | `null`, or the value wrapped, for the most negative number |
| `U unsignedAbs()` | the size as the unsigned type of the same width (a `uint` for an `int`), which always fits |
| `T? tryNegate()`, `T wrappingNegate()` | `-x` that cannot panic |

`/` cuts toward zero and `%` takes the sign of the left side, as in C.

```sg
void main() {
    int x = int.MaxValue
    print(x.tryAdd(1) ?? 0)
    print(x.wrappingAdd(1))
    print(x.saturatingAdd(1))
    print(10.tryDivide(0) ?? -1)
    print(15.clamp(0, 10))
    print(int.MinValue.unsignedAbs())
    print(int.MinValue.tryAbs() ?? 0)
    print(7.compare(9))
    print($"{-7 / 2} {-7 % 3}")
}
```

It prints:

```text
0
-2147483648
2147483647
-1
10
2147483648
0
-1
-3 -1
```

## Bits

On every integer type, as C23's `<stdbit.h>` has them; a count is an `int`.

| Signature | What it gives |
|---|---|
| `int countOnes()`, `int countZeros()` | how many bits are 1, or 0 |
| `int leadingZeros()`, `int trailingZeros()`, `int leadingOnes()`, `int trailingOnes()` | the bits from the top, or from the bottom, up to the first that differs |
| `T rotateLeft(int n)`, `T rotateRight(int n)` | the bits turned around by `n` |
| `T reverseBytes()`, `T reverseBits()` | the bytes, or the bits, in the other order |

On the unsigned types only:

| Signature | What it gives |
|---|---|
| `bool isPowerOfTwo()` | whether exactly one bit is 1 |
| `T nextPowerOfTwo()` | the smallest power of two not below the value; one past the type follows the rule of overflow |
| `int log2()` | the place of the highest 1; a panic for 0 in every build |

The operators `& | ^ ~ << >>` bind tighter than a comparison, so `flags & mask == 0` tests the masked value.
A shift count below 0, or not below the bits of the type, follows the rule of overflow.

```sg
void main() {
    uint flags = 0b1011_0000
    print($"{flags.countOnes()} {flags.leadingZeros()} {flags.trailingZeros()}")
    print($"{flags.rotateLeft(28):x8} {flags.rotateRight(4):x8}")
    uint word = 0x12345678
    print($"{word.reverseBytes():x8} {word.reverseBits():x8}")
    uint size = 100
    print($"{size.isPowerOfTwo()} {size.nextPowerOfTwo()} {size.log2()}")
}
```

It prints:

```text
3 24 4
0000000b 0000000b
78563412 1e6a2c48
false 128 6
```

## The bits of `float` and `double`

A fractional number as the bits that hold it, and back, as Rust's `to_bits` and `from_bits` and C#'s
`BitConverter.DoubleToUInt64Bits` (choice 1891). Every pattern of bits is a `float` or a `double`, so neither
needs `unsafe`, and a `NaN` keeps the bits it has: a value can carry a tag in them (NaN boxing).

| Signature | What it gives |
|---|---|
| `ulong toBits()` on a `double`, `uint toBits()` on a `float` | the bits of the number, as they lie in memory |
| `double double.fromBits(ulong bits)`, `float float.fromBits(uint bits)` | the number those bits are |

```sg
void main() {
    double x = 1.0
    print($"{x.toBits():x16} {(-2.0).toBits():x16} {double.fromBits(0x400921fb54442d18)}")
    float f = 0.5
    print($"{f.toBits():x8} {float.fromBits(0x3f800000)}")
    // A quiet NaN with a payload of its own, kept as it is.
    var tagged = double.fromBits(0x7ffc000000000042)
    print($"{tagged.isNaN()} {tagged.toBits() & 0xff:x2}")
}
```

It prints:

```text
3ff0000000000000 c000000000000000 3.141592653589793
3f000000 1
true 42
```

## Math on `float` and `double`

Each is a method of the value and gives the same type, from C's `<math.h>`:

- Rounding: `abs`, `floor`, `ceil`, `round` (half away from zero), `roundToEven`, `truncate`, `fraction`.
- Powers and logarithms: `sqrt`, `cbrt`, `pow(y)`, `hypot(y)`, `exp`, `exp2`, `expMinusOne`, `log`, `log2`,
  `log10`, `logOnePlus`.
- Angles, in radians: `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2(x)`, `sinh`, `cosh`, `tanh`, `asinh`,
  `acosh`, `atanh`.
- Special functions: `erf`, `erfc`, `gamma`, `logGamma`.
- Exact steps: `fusedMultiplyAdd(a, b)` (`x * a + b` rounded once), `ieeeRemainder(y)` (beside `%`, which is
  C's `fmod`), `copySign(y)`, `scaleByPowerOfTwo(n)`, `nextUp`, `nextDown`, and `exponent()`, an `int?` that
  is `null` for 0, `NaN` and the infinities.
- Tests: `isNaN`, `isInfinite`, `isFinite`, `isSignNegative`.
- `min(y)`, `max(y)`, `clamp(low, high)`, and the conversions above.

`==` and `<` are the hardware's, so `double.NaN == double.NaN` is `false`; `equals`, `compare`, `sort` and `==`
on a struct use a total order, in which `NaN` equals itself.

```sg
void main() {
    double x = 2.5
    print($"{x.floor()} {x.ceil()} {x.round()} {x.roundToEven()} {x.fraction()}")
    print($"{2.0.sqrt()} {2.0.pow(10.0)} {3.0.hypot(4.0)} {100.0.log10()}")
    print($"{1.0.atan2(1.0)} {2.0.fusedMultiplyAdd(3.0, 1.0)} {5.0.ieeeRemainder(3.0)} {5.0 % 3.0}")
    print($"{8.0.exponent() ?? 0} {1.0.nextUp()} {5.0.gamma()}")
    print($"{double.NaN == double.NaN} {double.NaN.equals(double.NaN)}")
    print($"{0.1 + 0.2} {1e21}")
}
```

It prints:

```text
2 3 3 2 0.5
1.4142135623730951 1024 5 2
0.7853981633974483 7 -1 2
3 1.0000000000000002 24
false true
0.30000000000000004 1e+21
```

## Numbers from text

| Signature | What it gives |
|---|---|
| `Result<T, NumberError> T.parse(read string text)` | the number the text writes, on every number type |
| `Result<T, NumberError> T.parseHex(read string text)` | hex digits (`ff`, `FF`, no `0x`), on every integer type |

`NumberError` is `Invalid(nint position)`, the place where the text stops being a number, or `OutOfRange`, a
whole number the type does not hold. `parse` reads a sign and digits, and for a fractional type a point and an
exponent too; nothing else (no spaces, no `_`, no `0x`). What `print` writes, `parse` reads back. A
fractional number past the type is `Infinity`, as in C.

```sg
void show(read string text) {
    match int.parse(text) {
        Ok(n) => print($"{text}: {n}")
        Error(e) => print($"{text}: {e}")
    }
}

void main() {
    show("-17")
    show("4x2")
    show("99999999999")
    print(double.parse("2.5e3") ?? 0.0)
    print(uint.parseHex("DEADbeef") ?? 0)
    match ulong.parse("") {
        Ok(n) => print(n)
        Error(Invalid(at)) => print($"invalid at {at}")
        Error(OutOfRange) => print("out of range")
    }
}
```

It prints:

```text
-17: -17
4x2: Invalid(1)
99999999999: OutOfRange
2500
3735928559
invalid at 0
```

## Numbers as bytes

On `short`, `ushort`, `int`, `uint`, `long` and `ulong`:

| Signature | What it gives |
|---|---|
| `T T.fromLittleEndian(read byte[] bytes)` | the number written by exactly as many bytes as the type takes, the lowest first |
| `T T.fromBigEndian(read byte[] bytes)` | the same, the highest first |
| `byte[N] toLittleEndian()`, `byte[N] toBigEndian()` | the bytes of the number |

A window whose width the program shows and that is wrong is a compile error; one whose width depends on data
panics with both numbers.

```sg
void main() {
    var data = List<byte> { 0x34, 0x12, 0x78, 0x56 }
    print(ushort.fromLittleEndian(data[0..2]))
    print($"{uint.fromBigEndian(data[0..4]):x8}")
    int n = 0x01020304
    print(n.toLittleEndian())
    print(n.toBigEndian())
}
```

It prints:

```text
4660
34127856
[4, 3, 2, 1]
[1, 2, 3, 4]
```

```sg error
void main() {
    var data = List<byte> { 1, 2, 3, 4 }
    print(ushort.fromLittleEndian(data[0..4]))
}
```

```text
error: `ushort.fromLittleEndian` reads 2 bytes, and this window has 4
```

Writing a number in hex or with a width is on the page [Text](./string.md#writing-values-into-text).
