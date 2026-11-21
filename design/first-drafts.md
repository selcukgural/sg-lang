# The first drafts

## Notation mapping

The C# direction was chosen on 5 November. The background essays (`topics/`), the memory model and the notes below are still
in the old notation:

| Old | New |
|---|---|
| `*T` | `T*` |
| `?*T` | `T*?` |
| `&T`, `&x` | `ref T`, `ref x` |
| `while cur \|n\|` | `while cur is not null` |
| `orelse` | `??` |
| `fn f(a: T) -> U` | `U f(T a)` |
| `a: T` (field) | `T a` |
| `Node { value: v }` | `Node { value = v }` |
| `i32` | `int` |

## Notes on the first drafts

Noticed on 5 November 2026, on a second reading of the first day's texts.

### Gaps in the first memory model

The memory model gaps in this section lost their subject when the generation model was dropped, and they are kept for the record.

- **An arena cannot be invalidated "in a single operation".** The model both keeps the generation in the slot header and says "when the arena
  dies all slot generations increase, a single operation". With a generation per slot, the work is proportional
  to the number of slots. For a truly single operation the generation must be at arena level, and then where the check
  looks, with the same pointer layout, has to be solved again.
- **ARM TBI does not cover 16 bits.** Solved: 16 bits and a mask were chosen, the reason is in [the decisions by topic](rules/README.md).
  The sentence "on ARM, TBI gives the mask for free" in the memory model does not hold.
- **Retired memory is not negligible.** The memory model says "the memory loss is negligible". With a 64-byte
  slot and 1 million alloc/free per second, at 16 bits that makes about 84 MB a day and about 30 GB a year
  (a calculation, not a measurement). The physical memory of a fully retired page must be given back to the operating
  system. The loss then shifts to the virtual address space, but a single long-lived object left on a page prevents
  the page from being given back.
- **If an empty page is given to another size class, generations can match by chance.** The slot boundaries shift, and an old
  pointer lands on a new slot. New generations must start above the highest old generation on the page.
- **What happens if `&` is taken to a field on the heap?** The model defines `&T` for local variables.
  `&n.value` is undefined. If this expression produces an unchecked `&T`, use-after-free comes back when the function it is
  passed down to frees the object. It looks as if a reference to a heap field has to produce a generation-checked
  `*T`.
- **2-10 percent is not this design's figure.** That measurement is for Vale's layout. Here, on every dereference,
  the slot is additionally computed from the page header and the generation is read from the header. The claim "cache behaviour is the same as
  C" also holds only for pointer size. The cost must be counted as unknown until it is measured.

### Reading list links

20 links were opened on 5 November. Two of them could not be verified by the automatic check and should be looked at in a browser:
Graydon's post (504) and the MIT Press page (403). Whether the error is permanent is not known. The information that
development of Vale has stopped comes from a secondary source and was not verified.
