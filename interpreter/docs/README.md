# The interpreter's documents

What the interpreter and the C back end do today, how they are built and tested, and what comes next.
How to run them is in [the interpreter's README](../README.md).

| Page | What it holds |
|---|---|
| [What runs today](features/README.md) | Every part of the language and its library that runs, by topic |
| [Rules already enforced](rules-enforced.md) | The design rules a program is held to, at compile time and at run time |
| [The C back end](c-back-end.md) | How a program is compiled through C, and what is compiled, slice by slice |
| [How it is built](architecture.md) | The source files, each with its role, and the exit codes of the command |
| [How it is tested](testing.md) | Golden files, programs with modules, fuzzing and mutants |
| [Not here yet](not-here-yet.md) | What each part still lacks, with the numbers of the choices that left it out |
| [How far it has come, and what is next](progress.md) | The order in which it was built, and the next slices |

Every choice made while building it is in [`design/decisions/`](../../design/decisions/README.md), and
how fast compiled programs are is in [`benchmarks/`](../../benchmarks/README.md).
