# How it is tested

Tests are golden files: every `.sg` file in `examples/` and `tests/cases/` is run and its output (what it
printed, then the error or panic message) is compared with the `.out` file next to it. After an intended change,
regenerate them with `UPDATE_GOLDEN=1 cargo test` and review the diff. What the program wrote to the standard
error comes after what it printed, and an exit code other than 0 and 1 is a last line (`exit code 3`). A
`.args` file next to a program gives its command line, one word on each line, where `{dir}` is an empty folder
of its own to write files into (it is written back as `{dir}` in the output); a `.in` file gives its input.

The tests of `tests/golden.rs` build and run their programs several at a time: together they keep as many
programs going as the machine has threads, and a report lists what differs in the order of the files, as a run
one by one would. Each program is built and run in a folder of its own, and a program that listens takes a port
the system chooses (`127.0.0.1:0`), so two programs never meet. On a machine of 16 threads the file takes a
little under two minutes, where one program at a time took thirteen; the longest test is `compiled`, and most
of its time is the C compiler.

Programs with modules are in `tests/modules/`: the folders there (`stock`, `billing`, `shop/parts`, `leaky`,
`links`, `aliasleak`, `clib`, `timing`) are modules, and each `.sg` file next to them is a program of one file that uses them. A folder with a `main.sg`
(`tests/modules/rootdir`, `examples/shop`, `examples/zlib`) is a program run as a folder; its output is in `main.out` and what
the test command prints for it in `tests.out`.

Two kinds of trying are not part of `cargo test`. The front end is fuzzed (`fuzz/` at the root of the
repository, a crate of its own with its own `README.md`): on any text, the parser, and all `sg build` does
before the C compiler runs, must end with a result or a message, never a panic, a hang or a stack run out
(1349-1355). And mutants are tried on the functions that make the rules of safety, named in
`.cargo/mutants.toml` with the commands that run them (1356-1360): a mutant the tests do not notice is a
test that is missing. A change to those functions is tried where it touched them with `git diff | cargo
mutants --in-diff /dev/stdin`, with the arguments `.cargo/mutants.toml` gives.
