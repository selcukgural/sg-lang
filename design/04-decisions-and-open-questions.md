# Decisions and open questions

The idea of the language and the principle every decision follows are on this page; the decisions themselves,
what is deferred, what is still open and what comes next have pages of their own, listed [at the end](#where-the-rest-is).
The reasons for the dropped generation model are in the [memory model](01-memory-model.md) file.

## The idea of the language

A language's reason to exist should fit in one sentence. The direction changed on 5 November, and the sentence
became: **a systems language as fast as C, without macros, that gives Rust's compile-time guarantees in a
notation that reads like C#.** Since 27 November 2026 (choices 2078 and 2079) it is: **a systems language small like
C, readable like C#, fast and memory safe like Rust.** The solutions come from Rust, the naming and the notation are ours. No arena. A
side project / research, with no revenue goal.

## Core principle: safe default

*Decided on 6 November 2026.* That people can trust this language matters as much as speed. If something can go
wrong, the default behaviour is not to stay silent but to warn or to stop. The risky path is allowed, but it is chosen on purpose:

- Checks are on by default and give a panic or a compile error.
- The unchecked path is visible in the code: `wrappingAdd`, `unsafe { }`.
- The choice is made per line. There is no compiler setting that turns checks off for the whole program.
- An error that can be caught at compile time is not left to run time: the code does not compile, and the error shows in the editor while typing. Panic
  is only for cases whose outcome depends on data and cannot be known at compile time (such as an index typed by the user).

- An error whose fix is known comes with code that shows the fix. An error says "what to write" as much as "what is wrong".

For this reason the checking part of the compiler is written as a library that the language server will also use. The rules for hints:

- The suggested code must compile, and the compiler's tests verify this. Where there is no certainty, no hint is given.
- The risky path (`unsafe`, `wrappingAdd`) is never suggested as a hint. The safe path is shown.
- A mechanical hint is applied in the editor with one click.
- A panic message also prints the values and the safe path: `index 5, length 3`, `xs.get(i)`.

If a performance decision conflicts with this principle, the principle wins, and the loss is recovered through the path that is written on purpose. Every new decision is
tested against it.

*Narrowed on 12 November 2026, in the author's words:* "to be as fast as C, and since it keeps memory safety, we take
Rust's way ... it keeps memory safety, it causes no memory leak, and we gain performance; and the language still offers
the developer the ways to do it safely. From now on we weigh this criterion in every choice." So the principle above
holds without exception where memory safety is at stake: a check that keeps memory safe (bounds, use after free, data
races) is never given up for speed. A run-time check that guards only the correctness of a result is weighed
differently: if removing it keeps memory safe and leaks nothing, the fast behaviour is the default of a release build,
the check stays on in debug builds and tests, and the checked form can always be written out (`tryAdd`). Integer
overflow was the first case; the bullet "there is no compiler setting" above no longer holds for it.

The goal of the language is not to be complex and to avoid bad notation. On its own the principle always pushes towards more rules, and every compile
error rule also rejects some correct code and adds to the rules that must be learned. So before a new rule is added
it is weighed with these four questions:

1. Does the error it catches silently give a wrong result, or is it visible anyway?
2. How often does it fire in real code?
3. Can the fix be shown in one line and with a hint?
4. Does it need a new concept?

A rule that does not satisfy all of them can still be added, but its cost is then accepted knowingly.

*Added on 11 November 2026.* The goal is performance and simplicity like C: the language stays as small as possible. Before a new feature
is added, the question is "can this be written without it", and there are never two ways to do the same thing. On the same day the decisions were reviewed with this in mind,
and what was removed is in the [dropped decisions](dropped-decisions.md).

*Restated on 12 November 2026, in the author's words.* The promise of the language is software that is safe and runs
fast: memory safe like Rust, as fast as C. It is not a promise of fast development. The language may be tiny, like C: C
lacks many features and everything is still written in it, operating systems included. Having the features Rust has is
not a goal. The two things that are not given up are performance and simplicity: a feature that costs nothing at run
time and does not complicate the language can stay (the collection methods, the text of a collection); a feature that
costs at run time needs a reason that is about safety, and one that adds concepts and rules needs to be worth them.

*Made exact on 27 November 2026 (choice 2078).* What SG is held to in speed is **safe Rust doing the same work**: it
is not to be slower. C is the yardstick, not the rival. In C the programmer keeps memory safe by hand, and pays
nothing at run time for it; SG keeps it safe itself, and where that needs a check while the program runs (the bounds
of a list, the count of a `shared` value, two windows of one list that may not overlap), C does not pay for the check
and safe Rust does. "Not slower than C" cannot hold at every such place by its own meaning; "not slower than the
language that gives the same guarantee" can. C stays what everything is measured against, since SG compiles through
C: a program of C written by hand is the ceiling, and the distance to it is what safety costs. So a place where SG is
slower than C is read in one of two ways. Where safe Rust pays the same, it is the price of the guarantee. Where safe
Rust does not, it is ours to mend: the code SG writes, or its library. [Slower than
C](../benchmarks/slower-than-c.md) says of each place which it is. Nothing else changes: what is new is not written
again in Rust to have something to measure against (where a program of C is what there is, as for a port, C is what
SG is measured against, and the cause of a difference is written down), and a case SG loses against Rust is
published as plainly as one it loses against C.

*Added on 12 November 2026.* We are not tied to a single language. Rust's guarantees, C#'s notation, Go's simplicity, Zig's
smallness: what each language does best is taken in the form that fits us, and is not taken if it does not fit. A proposal is accepted not because
"Rust does it this way" but because it fits the goals of this language.

## Where the rest is

| Page | What it holds |
|---|---|
| [Decisions by topic](rules/README.md) | Every rule of the language with its reason, 542 rows on 15 pages, and every topic from A to Z: [The foundations](rules/foundations.md) · [Ownership and borrowing](rules/ownership.md) · [Types](rules/types.md) · [`match`, `is` and narrowing](rules/patterns.md) · [Errors and panics](rules/errors.md) · [Statements, variables and names](rules/statements.md) · [Numbers](rules/numbers.md) · [Text and characters](rules/text.md) · [Collections](rules/collections.md) · [Modules and packages](rules/modules.md) · [Tasks, locks and async](rules/concurrency.md) · [Files, streams, the network and other programs](rules/input-output.md) · [C and the machine](rules/c-and-the-machine.md) · [The standard library](rules/library.md) · [The compiler and its tools](rules/tools.md) |
| [Deliberately deferred](deferred.md) | What waits until after the first version, and why: comptime, overloads, properties, generics of your own |
| [Directions](directions/README.md) | Four larger parts set out before they were built: async, calling C, reaching what C reaches, Windows |
| [Open questions](open-questions.md) | What is not settled yet, and how the questions already closed were closed |
| [Performance note](performance-note.md) | What the decisions cost at run time, as expected and as first measured |
| [Next steps](next-steps.md) | The plan: example programs, the interpreter and its back end slice by slice, then a known program written in SG |
| [Dropped decisions](dropped-decisions.md) | What no longer holds, and what was removed to keep the language small |
| [The first drafts](first-drafts.md) | The old notation beside the new, and the gaps found in the first day's texts |

The choices made while building each slice, with their numbers, are in [the records by day](decisions/README.md).
