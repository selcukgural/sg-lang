---
title: JSON
description: Json, JsonValue and JsonObject, JSON read strictly into values, looked into, built, changed and written back.
sidebar_position: 13
---

# JSON

`Json.parse` reads a JSON text (RFC 8259) into a `JsonValue`, and `toJson` writes one back. It is written in
SG in the module `std`. Reading is strict: what the RFC does not allow is an error with its line and column,
and so is a name given twice in one object, which the RFC leaves to the reader. Two programs that read one
text never get two different values.

```sg nocheck
enum JsonValue {
    Null
    Bool(bool value)
    Integer(long value)    // a whole number that fits a long
    Number(double value)   // every other number
    String(string value)
    Array(List<JsonValue> items)
    Object(JsonObject members)
}
```

A whole number that fits a `long` is an `Integer`, so an id above 2^53, which a `double` cannot hold exactly,
comes through as it was written. A number written with a fraction or an exponent (`1.0`, `1e3`, `-0`) is a
`Number`, and is written back with a `.` or an exponent (`2.0`), so that reading what was written gives the
same kind.

| Signature | What it does |
|---|---|
| `Result<JsonValue, JsonError> Json.parse(read string text, JsonOptions options = JsonOptions {})` | The value the text holds, spaces allowed around it, nested to 128 levels unless `options` says more |
| `string v.toJson(JsonOptions options = JsonOptions {})` | The value on one line, with no spaces, `$"{v}"` writes the same; on lines with `JsonOptions { indent = 2 }` |
| `JsonOptions { maxDepth = 128, indent = null }` | How deep arrays and objects may be nested, in writing and in reading, and the spaces of each level (`null`: one line). A `maxDepth` below 1 or a negative `indent` is a panic |
| `read JsonValue? v.get(read string name)` | The member of an object, `null` when there is none or `v` is not an object |
| `read JsonValue? v.at(nint index)` | The item of an array, `null` when there is none or `v` is not an array |
| `read string? v.asString()` | The text of a string, `null` for anything else |
| `long? v.asLong()` | The number of an `Integer`, `null` for anything else, a `Number` too |
| `double? v.asDouble()` | The number of a `Number` or an `Integer`, `null` for anything else |
| `bool? v.asBool()` | `true` or `false`, `null` for anything else |
| `read List<JsonValue>? v.asArray()` | The items of an array |
| `read JsonObject? v.asObject()` | The members of an object |
| `bool v.isNull()` | Whether it is `null` |
| `JsonValue v.clone()` | A copy of the value and of everything in it |

Nothing that looks into a value panics: the data comes from outside, and a missing member is an ordinary
thing. `?.` goes down a path and stops at the first `null`, and `??` gives what to use instead:

```sg
void main() {
    var text = """
        {"user": {"name": "Ada", "id": 9007199254740993, "langs": ["en", "fr"]}, "score": 7.5}
        """
    match Json.parse(read text) {
        Ok(doc) => {
            print(doc.get("user")?.get("name")?.asString() ?? "anonymous")
            print(doc.get("user")?.get("id")?.asLong() ?? 0)
            print(doc.get("user")?.get("langs")?.at(1)?.asString() ?? "-")
            print(doc.get("score")?.asDouble() ?? 0.0)
            print(doc.get("user")?.get("email") is null)
            print(doc.get("score")?.asLong() is null)
            print(doc.toJson(JsonOptions { indent = 2 }))
        }
        Error(e) => print(e)
    }
}
```

It prints:

```text
Ada
9007199254740993
fr
7.5
true
true
{
  "user": {
    "name": "Ada",
    "id": 9007199254740993,
    "langs": [
      "en",
      "fr"
    ]
  },
  "score": 7.5
}
```

## Objects

A `JsonObject` keeps its members in the order they were read or added, and finds one by name: by looking at
each name while there are 16 or fewer, through a table past that.

