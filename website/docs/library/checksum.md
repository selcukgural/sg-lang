---
title: CRC-32 and Adler-32
description: Crc32, the checksum of gzip, PNG and ZIP, and Adler32, the checksum of zlib, at once or piece by piece.
sidebar_position: 10
---

# CRC-32 and Adler-32

`Crc32` computes the CRC-32 that zlib, gzip, PNG and ZIP write into their files (IEEE 802.3, the polynomial
`0xEDB88320`), so a program can check what it reads from them and write what they expect. It is written in SG in
the module `std`.

A CRC finds a byte changed by accident: a file cut short, a bit flipped on a disk or a line. It does not stand
against someone who changes the bytes on purpose, who can make any CRC they like; that takes a hash such as
SHA-256 with a key.

| Signature | What it does |
|---|---|
| `uint Crc32.of(read byte[] data)` | The CRC-32 of all the bytes at once |
| `Crc32 Crc32()` | A CRC of no bytes yet, to take them in piece by piece |
| `void c.update(read byte[] data)` | Takes in more bytes |
| `uint c.value()` | The CRC-32 of the bytes taken in so far; more may still be taken in after it |

`Crc32.of` and the pieces give the same number however the bytes are cut:

```sg
void main() {
    var text = "123456789"
    print($"{Crc32.of(text.asBytes()):x8}")

    var c = Crc32()
    c.update("1234".asBytes())
    c.update("56789".asBytes())
    print($"{c.value():x8}")

    var none = List<byte>()
    print($"{Crc32.of(read none):x8}")
}
```

It prints:

```text
cbf43926
cbf43926
00000000
```

`cbf43926` is the check value of the standard: every CRC-32 gives it for the nine digits.

## How fast

The bytes are taken sixteen at a time through sixteen tables of 256 numbers, which the compiler makes once when
the program is compiled. Four gigabytes take 1.1 seconds on an Apple M3 Max, as the same algorithm
written in C does. zlib on macOS takes a tenth of that with the processor's own CRC instructions, which SG
cannot call yet; a program that needs that speed calls zlib's `crc32` through C, as the
[zlib example](https://github.com/selcukgural/sg-lang/tree/main/interpreter/examples/zlib) does.

## Adler-32

`Adler32` computes the Adler-32 that the zlib format writes after its data (RFC 1950): two sums modulo 65,521,
of the bytes and of the first sum after each. It is quicker to compute than a CRC and finds fewer changes in
short data. It is used as `Crc32` is.

| Signature | What it does |
|---|---|
| `uint Adler32.of(read byte[] data)` | The Adler-32 of all the bytes at once; 1 for no bytes |
| `Adler32 Adler32()` | An Adler-32 of no bytes yet, to take them in piece by piece |
| `void a.update(read byte[] data)` | Takes in more bytes |
| `uint a.value()` | The Adler-32 of the bytes taken in so far; more may still be taken in after it |

```sg
void main() {
    print($"{Adler32.of("Wikipedia".asBytes()):x8}")
    var a = Adler32()
    a.update("Wiki".asBytes())
    a.update("pedia".asBytes())
    print($"{a.value():x8}")
}
```

It prints:

```text
11e60398
11e60398
```

[Compression](./compression.md) computes both for the formats that need them.
