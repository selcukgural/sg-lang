---
name: ushort
kind: type
summary: An integer of 16 bits without a sign, from 0 to 65 535: a port, a field of a packet, a counter that wraps.
see: short, byte, uint, int, packed
learn: ../docs/learn/book/02-values.md#the-integer-types
library: ../docs/library/numbers.md
---

## Forms

| Form | What it is |
|---|---|
| `ushort` | an integer of 16 bits, from 0 to 65 535, never negative |
| `ushort port = 8080`, `0xffff` | a number written in the program, where a `ushort` goes |
| `ushort.MinValue`, `ushort.MaxValue` | the two ends, 0 and 65535 |
| `ushort w = b`, `int x = w`, `uint x = w` | a `byte` becomes a `ushort` on its own, and a `ushort` any wider type |
| `(ushort)n`, `n.tryToUshort()`, `n.wrappingToUshort()` | a wider or signed number made a `ushort`: a panic, `null`, or the low 16 bits when it does not fit |
| `ushort.fromBigEndian(bytes)`, `w.toBigEndian()` | a `ushort` out of two bytes, the highest first, and its two bytes; `LittleEndian` the same, the lowest first |
| `w.isPowerOfTwo()`, `w.log2()` | the methods of powers of two, which only unsigned types have |
| `ushort.parse(text)`, `ushort.parseHex(text)` | the number a text writes, as a `Result<ushort, NumberError>` |

## What it means

A `ushort` is a whole number from 0 to 65 535 in 16 bits, on every machine. Its range is that of a port of TCP
or UDP, and of the fields of two bytes in the headers of a network packet or a file: a length, a count, a
checksum. A number written in the program is a `ushort` where a `ushort` goes; one that does not fit, and any
negative number, is refused when the program is compiled.

Arithmetic happens in the type of its operands, so `ushort + ushort` is a `ushort`, also where the result goes
into an `int`, and `1 - 2` on two `ushort`s goes below 0. A result past an end is an overflow: a panic in the
interpreter, in tests and in a debug build, and a wrap in a release build. `wrappingAdd`, `tryAdd` and
`saturatingAdd` say one behaviour for every build, and so do the methods of `-` and `*`. `-w` is refused, since a
`ushort` is never negative; `>>` brings in zeros.

A `byte` becomes a `ushort` on its own, and a `ushort` becomes an `int`, a `uint`, a `long`, a `ulong`, an
`nint`, an `nuint`, a `float` or a `double`. A signed type never becomes a `ushort` on its own, not even an
`sbyte`: a `short` and a `ushort` do not mix, and an `int` holds both. The way to a `ushort` is a cast,
`(ushort)n`, which stops the program when the value does not fit (a release build keeps the low 16 bits), or
`tryToUshort`, `saturatingToUshort` and `wrappingToUshort`, which do the same in every build. After `...` in a
call of C a `ushort` is refused, since C reads an `int` there.

The methods are in [Numbers](../docs/library/numbers.md).

## Examples

### A port

```sg
void show(read string address) {
    var colon = address.lastIndexOf(':') ?? -1
    match ushort.parse(address[colon + 1..]) {
        Ok(port) => print($"{address}: port {port}")
        Error(OutOfRange) => print($"{address}: past the last port")
        Error(Invalid(at)) => print($"{address}: not a port, from byte {at}")
    }
}

void main() {
    ushort web = 8080
    print($"{web} {ushort.MinValue} {ushort.MaxValue}")
    show("localhost:8080")
    show("[::1]:443")
    show("example.org:70000")
    show("example.org:-1")
}
```

```text
8080 0 65535
localhost:8080: port 8080
[::1]:443: port 443
example.org:70000: past the last port
example.org:-1: not a port, from byte 0
```

A port goes from 0 to 65535, as a `ushort` does, so `ushort.parse` tests the range as it reads. A sign is not a
digit of an unsigned type: `-1` is `Invalid`, not `OutOfRange`.

### The header of a packet

```sg
struct UdpHeader {
    ushort source
    ushort target
    ushort length
    ushort checksum
}

UdpHeader headerOf(read byte[] b) {
    return UdpHeader {
        source = ushort.fromBigEndian(b[0..2]),
        target = ushort.fromBigEndian(b[2..4]),
        length = ushort.fromBigEndian(b[4..6]),
        checksum = ushort.fromBigEndian(b[6..8]),
    }
}

void main() {
    var packet = List<byte> { 0xc3, 0x50, 0x00, 0x35, 0x00, 0x1c, 0xab, 0xcd }
    var h = headerOf(packet[..])
    print($"{h.source} -> {h.target}, {h.length} bytes, sum {h.checksum:x4}")
    ushort port = 53
    print(port.toBigEndian())
    print(port.toLittleEndian())
}
```

```text
50000 -> 53, 28 bytes, sum abcd
[0, 53]
[53, 0]
```

The header of UDP is four `ushort`s, each written the highest byte first, as every number of the network is.
`fromBigEndian` reads exactly two bytes, and `toBigEndian` gives them back, where the memory of the machine keeps
the lowest first.

### The checksum of the internet

