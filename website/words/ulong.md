---
name: ulong
kind: type
summary: An integer of 64 bits without a sign, 0 to 18 446 744 073 709 551 615: for hashes, random numbers and sets of 64 bits.
see: long, uint, nuint, double
learn: ../docs/learn/book/02-values.md#the-integer-types
library: ../docs/library/numbers.md
---

## Forms

| Form | What it is |
|---|---|
| `ulong` | an integer of 64 bits, from 0 to 18 446 744 073 709 551 615 |
| `ulong seed = 0x9e37_79b9_7f4a_7c15` | a number written in the program, a `ulong` where the place says so |
| `ulong.MinValue`, `ulong.MaxValue` | the two ends |
| `a & b`, `a \| b`, `a ^ b`, `~a`, `a << n`, `a >> n` | the bits; `>>` brings in zeros, and `n` goes from 0 to 63 |
| `x.wrappingAdd(y)`, `x.wrappingMultiply(y)` | arithmetic that wraps around in every build, as a hash wants |
| `(ulong)n`, `n.tryToUlong()`, `n.wrappingToUlong()` | a value of another number type, as a `ulong` |
| `(long)x`, `x.tryToLong()`, `(double)x` | a `ulong` as a `long`, or as a fraction, rounded |
| `ulong.parse(text)`, `ulong.parseHex(text)` | the number a text writes, as a `Result<ulong, NumberError>` |

## What it means

A `ulong` is an integer of 64 bits without a sign, on every machine. It is the type of a 64-bit hash, of the
state of a generator of random numbers, and of a set of 64 flags, where every bit counts and none is a sign.

A number written in the program is a `ulong` only where the place says so, or where the other side of an
operator is one. A number past every `long`, as the offset of FNV, needs the type written: `ulong basis =
14695981039346656037`. With no type where it goes, a number is never unsigned.

`byte`, `ushort` and `uint` become a `ulong` on their own. A signed type never does, so an `int` or a `long`
beside a `ulong` is a compile error whose hint is the cast. A `long` and a `ulong` do not mix, since neither holds
every value of the other. A `ulong` goes to a `long` with `(long)x`, which stops the program for a value past
`long.MaxValue` (a release build keeps the 64 bits, read with a sign), or with `tryToLong()`,
`saturatingToLong()` or `wrappingToLong()`, the same in every build. It goes to a `double` only with `(double)x`,
which rounds past 2^53, and to an index with `(nint)x`.

Overflow, below 0 as past the top, stops the program in the interpreter, in tests and in a debug build, and
wraps in a release build. A hash means to wrap, so it says so with `wrappingMultiply`, which wraps in every
build. A panic writes a `ulong` in full. A shift count is from 0 to 63, and `>>` brings in zeros. The methods are in
[Numbers](../docs/library/numbers.md); in compiled code a `ulong` is C's `uint64_t`.

## Examples

### Numbers as written

```sg
void main() {
    ulong top = ulong.MaxValue
    ulong basis = 14695981039346656037
    ulong prime = 0x100_0000_01b3
    print($"{top} {basis} {prime}")
    print($"{top:x} {basis:X16} {prime:x}")
    ulong high = 1 << 63
    long sign = 1 << 63
    print($"{high} {sign}")
}
```

```text
18446744073709551615 14695981039346656037 1099511628211
ffffffffffffffff CBF29CE484222325 100000001b3
9223372036854775808 -9223372036854775808
```

`basis` is past every `long`, so it is written where a `ulong` goes. In `1 << 63` the shift takes the type of the
place: the top bit is a large number in a `ulong`, and the sign in a `long`.

### A hash: FNV-1a

```sg
ulong fnv1a(read byte[] data) {
    ulong hash = 0xcbf2_9ce4_8422_2325
    for b in data {
        hash ^= b
        hash = hash.wrappingMultiply(0x100_0000_01b3)
    }
    return hash
}

void main() {
    print($"{fnv1a("".asBytes()):x16}")
    print($"{fnv1a("a".asBytes()):x16}")
    var h = fnv1a("foobar".asBytes())
    print($"{h:x16} {h % 16}")
}
```

```text
cbf29ce484222325
af63dc4c8601ec8c
85944171f73967e8 8
```

