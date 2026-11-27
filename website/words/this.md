---
name: this
kind: value
summary: The value a method was called on; read this looks, ref this changes, this alone takes it.
see: read, ref, struct, enum, with, share, T*
learn: ../docs/learn/book/05-structs-and-enums.md#methods
---

## Forms

| Form | What it is |
|---|---|
| `T m(read this)` | a method that only looks at the value |
| `void m(ref this)` | a method that may change the value, or give it a new one with `this = ...` |
| `T m(this)` | a method that takes the value: the caller's variable is used up, a `Copyable` one copied |
| `this.field`, `this.m()` | a field of the value, and another method called on it |
| `match this`, `this is Red` | in a method of an enum, the variant it was called on |
| `return this`, `this with { ... }` | in a method that takes `this`, the value given back, as it is or changed |
| `void format(read this, ref Formatter f)`, `void dispose(ref this)` | the methods the built-in interfaces ask for |
| `read shared T this` | a method that borrows the owner, so that it can write `share this`: see `share` |

## What it means

`this` is the value a method was called on. It is the method's first parameter, and the way it is written says
what the method does with the value, as for any other parameter: `read this` borrows it to look, `ref this`
borrows it to change, and `this` alone takes it. The call writes nothing for it: `cart.add("tea")` is the same
whether `add` takes `ref this` or `read this`.

A field is always reached through `this.`, and so is another method on the same value: `this.count`,
`this.log()`. A bare `count` is a local variable, so a field and a variable never look alike. `this` is not a
name a program can give: a function without a `this` parameter has none, belongs to the type and is called on
it, `Counter.create()`.

A method that takes `this` owns the value: it may change it, move its fields out, and give it back with
`return this`. The caller's variable is used up, unless the type is `Copyable`; then the method gets a copy.
A `read this` or `ref this` method only borrows the value, so it moves no field out of it: the caller still
owns it whole. `take()` and `replace` give a field's value and leave another in its place.

A method is called through a `T*` as on the value itself; one that takes `this` uses the pointer up. Methods of
an enum take `this` the same three ways, and `match this` or `this is Red` says which variant the method was
called on.

## Examples

### The three ways

```sg
struct Counter {
    string name
    int count

    int value(read this) {
        return this.count
    }

    void add(ref this, int by) {
        this.count += by
        this.log()
    }

    void log(read this) {
        print($"{this.name} = {this.value()}")
    }

    string finish(this) {
        return $"{this.name}: {this.count}".toString()
    }
}

void main() {
    var hits = Counter { name = "hits", count = 0 }
    hits.add(2)
    hits.add(3)
    var report = hits.finish()
    print(report)
}
```

```text
hits = 2
hits = 5
hits: 5
```

`add` changes the counter and calls `log` on it; a `ref this` lends itself where a `read this` is asked for.
`finish` takes the counter, so `hits` cannot be used after that line.

### A `Copyable` value, and a method of an interface

```sg
struct Money : Copyable, Formattable {
    long cents

    Money plus(this, Money other) {
        return Money { cents = this.cents + other.cents }
    }

    void format(read this, ref Formatter f) {
        f.write($"{this.cents / 100}.{this.cents % 100:d2} EUR")
    }
}

void main() {
    var price = Money { cents = 1250 }
    var total = price.plus(Money { cents = 199 })
    print($"{price} + 1.99 = {total}")
}
```

```text
12.50 EUR + 1.99 = 14.49 EUR
```

`plus` takes `this`, but `Money` is `Copyable`, so it gets a copy and `price` is still there. `format` is the
method `Formattable` asks for; `print` and `$"{price}"` call it.

### An enum

```sg
enum Light : Formattable {
    Red
    Yellow
    Green

    void advance(ref this) {
        this = match this {
            Red => Light.Green
            Yellow => Light.Red
            Green => Light.Yellow
        }
    }

    bool stops(read this) {
        return this is Red
    }
}

void main() {
    var light = Light.Red
    for _ in 0..4 {
        print($"{light} {light.stops()}")
        light.advance()
    }
}
```

```text
Red true
Green false
Yellow false
Red true
```

`match this` looks at the variant the method was called on. `this = ...` in a `ref this` method gives the
caller's variable a new value.

### Giving `this` back

```sg
struct Request : Formattable {
    string url
    List<string> headers
    int timeout = 30

    Request create(string url) {
        return Request { url = url, headers = List<string>() }
    }

    Request header(this, string line) {
        this.headers.add(line)
        return this
    }

    Request waitFor(this, int seconds) {
        return this with { timeout = seconds }
    }
}

void main() {
    var request = Request("https://sg-lang.org")
        .header("Accept: text/html")
        .header("User-Agent: sg")
        .waitFor(5)
    print(request)
}
```

```text
Request { url = "https://sg-lang.org", headers = ["Accept: text/html", "User-Agent: sg"], timeout = 5 }
```

Each method takes the request, changes it and gives it back, so the calls are chained with nothing copied: the
same value goes from one to the next.

### A borrow of a field