| Signature | What it does |
|---|---|
| `JsonObject JsonObject()` | An empty object |
| `void o.add(read string name, JsonValue value)` | Adds a member at the end; a panic when there is one of that name, as `HashMap.add` |
| `void o.set(read string name, JsonValue value)` | Puts the value in place of the member of that name, where it is, or adds it at the end |
| `JsonValue? o.remove(read string name)` | Takes the member out and gives its value, `null` when there is none; the others keep their order |
| `read JsonValue? o.get(read string name)` | The value of the member, `null` when there is none |
| `bool o.contains(read string name)` | Whether there is a member of that name |
| `nint o.length()` | How many members there are |
| `read List<string> o.names()` | The names, in order |
| `read List<JsonValue> o.values()` | The values, in the order of the names: `values()[i]` is the value of `names()[i]` |

Two values are `==` when they are the same JSON: arrays item by item in order, objects member by member in
any order, since the RFC gives an object no order. `Integer(1)` and `Number(1.0)` are not the same.

## Structs of the program

Nothing turns a struct into JSON on its own: SG has no reflection and no macros. A program writes the two
functions it needs, and decides there what a missing or wrong member means:

```sg
struct Point : Copyable {
    double x
    double y
}

JsonValue toJson(Point p) {
    var o = JsonObject()
    o.add("x", JsonValue.Number(p.x))
    o.add("y", JsonValue.Number(p.y))
    return JsonValue.Object(o)
}

Point? fromJson(read JsonValue value) {
    var x = value.get("x")?.asDouble()
    var y = value.get("y")?.asDouble()
    if x is null || y is null {
        return null
    }
    return Point { x = x, y = y }
}

void main() {
    var points = List<JsonValue>()
    points.add(toJson(Point { x = 1.5, y = -2.0 }))
    points.add(toJson(Point { x = 0.0, y = 3.0 }))
    var text = JsonValue.Array(points).toJson()
    print(text)

    match Json.parse(read text) {
        Ok(doc) => {
            var items = doc.asArray()
            if items is not null {
                for item in items {
                    var p = fromJson(item)
                    if p is not null {
                        print($"({p.x}, {p.y})")
                    }
                }
            }
        }
        Error(e) => print(e)
    }
}
```

It prints:

```text
[{"x":1.5,"y":-2.0},{"x":0.0,"y":3.0}]
(1.5, -2)
(0, 3)
```

## Tokens one after another

`JsonReader` gives the tokens of JSON values written one after another, as jq and JSON Lines have them, for
a program that builds values of its own. It leaves to the program what `Json.parse` decides: a number comes
as it is written (`1.50`, `1e400`, twenty digits), and a name given twice comes twice. `next` gives a
`JsonToken`: `BeginArray`, `EndArray`, `BeginObject`, `EndObject`, `Name(text)`, `String(text)`,
`Number(text)`, `True`, `False`, `Null`, and `End` when only spaces are left. `print(token)` writes
`BeginArray`, `Name("id")`, `Number("1.50")`. `depth()` says how many arrays and objects are open,
`position()` where the next token is looked for.

The reader keeps where it is and what is open, not the text, so the text may grow between two calls. With
`more`, a token that reaches the end of the text is `UnexpectedEnd` and the reader is left as it was, to be
called again once more is read; `rebase(n)` tells it the first `n` bytes it went past were let go. What is
refused is what `Json.parse` refuses, nested to the `maxDepth` of the `JsonOptions` given to `create`.

```sg
string show(read JsonToken token) {
    match token {
        BeginArray => return "["
        EndArray => return "]"
        BeginObject => return "{"
        EndObject => return "}"
        Name(name) => return $"name {name}".toString()
        String(text) => return $"text {text}".toString()
        Number(written) => return $"number {written}".toString()
        True => return "true"
        False => return "false"
        Null => return "null"
        End => return "the end"
    }
}

void main() {
    // Two values one after another, a name given twice, and numbers no `double` holds as written.
    var text = "{\"id\": 1.50, \"id\": 2} [1e400, 12345678901234567890]"
    var reader = JsonReader()
    while true {
        match reader.next(read text) {
            Ok(token) => {
                print($"{reader.depth()}: {show(read token)}")
                if token == JsonToken.End {
                    break
                }
            }
            Error(e) => {
                print(e)
                break
            }
        }
    }

    // A text that grows: with `more`, a token cut at the end is `UnexpectedEnd`, and the reader
    // waits where it was.
    var growing = JsonReader()
    var piece = "[tr"
    for _ in 0..2 {
        match growing.next(read piece, true) {
            Ok(token) => print(show(read token))
            Error(e) => print(e)
        }
    }
    piece.write("ue]")
    for _ in 0..2 {
        match growing.next(read piece, true) {
            Ok(token) => print(show(read token))
            Error(e) => print(e)
        }
    }
}
```

