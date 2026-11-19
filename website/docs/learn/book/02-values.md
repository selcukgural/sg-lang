---
title: Values
description: Variables, the number types and their sizes, conversions, overflow, characters, constants and bits.
sidebar_position: 2
---

# Values

SG is a typed language: every variable has one type, fixed when it is declared. The number types are the machine's own, each
with a fixed size, so a program says exactly how much memory a value takes and what happens at its edges.

## Variables

A variable is declared with `var` and a value, or with a type and a value:

```sg
void main() {
    var count = 3
    int limit = 10
    long total = 0
    count += 1
    total += count * limit
    print($"{count} {limit} {total}")
}
```

It prints:

```text
4 10 40
```

`var` takes the type of its value: `count` is an `int`, because a whole number that fits in an `int` is one.
Every variable can be changed, and every variable has a value from the start; `int limit` alone, with no value,
is an error. A variable keeps its type, so an `int` cannot later hold a fraction:

```sg error
void main() {
    var count = 3
    count = 1.5
}
```

```text
error: expected `int`, found `double`
  --> example.sg:3:13
 3 |     count = 1.5
   |             ^^^
  hint: convert with `(int)`; it drops the fraction, and panics if the value does not fit
```

A name means one thing in a function: a block inside it cannot declare a variable with the name of one
outside it. Blocks side by side can reuse a name.

A variable that is never read is an error too, since it is either a leftover or a mistake:

```sg error
void main() {
    var count = 3
    int limit = 10
    print(limit)
}
```

```text
error: `count` is never used
  --> example.sg:2:9
 2 |     var count = 3
   |         ^^^^^
  hint: take the line away; to run what it is given and drop the value on purpose, write `_ = ...`
```

So is one that is given values but never read: `count = 4` does not count as a use.

## The integer types

| Type | Size | From | To |
|---|---|---|---|
| `sbyte` | 8 bits | -128 | 127 |
| `byte` | 8 bits | 0 | 255 |
| `short` | 16 bits | -32 768 | 32 767 |
| `ushort` | 16 bits | 0 | 65 535 |
| `int` | 32 bits | -2 147 483 648 | 2 147 483 647 |
| `uint` | 32 bits | 0 | 4 294 967 295 |
| `long` | 64 bits | about -9.2 × 10¹⁸ | about 9.2 × 10¹⁸ |
| `ulong` | 64 bits | 0 | about 1.8 × 10¹⁹ |
| `nint`, `nuint` | the size of an address | | |

