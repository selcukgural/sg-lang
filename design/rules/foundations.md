# The foundations

What the language is built on: the choices about safety, memory and notation that every other decision follows. Each row is a decision with its reason, in the order it was made; the choices
of each slice are in [the records by day](../decisions/README.md), and the other topics in
[the decisions by topic](README.md).

**On this page:** [C compatibility](#c-compatibility) · [Use-after-free](#use-after-free) · [Arena](#arena) · [Safety checks](#safety-checks) · [Macros](#macros) · [Notation direction](#notation-direction) · [Escape hatch](#escape-hatch) · [Language name](#language-name)

| Topic | Decision | Reason |
|---|---|---|
| <a id="c-compatibility"></a>C compatibility | Unimportant | Performance comes first |
| <a id="use-after-free"></a>Use-after-free | At compile time: single ownership and borrowing, Rust's rules | The error should appear at compile time, not in production. The problem is not the rules but Rust's notation |
| <a id="arena"></a>Arena | Not in the language | Going with Rust's solution is enough, no separate memory model is needed |
| <a id="safety-checks"></a>Safety checks | On in release too, cannot be turned off | If they could be turned off, the design would lose its reason |
| <a id="macros"></a>Macros | None | |
| <a id="notation-direction"></a>Notation direction | C#: type-first declarations, `public`, `int`, `var`, `Node { value = v }` | No semicolons, `while` without parentheses |
| <a id="escape-hatch"></a>Escape hatch | `raw T`, dereference only inside `unsafe { }`. *Built on 15 November 2026, in the sixty-ninth slice, as decided before it (["Calling C, the direction"](../directions/calling-c.md)):* a `raw T` points at plain data; `unsafe { ... }` around statements and `unsafe` in front of a value | Since `T*` is the owning pointer, the raw pointer gets a separate name |
| <a id="language-name"></a>Language name | SG. Source files end in `.sg` | The author's initials; short, and free as an extension. Decided on 12 November 2026 |
