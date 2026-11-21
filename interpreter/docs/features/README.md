# What runs today

Every part of the language and of its library that runs today (21 November 2026), by topic: each page
is a table, one row for each area, with what is supported and how it is written. What is compiled to C
is told in [The C back end](../c-back-end.md), the rules the checker holds a program to in
[Rules already enforced](../rules-enforced.md), and what is missing in [Not here yet](../not-here-yet.md).

| Page | Areas |
|---|---|
| [The language](language.md) | Functions; Modules; Types; Type aliases; Structs; Methods; Enums; `match`; Interfaces; Consts; Statics; `is`; Errors; Null; Variables; Control flow; Operators; Function values; Built-ins; Comparing by name |
| [Ownership and borrowing](ownership.md) | Types that carry a borrow; Ownership; Several owners; Changed through several owners |
| [Collections](collections.md) | `List<T>`; `HashMap<K, V>`; Text of a collection; `HashSet<T>`; Arrays; Slices; `for` over a collection |
| [Numbers and text](numbers-and-text.md) | Numbers; Text; Building text; Writing a number; Going over text; Characters; Unicode; Integer methods; Fractional methods; Numbers from text; Numbers as bytes; Constants |
| [Tasks and async](concurrency.md) | Tasks; Changed by several tasks; Async; Cancellation; Channels; A list of tasks; Atomic values |
| [C and the machine](c-and-the-machine.md) | Raw memory; Calling C; The layout of C; C calling SG; A library of SG for C; Without an operating system; A kernel; `volatile` |
| [The library](library.md) | Async input and output; Network; Time; Random numbers; Bytes as text, and checksums; The command line; JSON; Log; Regular expressions; Compression; Zip archives; Folders and the environment; Other programs; Signals; Files and the input |
| [Tests and the editor](tools.md) | Tests; Editor |
