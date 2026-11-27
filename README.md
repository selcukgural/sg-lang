<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="website/plain/assets/logo-dark.webp">
    <img src="website/plain/assets/logo-light.webp" alt="sg-lang: the letters SG with a wildcat's head over the G, in a ring of cyan light" width="420">
  </picture>
</p>

<p align="center"><strong>Small enough to know by heart. Safe and fast by construction.</strong></p>

<p align="center">
  <a href="https://sg-lang.org">Web site</a> ·
  <a href="https://sg-lang.org/learn/install">Install</a> ·
  <a href="https://sg-lang.org/learn/tour">Tour</a> ·
  <a href="https://sg-lang.org/learn/book/getting-started">Book</a> ·
  <a href="https://sg-lang.org/reference/">Reference</a> ·
  <a href="https://sg-lang.org/about/community">Community</a>
</p>

SG is a small systems language that makes fast software safe by construction: memory safe with no
collector, compiled through C with no runtime to carry. It is before its preview; nothing is
released yet and everything can still change.

```sg
int largest(read int[] values) {
    var best = values[0]
    for value in values {
        if value > best {
            best = value
        }
    }
    return best
}

void scale(ref int[] values, int by) {
    for i in 0..values.length() {
        values[i] *= by
    }
}

void main() {
    var samples = int[4] { 3, 9, 4, 1 }
    print(largest(read samples))
    scale(ref samples, 2)
    print(samples)
}
```

```text
9
[6, 18, 8, 2]
```

`read` lends the array to `largest` to look at, and `ref` lends it to `scale` to change; `main` owns
it all along. A function lent it with `read` that tries to write to it does not compile.

## Install

You need Rust to build `sg`, and a C compiler (`cc`, clang or gcc) to compile programs.

```bash
git clone https://github.com/selcukgural/sg-lang
cd sg-lang/interpreter
cargo build --release              # the command is target/release/sg
```

```bash
sg hello.sg                        # run a program
sg build hello.sg --release        # compile it through C
sg help read                       # the page of a word, in the terminal
```

Windows, the editor and everything else: [sg-lang.org](https://sg-lang.org).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT)
at your option. A program written in SG is yours, under whatever license you choose. The notices
of the logo, its font and the Unicode tables are in [brand/README.md](brand/README.md) and in the
files they cover.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion
in this work by you, as defined in the Apache-2.0 license, shall be dual licensed as above,
without any additional terms or conditions.
