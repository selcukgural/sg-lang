# RE2's search tests

`re2-search.txt` is copied unchanged from Go 1.27.4 (`src/regexp/testdata/re2-search.txt`), where it
was built by running `make log` in the [RE2](https://github.com/google/re2/) distribution; it is
under the BSD licence of the Go authors, kept beside it as `LICENSE`.

`check.sg` reads it as Go's `TestRE2Search` does (choice 1468): each pattern is tried on each text
of its block as it is (the leftmost match RE2 prefers) and held to the whole text
(`\A(?:pattern)\z`), the groups compared too and `isMatch` against them. The two results of the
leftmost-longest match of POSIX are not tried: `Regex` has no such match (choice 1473). Left out:
`\C`, one byte of a character, which neither Go nor SG has; `\B` in a text that is not all ASCII,
where RE2 looks between the bytes of a character and Go and SG do not; a text that is not UTF-8,
which is no `string`; and `\p{Greek}`, a script of Unicode, which `\p` does not name yet.
`tests/cli.rs` (`a_regex_suite`) runs it interpreted and compiled.