It prints:

```text
1: {
1: name id
1: number 1.50
1: name id
1: number 2
0: }
1: [
1: number 1e400
1: number 12345678901234567890
0: ]
0: the end
[
line 1, column 4: the text ends before the value
true
]
```

The `jq` of the trials (`trials/jq/sg`) reads its input this way, 64 KB at a time.

## When the text is not JSON

`Json.parse` gives a `JsonError`: `e.line` and `e.column`, counted from 1, the column in characters, and
`e.kind`, a `JsonErrorKind`: `UnexpectedEnd`, `UnexpectedCharacter(found)`, `InvalidNumber`,
`NumberOutOfRange`, `InvalidEscape`, `LoneSurrogate`, `ControlCharacter`, `DuplicateKey(name)`, `TooDeep(limit)` or
`TrailingText`. `{e}` writes them on one line.

```sg
void main() {
    var texts = List<string> {
        "[1, 2,]",
        "{\"id\": 1,\n \"id\": 2}",
        "{\"note\": 'single'}",
        "[01]",
        "[1e400]",
        "[\"\\ud800\"]",
        "[1] [2]"
    }
    for text in read texts {
        match Json.parse(text) {
            Ok(value) => print(value)
            Error(e) => print(e)
        }
    }
}
```

It prints:

```text
line 1, column 7: unexpected character ']'
line 2, column 2: duplicate key "id"
line 1, column 10: unexpected character '''
line 1, column 3: invalid number
line 1, column 2: number out of range
line 1, column 3: half of a surrogate pair without the other
line 1, column 5: text after the value
```

What is refused:

- comments, a comma before `]` or `}`, single quotes, names without quotes, `NaN` and `Infinity`;
- a byte order mark before the value, and spaces other than the space, tab, `\n` and `\r`;
- a number with a leading zero (`01`), a lone `.` (`1.`, `.5`), or beyond a `double` (`1e400`); one below
  what a `double` holds (`1e-400`) is 0;
- a control character in a string that is not escaped, an escape JSON does not have, and half of a surrogate
  pair (`\ud800` alone);
- a name twice in one object;
- arrays and objects nested deeper than 128 (or the `maxDepth` of `JsonOptions`), so that a text cannot
  exhaust the stack.

Writing escapes only what JSON needs: the quote, the backslash and the control characters (`\n`, `\t`, others
as `\u001b`). Other characters stay as they are, in UTF-8. Writing a `Number` that is NaN or infinite is a
panic, since no JSON reader would take it.

The reader is tested with every parsing case of [JSONTestSuite](https://github.com/nst/JSONTestSuite). It is
about 1.15 times as slow as cJSON on a document of 34 MB. `Json.parse` reads the whole text first, and the
values take more room than the text (`JsonReader`, above, reads a stream): about 10 to 30 bytes for each byte of it (a
`JsonValue` takes 32 bytes, and each array and each object with members is a block of its own), so 10 MB of
zeros in an array take about 170 MB, as a tree of values does in Go or Rust too. A program that reads JSON from outside bounds
the length of the text before it reads it.

A key in an error is shown with the characters a reader does not see escaped (`duplicate key "a\u202eb"`).
`remove` keeps the order of the others, so it takes a time that grows with the members after the one taken
out, as `shift_remove` of Rust's `IndexMap` does.

`toJson` writes as many levels as `Json.parse` reads, 128, and a value made in the program that is nested
deeper is a panic, as NaN is: what is written is read back. `JsonOptions { maxDepth = n }` given to both
writes and reads deeper. There is no upper bound, but each level takes some of the stack, in reading and in
writing:

```sg
void main() {
    var options = JsonOptions { maxDepth = 300 }
    var text = string.withCapacity(600)
    for _ in 0..300 {
        text.write('[')
    }
    for _ in 0..300 {
        text.write(']')
    }
    match Json.parse(read text) {
        Ok(_) => print("read")
        Error(e) => print(e)
    }
    var value = Json.parse(read text, options) ?? JsonValue.Null
    print(value.toJson(options).length())
}
```

```text
line 1, column 129: nested deeper than 128
600
```
