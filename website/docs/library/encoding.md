---
title: Hex and Base64
description: Hex and Base64, bytes written as text and read back, strictly.
sidebar_position: 9
---

# Hex and Base64

Bytes go into text, a URL, a header or a file of settings as hex or as Base64 (RFC 4648), and come back out
the same. Both are written in SG in the module `std`.

Reading is strict. A space, a line break, a character of another alphabet, padding where it cannot be, or bits
left over beyond the last byte is an error with its place, never skipped: two programs cannot read one text as
two different sets of bytes, and each set of bytes has one text only. A text with line breaks in it is read
after they are taken out (`text.replaceAll("\n", "")`).

## `Hex`

| Signature | What it does |
|---|---|
| `string Hex.encode(read byte[] data)` | Two lower-case digits for each byte |
| `void Hex.encodeInto(ref string target, read byte[] data)` | The same digits written at the end of `target`, with no new text made |
| `Result<List<byte>, DecodeError> Hex.decode(read string text)` | The bytes of the digits, upper or lower case |

## `Base64`

| Signature | What it does |
|---|---|
| `string Base64.encode(read byte[] data)` | The standard alphabet (`+`, `/`), padded with `=` to a multiple of four |
| `string Base64.encodeUrl(read byte[] data)` | The alphabet safe in a URL or a file name (`-`, `_`), without padding |
| `Result<List<byte>, DecodeError> Base64.decode(read string text)` | The bytes of a padded text in the standard alphabet |
| `Result<List<byte>, DecodeError> Base64.decodeUrl(read string text)` | The bytes of an unpadded text in the URL alphabet |

## `DecodeError`

| Variant | When |
|---|---|
| `InvalidCharacter(long position)` | A byte that is not of the alphabet, padding where it cannot be, or bits left beyond the last byte: its place, counted in bytes from the start |
| `InvalidLength` | A length the encoding never writes: an odd count of hex digits, a padded text whose length is not a multiple of four, an unpadded one with a lone character at its end |

A text goes in with `read`, as any text does; one that a call has just made is lent the same way:

```sg
void main() {
    var message = "Hi, SG!"
    var hex = Hex.encode(message.asBytes())
    var standard = Base64.encode(message.asBytes())
    var url = Base64.encodeUrl(message.asBytes())
    print(hex)
    print(standard)
    print(url)
    var back = Base64.decode(read standard) ?? List<byte>()
    print(string.fromUtf8(read back) ?? "")
    var id = "user-"
    Hex.encodeInto(ref id, read byte[4] { 0xDE, 0xAD, 0xBE, 0xEF })
    print(id)
}
```

It prints:

```text
48692c20534721
SGksIFNHIQ==
SGksIFNHIQ
Hi, SG!
user-deadbeef
```

A text that does not read back says where:

```sg
void check(read string text) {
    match Base64.decode(text) {
        Ok(bytes) => print($"{text}: {bytes.length()} bytes")
        Error(InvalidCharacter(at)) => print($"{text}: not Base64 at {at}")
        Error(InvalidLength) => print($"{text}: not a length Base64 has")
    }
}

void main() {
    check("SGVsbG8=")
    check("SGVs bG8")
    check("SGVsbG8")
    check("SGVsbG9=")
}
```

It prints:

```text
SGVsbG8=: 5 bytes
SGVs bG8: not Base64 at 4
SGVsbG8: not a length Base64 has
SGVsbG9=: not Base64 at 6
```

`SGVsbG9=` has a digit whose last bits are not zero: it would read as the same five bytes as `SGVsbG8=`, so it
is refused.
