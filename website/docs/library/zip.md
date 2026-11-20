---
title: Zip archives
description: ZipReader and ZipWriter, archives read and written as Go reads and writes them, checked for Zip Slip and bombs before anything is read.
sidebar_position: 17
---

# Zip archives

`ZipReader` reads a zip archive and `ZipWriter` writes one: entries stored as they are or compressed by
DEFLATE, zip64 for archives and entries past 4 GB, names in UTF-8. They read and write what Go's
`archive/zip`, Python's `zipfile` and `zip` read and write, and they are written in SG in the module `std`,
over the [compression](./compression.md) of `Deflate`.

An archive is a list of names and bytes that someone else made, so reading one is where two old attacks
land. A name such as `../../.bashrc` writes outside the folder a program unpacks into ("Zip Slip"), and a few
kilobytes can open into terabytes, the worst of them by giving every entry the same block of compressed data.
`ZipReader` checks the archive when it is opened, before any entry is given: a name that could lead out of a
folder, two entries of one name, entries whose data lie over each other, more entries than the directory
holds. An archive with any of these is refused whole. Each entry is then read with a limit, and gives no
more bytes than the size the archive writes for it. There is no call that unpacks a whole archive to the disk:
a program reads each entry and decides where it goes.

## Reading

| Signature | What it does |
|---|---|
| `Result<ZipReader, ZipError> ZipReader.open(read string path)` | The archive in the file at `path`, checked |
| `Result<ZipReader, ZipError> ZipReader.of(StreamReader source)` | The archive a reader of a file holds (it seeks, so not a pipe) |
| `List<ZipEntry> entries(read this)` | The entries, in the order of the archive: a list of the program's own |
| `Result<List<byte>, ZipError> readBytes(ref this, read ZipEntry entry, nint limit)` | The bytes of an entry, at most `limit` |
| `Result<void, ZipError> copyTo(ref this, read ZipEntry entry, ref StreamWriter target, long limit)` | Writes the bytes of an entry to `target`, a block at a time |

