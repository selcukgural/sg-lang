---
title: SHA-256 and HMAC
description: Sha256, Hmac.sha256 and Bytes.equalInConstantTime, the hash and the keyed tag of the standards.
sidebar_position: 11
---

# SHA-256 and HMAC

`Sha256` computes the SHA-256 of FIPS 180-4: 32 bytes that change completely when one byte of the input
changes, and that nobody knows how to match with other bytes. A program uses it to tell whether a file is
the one it expects, to name content by what it holds, or to check a download against the hash its site
gives. `Hmac.sha256` is HMAC-SHA-256 (RFC 2104): a hash under a secret key, the tag that shows a message
comes from someone who holds the key and was not changed on the way; webhooks, signed cookies and JWTs
with `HS256` are checked with it. Both are written in SG in the module `std`.

No other hash and no cipher come with the language. Encryption, passwords and TLS stay with C libraries
that are looked after for that (OpenSSL, libsodium), behind a module whose users write no `unsafe`. A
password is never stored as a SHA-256: that takes a hash made slow on purpose, such as Argon2 from
libsodium.

| Signature | What it does |
|---|---|
| `byte[32] Sha256.of(read byte[] data)` | The SHA-256 of all the bytes at once |
| `Sha256 Sha256()` | A hash of no bytes yet, to take them in piece by piece |
| `void h.update(read byte[] data)` | Takes in more bytes |
| `byte[32] h.finish()` | The hash of the bytes taken in so far; the hash itself does not change, and more bytes may be taken in after it |
| `byte[32] Hmac.sha256(read byte[] key, read byte[] data)` | The HMAC-SHA-256 of `data` under `key`; a key longer than 64 bytes is hashed first, as the RFC says |
| `bool Bytes.equalInConstantTime(read byte[] a, read byte[] b)` | Whether `a` and `b` hold the same bytes, in a time that depends only on their length |

`Sha256.of` and the pieces give the same hash however the bytes are cut. `Hex.encode` writes it as text:

```sg
void main() {
    var hash = Sha256.of("abc".asBytes())
    print(Hex.encode(read hash))

    var h = Sha256()
    h.update("a".asBytes())
    h.update("bc".asBytes())
    var same = h.finish()
    print(Hex.encode(read same))
}
```

It prints:

```text
ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
```

A file is hashed a block at a time, so that it never has to be in memory whole. `readSome` fills the start
of the buffer and gives how many bytes it put there, 0 at the end of the file:

```sg
Result<string, IoError> hashOf(read string path) {
    var reader = try File.openText(path)
    var buffer = byte[65536]
    var h = Sha256()
    while true {
        var count = try reader.readSome(ref buffer)
        if count == 0 {
            break
        }
        h.update(buffer[0..count])
    }
    var hash = h.finish()
    return Ok(Hex.encode(read hash))
}

Result<void, IoError> main() {
    try File.writeText("abc.txt", "abc")
    print(try hashOf("abc.txt"))
    try File.delete("abc.txt")
    return Ok()
}
```

It prints:

```text
ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
```

## Checking a tag

The one who sends a message computes its tag with the key they share with the receiver; the receiver
computes it again and compares. The comparison is `Bytes.equalInConstantTime`, never `==`: `==` stops at the
first byte that differs, so the time it takes tells someone who sends guesses how many of their first bytes
were right, and they can find a valid tag byte by byte. `equalInConstantTime` goes over every byte whatever
they hold (two lengths that differ give `false` at once: a length is not the secret).

```sg
void main() {
    var key = "a key both sides hold".asBytes()
    var message = "amount=100&to=alice".asBytes()
    var tag = Hmac.sha256(key, message)

    var received = Hmac.sha256(key, message)
    print(Bytes.equalInConstantTime(read tag, read received))

    var forged = Hmac.sha256(key, "amount=900&to=alice".asBytes())
    print(Bytes.equalInConstantTime(read tag, read forged))
}
```

It prints:

```text
true
false
```

## How fast

A compiled program hands each block of 64 bytes to the processor's SHA instructions where it has them
(the SHA2 extension of 64-bit Arm, SHA-NI on x86-64); elsewhere, and in the interpreter, the 64 rounds are
plain SG. The hash is the same either way. A gigabyte takes 0.41 seconds on an Apple M3 Max, where
OpenSSL takes 0.35; 100 MB of a file take 0.049 seconds, against 0.045 for Rust's `sha2` and 0.041 for
Zig's. An HMAC of each of 921,952 lines of a log takes 0.14 seconds, about Rust's time.