`int` is the usual choice. `nint` is as wide as an address on the machine (64 bits on today's computers), and
it is what lengths and positions are: `text.length()` and `list.length()` give an `nint`. Each type knows its
ends:

```sg
void main() {
    print($"{byte.MaxValue} {sbyte.MinValue}")
    print($"{int.MinValue} {int.MaxValue}")
    print(ulong.MaxValue)
}
```

It prints:

```text
255 -128
-2147483648 2147483647
18446744073709551615
```

## Numbers as written

A number can be written in decimal, in hex with `0x` or in binary with `0b`, and `_` may stand between digits
to group them. A number that does not fit its type is refused when the program is compiled.

```sg
void main() {
    int million = 1_000_000
    uint color = 0xff_80_00
    byte flags = 0b0000_0101
    print($"{million} {color} {flags}")
    print($"{color:x6} {flags:b8}")
}
```

It prints:

```text
1000000 16744448 5
ff8000 00000101
```

`{color:x6}` writes the value in hex with at least six digits, and `{flags:b8}` in binary; [Text](./03-text.md)
lists the formats.

## Conversions

An integer becomes a wider type on its own, but only where the wider type holds every value of the narrower
one on every machine: `int` to `long` or `nint`, `byte` to `ushort` or `int`, `uint` to `long`. A signed type
never becomes an unsigned one on its own, since a negative number has no place there. Going the other way is
written as a cast, `(int)x`, and the cast *panics* if the value does not fit: it never silently changes a
number.

```sg
long square(long x) {
    return x * x
}

void main() {
    int side = 50000
    print(square(side))
    long big = 4000000000
    int quarter = (int)(big / 4)
    print(quarter)
    var name = "language"
    int length = (int)name.length()
    print(length)
}
```

It prints:

```text
2500000000
1000000000
8
```

`side` becomes a `long` on its way into `square`, so `50000 * 50000` does not overflow. Mixing a signed and an
unsigned type of the same size is an error, because neither holds every value of the other, and the hint names
a type that holds both:

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

When a value might not fit and the program should decide what to do, the methods of each integer say it in
their names. `tryToInt()` gives `null` where the cast would panic (`??` gives a value for that case),
`wrappingToInt()` keeps the bits that fit, and `saturatingToInt()` gives the nearest end of the type. The same
methods exist for every target type: `tryToByte()`, `wrappingToUint()`, and so on.

```sg
void main() {
    long small = 1234
    long big = 5000000000
    print(small.tryToInt() ?? -1)
    print(big.tryToInt() ?? -1)
    print(big.wrappingToInt())
    print(big.saturatingToInt())
    int level = 300
    print(level.saturatingToByte())
}
```

It prints:

```text
1234
-1
705032704
2147483647
255
```

## Overflow

Arithmetic that goes past the end of a type is an *overflow*. In the interpreter, in tests and in a debug build
it is a panic, which names the values and the methods that do not panic:

```sg panic
int add(int a, int b) {
    return a + b
}

void main() {
    print(add(int.MaxValue, 1))
}
```

```text
panic: overflow: 2147483647 + 1 does not fit in int
  --> example.sg:2:12
 2 |     return a + b
   |            ^^^^^
  hint: `tryAdd` gives `null` instead, and `wrappingAdd` wraps around
```

A release build (`sg build --release`) wraps instead, as C does, and the same program prints `-2147483648`.
The check costs a little in every operation, and an overflow cannot make memory unsafe in SG (every index is
still checked), so the fast build drops it. This is Rust's rule too.

When the program wants one behaviour in every build, it says so with a method: `wrappingAdd` always wraps,
`tryAdd` always checks and gives `null`, `saturatingAdd` stops at the end of the type. They exist for
subtraction and multiplication too (`wrappingSubtract`, `tryMultiply`, ...), and `tryDivide` gives `null` for
a division by zero, which is a panic in every build.

```sg
void main() {
    int max = int.MaxValue
    print(max.wrappingAdd(1))
    print(max.tryAdd(1) ?? 0)
    print(max.saturatingAdd(1))
    byte level = 250
    print(level.wrappingAdd(10))
    print(level.saturatingAdd(10))
    print(10.tryDivide(0) ?? -1)
}
```

It prints:

```text
-2147483648
0
2147483647
4
255
-1
```

Integer division drops the fraction toward zero, and `%` takes the sign of the left side: `7 / 2` is `3`,
`-7 / 2` is `-3`, `-7 % 2` is `-1`.

## `float` and `double`

`double` is a 64-bit floating-point number and `float` a 32-bit one. A number with a point or an exponent
(`1.5`, `1e9`) takes the type it is used as, and is a `double` in a `var`. An integer becomes a `double` on its
own only where the `double` holds it exactly, so `int` does and `long` needs `(double)`. The arithmetic is the
hardware's and never panics: a division by zero gives infinity.

```sg
void main() {
    int count = 3
    double half = count / 2.0
    float small = 0.1
    double sum = 0.1 + 0.2
    print($"{half} {small} {sum}")
    print(2.0.sqrt())
    print($"{double.Pi:F3}")
    print(1.0 / 0.0)
    print((int)3.99)
}
```

It prints:

```text
1.5 0.1 0.30000000000000004
1.4142135623730951
3.142
Infinity
3
```

A number is printed with the fewest digits that read back as the same number. A cast to an integer drops the
fraction and panics if the result does not fit, as any cast does.

## `bool` and `char`

A `bool` is `true` or `false`, and conditions must be one: `if count` is an error whose hint is
`count != 0`. `&&` and `||` stop as soon as the answer is known, and `!` turns a `bool` around.

A `char` is one Unicode character, written in single quotes: `'a'`, `'é'`, `'\n'`, `'\u{1F600}'`. `(int)c`
gives its code and `(char)n` the character of a code; `n.tryToChar()` gives `null` for a number that is no
character. A character of ASCII is also a `byte` where an integer goes, which is handy for reading bytes.

```sg
void main() {
    bool ready = 3 > 2 && !(1 == 2)
    print(ready)
    char letter = 'é'
    print($"{letter} {(int)letter} {(char)65}")
    byte plus = '+'
    print(plus)
    print('a' < 'b')
    print('7'.isAsciiDigit())
}
```

It prints:

```text
true
é 233 A
43
true
true
```

## Consts

A `const` is a value fixed when the program is compiled. It is written at the top of a file, or in a block,
where it is seen to the end of the block. By custom its name is in PascalCase. It can be computed from numbers, other
consts and operators:

```sg
const int Size = 64
const int Half = Size / 2
const string Name = "buffer"

void main() {
    const int Quarter = Half / 2
    print($"{Name}: {Size} {Half} {Quarter}")
}
```

It prints:

```text
buffer: 64 32 16
```

An integer const can give the length of an array, and an overflow while computing a const is an error when
the program is compiled, not when it runs. A const cannot be assigned:

```sg error
const int Size = 64

void main() {
    Size = 32
}
```

```text
error: `Size` is a const: what it holds is fixed when the program is compiled
  --> example.sg:4:5
 4 |     Size = 32
   |     ^^^^
  hint: change a copy of it: `var copy = Size`
```

## Bits

On integers, `&` (and), `|` (or), `^` (exclusive or), `~` (not), `<<` and `>>` (shifts) work on the bits, and
each has an assignment form: `&=`, `|=`, `^=`, `<<=`, `>>=`.

```sg
void main() {
    uint flags = 0b1010_0101
    print($"{flags & 0x0f:b8}")
    print($"{flags | 0x100:b12}")
    print($"{flags ^ 0xff:b8}")
    print($"{flags << 4:x}")
    print(flags >> 2)
    print(flags & 1 == 1)
    flags &= ~1
    print($"{flags:b8}")
    print(flags.countOnes())
    flags |= 1 << 8
    print(flags)
}
```

It prints:

```text
00000101
000110100101
01011010
a50
41
true
10100100
3
420
```

The bit operators bind tighter than a comparison, unlike C, so `flags & 1 == 1` tests the masked value. A
shift count must be below the number of bits of the type: one that is known when the program is compiled is
checked there (`x << 32` on an `int` is an error), and one found while running follows the rule of overflow.
Integers also have `countOnes`, `leadingZeros`, `trailingZeros`, `rotateLeft`, `reverseBytes` and their kin.
