---
title: Getting started
description: Install sg, run a first program, build it, test it, and read what it says when something goes wrong.
sidebar_position: 1
---

# Getting started

This chapter takes you from nothing to a program that reads its command line, writes to both outputs, ends
with a code of its choosing, and has tests. On the way it shows what SG says when a program is wrong, because
the compiler's messages are a good part of how the language teaches itself.

## Installing

SG is built from source for now. You need Rust (stable) to build the `sg` command, and a C compiler to build
programs, since `sg build` compiles through C:

```bash
git clone https://github.com/selcukgural/sg-lang
cd sg-lang/interpreter
cargo build --release
```

The command is `target/release/sg`; put it on your `PATH`. [Install](../install.md) has the details for each
system and the editor extension.

## A first program

Save this as `hello.sg`:

```sg
void main() {
    print("Hello, world")
    var name = "Pasha"
    print($"Hello, {name}, from SG")
}
```

It prints:

```text
Hello, world
Hello, Pasha, from SG
```

Run it with `sg hello.sg`. `void main()` is where a program starts: `void` says it returns nothing. A line
ends a statement, so there are no semicolons, and a block is in braces. `$"..."` is a text with values in
braces; [Text](./03-text.md) has more of it.

## Running and building

`sg` does the same program two ways:

```bash
sg hello.sg                      # runs it in the interpreter, at once
sg build hello.sg                # compiles it through C to ./hello: a debug build
sg build hello.sg -o greet       # the same, named greet
sg build hello.sg --release      # optimised
```

The interpreter starts at once and is the quickest way to try something. A compiled program runs at about the
speed of C, since it is C underneath. The two print the same for every program; where they do not, that is a
bug in SG.

A debug build and a release build differ in one rule you meet early: integer overflow. In the interpreter and
in a debug build, `int.MaxValue + 1` stops the program with a panic; in a release build it wraps around, as it
does in C, because the check costs time in the tightest loops. Overflow never makes memory unsafe, so the
release build can afford to skip it. Every other check that keeps memory safe, such as an index past the end of
a list, stays in every build. [Values](./02-values.md) says how to choose wrapping or checking yourself.

## Tests

A test is written beside the code, at the top level of the file:

```sg test
int twice(int value) {
    return value * 2
}

test "twice doubles" {
    assert(twice(21) == 42)
}

test "twice keeps the sign" {
    assert(twice(-3) == -6)
}

void main() {
    print(twice(21))
}
```

`sg test example.sg` runs every test and prints a line for each:

```text
ok    twice doubles
ok    twice keeps the sign
2 test(s) passed
```

`sg test example.sg --compiled` runs the same tests compiled. A normal run (`sg example.sg`, `sg build`) leaves
the tests out. When an `assert` fails, the line says `FAIL`, `sg test` ends with code 1, and the panic shows
both sides of the comparison. Had the second test been written `assert(twice(-3) == 6)`, it would print:

```text
ok    twice doubles
FAIL  twice keeps the sign
panic: assert failed: twice(-3) == 6
  left: -6
  right: 6
  --> example.sg:10:5
 10 |     assert(twice(-3) == 6)
    |     ^^^^^^^^^^^^^^^^^^^^^^
1 of 2 test(s) failed
```

## A program that is a folder

A program can be several files. Put them in a folder and run the folder: `sg shop`, or
`sg test shop` for its tests. Every `.sg` file directly in the folder belongs to the program, and the files see each other's
functions and types with nothing written. A folder inside it is a module, which a file brings in with `use`:

```text
shop/
  main.sg          void main() { ... }
  prices.sg        int price(int count) { ... }
  stock/
    count.sg       public int total(int count) { ... }
```

`main.sg` can then be:

```sg nocheck
use stock

void main() {
    print(total(3))
    print(stock.total(4))
    print(price(2))
}
```

