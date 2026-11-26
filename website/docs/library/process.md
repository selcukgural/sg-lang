---
title: Programs and the system
description: Command, Process and ExitStatus to run other programs, Environment, and Signals.
sidebar_position: 8
---

# Programs and the system

`Command` runs another program and waits for it, or starts it and goes on with a `Process`. `ExitStatus` says
how a program ended, and `ProcessError` why it did not do its work. `Environment` reads the variables the
program was started with, and `Signals` waits for the signals of the system. All of them are written in SG in
the module `std`.

No shell reads the words of a `Command`: each argument reaches the program as it is written, so a text from
outside never becomes a command of its own. The programs on this page start `sh -c` on Linux and macOS and
`cmd /c` on Windows, since those are on every system, through a function `shell` that each of them holds
whole.

## `Command`

| Signature | What it does |
|---|---|
| `Command(read string program)` | A program to run: a name looked for in the folders of `PATH`, or a path |
| `void arg(ref this, read string word)`, `void args(ref this, read string[] words)` | Arguments, after the ones before |
| `void directory(ref this, read string path)` | The folder it works in |
| `void setVariable(ref this, read string name, read string value)` | A variable of its environment, beside the ones of this program |
| `void removeVariable(ref this, read string name)`, `void clearVariables(ref this)` | One left out, or all of them |
| `void input(ref this, read string text)` | What it reads as its input; without it, it reads nothing |
| `Result<void, ProcessError> run(read this)` | Runs it and waits; an end with anything but 0 is `Failed(status)` |
| `Result<string, ProcessError> output(read this)` | The same, and gives what it wrote, as text (`InvalidText` if it is not UTF-8) |
| `Result<ExitStatus, IoError> status(read this)` | Runs it and gives how it ended, whatever that was |
| `void pipeInput(ref this)`, `pipeOutput`, `pipeError` | Streams to take from the `Process` that `start` gives |
| `Result<Process, IoError> start(read this)` | Starts it and goes on |
| `runAsync`, `outputAsync`, `statusAsync` | The same in an async function, with a `read CancellationToken` last; a cancel stops the program |

A program run gets the output and the standard error of this one, with what was printed before already written
out. A name is looked for only in the folders of `PATH` that are absolute, never in the folder the program
works in.

```sg
Command shell(read string script) {
    if OperatingSystem.isWindows() {
        var cmd = Command("cmd")
        cmd.arg("/c")
        cmd.arg(script)
        return cmd
    }
    var sh = Command("sh")
    sh.arg("-c")
    sh.arg(script)
    return sh
}

Result<void, ProcessError> main() {
    var hello = shell("echo hello")
    var text = try hello.output()
    print($"[{text.trim()}]")

    match shell("exit 3").status() {
        Ok(status) => print($"{status} {status.success()} {status == ExitStatus.Code(3)}")
        Error(cause) => print(cause)
    }
    match shell("exit 4").run() {
        Ok => print("ran")
        Error(cause) => print(cause)
    }
    match Command("no-such-program-here").run() {
        Ok => print("ran")
        Error(Io(cause)) => print($"could not start: {cause}")
        Error(Failed(status)) => print(status)
    }
    return Ok()
}
```

It prints:

```text
[hello]
exited with code 3 false true
the program exited with code 4
could not start: not found
```

## `Process`

| Signature | What it does |
|---|---|
| `int id(read this)` | The number the system gave the program |
| `StreamWriter? takeInput(ref this)` | Its input, once, after `pipeInput` |
| `StreamReader? takeOutput(ref this)`, `takeError(ref this)` | Its output, or its standard error, once |
| `Result<ExitStatus, IoError> wait(this)` | Waits for it to end, and takes the `Process` |
| `Result<ExitStatus, IoError> waitAsync(this, read CancellationToken token)` | The same, async |
| `void kill(read this)` | Stops it at once (signal 9) |
| `void terminate(read this)` | Asks it to stop (signal 15) |
| `void signal(read this, Signal signal)` | Sends it a signal by its name |

A `Process` kept in a local variable is waited for on every path. One that ends without that stops its program
with the signal 9, so nothing goes on running unseen:

```sg error
Result<void, IoError> main() {
    var process = try Command("true").start()
    print(process.id())
    return Ok()
}
```

```text
error: the program in `process` is not waited for at this `return`
  a program that is not waited for is stopped where its `Process` ends, with the signal 9
```

