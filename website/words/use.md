---
name: use
kind: keyword
summary: Brings the public names of a module into a file, to be written without the module in front.
see: public, alias, sg, sg build, sg add
learn: ../docs/learn/book/09-modules.md#a-module-and-use
---

## Forms

| Form | What it is |
|---|---|
| `use accounts` | the public names of the module `accounts`, the folder `accounts/` beside the program |
| `use units.wind` | a module inside a module, named by its whole path from the root |
| `units.format(x)`, `accounts.Account` | a full name, which needs no `use` |
| `use money` | a package, by the name its line in `sg.project` gives it |
| `std.Hex` | a type of the language's own module, by its full name; its types need no `use` |

## What it means

`use` names a module at the top of a file, one on a line, before everything else. Every `public` name of that
module can then be written bare in the file: `Account` instead of `accounts.Account`. It counts for that file
only; each file writes the `use` lines it needs. The files of one module see each other with no `use`.

A module is a folder, and it is named by its path from the root of the program, also from inside another
module: `use units.wind`, even in a file of `units/`. The root is the file given to `sg`, with the folders beside
it as its modules, or every file of the folder given. A module is read only when a file names it, with `use` or
with a full name, so a folder nobody names is not looked at.

A full name, `units.format`, works with or without `use`. When two modules named with `use` have the same name,
the bare name is an error where it is used, and the full name settles it. A name is looked for in this order: a
variable, the file's own module, the names brought in by `use`, then the types of the language. So a type added
to the library never breaks a program that has a name of its own like it.