```sg
struct Contact {
    string name
    List<string> phones

    read string bestPhone(read this) {
        if this.phones.length() == 0 {
            return read this.name
        }
        return this.phones[0]
    }
}

void main() {
    var ada = Contact { name = "Ada", phones = List<string> { "555-0101", "555-0199" } }
    var bob = Contact { name = "Bob", phones = List<string>() }
    var phone = ada.bestPhone()
    print(phone)
    print(bob.bestPhone())
    ada.phones.clear()
}
```

```text
555-0101
Bob
```

`bestPhone` lends a text that lives in the contact, with no copy. While `phone` is used, `ada` is lent: the
`clear()` is allowed only after the last use of `phone`.

### Taking a field through `ref this`

```sg
struct Outbox {
    string? draft
    List<string> sent

    void send(ref this) {
        var text = this.draft.take() ?? return
        this.sent.add(text)
    }

    List<string> flush(ref this) {
        return this.sent.replace(List<string>())
    }
}

void main() {
    var box = Outbox { draft = "hello", sent = List<string>() }
    box.send()
    box.send()
    box.draft = "bye"
    box.send()
    print(box.flush())
    print(box.sent.length())
    print(box.draft ?? "no draft")
}
```

```text
["hello", "bye"]
0
no draft
```

`take()` gives the text and leaves `null` in `draft`; `replace` gives the list and leaves an empty one in
`sent`. The second `send` finds no draft and returns. The outbox is never left with a hole.

### Behind a pointer, with `dispose`

```sg
struct Connection : Disposable {
    string host
    int sent

    void send(ref this, read string text) {
        this.sent += (int)text.length()
    }

    void dispose(ref this) {
        print($"close {this.host} after {this.sent} bytes")
    }
}

void main() {
    var link = new Connection { host = "db", sent = 0 }
    link.send("hello")
    link.send("bye")
    print("done")
}
```

```text
done
close db after 8 bytes
```

`link` is a `Connection*`, and its methods are called as on the value. `dispose` is the method `Disposable`
asks for: the program never calls it, and it runs once where the last owner ends, with `this` the value that
ends.

## What the compiler refuses

### Writing through `read this`

```sg error
struct Counter {
    int count

    int bump(read this) {
        this.count += 1
        return this.count
    }
}

void main() {
    var c = Counter { count = 0 }
    print(c.bump())
}
```

```text
error: this field is reached through a `read` borrow and cannot be written
  --> example.sg:5:9
 5 |         this.count += 1
   |         ^^^^^^^^^^
  hint: take the value as `ref` instead of `read`
```

`int bump(ref this)` makes the program right.

### A field without `this.`

```sg error
struct Counter {
    int count

    int value(read this) {
        return count
    }
}

void main() {
    var c = Counter { count = 0 }
    print(c.value())
}
```

```text
error: there is no variable named `count`
  --> example.sg:5:16
 5 |         return count
   |                ^^^^^
  hint: a field is reached through `this`: `this.count`
```

### `this` in a function that has none

```sg error
struct Counter {
    int count

    Counter zero() {
        return Counter { count = this.count }
    }
}

void main() {
    var c = Counter.zero()
    print(c.count)
}
```

```text
error: there is no `this` here
  `this` exists in a method that takes it as its first parameter: `this`, `read this` or `ref this`
  --> example.sg:5:34
 5 |         return Counter { count = this.count }
   |                                  ^^^^
```

`zero` has no `this` parameter: it belongs to the type and is called as `Counter.zero()`, on no value.

### `this` anywhere but first

```sg error
struct Counter {
    int count

    int plus(int by, read this) {
        return this.count + by
    }
}

void main() {
    var c = Counter { count = 0 }
    print(c.plus(1))
}
```

```text
error: `this` must be the first parameter
  --> example.sg:4:22
 4 |     int plus(int by, read this) {
   |                      ^^^^^^^^^
```

### `this` as a name

```sg error
void main() {
    var this = 3
    print(this)
}
```

```text
error: `this` is the receiver of a method and cannot name a variable
  --> example.sg:2:9
 2 |     var this = 3
   |         ^^^^
```

### A field moved out through `ref this`

```sg error
struct Outbox {
    string? draft

    string? send(ref this) {
        return this.draft
    }
}

void main() {
    var box = Outbox { draft = "hello" }
    print(box.send() ?? "nothing")
}
```

```text
error: a `string?` field cannot be moved out through a borrow
  --> example.sg:5:16
 5 |         return this.draft
   |                ^^^^^^^^^^
  hint: take it with `this.draft.take()`, which leaves `null` there
```

The caller still owns the outbox, and its `draft` must hold a value when `send` ends. For a field that is not a
`T?`, the hint is `replace`, which leaves the value it is given.

### A method that takes `this`, called on a borrow

```sg error
struct Ticket {
    string code

    string spend(this) {
        return this.code
    }

    void redeem(ref this) {
        print(this.spend())
    }
}

void main() {
    var t = Ticket { code = "A1" }
    t.redeem()
}
```

```text
error: `spend` takes `this` by value and uses it up, but this value is only borrowed
  --> example.sg:9:15
 9 |         print(this.spend())
   |               ^^^^
```

`redeem` borrows the ticket, so it cannot give it away. Either `redeem` takes `this` too, or `spend` takes
`read this` and gives `read string` back.
