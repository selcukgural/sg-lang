---
title: How it got here
description: How SG started, the design that was dropped, and how the first code was written.
sidebar_position: 2
---

# How it got here

SG started on 4 November 2026 from a handful of questions about C, Rust, Zig, Go and C#: why C left memory to
the programmer, why ownership took forty years to arrive, and whether its guarantees could be had in a smaller
language. The name is the author's initials; source files end in `.sg`.

The first design used generation-checked pointers and arenas. It was dropped a day later for Rust's ownership
rules, which cost nothing when the program runs. Over the next week twelve example programs were written on
paper, and each question they raised was settled and written down. On 11 November the goal was put in one
line, performance and simplicity like C, and the design was made smaller to fit it.

The first code followed on 12 November: an interpreter in Rust, built one slice of the language at a time,
then the borrow check, then a back end that compiles through C, with every compiled program compared against
the interpreter and run under the sanitizers.

## What comes next

The plan is to write a well-known, widely used program in SG, one that has not been rewritten in Rust yet, and
see how the claim holds on real software. The program is [gifsicle](https://www.lcdf.org/gifsicle/), the C tool
that shrinks and edits GIF images: its work is bytes and bits, so it can be measured against the C original,
and it has had a heap overflow of the kind SG rules out. It is taken up when the language is further along.

## The full record

Every slice and every choice is written down in the repository:
[next steps](https://github.com/selcukgural/sg-lang/blob/main/design/next-steps.md) tells the slices in order,
[how far it has come](https://github.com/selcukgural/sg-lang/blob/main/interpreter/docs/progress.md) says what each one built, and
[the choices of each slice](https://github.com/selcukgural/sg-lang/blob/main/design/decisions/README.md) keeps every choice, day by day, with
its number.