The library, the module `std`, needs no `use`: its types are seen from every module, and `use std` is an error.
A type of your own with the name of one of its types comes first, and the library's is then written with the
module in front: `std.Duration`, `std.Hex`, `std.List<int>`, `std.File.readText(path)`. This holds for every
type of the library, `List`, `HashMap`, `File`, `IoError` and `TcpStream` among them, and a function, a const or
a static may have such a name too. Kept are only the words of the language, the types of numbers, `bool`,
`char`, `string` and `void`, and the three types its syntax makes: `Result` (by `try`, `Ok` and `Error`), `Task`
(by `async`) and `Func` (by a function's name used as a value). `Stdin`, `Stdout` and `Stderr` are kept too: they
are not types but the streams every function has.

A `use` that nothing needs is not an error. Two modules may `use` each other. There
is no `use ... as`: a type gets a second name with `alias`.

A package is named after `use` by the word its `require` line in `sg.project` gives it, or by the last part of
its address when the line gives none. Its root files are that module and its folders the modules under it
(`money.reader`). Inside a package, its own folders are named from its root, and the program's modules are not
there for it.

## Examples

### The library, with no `use`

```sg
void main() {
    var timeout = Duration.fromMilliseconds(1500)
    print(timeout)
    print(Hex.encode("SG".asBytes()))
    var value = Json.parse("[10, 20, 30]") ?? JsonValue.Null
    print(value.at(1)?.asLong() ?? 0)
}
```

```text
1.5s
5347
20
```

`Duration`, `Hex` and `Json` are types of the library, seen from every file.

### A name of your own, and the library's by its full name

```sg
struct Hex : Formattable {
    int column
    int row
}

void main() {
    var cell = Hex { column = 3, row = 5 }
    print(cell)
    print(std.Hex.encode("SG".asBytes()))
}
```

```text
Hex { column = 3, row = 5 }
5347
```

`Hex` is the struct of the program, a cell of a map. The library's `Hex` is still there under its full name.

### A `File` of your own

```sg
struct File {
    string name
    int lines
}

void main() {
    var notes = File { name = "notes.txt", lines = 3 }
    print(notes.name)
    var words = std.List<string>()
    words.add("sg")
    match std.File.readText(read "/no/such/file.txt") {
        Ok(text) => print(text)
        Error(NotFound) => print("not there")
        Error(other) => print(other)
    }
    print(words)
}
```

```text
notes.txt
not there
["sg"]
```

`File` here is the program's struct. The library's file functions are still there as `std.File`, and its
collections as `std.List<T>`, as for any type of the library.

### A module beside the program

```text
weather/
  main.sg
  units/
    temperature.sg
    wind/
      speed.sg
```

```sg file title="weather/units/temperature.sg"
public double fahrenheit(double celsius) {
    return celsius * 9 / 5 + 32
}

public string format(double celsius) {
    return $"{celsius} C".toString()
}
```

```sg file title="weather/units/wind/speed.sg"
public double metersPerSecond(double kmh) {
    return kmh / 3.6
}

public string format(double kmh) {
    return $"{kmh} km/h".toString()
}
```

```sg file title="weather/main.sg"
use units
use units.wind

void main() {
    var celsius = 21.5
    var kmh = 36.0
    print($"{units.format(celsius)}, {fahrenheit(celsius)} F")
    print($"{units.wind.format(kmh)}, {metersPerSecond(kmh)} m/s")
}
```

```text
$ sg weather
21.5 C, 70.7 F
36 km/h, 10 m/s
$ sg build weather -o weather-app && ./weather-app
21.5 C, 70.7 F
36 km/h, 10 m/s
```

`units` and `units.wind` are two modules: a folder inside a module is a module of its own, and `use units` does
not bring in `units.wind`. `fahrenheit` and `metersPerSecond` are written bare. Both modules have a `format`, so
`main.sg` writes it with the module in front.

### Two modules that use each other

```sg file title="planner/calendar/day.sg"
use holidays

public struct Day : Copyable {
    public int month
    public int date
    public int weekday

    public bool isWorkday(read this) {
        return this.weekday < 6 && !isHoliday(this)
    }
}
```

```sg file title="planner/holidays/list.sg"
use calendar

public bool isHoliday(Day day) {
    return day.month == 1 && day.date == 1 || day.month == 12 && day.date == 25
}
```

```sg file title="planner/main.sg"
use calendar

void main() {
    var newYear = Day { month = 1, date = 1, weekday = 4 }
    var nextDay = newYear with { date = 2, weekday = 5 }
    print(newYear.isWorkday())
    print(nextDay.isWorkday())
}
```

```text
$ sg planner
false
true
```

`calendar` asks `holidays`, and `holidays` takes the `Day` of `calendar`. Neither has to be split up to break
the circle.

### A package

The program `shop` takes the package `money` from the folder beside it while both are being written; a
`require` line alone would fetch it from its address.

```text
shop/
  sg.project
  main.sg
money/
  sg.project
  format.sg
  internal/
    group.sg
```

```text title="shop/sg.project"
require money example.com/ada/money 1.0.0
local money ../money
```

```text title="money/sg.project"
package example.com/ada/money
```

```sg file title="money/format.sg"
use internal

public string euros(long cents) {
    return $"{grouped(cents / 100)}.{cents % 100:d2} EUR".toString()
}
```

```sg file title="money/internal/group.sg"
public string grouped(long whole) {
    if whole < 1000 {
        return $"{whole}".toString()
    }
    return $"{grouped(whole / 1000)},{whole % 1000:d3}".toString()
}
```

```sg file title="shop/main.sg"
use money

void main() {
    print(euros(123456789))
    print(money.euros(5))
}
```

```text
$ cd shop
$ sg main.sg
1,234,567.89 EUR
0.05 EUR
```

`use money` is the name `require` gives. In the package, `use internal` names its folder `money/internal/` from
the package's root; the program would write `money.internal`, which a folder named `internal` keeps for `money`
alone.

## What the compiler refuses

### `use` below other lines

```sg error
const int Days = 7

use calendar

void main() {
    print(Days)
}
```

```text
error: `use` is written at the top of the file, before everything else
  --> example.sg:3:1
 3 | use calendar
   | ^^^
```

Comments may stand above the `use` lines; nothing else may. The same `use` line twice is refused too:
`` `use calendar` is written twice ``.

### A module that is not there

```sg error
use stock

void main() {
    print("inventory")
}
```

```text
error: there is no module named `stock`
  a module is a folder with source files, next to the files of the program: `stock/`
  --> example.sg:1:5
 1 | use stock
   |     ^^^^^
  hint: a package is named on a `require` line of `sg.project`, beside the program, which `sg add <address>` writes
```

A name `use` writes is a folder of the program or a package `sg.project` names, so the hint says how a package is
named. When a module of that name is further down, the hint gives its whole path: `use wind` in a file of `units/`
gets the hint `` `use units.wind` ``.

### `use std`

```sg error
use std

void main() {
    print(Duration.fromSeconds(3))
}
```

```text
error: `std` is the language's own module, and its types are seen from every module
  --> example.sg:1:5
 1 | use std
   |     ^^^
  hint: take the line away: `Duration`, `Instant` and `DateTime` need no `use`
```

### One name in two modules

With the folder `weather` above:

```sg file title="weather/main.sg"
use units
use units.wind

void main() {
    print(format(21.5))
}
```

```text
$ sg weather
error: `format` exists in `units` and `units.wind`
  --> weather/main.sg:5:11
 5 |     print(format(21.5))
   |           ^^^^^^^^^^^^
  hint: write the module in front: `units.format`
```
