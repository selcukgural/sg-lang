---
title: Programs and the system
description: Command, Process, Pipeline and ExitStatus to run other programs, Environment, and Signals.
sidebar_position: 8
---

# Programs and the system

`Command` runs another program and waits for it, or starts it and goes on with a `Process`. `Pipeline` joins
programs by pipes, as `a | b` at a shell. `ExitStatus` says how a program ended, and `ProcessError` why it did
not do its work. `Environment` reads the variables the
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
| `void input(ref this, read string text)` | What it reads as its input; without it, `output` gives it nothing to read and `run` its own input |
| `void inputFrom(ref this, read string path)` | It reads the file, as `< path` |
| `void outputTo(ref this, read string path)`, `appendOutputTo` | Its output into the file, made empty first (`> path`), or at its end (`>> path`) |
| `void errorTo(ref this, read string path)`, `appendErrorTo` | Its standard error the same way (`2> path`, `2>> path`) |
| `void discardOutput(ref this)`, `discardError` | Its output, or its standard error, goes nowhere (`> /dev/null`) |
| `void errorToOutput(ref this)` | Its standard error goes where its output goes (`2>&1`), into the text of `output` too |
| `Result<void, ProcessError> run(read this)` | Runs it and waits; an end with anything but 0 is `Failed(status)` |
| `Result<string, ProcessError> output(read this)` | The same, and gives what it wrote, as text (`InvalidText` if it is not UTF-8); empty when its output was sent to a file or nowhere |
| `Result<ExitStatus, IoError> status(read this)` | Runs it and gives how it ended, whatever that was |
| `void pipeInput(ref this)`, `pipeOutput`, `pipeError` | Streams to take from the `Process` that `start` gives |
| `Result<Process, IoError> start(read this)` | Starts it and goes on |
| `runAsync`, `outputAsync`, `statusAsync` | The same in an async function, with a `read CancellationToken` last; a cancel stops the program |

A program run gets the output and the standard error of this one, with what was printed before already written
out, unless it was told otherwise. For each stream the last word counts: `pipeOutput` after `outputTo` makes it
a pipe again. A file is opened as the program starts, so one that cannot be (`NotFound` for a folder that is
not there) is the error of `run`, `status`, `output` or `start`, and the program does not start; a file made
gets the mode 0o666 less what the mask of this program takes away. A name is looked for only in the folders of
`PATH` that are absolute, never in the folder the program works in.

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