```sg
ushort checksum(read byte[] data) {
    uint sum = 0
    nint i = 0
    while i + 1 < data.length() {
        sum += ushort.fromBigEndian(data[i..i + 2])
        i += 2
    }
    if i < data.length() {
        sum += (uint)data[i] << 8
    }
    while sum > 0xffff {
        sum = (sum & 0xffff) + (sum >> 16)
    }
    return ~(ushort)sum
}

void main() {
    var words = List<byte> { 0x00, 0x01, 0xf2, 0x03, 0xf4, 0xf5, 0xf6, 0xf7 }
    print($"{checksum(words[..]):x4}")
    var odd = List<byte> { 0x01, 0x02, 0x03 }
    print($"{checksum(odd[..]):x4}")
}
```

```text
220d
fbfd
```

IP, TCP and UDP check their headers with this sum of RFC 1071: the data as `ushort`s, added in a wider type, the
carries folded back into the low 16 bits, and every bit turned. Each `ushort` widens to a `uint` as it is added,
so the sum cannot overflow; `(ushort)sum` fits once the carries are folded. The first sum is the example of the
RFC.

### A counter that wraps

```sg
bool newer(ushort a, ushort b) {
    short ahead = b.wrappingSubtract(a).wrappingToShort()
    return ahead > 0
}

void main() {
    ushort seq = 65534
    for _ in 0..4 {
        print(seq)
        seq = seq.wrappingAdd(1)
    }
    print($"{newer(65535, 1)} {newer(1, 65535)} {newer(100, 200)}")
}
```

```text
65534
65535
0
1
true false true
```

A packet of sound or video carries a number of 16 bits that goes up by one and starts again at 0, and the
receiver tells which of two packets is newer. `wrappingAdd` goes from 65535 to 0 in every build. The distance
from `a` to `b`, wrapped and read with a sign, is positive when `b` is ahead by less than half the range: 1 comes
after 65535.

### Wider types and bits

```sg
void main() {
    byte low = 0x34
    byte high = 0x12
    ushort word = high
    word = word << 8 | low
    print($"{word:x4}")
    int asInt = word
    uint asUint = word
    float asFloat = word
    print($"{asInt + 1} {asUint * 2} {asFloat / 2}")
    print($"{word.countOnes()} {word.isPowerOfTwo()} {word.nextPowerOfTwo()} {word.log2()}")
}
```

```text
1234
4661 9320 2330
5 false 8192 12
```

A `byte` becomes a `ushort` on its own, and the shift happens in 16 bits, the type of its left side; `low`
widens for the `|`. A `ushort` becomes an `int`, a `uint` or a `float` on its own. The powers of two are methods
of the unsigned types only.

## What stops the program

### A sum that goes into an `int`

```sg panic
void main() {
    ushort a = 60_000
    ushort b = 10_000
    int total = a + b
    print(total)
}
```

```text
panic: overflow: 60000 + 10000 does not fit in ushort
  --> example.sg:4:17
 4 |     int total = a + b
   |                 ^^^^^
  hint: `tryAdd` gives `null` instead, and `wrappingAdd` wraps around
```

The sum is a `ushort`, since both sides are, and becomes an `int` only after it is found; a release build wraps
it to 4464. `(int)a + b` adds in an `int` and gives 70000.

### A negative number, cast

```sg panic
ushort toPort(int n) {
    return (ushort)n
}

void main() {
    print(toPort(443))
    print(toPort(-1))
}
```

```text
443
panic: value -1 does not fit in ushort
  --> example.sg:2:12
 2 |     return (ushort)n
   |            ^^^^^^^^^
  hint: `tryToUshort()` gives `null` when the value does not fit, and `wrappingToUshort()` cuts it to size
```

A release build keeps the low 16 bits, so -1 becomes 65535. `n.tryToUshort()` gives `null` for it in every
build.

## What the compiler refuses

### A negative number

```sg error
void main() {
    ushort port = -1
    print(port)
}
```

```text
error: -1 does not fit in ushort
  a `ushort` is never negative
  --> example.sg:2:19
 2 |     ushort port = -1
   |                   ^^
```

`(ushort)-1` is refused the same way. `ushort.MaxValue` is the largest.

### A number too large

```sg error
void main() {
    ushort mask = 0x1_0000
    print(mask)
}
```

```text
error: `0x1_0000` is 65536, which does not fit in ushort
  --> example.sg:2:19
 2 |     ushort mask = 0x1_0000
   |                   ^^^^^^^^
```

### A minus

```sg error
void main() {
    ushort port = 8080
    print(-port)
}
```

```text
error: operator `-` is not defined for `ushort`: a `ushort` is never negative
  --> example.sg:3:11
 3 |     print(-port)
   |           ^^^^^
  hint: convert first: `-(int)port`
```

### A signed type where a `ushort` goes

```sg error
void main() {
    short offset = 10
    ushort port = offset
    print(port)
}
```

```text
error: expected `ushort`, found `short`
  --> example.sg:3:19
 3 |     ushort port = offset
   |                   ^^^^^^
  hint: convert with `(ushort)`; it panics if the value does not fit
```

A `short` can be negative, so it never becomes a `ushort` on its own, even where its value is 10.

### An `int` where a `ushort` goes

```sg error
ushort portOf(int n) {
    return n
}

void main() {
    print(portOf(8080))
}
```

```text
error: expected `ushort`, found `int`
  --> example.sg:2:12
 2 |     return n
   |            ^
  hint: convert with `(ushort)`; it panics if the value does not fit
```
