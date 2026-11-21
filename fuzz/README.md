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
| `regex` | `Regex` of the language's own module (choice 1468): the input up to its first line break is a pattern compiled when the program runs, the rest a text it is matched against every way (`isMatch`, `find`, `findAll`, `captures`, `replaceAll`, `split`), in the interpreter |
| `inflate` | the decompressors of the language's own module (choice 1492): the first byte chooses DEFLATE, zlib or gzip, the rest is opened with a limit of a megabyte; an input of up to 2 KB is also compressed at the level its second byte chooses and must open into itself, in the interpreter |
| `zip` | the reader of zip archives of the language's own module (choice 1512): the input is written to a file and opened by `ZipReader`, each entry read with a limit of a megabyte; an input of up to 2 KB is also written by `ZipWriter` as an entry stored and one deflated, and must read back as itself, in the interpreter |

An input that is not UTF-8 is passed over: `sg` does not read such a file. An input of even
length is built as a debug build, one of odd length as a release build. A program is given a path
in a folder that is not there, so `embed` and `extern "./x.c"` find nothing and each input gives
the same on every run.

What counts as a finding: a panic of Rust (the targets are built with debug assertions, so an
overflow that a release build of `sg` would wrap is one), a stack run out, more than 2 GB of
memory, or an input that takes more than 10 seconds. A message of `sg` about the program is the
answer expected. For `regex` a panic of the SG program is a finding as well: a pattern and a text
end with a match or a `RegexError`; for `inflate` too: any bytes end with what they open into or a
`CompressError`; and for `zip`: any bytes end with the entries or a `ZipError`. While the targets are built (`cfg(fuzzing)`), the check of a pattern of `Regex` when
the program is compiled, the one place the compiler runs code (the `std` module's own, choice 1607), may
take 1 000 000 steps instead of 100 000 000 (choice 1352); `sg` itself is built as before.

## Running

It runs in a Linux container with Rust nightly and cargo-fuzz. The image is built once, with the
network, and again when `Cargo.lock` changes:

    podman build -t sg-fuzz -f fuzz/Containerfile fuzz

Then, from the folder of the repository, without the network:

    fuzz/run.sh check 10800      # the target and how many seconds; JOBS=6 by default

The corpus and what is found are kept in the podman volume `sg-fuzz-data`, not in the repository:
`/data/corpus/<target>` and `/data/artifacts/<target>`. The first run starts the corpus from the
programs of `interpreter/tests/cases` and `interpreter/examples` (for `inflate`, the small programs of
`tests/cases` compressed by gzip; for `zip`, the archives of `tests/zip-suite`); `sg.dict` gives the fuzzer the
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
consts (made of operators and casts, masked so that none overflows, since a const calls no function),
`print` with a format, `Copyable` structs, enums and `match` on them, `T?`, lists, text, and
functions that give a `Result`.

With `--borrow` first (`BORROW=1` for `run.sh`), the programs borrow (stage 3, choice 1593,
`src/borrow.rs`): `read` and `ref` borrows of texts and lists kept in variables, in struct fields, in
lists and arrays of structs and in `Disposable` values; given to functions as they are and lent again,
and a range of a list lent as `ref` beside a range that may be of the same list (1610);
taken out of what a call returns and beside a call that changes what they borrow in the parts of one
value; and the texts and lists they borrow from changed, replaced and moved, in `if`, `match`, `for`
over a range, a list or a `shared` list, and `while` whose condition changes a list, with `break` and
`continue`. These programs are not written to be right: while the checker refuses one, the statement
its first error names is taken away, so what is run keeps the borrows and changes nearest the rules.
Every text is long enough for the heap and every change frees what it replaced, so a borrow the
checker let live past a change reads freed memory, and the address sanitizer says so.

    cargo run --release -- --borrow once 42      # fitted, then run both ways
    cargo run --release -- --borrow gen 42       # the program as fitted (with SG), or as written

The first refusal of a program for anything but borrowing or a value never read is the generator's
mistake, kept in `rejected`. Moves of other values than lists, and tasks, come later.
