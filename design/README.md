# Design

The language on paper: what was decided and why. The code that builds it is in
[`interpreter/`](../interpreter/README.md) (what runs today: [its documents](../interpreter/docs/README.md)), and how fast it runs is in [`benchmarks/`](../benchmarks/README.md).

## The pages

| Page | What it holds |
|---|---|
| [Decisions and open questions](04-decisions-and-open-questions.md) | The idea of the language and the principle every decision follows, with the way to the rest. Start here |
| [Decisions by topic](rules/README.md) | The rules of the language, each with its reason, one page for each part, and every topic from A to Z |
| [Syntax draft](02-syntax-draft.md) | How SG is written, with the reasons behind each form |
| [Example programs](05-examples.md) | Programs written by hand to put the notation under stress |
| [From Rust to SG](06-rust-to-sg.md) | What a Rust programmer writes, and what SG writes for it |
| [Choices of each slice](decisions/README.md) | Every choice made while building the language, slice by slice and day by day, with its number |
| [Directions](directions/README.md) | Async, calling C, reaching what C reaches and Windows, each set out before it was built |
| [Deliberately deferred](deferred.md) | What waits until after the first version, and why |
| [Open questions](open-questions.md) | What is not settled yet |
| [Performance note](performance-note.md) | What the decisions cost at run time, as expected and as first measured |
| [Next steps](next-steps.md) | The plan, and the record of every slice of the interpreter and its back end |
| [Dropped decisions](dropped-decisions.md) | What no longer holds, and what was removed to keep the language small |
| [The first drafts](first-drafts.md) | The old notation beside the new, and notes on the first day's texts |
| [Where to start designing a language](03-roadmap.md) | The order this project follows, written before the first line of code |
| [Memory model](01-memory-model.md) | The first memory model, dropped for Rust's rules of ownership; kept for the record |
