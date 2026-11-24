# hashsum

The SHA-256 of each file it is given, worked out by a pool of workers and written in the order the files were
given, as GNU's `sha256sum` writes them: written in SG, in Rust and in Zig, the same algorithm each time
(choices 1854-1860). GNU's coreutils have no `sha256sum` that hashes in parallel; people reach for
`xargs -P` or `parallel`, which give the lines in the order they end.

```
hashsum [-j N] [--] [FILE...]
```

Each line is the hash in hex, two spaces and the name. A name with a `\`, a line's end or a CR in it is
written with them as `\\`, `\n` and `\r`, and a `\` before the hash, as GNU does. `-`, or no file at all, is
the standard input; a second `-` finds it at its end and gives the hash of nothing, as GNU's does. A file
that cannot be read (missing, a folder, one the program may not read) is told on the standard error, the
others go on, and the program ends with 1. `-j N` is the number of workers, by default the processors the
program may run on; a `-j` without a number above 0, or an option not known, ends it with 2. A closed
output ends it by SIGPIPE, as it ends `sha256sum`; an output that is full, with 1 and an error.

What it cannot take: a name that is not UTF-8. SG's arguments are texts, and a program given one that is not
ends at once, before `main`, saying which (`args[0] is not valid UTF-8`, exit 1); Rust and Zig take any
bytes (choice 1859). Whether the language should give the bytes of an argument is a question for itself.

## The pool

The same in the three (choice 1855):

- the files go to the workers on a channel with room for twice as many items as there are workers;
- each worker opens a file, reads it in blocks of 64 KB into the hash, and sends what it found (the line,
  or the message of an error) with the file's place back on a second channel of the same room;
- the main thread gives out files and writes the answers in order, holding those that come early in a ring
  of that room. It never gives out a file more than that room past the first one not yet written, so
  neither channel is ever full, nothing waits on a send, and what is held is bounded whatever the order in
  which the workers end;
- the standard input is read by the main thread itself when its turn comes, so the first `-` takes it all
  whichever worker would have been free.

The channel's sender is made before the workers are given their receivers: a channel no sender holds has
ended, and the first SG version, which made it after them, had one worker of sixteen do all the work while
the others found the channel ended at once. That is now an error of SG's compiler (choice 1860).

| | Workers | Channel | SHA-256 | Processors |
|---|---|---|---|---|
| SG | `Task.run` | `Channel<T>`, the receivers cloned (1280) | `std/hash` `Sha256`, the processor's SHA instructions | `Task.processorCount()` (1856) |
| Rust | `std::thread::scope` | `crossbeam-channel`, the bounded channel people use for a pool (`std::sync::mpsc` has one receiver) | `sha2`, the processor's SHA instructions | `std::thread::available_parallelism` |
| Zig | `std.Thread.spawn` | `std.Io.Queue`, the library's own bounded queue (with `close`) | `std.crypto.hash.sha2.Sha256` | `std.Thread.getCpuCount` |

Approved, the Zig version was to have a queue of its own, a mutex and a condition; Zig 0.17's library has
`std.Io.Queue`, which is that, and each language uses its own tools (1855). Zig's `getCpuCount` counts the
processors of the affinity but not a cgroup's quota; the measured cases are on macOS, where the three agree.

SG had no way to ask how many processors there are: `Task.processorCount()` was added for the trial (choice
1856), counted as Rust's `available_parallelism` counts them so that interpreted and compiled agree.

## Tests

- `python3 trials/measure.py --check hashsum` builds the five builds and holds them to each other on names
  that are escaped, an empty file, a folder, a missing file, the input once and twice, a name after `--`,
  one worker, three, more workers than files, `-j 0` and an option not known: the same output and exit code.
  The system's `sha256sum` is run beside them, loose: on macOS it writes a name as it is.
- `python3 -I trials/hashsum/tests/check.py [BINARY...] [--interpreted] [--gnu SHA256SUM]` holds them to a model
  of GNU's `sha256sum` written in Python (choice 1858): names with a line's end, `\`, CR, spaces, a leading
  `-`, 255 bytes, forty folders deep, a link, a broken one, a link to a folder; a missing file, a folder,
  a file no one may read (left out as root), a FIFO filled while it is read, `/dev/null`; the input alone,
  with no file, three times; 5,000 files of every size to 200 KB with every `-j` from 1 to 9, 64 and 1000,
  one file 500 times, one of 300 MB among small ones; the output closed (SIGPIPE) and full (`/dev/full`, on
  Linux: exit 1 and an error); `-j` without a number, with 0, with a word, an option not known (exit 2).
  With `--gnu`, GNU's `sha256sum` is held to the model first, on every case without `-j`. On Linux
  (GNU coreutils 9.7, run as a user who is not root) the two SG builds, Rust, the interpreter and GNU: 122
  runs, none differed; on macOS the five builds and the interpreter: 171 runs, none differed. The interpreter
  is left out of the cases of thousands of files: it hashes in SG and runs one task at a time.
- `interpreter/tests/cli.rs`, `the_trial_of_hashsum`, runs the SG version interpreted and compiled on every
  test run.
