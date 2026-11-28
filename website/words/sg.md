---
name: sg
kind: command
summary: Runs a program in the interpreter, from a file or a folder; the words after it are the program's own.
see: sg build, sg test, sg help, use, extern, Stdin
learn: ../docs/learn/book/01-getting-started.md#running-and-building
---

## Forms

| Form | What it is |
|---|---|
| `sg file.sg` | runs the program of one file, with the folders beside it as its modules |
| `sg folder` | runs the program of a folder: every `.sg` file directly in it, and the folders in it as its modules |
| `sg file.sg word...` | the words after the program are its own: `void main(List<string> args)` gets them |
| `sg -L folder file.sg`, `sg -Lfolder file.sg` | looks for the libraries of `extern` blocks in that folder first; written before the program, once for each folder |
| `sg --help`, `sg -h` | prints this page |
| `sg` | with nothing after it, writes how `sg` is used and ends with code 2 |

## What it means

`sg` checks the whole program, then runs it in the interpreter, at once and without making a program file.
The interpreter is the reference: a program built with `sg build` prints the same, and it checks what a debug
build checks, integer overflow among the rest. It needs no C compiler, unless the program calls C through an
`extern` block: then the wrappers of those functions are built with the C compiler `CC` names, else `cc` (Clang
on Windows), and `-L` names a folder where a library lies.

A file is a program with the folders beside it as its modules, brought in with `use`. A folder is a program
whose root is every `.sg` file directly in it; one of them has `main`. Either way, when that folder has an
`sg.project`, the packages it names come in too, as `sg.lock` says (see `sg add`). The first word after the
folders of `-L` is the program, and every word after it is the program's own, even one `sg` would read itself (`-L`,
`--help`, `--`). A word comes in as the system gave it; a byte in it that is not UTF-8 becomes a character from
U+10FF80 to U+10FFFF, and goes back to the system as it came. The words `test`, `build`, `emit-c`, `lsp`,
`help`, `add`, `update`, `remove` and `lock` in the first place are commands, so a folder with one of those names
is run as `sg ./test`.

The program's input is the input of `sg`, which `Stdin` reads. `print` and `Stdout` write to the output;
`printError`, `Stderr`, a panic, an error of the checker and what `sg` itself has to say go to the standard
error. What was printed goes out before any message on the standard error. Into a pipe whose reader is gone
(`sg main.sg | head`), the program ends quietly, by `SIGPIPE` on Linux and macOS, as compiled code does.

The exit code says how it went:

| Code | When |
|---|---|
| 0 | `main` ended, or returned `Ok()` |
| the code of `exit(code)` | the program called `exit`: a `byte`, 0 to 255 |
| 1 | the checker refused the program; it panicked; `main` returned an error, which is written after `error:`; or it ended with `shared` values still alive, which are named |
| 2 | the command line is not one `sg` takes, or a path in it leads nowhere: a file that cannot be read, a folder with no `.sg` file, a folder of `-L` that is not there |

Deep recursion ends in a panic too, `stack overflow`, which names the call: the interpreter looks at what is
left of its stack of 1 GB before each call.

## Examples

### A file

```sg file title="hello.sg"
void main() {
    print("Hello, world")
}
```

```text
$ sg hello.sg
Hello, world
$ sg build hello.sg -o hello
$ ./hello
Hello, world
```

`sg` runs the file at once. `sg build` compiles the same program, which prints the same.

### The program's own words

```sg file title="words.sg"
void main(List<string> args) {
    print($"{args.length()} words")
    for word in read args {
        print($"[{word}]")
    }
}
```

```text
$ sg words.sg
0 words
$ sg words.sg red "two words" ""
3 words
[red]
[two words]
[]
$ sg words.sg --help -L vendor -- --release
5 words
[--help]
[-L]
[vendor]
[--]
[--release]
```

The list holds the words after the program, not its name. The quotes are the shell's: `"two words"` is one word,
and `""` an empty one. After the program `sg` reads no word, so `--help` and `-L` are the program's; a program
that takes flags of its own reads them with `CommandLine`.

### Input, output and the exit code

```sg file title="sum.sg"
Result<void, IoError> main() {
    long total = 0
    var bad = 0
    var line = string.withCapacity(80)
    while try Stdin.readLine(ref line) {
        var cents = long.parse(read line) ?? -1
        if cents < 0 {
            printError($"not a price: {line}")
            bad += 1
            continue
        }
        total += cents
    }
    print($"{total} cents")
    if bad > 0 {
        exit(3)
    }
    return Ok()
}
```

```text title="prices.txt"
450
1200
tea
75
```

```text
$ printf '450\n1200\n' | sg sum.sg
1650 cents
$ sg sum.sg < prices.txt; echo "exit code $?"
not a price: tea
1725 cents
exit code 3
$ sg sum.sg < prices.txt 2>/dev/null > total.txt
$ cat total.txt
1725 cents
$ printf '45\377\n' | sg sum.sg; echo "exit code $?"
error: invalid text at byte 2
exit code 1
```

`Stdin` reads what comes into `sg`: a pipe, a file, or the terminal. The line about `tea` goes to the standard
error, so `2>/dev/null` leaves it out and `total.txt` gets the total alone. `exit(3)` ends the program with that
code. A byte that is not UTF-8 is an error of `readLine`, which `try` hands to `main`: `sg` writes it after
`error:` and ends with code 1.

