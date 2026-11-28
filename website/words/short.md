---
name: short
kind: type
summary: An integer of 16 bits, with a sign, from -32 768 to 32 767: a sample of sound, a field of two bytes.
see: ushort, sbyte, int, double, packed
learn: ../docs/learn/book/02-values.md#the-integer-types
library: ../docs/library/numbers.md
---

## Forms

| Form | What it is |
|---|---|
| `short` | an integer of 16 bits, from -32 768 to 32 767 |
| `short s = -12_000`, `0x7fff` | a number written in the program, where a `short` goes |
| `short.MinValue`, `short.MaxValue` | the two ends |
| `int x = s`, `double x = s` | a `short` becomes a wider signed type, or a fraction, on its own |
| `(short)n`, `n.tryToShort()`, `n.saturatingToShort()` | a wider number made a `short`: a panic, `null`, or the nearest end when it does not fit |
| `a.saturatingAdd(b)`, `a.wrappingAdd(b)` | arithmetic that stops at an end, or wraps, in every build |
| `short.fromLittleEndian(bytes)`, `s.toLittleEndian()` | a `short` out of two bytes, and its two bytes; `BigEndian` the same, the highest first |
| `short.parse(text)` | the number a text writes, as a `Result<short, NumberError>` |

## What it means

A `short` is a whole number from -32 768 to 32 767 in 16 bits, two's complement, on every machine. It is the
type of a sample of sound in 16 bits, as a WAV file and a sound card keep it, of a signed field of two bytes in a
file or a packet, and of C's `short`. A list of them takes half the memory of `int`s. A number written in the
program is a `short` where a `short` goes, and one that does not fit is refused when the program is compiled.

Arithmetic happens in the type of its operands, so `short + short` is a `short`, also where the result goes into
an `int`. A result past an end is an overflow: a panic in the interpreter, in tests and in a debug build, and a
wrap in a release build. For sound a wrap is the worst answer, since a loud peak turns into a loud peak of the
other sign; `saturatingAdd` stops at the end instead, in every build, and `tryAdd` and `wrappingAdd` say the
other two behaviours. `short.MinValue / -1` stops the program in every build.

A `short` becomes an `int`, a `long`, an `nint`, a `float` or a `double` on its own, and a `byte` or an `sbyte`
becomes a `short`. It never becomes an unsigned type on its own: a `short` and a `ushort` do not mix, and an
`int` holds both. The way back from a wider type is a cast, `(short)n`, which stops the program when the value
does not fit (a release build keeps the low 16 bits of an integer, and takes the nearest end for a fraction), or
`tryToShort`, `saturatingToShort` and `wrappingToShort`, which do the same in every build. After `...` in a call
of C a `short` is refused, since C reads an `int` there.

The methods are in [Numbers](../docs/library/numbers.md).

## Examples

### A number of 16 bits

```sg
void main() {
    short sample = -12_000
    short quiet = 0x00ff
    print($"{sample} {quiet} {short.MinValue} {short.MaxValue}")
    print($"{Memory.sizeOf<short>()} {Memory.sizeOf<int>()}")
    int wide = sample
    double level = sample
    print($"{wide * 10} {level / short.MaxValue}")
    print($"{sample:x4} {sample:X}")
}
```

```text
-12000 255 -32768 32767
2 4
-120000 -0.36622211371196634
d120 D120
```

A `short` takes two bytes and an `int` four. `sample` widens to an `int` and to a `double` on its own, so
`wide * 10` is an `int` and `level / short.MaxValue` a fraction. In hex a negative `short` is written as its
16 bits.

### Samples out of bytes

```sg
void main() {
    var data = List<byte> { 0x00, 0x00, 0xe8, 0x03, 0x18, 0xfc, 0x00, 0x80, 0xff, 0x7f }
    var samples = List<short>()
    for i in 0..data.length() / 2 {
        samples.add(short.fromLittleEndian(data[2 * i..2 * i + 2]))
    }
    print(samples)
    ushort peak = 0
    for s in read samples {
        peak = peak.max(s.unsignedAbs())
    }
    print(peak)
    var out = List<byte>()
    for s in read samples {
        out.addAll(read s.toLittleEndian())
    }
    print(out == data)
}
```

```text
[0, 1000, -1000, -32768, 32767]
32768
true
```

The samples of a WAV file lie in pairs of bytes, the lowest first. `fromLittleEndian` takes exactly two and reads
the highest bit as the sign. The loudest sample here is -32768, whose size does not fit in a `short`: `abs`
would overflow, and `unsignedAbs` gives it as a `ushort`. `toLittleEndian` gives the two bytes back.

### Mixing two sounds

```sg
short mix(short a, short b) {
    return a.saturatingAdd(b)
}

void main() {
    var voice = List<short> { 1000, 20_000, -25_000, 30_000 }
    var music = List<short> { 500, 15_000, -15_000, 10_000 }
    for i in 0..voice.length() {
        var a = voice[i]
        var b = music[i]
        print($"{mix(a, b)} {a.wrappingAdd(b)}")
    }
}
```

```text
1500 1500
32767 -30536
-32768 25536
32767 -25536
```

