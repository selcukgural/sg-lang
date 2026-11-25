---
title: Time
description: Duration, Instant, DateTime and DateTimeOffset, and the clocks under them.
sidebar_position: 5
---

# Time

A `Duration` is a span of time. An `Instant` is a reading of the monotonic clock, which never goes back, for
measuring how long something took. A `DateTime` is a moment of the calendar in UTC, and a `DateTimeOffset` the
same moment as the clocks of a place show it. All four are `Copyable`, `Equatable`, `Hashable` and `Comparable`,
and all but `Instant` are printed. They are written in SG, in the language's own module `std`, which a program
gets by naming one of them.

A method that compares (`compare`) takes the other value as `read`: `a.compare(read b)`. `<`, `==` and the
others need nothing written.

## `Duration`

A count of nanoseconds in a `long`; it may be negative. Its arithmetic follows the rule of the numbers: an
overflow panics in a debug build and in a test, and wraps in a release build.

| Signature | What it does |
|---|---|
| `Duration Duration.fromNanoseconds(long count)`, `fromMicroseconds`, `fromMilliseconds`, `fromSeconds` | A span of so many units |
| `long nanoseconds(read this)`, `microseconds`, `milliseconds`, `seconds` | The whole units in the span, cut toward zero |
| `double totalSeconds(read this)` | The span in seconds, with the fraction |
| `Duration plus(read this, Duration other)`, `minus(read this, Duration other)` | The sum, the difference |
| `Duration times(read this, long factor)` | The span `factor` times |
| `int compare(read this, read Duration other)` | Negative, 0 or positive |

`print` writes it in the unit its size calls for, with the fewest digits after the point: `1.5s`, `250ms`,
`3µs`, `-2ms`, `0s`. Seconds stay seconds: `90s`.

```sg
void main() {
    var step = Duration.fromMilliseconds(1500)
    print(step)
    print($"{step.seconds()} {step.milliseconds()} {step.totalSeconds()}")
    print(Duration.fromSeconds(90))
    print(Duration.fromMicroseconds(3))
    print(Duration.fromNanoseconds(-2000000))
    var total = step.plus(Duration.fromMilliseconds(250)).times(2)
    print(total)
    print(total.minus(Duration.fromSeconds(4)))
    print($"{step < total} {step.compare(read total)}")
}
```

It prints:

```text
1.5s
1 1500 1.5
90s
3µs
-2ms
3.5s
-500ms
true -1
```

## `Instant`

| Signature | What it does |
|---|---|
| `Instant Instant.now()` | Reads the monotonic clock; on Linux and macOS it stops while the machine sleeps, as Rust's `Instant` |
| `Duration elapsed(read this)` | The time gone by since this moment |
| `Duration since(read this, Instant earlier)` | The time from `earlier` to this; negative when `earlier` is later |
| `Instant plus(read this, Duration span)`, `minus(read this, Duration span)` | A moment a span later, or earlier |

Only the time between two instants means something, so an `Instant` is not printed:

```sg error
void main() {
    var start = Instant.now()
    print(start)
}
```

```text
error: `Instant` cannot be written as text: it is not `Formattable`
  an `Instant` means something only next to another one
  --> example.sg:3:11
 3 |     print(start)
   |           ^^^^^
  hint: the time between two is a `Duration`, which is written: `start.elapsed()`, `later.since(earlier)`
```

```sg
long work() {
    long total = 0
    for i in 0..1000000 {
        total += i
    }
    return total
}

void main() {
    var start = Instant.now()
    print(work())
    var took = start.elapsed()
    print(took >= Duration.fromSeconds(0))
    var later = start.plus(Duration.fromSeconds(2))
    print(later.since(start))
}
```

It prints:

```text
499999500000
true
2s
```

Mixing the two clocks is an error: `i.since(t)` with `t` a `DateTime` says that the time between two readings
is taken on one clock, since the clock of the system can be set and go back.

## `DateTime`

The nanoseconds since 1970-01-01T00:00:00Z, without leap seconds, for the years 1677 to 2262, in the
Gregorian calendar.

