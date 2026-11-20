---
title: Modules
description: A folder is a module; `use`, `public`, `readonly` and a folder `internal` say what other modules see; a program can be a file or a whole folder, and `sg.project` names its packages.
sidebar_position: 9
---

# Modules

A program grows past one file by putting files in folders. In SG **a folder is a module**: every `.sg` file in
it belongs to it, and the files of one module see each other with nothing written. There is no list of files
to keep and no header to include. What a module shows to the others is marked `public`; everything else stays
inside it.

The programs on this page are made of several files, so each block is one file, named above it, and the
output is what `sg` prints when it runs them in the folder shown.

## A module and `use`

Here a program uses a module `stock`, the folder next to it:

```text
inventory/
  main.sg
  stock/
    product.sg
    warehouse.sg
```

```sg nocheck title="inventory/stock/product.sg"
public struct Product {
    public readonly string code
    public long price
    int quantity

    public Product create(string code, long price) {
        return Product { code = code, price = price, quantity = 0 }
    }

    public int available(read this) {
        return this.quantity
    }
}

nint? indexOf(read List<Product> products, read string code) {
    for i in 0..products.length() {
        if products[i].code == code {
            return i
        }
    }
    return null
}
```

```sg nocheck title="inventory/stock/warehouse.sg"
public struct Warehouse {
    List<Product> products

    public Warehouse create() {
        return Warehouse { products = List<Product>() }
    }

    public void add(ref this, Product product) {
        this.products.add(product)
    }

    public bool receive(ref this, read string code, int count) {
        var at = indexOf(read this.products, code) ?? return false
        this.products[at].quantity += count
        return true
    }

    public int available(read this, read string code) {
        var at = indexOf(read this.products, code) ?? return 0
        return this.products[at].available()
    }
}
```

```sg nocheck title="inventory/main.sg"
use stock

void main() {
    var warehouse = Warehouse()
    warehouse.add(Product("K-1".clone(), 1500))
    warehouse.add(Product("D-1".clone(), 4000))
    print(warehouse.receive("K-1", 100))
    print(warehouse.receive("X-9", 5))
    print(warehouse.available("K-1"))
}
```

```text
$ cd inventory
$ sg main.sg
true
false
100
```

`warehouse.sg` uses `Product`, `indexOf` and the field `quantity` without a `use`: they are in its own module.
`main.sg` writes `use stock` at the top, before everything else, and gets the `public` names of the folder.
`Product(...)` calls the struct's `create` function, and `Warehouse()` is short for `Warehouse.create()`.

`sg build main.sg` compiles the same program, with its modules, into one executable. Only the modules the
program names are read: a folder nobody names is not looked at.

## `public`

Without `public`, a name stays inside its module. `public` goes on a struct, an enum, a function, a field, a
method, a constant, a static and a type alias. A field that is not public cannot be reached from outside:

```sg nocheck title="inventory/main.sg"
use stock

void main() {
    var pen = Product("K-1".clone(), 1500)
    pen.quantity = 5
}
```

```text
error: `quantity` is not public: it stays inside the `stock` module
  --> main.sg:5:9
 5 |     pen.quantity = 5
   |         ^^^^^^^^
```

A function that is not public is found, and the message shows where it is:

```text
error: `indexOf` exists in the `stock` module but is not public
  --> main.sg:5:11
 5 |     print(indexOf(read products, "K-1") ?? -1)
   |           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  it is defined here, without `public`:
  --> ./stock/product.sg:15:7
 15 | nint? indexOf(read List<Product> products, read string code) {
    |       ^^^^^^^
```

A struct with a field that is not public cannot be built field by field outside its module, since the outside
cannot give that field a value. It is built with its `create` function, which the hint names:

```text
error: `quantity` is not public, so a `Product` cannot be built field by field outside the `stock` module
  --> main.sg:4:15
 4 |     var pen = Product { code = "K-1".clone(), price = 1500, quantity = 3 }
   |               ^^^^^^^
  hint: `Product(code, price)`
```

