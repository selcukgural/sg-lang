---
title: Files and folders
description: File, StreamReader and StreamWriter, the standard streams, Directory, Path, FileInfo and IoError.
sidebar_position: 4
---

# Files and folders

`File` reads and writes whole files and opens them as streams. A `StreamReader` reads a file, the input of the
program or a connection through a block of its own; a `StreamWriter` writes through one. `Directory` makes,
lists and removes folders, and `Path` joins paths and takes them apart. Every call that asks the system gives a
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

## `StreamWriter`

| Signature | What it does |
|---|---|
| `Result<StreamWriter, IoError> File.createText(read string path)` | A writer of a new or emptied file |
| `Result<StreamWriter, IoError> File.openAppend(read string path)` | One that writes after what the file holds |
| `Result<StreamWriter, IoError> File.openWrite(read string path)` | One that writes from the start over what the file holds; a missing file is made |
| `Result<void, IoError> write(ref this, x)`, `writeLine(ref this, x)` | Writes whatever `print` takes, with or without a line end |
| `Result<void, IoError> writeBytes(ref this, read byte[] bytes)` | Writes the bytes as they are |
| `Result<void, IoError> flush(ref this)` | Hands what the block holds to the system |
| `Result<void, IoError> close(this)` | Writes what is left and closes the file |
| `Result<long, IoError> seek(ref this, SeekFrom to)` | Writes what it holds, then moves; gives the new place |
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
`fillAsync`; on a writer `writeAsync`, `writeLineAsync`, `writeBytesAsync`, `flushAsync`, `closeAsync`,
`syncDataAsync`, `syncAsync`, `setLengthAsync`, `seekAsync`. Each may take a `read CancellationToken` last (not
`closeAsync`). In an async function the calls without `Async` are an error, since they would hold the thread.
`buffered` and `consume` do not wait, and neither does a reader's `seek`, which asks the system where the
file stands and reads nothing, nor its `info`: they are called as they are.

The calls of the file system have async versions too (choice 1871), done on a thread of input and output
while the task waits: `File.infoAsync`, `existsAsync`, `deleteAsync`, `moveAsync`, `replaceAsync`, `copyAsync`,
`createTempAsync`, and `Directory.createAsync`, `createAllAsync`, `deleteAsync`, `deleteAllAsync`, `syncAsync`,
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
writes to `Stdout` and `printError(x)` to `Stderr`. `Stdin` cannot `seek` (`NotSeekable`).

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

A `DirectoryEntry` has `name` (without the folder) and `kind`. A `FileInfo` has `kind`, `size` (in bytes, a
`long`) and `modified` (a [`DateTime`](./time.md)). `FileKind` is `File`, `Directory`, `Link` or `Other`; only
a listing tells a `Link`, since `File.info` follows links.

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

## `IoError`

A built-in enum, `Copyable`, `Equatable` and `Formattable`:

| Variant | When |
|---|---|
| `NotFound` | No such file or folder; also a path with a zero byte in it |
| `PermissionDenied` | The system does not let the program do it |
| `IsDirectory`, `NotDirectory` | A folder where a file was meant, or a file where a folder was |
| `AlreadyExists`, `DirectoryNotEmpty` | Something is where one was to be made; a folder to remove holds something |
| `NoSpace` | The disk is full |
| `InvalidInput` | What was given cannot go to the system, a seek before the start, a length below 0, or a sync of what is not a file |
| `NotSeekable` | `seek` on a pipe, on `Stdin` or on a writer of `openAppend`; `setLength` on a standard stream or the input of another program |
| `TooLong` | `fill` on a reader whose block holds its limit already |
| `InvalidText(long position)` | A byte that is not UTF-8, at that place |
| `Other(int code)` | Any other error of the system, by its number |
| `Cancelled(Cancelled cancelled)` | A token was cancelled |
| `NotAFile` | `File.openRegular` met a pipe, a device or a socket, and did not open it |
| `ConnectionRefused`, `ConnectionReset`, `AddressInUse`, `AddressNotAvailable`, `HostNotFound`, `LineTooLong`, `MessageTooLong` | The network's |

Printed, an `IoError` is a sentence in lower case, as Go's errors are: `not found`, `permission denied`,
`invalid text at byte 812`, `system error 5`, so `printError($"{path}: {cause}")` reads as a tool's message.
`match` and `==` see the variants.

`IoError.fromErrno(n)` and `IoError.fromCode(n)` make one out of what a function of C left.