| Signature | What it does |
|---|---|
| `DateTime DateTime.now()` | Reads the clock of the system, in UTC; it can be set, and can go back |
| `Result<DateTime, DateError> DateTime.utc(int year, int month, int day, int hour, int minute, int second)` | A moment from its parts, each checked |
| `DateTime DateTime.fromUnixSeconds(long seconds)`, `fromUnixMilliseconds(long milliseconds)` | A moment from a count since 1970 |
| `Result<DateTime, DateError> DateTime.parse(read string text)` | A moment in RFC 3339, taken to UTC |
| `Result<DateTime, DateError> DateTime.parseHttpDate(read string text)` | A moment as HTTP writes it, in any of the three forms of RFC 9110 |
| `string toHttpDate(read this)` | The moment as HTTP writes it in `Date` and `Last-Modified`: `Sun, 06 Nov 1994 08:49:37 GMT` |
| `int year(read this)`, `month`, `day`, `hour`, `minute`, `second`, `nanosecond` | The parts, in UTC |
| `DayOfWeek dayOfWeek(read this)` | `Monday` to `Sunday` |
| `long unixSeconds(read this)`, `unixMilliseconds` | The count since 1970, rounded down |
| `Duration since(read this, DateTime earlier)` | The time from `earlier` to this |
| `DateTime plus(read this, Duration span)`, `minus` | A moment a span later, or earlier |
| `DateTimeOffset toLocal(read this)` | The moment at the offset of the system's time zone then |
| `Result<DateTimeOffset, DateError> withOffset(read this, Duration offset)` | The moment at an offset of your own |

`print` writes ISO 8601: `2026-11-15T09:30:00Z`, with the fraction of the second in 3, 6 or 9 digits when
there is one. Another format is written with the parts: `$"{t.day():D2}.{t.month():D2}.{t.year()}"`.

A `DateTime` is made by its functions only; `DateTime { ... }` is an error whose hint names them.