Two sounds are mixed by adding their samples. Where the sum passes an end, `saturatingAdd` holds it there, which
is heard as clipping; `wrappingAdd` jumps to the other sign, which is heard as a crack. Both do the same in every
build.

### Louder, and the average

```sg
void main() {
    var samples = List<short> { 12_000, -32_768, 32_767, -400 }
    double gain = 1.5
    var louder = List<short>()
    for s in read samples {
        louder.add((s * gain).saturatingToShort())
    }
    print(louder)
    long sum = 0
    for s in read samples {
        sum += s
    }
    print($"{sum} {(double)sum / (double)samples.length()}")
}
```

```text
[18000, -32768, 32767, -600]
11599 2899.75
```

In `s * gain` the `short` becomes a `double` on its own, since a `double` holds every `short` exactly.
`saturatingToShort` brings the product back, the nearest end where it does not fit. A sum of many samples is
kept in a `long`, and each `short` widens as it is added.

### A release build wraps

```sg file title="clip/main.sg"
short mix(short a, short b) {
    return a + b
}

void main() {
    print(mix(20_000, 15_000))
}
```

```text
$ cd clip
$ sg main.sg
panic: overflow: 20000 + 15000 does not fit in short
  --> main.sg:2:12
 2 |     return a + b
   |            ^^^^^
  hint: `tryAdd` gives `null` instead, and `wrappingAdd` wraps around
$ sg build --release main.sg -o mix
$ ./mix
-30536
```

The interpreter and a debug build stop at the overflow. A release build leaves the check out and the sum wraps to
the other sign. Where an overflow can happen, as when sounds are mixed, the program says what it wants with
`saturatingAdd`.

## What stops the program

### A fraction too large, cast

```sg panic
short toSample(double x) {
    return (short)(x * 32767)
}

void main() {
    print(toSample(0.5))
    print(toSample(1.5))
}
```

```text
16383
panic: value 49150.5 does not fit in short
  --> example.sg:2:12
 2 |     return (short)(x * 32767)
   |            ^^^^^^^^^^^^^^^^^^
  hint: `tryToShort()` gives `null` when the value does not fit
```

Sound as fractions goes from -1 to 1, and 1.5 is past the end. A release build takes the nearest end, 32767;
`(x * 32767).saturatingToShort()` does that in every build.

### A sum that goes into an `int`

```sg panic
void main() {
    short a = 30_000
    short b = 10_000
    int total = a + b
    print(total)
}
```

```text
panic: overflow: 30000 + 10000 does not fit in short
  --> example.sg:4:17
 4 |     int total = a + b
   |                 ^^^^^
  hint: the result goes into an `int`; compute in `int` from the start: `(int)a + b`
```

The sum is a `short`, since both sides are, and becomes an `int` only after it is found. `(int)a + b` adds in an
`int`.

## What the compiler refuses

### A number that does not fit

```sg error
void main() {
    short sample = 40_000
    print(sample)
}
```

```text
error: `40_000` does not fit in short
  --> example.sg:2:20
 2 |     short sample = 40_000
   |                    ^^^^^^
```

`short s = 0xffff` is refused the same way: it is 65535.

### An `int` added to a `short`

```sg error
void main() {
    short sample = 1000
    int step = 24
    sample += step
    print(sample)
}
```

```text
error: expected `short`, found `int`
  --> example.sg:4:15
 4 |     sample += step
   |               ^^^^
  hint: convert with `(short)step`; a value that does not fit panics, or is cut to size in a release build
```

### A `short` and a `ushort`

```sg error
void main() {
    short sample = -1200
    ushort offset = 300
    print(sample + offset)
}
```

```text
error: `short` and `ushort` do not mix: neither holds every value of the other
  --> example.sg:4:11
 4 |     print(sample + offset)
   |           ^^^^^^^^^^^^^^^
  hint: `int` holds both: `(int)offset`
```

### A window of the wrong width

```sg error
void main() {
    var data = List<byte> { 0xe8, 0x03, 0x18, 0xfc }
    print(short.fromLittleEndian(data[0..4]))
}
```

```text
error: `short.fromLittleEndian` reads 2 bytes, and this window has 4
  --> example.sg:3:34
 3 |     print(short.fromLittleEndian(data[0..4]))
   |                                  ^^^^^^^^^^
  hint: a window of 2 bytes: `data[0..2]`
```

A window whose width is known only when the program runs stops it there, with both numbers.

### A `short` after `...` in a call of C

```sg error
extern {
    int printf(read CString format, ...)
}

void main() {
    short level = -12
    var format = CString.from("%d\n") ?? panic("a zero byte inside")
    _ = unsafe printf(read format, level)
}
```

```text
error: a `short` does not go to C after `...`
  C reads it there as an `int`, made wider without a word
  --> example.sg:8:36
 8 |     _ = unsafe printf(read format, level)
   |                                    ^^^^^
  hint: `(int)level`
```

An `sbyte`, a `byte` and a `ushort` are refused there the same way.

### A shift past 15

```sg error
void main() {
    short bits = 1
    print(bits << 16)
}
```

```text
error: a shift by 16 is out of range: `short` has 16 bits
  --> example.sg:3:19
 3 |     print(bits << 16)
   |                   ^^
  hint: the count goes from 0 to 15
```

`int bits = 1` shifts in 32 bits.
