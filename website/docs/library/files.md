---
title: Files and folders
description: File, StreamReader and StreamWriter, the standard streams, Directory and watching a folder, Path, FileInfo and IoError.
sidebar_position: 4
---

# Files and folders

`File` reads and writes whole files and opens them as streams. A `StreamReader` reads a file, the input of the
program or a connection through a block of its own; a `StreamWriter` writes through one. `Directory` makes,
lists, removes and watches folders, and `Path` joins paths and takes them apart. Every call that asks the system gives a
`Result` whose error is an `IoError`.

A path is a `read string`: a literal goes in as it is, and a `string` variable is lent with `read path`. To use
a file, open it and look at the error rather than asking first whether it is there: what the system says can
change the next moment.

The programs on this page write into the folder they run in and remove what they made at the end.

## Whole files

| Signature | What it does |
|---|---|
| `Result<string, IoError> File.readText(read string path)` | The text of the file, checked to be UTF-8 |
| `Result<List<byte>, IoError> File.readBytes(read string path)` | The bytes of the file |
| `Result<void, IoError> File.writeText(read string path, read string text)` | Makes the file, or empties it, and writes the text |
| `Result<void, IoError> File.writeBytes(read string path, read byte[] bytes)` | The same with bytes |
| `Result<void, IoError> File.appendText(read string path, read string text)` | Writes after what the file holds; a missing file is made |
| `Result<void, IoError> File.delete(read string path)` | Removes the file |
| `Result<void, IoError> File.move(read string from, read string to)` | Renames it; never takes the place of a file (`AlreadyExists`, told in the same step) |
| `Result<void, IoError> File.replace(read string from, read string to)` | Renames it over `to`, in one step, even while `to` is open, on Windows as well |
| `Result<void, IoError> File.copy(read string from, read string to)` | A new file with the bytes and the permissions; `to` must not be there |
| `Result<FileInfo, IoError> File.info(read string path)` | What the system knows of it, a link followed |
| `bool File.exists(read string path)` | Whether the path names a file (a folder is not one) |
| `Result<string, IoError> File.createTemp()` | A new empty file in `Directory.temp()` that only the user can read and write, and its path |
| `Result<StreamWriter, IoError> File.openTemp()` | A writer of a new file with no name, gone when it is closed (see [A file with no name](#a-file-with-no-name)) |
| `Result<void, IoError> File.createLink(read string target, read string path)` | A symbolic link at `path` that holds `target` as it is given (see [Links and permissions](#links-and-permissions)) |
| `Result<void, IoError> File.createHardLink(read string existing, read string path)` | One more name of the file |
| `Result<string, IoError> File.readLink(read string path)` | What a symbolic link holds; a path that is no link is `InvalidInput` |
| `Result<FileInfo, IoError> File.linkInfo(read string path)` | What the system knows of the path itself, a link not followed (its `kind` is `Link`) |
| `Result<void, IoError> File.setPermissions(read string path, int mode)` | The bits of the mode of Unix, `0o644`; on Windows only whether it may be written |
| `Result<MappedFile, IoError> unsafe File.mapRead(read string path)` | The file put in the memory of the program to be read (see [A file in memory](#a-file-in-memory)) |

```sg
Result<void, IoError> main() {
    var path = "notes.txt"
    try File.writeText(read path, "one\ntwo\n")
    try File.appendText(read path, "three\n")
    var text = try File.readText(read path)
    for line in text.lines() {
        print(line)
    }
    var bytes = try File.readBytes(read path)
    print(bytes[..3])
    var info = try File.info(read path)
    print($"{info.kind} {info.size} {File.exists(read path)}")
    match File.readText("missing.txt") {
        Ok(_) => print("there")
        Error(cause) => print(cause)
    }
    try File.delete(read path)
    print(File.exists(read path))
    return Ok()
}
```

It prints:

```text
one
two
three
[111, 110, 101]
File 14 true
not found
false
```

`Result<void, IoError> main()` lets `try` pass every error on: the program prints it and ends with code 1.

### Links and permissions

`File.createLink(target, path)` makes a symbolic link, as `ln -s target path` does: the target is written into
the link as it is given, and a target that is not a whole path is looked for from the folder of the link
(choices 1973 to 1976). Every call that opens a file follows a link; `File.linkInfo` and `Directory.list` see
the link itself, `File.readLink` reads what it holds, and `File.delete` removes the link and not what it points
at. A link to nothing can be made; `File.info` of it is `NotFound`. `File.createHardLink(existing, path)` gives
the file a second name: both are the same file, and it is gone when the last name is. On Windows a link is made
without the rights of an administrator only in the mode of a developer (`PermissionDenied` otherwise), is a
link of a folder when its target is a folder, and has its `/` written `\`; a junction is no link there
(`linkInfo` says `Other`).

`FileInfo.permissions` holds the bits of the mode of Unix, which SG writes in octal: `0o644` is a file its owner
reads and writes and the others read, `0o755` a program or a folder anybody may run or go into (choices 1977,
1978). `File.setPermissions(path, mode)` sets them, through a link; a mode outside `0` to `0o7777` is
`InvalidInput`. Windows keeps only whether a file may be written: a mode without the bit of the owner's writing
(`0o200`) makes it a file that may only be read, and a file there reads as `0o666` or `0o444`, a folder as `0o777`
or `0o555`. Who owns a file is not changed here yet.

```sg
Result<void, IoError> main() {
    var folder = try Directory.createTemp()
    var file = Path.join(read folder, "notes.txt")
    try File.writeText(read file, "hello")
    var link = Path.join(read folder, "latest")
    try File.createLink("notes.txt", read link)
    print(try File.readLink(read link))
    print(try File.readText(read link))
    var itself = try File.linkInfo(read link)
    print(itself.kind)
    try File.setPermissions(read file, 0o444)
    var info = try File.info(read file)
    print((info.permissions & 0o200) != 0)
    match File.writeText(read file, "again") {
        Ok => print("written")
        Error(cause) => print(cause)
    }
    try File.setPermissions(read file, 0o644)
    try Directory.deleteAll(read folder)
    return Ok()
}
```

It prints:

```text
notes.txt
hello
Link
false
permission denied
```

### A file in memory

`File.mapRead(path)` puts a file in the memory of the program to be read (`mmap`, `MapViewOfFile`; choice
1980): `map.bytes()` lends its bytes as a `read byte[]`, the system reads each page from the disk the first time
it is read, and nothing is copied. Where the `MappedFile` ends the file is taken out of the memory again, and the
bytes it lent go with it, so a borrow of them does not outlive it. The call is written inside `unsafe`, since the
bytes are the file's as it is on the disk: another program that writes into the file changes them while they are
read, and one that cuts it short stops this program when it reads past the new end (`SIGBUS`; Windows lets no
program cut short a file that is mapped). A file that only this program writes, or that nobody changes, is read
this way safely. An empty file gives no bytes, a folder is `IsDirectory`, and anything that is not a regular file
is `NotAFile`, as for `File.openRegular`. A `MappedFile` may go to another task. There is no mapping to write yet. Measured, a file of 4 KB is read
faster with `readBytes` than it is mapped, one of 1 MB costs the same, and one of 64 MB half (macOS) or two
fifths (Linux) less, since nothing is copied.

```sg
Result<void, IoError> main() {
    var folder = try Directory.createTemp()
    var path = Path.join(read folder, "data.txt")
    try File.writeText(read path, "one two three")
    var map = try unsafe File.mapRead(read path)
    var bytes = map.bytes()
    var spaces = 0
    for b in bytes {
        if b == (byte)' ' {
            spaces += 1
        }
    }
    print($"{bytes.length()} bytes, {spaces} spaces")
    try Directory.deleteAll(read folder)
    return Ok()
}
```

It prints:

```text
13 bytes, 2 spaces
```

## `StreamWriter`

| Signature | What it does |
|---|---|
| `Result<StreamWriter, IoError> File.createText(read string path)` | A writer of a new or emptied file |
| `Result<StreamWriter, IoError> File.openAppend(read string path)` | One that writes after what the file holds |
| `Result<StreamWriter, IoError> File.openWrite(read string path)` | One that writes from the start over what the file holds; a missing file is made |
| `Result<StreamWriter, IoError> File.openUpdate(read string path)` | The same of a file that is there, opened to be read too (see [One file, read and written](#one-file-read-and-written)) |
| `Result<void, IoError> write(ref this, x)`, `writeLine(ref this, x)` | Writes whatever `print` takes, with or without a line end |
| `Result<void, IoError> writeBytes(ref this, read byte[] bytes)` | Writes the bytes as they are |
| `Result<void, IoError> writePieces(ref this, read List<read byte[]> pieces)` | Writes every piece in turn, as `writeBytes` of each would; what the block has no room for goes to the file with one call of the system (`writev`) on Linux and macOS |
| `Result<void, IoError> flush(ref this)` | Hands what the block holds to the system |
| `Result<void, IoError> close(this)` | Writes what is left and closes the file |
| `Result<long, IoError> seek(ref this, SeekFrom to)` | Writes what it holds, then moves; gives the new place |
| `Result<long, IoError> position(read this)` | Where the next write starts: where the file stands and what the writer holds beyond it; nothing is written |
| `Result<StreamReader, IoError> openReader(read this)` | A reader of the file it writes, for a writer of `File.openUpdate` or `File.openTemp` |
| `Result<void, IoError> syncData(ref this)` | Writes what it holds, then waits until the system has put the file's bytes on the disk, with what is needed to read them back |
| `Result<void, IoError> sync(ref this)` | The same, and the rest of what the system keeps of the file, its times among it |
| `Result<void, IoError> setLength(ref this, long length)` | Writes what it holds, then cuts the file to `length` bytes or makes it longer with zeros; where the writer stands does not move |
| `bool isTerminal(read this)` | Whether it writes to a terminal (see [below](#stdin-stdout-and-stderr)) |

### On the disk

`flush` hands the bytes to the system, which keeps them in memory a while: a program that is killed loses
nothing then, but a machine that loses its power may. `syncData` waits until they are on the disk: `fdatasync`
on Linux, `F_FULLFSYNC` on macOS, whose `fsync` leaves them in the disk's own cache, and `FlushFileBuffers` on
Windows, as Rust's `sync_data` is. A file made, renamed or removed is only certain once its folder is synced
too, with `Directory.sync`. This is how a program writes a file that a crash may not leave half written:

```sg
Result<void, IoError> save(read string folder, read string text) {
    var path = Path.join(folder, "settings.json")
    var fresh = Path.join(folder, "settings.json.new")
    var out = try File.createText(read fresh)
    try out.write(text)
    try out.syncData()
    try out.close()
    try File.replace(read fresh, read path)
    return Directory.sync(folder)
}

Result<void, IoError> main() {
    var folder = try Directory.createTemp()
    try save(read folder, "{\"theme\": \"dark\"}")
    print(try File.readText(read Path.join(read folder, "settings.json")))
    try Directory.deleteAll(read folder)
    return Ok()
}
```

It prints:

```text
{"theme": "dark"}
```

A sync that failed stays the writer's error, given again by every call after it: the system may have let go of
the bytes it could not write, so a later sync that succeeded would not mean they are on the disk. `Stdout`,
`Stderr` and the input of another program are not files the program holds: `syncData` and `sync` on them write
what they hold and are `InvalidInput` (written as `Stdout.syncData()`, an error of the compiler), `setLength`
is `NotSeekable`. `setLength` below 0 is `InvalidInput`. In an async function they are `syncDataAsync`,
`syncAsync` and `setLengthAsync`, done on a thread that may wait for the disk.

A writer kept in a local variable is closed, or moved somewhere, on every path before it ends: otherwise an
error writing its last block would be lost without a word.

```sg error
Result<void, IoError> main() {
    var out = try File.createText("x.txt")
    try out.writeLine("hi")
    return Ok()
}
```

```text
error: `out` is not closed at this `return`, so an error writing its buffer would be lost
  --> example.sg:4:5
 4 |     return Ok()
   |     ^^^^^^^^^^^
  it is opened here:
  --> example.sg:2:9
 2 |     var out = try File.createText("x.txt")
   |         ^^^
  hint: `try out.close()`; to drop the error on purpose, `_ = out.close()`
```

An exit with an error (`try`) lets the writer go: it writes its block as it ends.

### One file, read and written

`File.openUpdate(path)` opens a file that is there to be written and read (choices 2073 to 2075): it is not
emptied, a missing one is `NotFound`, and the writer starts at its beginning. `writer.openReader()` gives a
`StreamReader` of the same open file. The reader has a place of its own, so reading does not move where the writer
writes, and there may be several.

The two keep to the rule of a stream of C opened with `"r+"`: the reader sees what the writer has **flushed**, and
what the reader's own block holds stays until it **seeks**. A seek of such a reader, also one to where it stands
(`SeekFrom.Current(0)`), lets its block go, so the next read asks the file again. The reader lives on after its
writer is closed.

`position()` on a reader or a writer says where the next read or write starts, counted from the start of the
file. Nothing moves, and a writer writes nothing: what it still holds is counted in. `Stdin`, `Stdout`, `Stderr` and
the reader of a connection have no place (a compile error, as for `seek`); a pipe and a writer of
`File.openAppend` are `NotSeekable`. Neither call waits, so neither has an async form.

Only a writer of `File.openUpdate` or `File.openTemp` gives a reader: of `createText`, `openWrite` and `openAppend`,
whose files are opened to be written alone, `openReader` is `InvalidInput`.

```sg
Result<void, IoError> main() {
    var folder = try Directory.createTemp()
    var path = Path.join(read folder, "notes.txt")
    try File.writeText(read path, "one\ntwo\n")

    var out = try File.openUpdate(read path)
    var reader = try out.openReader()
    var line = ""
    _ = try reader.readLine(ref line)
    print(line)

    try out.write("ONE")
    var held = try out.position()
    print($"the writer stands at {held}")
    try out.flush()

    _ = try reader.seek(SeekFrom.Start(0))
    _ = try reader.readLine(ref line)
    print(line)
    var place = try reader.position()
    print($"the reader stands at {place}")

    try out.close()
    try Directory.deleteAll(read folder)
    return Ok()
}
```

It prints:

```text
one
the writer stands at 3
ONE
the reader stands at 4
```

The first `readLine` gives `one`, as the file was. `write("ONE")` stays in the writer's block; its `position()` is
3 all the same. After `flush` the file starts with `ONE`, and the reader, moved back to the start, reads it: the
seek let go of the block that still held `one`.

### A file with no name

`File.openTemp()` makes a new file in `Directory.temp()` that has **no name** (choice 2076), and gives its writer;
`openTempAsync()` in an async function. What was written is read back through `writer.openReader()`. The file is
gone when its last stream is closed, also when the program fails or is ended from outside, so there is nothing to
remove and nothing another program can find by name. `File.createTemp()` is for a file that needs a name.

Linux makes the file without a name (`O_TMPFILE`); where the file system has none of that, and on macOS, it is made
as `mkstemp` makes one and its name is taken away at once. On Windows the file has a name in the folder while it is
open, and Windows deletes it when the last handle is closed (`FILE_FLAG_DELETE_ON_CLOSE`).

```sg
Result<void, IoError> main() {
    var out = try File.openTemp()
    for i in 0..3 {
        try out.writeLine($"line {i}")
    }
    try out.flush()

    var reader = try out.openReader()
    for line in reader.lines() {
        print(try line)
    }
    try out.close()
    return Ok()
}
```

It prints:

```text
line 0
line 1
line 2
```

## `StreamReader`

| Signature | What it does |
|---|---|
| `Result<StreamReader, IoError> File.openText(read string path)` | A reader of the file |
| `Result<StreamReader, IoError> File.openRegular(read string path)` | The same, but only of a regular file: a folder is `IsDirectory`, anything else `NotAFile` |
| `Result<bool, IoError> readLine(ref this, ref string line)` | The next line into `line`, without its `\n` or `\r\n`; `false` at the end |
| `Result<bool, IoError> readLineBytes(ref this, ref List<byte> line)` | The same with the bytes of the line as they are, none checked to be UTF-8, for a search with `isMatchBytes` |
| `Result<string, IoError> readText(ref this)` | All that is left |
| `lines(ref this)` | Each line as a `Result<string, IoError>`, for a `for` |
| `Result<nint, IoError> readBytes(ref this, ref byte[] buffer)` | Fills the buffer unless the end comes first; how many bytes |
| `Result<nint, IoError> readSome(ref this, ref byte[] buffer)` | What the block holds, or what one read of the system gives; 0 at the end |
| `Result<bool, IoError> fill(ref this)` | One more read of the system behind what the block holds; `false` at the end |
| `read byte[] buffered(read this)` | What the block holds and has not been given out, lent where it lies |
| `consume(ref this, nint n)` | Gives out the first `n` bytes of the block; more than it holds is a panic |
| `Result<long, IoError> seek(ref this, SeekFrom to)` | Moves, inside its block when it can; gives the new place |
| `Result<long, IoError> position(read this)` | Where the next read starts; nothing moves (see [One file, read and written](#one-file-read-and-written)) |
| `Result<FileInfo, IoError> info(read this)` | What the system knows of the file it reads, asked of the open file |
| `bool isTerminal(read this)` | Whether it reads from a terminal (see [below](#stdin-stdout-and-stderr)) |

Text that is not UTF-8 is `IoError.InvalidText(position)`, counted from the start of the file; a program
that must go on past such a line, as `grep` does, reads it with `readLineBytes`. `SeekFrom` is
`Start(long offset)`, `End(long offset)` or `Current(long offset)`; a place before the start is
`InvalidInput`.

`File.openRegular` opens only a regular file (choice 1873). A folder is `IsDirectory`, and a pipe, a
device or a socket is `NotAFile`, refused before it is opened: a FIFO in a folder a server sends files
from is never opened, so the server does not wait on it, and a writer waiting for a reader of it is not
let go. (On Windows a device is known only by its handle, so it is opened and closed again.) `info()`
is `File.info` of the open file, as Rust's `File::metadata` and Go's `f.Stat()` are: what it says is of
the file the reader reads, also after its path has been given to another, and it walks no path, so it
waits for nothing and an async function calls it as it is. A server opens a file with `openRegular` and
asks the reader for its size and its time, which asks the disk once less than `File.info` and then
`openText`. `Stdin` and the reader of a connection have no `info()`.

### The lock of a file

A reader and a writer of a file can lock it, so that programs which ask for the lock take turns (choice 1979):

| Signature | What it does |
|---|---|
| `Result<void, IoError> lock(ref this)` | The whole file for this stream alone, waited for while another holds a lock of it |
| `Result<void, IoError> lockShared(ref this)` | The same, shared with others that lock it shared |
| `Result<bool, IoError> tryLock(ref this)` | As `lock`, but `false` at once where another holds a lock |
| `Result<bool, IoError> tryLockShared(ref this)` | As `lockShared`, but `false` at once where another holds it alone |
| `Result<void, IoError> unlock(ref this)` | Lets the lock go; a writer writes what it holds first, and lets go also when that fails |

The lock is of the whole file and is of the opening, not of the program: two openings of one file in one program
are two holders. It goes when the stream ends, and when the program ends however it ends. A second lock changes
the kind; the old one is let go first, so a `tryLock` that fails after a shared lock holds none, as with `flock`.
It is a lock for programs that ask for it (`flock` on Linux and macOS): one that reads or writes the file without
asking is not stopped. On Windows (`LockFileEx`) another opening that reads or writes a locked file is refused
there, in this program or another; a shared lock refuses writes there to its own opening too. The input of the program, a standard stream and the pipe of another program are `NotAFile`.

In an async function `lockAsync()` and `lockSharedAsync()` wait for it, with a token last if they are given one
(choice 1983): the lock is tried, and while another holds it the task waits on the clock, from 1 ms up to 10, and
tries again. A cancel ends the wait at once and no lock is taken after it; no thread is held while it waits, and
the order in which programs waited is not kept. `tryLock`, `tryLockShared` and a reader's `unlock` wait for
nothing and are called as they are; a writer's `unlockAsync` writes what it holds first (choice 1985).

```sg
Result<void, IoError> main() {
    var folder = try Directory.createTemp()
    var path = Path.join(read folder, "store.db")
    try File.writeText(read path, "")
    var first = try File.openWrite(read path)
    var second = try File.openText(read path)
    print(try first.tryLock())
    print(try second.tryLockShared())
    try first.write("data")
    try first.unlock()
    print(try second.tryLockShared())
    print(try second.readText())
    try first.close()
    try Directory.deleteAll(read folder)
    return Ok()
}
```

It prints:

```text
true
false
true
data
```

### Reading from the block

A parser that finds its own ends (a length in front, a `\r\n\r\n`, a byte that closes a record) reads
the block of the reader where it lies, with nothing copied out of it: `buffered()` lends what the block
holds, the parser takes what is whole, `consume(n)` gives that out, and `fill()` reads more when what is
left is only part of one. `fill` always reads more, also when the block holds bytes, so a parser that
found half a record does not go round for nothing. The borrow `buffered` gives keeps the reader from
being read, filled or consumed while it is used.

The block of a file, of `Stdin` and of the pipe of another program holds 64 KB; that of a connection
starts at 4 KB and doubles up to its limit (`conn.reader(limit)`, 64 KB unless given). `fill` on a block
that holds its limit already is `IoError.TooLong`, until `consume` makes room. `readLine` and the other
reads go on from the same block.

`Stdin` is one reader for every task, with one block. The first task that calls `fill`, `buffered` or
`consume` on it holds the input, and a read of `Stdin` by any other task after that is a panic, so the bytes
a task looks at do not change under it. Tasks that only read lines of the input share it until then.

```sg
// Records of a length byte and that many bytes.
Result<void, IoError> main() {
    var path = "records.bin"
    var out = try File.createText(read path)
    try out.writeBytes("\u{3}one\u{5}three\u{2}up".asBytes())
    try out.close()

    var reader = try File.openText(read path)
    while true {
        var view = reader.buffered()
        if view.length() > 0 && view.length() > view[0] {
            nint size = view[0]
            print(string.fromUtf8(view[1..size + 1]) ?? "not text")
            reader.consume(size + 1)
            continue
        }
        if !try reader.fill() {
            break
        }
    }
    try File.delete(read path)
    return Ok()
}
```

It prints:

```text
one
three
up
```

```sg
struct Point : Formattable {
    int x
    int y
}

Result<void, IoError> main() {
    var path = "points.txt"
    var out = try File.createText(read path)
    try out.writeLine("# points")
    try out.writeLine(Point { x = 1, y = 2 })
    try out.writeBytes("end\n".asBytes())
    try out.close()

    var reader = try File.openText(read path)
    var line = string.withCapacity(80)
    while try reader.readLine(ref line) {
        print($"[{line}]")
    }

    var again = try File.openText(read path)
    _ = try again.seek(SeekFrom.Start(2))
    var buffer = byte[6]
    var count = try again.readBytes(ref buffer)
    print($"{count} {buffer}")
    print(try again.seek(SeekFrom.End(-4)))
    for item in again.lines() {
        print(try item)
    }
    try File.delete(read path)
    return Ok()
}
```

It prints:

```text
[# points]
[Point { x = 1, y = 2 }]
[end]
6 [112, 111, 105, 110, 116, 115]
32
end
```

Each call has an async version for an async function: `File.readTextAsync`, `readBytesAsync`,
`writeTextAsync`, `writeBytesAsync`, `appendTextAsync`, `openTextAsync`, `openRegularAsync`, `createTextAsync`, `openAppendAsync`;
on a reader `readLineAsync`, `readLineBytesAsync`, `readTextAsync`, `readBytesAsync`, `readSomeAsync`,
`fillAsync`, `lockAsync`, `lockSharedAsync`; on a writer `writeAsync`, `writeLineAsync`, `writeBytesAsync`, `writePiecesAsync`, `flushAsync`, `closeAsync`,
`syncDataAsync`, `syncAsync`, `setLengthAsync`, `seekAsync`, `lockAsync`, `lockSharedAsync`, `unlockAsync`. Each may take a `read CancellationToken` last (not
`closeAsync`). In an async function the calls without `Async` are an error, since they would hold the thread.
`buffered` and `consume` do not wait, and neither does a reader's `seek`, which asks the system where the
file stands and reads nothing, nor its `info`: they are called as they are.

The calls of the file system have async versions too (choice 1871), done on a thread of input and output
while the task waits: `File.infoAsync`, `existsAsync`, `deleteAsync`, `moveAsync`, `replaceAsync`, `copyAsync`,
`createTempAsync`, `createLinkAsync`, `createHardLinkAsync`, `readLinkAsync`, `linkInfoAsync`, `setPermissionsAsync`,
`mapReadAsync` (written `await unsafe File.mapReadAsync(path)`), and `Directory.createAsync`, `createAllAsync`, `deleteAsync`, `deleteAllAsync`, `syncAsync`,
`listAsync`, `existsAsync`, `createTempAsync`, each giving what the call without `Async` gives and taking a token
last. On a local disk with its cache warm such a call takes a microsecond; on a disk of the network, a cold one
or one that hangs it can take as long as a read, and every task of the thread would wait with it, so in an
async function the calls without `Async` are an error here too, also through a function that is not async.
`Directory.current()` and `Directory.temp()` ask the process, not the disk, and are called as they are.

A thread taken for each call costs a hand-over, as it costs `tokio::fs`. On Linux, compiled, `File.infoAsync`
(and `existsAsync`, `Directory.existsAsync`), `File.openTextAsync`, `openRegularAsync` and a file reader's `readSomeAsync` are first
tried on the task's own thread in the forms the kernel answers only from what it already holds: a path found from
its cache alone (`openat2` with `RESOLVE_CACHED`), a read of what its cache holds (`preadv2` with `RWF_NOWAIT`).
Where it would wait, or has neither (a kernel before 5.12, a filter of system calls, overlayfs and tmpfs, which
have no `RWF_NOWAIT`), the call goes to the thread as before, so a disk that is slow still holds no task's thread
(choice 1872). A file server answers a file it has served before without a hand-over.

```sg
async Result<void, IoError> main() {
    var folder = try await Directory.createTempAsync()
    var path = Path.join(read folder, "notes.txt")
    try await File.writeTextAsync(read path, "one\n")
    var info = try await File.infoAsync(read path)
    print($"{info.kind} {info.size}")
    print(await File.existsAsync(read path))
    try await Directory.deleteAllAsync(read folder)
    print(await Directory.existsAsync(read folder))
    return Ok()
}
```

It prints:

```text
File 4
true
false
```

## `Stdin`, `Stdout` and `Stderr`

`Stdin` is a `StreamReader` of the input of the program, `Stdout` and `Stderr` are `StreamWriter`s. They are
lent, never moved or closed: `ref Stdout` gives one to a function that takes a `ref StreamWriter`. `print(x)`
writes to `Stdout` and `printError(x)` to `Stderr`. `Stdin` cannot `seek` (`NotSeekable`). Into a pipe whose
reader is gone (`program | head`), `Stdout` and `Stderr` end the program as `print` and `printError` do: by
`SIGPIPE` on Linux and macOS; on Windows `Stdout` ends it with 0 and `Stderr` gives the error (choice 2055).

`isTerminal()` tells whether a stream is a terminal (choice 1901), as C's `isatty` and Rust's `IsTerminal`
do: a tool that writes bytes refuses a terminal, and one that colours its output colours it only there.
Every reader and writer has it, and a file opened on `/dev/tty` is one; a connection and the pipes of
another program never are. It moves nothing, so a `read` borrow asks it, and it does not wait, so an async
function calls it as it is. On Windows it asks whether the stream is a console.

```sg
void main() {
    if Stdout.isTerminal() {
        printError("this writes a GIF, not text: send it into a file or a pipe")
        exit(1)
    }
    printError($"the input is typed: {Stdin.isTerminal()}")
}
```

```sg
Result<void, IoError> count(ref StreamWriter out, int upTo) {
    for i in 0..upTo {
        try out.write($" {i}")
    }
    try out.writeLine("")
    return Ok()
}

Result<void, IoError> main() {
    try Stdout.writeLine("to the output")
    try count(ref Stdout, 3)
    try Stderr.writeLine("to the standard error")
    var lines = 0
    var line = string.withCapacity(80)
    while try Stdin.readLine(ref line) {
        lines += 1
    }
    print($"{lines} lines of input")
    return Ok()
}
```

Run with no input and the standard error left out, it prints:

```text
to the output
 0 1 2
0 lines of input
```

## `Directory`

| Signature | What it does |
|---|---|
| `Result<void, IoError> Directory.create(read string path)` | Makes the folder; the one it is in must be there; `AlreadyExists` if something is |
| `Result<void, IoError> Directory.createAll(read string path)` | Makes it and every folder on the way; a folder already there is no error |
| `Result<void, IoError> Directory.delete(read string path)` | Removes an empty folder (`DirectoryNotEmpty`) |
| `Result<void, IoError> Directory.deleteAll(read string path)` | Removes it and all it holds; a link inside is removed, not followed |
| `Result<void, IoError> Directory.sync(read string path)` | Puts the names it holds on the disk, so that a file made, renamed or removed in it stays so (see [On the disk](#on-the-disk)); on Windows, which keeps them with the files, nothing more than `NotDirectory` for a path that is not a folder |
| `Result<List<DirectoryEntry>, IoError> Directory.list(read string path)` | The names in it, without `.` and `..`, in the order of their bytes on every system |
| `bool Directory.exists(read string path)` | Whether the path names a folder |
| `Result<string, IoError> Directory.current()` | The folder the program works in; it is never changed from inside |
| `string Directory.temp()` | The folder for files that do not have to stay: `TMPDIR` or `/tmp`, on Windows the one Windows gives |
| `Result<string, IoError> Directory.createTemp()` | A new folder in it that only the user can go into, and its path |
| `Result<DirectoryRoot, IoError> Directory.openRoot(read string path)` | The folder held open, and the only place its paths lead to (see [A folder as a root](#a-folder-as-a-root)) |

A `DirectoryEntry` has `name` (without the folder) and `kind`; a name that is not UTF-8 comes as
[below](#names-that-are-not-text), and opens the same file. A `FileInfo` has `kind`, `size` (in bytes, a
`long`), `modified` (a [`DateTime`](./time.md)) and `permissions` (an `int`, see [Links and
permissions](#links-and-permissions)). `FileKind` is `File`, `Directory`, `Link` or `Other`; only a listing and
`File.linkInfo` tell a `Link`, since `File.info` follows links.

```sg
Result<void, IoError> main() {
    var root = "work-folder"
    try Directory.createAll(read Path.join(read root, "logs/old"))
    try File.writeText(read Path.join(read root, "b.txt"), "b")
    try File.writeText(read Path.join(read root, "a.txt"), "a")
    for entry in try Directory.list(read root) {
        print($"{entry.name} {entry.kind}")
    }
    match Directory.delete(read root) {
        Ok => print("removed")
        Error(cause) => print(cause)
    }
    var a = Path.join(read root, "a.txt")
    var b = Path.join(read root, "b.txt")
    match File.move(read a, read b) {
        Ok => print("moved")
        Error(cause) => print(cause)
    }
    try File.replace(read a, read b)
    print(try File.readText(read b))
    try Directory.deleteAll(read root)
    print(Directory.exists(read root))
    return Ok()
}
```

It prints:

```text
a.txt File
b.txt File
logs Directory
folder not empty
already exists
a
false
```

### Watching a folder

| Signature | What it does |
|---|---|
| `Result<DirectoryWatcher, IoError> Directory.watch(read string path)` | Watches what is in the folder; a file is `NotDirectory` |
| `Result<DirectoryWatcher, IoError> Directory.watchAll(read string path)` | Watches the whole tree, folders made or moved in later too |
| `Result<FileChange, IoError> watcher.next()` | Waits for the next change and gives it |
| `async Result<FileChange, IoError> watcher.nextAsync(read token)` | The same without holding the thread; a cancel ends the wait (`Cancelled`) |

Every change after the call returns comes, in the order it came; nothing is told of what was there before. A
`FileChange` is `Created(path)`, `Deleted(path)`, `Changed(path)` or `Lost(path)`, the path being the folder as
it was given joined with the name. The same on Linux, macOS and Windows:

- A rename is the old name deleted and the new one made: not every system can tell which two go together.
- `Changed` is a file written or its attributes changed; a folder is never changed.
- Under `watchAll`, what is in a folder that comes in is told as made, name by name; a link is a name, and what it
  points at is not watched.
- A file made and written may come as made and changed, or only as made (macOS gathers what happens to a file).
- The same change twice in a row before it is taken is one, and at most 16,384 wait: beyond that they are dropped,
  and one `Lost` of the folder says so. `Lost` is also the system keeping no more; list that folder again.
- Once the folder itself is deleted or moved away, `next` gives what came before, then `NotFound`.

`watchAsync` and `watchAllAsync` start a watcher from an async function. Under it are inotify on Linux, FSEvents on
macOS and `ReadDirectoryChangesExW` on Windows; a watcher ends, and stops watching, where its owner ends.

```sg
void show(read FileChange change) {
    match change {
        Created(path) => print($"made {Path.fileName(read path)}")
        Deleted(path) => print($"deleted {Path.fileName(read path)}")
        Changed(path) => print($"written {Path.fileName(read path)}")
        Lost(path) => print($"lost, list {path} again")
    }
}

Result<void, IoError> main() {
    var root = "watched"
    try Directory.create(read root)
    var watcher = try Directory.watchAll(read root)
    try File.writeText(read Path.join(read root, "notes.txt"), "")
    try Directory.create(read Path.join(read root, "logs"))
    show(read try watcher.next())
    show(read try watcher.next())
    try File.move(read Path.join(read root, "logs"), read Path.join(read root, "old"))
    show(read try watcher.next())
    show(read try watcher.next())
    try File.appendText(read Path.join(read root, "notes.txt"), "a line")
    show(read try watcher.next())
    try Directory.deleteAll(read root)
    return Ok()
}
```

It prints:

```text
made notes.txt
made logs
deleted logs
made old
written notes.txt
```

### A folder as a root

A server that sends the files of a folder, a program that unpacks an archive, a tool that is given a name on its
command line: each joins a name that came from **outside** to a folder it trusts. `Path.joinInside` checks the
text of such a name, but a text cannot show a link: `uploads/latest` may be a link to `/etc`, and may become one
between the check and the opening.

`Directory.openRoot(path)` holds the folder open and gives a `DirectoryRoot` (choices 2066 to 2071). A path given
to it is written with `/` and counted from the folder, and what it names is found from the folder's handle, part
by part, so that it **stays inside**:

- a link on the way is followed while what it points at is inside the folder;
- a path that leads out is `IoError.OutsideRoot`: one that starts at the root of the system (`/etc/passwd`), one
  that climbs past the folder with `..`, and one that goes through a link that does either. A link whose text
  starts at the root of the system is refused wherever it points;
- `.` and `..` that stay inside are parts as any other: `css/../index.html` is `index.html`;
- a name that one of the systems cannot keep inside a folder is `InvalidInput` on all of them, as for
  `Path.joinInside`: an empty path or part (`a//b`, `a/`), a `\` or a `:`, a control character, a part that ends
  with `.` or a space, the name of a device of Windows (`CON`, `nul.txt`).

| Signature | What it does |
|---|---|
| `Result<StreamReader, IoError> openText(read this, read string path)` | A reader of the file, as `File.openText` |
| `Result<StreamReader, IoError> openRegular(read this, read string path)` | Only of a regular file, as `File.openRegular` |
| `Result<StreamWriter, IoError> createText(read this, read string path)` | A writer of the file, made or emptied |
| `Result<StreamWriter, IoError> openWrite(read this, read string path)` | A writer from its start over what it holds; a missing file is made |
| `Result<StreamWriter, IoError> openAppend(read this, read string path)` | A writer after what it holds; a missing file is made |
| `Result<FileInfo, IoError> info(read this, read string path)` | What the system knows of it, a link followed |
| `Result<FileInfo, IoError> linkInfo(read this, read string path)` | The same of the path itself: a link at its end is not followed, so one that leads out can be looked at |
| `bool exists(read this, read string path)` | Whether the path names anything inside, a file or a folder, a link followed |
| `Result<List<DirectoryEntry>, IoError> list(read this, read string path)` | The names in the folder, as `Directory.list`; `.` is the root itself |
| `Result<void, IoError> createDirectory(read this, read string path)` | Makes the folder; the one it is in must be there |
| `Result<void, IoError> delete(read this, read string path)` | Removes the file, or the link itself; a folder is `IsDirectory` |
| `Result<void, IoError> deleteDirectory(read this, read string path)` | Removes an empty folder; the root itself is `InvalidInput` |
| `Result<DirectoryRoot, IoError> openRoot(read this, read string path)` | A folder inside as a root of its own |

Each has an `Async` form for an async function, with a token last if it is given one, and so has
`Directory.openRootAsync`. Moving, copying, links, permissions, a whole tree made or removed and the calls that
read or write a whole file are not under a root yet.

The root is the **folder**, not its path: when the folder is renamed or moved the root is still that folder, and a
folder put where it was is another. Several tasks may use one root at once. A ring of links, and more than 40 links
in a row, is an error (`Other`). A path far longer than the system takes in one piece works under a root, since it
is walked part by part.

```sg
string shown(Result<StreamReader, IoError> opened) {
    match opened {
        Ok(file) => {
            var reader = file
            return reader.readText() ?? "(not text)"
        }
        Error(cause) => return $"({cause})".toString()
    }
}

Result<void, IoError> main() {
    var base = try Directory.createTemp()
    var site = Path.join(read base, "site")
    try Directory.createAll(read Path.join(read site, "css"))
    try File.writeText(read Path.join(read site, "index.html"), "home")
    try File.writeText(read Path.join(read site, "css/main.css"), "styles")
    try File.writeText(read Path.join(read base, "secret.txt"), "secret")
    try File.createLink("css/main.css", read Path.join(read site, "styles.css"))
    try File.createLink("../secret.txt", read Path.join(read site, "notes.txt"))

    var root = try Directory.openRoot(read site)
    print(shown(root.openText("index.html")))
    print(shown(root.openText("css/../index.html")))
    print(shown(root.openText("styles.css")))
    print(shown(root.openText("../secret.txt")))
    print(shown(root.openText("notes.txt")))
    print(shown(root.openText("/etc/passwd")))
    print(shown(root.openText("missing.html")))
    print(shown(root.openText("css\\main.css")))

    try Directory.deleteAll(read base)
    return Ok()
}
```

It prints:

```text
home
home
styles
(outside the root)
(outside the root)
(outside the root)
(not found)
(invalid input)
```

Line by line: `index.html` is opened; `css/../index.html` goes into `css` and back, and never leaves the folder;
`styles.css` is a link to `css/main.css`, which is inside, so it is followed. `../secret.txt` climbs out, `notes.txt`
is a link that climbs out, and `/etc/passwd` starts at the root of the system: all three are `OutsideRoot`, and the
file beside the folder is not read. `missing.html` is not there. A `\` is no part of a name under a root on any
system.

Under it: Linux asks the kernel to keep the path under the folder (`openat2` with `RESOLVE_BENEATH`), one call. On
macOS a path without `..` is opened in one call that refuses every link (`O_NOFOLLOW_ANY`), and walked when a link is
met. On Windows, and wherever the system has no such call, the path is walked: each folder on the way is opened by
its name in the folder before it, a link is read and never followed by the system, and `..` goes back one step,
taken only when what the system says is above is the very folder the walk came through. A name swapped for a link
by another program while the path is walked is an error, never a way out.

On Windows a folder that holds an open file or folder is not renamed by anyone, and a link made to something that is
not there yet is a link to a file, which Windows does not go through as a folder.

## `Path`

These functions look only at the separators in the text and never ask the system: `/` on Linux and macOS, `/`
and `\` on Windows, where a path may also start with a drive (`C:\`) or a share (`\\server\share\`).

| Signature | What it gives |
|---|---|
| `string Path.join(read string first, read string second)` | The two with one separator between them (`\` on Windows); a root at the start of the second is left out |
| `void Path.push(ref string path, read string name)` | `name` added to the end of `path` where it lies, as `join` would join them; a path built of many parts is not copied for each |
| `Result<string, IoError> Path.joinInside(read string folder, read string name)` | The same for a name from outside the program: `InvalidInput` for one that could lead out of the folder on any system |
| `read string Path.fileName(read string path)` | What comes after the last separator |
| `read string Path.extension(read string path)` | The end of the name from its last `.`, the `.` with it, or the empty text |
| `read string Path.parent(read string path)` | What comes before the last separator; a root stays whole |
| `List<byte> Path.systemBytes(read string name)` | The bytes the system is given for a name (below) |
| `string Path.fromSystemBytes(read byte[] bytes)` | The name the bytes are, as the program writes it; it never fails |

`joinInside` looks at the text alone: a link in the folder that leads out of it is beyond it. A program that opens
what the name names does it through [a folder as a root](#a-folder-as-a-root).

`joinInside` refuses an empty name or part, a root, `\`, `:`, `..`, a part that ends in `.` or a space, a
control character (C0, DEL and C1), and Windows' device names (`CON`, `NUL`, `COM1`, also `nul.txt`). A name from a request, an
archive or a file goes through it, never through `join`. `fileName`, `extension` and `parent` give views of
the path, not copies.

```sg
void main() {
    print(Path.join("logs", "today.txt"))
    print(Path.join("logs", "/etc"))
    var path = "logs/2026/report.tar.gz"
    print(Path.fileName(read path))
    print(Path.extension(read path))
    print(Path.parent(read path))
    print($"[{Path.parent("report.txt")}] [{Path.extension(".profile")}]")
    print(Path.joinInside("uploads", "2026/report.txt") ?? "refused")
    match Path.joinInside("uploads", "../etc/passwd") {
        Ok(full) => print(full)
        Error(cause) => print(cause)
    }
}
```

On Linux and macOS it prints:

```text
logs/today.txt
logs/etc
report.tar.gz
.gz
logs/2026
[] []
uploads/2026/report.txt
invalid input
```

### Names that are not text

A name of the system is any bytes on Linux and macOS, and any units of UTF-16 on Windows, while a `string`
is always UTF-8. Such a name comes into the program all the same: a word of the command line, a variable of
the environment, a name `Directory.list` gives. Each byte that is not UTF-8 is written as a character from
U+10FF80 to U+10FFFF (U+10FF00 and the byte), which goes back to that byte when the name is given to the
system, so `File.open(entry.name)` opens the file whatever its name holds. On Windows a half of a pair of
UTF-16 alone is three such bytes. `Path.systemBytes` gives the bytes themselves, to write a name as it is or
to compare it; `Path.fromSystemBytes` makes a name of bytes read out of data, such as an entry of an archive.

```sg
void main() {
    var bytes = Hex.decode("636166e9") ?? panic("hex")
    var name = Path.fromSystemBytes(read bytes)
    print(name == "caf\u{10FFE9}")
    var back = Path.systemBytes(read name)
    print(Hex.encode(read back))
}
```

It prints:

```text
true
636166e9
```

Printed, such a character is written as UTF-8, as any other; a program that lists names as `ls` does writes
`Path.systemBytes(name)` to the output instead. Only bytes from 0x80 come out of these characters, so a name
checked for `/`, `\`, `.` or a zero byte stays checked.

## `IoError`

A built-in enum, `Copyable`, `Equatable` and `Formattable`:

| Variant | When |
|---|---|
| `NotFound` | No such file or folder; also a path with a zero byte in it |
| `PermissionDenied` | The system does not let the program do it |
| `IsDirectory`, `NotDirectory` | A folder where a file was meant, or a file where a folder was |
| `AlreadyExists`, `DirectoryNotEmpty` | Something is where one was to be made; a folder to remove holds something |
| `NoSpace` | The disk is full |
| `InvalidInput` | What was given cannot go to the system, a seek before the start, a length below 0, a sync of what is not a file, `readLink` of what is no link, or a mode outside `0o7777` |
| `NotSeekable` | `seek` on a pipe, on `Stdin` or on a writer of `openAppend`; `setLength` on a standard stream or the input of another program |
| `TooLong` | `fill` on a reader whose block holds its limit already |
| `InvalidText(long position)` | A byte that is not UTF-8, at that place |
| `Other(int code)` | Any other error of the system, by its number |
| `Cancelled(Cancelled cancelled)` | A token was cancelled |
| `NotAFile` | `File.openRegular` or `File.mapRead` met a pipe, a device or a socket, and did not open it; a lock of a stream that is no file |
| `OutsideRoot` | A path given to a `DirectoryRoot` leads out of its folder: by `..`, by starting at the root of the system, or through a link |
| `NotATerminal` | No terminal where one was asked for: `Terminal.giveTo` or `takeBack` in a program without one, a program put in front (`Command.foreground`), `Terminal.rawMode` of an input or `Terminal.size` of an output that is none |
| `ConnectionRefused`, `ConnectionReset`, `AddressInUse`, `AddressNotAvailable`, `HostNotFound`, `LineTooLong`, `MessageTooLong` | The network's |

Printed, an `IoError` is a sentence in lower case, as Go's errors are: `not found`, `permission denied`,
`invalid text at byte 812`, `system error 5`, so `printError($"{path}: {cause}")` reads as a tool's message.
`match` and `==` see the variants.

`IoError.fromErrno(n)` and `IoError.fromCode(n)` make one out of what a function of C left.