Where the streams go, as a shell sends them with `>`, `>>`, `<` and `2>&1`:

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
    var first = shell("echo one")
    first.outputTo("log.txt")
    try first.run()
    var second = shell("echo two")
    second.appendOutputTo("log.txt")
    try second.run()

    var sort = Command("sort")
    sort.inputFrom("log.txt")
    sort.arg("-r")
    var sorted = try sort.output()
    print(sorted.trim().replaceAll("\n", ","))

    var noisy = shell("echo out&& echo err 1>&2")
    noisy.errorToOutput()
    var both = try noisy.output()
    print(both.trim().replaceAll("\n", ","))

    var missing = shell("echo never")
    missing.inputFrom("no-such-file.txt")
    match missing.run() {
        Ok => print("ran")
        Error(cause) => print($"did not start: {cause}")
    }
    return Ok()
}
```

On Linux and macOS it prints:

```text
two,one
out,err
did not start: not found
```

## `Process`

| Signature | What it does |
|---|---|
| `int id(read this)` | The number the system gave the program |
| `StreamWriter? takeInput(ref this)` | Its input, once, after `pipeInput` |
| `StreamReader? takeOutput(ref this)`, `takeError(ref this)` | Its output, or its standard error, once |
| `Result<ExitStatus, IoError> wait(this)` | Waits for it to end, and takes the `Process` |
| `Result<ExitStatus, IoError> waitAsync(this, read CancellationToken token)` | The same, async |
| `Result<ExitStatus?, IoError> tryWait(ref this)` | Whether it has ended, without waiting: `null` while it runs, how it ended once it has |
| `void detach(this)` | Lets it go on after this program; the runtime takes it away once it ends |
| `void kill(read this)` | Stops it at once (signal 9) |
| `void terminate(read this)` | Asks it to stop (signal 15) |
| `void signal(read this, Signal signal)` | Sends it a signal by its name |
| `int Process.currentId()`, `int Process.parentId()` | The number of this program, and of the one that started it |
| `Result<void, IoError> Process.signalId(int id, Signal signal)`, `Process.killId(int id)` | A signal, or the signal 9, to the program of that number, which need not be one this program started |

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

`tryWait` looks without waiting. Once it has seen the program end, the program has been waited for: every later
`tryWait` and `wait` give the same end, `kill` does nothing, and the end of the `Process` stops nothing. While it
runs, the rule above still holds: a `Process` that `tryWait` saw running is waited for as any other.

`detach` lets the program go on after its `Process` and after this program. It is not stopped, and this
program's runtime takes it away when it ends (on Linux a `pidfd` and on macOS a kqueue, watched by one thread
of its own), so it does not stay behind as a program that has ended and was never asked how. The pipes it still
holds are closed; a reader or a writer taken out before stays open. It stays in the group and the session of
this program, so a terminal that closes still sends it `Hangup`.

`Process.signalId` and `Process.killId` name a program by its number, as `kill` at a shell does. No program of
that number is `NotFound`, one this program may not send a signal to `PermissionDenied`, and a number of 0 or
less, which would name a group of programs on Linux and macOS, `InvalidInput`. A number is given again once
its program has been waited for, so one kept from long ago may name another program by now; a `Process` names
its own and no other until it is waited for. On Windows they stop the program as `kill` does, but `WindowChange`,
which only looks whether it is there; a program there that has ended may still be found for a moment after it
was waited for, while the system holds it, and a signal to it is then no error.

## `Pipeline`

Programs joined by pipes, as `a | b | c` at a shell: what each writes to its output is what the next reads,
through a pipe of the system from one to the other, never through this program. The input of the first and the
output of the last are what their `Command` says; the standard error of each is its own, and `errorToOutput`
sends it down the pipe too.

| Signature | What it does |
|---|---|
| `Pipeline()` | A pipeline with no program in it yet |
| `void add(ref this, Command command)` | One more program, at the end; the `Command` is taken |
| `Result<void, ProcessError> run(read this)` | Runs them all and waits; fails as `set -o pipefail` says |
| `Result<string, ProcessError> output(read this)` | The same, and gives what the last wrote, as text |
| `Result<List<ExitStatus>, IoError> status(read this)` | How each ended, in order, whatever that was |
| `Result<List<Process>, IoError> start(read this)` | Starts them all and goes on: the `Process` of each |
| `runAsync`, `outputAsync`, `statusAsync` | The same in an async function, with a `read CancellationToken` last; a cancel stops every program |

The pipeline failed when a program ended with anything but 0, and the last one that did is the error. A
program before the last that the signal `SIGPIPE` stopped did not fail: that is how a program ends whose reader
has read all it wanted and gone, as `yes` does in `yes | head -1`. Windows has no such signal. A program before
the last whose output was sent elsewhere, or one after the first given an input of its own, is a panic where the
pipeline starts, since the pipe takes that place; so is a pipeline with no program in it. A program that cannot
start stops those started before it, and is the error.

```sg
Command shell(read string script) {
    var sh = Command("sh")
    sh.arg("-c")
    sh.arg(script)
    return sh
}

Result<void, ProcessError> main() {
    var pipeline = Pipeline()
    pipeline.add(shell("printf 'pear\\napple\\nfig\\n'"))
    pipeline.add(Command("sort"))
    pipeline.add(shell("tr a-z A-Z"))
    var sorted = try pipeline.output()
    print(sorted.trim().replaceAll("\n", ","))

    var endless = Pipeline()
    endless.add(Command("yes"))
    endless.add(shell("head -n 2"))
    var two = try endless.output()
    print(two.trim().replaceAll("\n", ","))

    var failing = Pipeline()
    failing.add(shell("exit 3"))
    failing.add(Command("cat"))
    match failing.status() {
        Ok(all) => print($"{all[0]}, {all[1]}")
        Error(cause) => print(cause)
    }
    match failing.run() {
        Ok => print("done")
        Error(cause) => print(cause)
    }
    return Ok()
}
```

On Linux and macOS it prints:

```text
APPLE,FIG,PEAR
y,y
exited with code 3, exited with code 0
the program exited with code 3
```

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