The dates of HTTP (choice 1866): `toHttpDate` writes RFC 9110's IMF-fixdate, always in GMT, the part of a
second left out. `parseHttpDate` reads it and the two older forms a recipient must take: RFC 850's `Sunday,
06-Nov-94 08:49:37 GMT`, whose year 00 to 69 is 2000 to 2069 and 70 to 99 is 1970 to 1999, and C's asctime,
`Sun Nov  6 08:49:37 1994`. It reads the names as RFC 9110 writes them, in that case only, and the day of the
week must be the date's: `InvalidText` at the first byte that does not fit, at 0 for a day of the week that is
not the date's.

```sg
void main() {
    var t = DateTime.fromUnixSeconds(784111777)
    print(t.toHttpDate())
    print(DateTime.parseHttpDate("Sunday, 06-Nov-94 08:49:37 GMT") ?? DateTime.fromUnixSeconds(0))
    print(DateTime.parseHttpDate("Sun Nov  6 08:49:37 1994") ?? DateTime.fromUnixSeconds(0))
    print(DateTime.parseHttpDate("Mon, 06 Nov 1994 08:49:37 GMT") is Error(InvalidText(0)))
}
```

It prints:

```text
Sun, 06 Nov 1994 08:49:37 GMT
1994-11-06T08:49:37Z
1994-11-06T08:49:37Z
true
```

```sg
Result<void, DateError> main() {
    var launch = try DateTime.utc(2026, 11, 15, 9, 30, 0)
    print(launch)
    print($"{launch.hour()}:{launch.minute():D2} {launch.dayOfWeek()} {launch.unixSeconds()}")
    print($"{launch.day():D2}.{launch.month():D2}.{launch.year()}")
    print(DateTime.fromUnixMilliseconds(1500))
    var later = launch.plus(Duration.fromSeconds(36 * 3600))
    print(later)
    print(later.since(launch))
    print(try DateTime.parse("2026-11-18T12:30:05.5+03:00"))
    print(DateTime.utc(2026, 2, 30, 0, 0, 0) is Error(InvalidDay))
    return Ok()
}
```

It prints:

```text
2026-11-15T09:30:00Z
9:30 Sunday 1794735000
15.11.2026
1970-01-01T00:00:01.500Z
2026-11-16T21:30:00Z
129600s
2026-11-18T09:30:05.500Z
true
```

## `DateTimeOffset`

A moment and the offset from UTC at which it is seen: `2026-11-18T12:30:05+03:00`. The offset is kept as it
was: `plus` of a day after the clocks of the place changed gives the old offset, and
`t.toUtc().plus(span).toLocal()` the new one. Two are equal when both the moment and the offset are;
`a.toUtc() == b.toUtc()` asks only for the moment.

| Signature | What it does |
|---|---|
| `Result<DateTimeOffset, DateError> DateTimeOffset.local(int year, int month, int day, int hour, int minute, int second)` | The moment the clocks of the system's time zone show that at; `SkippedLocalTime` in the hour summer time skips, the earlier of two in the hour it repeats |
| `Result<DateTimeOffset, DateError> DateTimeOffset.parse(read string text)` | RFC 3339: the offset `Z`, `+hh:mm` or `-hh:mm` |
| `DateTime toUtc(read this)` | The moment in UTC |
| `Duration offset(read this)` | How far the place is ahead of UTC; negative west of it |
| `year`, `month`, `day`, `hour`, `minute`, `second`, `nanosecond`, `dayOfWeek` | The parts as the clocks of the place show them |
| `since`, `plus`, `minus`, `compare` | As on `DateTime`; `plus` keeps the offset |

The time zone of the system is `TZ` and `/etc/localtime` on Linux and macOS, and the zone set in Windows.

```sg
Result<void, DateError> main() {
    var moment = try DateTime.utc(2026, 11, 15, 9, 30, 0)
    var istanbul = try moment.withOffset(Duration.fromSeconds(3 * 3600))
    print(istanbul)
    print($"{istanbul.hour()} {istanbul.offset()} {istanbul.toUtc()}")
    var parsed = try DateTimeOffset.parse("2026-11-18T12:30:05-05:00")
    print($"{parsed} {parsed.toUtc()}")
    match DateTime.parse("2026-11-18 12:30") {
        Ok(t) => print(t)
        Error(e) => print(e)
    }
    print(moment.withOffset(Duration.fromSeconds(86400)) is Error(InvalidOffset))
    var now = DateTime.now()
    print(now.toLocal().toUtc() == now)
    return Ok()
}
```

It prints:

```text
2026-11-15T12:30:00+03:00
12 10800s 2026-11-15T09:30:00Z
2026-11-18T12:30:05-05:00 2026-11-18T17:30:05Z
InvalidText(10)
true
true
```

## `DateError`

`InvalidMonth`, `InvalidDay`, `InvalidHour`, `InvalidMinute`, `InvalidSecond` (a second of 60 too: leap
seconds are not counted), `OutOfRange` (outside the years a `DateTime` holds), `InvalidOffset` (a day or more
either way, or a part of a second), `SkippedLocalTime`, and `InvalidText(nint position)`, the place where a text
stops being RFC 3339. The parts are checked in order, the month before the day, so that the day is checked
against its month.

## `Clock`

Under these types lie three readings that only the module `std` can call: `Clock.monotonic()` for `Instant`,
`Clock.wall()` for `DateTime`, and `Clock.offsetAt(seconds)` for the time zone. A program uses the types, so a
reading always comes with its meaning:

```sg error
void main() {
    print(Clock.monotonic())
}
```

```text
error: there is no variable or type named `Clock`
```

To wait, an async function awaits `Task.delay(ms)`, and a deadline is `CancellationSource.after(ms)`: see
[Tasks](./tasks.md). Neither a `const` nor the condition of `lock ... when` may read the clock.

## How fast

On an Apple M3 Max, ten million moments are written in ISO 8601 in 0.49 seconds, the time of Rust's `chrono`
(0.50) and faster than Zig's own writing (0.70); 100 MB of RFC 3339, 3.4 million moments, are read in 0.136
seconds, against 0.173 for `chrono` and 0.125 for a reader written by hand in Zig; the clock is read ten
million times in 0.097 seconds, against 0.166 for Rust and 0.158 for Zig. A million moments seen in a time zone take 0.20 seconds where `chrono`
takes 0.05: SG asks the C library each time, `chrono` reads the rules of the zone once ([the
tables](../benchmarks/library.md#time)).
