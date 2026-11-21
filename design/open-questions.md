# Open questions

- Generics: closed on 12 November 2026 as "deferred", see [Deliberately deferred](deferred.md).

- Notation. The [C# variant](02-syntax-draft.md) is the current draft, and it will be stressed with example programs.
- Packages: *20 November 2026 (choices 1551, 1552):* a package is the URL of a repository and a version tag,
  pinned by the SHA-256 of its contents, with versions chosen by minimal version selection and no registry,
  as Zig's and Go's; fetched by the system's `git` (1558 in place of `git`, `curl` and `tar`). XML goes into a package of the
  language's own, not into `std` (1551 replaces 1413). The choices beneath it were made on 20 November 2026
  (1555-1564, rows ["Packages"](rules/modules.md#packages) and ["Packages, as built"](rules/modules.md#packages-as-built); the library boundary is the folder `internal`,
  1561); fetching, the lock, the cache and the commands were built in the hundred-and-thirty-eighth slice
  (1571-1588), and made safer after the second review of safety in the hundred-and-thirty-ninth (1591).
- Stack overflow on deep structures: freeing a long linked list is recursive. For types that contain themselves the compiler could turn freeing into a loop,
  and the overflow could be a readable panic. Not tested.
  *19 November 2026:* a list of one link per node is freed in a loop in compiled code (choice 13). The interpreter panics with *stack overflow* when
  writing or comparing a value nested too deep for its stack (choice 1343). Compiled code still ends at the guard page with no message, as deep
  recursion does; that is the open stack depth question.
- Inside `Func<read T, U>`, `U` cannot depend on the argument, so `orders.map(o => read o.customer)` does not compile and `.clone()` is needed. Whether this has to be overcome has not been tested.
- Type-first grammar without semicolons: the distinction between `Node* n` and `a * b`, and between `Node? x` and `a ? b : c`,
  needs lookahead. The end-of-line rule must also be settled at the interpreter stage. A `T { ... }` initialiser cannot be told apart from a block
  inside a condition (`if x == Node { ... } {`); parentheses could be required there, as in Rust.
  *12 November 2026:* the end-of-line rule, the place of `{`/`else` and the statement rule were approved and are in [the decisions by topic](rules/README.md). The `Node* n` and `Node? x`
  distinction was resolved by the statement rule. `T { ... }` inside a condition was also decided the same day (["Struct construction in a condition"](rules/statements.md#struct-construction-in-a-condition)).
  This item is closed.
- The language the compiler is written in. Today the checker, the interpreter and the C back end are written in Rust, as the
  [roadmap](03-roadmap.md) says, which counts writing the language in itself among the traps. A note to come back to, not a
  goal: after the showcase program, look again at whether the compiler should be written in SG. It would be the largest program
  in the language and show the claim on itself; since SG compiles to C, the C it writes for its own compiler could be kept in
  the repository, so building from nothing needs only a C compiler. Against it while the language still changes every few
  days: each change would break the compiler's own code. Rust's compiler moved from OCaml to Rust, Go's from C to Go, C#'s
  from C++ to C#, Zig's from C++ to Zig; Swift's is still mostly C++, and TypeScript's is moving from TypeScript to Go for
  speed. Noted on 13 November 2026.
- A read that gives what is there. `reader.readBytes(ref buffer)` fills the buffer unless the end comes first (row "Bytes piece
  by piece"). For a file that costs nothing, but on a pipe, a terminal or, later, the network the call waits until the buffer is
  full: a server could not answer a message of 100 bytes while it waits for 4 KB. When the network comes, a second read that
  gives what is ready (Rust's `read`, Go's `Read`, with a short count that is not the end) may be needed beside it; whether it
  is, and its name, are to be decided then. Noted on 14 November 2026. *For the network, settled on 15 November 2026:*
  `stream.receive(ref buffer)` gives what has come (["The network, as written"](directions/async.md#the-network-as-written)); for a pipe or a terminal it is still open.
  *Settled for every reader on 15 November 2026, before the sixty-sixth slice:* `reader.readSome(ref buffer)` ("A read that
  gives what is there").
- A variable given values that are never read: `var p = rect.corner` followed by `p.x = 0` and nothing more writes to a copy
  (a `Copyable` struct is copied), which may well be a mistake: the writer meant the corner of the rectangle. gcc warns on
  it in the C; SG allows it for now, and compiled code tells the C compiler the variable is used. Whether it becomes a
  compile error with a hint (`ref` to change the original, `_ = ` to show it is meant), as Go makes an unused variable
  one, is to be decided in a slice of its own. Noted on 15 November 2026, in the seventy-fifth slice. *Decided on 15 November
  2026, before the seventy-sixth slice:* a compile error (["A variable nothing reads"](directions/calling-c.md#a-variable-nothing-reads)). A value written over before it is
  read, or given after the last read, is not looked at yet; whether it should be is still open.
