# Tests and the editor

The test command and the language server. Part of [What runs today](README.md), as of 21 November 2026.

| Area | Supported |
|---|---|
| Tests | `test "name" { ... }` at the top level of a file, run by `sg test file.sg`: one line per test (`ok` or `FAIL` with the reason), exit code 1 if any failed. In a test `try` passes on an error of any type, and the test fails with it. A test that passes and leaves `shared` values alive fails with the leak report. Compiled (`sg test --compiled`), each test runs in a process of its own, the program of tests started again with `--only N`; a debug build counts the blocks of its values, and a program or a test that ends with some not given back fails with a report. A normal run leaves the tests out |
| Editor | `sg lsp`, the language server an editor starts: the first error of the program a file belongs to as it is written, with its notes and hint and a link to a second place it names, what the editor has not saved read instead of the disk; a fix applied with one click for a conversion `(int)x`, `.clone()` of a text only borrowed, `_ = ` before a value dropped on purpose, and a missing `use`. `editors/vscode`: colours (a TextMate grammar), brackets, comments and indentation for VS Code, and its client of `sg lsp` |
| Help | `sg help <name>`: the page of a word of SG in the terminal, from `sg` itself with no network (the text of its manual page, made from the same Markdown as the site); `sg help index`, every name with its line; `sg help`, the words with a page |