Each `byte` widens to a `ulong` in `hash ^= b`. The product passes the top almost every time, and
`wrappingMultiply` keeps the low 64 bits, the same in every build; `hash *= ...` would stop the program at the
first byte.

### Random numbers: SplitMix64

```sg
struct SplitMix {
    ulong state
}

ulong next(ref SplitMix r) {
    r.state = r.state.wrappingAdd(0x9e37_79b9_7f4a_7c15)
    var z = r.state
    z = (z ^ z >> 30).wrappingMultiply(0xbf58_476d_1ce4_e5b9)
    z = (z ^ z >> 27).wrappingMultiply(0x94d0_49bb_1331_11eb)
    return z ^ z >> 31
}

void main() {
    var r = SplitMix { state = 0 }
    print($"{next(ref r):x16}")
    print($"{next(ref r):x16}")
    var roll = next(ref r) % 6 + 1
    print(roll)
    double unit = (double)(next(ref r) >> 11) / (double)((ulong)1 << 53)
    print(unit)
}
```

```text
e220a8397b1dcdaf
6e789e6aa1b965f4
2
0.9708819781538285
```

A shift binds tighter than `^`, so `z ^ z >> 30` shifts first. `% 6 + 1` gives a `ulong` from 1 to 6. The top 53
bits, divided by 2^53, are a `double` from 0 up to 1 with every step exact; `(ulong)1 << 53` shifts a `ulong`,
where `1 << 53` alone would be an `int` shifted too far.

### A set of 64 flags

```sg
void main() {
    ulong seen = 0
    var bits = List<int> { 0, 5, 63, 5, 40 }
    for b in read bits {
        seen |= 1 << b
    }
    print($"{seen:x16}")
    print($"{seen.countOnes()} {seen.trailingZeros()} {seen.leadingZeros()}")
    int b = 40
    print($"{seen & 1 << b != 0} {seen & 1 << 41 != 0}")
    seen &= ~(1 << 63)
    print($"{seen:x16} {64 - seen.leadingZeros()}")
}
```

```text
8000010000000021
4 0 0
true false
0000010000000021 41
```

`1 << b` takes the type of the other side, `ulong`, so bit 63 is reached. `&` binds tighter than `!=`, and `<<`
tighter than `&`, so `seen & 1 << b != 0` tests one bit. A count of bits is an `int`.

### Into a `ulong` and out of one

```sg
void main() {
    uint size = 4_000_000_000
    ulong total = size
    total *= 5
    print(total)
    long change = -3
    print((long)total + change)
    print($"{total.tryToLong() ?? -1} {ulong.MaxValue.tryToLong() ?? -1} {ulong.MaxValue.wrappingToLong()}")
    print($"{change.tryToUlong() ?? 0} {change.saturatingToUlong()} {change.wrappingToUlong()}")
    print($"{(double)ulong.MaxValue} {(double)9_007_199_254_740_993}")
    var names = List<string> { "Ada", "Grace" }
    ulong at = 1
    print(names[(nint)at])
}
```

```text
20000000000
19999999997
20000000000 -1 -1
0 0 18446744073709551613
18446744073709552000 9007199254740992
Grace
```

A `uint` widens on its own; a `long` meets a `ulong` only after a cast. `wrappingToLong` reads the same 64 bits
with a sign, so the top is -1. A `double` holds whole numbers exactly only up to 2^53, so `(double)` rounds past
it. An index is an `nint`.

### Read from text and bytes

```sg
void main() {
    print(ulong.parse("18446744073709551615") ?? 0)
    match ulong.parse("18446744073709551616") {
        Ok(n) => print(n)
        Error(e) => print(e)
    }
    print(ulong.parse("-1") is Error(Invalid(0)))
    print($"{ulong.parseHex("cbf29ce484222325") ?? 0:x}")
    var bytes = List<byte> { 1, 0, 0, 0, 0, 0, 0, 0x80 }
    print(ulong.fromLittleEndian(bytes[..]))
    print(ulong.fromBigEndian(bytes[..]))
}
```

```text
18446744073709551615
OutOfRange
true
cbf29ce484222325
9223372036854775809
72057594037928064
```

`OutOfRange` says the number is past the top; a `-` is not a `ulong`'s, and stops the text at its first byte.
`fromLittleEndian` reads eight bytes, the lowest first.