This program needs the other files of the folder, so it is not run on this page. `price` comes from
`prices.sg` with nothing written; `total` comes from the module `stock`, either through `use stock` or by its
full name, `stock.total(4)`. Only the names marked `public` in a module are seen from outside it. If you leave
out the `use` and write `total(3)`, the compiler says what is missing:

```text
error: there is no function named `total`
  --> shop/main.sg:2:11
 2 |     print(total(3))
   |           ^^^^^
  hint: `use stock` at the top of the file
```

`sg build shop -o shop-app` compiles the folder; give the program a name with `-o`, since the folder already
has the name `shop`.

## The command line

`main` can take the words after the program's name:

```sg
void main(List<string> args) {
    if args.length() == 0 {
        print("no words")
        return
    }
    for word in read args {
        print(word)
    }
}
```

It prints:

```text
no words
```

`sg words.sg red green` runs it with two words, and a compiled program takes them the same way:
`./words red green` prints `red` and `green`. The list holds only the words, not the program's own name.
`for word in read args` goes over the list without taking it apart; `read` is explained in
[Functions](./04-functions.md).

## Exit codes and the standard error

`print` writes to the standard output. `printError` writes the same way to the standard error, where messages
for a person belong, so that they do not mix with output another program reads. `exit(code)` ends the program
at once with that code:

```sg
void main(List<string> args) {
    print("the result goes here")
    printError("a message for the person goes here")
    if args.length() > 3 {
        printError("too many words")
        exit(2)
    }
}
```

Its standard output is:

```text
the result goes here
```

and the second line goes to the standard error: in a terminal you see both, and `sg example.sg 2>/dev/null`
shows only the first. A program that reaches the end of `main` ends with code 0. The code is a `byte`, because the
system keeps only eight bits of it, so a code that does not fit is refused when the program is compiled:

```sg error
void main() {
    exit(256)
}
```

```text
error: `256` does not fit in byte
  an exit code is a `byte`: the system keeps eight bits, so 256 would end as 0
  --> example.sg:2:10
 2 |     exit(256)
   |          ^^^
```

`exit` does not run the rest of the program, and it does not flush a file you are still writing. A program
whose work can fail can also be written `Result<void, E> main()`: if it ends with an error, `sg` prints
`error:` and the error and exits with code 1. [Null and errors](./06-null-and-errors.md) covers errors as values.

## When the compiler refuses a program

The compiler checks the whole program before anything runs, and stops at the first fault. Its message names the
file, the line and the column, shows the line, and often ends with a hint:

```sg error
void main() {
    var count = 3
    count = "three"
}
```

```text
error: expected `int`, found `string`
  --> example.sg:3:13
 3 |     count = "three"
   |             ^^^^^^^
```

A variable keeps the type it started with, and `"three"` is text. Read the hint when there is one: it is
usually the fix. The editor extension shows the same message as you type.

## When a program panics

Some faults can only be found while the program runs: an index past the end of a list, a division by zero, an
overflow. SG does not go on with a wrong value; it stops the program with a *panic*, which names the fault and
the line, and the program ends with code 1:

```sg panic
void main() {
    var scores = List<int> { 90, 85, 77 }
    print(scores[3])
}
```

```text
panic: index 3 is out of range: the length is 3
  --> example.sg:3:11
 3 |     print(scores[3])
   |           ^^^^^^^^^
  hint: `.get(i)` reads with `null` instead; before a write, compare the index with `.length()`
```

A compiled program panics with the same message, in a debug build and in a release build. You can panic on
purpose too, when something has gone wrong that the program cannot handle:

```sg panic
void setAge(int age) {
    if age < 0 {
        panic($"an age of {age} is below zero")
    }
    print(age)
}

void main() {
    setAge(-1)
}
```

```text
panic: an age of -1 is below zero
  --> example.sg:3:9
```

A panic is for a bug, not for an error the program expects, such as a file that is not there; those are values
that the program looks at, as [Null and errors](./06-null-and-errors.md) shows.
