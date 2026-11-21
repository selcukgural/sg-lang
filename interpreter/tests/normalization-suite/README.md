# Unicode's normalization tests

`NormalizationTest.txt` is copied unchanged from the Unicode Character Database 17.0.0
(<https://www.unicode.org/Public/17.0.0/ucd/NormalizationTest.txt>, SHA-256
`5019ffd530751a741900c849c0e010332f142a3612234639bd200b82138a87db`), under the Unicode licence kept
beside it as `LICENSE`.

`check.sg` runs it as the header of the file asks (choice 1614): each of the five texts of a line
put in each of the four forms with `text.normalize(form)` gives the text the header says, and
`text.isNormalized(form)` is true of it just when `normalize` gives it back; each code point that
no line of Part 1 names is left as it is by all four forms. `tests/cli.rs`
(`a_normalization_suite`) runs it interpreted and compiled, and `src/unicode.rs`
(`normalization_suite`) runs the same lines through the interpreter's side on its own.