## What stops the program

### A hash with `*`

```sg panic
ulong hash(read byte[] data) {
    ulong h = 0xcbf2_9ce4_8422_2325
    for b in data {
        h ^= b
        h *= 0x100_0000_01b3
    }
    return h
}

void main() {
    print(hash("a".asBytes()))
}
```

```text
panic: overflow: 14695981039346656068 * 1099511628211 does not fit in ulong
  --> example.sg:5:9
 5 |         h *= 0x100_0000_01b3
   |         ^^^^^^^^^^^^^^^^^^^^
  hint: `tryMultiply` gives `null` instead, and `wrappingMultiply` wraps around
```

A release build wraps and prints the right hash, but the interpreter and the tests stop. `wrappingMultiply` says
the wrap is meant, in every build.

### A negative `long` cast

```sg panic
ulong toUlong(long value) {
    return (ulong)value
}

void main() {
    print(toUlong(-5))
}
```

```text
panic: value -5 does not fit in ulong
  --> example.sg:2:12
 2 |     return (ulong)value
   |            ^^^^^^^^^^^^
  hint: `tryToUlong()` gives `null` when the value does not fit, and `wrappingToUlong()` cuts it to size
```

In a release build the cast keeps the 64 bits and gives 18446744073709551611.

### A `ulong` past every `long`

```sg panic
long toLong(ulong value) {
    return (long)value
}

void main() {
    print(toLong(ulong.MaxValue))
}
```

```text
panic: value 18446744073709551615 does not fit in long
  --> example.sg:2:12
 2 |     return (long)value
   |            ^^^^^^^^^^^
  hint: `tryToLong()` gives `null` when the value does not fit, and `wrappingToLong()` cuts it to size
```

In a release build it gives -1.

## What the compiler refuses

### An `int` into a hash

```sg error
ulong mix(ulong hash, int value) {
    hash ^= value
    return hash
}

void main() {
    print(mix(0xcbf2_9ce4_8422_2325, 7))
}
```

```text
error: expected `ulong`, found `int`
  --> example.sg:2:13
 2 |     hash ^= value
   |             ^^^^^
  hint: convert with `(ulong)value`; a value that does not fit panics, or is cut to size in a release build
```

`wrappingToUlong()` keeps the bits of a negative value; a `byte` or a `uint` widens on its own.

### A `long` and a `ulong`

```sg error
void main() {
    ulong hash = 0xcbf2_9ce4_8422_2325
    long seed = 7
    print(hash ^ seed)
}
```

```text
error: `ulong` and `long` do not mix: neither holds every value of the other
  --> example.sg:4:11
 4 |     print(hash ^ seed)
   |           ^^^^^^^^^^^
  hint: convert one side to the type of the other: `(ulong)seed`; a value that does not fit panics, or is cut to size in a release build
```

### A `ulong` where a `double` goes

```sg error
void main() {
    ulong bytes = 3_500_000_000
    double gigabytes = bytes
    print(gigabytes / 1e9)
}
```

```text
error: expected `double`, found `ulong`
  --> example.sg:3:24
 3 |     double gigabytes = bytes
   |                        ^^^^^
  hint: a `double` holds whole numbers exactly up to 2^53, and `ulong` goes past that; convert with `(double)`, which rounds
```

### A `ulong` as an index

```sg error
void main() {
    var names = List<string> { "Ada", "Grace" }
    ulong at = 1
    print(names[at])
}
```

```text
error: expected `nint`, found `ulong`
  --> example.sg:4:17
 4 |     print(names[at])
   |                 ^^
  hint: convert with `(nint)at`; a value that does not fit panics, or is cut to size in a release build
```

### A minus

```sg error
void main() {
    ulong n = 5
    print(-n)
}
```

```text
error: operator `-` is not defined for `ulong`: a `ulong` is never negative
  --> example.sg:3:11
 3 |     print(-n)
   |           ^^
  hint: convert first: `-(long)n`
```

### A shift past 63

```sg error
void main() {
    ulong n = 1 << 64
    print(n)
}
```

```text
error: a shift by 64 is out of range: `ulong` has 64 bits
  --> example.sg:2:20
 2 |     ulong n = 1 << 64
   |                    ^^
  hint: the count goes from 0 to 63
```
