---
name: in
kind: keyword
summary: Stands between the name and the items in a for.
see: for, read, ref
learn: ../docs/learn/book/08-collections.md#for-over-a-collection
---

## Forms

| Form | What it is |
|---|---|
| `for i in a..b` | the name `i`, then the numbers it takes |
| `for x in xs` | the name `x`, then the collection it goes over |
| `for x in read xs`, `for x in ref xs` | the same, with the items lent |

## What it means

`in` is written only in the head of a `for`: the name of each round comes before it, and what the loop goes
over comes after it. Everything about the loop is on the page of `for`.

`in` is not an operator. Whether a list, a map, a set or a text holds something is asked with `contains`. And
since `in` is a word of the language, it cannot be the name of a variable.

## Examples

### A range and a list

```sg
void main() {
    var words = List<string> { "red", "green", "blue" }
    for i in 0..words.length() {
        print($"{i} {words[i]}")
    }
    for word in read words {
        print(word.length())
    }
}
```

```text
0 red
1 green
2 blue
3
5
4
```

After `in` comes a range, `0..words.length()`, or a collection, here lent with `read`.

### Whether something is there

```sg
void main() {
    var words = List<string> { "red", "green", "blue" }
    print(words.contains("green"))
    var ages = HashMap<string, int> { {"ada", 36} }
    print(ages.contains("grace"))
    print("green tea".contains("tea"))
    var n = 7
    print(n >= 1 && n <= 9)
}
```

```text
true
false
true
true
```

`contains` asks a list, a map (by its keys) or a text. A number is held between two ends with `>=` and `<=`.

## What the compiler refuses

### `in` as a test

```sg error
void main() {
    var xs = List<int> { 1, 2 }
    if 2 in xs {
        print("yes")
    }
}
```

```text
error: expected `{`, found `in`
  --> example.sg:3:10
 3 |     if 2 in xs {
   |          ^^
```

The test is written `if xs.contains(2)`.

### A `for` without `in`

```sg error
void main() {
    var xs = List<int> { 1, 2 }
    for x of xs {
        print(x)
    }
}
```

```text
error: expected `in`, found `of`
  --> example.sg:3:11
 3 |     for x of xs {
   |           ^^
```

### Two names before `in`

```sg error
void main() {
    var xs = List<int> { 1, 2 }
    for i, x in read xs {
        print(x)
    }
}
```

```text
error: expected `in`, found `,`
  --> example.sg:3:10
 3 |     for i, x in read xs {
   |          ^
```

A `for` has one name. For the positions too, go over `0..xs.length()` and read `xs[i]`.

### `in` as a name

```sg error
void main() {
    var in = 3
    print(in)
}
```

```text
error: expected a variable name, found `in`
  `in` is a word of the language, not a name
  --> example.sg:2:9
 2 |     var in = 3
   |         ^^
```