A `ZipEntry` has `name` (the path in the archive, with `/` between folders; a folder's ends in `/`), `size`
(the bytes it opens into), `compressedSize`, `modified` (a `DateTime` in UTC), `isDirectory` and `method` (0
stored, 8 deflated, another number for a method that is not read). `entries()` gives a list of its own, so the
reader can read while the program goes over it.

## Writing

| Signature | What it does |
|---|---|
| `ZipWriter ZipWriter.create(StreamWriter target)` | A writer of an archive into `target` |
| `Result<void, ZipError> add(ref this, read string name, read byte[] data, ZipMethod method, DateTime modified)` | An entry that holds `data` |
| `Result<void, ZipError> addFrom(ref this, read string name, ref StreamReader source, ZipMethod method, DateTime modified)` | An entry that holds what is left in `source`, read a block at a time |
| `Result<void, ZipError> addDirectory(ref this, read string name, DateTime modified)` | A folder; the `/` is added when it is not there |
| `Result<void, ZipError> close(this)` | Writes the central directory and closes `target` |

`ZipMethod.Deflated` compresses at level 6, zlib's default; `ZipMethod.Stored` keeps the bytes as they are.
Each entry takes the time it was last changed: a program that archives files gives `FileInfo.modified`, a build
that wants the same bytes every time gives a fixed time. The time is written for DOS in UTC and again in the
field of Unix time, so the same entries give the same bytes on every machine. A writer refuses the names a
reader refuses, so what SG writes SG reads; like a `StreamWriter`, a local one is closed before it ends.

```sg
Result<void, ZipError> write(read string path) {
    var when = DateTime.utc(2026, 11, 20, 12, 0, 0) ?? DateTime.fromUnixSeconds(0)
    var zip = ZipWriter.create(File.createText(path) ?? panic("cannot write"))
    try zip.addDirectory("notes", when)
    try zip.add("notes/todo.txt", "write the reference\n".repeat(20).asBytes(), ZipMethod.Deflated, when)
    try zip.add("notes/çay.txt", "two sugars\n".asBytes(), ZipMethod.Stored, when)
    return zip.close()
}

Result<void, ZipError> main() {
    var path = "notes.zip"
    try write(read path)
    var zip = try ZipReader.open(read path)
    for entry in zip.entries() {
        print($"{entry.name}: {entry.size} bytes, {entry.compressedSize} in the archive, {entry.modified}")
        if !entry.isDirectory {
            var bytes = try zip.readBytes(read entry, 1 << 20)
            var text = string.fromUtf8(read bytes) ?? ""
            nint end = text.indexOf('\n') ?? text.length()
            print($"  the first line: {text[..end]}")
        }
    }
    _ = File.delete(read path)
    return Ok()
}
```

It prints:

```text
notes/: 0 bytes, 0 in the archive, 2026-11-20T12:00:00Z
notes/todo.txt: 400 bytes, 27 in the archive, 2026-11-20T12:00:00Z
  the first line: write the reference
notes/çay.txt: 11 bytes, 11 in the archive, 2026-11-20T12:00:00Z
  the first line: two sugars
```

## Errors

`ZipError` says why an archive could not be read or written:

| Variant | When |
|---|---|
| `NotZip` | No end of a central directory: the bytes are not a zip archive |
| `Corrupt(long position)` | The archive is not what the format says, at this byte of it |
| `Truncated` | The archive ends before what it holds does |
| `InsecurePath(string name)` | A name could lead out of the folder it is unpacked in |
| `DuplicateName(string name)` | Two entries have this name (a file and a folder of one name too) |
| `Overlap(string name)` | The entry's data lies over another entry's |
| `NameMismatch(string name)` | The header before the entry's data names another entry |
| `TooLarge` | The entry is larger than the limit |
| `SizeMismatch(string name)` | The entry opens into another number of bytes than its size in the archive |
| `ChecksumMismatch(string name)` | The entry's CRC-32 is not the one in the archive |
| `Encrypted` | The entry is encrypted; encryption is not read |
| `UnsupportedMethod(int code)` | The entry is compressed by another method than stored or deflated |
| `Io(IoError error)` | Reading or writing the stream failed |

A name could lead out of a folder when it is empty, begins with `/`, holds a `\`, a `:` or a NUL, has a part
`..`, or has a part that Windows keeps for a device (`CON`, `PRN`, `AUX`, `NUL`, `COM1` to `COM9`, `LPT1` to
`LPT9`, with an extension or not). The rule is the same on every system, so an archive that is safe on Linux is
safe on Windows too.

```sg
void show(read string what, Result<void, ZipError> result) {
    match result {
        Ok => print($"{what}: written")
        Error(e) => print($"{what}: {e}")
    }
}

Result<void, ZipError> main() {
    var when = DateTime.fromUnixSeconds(0)
    var zip = ZipWriter.create(File.createText("names.zip") ?? panic("cannot write"))
    show("../escape", zip.add("../escape", "x".asBytes(), ZipMethod.Stored, when))
    show("aux.txt", zip.add("logs/aux.txt", "x".asBytes(), ZipMethod.Stored, when))
    show("a.txt", zip.add("a.txt", "x".repeat(5000).asBytes(), ZipMethod.Deflated, when))
    show("a.txt again", zip.add("a.txt", "x".asBytes(), ZipMethod.Stored, when))
    try zip.close()

    var reader = try ZipReader.open("names.zip")
    var entries = reader.entries()
    match reader.readBytes(read entries[0], 1000) {
        Ok(bytes) => print($"{bytes.length()} bytes")
        Error(e) => print(e)
    }
    _ = File.delete("names.zip")
    return Ok()
}
```

It prints:

```text
../escape: the name "../escape" could lead out of the folder it is unpacked in
aux.txt: the name "logs/aux.txt" could lead out of the folder it is unpacked in
a.txt: written
a.txt again: two entries are named "a.txt"
the entry is larger than the limit
```

A name in a message is quoted, and what is not seen or drives a terminal is escaped, so a name in an archive
cannot forge what a program prints.

## How fast

70,000 small entries are read in 31 ms and written in 96 ms on an Apple M3 Max; Go's `archive/zip` takes
90 ms and 42 ms. Writing takes zlib's own time or less: zlib in C, compressing the same entries with nothing of
zip around them, takes 125 ms, half of it building the trees of Huffman for every block, which its bytes need.
Go's compressor is another one and makes other bytes. A large entry opens at the speed of
[`Deflate`](./compression.md#how-fast), whose decoder takes about 2.6 times zlib's time.

Not yet: encryption, methods other than stored and deflated (bzip2, LZMA, Zstandard), the comments of an
archive, the modes of Unix and links, and an archive held in memory.
