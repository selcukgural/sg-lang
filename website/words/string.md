---
name: string
kind: type
summary: Text in UTF-8 that owns its memory; read string is a borrow of it.
see: read, char, byte, List
learn: ../docs/learn/book/03-text.md
library: ../docs/library/string.md
---

## Forms

| Form | What it is |
|---|---|
| `"text"`, `"""raw"""` | a text written in the program |
| `$"{x} items"` | values put into a text |
| `string`, `read string` | a text that owns its bytes, a look at one |
| `text.length()` | the number of bytes |
| `text[i]`, `text[a..b]` | one byte, a part as a `read string` |
| `text.write(x)` | adds `x` at the end |
| `.clone()`, `.toString()` | a text of its own, from a look or from `$"..."` |
| `string.fromUtf8(bytes)` | bytes as text, once checked |

## What it means

A `string` is always valid UTF-8. It owns its bytes: it grows, and lets them go when it ends. A `read string`
looks at text something else owns, as a pointer and a length, and copies nothing. Trimming, splitting and a
range `text[a..b]` give a `read string`, so they cost no allocation; the compiler checks that the look never
outlives what it looks at.

A text written in the program is a `string` that takes no memory until it is changed. `$"..."` writes values
into a text without making one; `.toString()` makes it, and that is where the allocation shows. A `string`
grows with `write`: `+` and `+=` do not join texts.

`length()` counts bytes, and `text[i]` reads a byte, so a place is found in constant time. `chars()` goes by
character. A range that would cut a character in two stops the program, and bytes become text only through
`string.fromUtf8`, which checks them: no invalid text ever exists.

The methods are in [Text](../docs/library/string.md).

## Examples

### Bytes and characters

```sg
void main() {
    var word = "naïve"
    print($"{word} has {word.length()} bytes")
    nint letters = 0
    for _ in word.chars() {
        letters += 1
    }
    print($"and {letters} characters")
    print(word[0] == 'n')
}
```

```text
naïve has 6 bytes
and 5 characters
true
```

`ï` takes two bytes in UTF-8.

### Build a text

```sg
void main() {
    var line = string.withCapacity(64)
    for i in 1..4 {
        line.write(i)
        line.write(',')
    }
    line.write($" and {2.5:F1}")
    print(line)
    string label = $"{line.length()} bytes".toString()
    print(label)
}
```

```text
1,2,3, and 2.5
14 bytes
```

`withCapacity` takes room for 64 bytes at the start, so the loop allocates once.

### Lend it, or a part of it

```sg
nint vowels(read string text) {
    nint count = 0
    for c in text.chars() {
        if "aeiou".contains(c) {
            count += 1
        }
    }
    return count
}

void main() {
    var name = "Ada Lovelace".clone()
    print(vowels(read name))
    print(vowels("Grace"))
    print(vowels(name[4..]))
    string first = name[0..3].clone()
    name.clear()
    print($"[{first}] [{name}]")
}
```

```text
5
2
4
[Ada] []
```

`name[4..]` lends a part with no copy. `first` is a text of its own, so it lives on after `name` is cleared.

### Split, with nothing copied

```sg
void main() {
    var line = "  name = Pasha ; age = 7  "
    for field in line.split(';') {
        var parts = field.split('=')
        var key = (parts.next() ?? "").trim()
        var value = (parts.next() ?? "").trim()
        print($"{key} -> {value}")
    }
}
```

```text
name -> Pasha
age -> 7
```

Every part, trimmed or not, looks at `line`.

### Bytes as text

```sg
void main() {
    var good = List<byte> { 0x53, 0x47, 0x21 }
    var bad = List<byte> { 0x53, 0xff, 0x21 }
    match string.fromUtf8(read good) {
        Ok(text) => print(text)
        Error(e) => print($"not text from byte {e.position}")
    }
    match string.fromUtf8(read bad) {
        Ok(text) => print(text)
        Error(e) => print($"not text from byte {e.position}")
    }
    print("SG".asBytes()[1])
}
```

```text
SG!
not text from byte 1
71
```

`fromUtf8` checks the bytes and gives a `read string` that looks at them, with no copy. `asBytes()` goes the
other way, for free.

## What stops the program

### A range through a character

```sg panic
void main() {
    var word = "naïve"
    print(word[0..3])
}
```

```text
panic: range 0..3 cuts a character in half
  `ï` takes bytes 2..4; a `string` is always valid UTF-8
  --> example.sg:3:11
 3 |     print(word[0..3])
   |           ^^^^^^^^^^
```

## What the compiler refuses

### Joining with `+=`

```sg error
void main() {
    var text = "one".clone()
    text += " two"
    print(text)
}
```

```text
error: `+=` needs a number on the left, found `string`
  --> example.sg:3:5
 3 |     text += " two"
   |     ^^^^^^^^^^^^^^
  hint: a text grows with `write`: `text.write(...)`
```

### `$"..."` where a `string` goes

```sg error
void main() {
    int count = 3
    string label = $"{count} items"
    print(label)
}
```

```text
error: expected `string`; `$"..."` does not become a `string` on its own
  --> example.sg:3:20
 3 |     string label = $"{count} items"
   |                    ^^^^^^^^^^^^^^^^
  hint: add `.toString()` at the end; that is where the allocation shows
```

### Changing a text a part of it still looks at

```sg error
void main() {
    var name = "Ada Lovelace".clone()
    var first = name[0..3]
    name.clear()
    print(first)
}
```

```text
error: `name` cannot be changed here: `first` borrows from it and is used again later
  --> example.sg:4:5
 4 |     name.clear()
   |     ^^^^
  `first` borrowed it here:
  --> example.sg:3:17
 3 |     var first = name[0..3]
   |                 ^^^^^^^^^^
  hint: finish with `first` before this line: a borrow ends at its last use
```

### Writing a byte

```sg error
void main() {
    var name = "pasha".clone()
    name[0] = 'P'
    print(name)
}
```

```text
error: a byte cannot be written into a `string`
  a `string` is always valid UTF-8; `text[i]` only reads
  --> example.sg:3:5
 3 |     name[0] = 'P'
   |     ^^^^^^^
```

One byte written alone could leave half a character behind.
