---
title: The repository
description: What each folder of the repository holds, the design documents, and the background essays.
sidebar_position: 3
---

# The repository

Everything about SG is kept in the open in [one repository](https://github.com/selcukgural/sg-lang): the design, the code, the measurements
and the background reading behind them.

## The folders

| Folder | What it holds |
|---|---|
| [design](https://github.com/selcukgural/sg-lang/blob/main/design/README.md) | The language itself, on paper, and [every choice made while building it](https://github.com/selcukgural/sg-lang/blob/main/design/decisions/README.md), slice by slice |
| [interpreter](https://github.com/selcukgural/sg-lang/blob/main/interpreter/README.md) | The code, in Rust: checker, interpreter, C back end, tests, and the programs measured against C; [its documents](https://github.com/selcukgural/sg-lang/blob/main/interpreter/docs/README.md) |
| [benchmarks](https://github.com/selcukgural/sg-lang/blob/main/benchmarks/README.md) | The measurements: SG against C, Rust and Zig, where it is still slower, and how it is measured |
| [trials](https://github.com/selcukgural/sg-lang/blob/main/trials/README.md) | Programs people write (`grep`, `wc`, ...) and the modules of the library in SG, Rust and Zig, measured against each other |
| [topics](https://github.com/selcukgural/sg-lang/tree/main/topics) | Background: essays on C, Rust, Zig and Go, and on the problem SG sets out to solve |
| [sources](https://github.com/selcukgural/sg-lang/blob/main/sources/reading-list.md) | Reading list: books, Hylo, Vale, Rust writings, tools |
| [website](https://github.com/selcukgural/sg-lang/blob/main/website/README.md) | This site, built with Docusaurus |
| [editors](https://github.com/selcukgural/sg-lang/tree/main/editors) | The extension for VS Code, which runs `sg lsp` |
| [brand](https://github.com/selcukgural/sg-lang/tree/main/brand) | The logo, its icons and how they are made; Pasha, the mascot, and Pasha's badge |

## The design documents

If you read one, read [decisions and open questions](https://github.com/selcukgural/sg-lang/blob/main/design/04-decisions-and-open-questions.md):
the idea of the language, the principle every decision follows, and the way to the rest, every decision by
topic with its reason and its cost, what is still open, and what comes next. The others:

| Document | What it holds |
|---|---|
| [Decisions by topic](https://github.com/selcukgural/sg-lang/blob/main/design/rules/README.md) | The rules of the language, each with its reason, one page for each part |
| [Example programs](https://github.com/selcukgural/sg-lang/blob/main/design/05-examples.md) | Twelve programs written before there was a compiler, and the questions they raised |
| [From Rust to SG](../learn/for-rust.md) | What a Rust programmer writes and what SG writes for it, with how far each line is built |
| [Syntax draft](https://github.com/selcukgural/sg-lang/blob/main/design/02-syntax-draft.md) | The ownership notation, and the earlier drafts that were dropped |
| [Memory model](https://github.com/selcukgural/sg-lang/blob/main/design/01-memory-model.md) | The first model (generation-checked pointers), kept for the record; it was dropped |
| [Roadmap](https://github.com/selcukgural/sg-lang/blob/main/design/03-roadmap.md) | Where to start when designing a language, the steps and the traps |

## The background essays

Best read in order:

| Essay | What it holds |
|---|---|
| [C and manual memory](https://github.com/selcukgural/sg-lang/blob/main/topics/01-c-and-manual-memory.md) | Why memory management was left to the developer |
| [History of Rust ownership](https://github.com/selcukgural/sg-lang/blob/main/topics/02-rust-ownership-history.md) | The roots of the idea, why it took 40 years, why C++ could not do it |
| [Zig](https://github.com/selcukgural/sg-lang/blob/main/topics/03-zig.md) | Where it stands on safety, how long the move from C takes |
| [Rust syntax and C#](https://github.com/selcukgural/sg-lang/blob/main/topics/04-rust-syntax-and-csharp.md) | Where the noise comes from, the "second C++?" question |
| [Go](https://github.com/selcukgural/sg-lang/blob/main/topics/05-go.md) | Its philosophy, its weak sides |
| [Safe C attempts](https://github.com/selcukgural/sg-lang/blob/main/topics/06-safe-c-attempts.md) | From Cyclone to C3, the layers of pointer safety |
| [Use-after-free](https://github.com/selcukgural/sg-lang/blob/main/topics/07-use-after-free.md) | The explicit error idea, why a simple table is not enough |
