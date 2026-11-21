# Design

The language on paper: what was decided and why. The code that builds it is in
[`interpreter/`](../interpreter/README.md), and how fast it runs is in [`benchmarks/`](../benchmarks/README.md).

## The pages

| Page | What it holds |
|---|---|
| [Decisions and open questions](04-decisions-and-open-questions.md) | The rules of the language, each decision with its reason, what is deferred and what is still open. Start here |
| [Syntax draft](02-syntax-draft.md) | How SG is written, with the reasons behind each form |
| [Example programs](05-examples.md) | Programs written by hand to put the notation under stress |
| [From Rust to SG](06-rust-to-sg.md) | What a Rust programmer writes, and what SG writes for it |
| [Choices of each slice](decisions/README.md) | Every choice made while building the language, slice by slice and day by day, with its number |
| [Where to start designing a language](03-roadmap.md) | The order this project follows, written before the first line of code |
| [Memory model](01-memory-model.md) | The first memory model, dropped for Rust's rules of ownership; kept for the record |
