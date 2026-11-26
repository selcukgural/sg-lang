# How it is built

## The source files

| File | Role |
|---|---|
| `src/lexer.rs` | Tokens, the line-end rule, `$"..."` pieces |
| `src/parser.rs` | Hand-written recursive descent parser |
| `src/types.rs` | Types as the checker sees them |
| `src/check.rs` | Static checks: names, types, `null` safety, use after move (per variable and per field), `match` covering every variant, `return` on every path, unused `Result`, what `try` may pass on, what `Copyable` and `Formattable` promise. Writes resolved details (literal types, field indexes, implicit conversions, what each call runs) into the tree |
| `src/interp.rs` | Runs the checked tree; only data-dependent failures (panics) are left to it. Every task that runs code is a fiber, a thread of its own of which one runs at a time. The fields of a value of a `Disposable` type wait in a list when their last owner lets them go, and `dispose` runs on them between statements |
| `src/emit_c.rs` | The back end: writes the checked tree as C, with the same panics; tasks are threads of the system |
| `src/opening.rs` | After the checker: where a `shared checked` value is reached and for how long it is borrowed (the `Open` and `Within` nodes the interpreter follows), the conflicts through the same variable that are certain before the program runs, and the borrows that would outlive their line |
| `src/waits.rs` | After the checker, on the whole program: which functions wait (take a lock, wait for a task or await one, themselves or through the functions they call), and what waits inside a `lock` block; which functions hold the thread, and what does in an async function; an async function that awaits itself |
| `src/unread.rs` | Last of the checks: the local variables whose value nothing reads, from what the checker noted each name stands for, telling a read from a write along the checked tree of each function and test |
| `src/ffi.rs` | For the interpreter: the library of wrappers for the functions of C a program calls, built with `cc` in a folder of its own and opened with `dlopen` |
| `src/raw.rs` | For the interpreter: raw memory, real addresses from `calloc`, each checked against the block it came from |
| `src/sums.rs` | For the back end: the sums in a loop that cannot overflow for a number of rounds, so that a checked build runs those rounds without their test |
| `src/unicode.rs`, `src/unicode_tables.rs` | What a character is, its general category and its case: the tables of Unicode 17.0, one record for each code point found in three steps, read by the interpreter and written into compiled code as C when a program calls a method that needs them. `tools/unicode_tables.py` makes the tables from the Unicode Character Database (each file checked against its SHA-256) and checks every code point against Python's tables of the same version |
| `src/diag.rs` | Error and panic messages |
| `src/source.rs` | The files of a program: which folders are modules and which of them are read. All files are kept as one text, so a position says which file it is in; the language's own module comes last, when a file names one of its types |
| `src/std/time.sg` | The language's own module `std`, written in SG: `Duration`, `Instant`, `DateTime`, `DateTimeOffset`, the calendar and the reading of RFC 3339. Only it calls the readings of the clock and of the time zone, `Clock.monotonic()`, `Clock.wall()` and `Clock.offsetAt(seconds)` |
| `src/std/random.sg` | The same module's random numbers: `Random` (xoshiro256\*\* seeded with SplitMix64) and `SecureRandom`. Only it calls `Entropy.fill`, the random bytes of the system. A program gets each file of `std` when it names one of its types. A panic inside the module is shown at the program's call that led there (`Interp::call`; in compiled code `Emitter::tracked`) |
| `src/std/files.sg`, `src/std/path.sg`, `src/std/environment.sg` | The same module's folders (`Directory`, `DirectoryEntry`, `FileInfo`, `FileKind`, and the work of `File.info` and `File.exists`), paths (`Path`) and variables of the environment (`Environment`, `VariableError`). Only they call `System.list`, `System.status` and the other calls of the system; a file of `std` that names a type of another brings it along (a `FileInfo` holds a `DateTime`) |
| `src/std/process.sg` | The same module's other programs (`Command`, `Process`, `Pipeline`, `ExitStatus`, `ProcessError`): where the streams of a program go, the pipes of a `Pipeline` and when it failed, the search in `PATH`, the checks of what the system cannot take, the pipes taken out of a running program, its stop when it is not waited for, and the reading of how a program ended, over `System.run`, `System.start`, `System.wait`, `System.waitAsync`, `System.tryWait`, `System.detach`, `System.signal`, `System.signalId`, `System.processId`, `System.pipeEnds`, `System.closeEnd`, `System.reader`, `System.writer` and `System.executable` |
| `src/std/net.sg` | The same module's datagrams and addresses (`UdpSocket`, `SocketAddress`, `Received`): reading and printing a v4 and a v6 address, and the two ends of a `TcpStream`. Only it calls the sockets of the system (`System.udpBind`, `System.udpSend`, `System.udpReceive`, `System.lookup` and the others) |
| `src/std/signals.sg` | The same module's signals (`Signal`, `Signals`, `SignalListener`). Only it calls `System.signalListen`, `System.signalNext` and the other calls of the signals |
| `src/std/regex.sg`, `src/std/unicode.sg` | The same module's regular expressions (`Regex`, `Match`, `Captures`, `RegexError`: a tree, a program of Thompson's construction and its reverse, lazy DFAs and Pike's machine) and the general category of a character (`UnicodeCategory`, the work of `c.category()`). Only they call `System.charCategory` and `System.charFold`, which read the Unicode tables; `Regex.problem` is run by the checker on each pattern written in the program (`patterns_checked` in `src/check.rs`) |
| `src/std/compression.sg` | The same module's compression (`Deflate`, `Zlib`, `Gzip`, `Compressor`, `Decompressor`, `CompressError`): zlib's `deflate.c` and `trees.c` written again in SG, with zlib's licence at its top, and an inflate of tables of 9 and 6 bits. The checker looks at a level written in the program (`level_written` in `src/check.rs`) |
| `src/signals.rs` | For the interpreter: the signals of the process, a handler that writes the kind into a pipe and a thread that hands each to its listeners, waking the fiber that waits through the scheduler |
| `src/lsp.rs` | The language server (`sg lsp`): the messages of the Language Server Protocol on the input and output, the program each open file belongs to, a check 300 ms after the last change on a thread of its own, the first error as a diagnostic and its fix as a quick fix, positions in UTF-16 as the protocol counts them |
| `src/json.rs` | JSON read and written for the language server, so that `sg` keeps no dependencies |
| `src/lib.rs`, `src/main.rs` | Library entry point and the command line |

