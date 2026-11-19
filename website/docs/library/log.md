---
title: Log
description: Log and LogLevel, lines of a log written to the standard error, a level that is off costing almost nothing, and a message never read for anything in it.
sidebar_position: 14
---

# Log

`Log` writes lines of a log to the standard error, each with the time in UTC and its level. It is written in
SG in the module `std`. There is nothing to set up: the first line written makes it ready.

```sg
void main() {
    var port = 8080
    Log.info($"listening on {port}")
    Log.debug("not written: Debug is below Info")
    Log.warn("disk 91% full")
    print("done")
}
```

It prints `done`, and on the standard error:

```console
2026-11-19T20:15:03.123Z INFO  listening on 8080
2026-11-19T20:15:03.123Z WARN  disk 91% full
```

The time is RFC 3339 with milliseconds, the level is padded to five letters so the messages line up, and
there is nothing else on the line: no file, no line number, no fields of its own.

| Signature | What it does |
|---|---|
| `void Log.trace(read string message)` | A line at `Trace`, the most detailed level |
| `void Log.debug(read string message)` | A line at `Debug` |
| `void Log.info(read string message)` | A line at `Info` |
| `void Log.warn(read string message)` | A line at `Warn` |
| `void Log.error(read string message)` | A line at `Error`; the program goes on |
| `void Log.setLevel(LogLevel level)` | From now on, lines of `level` and above are written; `Off` writes none |
| `LogLevel Log.level()` | The level lines are written from |
| `bool Log.isOn(LogLevel level)` | Whether a line of `level` would be written |

`LogLevel` is `Trace`, `Debug`, `Info`, `Warn`, `Error` or `Off`, in that order: `LogLevel.Warn >
LogLevel.Info`. Any task may set the level, and every task's next line follows it.

## The level

A program starts at `Info`, or at the level the variable of the environment `SG_LOG` names: `trace`,
`debug`, `info`, `warn`, `error` or `off`, in any case. `Log.setLevel` changes it afterwards, so a program
with a `--verbose` flag of its own has the last word. A value of `SG_LOG` that names no level does not stop
the program: one line says so, and `Info` is used.

```console
$ SG_LOG=debug ./server
$ SG_LOG=verbose ./server
2026-11-19T20:15:03.123Z WARN  SG_LOG is "verbose", which is not a level (trace, debug, info, warn, error, off); Info is used
```

## A level that is off

The compiler writes `Log.debug($"...")` as `if Log.isOn(LogLevel.Debug) { Log.debug($"...") }`, so the text
of a line that is not written is never built: a level that is off costs a load and a compare. A call inside
the text does not run either, as with the macros of Rust's `log`:

```sg
int look(ref int count) {
    count += 1
    return count
}

void main() {
    var count = 0
    Log.debug($"found {look(ref count)}")
    print($"looked {count} times")
    Log.setLevel(LogLevel.Debug)
    Log.debug($"found {look(ref count)}")
    print($"looked {count} times")
}
```

It prints:

```text
looked 0 times
looked 1 times
```

Work done only for the log goes inside `Log.isOn`:

```sg nocheck
if Log.isOn(LogLevel.Trace) {
    var summary = summarize(read requests)
    Log.trace($"requests: {summary}")
}
```

## A message is only written

The text of a message is never read for anything in it: no `${...}` is looked up and no pattern takes
arguments, so a text from outside cannot make the log do anything (Log4Shell, CVE-2021-44228, came from a
lookup in a message). It takes a finished `string`; `$"..."` is put together when the program is compiled,
and a value put into it is never read again.

Each line comes from one call. A line break and every other control character in a message is escaped when
it is written (`\n`, `\r`, `\t`, the others as `\u001b`), and so are the characters that cannot be seen or
that turn the text around them: the marks of direction, the characters of no width and the byte order mark
(`‮`). A backslash is written `\\`, so a `\n` that was in the text is not taken for an escaped line
break. A user who sends a line break cannot add a line of their own, and a log shown in a terminal cannot be
driven by escape sequences.

```sg
void main() {
    var user = "bob\n2026-11-19T20:15:03.123Z INFO  admin logged in"
    Log.warn($"login failed for {user}")
    Log.warn("${jndi:ldap://example.com/a}")
    print("written")
}
```

It prints `written`, and on the standard error:

```console
2026-11-19T20:15:03.123Z WARN  login failed for bob\n2026-11-19T20:15:03.123Z INFO  admin logged in
2026-11-19T20:15:03.123Z WARN  ${jndi:ldap://example.com/a}
```

Each line is made whole and written at once, so the lines of tasks never mix, with each other or with
`printError`. A line goes out when it is written: nothing waits in a buffer to be lost when the program
stops.

## Colours

When the standard error is a terminal, the level is coloured: `ERROR` red, `WARN` yellow, `INFO` green,
`DEBUG` blue and `TRACE` grey; the message never is. Into a file or a pipe there are no colours, and none
when `NO_COLOR` is set or `TERM` is `dumb`. On Windows the console is asked to take the escape sequences, and
an old one that refuses them shows no colour.

## Not here yet

Files and their rotation, fields of a structured log (JSON lines), a level for each module, and a way to
force colours on. A program that needs them writes its own lines with `Stderr` or a `StreamWriter`.
