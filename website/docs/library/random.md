---
title: Random numbers
description: Random, a fast generator that a seed sets, and SecureRandom, the random bytes of the system.
sidebar_position: 7
---

# Random numbers

There are two sources of random numbers. A `Random` is a fast generator whose numbers a seed decides: the same
seed gives the same numbers in the interpreter, in compiled code and on every system, which is what a
simulation, a game or a test wants. `SecureRandom` reads the random bytes of the system, which nobody can work
out from the ones before: that is what a key, a token or a password is made of. Both are written in SG in the
module `std`.

## `Random`

The generator is xoshiro256\*\*, its state set from the seed with SplitMix64, as C#'s `System.Random` does.
Whoever sees some of its numbers can work out the rest, so it is never for a secret.

| Signature | What it does |
|---|---|
| `Random Random.fromSeed(long seed)` | A generator whose numbers the seed decides |
| `Random Random.fromSystem()` | One seeded with 256 random bits of the system: different on every run |
| `int nextInt(ref this, int low, int high)` | A number from `low` up to `high`, without `high`, each as likely as the others |
| `long nextLong(ref this, long low, long high)` | The same for a `long` |
| `double nextDouble(ref this)` | A fraction from 0 up to 1, without 1 |
| `bool nextBool(ref this)` | `true` or `false`, as likely |
| `ulong nextUlong(ref this)` | 64 random bits |
| `void fill(ref this, ref byte[] target)` | Every byte of `target`, eight from each draw, the lowest first |

`nextInt` and `nextLong` panic when there is no number between the two ends: `` `nextInt` is given no number to
choose from: 5 up to 5, without 5 ``. They pick without favouring any number (Lemire's way: one multiplication,
and a second draw only when the first would favour some).

A generator changes with every number it gives, so its methods take `ref this` and a function that draws takes
a `ref Random`. A seed of 42 gives these numbers on every system:

```sg
int roll(ref Random r) {
    return r.nextInt(1, 7)
}

void shuffle(ref List<string> items, ref Random r) {
    var i = items.length() - 1
    while i > 0 {
        var j = (nint)r.nextLong(0, (long)i + 1)
        var kept = items[i].clone()
        items[i] = items[j].clone()
        items[j] = kept
        i -= 1
    }
}

void main() {
    var r = Random.fromSeed(42)
    print(r.nextUlong())
    var rolls = List<int>()
    for _ in 0..10 {
        rolls.add(roll(ref r))
    }
    print(rolls)
    print(r.nextLong(-1000, 1000))
    print(r.nextDouble())
    print(r.nextBool())
    var bytes = byte[6]
    r.fill(ref bytes)
    print(bytes)
    var names = List<string> { "ada", "bea", "cem", "dua", "eda" }
    shuffle(ref names, ref r)
    print(names)
    var again = Random.fromSeed(42)
    print(again.nextUlong())
}
```

It prints:

```text
1546998764402558742
[3, 5, 6, 6, 5, 5, 6, 5, 4, 5]
-643
0.8010242975288078
false
[205, 135, 216, 242, 59, 236]
["eda", "dua", "ada", "bea", "cem"]
1546998764402558742
```

`shuffle` is Fisher-Yates: each order of the items is as likely as the others.

### Not copied, not printed

A `Random` is not `Copyable`: a copy would give the same numbers again, which is rarely what was meant. It is
lent with `ref`, or moved:

```sg error
void main() {
    var r = Random.fromSeed(1)
    var copy = r
    print(r.nextInt(0, 10))
    print(copy.nextInt(0, 10))
}
```

```text
error: `r` was moved and can no longer be used
  a `Random` is not `Copyable`: a copy would give the same numbers again
  --> example.sg:4:11
 4 |     print(r.nextInt(0, 10))
   |           ^
  it was moved here:
  --> example.sg:3:16
 3 |     var copy = r
   |                ^
  hint: lend it where it is needed: `ref r`; for numbers of their own, another `Random.fromSeed(seed)` or `Random.fromSystem()`
```

Through `read` it cannot draw (`` `nextInt` takes `ref this`, but this value is reached through a `read`
borrow ``), and `print(r)` is an error, since its state is not for showing. A generator moved into a task gives
there what it would have given here, so work split over tasks stays the same from run to run when each task
gets a generator of its own seed.

A `const` cannot use one, since a const calls no function (a `static` can), and the condition of
`lock ... when` may not read random bytes.

## `SecureRandom`

The random bytes of the system: `getrandom` on Linux, `arc4random_buf` on macOS and the BSDs,
`ProcessPrng` on Windows. The system does not fail to give them; if it ever did, the program would end with a
panic rather than go on with numbers someone could guess.

| Signature | What it does |
|---|---|
| `void SecureRandom.fill(ref byte[] target)` | Every byte of `target` |
| `ulong SecureRandom.nextUlong()` | 64 random bits |
| `int SecureRandom.nextInt(int low, int high)` | A number from `low` up to `high`, without `high`, each as likely; a panic when there is none |
| `long SecureRandom.nextLong(long low, long high)` | The same for a `long` |
| `string SecureRandom.hexString(int length)` | `length` hex digits (`0`-`9`, `a`-`f`), four random bits each: 32 digits are 128 bits; a panic below 0 |

`SecureRandom` holds no state, so its functions are called on the type. What they give is different on every
run, so the program below prints only what does not change:

```sg
void main() {
    var token = SecureRandom.hexString(32)
    print(token.length())
    var key = byte[32]
    SecureRandom.fill(ref key)
    var die = SecureRandom.nextInt(1, 7)
    print(die >= 1 && die < 7)
    var r = Random.fromSystem()
    print(r.nextInt(0, 10) < 10)
    Memory.zeroSecurely(ref key)
    print(key[0])
}
```

It prints:

```text
32
true
true
0
```

`Memory.zeroSecurely(ref key)` writes zeros over the key when it is no longer needed, in a way the C compiler
keeps although nothing reads the bytes again.

A token for a link or a file name is `SecureRandom.hexString(32)`; a key is `SecureRandom.fill` into a
`byte[32]`; a password to show once is a few `SecureRandom.nextInt` draws into an alphabet. A `Random`, even
one from `Random.fromSystem()`, is never any of those.
