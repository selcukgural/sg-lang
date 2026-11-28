# SG for Visual Studio Code

Colours for SG (`.sg` files), with the brackets, `//` comments and indentation of the language,
and the errors of the checker as you type, from `sg lsp` (choices 1369-1376).

## Colours

The grammar is a TextMate grammar (`syntaxes/sg.tmLanguage.json`), which Sublime Text and the
JetBrains editors read too. It colours the words of the language, the built-in number types,
numbers in decimal, hex and binary, texts with their escapes, `$"..."` with the values inside
`{...}` coloured as code and their format (`{n:x8}`), raw text (`"""..."""`), characters,
operators, the names of functions where they are called or declared, and names in PascalCase as
types (types, variants and consts are written so).

## Errors as you type

The extension starts `sg lsp`, the language server built into `sg`. A file is checked as part of
the program `sg` would read it in (the nearest folder above it with a `main.sg`, or the file
alone), with what the editor holds and has not saved, 300 ms after the last change. The checker
stops at the first error, so one error is shown at a time, with its notes and its hint; a second
place the error names (where a value was moved) is a link. A hint whose code is certain comes as a
quick fix (the light bulb, or Ctrl+. / Cmd+.): a conversion `(int)x`, `.clone()` of a text that is
only borrowed, `_ = ` before a value dropped on purpose, and a missing `use` line. When the program
has no error, a `use` line that brings no name the file uses is shown dimmed.

`sg.path` in the settings names the `sg` program; without it, `sg` is looked for on the `PATH`. A
workspace that is not trusted cannot set it.

## Trying it

From the folder of the repository, once (it fetches `vscode-languageclient`, pinned in
`package-lock.json`):

    (cd editors/vscode && npm ci --ignore-scripts)
    code --extensionDevelopmentPath="$PWD/editors/vscode" interpreter/examples

opens a window of VS Code with the extension loaded; set `sg.path` there to
`interpreter/target/release/sg` if `sg` is not on the `PATH`. It is not published to the
Marketplace yet.
