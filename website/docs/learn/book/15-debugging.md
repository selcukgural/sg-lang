---
title: Debugging a program
description: A compiled program in gdb, lldb or the debugger of Windows, on the lines of its own files.
sidebar_position: 15
---

# Debugging a program

A debug build is made for the debugger of its system: a breakpoint is set on a line of a `.sg` file, the
stack names the program's own files and lines, and the source is shown from where it lies. SG compiles
through C, and the C it writes says, before each statement, which line of SG it stands for, so the C compiler
writes the lines of SG into what the debugger reads.

```bash
sg build main.sg                 # a debug build: ./main, and what the debugger reads
```

- **Linux**: what gdb reads is inside the program.
- **macOS**: it is beside the program, in `main.dSYM`, which the C compiler makes; lldb finds it there. A
  program lldb starts from a folder macOS guards, such as `~/Documents` or `~/Desktop`, waits until the
  window that asks leave for it is answered.
- **Windows**: it is beside the program, in `main.pdb`, which the debugger of Visual Studio and WinDbg read.

A release build carries none of it, as Rust's carries none by default; a flag of C gives it:

```bash
sg build main.sg --release -- -g
```

## A session

```sg
int sum(read List<int> xs) {
    var total = 0
    for x in xs {
        total += x
    }
    return total
}

void main() {
    var xs = List<int>()
    xs.add(3)
    xs.add(4)
    print(sum(read xs))
}
```

It prints:

```text
7
```

In gdb, stopped on the line of `total += x`:

```text
$ sg build main.sg && gdb -q ./main
(gdb) break main.sg:4
Breakpoint 1 at 0xab48: file /home/me/sums/main.sg, line 4.
(gdb) run
Thread 2 "main" hit Breakpoint 1, f_sum (v_xs=0xfffff7d4e808) at /home/me/sums/main.sg:4
4	        total += x
(gdb) bt
#0  f_sum (v_xs=0xfffff7d4e808) at /home/me/sums/main.sg:4
#1  0x0000aaaaaaaaabf0 in f_main () at /home/me/sums/main.sg:13
#2  0x0000aaaaaaaaac50 in sg_main (argc=1, argv=0xfffffffffc88) at /home/me/sums/main.c:3190
#3  0x0000aaaaaaaa1b8c in sg_main_thread (given=0xfffffffffac8) at /tmp/sg-ffi-102-18eb82bbdd8a1e54-0/main.c:476
(gdb) info locals
v_x_4 = 0xffffb0000c70
sg_t2 = 0xfffff7d4e808
sg_t3 = 0
v_total_1 = 0
```

lldb says the same with its own words (`breakpoint set --file main.sg --line 4`, `thread backtrace`,
`frame variable`). The program runs on a thread of its own, the second, which has the stack of 1 GB that
`main` gets.

## What the names are

The debugger shows the names of the C, which keep the names of SG inside them:

| In SG | In the debugger | |
|---|---|---|
| a function `sum` | `f_sum` | |
| a function `area` of a module `shapes` | `f_6shapes_area` | the length of the module's name, then the name |
| a method `Point.length` | `f_Point_length` | |
| a parameter `xs` | `v_xs` | |
| a variable `total` | `v_total_1` | a number after it, since two blocks may each have a `total` |
| a field `x` | `m_x` | `print v_p.m_x` |
| what a statement computes on the way | `sg_t2`, `sg_t3` | the place in a list a `for` is at, for one |

They are not given the names of SG themselves, since those would meet the names of C and of its headers (a
variable `errno`, `stdout` or `int`). A `List<int>` is a struct of C with `ptr`, `len` and `cap`: `print
*v_xs->ptr@v_xs->len` in gdb shows its items, and `parray 2 v_xs->ptr` in lldb.

Frames that are the C's own, such as `sg_main` and `sg_main_thread`, which start the program, name the C
file: `main.c`, beside the program, or in the temporary folder it was compiled in. It was written into that
folder and is gone when the build ends, so the debugger cannot show it;
`sg emit-c main.sg > main.c` writes it where the debugger finds it. A function of the language's own module
is named by the name its messages give, `<built-in>/std/path.sg`: the debugger shows its lines but not its
text, which is inside `sg`.

## An optimised build

`sg build --release -- -g` debugs a program as it runs at full speed, with what that costs in any language
compiled by an optimiser: a value may be *optimised out*, a function may be inlined into its caller, and a line
that only computes may have no place of its own, its work folded into the next or done once before a loop. A
line that calls, such as a `print`, keeps its place. When a line cannot be stopped on, the debug build is the
one to use.

## What it keeps of your machine

A program built with what a debugger reads names the full path of each of its files, `/home/me/sums/main.sg`,
so that the debugger finds them however it was started. A debug build, or a release build given `-g`, keeps
that path; a release build without `-g` names no file of SG. A program given to others is built with
`--release`.
