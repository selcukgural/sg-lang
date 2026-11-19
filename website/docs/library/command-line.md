---
title: The command line
description: CommandLine and Arguments, the flags, options and words a program takes, read the GNU way.
sidebar_position: 12
---

# The command line

`void main(List<string> args)` gets the words written after the program's name. `CommandLine` reads them
the way most tools on Linux and macOS do: a program declares the flags, the options with a value and the
words it takes, and gets them back by name, with `--help` written for it. It is written in SG in the module
`std`.

| Signature | What it does |
|---|---|
| `CommandLine CommandLine(read string program, read string about)` | A command line for the program `program`; `about` is the first line of its help (empty for none) |
| `void cl.flag(read string name, char? letter, read string help)` | A flag, there or not: `--name`, and `-l` for its letter (`null` for none) |
| `void cl.option(read string name, char? letter, read string valueName, read string help)` | An option with a value: `--name x`, `--name=x`, `-l x`, `-lx` |
| `void cl.positional(read string name, read string help)` | A word that must be given, the next one in order |
| `void cl.rest(read string name, read string help)` | The words left after the declared ones, none or more; once, last |
| `void cl.version(read string version)` | Adds `--version`, which prints the program's name and `version` |
| `Arguments cl.parseOrExit(read string[] args)` | The arguments, or the program ends: the help or the version printed with the code 0, an error to the standard error with 2 |
| `Result<Arguments, CommandLineError> cl.parse(read string[] args)` | The arguments, or why they could not be read; the help and the version are errors too |
| `string cl.help()` | The text `--help` prints |
| `bool a.flag(read string name)` | Whether the flag was given |
| `read string? a.option(read string name)` | The value the option was given last, `null` when it was not |
| `read List<string> a.options(read string name)` | Every value the option was given, in order |
| `read string a.positional(read string name)` | The word given in that place |
| `read List<string> a.rest()` | The words left |

Names are lower-case letters, digits and `-`; a short letter is an ASCII letter or digit, and `-h` is the
help's own. The values are text: a program that wants a number reads it with `int.parse`.

```sg
void main() {
    var cl = CommandLine("gifcrop", "Crops GIF images.")
    cl.flag("verbose", 'v', "Print each step")
    cl.option("output", 'o', "FILE", "Where to write")
    cl.flag("dry-run", null, "Show what would be done")
    cl.positional("input", "The GIF to read")
    cl.rest("files", "More GIFs")
    cl.version("1.2.0")

    // A real program passes `args`; here the words are written out.
    var args = List<string> { "in.gif", "-vo", "small.gif", "more.gif", "--dry-run" }
    var a = cl.parseOrExit(read args)
    print(a.positional("input"))
    print(a.option("output") ?? "out.gif")
    print(a.flag("verbose"))
    print(a.flag("dry-run"))
    print(a.rest().length())
    print("--")
    print(cl.help())
}
```

It prints:

```text
in.gif
small.gif
true
true
1
--
Crops GIF images.

Usage: gifcrop [options] <input> [files...]

Arguments:
  <input>              The GIF to read
  [files...]           More GIFs

Options:
  -v, --verbose        Print each step
  -o, --output FILE    Where to write
      --dry-run        Show what would be done
  -h, --help           Print this help
      --version        Print the version
```

## How the words are read

- `-vq` is `-v -q`, and `-vo small.gif` or `-vosmall.gif` ends with the option that takes the value.
- An option that takes a value takes the next word, whatever it is: `--offset -5` gives `-5`.
- Options may come after the words: `gifcrop in.gif -v` is `gifcrop -v in.gif`.
- After `--` every word is a word, even one that starts with `-`; a lone `-` is a word too (by custom, the
  standard input).
- An option given twice: `a.option` gives the last value, `a.options` all of them.
- A long name is never shortened: `--verb` is not `--verbose`. A script that wrote `--verb` would read
  another option the day `--verbatim` is added. The nearest declared name is told instead.

## When the words are wrong

`parseOrExit` writes the error and a line that names `--help` to the standard error, and ends the program
with the code 2, as most tools do:

```text
gifcrop: unknown option '--verbos' (did you mean '--verbose'?)
Try 'gifcrop --help'.
```

A program that wants to do something else uses `parse`, which gives a `CommandLineError`: `HelpAsked(text)`,
`VersionAsked(text)`, `UnknownOption(word, suggestion)`, `MissingValue(option)`, `UnexpectedValue(option)`,
`MissingArgument(name)` or `TooManyArguments(word)`. `{cause}` writes the same line `parseOrExit` writes.

```sg
void main() {
    var cl = CommandLine("copy", "")
    cl.positional("from", "What to copy")
    cl.positional("to", "Where to put it")
    var tries = List<string> { "a.txt", "--force", "a.txt b.txt c.txt" }
    for line in read tries {
        var words = List<string>()
        for word in line.splitWhitespace() {
            words.add(word.clone())
        }
        match cl.parse(read words) {
            Ok(a) => print($"{a.positional("from")} -> {a.positional("to")}")
            Error(HelpAsked(_)) => print("help")
            Error(cause) => print($"copy: {cause}")
        }
    }
}
```

It prints:

```text
copy: missing <to>
copy: unknown option '--force'
copy: unexpected argument 'c.txt'
```

## Mistakes of the program

A name declared twice, a letter taken twice, `-h` or `--help` declared by the program, a name that is not
lower-case, a `positional` after the `rest`; asking `a.flag("verbos")` when `verbose` was declared, or
`a.option` of a flag: each is a panic that says what to write instead. The names are text, so the compiler
cannot see them, but the declaring runs on every run, so the first run shows the mistake whatever the
command line holds.