It is a library with a thin command line on top, so that the language server (`sg lsp`) uses the same parser
and checks as the command. An error carries its hint as text and, where the code it gives is certain, as an
edit (`Diag::fix`), which an editor applies with one click.

## Exit codes

The command's exit codes, tested from outside in `tests/cli.rs` (1322):

| Code | When |
|---|---|
| 0 | All went well: the program ended, every test passed, the build was made |
| what `exit` gives | The program called `exit(code)`, a `byte` |
| 1 | An error the checker or the C compiler finds, a panic, a failed test, a program that leaks or races |
| 2 | A command line that is not one of the usages (a second program, a flag given twice, `-o` or `--target` with nothing after it, `-o` with `emit-c`), a folder of `-L` that is not there, a program that cannot be read (no such file, not UTF-8, a folder with no `.sg` file), an output into a folder that is not there, an output that is a folder (1398) |
| 0 or 1, `sg lsp` | The language server ends with 0 when the editor asked it to (`shutdown`, then `exit`), and with 1 when its input ends or it is told `exit` without `shutdown`, as the protocol has it |
| `SIGPIPE` | On Linux and macOS, an output closed before the program was through (`sg x.sg \| head`), interpreted and compiled, as C's and Go's programs end (1330), and before the tool was through with what it writes itself (`sg emit-c x.sg \| head`, 1399); on Windows that is 0 |
