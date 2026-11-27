---
title: Install
description: Build the sg command from source, and set up an editor.
---

# Install

SG is built from source for now; packages come with the preview. It runs on Linux, macOS and Windows.

## What it needs

- **Rust**, stable, to build the `sg` command. `sg` itself has no dependencies.
- **A C compiler** to build programs: `sg build` compiles through C. On Linux and macOS that is `cc` (GCC or
  Clang); on Windows, LLVM's Clang (`winget install LLVM.LLVM`) with the Build Tools of Visual Studio.

Running a program in the interpreter (`sg file.sg`) needs no C compiler.

## Build it

```bash
git clone https://github.com/selcukgural/sg-lang
cd sg-lang/interpreter
cargo build --release
```

The command is `target/release/sg`; put it on your `PATH`.

## Use it

```bash
sg hello.sg                      # runs the program in the interpreter
sg build hello.sg -o hello       # compiles it through C: a debug build, which checks overflow
sg build hello.sg --release      # optimised; overflow wraps
sg test hello.sg                 # runs its tests; --compiled runs them compiled
sg emit-c hello.sg               # prints the C
sg help read                     # the page of a word, with no network
```

A folder is a program too (`sg shop`): every `.sg` file in it, with the folders beside it as its modules.

## An editor

The extension for VS Code is in the repository (`editors/vscode`): colours, and the first error of the program
as you type, with a fix applied with one click for some hints. It starts `sg lsp`, the language server built
into `sg`. It is not on the Marketplace yet; until then:

```bash
(cd editors/vscode && npm ci --ignore-scripts)
code --extensionDevelopmentPath="$PWD/editors/vscode" .
```

and set `sg.path` to the `sg` you built if it is not on the `PATH`.