This is how a module keeps a rule true: no code outside `stock` can make a quantity below zero, because no
code outside can write it. A `public` name cannot show a type that is not public either: a `public` function
that returns a private struct is an error, with the hint to make both public or neither.

## `readonly`

`readonly` on a field means it is given when the value is built and never changed afterwards, from inside the
module or outside. `pen.price = 1750` is fine for the `public long price` above, but `code` is
`public readonly`:

```text
error: `code` is `readonly`: it is given when the value is built and not changed afterwards
  --> main.sg:6:9
 6 |     pen.code = "K-2".clone()
   |         ^^^^
```

It holds all the way down: nothing behind a `readonly` field changes either. A `readonly List` cannot be added
to, lent as `ref` or given to a method that takes `ref this`. The value that holds the field can still be
replaced as a whole. The check is made by the compiler and costs nothing when the program runs.

## Full names

A name of a module can be written with the module in front, with or without `use`: `stock.Product`,
`billing.Line { ... }`, `stock.Color.Red`, `List<stock.Product>`. When two modules named with `use` have the
same name, the bare name is an error where it is used, and the hint is the full name. If `stock` in the `shop`
program below had a `vat` function as `billing` does, `print(vat(4500))` with both `use` lines would give:

```text
error: `vat` exists in `stock` and `billing`
  --> shop/main.sg:5:11
 5 |     print(vat(4500))
   |           ^^^^^^^^^
  hint: write the module in front: `stock.vat`
```

A name of the file's own module comes before the names brought in by `use`, and a variable comes before both,
so adding a name to a module never breaks the files that use it.

Without any `use`, the compiler still helps when a name is missing:

```text
error: there is no function named `Warehouse`
  --> main.sg:2:21
 2 |     var warehouse = Warehouse()
   |                     ^^^^^^^^^
  hint: `use stock` at the top of the file
```

## A program that is a folder

`sg main.sg` makes that one file the root of the program. `sg shop` runs the folder `shop` as the program:
every file in it is the root module, and its folders are the modules. The file with `main` can then be split
up freely. Modules inside modules are named by their whole path from the root: `use stock.units`.

```text
shop/
  main.sg
  describe.sg
  billing/
    line.sg
  stock/
    product.sg
    units/
      unit.sg
```

```sg nocheck title="shop/main.sg"
use stock
use stock.units

void main() {
    var pen = Product("pen".clone(), 1500)
    var line = billing.Line { name = pen.name.clone(), count = 3, total = pen.price * 3 }
    print(describe(read line))
    print(billing.vat(line.total))
    var box = Unit.Box(12)
    print($"{box} {pieces(read box)} {stock.units.pieces(read Unit.Piece)}")
}
```

```sg nocheck title="shop/describe.sg"
string describe(read billing.Line line) {
    return $"{line.count} x {line.name}: {line.total}".toString()
}
```

```sg nocheck title="shop/billing/line.sg"
public struct Line {
    public string name
    public int count
    public long total
}

public long vat(long total) {
    return total / 5
}
```

```sg nocheck title="shop/stock/product.sg"
public struct Product {
    public readonly string name
    public readonly long price

    public Product create(string name, long price) {
        return Product { name = name, price = price }
    }
}
```

```sg nocheck title="shop/stock/units/unit.sg"
public enum Unit : Formattable {
    Piece
    Box(int pieces)
}

public int pieces(read Unit unit) {
    return match unit {
        Piece => 1
        Box(count) => count
    }
}
```

```text
$ sg shop
3 x pen: 4500
900
Box(12) 12 1
$ sg build shop -o shop-app && ./shop-app
3 x pen: 4500
900
Box(12) 12 1
```

`main.sg` calls `describe`, which is in another file of the same folder. Run as a single file,
`sg shop/main.sg`, the program is only that file, and `describe` is not found. Both forms are useful: a folder of
many small programs, each with its own `main`, is run file by file; one program spread over files is run as a
folder.

A folder is a module only if its name can be written as a name (letters, digits and `_`).

## `internal`

