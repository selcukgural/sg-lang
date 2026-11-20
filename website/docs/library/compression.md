---
title: Compression
description: Deflate, Zlib and Gzip, both ways, at once or as a stream; zlib's bytes at every level, and a limit on what is opened.
sidebar_position: 16
---

# Compression

`Deflate`, `Zlib` and `Gzip` compress bytes and open them again. DEFLATE (RFC 1951) is the compression that
zip archives and PNG images hold; zlib (RFC 1950) puts two bytes before it and an Adler-32 after it, as HTTP's
`deflate` and many file formats do; gzip (RFC 1952) puts a header before it and a CRC-32 and the length after
it, as `.gz` files and HTTP's `gzip` do. They are written in SG in the module `std`.

The compressor is zlib's, written again in SG step for step: at every level from 0 (stored as it is) to 9
(smallest) it gives the same bytes as zlib 1.3.1, so a file made by SG is the file `gzip` and every program that
calls zlib would make, and a test can compare bytes. The decompressor opens what any of them made.

Opening always takes a limit: the most bytes the data may open into. A few kilobytes can open into gigabytes
(a "zip bomb"), so a program that opens what it was sent says how much it is ready to hold, and data that would
open into more is the error `TooLarge`, not memory run out.

| Signature | What it does |
|---|---|
| `List<byte> Gzip.compress(read byte[] data, int level)` | The data compressed at `level`, 0 to 9 |
| `Result<List<byte>, CompressError> Gzip.decompress(read byte[] data, nint limit)` | The data opened, at most `limit` bytes |
| `Compressor Gzip.writer(StreamWriter target, int level)` | A writer that compresses what it is given into `target` |
| `Decompressor Gzip.reader(StreamReader source, long limit)` | A reader of what `source` holds, opened as it is read |

`Zlib` and `Deflate` have the same four. A level outside 0 to 9 written in the program is a compile error;
one made when the program runs is a panic. 6 is zlib's default, 1 the fastest that compresses, and 0 stores
the bytes as they are, in blocks of the format. A limit below 0 is a panic.

```sg
Result<void, CompressError> main() {
    var text = "SG compresses what it is given, and opens it again. ".repeat(50)
    var packed = Gzip.compress(text.asBytes(), 6)
    print($"{text.length()} bytes compress to {packed.length()}")
    var opened = try Gzip.decompress(read packed, 1 << 20)
    print((string.fromUtf8(read opened) ?? "not text") == text)

    var smallest = Zlib.compress(text.asBytes(), 9)
    var stored = Deflate.compress(text.asBytes(), 0)
    print($"zlib at 9: {smallest.length()}, DEFLATE stored: {stored.length()}")
    return Ok()
}
```

It prints:

```text
2600 bytes compress to 90
true
zlib at 9: 78, DEFLATE stored: 2605
```

A gzip header written by SG holds no name and no time, and 255 for the system that made it, so the same data
gives the same bytes on every machine. When gzip is opened, a name, a comment and extra fields in the header are
read and passed over, and a CRC of the header is checked; several members one after the other (what `cat a.gz
b.gz` makes) open as one.

## Errors

`CompressError` says why data could not be opened:

| Variant | When |
|---|---|
| `Corrupt(long position)` | The data is not what the format says; the position is the byte of the compressed data where the part that is wrong begins (a header, the codes of a block, a code) |
| `Truncated` | The data ends before its stream does |
| `ChecksumMismatch` | It opened, but its CRC-32 (gzip) or Adler-32 (zlib), or its length, is not the one written after it |
| `TooLarge` | It opens into more bytes than the limit |
| `TrailingData` | Bytes follow the end of the stream |
| `Io(IoError error)` | Reading the stream failed, or the text read is not UTF-8 (`InvalidText(position)`, counted in the opened bytes) |

Bytes after the end are an error rather than passed over: they are often a second file glued on, or the sign
of a stream cut and joined wrongly.

```sg
void show(Result<List<byte>, CompressError> result) {
    match result {
        Ok(bytes) => print($"{bytes.length()} bytes")
        Error(cause) => print($"{cause}")
    }
}

void main() {
    var zeros = List<byte>()
    for _ in 0..1000 {
        zeros.addAll(read byte[1000])
    }
    var bomb = Gzip.compress(read zeros, 9)
    print($"a megabyte of zeros is {bomb.length()} bytes")
    show(Gzip.decompress(read bomb, 1000))
    show(Gzip.decompress(read bomb, 1_000_000))

    var cut = List<byte>()
    cut.addAll(bomb[..bomb.length() - 4])
    show(Gzip.decompress(read cut, 1_000_000))
    bomb[20] ^= 1
    show(Gzip.decompress(read bomb, 1_000_000))
}
```

It prints:

```text
a megabyte of zeros is 1003 bytes
the data opens into more bytes than the limit
1000000 bytes
the compressed data ends before its stream does
corrupt compressed data at byte 11
```

## Streams

`Compressor` compresses into a `StreamWriter` a piece at a time, with `writeBytes`, and `close` ends the
stream, writes what is left and closes the writer; like a `StreamWriter`, a local one is closed before it ends
(`Closable`). At levels 1 to 9 the bytes are the same however the data is cut into pieces; at level 0 the stored
blocks follow the pieces, as zlib's do.

`Decompressor` reads a `StreamReader` and gives what it opens with the calls a `StreamReader` has:

| Signature | What it does |
|---|---|
| `Result<nint, CompressError> readSome(ref this, ref byte[] buffer)` | What is opened next, as much as is ready; 0 at the end |
| `Result<nint, CompressError> readBytes(ref this, ref byte[] buffer)` | Fills the buffer unless the end comes first; how many bytes |
| `Result<bool, CompressError> readLine(ref this, ref string line)` | The next line into `line`, without its `\n` or `\r\n`; `false` at the end |
| `Result<string, CompressError> readText(ref this)` | All that is left, as text |

The limit counts every byte the reader opens; the bytes up to it are given, and asking for one more is
`TooLarge`. The end of the stream must be the end of the source too.

```sg
Result<void, IoError> write(read string path) {
    var writer = Gzip.writer(try File.createText(read path), 6)
    for i in 0..1000 {
        try writer.writeBytes($"event {i}\n".toString().asBytes())
    }
    return writer.close()
}

Result<nint, CompressError> count(StreamReader source) {
    var reader = Gzip.reader(source, 1 << 20)
    var line = string.withCapacity(32)
    nint lines = 0
    while try reader.readLine(ref line) {
        if lines == 0 {
            print($"the first: {line}")
        }
        lines += 1
    }
    return Ok(lines)
}

Result<void, IoError> main() {
    var path = "events.gz"
    try write(read path)
    var info = try File.info(read path)
    print($"{info.size} bytes")
    match count(try File.openText(read path)) {
        Ok(lines) => print($"{lines} lines")
        Error(cause) => print(cause)
    }
    try File.delete(read path)
    return Ok()
}
```

It prints:

```text
1983 bytes
the first: event 0
1000 lines
```

## How fast

Sixteen megabytes of text compress at level 6 in about the time zlib takes, on an Apple M3 Max (`bench/gzip`,
1.04 times zlib's time). Opening is slower: `bench/gunzip`, which opens the same text twenty times, takes 2.6
times zlib's time. The decoder reads its codes from tables as zlib's does, but each byte it writes and each entry
it reads is checked against the end of its list, where zlib's C trusts its pointers; a program that needs zlib's
speed calls zlib through C, as the
[zlib example](https://github.com/selcukgural/sg-lang/tree/main/interpreter/examples/zlib) does.

[Zip archives](./zip.md) are built on `Deflate`.