```sg
Command shell(read string script) {
    if OperatingSystem.isWindows() {
        var cmd = Command("cmd")
        cmd.arg("/c")
        cmd.arg(script)
        return cmd
    }
    var sh = Command("sh")
    sh.arg("-c")
    sh.arg(script)
    return sh
}

Result<void, IoError> main() {
    var command = shell("echo one&& echo two")
    command.pipeOutput()
    var process = try command.start()
    var output = process.takeOutput() ?? panic("asked for with pipeOutput")
    print(process.takeOutput() is null)
    for line in output.lines() {
        var text = try line
        print($"<{text.trim()}>")
    }
    print(try process.wait())

    var running = try shell("sleep 5").start()
    running.kill()
    print(try running.wait())
    return Ok()
}
```

On Linux and macOS it prints:

```text
true
<one>
<two>
exited with code 0
was stopped by signal 9
```

Windows has no signals to send: `kill`, `terminate` and `signal` end the program with the code 1 there, so the
last line is `exited with code 1`.

## `ExitStatus` and `ProcessError`

`ExitStatus` is `Code(int code)`, an end by itself (0 says the program did its work), or `Signal(int number)`, a
signal of the system that stopped it. `bool success(read this)` is `true` for `Code(0)`. It prints as
`exited with code 3` or `was stopped by signal 9`; on Windows a code with its highest bit set is written in
hex, as Windows writes it: `exited with code 0xC0000005`.

`ProcessError` is `Io(IoError cause)`, the program could not be started (`NotFound`, `PermissionDenied`,
`InvalidInput` for a word with a zero byte in it) or its output was not UTF-8, or `Failed(ExitStatus status)`,
it ran and did not end with 0. It prints as the `IoError` does (`not found`), or `the program exited with code 4`.

## `Environment`

| Signature | What it does |
|---|---|
| `Result<string, VariableError> Environment.get(read string name)` | The text of the variable |

`VariableError` is `Missing`. A name or a value that is not UTF-8 is a name of the system, written as
[Names that are not text](./files.md#names-that-are-not-text) says; so is a word of the command line. The
variables are only read: setting one while another thread reads one is not safe in C, and a program run by a
`Command` gets an environment of its own through `setVariable`. `Environment.set(...)` is an error that says
so. The words of the command line come to `void main(List<string> args)`.

## `Signals`

While a listener of a signal lives, that signal does not end the program: it is kept until the program takes
it, as in Go and Tokio, instead of a handler that would run between any two steps of any thread.

| Signature | What it does |
|---|---|
| `SignalListener Signals.listen(read Signal[] signals)` | Starts listening; no signal at all is a panic |
| `Signal next(ref this)` | Waits for the next signal and gives it; one that came before is given at once |
| `Result<Signal, Cancelled> nextAsync(ref this, read CancellationToken token)` | The same, async |
| `void Signals.raise(Signal signal)` | Sends the signal to this program, as C's `raise` does |

`Signal` is `Interrupt` (Ctrl-C), `Terminate` (the polite stop), `Hangup`, `Quit`, `User1`, `User2` or
`WindowChange`; their numbers are not given, since `User1` is 30 on macOS and 10 on Linux. Several of one kind
before a `next` are one, as the system does not count them either, and every listener of a signal gets it. On
Windows `Interrupt` is Ctrl-C and Ctrl-Break, `Terminate` the console closed or the system shutting down; the
others never come there, but `raise` hands them to listeners all the same.

```sg
void main() {
    print(Environment.get("PATH") is Ok(_))
    match Environment.get("SG_NO_SUCH_VARIABLE") {
        Ok(value) => print(value)
        Error(cause) => print(cause)
    }
    print(Environment.get("SG_NO_SUCH_VARIABLE") ?? "/")

    var signals = Signals.listen(read List<Signal> { Signal.User1, Signal.Interrupt })
    Signals.raise(Signal.User1)
    Signals.raise(Signal.User1)
    Signals.raise(Signal.Interrupt)
    print(signals.next())
    print(signals.next())
}
```

It prints:

```text
true
Missing
/
User1
Interrupt
```

A server listens for `Interrupt` and `Terminate`, and when one comes, cancels a `CancellationSource` that its
tasks look at (see [Tasks](./tasks.md)), so that it ends its work instead of being cut off.