A library of several folders often has modules its own folders need and its users should not touch. A
module in a folder named `internal` is named only from inside the folder that holds it: `stock/internal/`
is for `stock` and the folders under it.

```text
error: `stock.internal` is in a folder `internal`: only the modules inside `stock` name it
  a folder `internal` keeps what is inside it for the folder that holds it
  --> main.sg:2:11
 2 |     print(stock.internal.secret())
   |           ^^^^^^^^^^^^^^^^^^^^^^^
```

## Packages

A package is a library kept in its own repository: its address and a version tag (`v1.4.0`) name it, and no
registry stands between. A program names the packages it uses in a file `sg.project` beside it, a line for
each:

```text
package example.com/ada/shop

require xml example.com/ada/xml 1.4.0
require letters example.com/bob/utf 1.1.0
```

The word after `require` is the name the program writes after `use`; without it, the last part of the address
is the name. `sg add` writes such a line, with the newest version that fits, and fetches the package:

```text
$ sg add example.com/ada/xml@1.4
example.com/ada/xml 1.4.0 (added)
example.com/bob/utf 1.0.0 (added)
```

A package is fetched with `git` from `https://` and its address, and no code of it runs while it is fetched
or built. Its files are kept once for you, read only, in a cache (`SG_CACHE` names another folder), and
`sg.lock` beside `sg.project` holds the SHA-256 of each package's files: the program builds from the same
files on every machine, and a tag moved to other files is refused. The versions are chosen as Go chooses
them, the least each `require` asks for, so a new release of a package changes no build until you run
`sg update`. `sg remove xml` takes a package away, and `sg lock` writes `sg.lock` again after you change
`sg.project` by hand. A package another package uses is fetched too, and the program need not name it.

A `local` line takes a package from a folder instead, written from the folder of `sg.project`, while you
work on both:

```text
local xml ../lib/xml
local letters ../lib/utf
```

A package has an `sg.project` of its own, which gives its address with `package` and names the packages it
uses. Its root files are the module `xml`, its folders `xml.reader` and so on:

```sg nocheck title="shop/main.sg"
use xml
use xml.reader

void main() {
    print(tag("item"))
    var reader = Reader { name = "doc".clone() }
    print(describe(read reader))
    print(letters.bracket("x"))
}
```

Inside the package, its folders are named from its own root (`use reader`), and the packages it uses by the
names its own `sg.project` gives them. A package sees nothing of the program that uses it: a `use stock` in
it does not find the program's folder `stock`. Its `internal` folder is its own. A folder beside the program
cannot have the name of a package, and a package kept inside the program's folder is read only as the
package.

A package with `unsafe` or `extern` in it is built only when the program allows it, with `unsafe` at the end
of the package's line, also for a package it uses through another:

```text
require zlib example.com/bo/zlib 1.2.0 unsafe
```

`sg add` and `sg update` show which packages use `unsafe` and which libraries of C they link, and an update
to a version that starts to use `unsafe` stops until you allow it.

## Type aliases: `using`

`using Name = Type` gives a type a second name. It is the same type, not a new one, so values pass between the
two names freely. An alias also builds what it names:

```sg
using NodeRef = shared Node
using Scores = HashMap<string, int>

struct Node {
    int value
    NodeRef? next
}

int length(read NodeRef? list) {
    var count = 0
    NodeRef? at = share list
    while at is not null {
        count += 1
        at = share at.next
    }
    return count
}

int best(read Scores scores) {
    var top = 0
    for entry in scores {
        top = top.max(entry.value)
    }
    return top
}

void main() {
    var last = new NodeRef { value = 2, next = null }
    var first = new NodeRef { value = 1, next = share last }
    print(length(read first))

    HashMap<string, int> scores = HashMap<string, int> { {"ada", 36}, {"grace", 45} }
    print(best(read scores))
}
```

It prints:

```text
2
45
```

`best` takes a `read Scores` and is given a `HashMap<string, int>`: they are one type. An alias is private to
its module unless it is written `public using`. A collection is still built by the name of its type:
`Scores()` is an error whose hint is `HashMap<string, int>()`.
