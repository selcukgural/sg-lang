# jq

A `jq` of JSON, written in SG, in Rust and in Zig, the same algorithm each time (choices 1804-1812).
Every case holds the three to **jq 1.8.2 itself**: the same bytes on the standard output and the same exit
code. The standard error is not compared; each words its errors its own way, as jq's own are worded
`jq: error (at <file>:N): ...`. Where this page does not say what happens, jq 1.8.2 says it: run it.

```
jq [-c] [-r] [-n] [-s] [-e] [-S] [--tab] [--indent n] [--arg name text] [--argjson name json] FILTER [FILE...]
```

Short flags may be joined (`-cr`). The first word that is not a flag is the filter, the rest are files,
read one after another as one stream; with no file, the standard input. A missing file is told and the
others are read; the exit code is then 2.

## The language

- **Paths**: `.`, `..`, `.name`, `."name"`, `.["name"]`, `.[n]` (from the end when negative, `null` past
  either end), `.[f]`, `.[a:b]` on arrays and on texts (counted in characters; either end may be left out),
  `.[]`, and `?` after any of them or any term (`try` without `catch`). `.a.b[0]` chains.
- **Building**: `[f]`, `{name: f, $x, "text": f, "\(f)": g, (f): g, name}`; a member whose value has several
  outputs gives as many objects, as jq does. Literals: numbers, texts with `\(...)` inside, `true`, `false`,
  `null`.
- **Operators**, loosest first: `|`, `,`, `//` (right), `=` `|=` `+=` `-=` `*=` `/=` `%=` `//=`, `or`,
  `and`, `== != < <= > >=`, `+ -`, `* / %`, unary `-`, postfix. `e as $x | g`, as jq 1.8.2's grammar has
  it: `e` is the whole expression on its left (`1 + .a as $x | $x` binds `1 + .a`), `g` the rest of the
  pipe, and a binding, or a `def`, may stand after `,` (`1, .a as $x | $x, 2`). `-` before a term makes a
  term (`-1 as $x | $x, 2` binds -1; `- 1 + 2` is 1); `reduce` and `foreach` read from an expression,
  `try` takes a term.
  Arithmetic is jq's whole: numbers; `+` of texts, arrays and objects (the right one's members win) and
  `null`; `-` of arrays; `*` of objects (deep), of a text by a number; `/` of texts (split); `%`.
  Comparisons and `sort` use jq's order: `null < false < true < numbers < texts < arrays < objects`,
  texts by their UTF-8 bytes, arrays item by item, objects by their sorted names and then the values.
- **Forms**: `if c then a elif c2 then b else d end` (`else` may be left out), `reduce f as $x (init; update)`,
  `foreach f as $x (init; update)` and `foreach f as $x (init; update; extract)`,
  `def name: body;` and `def name(f; $x): body;`, recursive, scoped as jq scopes them (a filter argument
  is a closure over where it was written). `# comments` to the end of the line.
- **Built in**: `empty`, `error`, `error(m)`, `not`, `isempty(g)`, `infinite`, `nan`, `length`, `utf8bytelength`, `keys`, `keys_unsorted`,
  `values`, `has(k)`, `in(o)`, `map(f)`, `map_values(f)`, `select(f)`, `recurse`, `to_entries`,
  `from_entries`, `with_entries(f)`, `add`, `add(f)`, `any`, `all`, `any(f)`, `all(f)`, `any(g; c)`,
  `all(g; c)`, `range(n)`, `range(a; b)`, `range(a; b; step)`, `limit(n; f)`, `first`, `last`, `first(f)`,
  `last(f)`, `sort`, `sort_by(f)`, `group_by(f)`, `unique`, `unique_by(f)`, `min`, `max`, `min_by(f)`,
  `max_by(f)`, `reverse`, `tostring`, `tonumber`, `tojson`, `fromjson`, `type`, `nulls`, `booleans`,
  `numbers`, `strings`, `arrays`, `objects`, `iterables`, `scalars`, `split(s)`, `join(s)`, `test(re)`,
  `test(re; flags)`, `ascii_downcase`, `ascii_upcase`, `startswith(s)`, `endswith(s)`, `ltrimstr(s)`,
  `rtrimstr(s)`, `floor`, `sqrt`, `path(f)`, `paths`, `getpath(p)`, `setpath(p; v)`, `del(f)`, `@text`,
  `@json`, `@csv`, `@tsv`, `@base64`, `@base64d`, `@uri`, `input`, `inputs`.