### A folder

```sg file title="shop/main.sg"
use stock

void main(List<string> args) {
    var count = int.parse(args.get(0) ?? "2") ?? 2
    print($"{count} cups: {price(count)} cents, {left(count)} left")
}
```

```sg file title="shop/prices.sg"
int price(int count) {
    return count * 450
}
```

```sg file title="shop/stock/count.sg"
public int left(int sold) {
    return 40 - sold
}
```

```text
$ sg shop
2 cups: 900 cents, 38 left
$ sg shop 5
5 cups: 2250 cents, 35 left
$ cd shop
$ sg . 3
3 cups: 1350 cents, 37 left
```

The program is `main.sg` and `prices.sg`, which see each other's names with nothing written; `stock` is a
module, which `main.sg` brings in with `use`. The words after the folder are the program's, as after a file.

### When the program stops

```sg file title="stock.sg"
void main(List<string> args) {
    var shelves = List<int> { 12, 0, 7 }
    for word in read args {
        var at = int.parse(word) ?? panic($"not a shelf: {word}")
        print($"shelf {at}: {shelves[at]}")
    }
}
```

```text
$ sg stock.sg 0 2 5; echo "exit code $?"
shelf 0: 12
shelf 2: 7
panic: index 5 is out of range: the length is 3
  --> stock.sg:5:30
 5 |         print($"shelf {at}: {shelves[at]}")
   |                              ^^^^^^^^^^^
  hint: `.get(i)` reads with `null` instead; before a write, compare the index with `.length()`
exit code 1
$ sg stock.sg 1 x 2>/dev/null; echo "exit code $?"
shelf 1: 0
exit code 1
$ sg build stock.sg -o stock
$ ./stock 1 x 2>/dev/null; echo "exit code $?"
shelf 1: 0
exit code 1
```

The lines printed before the panic go out first. The panic names the fault and the line, on the standard error,
and the program ends with code 1. A compiled program panics with the same text and the same code.

### A library of C in a folder: `-L`

```c title="dice/vendor/dice.c"
#include <stdint.h>

int32_t dice_mean(int32_t count, int32_t sides) {
    return count * (sides + 1) / 2;
}
```

```sg file title="dice/main.sg"
extern "dice" {
    int dice_mean(int count, int sides)
}

void main(List<string> args) {
    var count = int.parse(args.get(0) ?? "3") ?? 3
    print(unsafe dice_mean(count, 6))
}
```

```text
$ cd dice
$ cc -fPIC -c -o vendor/dice.o vendor/dice.c
$ ar rcs vendor/libdice.a vendor/dice.o
$ sg -L vendor main.sg 4
14
$ sg -Lvendor main.sg
10
```

`extern "dice"` links `-ldice`, which the C compiler looks for where it looks for libraries, and in the folder
`-L` names first. `-L` comes before the program; `sg build` and `sg test` take it too.

## Mistakes it tells

### A file of a folder, run alone

With the folder `shop` of the example above:

```text
$ sg shop/main.sg
error: there is no function named `price`
  --> shop/main.sg:5:28
 5 |     print($"{count} cups: {price(count)} cents, {left(count)} left")
   |                            ^^^^^
  hint: `price` is in `shop/prices.sg`; the files of a folder are one program: name the folder, `shop`, in place of the file
```

A file alone is a program of that file and the modules beside it: `prices.sg` is not part of it. The folder is
the program, `sg shop`; when a name not found is in a file beside the one given, the hint says so.

### `-L` after the program

With the folder `dice` of the example above:

```text
$ cd dice
$ sg main.sg -L vendor 2>&1 | head -1
error: the library `dice` was not found
```

After the program, `-L` and `vendor` are the program's own words, so the library is looked for where the C
compiler looks alone. The message ends with the hint `` `-L folder` names a folder where the library lies ``.

### A folder named as a command

```sg file title="test/main.sg"
void main() {
    print("the program in test")
}
```

```text
$ sg test 2>/dev/null; echo "exit code $?"
exit code 2
$ sg ./test
the program in test
```

`sg test` is the command that runs tests, and it needs a path. `./test` is a path, so `sg` runs it.

### A path that leads nowhere

```text
$ sg nothere.sg; echo "exit code $?"
error: cannot read nothere.sg: No such file or directory (os error 2)
exit code 2
$ mkdir -p empty && sg empty; echo "exit code $?"
error: there is no `.sg` file in empty
exit code 2
$ sg -L nothere hello.sg; echo "exit code $?"
error: -L nothere: no such folder
exit code 2
```

### A module run as a program

```text
$ sg shop/stock
error: there is no `main` function
  --> shop/stock/count.sg:3:2
 3 | }
   |  ^
  hint: a program starts at `void main() { ... }`
```

`shop/stock` is a module of the program `shop`: it has no `main`.

### A `main` that returns a number

```sg error
int main() {
    return 3
}
```

```text
error: `main` is written `void main()` or `Result<void, E> main()`
  to see the command line: `void main(List<string> args)`; to await, `async` in front
  --> example.sg:1:5
 1 | int main() {
   |     ^^^^
```

The exit code is given with `exit(code)`, and an error by returning it from `Result<void, E> main()`.
