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
| `Result<void, IoError> File.replace(read string from, read string to)` | Renames it over `to`, in one step |
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
NotFound
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
| `Result<bool, IoError> readLine(ref this, ref string line)` | The next line into `line`, without its `\n` or `\r\n`; `false` at the end |
| `Result<string, IoError> readText(ref this)` | All that is left |
| `lines(ref this)` | Each line as a `Result<string, IoError>`, for a `for` |
| `Result<nint, IoError> readBytes(ref this, ref byte[] buffer)` | Fills the buffer unless the end comes first; how many bytes |
| `Result<nint, IoError> readSome(ref this, ref byte[] buffer)` | What the block holds, or what one read of the system gives; 0 at the end |
| `Result<long, IoError> seek(ref this, SeekFrom to)` | Moves, inside its block when it can; gives the new place |

Text that is not UTF-8 is `IoError.InvalidText(position)`, counted from the start of the file. `SeekFrom` is
`Start(long offset)`, `End(long offset)` or `Current(long offset)`; a place before the start is
`InvalidInput`.

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
`writeTextAsync`, `writeBytesAsync`, `appendTextAsync`, `openTextAsync`, `createTextAsync`, `openAppendAsync`;
on a reader `readLineAsync`, `readTextAsync`, `readBytesAsync`, `readSomeAsync`; on a writer `writeAsync`,
`writeLineAsync`, `writeBytesAsync`, `flushAsync`, `closeAsync`, and `seekAsync` on both. Each may take a
`read CancellationToken` last (not `closeAsync`). In an async function the calls without `Async` are an error,
since they would hold the thread.

## `Stdin`, `Stdout` and `Stderr`

`Stdin` is a `StreamReader` of the input of the program, `Stdout` and `Stderr` are `StreamWriter`s. They are
lent, never moved or closed: `ref Stdout` gives one to a function that takes a `ref StreamWriter`. `print(x)`
writes to `Stdout` and `printError(x)` to `Stderr`. `Stdin` cannot `seek` (`NotSeekable`).

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
DirectoryNotEmpty
AlreadyExists
a
false
```

## `Path`

These functions look only at the separators in the text and never ask the system: `/` on Linux and macOS, `/`
and `\` on Windows, where a path may also start with a drive (`C:\`) or a share (`\\server\share\`).

| Signature | What it gives |
|---|---|
| `string Path.join(read string first, read string second)` | The two with one separator between them (`\` on Windows); a root at the start of the second is left out |
| `Result<string, IoError> Path.joinInside(read string folder, read string name)` | The same for a name from outside the program: `InvalidInput` for one that could lead out of the folder on any system |
| `read string Path.fileName(read string path)` | What comes after the last separator |
| `read string Path.extension(read string path)` | The end of the name from its last `.`, the `.` with it, or the empty text |
| `read string Path.parent(read string path)` | What comes before the last separator; a root stays whole |

`joinInside` refuses an empty name or part, a root, `\`, `:`, `..`, a part that ends in `.` or a space, a
control character, and Windows' device names (`CON`, `NUL`, `COM1`, also `nul.txt`). A name from a request, an
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
InvalidInput
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
| `InvalidInput` | What was given cannot go to the system, or a seek before the start |
| `NotSeekable` | `seek` on a pipe, on `Stdin` or on a writer of `openAppend` |
| `InvalidText(long position)` | A byte that is not UTF-8, at that place |
| `Other(int code)` | Any other error of the system, by its number |
| `Cancelled(Cancelled cancelled)` | A token was cancelled |
| `ConnectionRefused`, `ConnectionReset`, `AddressInUse`, `HostNotFound`, `LineTooLong`, `MessageTooLong` | The network's |

`IoError.fromErrno(n)` and `IoError.fromCode(n)` make one out of what a function of C left.