- `-n` gives `null` as the one input (`input`/`inputs` then read the stream); `-s` gives one array of every
  value read; `--arg name text` binds `$name` to a text, `--argjson name json` to a value; `-e` sets the
  exit code by the last output (1 when it is `false` or `null`, 4 when there was none), as jq does.

What is not here: `label`/`break`, `try ... catch`, `?//`, destructuring (`. as [$a, $b]`), `limit`'s
siblings `until`/`while`/`repeat`, `getpath` on streams, `@sh`/`@html`, SQL-like and date builtins,
`$ENV`, `env`, `$__loc__`, `input_filename`, modules, `--stream`, `--seq`, `-a`, `-j`, `-C`. `test` reads the pattern
with each language's regular expressions (SG's `Regex`, Rust's `regex`, PCRE2 in Zig); jq's are
Oniguruma's. The cases use patterns the four read alike.

## Output, byte for byte as jq 1.8.2

- Pretty by default, two spaces a level, `"name": value`, `[]` and `{}` when empty; `--indent n` (0 to 7;
  0 still breaks lines but indents nothing), `--tab`, `-c` one line with no spaces. `-S` sorts the names of
  every object written, by their bytes. `-r` writes a text without quotes or escapes. One value a line.
- A text escapes `"`, `\`, `\n`, `\t`, `\r`, `\b`, `\f`, every other byte below 0x20 and 0x7F as `\u00XX`
  (lower-case hex), and nothing else: `/` and letters past ASCII go out as they are.
- **Numbers.** A number read and not changed keeps its literal, written as decNumber writes it
  (to-scientific-string): the digits of its coefficient and an exponent; plain when the exponent is 0 or
  below and the adjusted exponent (exponent + digits − 1) is −6 or above, else `d.dddE+n` / `E-n`.
  `1.5e3` → `1.5E+3`, `1.0e2` → `1.0E+2`, `1e-7` → `1E-7`, `0.00001` stays, `2.0` stays, `1.10` stays,
  `-0` stays, `123456789012345678901234567890` stays. A literal of the filter keeps its text the same way.
  Comparing two literals compares them exactly (`100000000000000000001 > 100000000000000000000`).
  A number computed is a `double`, written as jq writes one: the fewest digits that read back the same (17
  at most); with an exponent when the decimal point would be 4 or more places left of the first digit, or
  more than 15 places right of the last (`1e-05`, `0.0001`, `1e+16`, `1000000000000000`,
  `12345678901234568`, `1e+21`, `1.5e+300`, `5e-324`, `2e-310`, the exponent of two digits at least, with
  its sign); a `double` exactly halfway between the two shortest that read back as it takes the even one, as
  jq's `dtoa` does (`1e15 + 0.3`, which is `...000.25`, is `1000000000000000.2`); whole values without a
  point (`3`, `-0`); NaN as `null`; infinity as
  `1.7976931348623157e+308` and its negative. A literal too large for decNumber (`1e999999999999`) is read
  as a `double` and is infinite.

## Input

- Values one after another, with or without spaces between them (`{}{}[]1 2"a""b"` is six; `12` is one).
  At the top, a number or a word is followed by a space, the end or the start of another value: `1,2`,
  `1]` and `truefalse` are errors before anything is written (`1 ,2` writes `1` first). A value that does
  not read is told with where it is, the values before it have been worked, and the exit code is 5, as jq
  1.8.2 gives (not 2, as older jq did).
- The files named are **one stream of bytes**: `1 2 3` with no new line at its end, then a file `1`, reads
  as `1 2 31`. `-` is the standard input. A file that cannot be opened is told; jq's main loop then stops
  after the value it is reading (`jq . missing three.json` writes only the first value of `three.json`),
  while `-n` with `inputs` and `-s` read every file; the exit code is 2 either way.
- **Read as RFC 8259**, with jq's departures followed where they are not a quirk: a name twice in an object
  keeps the last value at the first one's place (`{"a":1,"a":2,"b":3,"a":4}` → `{"a":4,"b":3}`); a byte
  order mark at the very start of the stream is skipped, and the start of one followed by the end is read as
  nothing, with no error (after a space it is a bad byte, exit 5).
- **Depth**, as jq counts it: an array or an object is a level, and the name of a member one more while its
  value is read; the depth is looked at only when an array or an object opens, and what is open then must
  be below 10,000. So `[` 10,000 deep reads, 10,001 does not, `{"a":` 5,000 deep reads and 5,001 does not.
- **Numbers**: the literal is kept as written above. Its `double` is found as jq finds it, from the literal
  rounded to 17 significant digits, half to even: `128218505994187403 + 0` is `128218505994187400`. A
  literal beyond a `double` (`1e1000`, `1e-400`) keeps its literal and is infinite or zero as a `double`.
- **Bad UTF-8** inside a text becomes U+FFFD, by jq's decoder (`jvp_utf8_next`): a lead byte gives the
  length (0x00-0x7F 1, 0xC2-0xDF 2, 0xE0-0xEF 3, 0xF0-0xF4 4; 0x80-0xBF, 0xC0, 0xC1, 0xF5-0xFF are bad
  alone); a sequence that the text ends inside is one U+FFFD; a byte in it that is not 0x80-0xBF ends it
  there, one U+FFFD for the bytes before, and is read again as a lead; an overlong form, a surrogate
  (0xD800-0xDFFF) or a value past 0x10FFFF is one U+FFFD for the whole sequence. Every program checks the
  bytes first and replaces only where it finds a bad one, so the valid input pays one check.
- `\uD800` (a high surrogate with no low after it) is an error, as in jq.
- **jq's quirks, not followed** (the three refuse these with exit code 5; jq takes them): `01`, `00`,
  `-01`, `+1`, `.5`, `1.`, `nan`, `NaN`, `-nan`, `infinity`, a lone low surrogate `"\udc00"` (jq gives
  U+FFFD), a NUL byte right after a number (jq reads the number as a C string).
- Read in pieces of 64 KB, not all at once: `tail -f log | jq -c .` writes each value when it has come. A
  token longer than what has been read waits for more; what is kept grows by doubling, so a text of 100 MB
  is read in linear time.

## jq 1.8.2's own ways, followed

Found by running jq while the three were written; each is in the tests.

- `?` after an index (`.a?`, `.[e]?`, `.[]?`, a slice) makes only that index optional: what it indexes may
  still fail. `(f)?`, or `?` after anything else, is `try`. `try` does not catch an error raised by what
  runs on its outputs, and **`//` catches no error at all**: `error("x") // 1` is an error.
- Functions jq writes in C (`has`, `setpath`, `test(re; flags)`, `split`, ...) take their **last** argument
  as the outer loop; `range` and `limit`, written in jq, take the first.
- `reverse` is `[.[length - 1 - range(0; length)]]`: a text is an error, `null` and `{}` give `[]`.
  `from_entries` takes the key from `key`, `Key`, `name` or `Name` (with `//`) and the value from `value`,
  else `Value`; a key that is not a text is an error. `ltrimstr` and `rtrimstr` of what is not a text are
  errors. `"ab" * 0.5` is `""`, `"ab" * -1` is `null`. `limit` of a negative count is an error, of a count
  that is not a number too; `range(a; b; 0)` is empty. `.[] |= empty` takes away every path at the end
  (`[1,2,3]` → `[]`).
- A member's value in `{...}` is any expression but `,`, `as` and `def` outside brackets: `{a: 1 + 2}`.
  A key in parentheses that is a constant, as jq folds constants when it reads the filter, and not a text
  is refused then, with exit code 3: a literal, `[c, ...]` (only the chain `,` makes on its own, which leans
  left: `[1, (2, 3)]` is not folded), `{"name": c, ...}`, and an operator on two constants that gives no
  error. `{(1 + 1): 2}` and `{([1]): 3}` are exit 3; `{(-1): 2}`, `{(1 / 0): 2}` and `{(1, "a"): 2}` fail
  when run, exit 5.
- **Paths through what is not a path** (`path`, `paths`, `del`, the left of `|=`): a value made by `reduce`,
  `last(f)`, a variable or a constant stands where its input stood when it is that input, the same value
  as jq's `jv_identical` tells (the same `null`, `false`, `true` or computed number, the same literal,
  text, array or object in memory): `path(reduce empty as $x (.; .))` is `[]`, `null | path(last(.a, .b))`
  is `[]`, `{"a":1} | path(.a | 1)` is an error. A whole number of at most 15 digits read is held as its
  value (choice 1837), never as the same value as another: `1 | path(. as $v | $v)` is an error here where
  jq gives `[[]]`.
- `last(f)` of no output gives none, as jq 1.8.2's own `last` does; `..` calls the program's own `def
  recurse` when it has one.
- Values nested deeper than 10,000 are written `<skipped: too deep>`, by `tojson` too. Comparing past
  10,000 levels of arrays and objects is an error: `Equality check too deep` for `==` and `!=`, where a value
  held twice is equal to itself at once at any depth, and `Comparison too deep` for `<`, `sort`, `group_by`
  and their kin, where the sorted names of an object are one level more. `unique` sorts the items themselves.
- `test` reads its pattern as Oniguruma's Perl syntax: `^` is the start of the text, `$` its end or the place
  before a last new line; `i` folds case, `x` passes over spaces and comments, `p` lets `.` match a new line,
  `n` counts no empty match, `g`, `l` and `s` change nothing a test sees; `test([re, flags])` is
  `test(re; flags)`. The three read the pattern with their own engines and give these meanings (SG's
  `Regex` with `(?x)`, choice 1822).
- `-e`: the exit code follows the last input that gave an output (1 for `false` or `null`), 4 when none did,
  5 when the last input ended in an error; a file that could not be opened makes it 2 over all of these.
- The command line: options come before and after the filter and the files; only `--word` and `-` with
  letters are options, so `-1`, `-.a`, `-(1,2)` and `- - 1` are filters; of `-c`, `--tab` and `--indent n`
  the last holds; `--indent -1` is a tab and above 7 is exit 2; with no filter the filter is `.`, and a
  filter that is empty or only definitions is exit 3. `-n` with `-s` makes `input` give every value as one
  array.
- Numbers: a literal keeps its text up to an adjusted exponent of 999,999,999 (past it, an infinity that
  compares above every literal); below -999,999,999 it keeps fewer digits down to the exponent
  -1,147,483,646 and is rounded there, half up (`15e-1147483647` is `2E-1147483646`). Negating a literal zero
  gives the literal without its sign; a literal read is compared exactly with another read (`2` read from
  the input is above `1.9999999999999999999`).

## How it is written: each language its own way, the algorithm the same (choices 1807, 1819)

Each version uses what its language has and its developers use: `Rc::make_mut` and serde in Rust, counts
kept by hand and `std.json.Scanner` in Zig, `shared` and `JsonReader` in SG; that difference is what is
measured. What is a way of working rather than a feature of a language is the same in the three, so that a
difference is never one of algorithm (1819):

1. **Reading**: the bytes in pieces of 64 KB; UTF-8 checked, replaced as above where bad; the tokens of
   each value read by the language's JSON library and the program's own value **built straight from
   them**, with no tree of the library's in between: SG `JsonReader` (choice 1814), Rust `serde_json`'s
   `StreamDeserializer` with a `DeserializeSeed` of the program's value (`arbitrary_precision` for the
   literal of each number), Zig `std.json.Scanner`.
