# JSONTestSuite: the parsing cases

`test_parsing/` and `LICENSE` are copied unchanged from Nicolas Seriot's
[JSONTestSuite](https://github.com/nst/JSONTestSuite), commit
`1ef36fa01286573e846ac449e8683f8833c5b26a` (MIT licence, kept beside them).

`check.sg` reads every file with `Json.parse` (choice 1442): each `y_` file must be read, each `n_`
file refused, and each `i_` file, which the RFC leaves to the reader, as SG's rules decide. A file
that is not UTF-8 is refused before `Json.parse`, when it is made a `string`. Two `y_` files,
`y_object_duplicated_key*`, are refused on purpose: a name twice in one object is an error in SG
(choice 1411), which the RFC allows, since it only says names SHOULD be unique. `tests/cli.rs`
(`a_json_suite`) runs it interpreted and compiled.
