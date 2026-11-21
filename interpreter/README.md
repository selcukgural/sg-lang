# Interpreter

A tree-walking interpreter for SG, written in Rust with no dependencies. Its job is to test the design by
running real programs, not to be fast. Source files end in `.sg`; the crate and the command are named `sg`.

```
cargo run -- examples/stack.sg                  # runs the program
cargo run -- examples/wc.sg examples/hello.sg   # the words after the file are the program's command line
cargo run -- test examples/tests_stock.sg       # runs its tests
cargo run -- examples/shop/main.sg              # a program with modules: the folders next to the file
cargo run -- test examples/shop                 # the tests of every module of the program in that folder
cargo run -- test examples/shop --compiled      # the same tests, compiled through C
cargo run -- examples/zlib/main.sg              # a library of C (zlib) behind a safe module
cargo run -- build examples/primes.sg -o primes # compiles it through C (needs `cc`)
cargo run -- emit-c examples/primes.sg          # prints the C
cargo test
cargo run -- build examples/primes.sg --release # the same as a release build: optimised, overflow wraps
cargo run -- build examples/bare --freestanding # an object for a machine without an operating system
examples/kernel/run.sh                          # a RISC-V kernel booted in QEMU (Clang, LLD and QEMU)
python3 bench/run.py                            # compiled SG against C written by hand
```

On Windows the same (but `examples/kernel/run.sh`, a script of `sh`), with LLVM's Clang (`winget install LLVM.LLVM`) and the Build Tools of Visual Studio;
the examples of zlib, SQLite and curl and their benchmarks take their libraries from vcpkg (`-L` for its
`lib` and `bin`, `CPATH` for its `include`), and the kernel's test QEMU
(`winget install SoftwareFreedomConservancy.QEMU`, which goes to `C:\Program Files\qemu`).

`cargo test` boots that kernel in QEMU too (`tests/kernel.rs`), so it needs a Clang that knows RISC-V
(`CC`, or `clang`), LLD and `qemu-system-riscv64`, and fails, saying what to install, without them. On
macOS, whose own Clang knows only its own processor: `brew install llvm lld qemu`, then
`CC=$(brew --prefix llvm)/bin/clang PATH=$(brew --prefix lld)/bin:$PATH cargo test`. On Linux the image
of `tests/Containerfile` has them.

## Documents

| Page | What it holds |
|---|---|
| [What runs today](docs/features/README.md) | Every part of the language and its library that runs, by topic |
| [Rules already enforced](docs/rules-enforced.md) | The design rules a program is held to, at compile time and at run time |
| [The C back end](docs/c-back-end.md) | How a program is compiled through C, and what is compiled, slice by slice |
| [How it is built](docs/architecture.md) | The source files, each with its role, and the exit codes of the command |
| [How it is tested](docs/testing.md) | Golden files, programs with modules, fuzzing and mutants |
| [Not here yet](docs/not-here-yet.md) | What each part still lacks, with the numbers of the choices that left it out |
| [How far it has come, and what is next](docs/progress.md) | The order in which it was built, and the next slices |

## Choices

Every choice made while building it, slice by slice and day by day, with what each slice found and how it
was tested, is in [`design/decisions/`](../design/decisions/README.md). The numbers that the code, the tests
and this page cite (`choice 886`, `1340`) are the numbers of those choices.

## Measurements

How fast compiled programs are is in [`benchmarks/`](../benchmarks/README.md): against C
([macOS](../benchmarks/against-c.md), [Windows](../benchmarks/against-c-windows.md)), the modules of `std`
against Rust and Zig ([the library](../benchmarks/library.md)), and every place where SG is still slower
([Slower than C](../benchmarks/slower-than-c.md)).