2. **Values**: null, false, true, a number (a `double` and, when it was read and not changed, its literal),
   a text, an array, an object keeping the order its names came in. A value is 16 bytes. **A number read**
   (choices 1824, 1837): a whole number of at most 15 digits is its `double`, found from its digits and
   held in the value; any other keeps its literal apart, with the texts, arrays and objects, as it was
   written when that is decNumber's form already, and its `double` is worked out the first time it is
   wanted and kept. **Two numbers read** are compared by their `double`s first, which a literal's `double`
   orders as the literals are ordered; only when the `double`s are the same and one of the two has more
   than 15 digits (or lies beyond 1E±307) are the literals compared exactly. A number computed is a
   `double`, and a whole one below 10^16 is written as an integer (1825). Texts, arrays and objects are shared by
   a count of their holders and changed in place when there is only one (jq's copy on write), so
   `reduce range(0; n) as $i ([]; . + [$i])` takes linear time. Rust: `Rc` and `Rc::make_mut`; Zig: a count
   kept by hand; SG: `shared` and `makeUnique` (choice 1813).
3. **The filter** is read into a tree, then evaluated by walking it **with a stack of its own on the heap**,
   not the machine's: a call of a `def` nested a million deep runs (jq: 0.25 s, 150 MB), and a call in tail
   position takes no room, so `def f: if . < 10000000 then .+1|f else . end; 0|f` runs in constant memory
   (jq: 1.7 s, 3 MB). Several outputs are made one at a time by going back to the last point that has more
   (backtracking), never gathered in a list first, so `limit(3; range(0; infinite))` and `first(inputs)`
   end at once and `| head -1` reads no more than it needs.
4. **Single outputs at once**: a node that gives exactly one output and calls no function of the filter is
   worked out directly, by a function that follows the tree of the filter (as deep as the filter is
   written, never as deep as the data), with no frame: `.`, a literal, a variable, `.a`, `.[e]` and
   `.[a:b]` of such (not with `?`), negation, arithmetic, comparison, `and`, `or` and `//` of such, `a | b`
   and `a as $x | b` of such, `{...}` and `"\(...)"` of such, a format, a native function of such
   arguments, and `if` whose parts are such. `[f]` of such an `f`, and `[t[] | f]` with such a `t` and `f`
   (`map`, the keys of `sort_by` and its kin), go over the items in one loop; the update of `reduce`, when
   it is such, is worked out at once and handed the state, so that it stays the one holder. `=` and `|=`
   set each path on the one holder of the input. An array or an object made whole gives back the room
   it grew by, up to 4,096 members, and `[f]` of one output is a list of one (choice 1819: what one of the
   three did, the others do). `..` is worked out by the machine with a stack of its own, depth first,
   not as jq's `def recurse: recurse(.[]?)`; `sort` of an array held once sorts it where it lies.
5. **Writing** into a block of 64 KB of the program's own, given to the system when full and at the end; a
   closed pipe ends the program at once by SIGPIPE, as jq ends (Rust's runtime ignores the signal, so the
   Rust version puts it back and raises it).

## Cases (choice 1808)

Inputs from [`inputs/json.sg`](../inputs/json.sg), seed 1, and the hostile ones made by `measure.py`.

- **Speed**: `.` on 100 MB of records and on 1 GB; `-c '.[] | select(.active and .score > 5000) | {id, name,
  city: .address.city}'`; `group_by(.address.city) | map({city: .[0].address.city, n: length, avg: (map(.score)
  | add / length)})`; `sort_by(.balance) | .[0], .[-1]`; `-r '.[] | [.id, .name, .score] | @csv'`;
  `reduce .[] as $r ({}; .[$r.address.city] += 1)`; `[.. | numbers] | add` on the polygons;
  `-c 'select((.tags | length) > 2) | .tags'` on a record a line; ten million short lines out.
- **Limits**: one text of 100 MB; an object of ten million members; 10,000 levels of nesting (and 10,001);
  `[range(0; 10000000)] | sort`; a `def` a million deep, not in tail position, and ten million in tail
  position; `test("(a+)+$")` on a long run of `a` and a `b`; `| head -1` on 1 GB.
- **What is broken**: bad UTF-8 of each kind above; `\uD800`; `\u0000`; a byte order mark; lines ended
  with `\r\n`; 1 GB cut off inside its last value; an error in the middle of the stream (what came before
  printed, exit 5); `1e1000`, `-0`, `1e-400`, `18446744073709551617`, `0.1 + 0.2`, a literal of a hundred
  digits; jq's quirks above.

## Tests against jq 1.8.2

Two suites compare a version with jq 1.8.2 case by case, the bytes of the standard output and the exit
code; each version passes both (choice 1839). They are run by hand, with jq 1.8.2 on the `PATH` (or named by
`JQ=`), not by `cargo test`:

```
python3 -I trials/jq/tests/filters.py <jq>    # 3,687 cases, written with the Rust version
python3 -I trials/jq/tests/streams.py <jq>    # 6,389 cases, written with the Zig version
```

[`filters.py`](tests/filters.py) runs filters on several inputs, numbers, output styles, input and its
errors, bad UTF-8, the language, deep nesting, flags and files. [`streams.py`](tests/streams.py) runs
documents under many filters, numbers and doubles written as jq writes them, input with marks and errors,
nesting, recursion, paths, regular expressions, arguments, jq's grammar, constant keys, ending early,
files, and inputs of 2 MB made by `json-input` (built with `interpreter/target/release/sg`). What they write
goes to `trials/target/jq-tests`. The differences they count as meant are jq's quirks of input listed
above, and `test("(a+)+$")` on a long run of `a`, which jq gives up on (Oniguruma's limit of retries)
where SG's and Rust's expressions answer `false`. `cargo test the_trial_of_jq` builds the SG version and
runs a few cases of its own without jq.
