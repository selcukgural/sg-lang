# Fuzzing the front end of `sg`

The lexer, the parser, the checker, the consts it finds and the C written from a program must end
with a result or a message on any text, never with a panic, a hang, a stack run out or memory run
out (choices 1323 and 1349-1355 in `interpreter/README.md`). This crate holds the targets that
try them with libFuzzer. It is a crate of its own, so that `sg` keeps no dependencies, and it is
not part of `cargo test`.

| Target  | What it runs on each input                                                                       |
|---------|--------------------------------------------------------------------------------------------------|
| `parse` | the lexer and the parser (`sg::parser::parse`), and the message of an error                      |
| `check` | all `sg build` does before the C compiler runs (`sg::emit_c_sources`), and the message of an error |

An input that is not UTF-8 is passed over: `sg` does not read such a file. An input of even
length is built as a debug build, one of odd length as a release build. A program is given a path
in a folder that is not there, so `embed` and `extern "./x.c"` find nothing and each input gives
the same on every run.

What counts as a finding: a panic of Rust (the targets are built with debug assertions, so an
overflow that a release build of `sg` would wrap is one), a stack run out, more than 2 GB of
memory, or an input that takes more than 10 seconds. A message of `sg` about the program is the
answer expected. While the targets are built (`cfg(fuzzing)`), a const may take 1 000 000 steps
instead of 100 000 000 (choice 1352); `sg` itself is built as before.

## Running

It runs in a Linux container with Rust nightly and cargo-fuzz. The image is built once, with the
network, and again when `Cargo.lock` changes:

    podman build -t sg-fuzz -f fuzz/Containerfile fuzz

Then, from the folder of the repository, without the network:

    fuzz/run.sh check 10800      # the target and how many seconds; JOBS=6 by default

The corpus and what is found are kept in the podman volume `sg-fuzz-data`, not in the repository:
`/data/corpus/<target>` and `/data/artifacts/<target>`. The first run starts the corpus from the
programs of `interpreter/tests/cases` and `interpreter/examples`; `sg.dict` gives the fuzzer the
words and marks of the language.

An input that fails is made smaller with `cargo fuzz tmin` and becomes a test in
`interpreter/tests/cases` before it is fixed (choice 1321).

## Fuzzing for differences

`difference/` is a crate of its own, with no dependencies, on stable Rust (choices 1324 and
1361-1368). Its generator makes, from a seed, a program of SG that the checker takes, that ends and
whose only output is `print`; the same seed gives the same program on every machine. Each program
is run by the interpreter (the `sg` program, in a process of its own) and compiled as a debug build
under the address and undefined-behaviour sanitizers, and the two must print the same, a panic's
message and its code included; when it does not panic, its release build (`-O2`, under the
undefined-behaviour sanitizer) must print what the debug build printed.

    cargo run --release -- gen 42           # the program of a seed
    cargo run --release -- once 42          # runs it both ways and says what it found
    cargo run --release -- run out 600 4    # seeds one after another, 4 at once, for 600 seconds
    cargo run --release -- reduce 42 small.sg

`SG` names the `sg` program and `CC` the C compiler. In the container, without the network:

    fuzz/difference/run.sh 7200 6

builds `sg` and the generator in the image of the tests and keeps what it finds, with its seed, in
the volume `sg-fuzz-data` (`/data/difference/<date>/found`). A program the checker refuses is kept
in `rejected` and read: the generator wrote what it should not, or the checker is stricter than it
should be. Before it starts, the run builds a small C program with the sanitizers, so that a C
compiler without them stops it at once instead of making every program a finding.

What it writes (stages 1 and 2 of choice 1365): the integer types, `bool`, arithmetic, the bit
operators, conversions, comparisons, `if`, `match`, `for` and `while`, functions, recursion, arrays,
consts, `print` with a format, `Copyable` structs, enums and `match` on them, `T?`, lists, text, and
functions that give a `Result`. Borrows and moves, and tasks, come later.
