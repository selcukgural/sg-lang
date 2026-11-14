//! The first back end: writes a checked program as C.
//!
//! The checker has already done the work that makes a program safe; what is left to run time
//! are the same data-dependent failures the interpreter reports, with the same messages:
//! overflow, a shift count outside the bits of the type, division by zero, a cast that does
//! not fit (from a fractional number too), a code that is no character, `assert` and `panic`.
//! A panic of a cast or of an overflow names the method that does not panic, as the
//! interpreter's does.
//!
//! Only a part of the language is compiled so far: integers, `float` and `double` (with their
//! constants, `parse`, the conversions that do not panic and the math of `<math.h>`), `bool`, `char`,
//! text (with `trim`, the searches, the iterators `chars`, `split`, `splitWhitespace` and `lines`,
//! `replaceAll` and `repeat`, `write` and `clear`, which add to a text where it is, as `f.write` does, and the width and the
//! letter after a value in `$"..."`), functions, structs,
//! enums with methods, `match` and `is` (with patterns inside patterns, `or`, ranges, `null`, `true` and
//! `false`, and on text, a `bool` and a `T?`; the names of `is` reach along `&&`, those of `is not` along `||`
//! and past an `if` that leaves), `Result` and `try`, `List`, `HashMap` and `HashSet`, arrays and
//! slices, numbers read out of bytes and written into them (`fromLittleEndian`, `toBigEndian`), function values, `T*` and `new`, `T?`, `read` and `ref`, `print` and `printError`, files
//! read and written whole or piece by piece, as text or as bytes (`StreamReader`, `StreamWriter`,
//! `File.openAppend`, and `Stdin`, `Stdout` and `Stderr` as such streams), the command line and `exit` (see `IO_RUNTIME`).
//! A type alias is the type it names. Anything else, `shared` and `weak` among them, is an error
//! that names what is missing, so nothing is compiled wrongly in silence.
//!
//! How values are represented: the integer types are fixed-size C integers (`int8_t` to `uint64_t`), and
//! `float` and `double` are C's, computed with no fused multiply-add (see `RUNTIME`); a
//! struct is a C struct passed by value; `read T` / `ref T` are pointers; a `string` is its
//! bytes, a length and a capacity, and a `read string` is a pointer and a length. A `char` is a
//! `uint32_t`, and what goes over a text is a small struct (`sg_iter`) whose parts are `read string`s
//! into the text. A `T*` is a
//! pointer to one `malloc`ed `T`. A `T*?` is the same pointer, where `NULL` is `null`: it
//! costs nothing. Any other `T?` is the value and a flag beside it. An enum is a tag and,
//! beside it, room for the largest variant (a C union); a `match` is a chain of tests on the tag.
//! An arm with patterns inside tests the tags and the values it names, joined by `&&`; an arm
//! on text is a test of the length and of the bytes (`sg_eq`). A range is two comparisons and a
//! `or` is `||`; an `or` with names is opened into one pattern for each way to match, and the way
//! that matched sets the names (`take_apart_choices`).
//! A `Result<T, E>` is such an enum with the variants `Ok(T value)` and `Error(E cause)`; `try`
//! is one test on the tag and, on an error, a `return`. A `List<T>` is a pointer to its items,
//! a length and a capacity; `xs[i]` is the item where it is, behind a check of the index, and
//! `addAll` is one test for room and a `memcpy` (the items are `Copyable`). In a checked build an
//! index or an end of a range written `a + c` is tested for overflow with the place: the sum wraps,
//! and an overflow is told only where the check of the place fails (`index_sum`, `sg_sum`).
//! A sum kept in a loop (`sum += b`) runs without its overflow test for as many rounds as it
//! has room for: the loop is written twice, and the second copy, with the test, takes over
//! (`counted_loop`, `sums.rs`).
//! An assignment computes its right side first (into a C variable when the place is more than a
//! variable), then takes the address of the place, so an item is found after the right side has
//! grown its list.
//!
//! An array `T[N]` is a C array inside a struct, so that it is passed, returned and copied by
//! value like any other value: it lies where its variable or its field lies, and nothing is
//! allocated for it. A slice (`read T[]`, `ref T[]`) is a window: a pointer to the first item
//! and a count, whatever the items are kept in. A range `xs[a..b]` checks its two ends once and
//! gives such a window; an index into a window is checked against the count. `sort` and
//! `reverse` on a window move the items where they are, with the list's merge sort.
//!
//! Writing and comparing: for each struct, enum and list that is written or compared, one
//! helper is made that writes it into a text (`sg_show_...`) and one that tells whether two
//! are equal (`sg_same_...`). They call the `format`, `equals` and `compare` a struct or an
//! enum wrote.
//!
//! Borrows inside values: a field that borrows is the same pointer a borrowed variable is,
//! and a function value is the address of the function.
//!
//! Hash tables: a `HashMap` and a `HashSet` keep their entries one after the other and, beside
//! them, the slots that find an entry from the hash of its key (see `TABLE`). The hash has a
//! key drawn when the program starts.
//!
//! Who frees: the checker has decided who owns each value, so the back end only follows it.
//! An owned value is dropped where its variable ends. A use that moves the value (marked by
//! the checker) empties the place it came from, so the later drop finds nothing to free.
//!
//! Order: `try`, `??` and `replace` are written as GNU statement expressions (clang and gcc
//! both have them), so they run where they stand, between what is written before and after.
//!
//! Leaving from inside an expression: `try` and `?? return` can leave the function in the
//! middle of a statement. They are compiled where nothing that would have to be freed has been
//! computed before them in that statement (see `Emitter::head`); elsewhere they are an error
//! that asks for a name.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use crate::ast::*;
use crate::diag::{Diag, Span};
use crate::source::Sources;
use crate::sums;
use crate::types::{FloatTy, IntTy, TextIter, Ty};

type Res<T> = Result<T, Diag>;

/// What every compiled program starts with: the checks, written once.
const RUNTIME: &str = r#"#include <math.h>
#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* The generated code puts every operation in parentheses. */
#if defined(__clang__)
#pragma clang diagnostic ignored "-Wparentheses-equality"
#pragma clang diagnostic ignored "-Wgnu-statement-expression"
#endif

/* A fractional operation is rounded on its own, as the interpreter does it: `a * b + c` is
   never one fused instruction. gcc does not read this line; `sg build` passes
   `-ffp-contract=off` for it. */
#if defined(__clang__)
#pragma STDC FP_CONTRACT OFF
#endif

/* A panic is never the usual path: its code is kept out of the way of the code that runs. */
#define SG_COLD __attribute__((cold, noinline))

SG_COLD static _Noreturn void sg_die(const char* loc) {
    fputs(loc, stderr);
    exit(1);
}
/* A failing operation hands its numbers over as `__int128`, which holds a value of every
   integer type, signed or not; they are written in full. */
SG_COLD static void sg_err_int(__int128 v) {
    if (v < 0) fprintf(stderr, "%lld", (long long)v);
    else fprintf(stderr, "%llu", (unsigned long long)v);
}
/* The way out that does not panic, written under the place as the interpreter writes it. */
SG_COLD static _Noreturn void sg_die_hint(const char* loc, const char* hint) {
    fputs(loc, stderr);
    fprintf(stderr, "  hint: %s\n", hint);
    exit(1);
}
SG_COLD static _Noreturn void sg_overflow(__int128 a, const char* op, __int128 b, const char* ty, const char* loc) {
    fflush(stdout);
    fputs("panic: overflow: ", stderr);
    sg_err_int(a);
    fprintf(stderr, " %s ", op);
    sg_err_int(b);
    fprintf(stderr, " does not fit in %s\n", ty);
    switch (op[0]) {
    case '+': sg_die_hint(loc, "`tryAdd` gives `null` instead, and `wrappingAdd` wraps around");
    case '-': sg_die_hint(loc, "`trySubtract` gives `null` instead, and `wrappingSubtract` wraps around");
    case '*': sg_die_hint(loc, "`tryMultiply` gives `null` instead, and `wrappingMultiply` wraps around");
    default: sg_die_hint(loc, "`tryDivide` gives `null` instead");
    }
}
SG_COLD static _Noreturn void sg_neg_overflow(__int128 a, const char* ty, const char* loc) {
    fflush(stdout);
    fputs("panic: overflow: -(", stderr);
    sg_err_int(a);
    fprintf(stderr, ") does not fit in %s\n", ty);
    sg_die(loc);
}
SG_COLD static _Noreturn void sg_shift_fail(__int128 n, const char* ty, int bits, const char* loc) {
    fflush(stdout);
    fputs("panic: shift by ", stderr);
    sg_err_int(n);
    fprintf(stderr, " is out of range: %s has %d bits\n", ty, bits);
    sg_die(loc);
}
SG_COLD static _Noreturn void sg_div_zero(__int128 a, const char* op, const char* loc) {
    fflush(stdout);
    fputs("panic: division by zero: ", stderr);
    sg_err_int(a);
    fprintf(stderr, " %s 0\n", op);
    sg_die(loc);
}
SG_COLD static _Noreturn void sg_cast_fail(__int128 v, const char* ty, const char* loc) {
    fflush(stdout);
    fputs("panic: value ", stderr);
    sg_err_int(v);
    fprintf(stderr, " does not fit in %s\n", ty);
    fputs(loc, stderr);
    fprintf(stderr, "  hint: `tryTo%c%s()` gives `null` when the value does not fit, and `wrappingTo%c%s()` cuts it to size\n",
            ty[0] - 'a' + 'A', ty + 1, ty[0] - 'a' + 'A', ty + 1);
    exit(1);
}
SG_COLD static _Noreturn void sg_panic(const char* text, const char* loc) {
    fflush(stdout);
    fprintf(stderr, "panic: %s\n", text);
    sg_die(loc);
}
SG_COLD static _Noreturn void sg_assert_fail(const char* text, const char* loc) {
    fflush(stdout);
    fprintf(stderr, "panic: assert failed: %s\n", text);
    sg_die(loc);
}
SG_COLD static _Noreturn void sg_assert_fail_i(const char* text, __int128 l, __int128 r, const char* loc) {
    fflush(stdout);
    fprintf(stderr, "panic: assert failed: %s\n  left: ", text);
    sg_err_int(l);
    fputs("\n  right: ", stderr);
    sg_err_int(r);
    fputs("\n", stderr);
    sg_die(loc);
}
SG_COLD static _Noreturn void sg_assert_fail_b(const char* text, bool l, bool r, const char* loc) {
    fflush(stdout);
    fprintf(stderr, "panic: assert failed: %s\n  left: %s\n  right: %s\n", text, l ? "true" : "false", r ? "true" : "false");
    sg_die(loc);
}

/* A fractional number as text: the fewest digits that read back as the same number (as a
   `float` when `single`), laid out as JavaScript does it: `0.1`, `100`, `1.5e+21`, `1e-7`.
   Writes at most 32 bytes into `out` and gives how many. */
static int sg_shortest(char* out, double v, bool single) {
    if (v != v) { memcpy(out, "NaN", 3); return 3; }
    if (isinf(v)) { if (v > 0) { memcpy(out, "Infinity", 8); return 8; } memcpy(out, "-Infinity", 9); return 9; }
    if (v == 0) { if (signbit(v)) { memcpy(out, "-0", 2); return 2; } out[0] = '0'; return 1; }
    char written[40];
    int most = single ? 9 : 17;
    for (int p = 1; p <= most; p++) {
        snprintf(written, sizeof written, "%.*e", p - 1, v);
        if (single ? strtof(written, NULL) == (float)v : strtod(written, NULL) == v) break;
    }
    bool negative = written[0] == '-';
    const char* at = written + negative;
    char digits[24];
    int count = 0, n = 0;
    while (*at != 'e') { if (*at != '.') digits[count++] = *at; at++; }
    int point = atoi(at + 1) + 1;
    while (count > 1 && digits[count - 1] == '0') count--;
    if (negative) out[n++] = '-';
    if (count <= point && point <= 21) {
        memcpy(out + n, digits, (size_t)count); n += count;
        for (int i = count; i < point; i++) out[n++] = '0';
    } else if (0 < point && point <= 21) {
        memcpy(out + n, digits, (size_t)point); n += point;
        out[n++] = '.';
        memcpy(out + n, digits + point, (size_t)(count - point)); n += count - point;
    } else if (-6 < point && point <= 0) {
        out[n++] = '0'; out[n++] = '.';
        for (int i = 0; i < -point; i++) out[n++] = '0';
        memcpy(out + n, digits, (size_t)count); n += count;
    } else {
        out[n++] = digits[0];
        if (count > 1) { out[n++] = '.'; memcpy(out + n, digits + 1, (size_t)(count - 1)); n += count - 1; }
        n += snprintf(out + n, 8, "e%c%d", point - 1 < 0 ? '-' : '+', abs(point - 1));
    }
    return n;
}
SG_COLD static void sg_err_float(double v, bool single) {
    char text[40];
    int n = sg_shortest(text, v, single);
    fwrite(text, 1, (size_t)n, stderr);
}
SG_COLD static _Noreturn void sg_fcast_fail(double v, bool single, const char* ty, const char* loc) {
    fflush(stdout);
    fputs("panic: value ", stderr);
    sg_err_float(v, single);
    fprintf(stderr, " does not fit in %s\n", ty);
    fputs(loc, stderr);
    fprintf(stderr, "  hint: `tryTo%c%s()` gives `null` when the value does not fit\n", ty[0] - 'a' + 'A', ty + 1);
    exit(1);
}
SG_COLD static _Noreturn void sg_assert_fail_fd(const char* text, double l, double r, bool single, const char* loc) {
    fflush(stdout);
    fprintf(stderr, "panic: assert failed: %s\n  left: ", text);
    sg_err_float(l, single);
    fputs("\n  right: ", stderr);
    sg_err_float(r, single);
    fputs("\n", stderr);
    sg_die(loc);
}
SG_COLD static _Noreturn void sg_assert_fail_d(const char* text, double l, double r, const char* loc) { sg_assert_fail_fd(text, l, r, false, loc); }
SG_COLD static _Noreturn void sg_assert_fail_f(const char* text, double l, double r, const char* loc) { sg_assert_fail_fd(text, l, r, true, loc); }
/* `equals` and `compare` on fractional numbers: a total order where `NaN` equals itself and
   comes first, and `-0.0` equals `0.0`. A `float` comes in as the `double` that holds it. */
static inline bool sg_feq(double a, double b) { return a == b || (a != a && b != b); }
static inline int32_t sg_fcmp(double a, double b) {
    if (a != a) return b != b ? 0 : -1;
    if (b != b) return 1;
    return (a > b) - (a < b);
}
/* `min` and `max` on fractional numbers, as C# has them: `NaN` if either side is one, and
   `-0.0` is less than `0.0`. C's `fmin` gives the other side for a `NaN`, and either zero. */
static inline double sg_fmin(double a, double b) {
    if (a != a || b != b) return NAN;
    if (a == b) return signbit(a) ? a : b;
    return a < b ? a : b;
}
static inline double sg_fmax(double a, double b) {
    if (a != a || b != b) return NAN;
    if (a == b) return signbit(a) ? b : a;
    return a > b ? a : b;
}
/* `int.parse` and the other integer types (`parseHex` with `base` 16): -1 when the text is a number between `lo` and
   `hi`, -2 when it is a number past them, otherwise where it stops being a number (its length
   when it ends too early). The same rules as `parse_number` in the interpreter. */
static int sg_digit(uint8_t c, int base) {
    int d = c >= '0' && c <= '9' ? c - '0' : (c | 0x20) >= 'a' && (c | 0x20) <= 'z' ? (c | 0x20) - 'a' + 10 : 99;
    return d < base ? d : -1;
}
static int64_t sg_read_whole(const uint8_t* p, int64_t n, int base, __int128 lo, __int128 hi, __int128* out) {
    int64_t start = n > 0 && (p[0] == '-' || p[0] == '+');
    if (start && p[0] == '-' && lo == 0) return 0;
    int64_t at = start;
    while (at < n && sg_digit(p[at], base) >= 0) at++;
    if (at == start || at < n) return at;
    __int128 v = 0;
    for (int64_t i = start; i < n; i++) {
        v = v * base + sg_digit(p[i], base);
        if (v > ((__int128)1 << 64)) return -2;
    }
    if (p[0] == '-') v = -v;
    if (v < lo || v > hi) return -2;
    *out = v;
    return -1;
}
/* `double.parse` and `float.parse`, with the same answers. What `print` writes is read back:
   `NaN`, `Infinity`, `-Infinity`. A number of few digits and a small exponent, as most are, is
   its digits times or over a power of ten, both held exactly, so one operation rounds it
   correctly (Clinger's fast path; Rust takes it too). Any other goes to the C library, which
   rounds to the nearest number as Rust does, and gives an infinity past the largest; the
   program sets no locale, so the point is a point. */
static int64_t sg_read_fraction(const uint8_t* p, int64_t n, bool single, double* out) {
    static const double tens[] = { 1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10, 1e11,
                                   1e12, 1e13, 1e14, 1e15, 1e16, 1e17, 1e18, 1e19, 1e20, 1e21, 1e22 };
    int64_t start = n > 0 && (p[0] == '-' || p[0] == '+');
    if (!start && n == 3 && memcmp(p, "NaN", 3) == 0) { *out = NAN; return -1; }
    if (n - start == 8 && memcmp(p + start, "Infinity", 8) == 0) { *out = start && p[0] == '-' ? -INFINITY : INFINITY; return -1; }
    int64_t at = start, digits = 0, kept = 0, scale = 0;
    uint64_t m = 0;
    while (at < n && p[at] >= '0' && p[at] <= '9') {
        if (kept < 19) { m = m * 10 + (uint64_t)(p[at] - '0'); kept += m != 0; } else scale++;
        at++; digits++;
    }
    if (at < n && p[at] == '.') {
        at++;
        while (at < n && p[at] >= '0' && p[at] <= '9') {
            if (kept < 19) { m = m * 10 + (uint64_t)(p[at] - '0'); kept += m != 0; scale--; }
            at++; digits++;
        }
    }
    if (digits == 0) return at;
    int64_t exponent = 0;
    if (at < n && (p[at] == 'e' || p[at] == 'E')) {
        at++;
        bool below = at < n && p[at] == '-';
        if (at < n && (p[at] == '-' || p[at] == '+')) at++;
        int64_t from = at;
        while (at < n && p[at] >= '0' && p[at] <= '9') { if (exponent < 100000) exponent = exponent * 10 + (p[at] - '0'); at++; }
        if (at == from) return at;
        if (below) exponent = -exponent;
    }
    if (at < n) return at;
    int64_t e = scale + exponent;
    bool negative = p[0] == '-';
    if (kept < 19 && (single ? m <= (1u << 24) && e >= -10 && e <= 10 : m <= ((uint64_t)1 << 53) && e >= -22 && e <= 22)) {
        if (single) {
            float v = (float)m;
            v = e < 0 ? v / (float)tens[-e] : v * (float)tens[e];
            *out = negative ? -(double)v : (double)v;
        } else {
            double v = (double)m;
            v = e < 0 ? v / tens[-e] : v * tens[e];
            *out = negative ? -v : v;
        }
        return -1;
    }
    char small[64];
    char* text = n < 64 ? small : malloc((size_t)n + 1);
    if (!text) abort();
    memcpy(text, p, (size_t)n);
    text[n] = 0;
    double v = single ? (double)strtof(text, NULL) : strtod(text, NULL);
    if (text != small) free(text);
    *out = v;
    return -1;
}
/* The whole numbers a type holds, as the powers of two at its ends: `lo <= t < hi`. */
#define SG_LO(T) ((T)-1 < 0 ? -__builtin_ldexp(1.0, (int)sizeof(T) * 8 - 1) : 0.0)
#define SG_HI(T) ((T)-1 < 0 ? __builtin_ldexp(1.0, (int)sizeof(T) * 8 - 1) : __builtin_ldexp(1.0, (int)sizeof(T) * 8))
#define SG_MAX(T) ((T)-1 < 0 ? (T)((((uint64_t)1) << (sizeof(T) * 8 - 1)) - 1) : (T)-1)

/* One set of checked operations for each integer type. Overflow is caught by the compiler's
   own overflow builtins: one branch that is never taken in a correct program. `sg_steps`:
   how many rounds of a loop a sum can take when one round adds between `lo` and `hi`, before
   it could overflow; `sg_upto`: the counter of a loop that many rounds on, or the most the
   type holds (see `sums.rs`). */
static inline __int128 sg_fewer(__int128 a, __int128 b) { return a < b ? a : b; }
#define SG_INT(T, N, NAME) \
static inline T sg_add_##N(T a, T b, const char* loc) { T r; if (__builtin_expect(__builtin_add_overflow(a, b, &r), 0)) sg_overflow(a, "+", b, NAME, loc); return r; } \
static inline T sg_sub_##N(T a, T b, const char* loc) { T r; if (__builtin_expect(__builtin_sub_overflow(a, b, &r), 0)) sg_overflow(a, "-", b, NAME, loc); return r; } \
static inline T sg_mul_##N(T a, T b, const char* loc) { T r; if (__builtin_expect(__builtin_mul_overflow(a, b, &r), 0)) sg_overflow(a, "*", b, NAME, loc); return r; } \
static inline T sg_div_##N(T a, T b, const char* loc) { \
    if (__builtin_expect(b == 0, 0)) sg_div_zero(a, "/", loc); \
    if ((T)-1 < 0 && b == (T)-1) { T r; if (__builtin_sub_overflow((T)0, a, &r)) sg_overflow(a, "/", b, NAME, loc); return r; } \
    return a / b; } \
static inline T sg_rem_##N(T a, T b, const char* loc) { \
    if (__builtin_expect(b == 0, 0)) sg_div_zero(a, "%", loc); \
    if ((T)-1 < 0 && b == (T)-1) return 0; \
    return a % b; } \
static inline T sg_neg_##N(T a, const char* loc) { T r; if (__builtin_expect(__builtin_sub_overflow((T)0, a, &r), 0)) sg_neg_overflow(a, NAME, loc); return r; } \
static inline T sg_cast_##N(__int128 v, const char* loc) { T r = (T)v; if (__builtin_expect((__int128)r != v, 0)) sg_cast_fail(v, NAME, loc); return r; } \
static inline T sg_fcast_##N(double v, bool single, const char* loc) { double t = __builtin_trunc(v); if (__builtin_expect(!(t >= SG_LO(T) && t < SG_HI(T)), 0)) sg_fcast_fail(v, single, NAME, loc); return (T)t; } \
static inline T sg_sat_##N(double v) { double t = __builtin_trunc(v); if (t >= SG_LO(T) && t < SG_HI(T)) return (T)t; if (v != v) return 0; return v < 0 ? (T)SG_LO(T) : SG_MAX(T); } \
static inline T sg_wadd_##N(T a, T b) { return (T)((uint64_t)a + (uint64_t)b); } \
static inline T sg_wsub_##N(T a, T b) { return (T)((uint64_t)a - (uint64_t)b); } \
static inline T sg_wmul_##N(T a, T b) { return (T)((uint64_t)a * (uint64_t)b); } \
static inline T sg_shl_##N(T a, __int128 n, const char* loc) { if (__builtin_expect(n < 0 || n >= (__int128)sizeof(T) * 8, 0)) sg_shift_fail(n, NAME, (int)sizeof(T) * 8, loc); return (T)((uint64_t)a << n); } \
static inline T sg_shr_##N(T a, __int128 n, const char* loc) { if (__builtin_expect(n < 0 || n >= (__int128)sizeof(T) * 8, 0)) sg_shift_fail(n, NAME, (int)sizeof(T) * 8, loc); return (T)(a >> n); } \
static inline T sg_wshl_##N(T a, uint64_t n) { return (T)((uint64_t)a << (n & (sizeof(T) * 8 - 1))); } \
static inline T sg_wshr_##N(T a, uint64_t n) { return (T)(a >> (n & (sizeof(T) * 8 - 1))); } \
static inline __int128 sg_steps_##N(T sum, int64_t lo, int64_t hi) { \
    __int128 room = (__int128)1 << 100; \
    if (hi > 0) room = ((__int128)SG_MAX(T) - sum) / hi; \
    if (lo < 0) { __int128 down = ((__int128)sum - ((T)-1 < 0 ? -(__int128)SG_MAX(T) - 1 : 0)) / -lo; if (down < room) room = down; } \
    return room; } \
static inline T sg_upto_##N(T at, __int128 rounds) { return (__int128)at + rounds > (__int128)SG_MAX(T) ? SG_MAX(T) : (T)(at + rounds); }

SG_INT(int8_t, sbyte, "sbyte")
SG_INT(uint8_t, byte, "byte")
SG_INT(int16_t, short, "short")
SG_INT(uint16_t, ushort, "ushort")
SG_INT(int32_t, int, "int")
SG_INT(uint32_t, uint, "uint")
SG_INT(int64_t, long, "long")
SG_INT(uint64_t, ulong, "ulong")
SG_INT(int64_t, nint, "nint")
SG_INT(uint64_t, nuint, "nuint")

/* Text. A `read string` is a window: where the bytes are and how many. A `string` owns its
   bytes; capacity 0 means they are a constant inside the program, or were moved away, and
   there is nothing to free. */
typedef struct { const uint8_t* ptr; int64_t len; } sg_str;
typedef struct { uint8_t* ptr; int64_t len; int64_t cap; } sg_string;
#define SG_LIT(text, n) ((sg_string){ (uint8_t*)(text), (n), 0 })
#define SG_STR(text, n) ((sg_str){ (const uint8_t*)(text), (n) })
/* A `read string?` with nothing in it: a window of length -1. */
#define SG_NO_STR ((sg_str){ NULL, -1 })
static inline sg_str sg_view(const sg_string* s) { return (sg_str){ s->ptr, s->len }; }
static inline void sg_drop_string(sg_string* s) { if (s->cap > 0) free(s->ptr); }
static inline sg_string sg_move_string(sg_string* s) { sg_string v = *s; s->cap = 0; return v; }
static sg_string sg_clone(sg_str s) {
    if (s.len == 0) return SG_LIT("", 0);
    uint8_t* bytes = malloc((size_t)s.len);
    if (!bytes) abort();
    memcpy(bytes, s.ptr, (size_t)s.len);
    return (sg_string){ bytes, s.len, s.len };
}
static inline bool sg_eq(sg_str a, sg_str b) { return a.len == b.len && (a.len == 0 || memcmp(a.ptr, b.ptr, (size_t)a.len) == 0); }
static inline int sg_cmp(sg_str a, sg_str b) {
    int64_t n = a.len < b.len ? a.len : b.len;
    int c = n ? memcmp(a.ptr, b.ptr, (size_t)n) : 0;
    return c ? c : (a.len > b.len) - (a.len < b.len);
}
SG_COLD static _Noreturn void sg_index_fail(long long at, long long len, const char* loc) {
    fflush(stdout);
    fprintf(stderr, "panic: index %lld is out of range: the length is %lld\n", at, len);
    sg_die(loc);
}
SG_COLD static _Noreturn void sg_range_fail(sg_str s, long long from, long long to, const char* loc) {
    fflush(stdout);
    if (from < 0 || from > s.len || to < 0 || to > s.len) {
        fprintf(stderr, "panic: range %lld..%lld is out of range: the length is %lld\n", from, to, (long long)s.len);
    } else if (from > to) {
        fprintf(stderr, "panic: range %lld..%lld ends before it starts\n", from, to);
    } else {
        /* One end is inside a character: find where that character starts and ends. */
        long long cut = (from < s.len && (s.ptr[from] & 0xC0) == 0x80) ? from : to;
        long long first = cut;
        while (first > 0 && (s.ptr[first] & 0xC0) == 0x80) first--;
        long long last = first + 1;
        while (last < s.len && (s.ptr[last] & 0xC0) == 0x80) last++;
        fprintf(stderr, "panic: range %lld..%lld cuts a character in half\n  `%.*s` takes bytes %lld..%lld; a `string` is always valid UTF-8\n",
                from, to, (int)(last - first), (const char*)s.ptr + first, first, last);
    }
    sg_die(loc);
}
/* A byte of the text; the index is checked in every build, because this is what keeps memory safe. */
static inline uint8_t sg_at(sg_str s, int64_t at, const char* loc) {
    if (__builtin_expect(at < 0 || at >= s.len, 0)) sg_index_fail(at, s.len, loc);
    return s.ptr[at];
}
static inline sg_str sg_slice(sg_str s, int64_t from, int64_t to, const char* loc) {
    if (__builtin_expect(from < 0 || to > s.len || from > to
            || (from < s.len && (s.ptr[from] & 0xC0) == 0x80) || (to < s.len && (s.ptr[to] & 0xC0) == 0x80), 0))
        sg_range_fail(s, from, to, loc);
    return (sg_str){ s.ptr + from, to - from };
}
/* A range of items: its ends are checked in every build, like an index. */
SG_COLD static _Noreturn void sg_span_fail(long long from, long long to, long long len, const char* loc) {
    fflush(stdout);
    if (from < 0 || from > len || to < 0 || to > len) fprintf(stderr, "panic: range %lld..%lld is out of range: the length is %lld\n", from, to, len);
    else fprintf(stderr, "panic: range %lld..%lld ends before it starts\n", from, to);
    sg_die(loc);
}
/* An index or an end of a range written `a + c`, with `c` a constant that is not negative and
   the sum of a signed type, in a checked build. The sum is not tested on its own: it wraps, a
   sum that overflowed is negative, and the check of the place refuses it. Only there, on the
   way out, is the overflow found and told as the test of the sum would have told it: `a` is
   what the wrapped sum less `c` is in the type of the sum. One test where there were two, and
   the program stops where and as it stopped before. */
typedef struct { long long c; const char* ty; int bits; int first; const char* loc; } sg_sum;
SG_COLD static void sg_sum_told(long long at, const sg_sum* s) {
    if (!s || at >= 0) return;
    int drop = 64 - s->bits;
    long long a = (long long)(((unsigned long long)at - (unsigned long long)s->c) << drop) >> drop;
    if (a < 0) return;
    if (s->first) sg_overflow(s->c, "+", a, s->ty, s->loc);
    sg_overflow(a, "+", s->c, s->ty, s->loc);
}
SG_COLD static _Noreturn void sg_index_sum_fail(long long at, long long len, const sg_sum* s, const char* loc) {
    sg_sum_told(at, s);
    sg_index_fail(at, len, loc);
}
SG_COLD static _Noreturn void sg_span_sum_fail(long long from, long long to, long long len, const sg_sum* sf, const sg_sum* st, const char* loc) {
    sg_sum_told(from, sf);
    sg_sum_told(to, st);
    sg_span_fail(from, to, len, loc);
}
static inline uint8_t sg_ats(sg_str s, int64_t at, const sg_sum* sum, const char* loc) {
    if (__builtin_expect((uint64_t)at >= (uint64_t)s.len, 0)) sg_index_sum_fail(at, s.len, sum, loc);
    return s.ptr[at];
}
static inline sg_str sg_slices(sg_str s, int64_t from, int64_t to, const sg_sum* sf, const sg_sum* st, const char* loc) {
    if (__builtin_expect(from < 0 || to > s.len || from > to
            || (from < s.len && (s.ptr[from] & 0xC0) == 0x80) || (to < s.len && (s.ptr[to] & 0xC0) == 0x80), 0)) {
        sg_sum_told(from, sf);
        sg_sum_told(to, st);
        sg_range_fail(s, from, to, loc);
    }
    return (sg_str){ s.ptr + from, to - from };
}
/* A number kept in bytes, the lowest byte first (little-endian) or the highest first
   (big-endian): one load or one store, turned around where the machine keeps its bytes the
   other way. */
#if __BYTE_ORDER__ == __ORDER_BIG_ENDIAN__
#define SG_LE16(v) __builtin_bswap16(v)
#define SG_LE32(v) __builtin_bswap32(v)
#define SG_LE64(v) __builtin_bswap64(v)
#define SG_BE16(v) (v)
#define SG_BE32(v) (v)
#define SG_BE64(v) (v)
#else
#define SG_LE16(v) (v)
#define SG_LE32(v) (v)
#define SG_LE64(v) (v)
#define SG_BE16(v) __builtin_bswap16(v)
#define SG_BE32(v) __builtin_bswap32(v)
#define SG_BE64(v) __builtin_bswap64(v)
#endif
SG_COLD static _Noreturn void sg_bytes_fail(const char* what, int64_t want, int64_t len, const char* loc) {
    fflush(stdout);
    fprintf(stderr, "panic: `%s` reads %lld bytes, and it was given %lld\n", what, (long long)want, (long long)len);
    sg_die(loc);
}
/* Where the first byte lies that is not part of valid UTF-8, or -1 when the bytes are text. */
static int64_t sg_utf8_bad(const uint8_t* bytes, int64_t len) {
    int64_t at = 0;
    while (at < len) {
        /* Eight bytes at a time while none of them has its high bit set: plain ASCII. */
        if (at + 8 <= len) {
            uint64_t word;
            memcpy(&word, bytes + at, 8);
            if (!(word & 0x8080808080808080ULL)) { at += 8; continue; }
        }
        uint8_t first = bytes[at];
        if (first < 0x80) { at++; continue; }
        int64_t more;
        uint8_t low = 0x80, high = 0xBF;
        if (first >= 0xC2 && first <= 0xDF) more = 1;
        else if (first >= 0xE0 && first <= 0xEF) { more = 2; if (first == 0xE0) low = 0xA0; if (first == 0xED) high = 0x9F; }
        else if (first >= 0xF0 && first <= 0xF4) { more = 3; if (first == 0xF0) low = 0x90; if (first == 0xF4) high = 0x8F; }
        else return at;
        if (at + more >= len) {
            /* The character is cut off by the end: it is still bad from its first byte on. */
            return at;
        }
        if (bytes[at + 1] < low || bytes[at + 1] > high) return at;
        for (int64_t i = 2; i <= more; i++) if ((bytes[at + i] & 0xC0) != 0x80) return at;
        at += more + 1;
    }
    return -1;
}
/* A short text is written once, into a buffer on the stack, and copied; only a longer one
   is written a second time, into the room it was found to need. */
static sg_string sg_format(const char* format, ...) {
    char small[128];
    va_list count, write;
    va_start(count, format);
    va_copy(write, count);
    int n = vsnprintf(small, sizeof small, format, count);
    va_end(count);
    if (n <= 0) { va_end(write); return SG_LIT("", 0); }
    uint8_t* bytes = malloc((size_t)n + 1);
    if (!bytes) abort();
    if (n < (int)sizeof small) memcpy(bytes, small, (size_t)n);
    else vsnprintf((char*)bytes, (size_t)n + 1, format, write);
    va_end(write);
    return (sg_string){ bytes, n, n };
}
SG_COLD static _Noreturn void sg_panicf(const char* loc, const char* format, ...);
/* Set by the program of tests: there `exit` fails the test, as in the interpreter. */
static bool sg_in_tests = false;
/* `exit(code)`: what was printed goes out, and nothing is freed: the system takes it all. */
static _Noreturn void sg_exit(uint8_t code, const char* loc) {
    if (sg_in_tests) sg_panicf(loc, "`exit(%d)` in a test: a test cannot end the program", (int)code);
    fflush(stdout);
    exit(code);
}
SG_COLD static _Noreturn void sg_panicf(const char* loc, const char* format, ...) {
    va_list args;
    va_start(args, format);
    fflush(stdout);
    fputs("panic: ", stderr);
    vfprintf(stderr, format, args);
    va_end(args);
    fputc('\n', stderr);
    sg_die(loc);
}
/* Hash tables. The hash is SipHash-1-3 with a key drawn when the program starts, so that
   nobody who chooses the keys can make them all land in one place. */
static uint64_t sg_seed[2];
static void sg_seed_init(void) {
#if defined(__APPLE__) || defined(__FreeBSD__) || defined(__OpenBSD__) || defined(__NetBSD__)
    arc4random_buf(sg_seed, sizeof sg_seed);
#else
    FILE* source = fopen("/dev/urandom", "rb");
    if (!source || fread(sg_seed, sizeof sg_seed, 1, source) != 1) abort();
    fclose(source);
#endif
}
#define SG_ROTL(x, b) (((x) << (b)) | ((x) >> (64 - (b))))
#define SG_SIPROUND \
    v0 += v1; v1 = SG_ROTL(v1, 13); v1 ^= v0; v0 = SG_ROTL(v0, 32); \
    v2 += v3; v3 = SG_ROTL(v3, 16); v3 ^= v2; \
    v0 += v3; v3 = SG_ROTL(v3, 21); v3 ^= v0; \
    v2 += v1; v1 = SG_ROTL(v1, 17); v1 ^= v2; v2 = SG_ROTL(v2, 32);
/* `h`: the hash of what came before, for a key made of several parts. */
static uint64_t sg_hash_bytes(uint64_t h, const uint8_t* data, int64_t len) {
    uint64_t k0 = sg_seed[0] ^ h, k1 = sg_seed[1] ^ SG_ROTL(h, 32);
    uint64_t v0 = k0 ^ 0x736f6d6570736575ULL, v1 = k1 ^ 0x646f72616e646f6dULL, v2 = k0 ^ 0x6c7967656e657261ULL, v3 = k1 ^ 0x7465646279746573ULL;
    int64_t whole = len & ~(int64_t)7;
    for (int64_t i = 0; i < whole; i += 8) {
        uint64_t m;
        memcpy(&m, data + i, 8);
        v3 ^= m; SG_SIPROUND v0 ^= m;
    }
    uint64_t last = (uint64_t)len << 56;
    for (int64_t i = whole; i < len; i++) last |= (uint64_t)data[i] << (8 * (i - whole));
    v3 ^= last; SG_SIPROUND v0 ^= last;
    v2 ^= 0xff;
    SG_SIPROUND SG_SIPROUND SG_SIPROUND
    return v0 ^ v1 ^ v2 ^ v3;
}
static inline uint64_t sg_hash_word(uint64_t h, uint64_t word) {
    uint64_t k0 = sg_seed[0] ^ h, k1 = sg_seed[1] ^ SG_ROTL(h, 32);
    uint64_t v0 = k0 ^ 0x736f6d6570736575ULL, v1 = k1 ^ 0x646f72616e646f6dULL, v2 = k0 ^ 0x6c7967656e657261ULL, v3 = k1 ^ 0x7465646279746573ULL;
    v3 ^= word; SG_SIPROUND v0 ^= word;
    uint64_t last = (uint64_t)8 << 56;
    v3 ^= last; SG_SIPROUND v0 ^= last;
    v2 ^= 0xff;
    SG_SIPROUND SG_SIPROUND SG_SIPROUND
    return v0 ^ v1 ^ v2 ^ v3;
}
/* A list that is full gets room for twice as many items. */
static void* sg_grow(void* items, int64_t* cap, size_t size) {
    int64_t room = *cap ? *cap * 2 : 4;
    void* grown = realloc(items, (size_t)room * size);
    if (!grown) abort();
    *cap = room;
    return grown;
}
/* Room for at least `need` items: twice as many as there was room for, or `need` if that is
   more, so that adding many items one call at a time still grows the block only now and then. */
static void* sg_reserve(void* items, int64_t* cap, int64_t need, size_t size) {
    int64_t room = *cap ? *cap * 2 : 4;
    if (room < need) room = need;
    if ((uint64_t)room > PTRDIFF_MAX / size) abort();
    void* grown = realloc(items, (size_t)room * size);
    if (!grown) abort();
    *cap = room;
    return grown;
}
static sg_string sg_join(sg_str separator, const sg_string* items, int64_t count) {
    int64_t len = 0;
    for (int64_t i = 0; i < count; i++) len += items[i].len + (i ? separator.len : 0);
    if (len == 0) return SG_LIT("", 0);
    uint8_t* bytes = malloc((size_t)len);
    if (!bytes) abort();
    uint8_t* at = bytes;
    for (int64_t i = 0; i < count; i++) {
        if (i && separator.len) { memcpy(at, separator.ptr, (size_t)separator.len); at += separator.len; }
        if (items[i].len) { memcpy(at, items[i].ptr, (size_t)items[i].len); at += items[i].len; }
    }
    return (sg_string){ bytes, len, len };
}
/* Text that is put together piece by piece: what a `Formatter` writes into. Room for `count`
   more bytes: the block doubles (at least 32 bytes), and a text with no block of its own (a
   constant in the program) is copied out first. */
static void sg_room(sg_string* out, int64_t count) {
    if (out->len + count <= out->cap) return;
    int64_t room = out->cap ? out->cap * 2 : 32;
    while (room < out->len + count) room *= 2;
    uint8_t* grown = out->cap ? realloc(out->ptr, (size_t)room) : malloc((size_t)room);
    if (!grown) abort();
    if (!out->cap && out->len) memcpy(grown, out->ptr, (size_t)out->len);
    out->ptr = grown;
    out->cap = room;
}
static void sg_put(sg_string* out, const void* bytes, int64_t count) {
    if (count <= 0) return;
    sg_room(out, count);
    memcpy(out->ptr + out->len, bytes, (size_t)count);
    out->len += count;
}
/* `string.withCapacity(n)`: an empty text with room for `n` bytes; none is taken for 0. */
SG_COLD static _Noreturn void sg_capacity_bad(int64_t n, const char* loc) {
    fflush(stdout);
    if (n < 0) fprintf(stderr, "panic: the capacity of a text cannot be negative: %lld\n", (long long)n);
    else fprintf(stderr, "panic: there is no memory for a text of %lld bytes\n", (long long)n);
    sg_die(loc);
}
static sg_string sg_with_capacity(int64_t n, const char* loc) {
    if (__builtin_expect(n < 0, 0)) sg_capacity_bad(n, loc);
    if (n == 0) return (sg_string){ (uint8_t*)"", 0, 0 };
    uint8_t* bytes = malloc((size_t)n);
    if (!bytes) sg_capacity_bad(n, loc);
    return (sg_string){ bytes, 0, n };
}
static void sg_puts(sg_string* out, const char* text) { sg_put(out, text, (int64_t)strlen(text)); }
static void sg_putf(sg_string* out, const char* format, ...) {
    va_list count, write;
    va_start(count, format);
    va_copy(write, count);
    /* Straight into the block, where it fits (the usual case once a text has grown). */
    int64_t room = out->cap - out->len;
    char small[128];
    int n = out->cap ? vsnprintf((char*)out->ptr + out->len, (size_t)room, format, count) : vsnprintf(small, sizeof small, format, count);
    va_end(count);
    if (out->cap && n < room) {
        out->len += n;
    } else if (!out->cap && n < (int)sizeof small) {
        sg_put(out, small, n);
    } else {
        sg_room(out, (int64_t)n + 1);
        vsnprintf((char*)out->ptr + out->len, (size_t)n + 1, format, write);
        out->len += n;
    }
    va_end(write);
}
/* How many spaces a value of `n` bytes needs in front of it or after it to take `width`
   characters: `{name,10}`. A character is counted once, however many bytes it has. */
static int sg_pad(int width, const char* p, int n) {
    int count = 0;
    for (int i = 0; i < n; i++) count += ((unsigned char)p[i] & 0xC0) != 0x80;
    return width > count ? width - count : 0;
}
/* The width to give `printf` for such a value, which counts bytes: one conversion
   (`%*.*s`) instead of the spaces and the value apart. */
static inline int sg_width(int width, const char* p, int n) {
    return n + sg_pad(width, p, n);
}
/* The same for a value just written at the end of a text, from `from` on: the spaces go in
   front of it (`width` > 0) or after it. */
static void sg_pad_from(sg_string* out, int64_t from, int width) {
    int fill = sg_pad(width < 0 ? -width : width, (const char*)out->ptr + from, (int)(out->len - from));
    if (!fill) return;
    sg_room(out, fill);
    uint8_t* at = out->ptr + from;
    if (width > 0) { memmove(at + fill, at, (size_t)(out->len - from)); memset(at, ' ', (size_t)fill); }
    else memset(out->ptr + out->len, ' ', (size_t)fill);
    out->len += fill;
}
/* `{x:b8}`: the bits of an integer, with at least `least` digits. `out` has room for 64 or
   for `least`, whichever is more. Gives how many were written. */
static int sg_binary(char* out, unsigned long long v, int least) {
    int n = v ? 64 - __builtin_clzll(v) : 1;
    if (least > n) n = least;
    for (int i = 0; i < n; i++) out[n - 1 - i] = i < 64 && (v >> i & 1) ? '1' : '0';
    return n;
}
/* `{x:F2}`: a fractional number with `digits` digits after the point, rounded on its exact
   value to the nearest (a tie to the even digit), as Rust and `printf` round it; `NaN` and
   the infinities as `print` writes them. The value is `m` times 2 to the power `-k`
   exactly; times ten to the `digits` it is a whole number and a fraction, both held in 128
   bits, so the rounding is exact and no `snprintf` is needed (it reads its format and
   costs a sixth of a line of the table benchmark). A value too large or too small for
   that, or more than 18 digits, goes to `snprintf`. */
static int sg_fixed(char* out, size_t room, double v, int digits, bool single) {
    if (!isfinite(v)) return sg_shortest(out, v, single);
    if (digits <= 18 && v != 0) {
        int e;
        uint64_t m = (uint64_t)ldexp(frexp(fabs(v), &e), 53);
        int k = 53 - e;
        if (k > 0 && k <= 120) {
            static const uint64_t tens[] = { 1ULL, 10ULL, 100ULL, 1000ULL, 10000ULL, 100000ULL, 1000000ULL,
                10000000ULL, 100000000ULL, 1000000000ULL, 10000000000ULL, 100000000000ULL, 1000000000000ULL,
                10000000000000ULL, 100000000000000ULL, 1000000000000000ULL, 10000000000000000ULL,
                100000000000000000ULL, 1000000000000000000ULL };
            unsigned __int128 n = (unsigned __int128)m * tens[digits];
            unsigned __int128 q = n >> k, rest = n - (q << k), half = (unsigned __int128)1 << (k - 1);
            if (rest > half || (rest == half && (q & 1))) q++;
            char text[48];
            char* at = text + sizeof text;
            int count = 0;
            while (q > UINT64_MAX) { *--at = (char)('0' + (int)(q % 10)); q /= 10; count++; }
            uint64_t low = (uint64_t)q;
            do { *--at = (char)('0' + low % 10); low /= 10; count++; } while (low);
            while (count <= digits) { *--at = '0'; count++; }
            int len = 0;
            if (signbit(v)) out[len++] = '-';
            memcpy(out + len, at, (size_t)(count - digits));
            len += count - digits;
            if (digits) { out[len++] = '.'; memcpy(out + len, at + count - digits, (size_t)digits); len += digits; }
            return len;
        }
    }
    return snprintf(out, room, "%.*f", digits, v);
}
static void sg_put_digits(sg_string* out, unsigned long long left, bool negative) {
    char digits[24];
    char* at = digits + sizeof digits;
    do { *--at = (char)('0' + left % 10); left /= 10; } while (left);
    if (negative) *--at = '-';
    sg_put(out, at, digits + sizeof digits - at);
}
static void sg_put_int(sg_string* out, long long value) {
    sg_put_digits(out, value < 0 ? 0ULL - (unsigned long long)value : (unsigned long long)value, value < 0);
}
static void sg_put_uint(sg_string* out, unsigned long long value) {
    sg_put_digits(out, value, false);
}
static void sg_put_double(sg_string* out, double value) {
    char text[40];
    sg_put(out, text, sg_shortest(text, value, false));
}
static void sg_put_float(sg_string* out, float value) {
    char text[40];
    sg_put(out, text, sg_shortest(text, value, true));
}
/* Where a struct, an enum or a collection is written before it is printed. One block for the
   whole program: a statement writes above what is there and gives the room back when it is
   done, so printing allocates only when a line is longer than every line before it. A block
   that one long line made larger than SG_SCRATCH_KEPT is freed as soon as nothing is written
   in it, so the program does not hold that memory until it ends. */
static sg_string sg_scratch = { (uint8_t*)"", 0, 0 };
#define SG_SCRATCH_KEPT 65536
static inline void sg_scratch_back(int64_t to) {
    if (sg_scratch.len > to) sg_scratch.len = to;
    if (__builtin_expect(sg_scratch.cap > SG_SCRATCH_KEPT, 0) && sg_scratch.len == 0) {
        free(sg_scratch.ptr);
        sg_scratch = (sg_string){ (uint8_t*)"", 0, 0 };
    }
}
/* Text inside another value is written in quotes, so that the parts can be told apart. */
static void sg_put_quoted(sg_string* out, sg_str s) {
    sg_put(out, "\"", 1);
    for (int64_t i = 0; i < s.len; i++) {
        uint8_t c = s.ptr[i];
        if (c == '"' || c == '\\') { sg_put(out, "\\", 1); sg_put(out, &c, 1); }
        else if (c == '\n') sg_puts(out, "\\n");
        else if (c == '\t') sg_puts(out, "\\t");
        else if (c == '\r') sg_puts(out, "\\r");
        else if (c == 0) sg_puts(out, "\\0");
        else if (c < 32 || c == 127) sg_putf(out, "\\u{%x}", (unsigned)c);
        else sg_put(out, &c, 1);
    }
    sg_put(out, "\"", 1);
}
/* A character as the bytes UTF-8 gives it: one to four, written into `out`. */
static inline int sg_utf8(char* out, uint32_t c) {
    if (c < 0x80) { out[0] = (char)c; return 1; }
    if (c < 0x800) { out[0] = (char)(0xC0 | (c >> 6)); out[1] = (char)(0x80 | (c & 0x3F)); return 2; }
    if (c < 0x10000) { out[0] = (char)(0xE0 | (c >> 12)); out[1] = (char)(0x80 | ((c >> 6) & 0x3F)); out[2] = (char)(0x80 | (c & 0x3F)); return 3; }
    out[0] = (char)(0xF0 | (c >> 18)); out[1] = (char)(0x80 | ((c >> 12) & 0x3F)); out[2] = (char)(0x80 | ((c >> 6) & 0x3F)); out[3] = (char)(0x80 | (c & 0x3F));
    return 4;
}
/* A character inside another value: in single quotes, with the escapes of a character constant. */
static int sg_quote_char(char* out, uint32_t c) {
    out[0] = '\'';
    int n = 1;
    switch (c) {
    case '\'': case '\\': out[n++] = '\\'; out[n++] = (char)c; break;
    case '\n': out[n++] = '\\'; out[n++] = 'n'; break;
    case '\t': out[n++] = '\\'; out[n++] = 't'; break;
    case '\r': out[n++] = '\\'; out[n++] = 'r'; break;
    case 0: out[n++] = '\\'; out[n++] = '0'; break;
    default:
        if (c < 32 || c == 127) n += snprintf(out + n, 12, "\\u{%x}", (unsigned)c);
        else n += sg_utf8(out + n, c);
    }
    out[n++] = '\'';
    return n;
}
static void sg_put_quoted_char(sg_string* out, uint32_t c) { char b[16]; sg_put(out, b, sg_quote_char(b, c)); }
SG_COLD static _Noreturn void sg_assert_fail_c(const char* text, uint32_t l, uint32_t r, const char* loc) {
    char a[16], b[16];
    int an = sg_quote_char(a, l), bn = sg_quote_char(b, r);
    fflush(stdout);
    fprintf(stderr, "panic: assert failed: %s\n  left: %.*s\n  right: %.*s\n", text, an, a, bn, b);
    sg_die(loc);
}
SG_COLD static _Noreturn void sg_char_fail(__int128 v, const char* loc) {
    fflush(stdout);
    fputs("panic: ", stderr);
    sg_err_int(v);
    fputs(" is not the code of a character\n", stderr);
    sg_die_hint(loc, "`tryToChar()` gives `null` when no character has the code");
}
SG_COLD static _Noreturn void sg_code_fail(uint32_t c, const char* ty, const char* loc) {
    fflush(stdout);
    fprintf(stderr, "panic: value %u does not fit in %s\n", (unsigned)c, ty);
    sg_die_hint(loc, "`(int)c` holds the code of every character");
}
/* A text holds characters only, so a code that is not one stops the program in every build. */
static inline bool sg_is_char(__int128 v) { return v >= 0 && v <= 0x10FFFF && !(v >= 0xD800 && v <= 0xDFFF); }
static inline uint32_t sg_to_char(__int128 v, const char* loc) { if (__builtin_expect(!sg_is_char(v), 0)) sg_char_fail(v, loc); return (uint32_t)v; }
/* White space as Unicode has it (the White_Space property): what `trim` takes off. */
static inline bool sg_is_space(uint32_t c) {
    if (c <= 0x20) return c == 0x20 || (c >= 0x09 && c <= 0x0D);
    if (c < 0x85) return false;
    return c == 0x85 || c == 0xA0 || c == 0x1680 || (c >= 0x2000 && c <= 0x200A) || c == 0x2028 || c == 0x2029 || c == 0x202F || c == 0x205F || c == 0x3000;
}
static inline bool sg_ascii_space(uint32_t c) { return c == ' ' || c == '\t' || c == '\n' || c == 0x0C || c == '\r'; }
/* The character that starts at `p`, and how many bytes it takes. The text is valid UTF-8. */
static inline uint32_t sg_decode(const uint8_t* p, int* n) {
    uint32_t c = p[0];
    if (c < 0x80) { *n = 1; return c; }
    if (c < 0xE0) { *n = 2; return ((c & 0x1F) << 6) | (p[1] & 0x3F); }
    if (c < 0xF0) { *n = 3; return ((c & 0x0F) << 12) | ((p[1] & 0x3F) << 6) | (p[2] & 0x3F); }
    *n = 4;
    return ((c & 0x07) << 18) | ((p[1] & 0x3F) << 12) | ((p[2] & 0x3F) << 6) | (p[3] & 0x3F);
}
static inline sg_str sg_char_str(uint8_t* buffer, uint32_t c) { return (sg_str){ buffer, sg_utf8((char*)buffer, c) }; }
/* Where `n` first lies in `h`, in bytes, or -1. */
static int64_t sg_find(sg_str h, sg_str n) {
    if (n.len == 0) return 0;
    if (n.len > h.len) return -1;
    const uint8_t* p = h.ptr;
    const uint8_t* end = h.ptr + (h.len - n.len + 1);
    while (p < end) {
        p = memchr(p, n.ptr[0], (size_t)(end - p));
        if (!p) return -1;
        if (memcmp(p, n.ptr, (size_t)n.len) == 0) return p - h.ptr;
        p++;
    }
    return -1;
}
static int64_t sg_find_last(sg_str h, sg_str n) {
    for (int64_t at = h.len - n.len; at >= 0; at--)
        if (n.len == 0 || memcmp(h.ptr + at, n.ptr, (size_t)n.len) == 0) return at;
    return -1;
}
static inline bool sg_starts(sg_str h, sg_str n) { return n.len <= h.len && (n.len == 0 || memcmp(h.ptr, n.ptr, (size_t)n.len) == 0); }
static inline bool sg_ends(sg_str h, sg_str n) { return n.len <= h.len && (n.len == 0 || memcmp(h.ptr + (h.len - n.len), n.ptr, (size_t)n.len) == 0); }
static bool sg_eq_ascii_case(sg_str a, sg_str b) {
    if (a.len != b.len) return false;
    for (int64_t i = 0; i < a.len; i++) {
        uint8_t x = a.ptr[i], y = b.ptr[i];
        if (x != y && ((x | 32) != (y | 32) || (uint8_t)((x | 32) - 'a') >= 26)) return false;
    }
    return true;
}
static sg_str sg_trim_start(sg_str s) {
    int64_t at = 0;
    while (at < s.len) {
        int n;
        if (!sg_is_space(sg_decode(s.ptr + at, &n))) break;
        at += n;
    }
    return (sg_str){ s.ptr + at, s.len - at };
}
static sg_str sg_trim_end(sg_str s) {
    int64_t end = s.len;
    while (end > 0) {
        int64_t start = end - 1;
        while (start > 0 && (s.ptr[start] & 0xC0) == 0x80) start--;
        int n;
        if (!sg_is_space(sg_decode(s.ptr + start, &n))) break;
        end = start;
    }
    return (sg_str){ s.ptr, end };
}
static inline sg_str sg_trim(sg_str s) { return sg_trim_end(sg_trim_start(s)); }
/* What goes over a text: where it is, and for `split` the separator. A separator that is one
   character is kept inside (`mark`, with `sep.ptr` NULL), so that the iterator can be moved. */
typedef struct { sg_str s; int64_t at; sg_str sep; uint8_t mark[4]; bool done; } sg_iter;
static inline sg_iter sg_iter_of(sg_str s) { return (sg_iter){ s, 0, { NULL, 0 }, { 0 }, false }; }
SG_COLD static _Noreturn void sg_split_empty(const char* loc) {
    fflush(stdout);
    fputs("panic: the separator of `split` is empty\n", stderr);
    sg_die_hint(loc, "the characters, one after the other: `text.chars()`");
}
static inline sg_iter sg_split_str(sg_str s, sg_str sep, const char* loc) {
    if (__builtin_expect(sep.len == 0, 0)) sg_split_empty(loc);
    return (sg_iter){ s, 0, sep, { 0 }, false };
}
/* `text.replaceAll(old, new)`: a new text, put together from the pieces between the places
   where `old` lies and a copy of `new` in each of them. */
SG_COLD static _Noreturn void sg_replace_empty(const char* loc) {
    fflush(stdout);
    fputs("panic: the text that `replaceAll` looks for is empty\n", stderr);
    sg_die(loc);
}
static sg_string sg_replace_all(sg_str s, sg_str old, sg_str put, const char* loc) {
    if (__builtin_expect(old.len == 0, 0)) sg_replace_empty(loc);
    if (s.len == 0) return SG_LIT("", 0);
    sg_string out = { (uint8_t*)"", 0, 0 };
    sg_room(&out, s.len);
    int64_t at = 0, found;
    while ((found = sg_find((sg_str){ s.ptr + at, s.len - at }, old)) >= 0) {
        sg_put(&out, s.ptr + at, found);
        sg_put(&out, put.ptr, put.len);
        at += found + old.len;
    }
    sg_put(&out, s.ptr + at, s.len - at);
    return out;
}
/* `text.repeat(n)`: one block of the whole size; the bytes written so far are copied after
   themselves, so a text repeated n times takes about log2(n) copies. */
SG_COLD static _Noreturn void sg_repeat_bad(int64_t len, int64_t n, const char* loc) {
    fflush(stdout);
    if (n < 0) fprintf(stderr, "panic: a text cannot be repeated %lld times\n", (long long)n);
    else fprintf(stderr, "panic: there is no memory for a text of %lld bytes repeated %lld times\n", (long long)len, (long long)n);
    sg_die(loc);
}
static sg_string sg_repeat(sg_str s, int64_t n, const char* loc) {
    if (__builtin_expect(n < 0, 0)) sg_repeat_bad(s.len, n, loc);
    if (n == 0 || s.len == 0) return SG_LIT("", 0);
    if (__builtin_expect(s.len > PTRDIFF_MAX / n, 0)) sg_repeat_bad(s.len, n, loc);
    int64_t size = s.len * n;
    uint8_t* bytes = malloc((size_t)size);
    if (!bytes) sg_repeat_bad(s.len, n, loc);
    memcpy(bytes, s.ptr, (size_t)s.len);
    for (int64_t done = s.len; done < size; ) {
        int64_t part = done < size - done ? done : size - done;
        memcpy(bytes + done, bytes, (size_t)part);
        done += part;
    }
    return (sg_string){ bytes, size, size };
}
static inline sg_iter sg_split_char(sg_str s, uint32_t c) {
    sg_iter it = sg_iter_of(s);
    it.sep.len = sg_utf8((char*)it.mark, c);
    return it;
}
static inline bool sg_chars_next(sg_iter* it, uint32_t* out) {
    if (it->at >= it->s.len) return false;
    int n;
    *out = sg_decode(it->s.ptr + it->at, &n);
    it->at += n;
    return true;
}
static bool sg_split_next(sg_iter* it, sg_str* out) {
    if (it->done) return false;
    sg_str rest = { it->s.ptr + it->at, it->s.len - it->at };
    sg_str sep = it->sep.ptr ? it->sep : (sg_str){ it->mark, it->sep.len };
    int64_t end = sg_find(rest, sep);
    if (end < 0) { it->done = true; *out = rest; return true; }
    *out = (sg_str){ rest.ptr, end };
    it->at += end + sep.len;
    return true;
}
static bool sg_words_next(sg_iter* it, sg_str* out) {
    const uint8_t* p = it->s.ptr;
    int64_t at = it->at, len = it->s.len;
    int n;
    while (at < len && sg_is_space(sg_decode(p + at, &n))) at += n;
    int64_t start = at;
    while (at < len && !sg_is_space(sg_decode(p + at, &n))) at += n;
    it->at = at;
    if (start == at) return false;
    *out = (sg_str){ p + start, at - start };
    return true;
}
/* A line ends at `\n`; a `\r` right before it is not part of the line. */
static bool sg_lines_next(sg_iter* it, sg_str* out) {
    if (it->at >= it->s.len) return false;
    sg_str rest = { it->s.ptr + it->at, it->s.len - it->at };
    const uint8_t* nl = memchr(rest.ptr, '\n', (size_t)rest.len);
    if (!nl) { *out = rest; it->at = it->s.len; return true; }
    int64_t end = nl - rest.ptr;
    it->at += end + 1;
    if (end > 0 && rest.ptr[end - 1] == '\r') end--;
    *out = (sg_str){ rest.ptr, end };
    return true;
}
static inline bool sg_ascii_digit(uint32_t c) { return c - '0' < 10u; }
static inline bool sg_ascii_upper_is(uint32_t c) { return c - 'A' < 26u; }
static inline bool sg_ascii_lower_is(uint32_t c) { return c - 'a' < 26u; }
static inline bool sg_ascii_letter(uint32_t c) { return (c | 32u) - 'a' < 26u; }
static inline bool sg_ascii_alnum(uint32_t c) { return sg_ascii_digit(c) || sg_ascii_letter(c); }
static inline uint32_t sg_ascii_upper(uint32_t c) { return c >= 'a' && c <= 'z' ? c - 32 : c; }
static inline uint32_t sg_ascii_lower(uint32_t c) { return c >= 'A' && c <= 'Z' ? c + 32 : c; }
SG_COLD static _Noreturn void sg_assert_fail_t(const char* text, sg_str l, sg_str r, const char* loc) {
    fflush(stdout);
    fprintf(stderr, "panic: assert failed: %s\n  left: %.*s\n  right: %.*s\n", text, (int)l.len, (const char*)l.ptr, (int)r.len, (const char*)r.ptr);
    sg_die(loc);
}
/* Puts the items in order and keeps equal items in the order they had (a merge sort). */
static void sg_sort(void* items, int64_t count, size_t size, int (*compare)(const void*, const void*)) {
    if (count < 2) return;
    uint8_t* from = items;
    uint8_t* to = malloc((size_t)count * size);
    if (!to) abort();
    for (int64_t width = 1; width < count; width *= 2) {
        for (int64_t low = 0; low < count; low += 2 * width) {
            int64_t middle = low + width < count ? low + width : count;
            int64_t high = low + 2 * width < count ? low + 2 * width : count;
            int64_t i = low, j = middle, k = low;
            while (i < middle && j < high) {
                if (compare(from + j * size, from + i * size) < 0) { memcpy(to + k * size, from + j * size, size); j++; }
                else { memcpy(to + k * size, from + i * size, size); i++; }
                k++;
            }
            if (i < middle) memcpy(to + k * size, from + i * size, (size_t)(middle - i) * size);
            if (j < high) memcpy(to + k * size, from + j * size, (size_t)(high - j) * size);
        }
        uint8_t* swap = from; from = to; to = swap;
    }
    if (from != (uint8_t*)items) { memcpy(items, from, (size_t)count * size); to = from; }
    free(to);
}
static void sg_quoted(sg_str s) {
    fputc('"', stderr);
    for (int64_t i = 0; i < s.len; i++) {
        uint8_t c = s.ptr[i];
        if (c == '"' || c == '\\') { fputc('\\', stderr); fputc(c, stderr); }
        else if (c == '\n') fputs("\\n", stderr);
        else if (c == '\t') fputs("\\t", stderr);
        else if (c == '\r') fputs("\\r", stderr);
        else fputc(c, stderr);
    }
    fputc('"', stderr);
}
SG_COLD static _Noreturn void sg_assert_fail_s(const char* text, sg_str l, sg_str r, const char* loc) {
    fflush(stdout);
    fprintf(stderr, "panic: assert failed: %s\n  left: ", text);
    sg_quoted(l);
    fputs("\n  right: ", stderr);
    sg_quoted(r);
    fputc('\n', stderr);
    sg_die(loc);
}
"#;

/// The hash table behind a `HashMap` or a `HashSet`, written once for each type of table.
/// The entries lie one after the other in `entries`, in the order they were added; `slots`
/// says, for each place a hash can point at, which entry is there (its position plus one, 0 for
/// none). An entry that is removed is replaced by the last one. `@N` is the name of the type,
/// `@K` the key, `@P` the key as it is given to look something up; the lines between `@V{`
/// and `@V}` are for a map only.
const TABLE: &str = r#"
static int64_t sg_find_@N(const sg_@N* m, @P key) {
    if (m->len == 0) return -1;
    uint64_t at = sg_khash_@N(key) & (uint64_t)m->mask;
    for (;;) {
        uint32_t slot = m->slots[at];
        if (slot == 0) return -1;
        if (sg_keq_@N(&m->entries[slot - 1].m_key, key)) return (int64_t)slot - 1;
        at = (at + 1) & (uint64_t)m->mask;
    }
}
static void sg_place_@N(sg_@N* m, int64_t entry) {
    uint64_t at = sg_khash_@N(sg_kof_@N(&m->entries[entry].m_key)) & (uint64_t)m->mask;
    while (m->slots[at] != 0) at = (at + 1) & (uint64_t)m->mask;
    m->slots[at] = (uint32_t)entry + 1;
}
/* Room for one more entry; the slots are kept at most three quarters full. */
static void sg_room_@N(sg_@N* m) {
    if (m->len == m->cap) {
        if (m->len >= 0x7fffffff) abort();
        m->entries = sg_grow(m->entries, &m->cap, sizeof(sg_E_@N));
    }
    if (!m->slots || (m->len + 1) * 4 > (m->mask + 1) * 3) {
        int64_t size = m->slots ? (m->mask + 1) * 2 : 8;
        free(m->slots);
        m->slots = calloc((size_t)size, sizeof(uint32_t));
        if (!m->slots) abort();
        m->mask = size - 1;
        for (int64_t i = 0; i < m->len; i++) sg_place_@N(m, i);
    }
}
/* A key that is known not to be there yet. */
static inline void sg_put_@N(sg_@N* m, @K key@V{, @W value@V}) {
    sg_room_@N(m);
    m->entries[m->len].m_key = key;@V{
    m->entries[m->len].m_value = value;@V}
    m->len++;
    sg_place_@N(m, m->len - 1);
}
/* Takes the entry out of the table without freeing what it holds. The slots after it move
   up so that every key is still found from its own place; the last entry takes its room. */
static void sg_take_@N(sg_@N* m, int64_t entry) {
    uint64_t mask = (uint64_t)m->mask;
    uint64_t hole = sg_khash_@N(sg_kof_@N(&m->entries[entry].m_key)) & mask;
    while (m->slots[hole] != (uint32_t)entry + 1) hole = (hole + 1) & mask;
    for (uint64_t next = (hole + 1) & mask; m->slots[next] != 0; next = (next + 1) & mask) {
        uint64_t home = sg_khash_@N(sg_kof_@N(&m->entries[m->slots[next] - 1].m_key)) & mask;
        if (((next - home) & mask) >= ((next - hole) & mask)) {
            m->slots[hole] = m->slots[next];
            hole = next;
        }
    }
    m->slots[hole] = 0;
    int64_t last = m->len - 1;
    if (entry != last) {
        uint64_t at = sg_khash_@N(sg_kof_@N(&m->entries[last].m_key)) & mask;
        while (m->slots[at] != (uint32_t)last + 1) at = (at + 1) & mask;
        m->slots[at] = (uint32_t)entry + 1;
        m->entries[entry] = m->entries[last];
    }
    m->len = last;
}
SG_COLD static _Noreturn void sg_fail_@N(@P key, bool missing, const char* loc) {
    sg_string text = SG_LIT("", 0);
    sg_kshow_@N(&text, key);
    if (missing) sg_panicf(loc, "key %.*s is not in the map\n  `map.get(key)` gives `null` for a missing key; `map.add(key, value)` puts one in", (int)text.len, (const char*)text.ptr);
    sg_panicf(loc, "key %.*s is already in the map\n  to change what is kept under a key, write `map[key] = value`", (int)text.len, (const char*)text.ptr);
}
static inline bool sg_contains_@N(const sg_@N* m, @P key) { return sg_find_@N(m, key) >= 0; }
static void sg_drop_from_@N(sg_@N* m, int64_t from) {
    for (int64_t i = from; i < m->len; i++) { @DROPE(m->entries[i]) }
    free(m->entries);
    free(m->slots);
}
static inline void sg_drop_@N(sg_@N* m) { sg_drop_from_@N(m, 0); }
static inline sg_@N sg_move_@N(sg_@N* m) { sg_@N out = *m; *m = (sg_@N){ 0 }; return out; }
static void sg_clear_@N(sg_@N* m) {
    for (int64_t i = 0; i < m->len; i++) { @DROPE(m->entries[i]) }
    m->len = 0;
    if (m->slots) memset(m->slots, 0, (size_t)(m->mask + 1) * sizeof(uint32_t));
}@V{
static inline void sg_add_@N(sg_@N* m, @K key, @W value, const char* loc) {
    if (__builtin_expect(sg_find_@N(m, sg_kof_@N(&key)) >= 0, 0)) sg_fail_@N(sg_kof_@N(&key), false, loc);
    sg_put_@N(m, key, value);
}
static inline @W* sg_at_@N(const sg_@N* m, @P key, const char* loc) {
    int64_t entry = sg_find_@N(m, key);
    if (__builtin_expect(entry < 0, 0)) sg_fail_@N(key, true, loc);
    return &m->entries[entry].m_value;
}@V}@S{
static inline bool sg_add_@N(sg_@N* m, @K key) {
    if (sg_find_@N(m, sg_kof_@N(&key)) >= 0) { @DROPK(key) return false; }
    sg_put_@N(m, key);
    return true;
}
static inline bool sg_remove_@N(sg_@N* m, @P key) {
    int64_t entry = sg_find_@N(m, key);
    if (entry < 0) return false;
    @K gone = m->entries[entry].m_key;
    sg_take_@N(m, entry);
    @DROPK(gone)
    (void)gone;
    return true;
}@S}
"#;

/// What the `test` blocks of a program call: it stands before them.
const TEST_SUPPORT: &str = r#"
static int sg_test_error = 0;
/* A `try` in a test met an error: it is written, and the test has failed. */
static void sg_test_failed(sg_str text) {
    fflush(stdout);
    fprintf(stderr, "error: %.*s\n", (int)text.len, (const char*)text.ptr);
    sg_test_error = 1;
}
"#;

/// What a program that reads or writes files, or reads its input, needs: the calls of the
/// system (`open`, `read`, `write`) and the input in blocks of its own. Added only to such a
/// program.
const IO_RUNTIME: &str = r#"
#include <errno.h>
#include <fcntl.h>
#include <sys/stat.h>
#include <unistd.h>

/* The variant of `IoError` for a code the system gave: 0 NotFound, 1 PermissionDenied,
   2 IsDirectory, 3 NoSpace; 5 is Other, which keeps the code. */
static int sg_io_kind(int code) {
    switch (code) {
    case ENOENT: case ENOTDIR: return 0;
    case EPERM: case EACCES: case EROFS: return 1;
    case EISDIR: return 2;
    case ENOSPC:
#ifdef EDQUOT
    case EDQUOT:
#endif
        return 3;
    default: return 5;
    }
}
/* A path as the system takes it, ended by a zero byte. No file has a zero byte in its name
   (the system would see the name end there), so such a path is not found. */
static char* sg_path(sg_str path, int* code) {
    if (path.len > 0 && memchr(path.ptr, 0, (size_t)path.len)) { *code = ENOENT; return NULL; }
    char* name = malloc((size_t)path.len + 1);
    if (!name) abort();
    if (path.len > 0) memcpy(name, path.ptr, (size_t)path.len);
    name[path.len] = 0;
    return name;
}
/* The whole file into a block of its own: 0, or the code the system gave. */
static int sg_read_file(sg_str path, uint8_t** bytes, int64_t* len) {
    int code = 0;
    char* name = sg_path(path, &code);
    if (!name) return code;
    int fd = open(name, O_RDONLY);
    free(name);
    if (fd < 0) return errno;
    struct stat info;
    size_t cap = fstat(fd, &info) == 0 && S_ISREG(info.st_mode) && info.st_size > 0 ? (size_t)info.st_size + 1 : 4096;
    uint8_t* block = malloc(cap);
    if (!block) abort();
    size_t n = 0;
    for (;;) {
        if (n == cap) { cap *= 2; block = realloc(block, cap); if (!block) abort(); }
        ssize_t got = read(fd, block + n, cap - n);
        if (got < 0) { if (errno == EINTR) continue; code = errno; break; }
        if (got == 0) break;
        n += (size_t)got;
    }
    close(fd);
    if (code) { free(block); return code; }
    *bytes = block;
    *len = (int64_t)n;
    return 0;
}
/* The bytes into the file, which is made if it is not there; `append` keeps what it holds. */
static int sg_write_file(sg_str path, const uint8_t* bytes, int64_t len, bool append) {
    int code = 0;
    char* name = sg_path(path, &code);
    if (!name) return code;
    int fd = open(name, O_WRONLY | O_CREAT | (append ? O_APPEND : O_TRUNC), 0666);
    free(name);
    if (fd < 0) return errno;
    int64_t done = 0;
    while (done < len) {
        ssize_t put = write(fd, bytes + done, (size_t)(len - done));
        if (put < 0) { if (errno == EINTR) continue; code = errno; break; }
        done += put;
    }
    if (close(fd) != 0 && !code) code = errno;
    return code;
}
/* A file or the input, read in blocks of 64 KB that the reader keeps. `done` counts the bytes
   given out: where an `InvalidText` lies. A reader that was moved has no block. On a terminal,
   what was printed goes out before the program waits for the input. */
typedef struct { int fd; uint8_t* block; int64_t at, len, done; bool ended; } sg_reader;
static uint8_t sg_in_block[65536];
static sg_reader sg_stdin = { 0, sg_in_block, 0, 0, 0, false };
static int sg_in_terminal = -1;
static int sg_reader_fill(sg_reader* r) {
    if (r == &sg_stdin) {
        if (sg_in_terminal < 0) sg_in_terminal = isatty(0);
        if (sg_in_terminal) fflush(stdout);
    }
    for (;;) {
        ssize_t got = read(r->fd, r->block, 65536);
        if (got < 0) { if (errno == EINTR) continue; return errno; }
        r->at = 0;
        r->len = got;
        if (got == 0) r->ended = true;
        return 0;
    }
}
/* The next line into `line`, without its `\n` or `\r\n`: 0 with `*more` false at the end,
   the code the system gave, or -1 with `*bad` where the line is not UTF-8. */
static int sg_read_line(sg_reader* r, sg_string* line, bool* more, int64_t* bad) {
    line->len = 0;
    int64_t start = r->done;
    bool any = false;
    for (;;) {
        if (r->at == r->len) {
            if (r->ended) break;
            int code = sg_reader_fill(r);
            if (code) return code;
            continue;
        }
        any = true;
        const uint8_t* from = r->block + r->at;
        const uint8_t* end = memchr(from, '\n', (size_t)(r->len - r->at));
        int64_t n = end ? end - from + 1 : r->len - r->at;
        sg_put(line, from, n);
        r->at += n;
        r->done += n;
        if (end) break;
    }
    *more = any;
    if (!any) return 0;
    if (line->len > 0 && line->ptr[line->len - 1] == '\n') {
        line->len--;
        if (line->len > 0 && line->ptr[line->len - 1] == '\r') line->len--;
    }
    int64_t at = sg_utf8_bad(line->ptr, line->len);
    if (at >= 0) { line->len = 0; *bad = start + at; return -1; }
    return 0;
}
/* What is left. */
static int sg_read_rest(sg_reader* r, sg_string* text) {
    *text = SG_LIT("", 0);
    for (;;) {
        if (r->at < r->len) {
            sg_put(text, r->block + r->at, r->len - r->at);
            r->done += r->len - r->at;
            r->at = r->len;
        }
        if (r->ended) return 0;
        int code = sg_reader_fill(r);
        if (code) { sg_drop_string(text); *text = SG_LIT("", 0); return code; }
    }
}
/* Bytes into `to`, as many as `len` unless the end comes first; `*got` says how many. What the
   block holds goes first, and a rest of 64 KB or more is read straight into `to`. */
static int sg_read_bytes(sg_reader* r, uint8_t* to, int64_t len, int64_t* got) {
    int64_t n = 0;
    int code = 0;
    while (n < len) {
        if (r->at < r->len) {
            int64_t take = r->len - r->at < len - n ? r->len - r->at : len - n;
            memcpy(to + n, r->block + r->at, (size_t)take);
            r->at += take;
            n += take;
            continue;
        }
        if (r->ended) break;
        if (len - n < 65536) {
            code = sg_reader_fill(r);
            if (code) break;
            continue;
        }
        if (r == &sg_stdin) {
            if (sg_in_terminal < 0) sg_in_terminal = isatty(0);
            if (sg_in_terminal) fflush(stdout);
        }
        ssize_t put = read(r->fd, to + n, (size_t)(len - n));
        if (put < 0) { if (errno == EINTR) continue; code = errno; break; }
        if (put == 0) { r->ended = true; break; }
        n += put;
    }
    r->done += n;
    *got = n;
    return code;
}
/* A folder opens, but cannot be read: it is told at once. */
static int sg_open_reader(sg_str path, sg_reader* out) {
    int code = 0;
    char* name = sg_path(path, &code);
    if (!name) return code;
    int fd = open(name, O_RDONLY);
    free(name);
    if (fd < 0) return errno;
    struct stat info;
    if (fstat(fd, &info) == 0 && S_ISDIR(info.st_mode)) { close(fd); return EISDIR; }
    uint8_t* block = malloc(65536);
    if (!block) abort();
    *out = (sg_reader){ fd, block, 0, 0, 0, false };
    return 0;
}
static inline void sg_drop_reader(sg_reader* r) {
    if (!r->block) return;
    close(r->fd);
    free(r->block);
    r->block = NULL;
}
static inline sg_reader sg_move_reader(sg_reader* r) { sg_reader out = *r; r->block = NULL; return out; }
/* `reader.lines()`: where it reads from, and whether it has met the end or an error. */
typedef struct { sg_reader* r; bool done; } sg_read_lines;

/* A file written through a block of 64 KB, or the output and the standard error (`std` 1 and
   2), written through C's `stdout` and `stderr` so that they keep their order with `print`.
   `failed`: the first error, which every later call gives again. A writer that was moved or
   closed has `fd` -1. */
typedef struct { int fd; int std; int failed; sg_string buf; } sg_writer;
static sg_writer sg_stdout = { 1, 1, 0, { (uint8_t*)"", 0, 0 } };
static sg_writer sg_stderr = { 2, 2, 0, { (uint8_t*)"", 0, 0 } };
static int sg_write_all(int fd, const uint8_t* bytes, int64_t len) {
    while (len > 0) {
        ssize_t put = write(fd, bytes, (size_t)len);
        if (put < 0) { if (errno == EINTR) continue; return errno; }
        bytes += put;
        len -= put;
    }
    return 0;
}
static int sg_writer_out(sg_writer* w) {
    int code = sg_write_all(w->fd, w->buf.ptr, w->buf.len);
    w->buf.len = 0;
    if (code) w->failed = code;
    return code;
}
/* After something was put into the block: a standard stream writes it at once, a file when the
   block holds 64 KB. */
static int sg_writer_wrote(sg_writer* w) {
    if (w->failed) { w->buf.len = 0; return w->failed; }
    if (w->std) {
        if (w->std == 2) fflush(stdout);
        size_t len = (size_t)w->buf.len;
        w->buf.len = 0;
        if (len > 0 && fwrite(w->buf.ptr, 1, len, w->std == 1 ? stdout : stderr) != len) return w->failed = errno ? errno : EIO;
        return 0;
    }
    return w->buf.len >= 65536 ? sg_writer_out(w) : 0;
}
static int sg_writer_flush(sg_writer* w) {
    if (w->failed) return w->failed;
    if (w->std) return fflush(w->std == 1 ? stdout : stderr) == 0 ? 0 : (w->failed = errno ? errno : EIO);
    return sg_writer_out(w);
}
/* The block goes out and the file is closed; the first error is given. */
static int sg_writer_close(sg_writer* w) {
    int code = w->failed ? w->failed : sg_writer_out(w);
    if (close(w->fd) != 0 && !code) code = errno;
    w->fd = -1;
    sg_drop_string(&w->buf);
    w->buf = (sg_string){ (uint8_t*)"", 0, 0 };
    return code;
}
/* A writer that ends without `close` writes its block and closes, and the error is lost. */
static inline void sg_drop_writer(sg_writer* w) {
    if (w->fd < 0 || w->std) return;
    if (!w->failed) (void)sg_write_all(w->fd, w->buf.ptr, w->buf.len);
    close(w->fd);
    w->fd = -1;
    sg_drop_string(&w->buf);
    w->buf = (sg_string){ (uint8_t*)"", 0, 0 };
}
static inline sg_writer sg_move_writer(sg_writer* w) {
    sg_writer out = *w;
    w->fd = -1;
    w->buf = (sg_string){ (uint8_t*)"", 0, 0 };
    return out;
}
/* The bytes as they are: a standard stream writes them at once, a file puts them into its block,
   or, when they are 64 KB or more, writes the block and then them straight. */
static int sg_writer_bytes(sg_writer* w, const uint8_t* bytes, int64_t len) {
    if (w->failed) return w->failed;
    if (len == 0) return 0;
    if (w->std) {
        if (w->std == 2) fflush(stdout);
        if (fwrite(bytes, 1, (size_t)len, w->std == 1 ? stdout : stderr) != (size_t)len) return w->failed = errno ? errno : EIO;
        return 0;
    }
    if (len < 65536) {
        sg_put(&w->buf, bytes, len);
        return sg_writer_wrote(w);
    }
    int code = sg_writer_out(w);
    if (code) return code;
    code = sg_write_all(w->fd, bytes, len);
    if (code) w->failed = code;
    return code;
}
/* `File.createText` makes or empties the file; `File.openAppend` (`append`) makes a missing one
   and writes after what it holds. */
static int sg_open_writer(sg_str path, bool append, sg_writer* out) {
    int code = 0;
    char* name = sg_path(path, &code);
    if (!name) return code;
    int fd = open(name, O_WRONLY | O_CREAT | (append ? O_APPEND : O_TRUNC), 0666);
    free(name);
    if (fd < 0) return errno;
    *out = (sg_writer){ fd, 0, 0, { (uint8_t*)"", 0, 0 } };
    return 0;
}
"#;

/// What a program of tests ends with: each `test` block runs in a process of its own, so a
/// panic ends that test and not the ones after it. What the test writes as it stops (the
/// panic, or the error a `try` met) is caught and written under its `FAIL` line. With
/// `--only N` the program runs one test in this process and ends with 0 if it passed, which
/// is how a test is looked at on its own (for leaks, in a debugger).
const TEST_RUNNER: &str = r#"
#include <sys/wait.h>
#include <unistd.h>

typedef struct { const char* name; void (*run)(void); } sg_test;
static int sg_run_test(const sg_test* test) {
    sg_test_error = 0;
    test->run();
    fflush(stdout);
    return sg_test_error;
}
static int sg_run_tests(const sg_test* tests, int count, int argc, char** argv) {
    sg_in_tests = true;
    if (argc == 3 && strcmp(argv[1], "--only") == 0) {
        long only = strtol(argv[2], NULL, 10);
        if (only < 0 || only >= count) return 3;
        return sg_run_test(&tests[only]);
    }
    int failed = 0;
    for (int i = 0; i < count; i++) {
        int ends[2];
        fflush(stdout);
        fflush(stderr);
        if (pipe(ends) != 0) { perror("pipe"); return 2; }
        pid_t child = fork();
        if (child < 0) { perror("fork"); return 2; }
        if (child == 0) {
            close(ends[0]);
            dup2(ends[1], 2);
            close(ends[1]);
            exit(sg_run_test(&tests[i]));
        }
        close(ends[1]);
        size_t len = 0, cap = 256;
        char* reason = malloc(cap);
        if (!reason) abort();
        for (;;) {
            if (len == cap) { cap *= 2; reason = realloc(reason, cap); if (!reason) abort(); }
            ssize_t got = read(ends[0], reason + len, cap - len);
            if (got <= 0) break;
            len += (size_t)got;
        }
        close(ends[0]);
        int status = 0;
        waitpid(child, &status, 0);
        while (len > 0 && (reason[len - 1] == '\n' || reason[len - 1] == ' ')) len--;
        if (WIFEXITED(status) && WEXITSTATUS(status) == 0) {
            printf("ok    %s\n", tests[i].name);
        } else {
            failed++;
            printf("FAIL  %s\n", tests[i].name);
            if (len > 0) printf("%.*s\n", (int)len, reason);
            else if (WIFSIGNALED(status)) printf("the test was stopped by signal %d\n", WTERMSIG(status));
        }
        free(reason);
    }
    if (failed == 0) printf("%d test(s) passed\n", count);
    else printf("%d of %d test(s) failed\n", failed, count);
    return failed == 0 ? 0 : 1;
}
"#;

/// Writes the program as C. A panic message names the file its line is in. `checked`: a
/// debug build, where `+`, `-`, `*` and unary `-` panic on overflow; without it, a release
/// build, where they wrap. `tests`: the program that runs the `test` blocks instead of `main`.
pub fn emit(prog: &Program, sources: &Sources, checked: bool, tests: bool) -> Res<String> {
    let structs = prog.structs.iter().map(|decl| (decl.name.as_str(), decl)).collect();
    let enums = prog.enums.iter().map(|decl| (decl.name.as_str(), decl)).collect();
    let mut emitter = Emitter {
        sources,
        files: &prog.files,
        structs,
        enums,
        aliases: prog.aliases.iter().map(|alias| (alias.name.as_str(), alias)).collect(),
        checked,
        locs: Vec::new(),
        sums: Vec::new(),
        temps: 0,
        scopes: Vec::new(),
        loops: Vec::new(),
        pre: Vec::new(),
        post: Vec::new(),
        types: String::new(),
        glue_protos: String::new(),
        glue: String::new(),
        made: HashSet::new(),
        sigs: HashMap::new(),
        ret: Ty::Void,
        maybe: 0,
        late: None,
        head: false,
        exits: 0,
        ways_out: 0,
        in_test: false,
        late_types: String::new(),
        unchecked: Vec::new(),
        break_to: None,
    };
    // Every struct and enum is named first, so that a type can point at itself or at a later
    // one. Then the types themselves, each after the types it holds by value.
    for decl in &prog.structs {
        let _ = writeln!(emitter.types, "typedef struct S_{0} S_{0};", cname(&decl.name));
    }
    for decl in &prog.enums {
        let _ = writeln!(emitter.types, "typedef struct E_{0} E_{0};", cname(&decl.name));
    }
    let mut done: Vec<&str> = Vec::new();
    for decl in &prog.structs {
        emitter.struct_decl(decl, &mut done)?;
    }
    for decl in &prog.enums {
        emitter.enum_decl(decl, &mut done)?;
    }

    // What each function takes: an argument is shaped for the parameter it goes to.
    for func in &prog.funcs {
        let params = func.params.iter().map(|param| emitter.written(&param.ty)).collect::<Res<Vec<Ty>>>()?;
        emitter.sigs.insert(format!("f_{}", cname(&func.name)), params);
    }
    let typed = || {
        let structs = prog.structs.iter().map(|decl| (&decl.name, &decl.methods));
        structs.chain(prog.enums.iter().map(|decl| (&decl.name, &decl.methods)))
    };
    for (owner, methods) in typed() {
        for method in methods {
            let params = method.params.iter().map(|param| emitter.written(&param.ty)).collect::<Res<Vec<Ty>>>()?;
            emitter.sigs.insert(format!("f_{}_{}", cname(owner), method.name), params);
        }
    }

    let mut protos = String::new();
    let mut bodies = String::new();
    for func in &prog.funcs {
        emitter.func(func, None, &mut protos, &mut bodies)?;
    }
    for (owner, methods) in typed() {
        for method in methods {
            emitter.func(method, Some(owner), &mut protos, &mut bodies)?;
        }
    }

    // The `test` blocks, each a function, and the list the runner goes through. A test of a
    // module has its module in front of its name.
    let mut listed = String::new();
    if tests {
        for (number, test) in prog.tests.iter().enumerate() {
            emitter.test(test, number, &mut protos, &mut bodies)?;
            let name = match sources.file_at(test.name_span.start).module.as_str() {
                "" => test.name.clone(),
                module => format!("{module}: {}", test.name),
            };
            let _ = writeln!(listed, "    {{ {}, t_{number} }},", c_string(&name));
        }
    }

    // Where the program starts. An error that `main` ends with is written out, and the
    // program ends with the exit code 1.
    let main = prog.funcs.iter().find(|func| func.name == "main");
    let start = match main {
        _ if tests => format!(
            "static const sg_test sg_tests[] = {{\n{listed}    {{ NULL, NULL }}\n}};\nint main(int argc, char** argv) {{\n    return sg_run_tests(sg_tests, {}, argc, argv);\n}}\n",
            prog.tests.len()
        ),
        None => unreachable!("the checker asks for `main`"),
        Some(main) => emitter.start(main)?,
    };

    // A program with a hash table draws the key of its hash before anything else runs.
    let start = match emitter.made.contains("tables") {
        true => start.replacen(") {\n", ") {\n    sg_seed_init();\n", 1),
        false => start,
    };

    let mut out = String::from(RUNTIME);
    if emitter.made.contains("io") {
        out.push_str(IO_RUNTIME);
    }
    if tests {
        out.push_str(TEST_SUPPORT);
    }
    out.push('\n');
    out.push_str(&emitter.types);
    out.push_str(&emitter.late_types);
    out.push('\n');
    out.push_str(&emitter.glue_protos);
    out.push('\n');
    // The helpers call the methods a type wrote itself (`format`, `equals`, `compare`).
    out.push_str(&protos);
    out.push('\n');
    out.push_str(&emitter.glue);
    for (number, text) in emitter.locs.iter().enumerate() {
        let _ = writeln!(out, "static const char sg_loc_{number}[] = {};", c_string(text));
    }
    for (number, sum) in emitter.sums.iter().enumerate() {
        let _ = writeln!(out, "static const sg_sum sg_sum_{number} = {sum};");
    }
    out.push('\n');
    out.push_str(&bodies);
    if tests {
        out.push_str(TEST_RUNNER);
    }
    out.push_str(&start);
    Ok(out)
}

fn not_yet(span: Span, what: &str) -> Diag {
    Diag::error(span, format!("{what} is not in the C back end yet"))
        .note("the back end compiles integers, `bool`, text, functions, structs, enums and `match`, `Result` and `try`, the collections, arrays and slices, function values, `T*`, `T?`, `read` and `ref`, and `print`")
}

/// A C string literal for any text.
/// Whether a value of this type is written by the helper of its type: a struct, an enum
/// or a collection, also behind a borrow or a pointer.
/// `Stdin`, `Stdout` and `Stderr`: every function has them, as `ref` parameters the program
/// gives, which are the streams of the runtime.
fn streams() -> Vec<Local> {
    [("Stdin", "sg_stdin", Ty::Reader), ("Stdout", "sg_stdout", Ty::Writer), ("Stderr", "sg_stderr", Ty::Writer)]
        .into_iter()
        .map(|(name, global, ty)| Local { name: name.to_string(), cname: format!("(&{global})"), ty: Ty::Ref(Box::new(ty)), gone: false, drop: None })
        .collect()
}

/// The runtime function that moves an iterator over a text on.
fn next_of(kind: TextIter) -> &'static str {
    match kind {
        TextIter::Chars => "sg_chars_next",
        TextIter::Split => "sg_split_next",
        TextIter::SplitWhitespace => "sg_words_next",
        TextIter::Lines => "sg_lines_next",
        TextIter::ReadLines => "sg_read_lines_next",
    }
}

/// The methods of `char` (and the ASCII ones of `byte`) that the back end writes inline.
fn is_char_method(name: &str) -> bool {
    name.starts_with("isAscii") || matches!(name, "toAsciiUpper" | "toAsciiLower" | "isWhitespace" | "utf8Length" | "tryToChar")
}

fn has_helper(ty: &Ty) -> bool {
    let mut core = ty.strip_borrow();
    while let Ty::Ptr(inner) = core {
        core = inner;
    }
    matches!(core, Ty::Struct(_) | Ty::Enum(_) | Ty::List(_) | Ty::Map(..) | Ty::Set(_) | Ty::Array(..) | Ty::Slice(_))
}

/// The C statement that adds this text to what `out` is writing.
fn put_text(text: &str) -> String {
    format!("sg_put(out, {}, {});", c_string(text), text.len())
}

fn c_string(text: &str) -> String {
    let mut out = String::from("\"");
    for byte in text.bytes() {
        match byte {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            b'\n' => out.push_str("\\n"),
            b'\t' => out.push_str("\\t"),
            // `??` followed by some characters is a trigraph in C.
            b'?' => out.push_str("\\?"),
            32..=126 => out.push(byte as char),
            // Three octal digits: nothing after them can be read as part of the escape.
            _ => {
                let _ = write!(out, "\\{byte:03o}");
            }
        }
    }
    out.push('"');
    out
}

fn int_name(ty: IntTy) -> &'static str {
    ty.name()
}

fn int_ctype(ty: IntTy) -> &'static str {
    match ty {
        IntTy::Sbyte => "int8_t",
        IntTy::Byte => "uint8_t",
        IntTy::Short => "int16_t",
        IntTy::Ushort => "uint16_t",
        IntTy::Int => "int32_t",
        IntTy::Uint => "uint32_t",
        IntTy::Long | IntTy::Nint => "int64_t",
        IntTy::Ulong | IntTy::Nuint => "uint64_t",
    }
}

/// A number in C, in its type. The least `long` cannot be written as a C constant, since
/// `9223372036854775808` alone is too large; a number past every `long` is unsigned.
fn int_literal(value: i128, ty: IntTy) -> String {
    let ctype = int_ctype(ty);
    match value {
        v if v == i64::MIN as i128 => format!("(({ctype})(-9223372036854775807LL - 1))"),
        v if v > i64::MAX as i128 => format!("(({ctype}){v}ULL)"),
        v => format!("(({ctype}){v}LL)"),
    }
}

fn float_ctype(ty: FloatTy) -> &'static str {
    match ty {
        FloatTy::Float => "float",
        FloatTy::Double => "double",
    }
}

/// A fractional number in C, in its type: `1.5`, `1e9`, `2.0f`. A whole number made
/// fractional by the checker gets its point.
fn float_literal(text: &str, ty: FloatTy) -> String {
    let point = if text.contains(['.', 'e']) { "" } else { ".0" };
    let suffix = if ty == FloatTy::Float { "f" } else { "" };
    format!("({text}{point}{suffix})")
}

/// `int.MaxValue`, `double.NaN`, `float.Pi` in C. A fractional constant is written with the
/// fewest digits that give it back.
fn constant(ty: &Ty, name: &str) -> String {
    match ty {
        Ty::Int(ty) => int_literal(if name == "MinValue" { ty.min() } else { ty.max() }, *ty),
        Ty::Float(ty) => {
            let ctype = float_ctype(*ty);
            let (single, double) = match name {
                "NaN" => return format!("(({ctype})NAN)"),
                "Infinity" => return format!("(({ctype})INFINITY)"),
                "MinValue" => (f32::MIN, f64::MIN),
                "MaxValue" => (f32::MAX, f64::MAX),
                "Pi" => (std::f32::consts::PI, std::f64::consts::PI),
                _ => (std::f32::consts::E, std::f64::consts::E),
            };
            match ty {
                FloatTy::Float => format!("({single:e}f)"),
                FloatTy::Double => format!("({double:e})"),
            }
        }
        _ => unreachable!("the checker gives constants to number types only"),
    }
}

/// The C function that writes a fractional number into a text.
fn float_put(ty: FloatTy) -> &'static str {
    match ty {
        FloatTy::Float => "sg_put_float",
        FloatTy::Double => "sg_put_double",
    }
}

/// The C name of a math function on the type: `sqrt`, `sqrtf`.
fn float_fn(name: &str, ty: FloatTy) -> String {
    let base = match name {
        "abs" => "fabs",
        "truncate" => "trunc",
        "fusedMultiplyAdd" => "fma",
        other => other,
    };
    if ty == FloatTy::Float { format!("{base}f") } else { base.to_string() }
}

/// How `printf` and the writing helpers take a number of this type: as it is, signed or not.
fn int_shown(ty: IntTy) -> (&'static str, &'static str, &'static str) {
    if ty.signed() {
        ("%lld", "long long", "sg_put_int")
    } else {
        ("%llu", "unsigned long long", "sg_put_uint")
    }
}

/// The width after `,` (`{n,8}`, `{name,-10}`) put on the value just added to a `printf`
/// format. `printf` counts bytes: a number (`ascii`, also one written as text first) takes
/// the width in its conversion (`%8lld`, `%10.*s`); a text, a character or what a helper
/// wrote is counted in characters by `sg_width`, which gives the width in bytes.
fn widen(format: &mut String, values: &mut Vec<String>, width: i32, ascii: bool) {
    if width == 0 {
        return;
    }
    if !ascii && format.ends_with("%.*s") {
        let (ptr, len) = (values.pop().expect("a text has its bytes"), values.pop().expect("a text has its length"));
        format.truncate(format.len() - 4);
        format.push_str(if width > 0 { "%*.*s" } else { "%-*.*s" });
        values.extend([format!("sg_width({}, {ptr}, {len})", width.abs()), len, ptr]);
    } else {
        let at = format.rfind('%').expect("a value was just added") + 1;
        format.insert_str(at, &width.to_string());
    }
}

/// A borrow of a borrow is the inner borrow: looking at a field that itself borrows gives
/// the pointer the field holds.
fn flat(mut ty: &Ty) -> &Ty {
    while let Ty::Read(inner) | Ty::Ref(inner) = ty {
        if !inner.is_borrow() {
            break;
        }
        ty = inner;
    }
    ty
}

/// The C place of the value behind the borrows and pointers at `place`, and its type.
fn behind<'t>(place: &str, ty: &'t Ty) -> (String, &'t Ty) {
    let (mut at, mut ty) = (place.to_string(), flat(ty));
    while let Ty::Read(inner) | Ty::Ref(inner) | Ty::Ptr(inner) = ty {
        (at, ty) = (format!("(*{at})"), flat(inner));
    }
    (at, ty)
}

/// `read string`: the one borrow that is not a plain pointer.
fn is_text_view(ty: &Ty) -> bool {
    matches!(flat(ty), Ty::Read(inner) if **inner == Ty::Str)
}

/// `read T[]` and `ref T[]`: a window onto items, kept as where they are and how many. Like
/// `read string`, it is a value of its own and not a pointer to one.
fn is_window(ty: &Ty) -> bool {
    matches!(flat(ty), Ty::Read(inner) | Ty::Ref(inner) if matches!(**inner, Ty::Slice(_)))
}

/// The array or the slice a value is or leads to, behind borrows and pointers.
fn items_behind(ty: &Ty) -> Option<&Ty> {
    let mut ty = ty.strip_borrow();
    while let Ty::Ptr(inner) = ty {
        ty = inner;
    }
    matches!(ty, Ty::Array(..) | Ty::Slice(_)).then_some(ty)
}

fn is_text(ty: &Ty) -> bool {
    *ty.strip_borrow() == Ty::Str
}

/// Whether this is text on the heap: `string*`, or a borrow of one.
fn is_text_behind_pointer(ty: &Ty) -> bool {
    let mut ty = ty.strip_borrow();
    let mut behind = false;
    while let Ty::Ptr(inner) = ty {
        (ty, behind) = (inner, true);
    }
    behind && *ty == Ty::Str
}

/// The name a type has inside the names of its helpers: `sg_drop_P_S_Node`. `None` for a type
/// the back end does not hold in a `T*` or a `T?` yet.
fn mangle(ty: &Ty) -> Option<String> {
    Some(match ty {
        Ty::Int(ty) => ty.name().to_string(),
        Ty::Float(ty) => ty.name().to_string(),
        Ty::Bool => "bool".to_string(),
        Ty::Char => "char".to_string(),
        Ty::Str => "string".to_string(),
        Ty::Reader => "reader".to_string(),
        Ty::Writer => "writer".to_string(),
        Ty::Struct(name) => format!("S_{}", cname(name)),
        Ty::Enum(name) => format!("E_{}", cname(name)),
        Ty::Ptr(inner) => format!("P_{}", mangle(inner)?),
        Ty::Nullable(inner) => match &**inner {
            // A pointer that may be `null` is the same pointer.
            Ty::Ptr(_) => mangle(inner)?,
            Ty::Nullable(_) => return None,
            _ => format!("O_{}", mangle(inner)?),
        },
        Ty::List(item) => format!("L_{}", mangle(item)?),
        // An array has its length in its name: `int[3]` and `int[4]` are two types.
        Ty::Array(item, length) => format!("A{length}_{}", mangle(item)?),
        // A window onto items, whether it is read through or written through.
        Ty::Slice(item) => format!("V_{}", mangle(item)?),
        Ty::Read(_) | Ty::Ref(_) if is_window(ty) => mangle(flat(ty).strip_borrow())?,
        // A look at a text, where it is held by something else: `Result<read string, E>`.
        Ty::Read(_) | Ty::Ref(_) if is_text_view(ty) => "str".to_string(),
        Ty::Set(item) => format!("H_{}", mangle(item)?),
        Ty::Map(key, value) => {
            let key = mangle(key)?;
            format!("M{}_{key}_{}", key.len(), mangle(value)?)
        }
        // What a `for` over a map gives: an entry of that map.
        Ty::Entry(key, value) => format!("E_{}", mangle(&Ty::Map(key.clone(), value.clone()))?),
        // A function type is named after the way it is written, letter by letter.
        Ty::Func(..) => {
            let mut name = String::from("F_");
            for letter in ty.to_string().chars() {
                match letter {
                    'a'..='z' | 'A'..='Z' | '0'..='9' => name.push(letter),
                    '_' => name.push_str("__"),
                    other => {
                        let _ = write!(name, "_{:02x}", other as u32);
                    }
                }
            }
            name
        }
        // The length of the first name keeps two different pairs from sharing a name.
        Ty::Result(ok, error) => {
            let ok = match &**ok {
                Ty::Void => "void".to_string(),
                // Success that is a look at a value someone else owns: a pointer to it.
                Ty::Read(inner) if !is_text_view(ok) && !is_window(ok) && !inner.is_borrow() && lent_option(ok).is_none() => format!("B_{}", mangle(inner)?),
                _ => mangle(ok)?,
            };
            format!("R{}_{ok}_{}", ok.len(), mangle(error)?)
        }
        _ => return None,
    })
}

/// A full name inside a C name. A name of the root is written as it is. A name of a module has
/// each folder in front, with its length: `shop.parts.Part` is `4shop_5parts_Part`. No name
/// of the root starts with a digit, so the two never meet, and the lengths keep two modules
/// apart whatever their names are.
fn cname(full: &str) -> String {
    let mut parts: Vec<&str> = full.split('.').collect();
    let name = parts.pop().unwrap_or(full);
    let mut out = String::new();
    for part in parts {
        let _ = write!(out, "{}{part}_", part.len());
    }
    out + name
}

/// Whether computing the expression can leave the function: it holds a `try`, a `?? return`,
/// or a `match`, whose arms may.
fn may_leave(expr: &Expr) -> bool {
    let any = |list: &mut dyn Iterator<Item = &Expr>| list.filter(|inner| may_leave(inner)).count() > 0;
    match &expr.kind {
        ExprKind::Try { .. } | ExprKind::Return(_) | ExprKind::Match { .. } => true,
        ExprKind::Int(..) | ExprKind::Float(..) | ExprKind::Bool(_) | ExprKind::Str(_) | ExprKind::Char(_) | ExprKind::Null | ExprKind::Var(_) | ExprKind::FuncRef(_) | ExprKind::Constant { .. } => false,
        ExprKind::Array { args, items, .. } => any(&mut args.iter().chain(items.iter().flatten())),
        ExprKind::AsSlice(inner) => may_leave(inner),
        ExprKind::Field { recv: inner, .. }
        | ExprKind::New(inner)
        | ExprKind::NewShared(inner)
        | ExprKind::Share(inner)
        | ExprKind::WeakOf(inner)
        | ExprKind::Load(inner)
        | ExprKind::Unbox(inner)
        | ExprKind::Widen(_, inner)
        | ExprKind::IsNull { expr: inner, .. }
        | ExprKind::IsVariant { expr: inner, .. }
        | ExprKind::Unary(_, inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::Borrow { expr: inner, .. } => may_leave(inner),
        ExprKind::Binary(_, left, right) | ExprKind::Coalesce { left, right, .. } => may_leave(left) || may_leave(right),
        ExprKind::Index { recv, index, .. } => may_leave(recv) || may_leave(index),
        ExprKind::Slice { recv, from, to } => may_leave(recv) || any(&mut from.iter().chain(to).map(|inner| &**inner)),
        ExprKind::Interp(parts) => any(&mut parts.iter().filter_map(|part| match part {
            InterpPart::Expr(inner, _) => Some(inner),
            InterpPart::Text(_) => None,
        })),
        ExprKind::StructLit { fields, .. } => any(&mut fields.iter().map(|field| &field.value)),
        ExprKind::Variant { args, .. } | ExprKind::Call { args, .. } => any(&mut args.iter()),
        ExprKind::Method { recv, args, .. } => may_leave(recv) || any(&mut args.iter()),
        ExprKind::Collection { args, items, .. } => any(&mut args.iter()) || any(&mut items.iter().flatten().flat_map(|item| item.values.iter())),
    }
}

/// A `T?` that is kept as a value and a flag: every `T?` but a pointer.
fn is_option(ty: &Ty) -> bool {
    matches!(ty, Ty::Nullable(inner) if !matches!(**inner, Ty::Ptr(_)))
}

/// A `read T?` whose `T?` would be a value and a flag: the `T?` behind it. In C it is not a
/// pointer to that pair but a pointer to the `T`, null when there is nothing (for text, a
/// window of length -1): what a function gives back points straight at what it was lent.
/// A `ref T?` stays a pointer to the pair, because `null` can be written through it.
fn lent_option(ty: &Ty) -> Option<&Ty> {
    match ty {
        Ty::Read(inner) if is_option(inner) => Some(inner),
        _ => None,
    }
}

/// The structs and enums a field of this type needs to be defined first.
fn held_types<'t>(ty: &'t Ty, found: &mut Vec<&'t str>) {
    match ty {
        Ty::Struct(name) | Ty::Enum(name) => found.push(name),
        Ty::Nullable(inner) | Ty::Ptr(inner) | Ty::Array(inner, _) => held_types(inner, found),
        Ty::Result(ok, error) => {
            held_types(ok, found);
            held_types(error, found);
        }
        _ => {}
    }
}

/// The list a value is or leads to, behind borrows and pointers, and how many pointers.
fn list_behind(ty: &Ty) -> Option<(&Ty, usize)> {
    let mut ty = ty.strip_borrow();
    let mut boxes = 0;
    while let Ty::Ptr(inner) = ty {
        (ty, boxes) = (inner, boxes + 1);
    }
    matches!(ty, Ty::List(_)).then_some((ty, boxes))
}

/// The map or set a value is or leads to, behind borrows and pointers, and how many pointers.
fn table_behind(ty: &Ty) -> Option<(&Ty, usize)> {
    let mut ty = ty.strip_borrow();
    let mut boxes = 0;
    while let Ty::Ptr(inner) = ty {
        (ty, boxes) = (inner, boxes + 1);
    }
    matches!(ty, Ty::Map(..) | Ty::Set(_)).then_some((ty, boxes))
}

/// The table template for a map (`map`) or for a set: keeps the lines meant for it.
fn table_lines(map: bool) -> String {
    let mut out = String::new();
    let mut rest = TABLE;
    let (mut keep, mut stack) = (true, Vec::new());
    while let Some(at) = rest.find('@') {
        let (before, mark) = rest.split_at(at);
        if keep {
            out.push_str(before);
        }
        let open = mark.starts_with("@V{") || mark.starts_with("@S{");
        let close = mark.starts_with("@V}") || mark.starts_with("@S}");
        if open {
            stack.push(keep);
            keep = keep && (mark.starts_with("@V{") == map);
            rest = &mark[3..];
        } else if close {
            keep = stack.pop().expect("the template closes what it opened");
            rest = &mark[3..];
        } else {
            if keep {
                out.push('@');
            }
            rest = &mark[1..];
        }
    }
    if keep {
        out.push_str(rest);
    }
    out
}

/// A value that is written out in full where it is used: when it is computed cannot matter.
fn is_constant(expr: &Expr) -> bool {
    matches!(expr.kind, ExprKind::Int(..) | ExprKind::Bool(_) | ExprKind::Str(_) | ExprKind::Null)
}

/// What one variant of an enum carries: the name, the type and the place in the source of each value.
type Carried = Vec<(String, Ty, Span)>;

/// The tag an enum has after its value was moved out: no variant, so nothing is dropped.
const MOVED_TAG: usize = 255;

/// What computing an expression can do besides giving its value. It decides whether the
/// order in which two expressions are computed can be seen.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Effect {
    /// Nothing: it reads what is there.
    Reads,
    /// It can stop the program: a checked operation.
    Stops,
    /// It calls a function, which can print or stop, but cannot change what its neighbours read.
    Acts,
    /// It can change a value that an expression beside it reads: it lends with `ref`.
    Changes,
}

/// An index written `a + c` (see `Emitter::index_sum`): the sum, the type it is widened to, `a`,
/// `c`, whether `c` is written first, and the type of the sum.
struct IndexSum<'a> {
    sum: &'a Expr,
    widened: Option<&'a Ty>,
    other: &'a Expr,
    c: i128,
    first: bool,
    ty: IntTy,
}

/// One piece of a line that is being written.
enum Piece {
    /// A `printf` format and the values for it, each with its comma in front.
    Text(String, String),
    /// A value written by the helper of its type: the helper, a pointer to the value, and the
    /// width it takes (`{p,20}`; 0 when none is written).
    Show(String, String, i32),
}

/// One of several values that C would compute in an order of its own choosing: the arguments
/// of a call, the two sides of an operator, the fields of a literal.
struct Part {
    /// The C type of the value; empty if it cannot be kept in a variable (`panic(...)`).
    ctype: String,
    value: String,
    effect: Effect,
    /// A constant, or already kept in a variable: when it is computed cannot matter.
    settled: bool,
    /// The lines `pre` had before and after the value was written.
    pre: (usize, usize),
    /// Computing it can leave the function: it holds a `try` or a `?? return`.
    leaves: bool,
    span: Span,
}

/// The values that a call, a struct or an enum value is taking over, as far as they are written.
struct Taking {
    /// Whether the next value can still leave the function (see `Emitter::head`).
    head: bool,
    /// For each value kept under a name: the line that empties the name once it is handed over.
    handed: Vec<String>,
}

/// Where the value of a `match` goes.
#[derive(Clone)]
enum Dest {
    /// Nowhere: the `match` is a statement.
    Unused,
    /// Into a C variable that is already declared, as a value of this type.
    Place(String, Ty),
    /// Out of the function: `return match ...`.
    Return,
}

/// A variable of the function being written.
struct Local {
    name: String,
    /// Its name in C: no two variables of a function share one, so an inner variable never
    /// hides an outer one that still has to be dropped.
    cname: String,
    ty: Ty,
    /// The value was moved out on every way that leads here, so there is nothing to drop.
    /// Only ever set when that is certain; when in doubt the drop is written and finds the
    /// place empty.
    gone: bool,
    /// The line that frees it, when that is not the drop of its type: the list a `for` is
    /// going over has given away the items up to the one of this round.
    drop: Option<String>,
}

struct Emitter<'a> {
    sources: &'a Sources,
    /// The files of the program: a type name is looked up from the file it is written in.
    files: &'a [FileScope],
    structs: HashMap<&'a str, &'a StructDecl>,
    enums: HashMap<&'a str, &'a EnumDecl>,
    /// `using P = Point`: each alias by full name.
    aliases: HashMap<&'a str, &'a AliasDecl>,
    /// A debug build: overflow panics. Otherwise it wraps.
    checked: bool,
    /// The place of each line that can panic, as it is printed under the message.
    locs: Vec<String>,
    /// An index or an end of a range written `a + c` in a checked build: what the place's
    /// check needs to tell an overflow of the sum (see `sg_sum`), one `static` each.
    sums: Vec<String>,
    temps: usize,
    /// The variables in scope, innermost last.
    scopes: Vec<Vec<Local>>,
    /// For each loop around: how many scopes were open where it starts.
    loops: Vec<usize>,
    /// Lines that go before the statement being written: values kept in a temporary.
    pre: Vec<String>,
    /// Lines that go after it: those temporaries are dropped.
    post: Vec<String>,
    /// The C types: the structs, and a value-and-flag struct for each `T?` that is used.
    types: String,
    /// The helpers that drop, move and allocate each type that is used: first all their
    /// declarations, then their bodies, so that they can call each other.
    glue_protos: String,
    glue: String,
    /// The `T*` and `T?` types whose helpers are written.
    made: HashSet<String>,
    /// The parameter types of each function, by its C name.
    sigs: HashMap<String, Vec<Ty>>,
    /// What the function being written returns.
    ret: Ty,
    /// How many expressions that run only sometimes are being written (the right side of
    /// `??`, `&&` and `||`): a move in there is not certain.
    maybe: usize,
    /// An `is` with names after the first part of a chain is being written: the value it
    /// looks at, when a call makes it, is declared before the statement and made in the
    /// condition. The declarations, and the assignments that make the values.
    late: Option<(Vec<String>, Vec<String>)>,
    /// The expression about to be written is computed while nothing that would have to be
    /// freed is waiting in its statement: no value that owns something has been computed
    /// before it and is still unnamed. Only there can `try` and `?? return` leave the function,
    /// because what they free on the way out is exactly what has a name: the variables, and
    /// the temporaries made so far. `value` reads it and clears it; an expression hands it on
    /// to the part of itself that is computed first.
    head: bool,
    /// How many ways out of the function were written inside expressions.
    exits: usize,
    /// How many ways out of where the program is were written at all: `return`, `try`,
    /// `?? return`, `break` and `continue`. A `match` inside an expression is asked whether
    /// one of its arms leaves.
    ways_out: usize,
    /// A `test` block is being written: a `try` that meets an error writes it and ends the test.
    in_test: bool,
    /// What the entries of the hash tables are made of: written after every struct is whole.
    late_types: String,
    /// The copy of a loop that runs without the overflow test of its sums is being written:
    /// the statements that add to them, written without the test.
    unchecked: Vec<&'a Stmt>,
    /// In that copy, `break` leaves both copies: it goes to this label after them, and the
    /// flag says that one did.
    break_to: Option<(String, bool)>,
}

impl<'a> Emitter<'a> {
    /// The name of the C constant that holds where `span` is in the source.
    fn loc(&mut self, span: Span) -> String {
        self.locs.push(self.sources.snippet(span));
        format!("sg_loc_{}", self.locs.len() - 1)
    }

    /// An index or an end of a range in a checked build written `a + c` (or `c + a`), with `c`
    /// a constant and the sum of a signed type: the sum, wrapping, and the name of what tells
    /// its overflow where the place is checked (see `sg_sum` in the prelude). A sum that
    /// overflowed is negative, so the check of the place cannot miss it; nothing else is
    /// computed between the two. `None` for every other index, which is computed as it is.
    fn index_sum(&mut self, end: &'a Expr) -> Res<Option<(String, String)>> {
        let Some(IndexSum { sum, widened, other, c, first, ty }) = self.sum_parts(end) else { return Ok(None) };
        let value = self.value(other)?;
        let literal = int_literal(c, ty);
        let (a, b) = if first { (&literal, &value) } else { (&value, &literal) };
        let mut wrapped = format!("sg_wadd_{}({a}, {b})", int_name(ty));
        if let Some(widened) = widened {
            wrapped = format!("(({}){wrapped})", self.ctype(widened, end.span)?);
        }
        let loc = self.loc(sum.span);
        self.sums.push(format!("{{ {c}LL, \"{}\", {}, {}, {loc} }}", ty.name(), ty.bits(), first as u8));
        Ok(Some((wrapped, format!("sg_sum_{}", self.sums.len() - 1))))
    }

    /// The parts of an index that `index_sum` writes as a wrapping sum, if it is one.
    fn sum_parts(&self, end: &'a Expr) -> Option<IndexSum<'a>> {
        if !self.checked {
            return None;
        }
        let (sum, widened) = match &end.kind {
            ExprKind::Widen(ty, inner) => (&**inner, Some(ty)),
            _ => (end, None),
        };
        let ExprKind::Binary(BinOp::Add, left, right) = &sum.kind else { return None };
        let Ty::Int(ty) = sum.ty.strip_borrow() else { return None };
        let (other, c, first) = match (&left.kind, &right.kind) {
            (_, ExprKind::Int(c, _)) => (&**left, *c, false),
            (ExprKind::Int(c, _), _) => (&**right, *c, true),
            _ => return None,
        };
        (ty.signed() && c >= 0).then_some(IndexSum { sum, widened, other, c, first, ty: *ty })
    }

    /// An index as `index_sum` gives it where it can, and as it is otherwise.
    fn index_or_sum(&mut self, end: &'a Expr) -> Res<(String, Option<String>)> {
        Ok(match self.index_sum(end)? {
            Some((value, sum)) => (value, Some(sum)),
            None => (self.value(end)?, None),
        })
    }

    /// The ends of a range, the start first, and whether each may be a sum that the check of the
    /// range tells (`index_sum`). The start may be only where the end cannot stop the program or
    /// do anything else: computed between the two, the end would come before the overflow.
    /// An end that is such a sum is quiet too when what it adds to is: it wraps, and its overflow
    /// is told after that of the start.
    fn range_ends(&self, from: &'a Option<Box<Expr>>, to: &'a Option<Box<Expr>>) -> Vec<(&'a Expr, bool)> {
        let quiet = to.as_deref().is_none_or(|to| {
            let computed = self.sum_parts(to).map_or(to, |sum| sum.other);
            self.effect(computed) == Effect::Reads
        });
        from.as_deref().map(|from| (from, quiet)).into_iter().chain(to.as_deref().map(|to| (to, true))).collect()
    }

    fn temp(&mut self) -> String {
        self.temps += 1;
        format!("sg_t{}", self.temps)
    }

    fn text(&self, span: Span) -> &'a str {
        &self.sources.text[span.start..span.end]
    }

    // ---- types ----

    /// The C type of a value.
    fn ctype(&mut self, ty: &Ty, span: Span) -> Res<String> {
        let ty = flat(ty);
        Ok(match ty {
            Ty::Int(ty) => int_ctype(*ty).to_string(),
            Ty::Float(ty) => float_ctype(*ty).to_string(),
            Ty::Bool => "bool".to_string(),
            Ty::Char => "uint32_t".to_string(),
            Ty::TextIter(TextIter::ReadLines) | Ty::Reader | Ty::Writer => {
                self.made.insert("io".to_string());
                match ty {
                    Ty::Reader => "sg_reader",
                    Ty::Writer => "sg_writer",
                    _ => "sg_read_lines",
                }
                .to_string()
            }
            Ty::TextIter(_) => "sg_iter".to_string(),
            Ty::Void => "void".to_string(),
            // A `Formatter` is the text being written.
            Ty::Str | Ty::Formatter => "sg_string".to_string(),
            Ty::Struct(name) => format!("S_{}", cname(name)),
            Ty::Enum(name) => format!("E_{}", cname(name)),
            // A window is one C type whether it is read through or written through.
            Ty::Read(inner) | Ty::Ref(inner) if matches!(**inner, Ty::Slice(_)) => self.ctype(inner, span)?,
            Ty::Result(..) | Ty::List(_) | Ty::Func(..) | Ty::Map(..) | Ty::Set(_) | Ty::Entry(..) | Ty::Array(..) | Ty::Slice(_) => {
                // An entry is known where its map is.
                if let Ty::Entry(key, value) = ty {
                    self.need(&Ty::Map(key.clone(), value.clone()), span)?;
                }
                let Some(name) = mangle(ty) else { return Err(not_yet(span, &format!("the type `{ty}`"))) };
                self.need(ty, span)?;
                format!("sg_{name}")
            }
            _ if is_text_view(ty) => "sg_str".to_string(),
            Ty::Read(inner) if is_option(inner) => {
                let Ty::Nullable(held) = &**inner else { unreachable!("an option is a `T?`") };
                if **held == Ty::Str { "sg_str".to_string() } else { format!("{}*", self.ctype(held, span)?) }
            }
            Ty::Read(inner) | Ty::Ref(inner) => format!("{}*", self.ctype(inner, span)?),
            Ty::Ptr(inner) | Ty::Nullable(inner) => {
                let Some(name) = mangle(ty) else { return Err(not_yet(span, &format!("the type `{ty}`"))) };
                self.need(ty, span)?;
                match ty {
                    Ty::Ptr(_) => format!("{}*", self.ctype(inner, span)?),
                    _ if is_option(ty) => format!("sg_{name}"),
                    _ => self.ctype(inner, span)?,
                }
            }
            other => return Err(not_yet(span, &format!("the type `{other}`"))),
        })
    }

    /// A `read T?` with nothing in it.
    fn lent_none(&self, option: &Ty) -> &'static str {
        if matches!(option, Ty::Nullable(inner) if **inner == Ty::Str) { "SG_NO_STR" } else { "NULL" }
    }

    /// The C test that the `read T?` in `value` (a name) has something.
    fn lent_holds(&self, option: &Ty, value: &str) -> String {
        if matches!(option, Ty::Nullable(inner) if **inner == Ty::Str) { format!("({value}.len >= 0)") } else { format!("({value} != NULL)") }
    }

    /// A `read T?` made from a `T?` that is kept somewhere; `address`: a pointer to that `T?`.
    /// The test for the flag is made here, once, where it is lent.
    fn lend(&mut self, address: &str, option: &Ty, span: Span) -> Res<String> {
        let (ctype, temp) = (self.ctype(option, span)?, self.temp());
        let held = if self.lent_none(option) == "NULL" { format!("&{temp}->value") } else { format!("sg_view(&{temp}->value)") };
        Ok(format!("({{ {ctype}* {temp} = {address}; {temp}->has ? {held} : {}; }})", self.lent_none(option)))
    }

    /// The `T?` that a `read T?` stands for, as a value: a flag and a copy of the bytes that
    /// nobody drops. It is what a `T?` that can be copied gives (`int? n = limit`), and the
    /// way for the few places that read a `read T?` as a kept `T?`.
    fn unlent(&mut self, value: &str, option: &Ty, span: Span) -> Res<String> {
        let (ctype, lent, temp) = (self.ctype(option, span)?, self.ctype(&Ty::Read(Box::new(option.clone())), span)?, self.temp());
        let copy = if self.lent_none(option) == "NULL" { format!("*{temp}") } else { format!("{{ (uint8_t*){temp}.ptr, {temp}.len, 0 }}") };
        Ok(format!("({{ {lent} {temp} = {value}; {} ? (({ctype}){{ true, {copy} }}) : (({ctype}){{ false }}); }})", self.lent_holds(option, &temp)))
    }

    /// The same as a place, kept beside the statement.
    fn boxed(&mut self, value: &str, option: &Ty, span: Span) -> Res<String> {
        let (ctype, temp) = (self.ctype(option, span)?, self.temp());
        let made = self.unlent(value, option, span)?;
        self.pre.push(format!("{ctype} {temp};"));
        Ok(format!("(*({temp} = {made}, &{temp}))"))
    }

    /// Writes, once, what a `T*` or a `T?` needs: its C type and the helpers that drop it,
    /// move it and, for a `T*`, allocate it.
    fn need(&mut self, ty: &Ty, span: Span) -> Res<()> {
        let Some(name) = mangle(ty) else { return Ok(()) };
        match ty {
            Ty::Ptr(inner) => {
                if !self.made.insert(name.clone()) {
                    return Ok(());
                }
                let target = self.ctype(inner, span)?;
                let drop_inner = match self.droppable(inner) {
                    true => format!("sg_drop_{}(p); ", mangle(inner).expect("it has a C type")),
                    false => String::new(),
                };
                let _ = writeln!(self.glue_protos, "static inline {target}* sg_new_{name}({target} value);");
                let _ = writeln!(self.glue_protos, "static inline {target}* sg_move_{name}({target}** v);");
                let _ = writeln!(self.glue_protos, "static inline void sg_drop_{name}({target}** v);");
                let _ = writeln!(
                    self.glue,
                    "static inline {target}* sg_new_{name}({target} value) {{\n    {target}* p = malloc(sizeof({target}));\n    if (!p) abort();\n    *p = value;\n    return p;\n}}"
                );
                let _ = writeln!(self.glue, "static inline {target}* sg_move_{name}({target}** v) {{ {target}* out = *v; *v = NULL; return out; }}");
                // A struct with one pointer to its own kind is a chain: it is freed in a loop,
                // link by link, so a long list does not need a deep stack. The loop drops the
                // other fields of each link itself.
                let mut chain = None;
                if let Ty::Struct(of) = &**inner {
                    let decl: &'a StructDecl = self.structs[of.as_str()];
                    let is_link = |field: &FieldDecl| self.written(&field.ty).is_ok_and(|field| field == *ty || field == Ty::Nullable(Box::new(ty.clone())));
                    let mut links = decl.fields.iter().filter(|field| is_link(field));
                    if let (Some(link), None) = (links.next(), links.next()) {
                        let mut rest = String::new();
                        for field in decl.fields.iter().filter(|field| field.name != link.name) {
                            let field_ty = self.written(&field.ty)?;
                            if self.droppable(&field_ty) {
                                let _ = write!(rest, "{} ", self.drop_of(&field_ty, &format!("p->m_{}", field.name)));
                            }
                        }
                        chain = Some((link.name.clone(), rest));
                    }
                }
                let _ = match chain {
                    Some((link, rest)) => writeln!(
                        self.glue,
                        "static inline void sg_drop_{name}({target}** v) {{\n    {target}* p = *v;\n    while (p) {{ {target}* next = p->m_{link}; {rest}free(p); p = next; }}\n}}"
                    ),
                    None => writeln!(self.glue, "static inline void sg_drop_{name}({target}** v) {{ {target}* p = *v; if (p) {{ {drop_inner}free(p); }} }}"),
                };
            }
            Ty::Nullable(inner) if is_option(ty) => {
                if !self.made.insert(name.clone()) {
                    return Ok(());
                }
                let value = self.ctype(inner, span)?;
                let _ = writeln!(self.types, "typedef struct {{ bool has; {value} value; }} sg_{name};");
                // Taking the value out leaves `null`.
                let _ = writeln!(self.glue_protos, "static inline sg_{name} sg_move_{name}(sg_{name}* v);");
                let _ = writeln!(self.glue, "static inline sg_{name} sg_move_{name}(sg_{name}* v) {{ sg_{name} out = *v; v->has = false; return out; }}");
                if self.droppable(inner) {
                    let inner = mangle(inner).expect("it has a C type");
                    let _ = writeln!(self.glue_protos, "static inline void sg_drop_{name}(sg_{name}* v);");
                    let _ = writeln!(self.glue, "static inline void sg_drop_{name}(sg_{name}* v) {{ if (v->has) sg_drop_{inner}(&v->value); }}");
                }
            }
            Ty::Nullable(inner) => self.need(inner, span)?,
            // A hash table: the entries one after the other, and the slots that find them.
            Ty::Map(key, _) | Ty::Set(key) => {
                if !self.made.insert(name.clone()) {
                    return Ok(());
                }
                self.made.insert("tables".to_string());
                let value = match ty {
                    Ty::Map(_, value) => Some((**value).clone()),
                    _ => None,
                };
                let key_c = self.ctype(key, span)?;
                let value_c = match &value {
                    Some(value) => self.ctype(value, span)?,
                    None => String::new(),
                };
                // The entry is only pointed at by the table, so a struct can hold a map of
                // its own kind; what the entry is made of is written after all the structs.
                let _ = writeln!(self.types, "typedef struct sg_E_{name} sg_E_{name};");
                let _ = writeln!(self.types, "typedef struct {{ sg_E_{name}* entries; uint32_t* slots; int64_t len; int64_t cap; int64_t mask; }} sg_{name};");
                let held = if value.is_some() { format!(" {value_c} m_value;") } else { String::new() };
                let _ = writeln!(self.late_types, "struct sg_E_{name} {{ {key_c} m_key;{held} }};");
                // How a key is given, hashed, compared and written.
                let (given, hash, equal, of, show) = match &**key {
                    Ty::Int(_) | Ty::Bool | Ty::Char => (
                        key_c.clone(),
                        "sg_hash_word(0, (uint64_t)key)".to_string(),
                        "*held == key".to_string(),
                        "*held".to_string(),
                        match &**key {
                            Ty::Int(ty) => {
                                let (_, cast, put) = int_shown(*ty);
                                format!("{put}(out, ({cast})key);")
                            }
                            Ty::Char => "sg_put_quoted_char(out, key);".to_string(),
                            _ => "sg_puts(out, key ? \"true\" : \"false\");".to_string(),
                        },
                    ),
                    Ty::Str => (
                        "sg_str".to_string(),
                        "sg_hash_bytes(0, key.ptr, key.len)".to_string(),
                        "sg_eq(sg_view(held), key)".to_string(),
                        "sg_view(held)".to_string(),
                        "sg_put_quoted(out, key);".to_string(),
                    ),
                    _ => (
                        format!("{key_c} const*"),
                        format!("{}(key, 0)", self.hasher(key, span)?),
                        format!("{}(held, key)", self.equaler(key, span)?),
                        "held".to_string(),
                        format!("{}(out, key);", self.shower(key, span)?),
                    ),
                };
                let mut code = String::new();
                let _ = writeln!(code, "static inline uint64_t sg_khash_{name}({given} key) {{ return {hash}; }}");
                let _ = writeln!(code, "static inline bool sg_keq_{name}({key_c} const* held, {given} key) {{ return {equal}; }}");
                let _ = writeln!(code, "static inline {given} sg_kof_{name}({key_c} const* held) {{ return {of}; }}");
                let _ = writeln!(code, "static void sg_kshow_{name}(sg_string* out, {given} key) {{ {show} }}");
                let drop_key = if self.droppable(key) { self.drop_of(key, "$") } else { String::new() };
                let drop_value = match &value {
                    Some(value) if self.droppable(value) => self.drop_of(value, "$.m_value"),
                    _ => String::new(),
                };
                let mut lines = table_lines(value.is_some());
                // `@DROPE(x)` frees an entry, `@DROPK(x)` a key.
                for (mark, drop) in [("@DROPE(", format!("{} {drop_value}", drop_key.replace('$', "$.m_key"))), ("@DROPK(", drop_key.clone())] {
                    while let Some(at) = lines.find(mark) {
                        let end = at + lines[at..].find(')').expect("the mark is closed");
                        let place = lines[at + mark.len()..end].to_string();
                        lines.replace_range(at..=end, &drop.replace('$', &place));
                    }
                }
                code.push_str(&lines.replace("@N", &name).replace("@K", &key_c).replace("@P", &given).replace("@W", &value_c));
                // What frees one entry: the `for` that was handed the map owns them one by one.
                if value.is_some() && (!drop_key.is_empty() || !drop_value.is_empty()) {
                    let _ = writeln!(self.glue_protos, "static inline void sg_drop_E_{name}(sg_E_{name}* v);");
                    let _ = writeln!(code, "static inline void sg_drop_E_{name}(sg_E_{name}* v) {{ {} {} }}", drop_key.replace('$', "v->m_key"), drop_value.replace('$', "(*v)"));
                }
                let _ = writeln!(
                    self.glue_protos,
                    "static void sg_drop_from_{name}(sg_{name}* m, int64_t from);\nstatic inline void sg_drop_{name}(sg_{name}* m);\nstatic inline sg_{name} sg_move_{name}(sg_{name}* m);"
                );
                self.glue.push_str(&code);
            }
            // A function value is the address of a function: one pointer.
            Ty::Func(params, ret) => {
                if !self.made.insert(name.clone()) {
                    return Ok(());
                }
                let mut takes = Vec::new();
                for param in params {
                    takes.push(self.ctype(param, span)?);
                }
                let takes = if takes.is_empty() { "void".to_string() } else { takes.join(", ") };
                let gives = self.ctype(ret, span)?;
                let _ = writeln!(self.types, "typedef {gives} (*sg_{name})({takes});");
            }
            // A list: where its items are, how many there are and how many there is room for.
            // An empty list has no block. Only a pointer to the items is kept, so a struct
            // can hold a list of its own kind.
            Ty::List(item) => {
                if !self.made.insert(name.clone()) {
                    return Ok(());
                }
                let held = self.ctype(item, span)?;
                let list = format!("sg_{name}");
                let _ = writeln!(self.types, "typedef struct {{ {held}* ptr; int64_t len; int64_t cap; }} {list};");
                let drop_item = if self.droppable(item) { self.drop_of(item, "v->ptr[i]") } else { String::new() };
                let drop_items = |from: &str| match drop_item.is_empty() {
                    false => format!("for (int64_t i = {from}; i < v->len; i++) {drop_item} "),
                    true => String::new(),
                };
                let _ = writeln!(
                    self.glue_protos,
                    "static void sg_drop_from_{name}({list}* v, int64_t from);\nstatic inline void sg_drop_{name}({list}* v);\nstatic inline {list} sg_move_{name}({list}* v);"
                );
                // What a `for` that was handed the list leaves behind: the items from `from` on.
                let _ = writeln!(self.glue, "static void sg_drop_from_{name}({list}* v, int64_t from) {{ {}free(v->ptr); }}", drop_items("from"));
                let _ = writeln!(self.glue, "static inline void sg_drop_{name}({list}* v) {{ sg_drop_from_{name}(v, 0); }}");
                let _ = writeln!(self.glue, "static inline {list} sg_move_{name}({list}* v) {{ {list} out = *v; *v = ({list}){{ 0 }}; return out; }}");
                let _ = writeln!(
                    self.glue,
                    "static inline void sg_add_{name}({list}* v, {held} item) {{ if (__builtin_expect(v->len == v->cap, 0)) v->ptr = sg_grow(v->ptr, &v->cap, sizeof({held})); v->ptr[v->len++] = item; }}"
                );
                // An item, as the place it is in. The index is checked in every build.
                let _ = writeln!(
                    self.glue,
                    "static inline {held}* sg_at_{name}(const {list}* v, int64_t at, const char* loc) {{ if (__builtin_expect((uint64_t)at >= (uint64_t)v->len, 0)) sg_index_fail(at, v->len, loc); return &v->ptr[at]; }}"
                );
                // The same where the index is `a + c` (see `sg_sum`).
                let _ = writeln!(
                    self.glue,
                    "static inline {held}* sg_ats_{name}(const {list}* v, int64_t at, const sg_sum* sum, const char* loc) {{ if (__builtin_expect((uint64_t)at >= (uint64_t)v->len, 0)) sg_index_sum_fail(at, v->len, sum, loc); return &v->ptr[at]; }}"
                );
                let _ = writeln!(
                    self.glue,
                    "static inline void sg_reverse_{name}({list}* v) {{ for (int64_t i = 0, j = v->len - 1; i < j; i++, j--) {{ {held} item = v->ptr[i]; v->ptr[i] = v->ptr[j]; v->ptr[j] = item; }} }}"
                );
                let _ = writeln!(self.glue, "static inline void sg_clear_{name}({list}* v) {{ {}v->len = 0; }}", drop_items("0"));
                self.sigs.insert(format!("sg_add_{name}"), vec![(**item).clone()]);
            }
            // An array: its items, where the value is. C does not hand a bare array to a
            // function or give one back, so it is the one member of a struct.
            Ty::Array(item, length) => {
                if !self.made.insert(name.clone()) {
                    return Ok(());
                }
                let held = self.ctype(item, span)?;
                let array = format!("sg_{name}");
                let _ = writeln!(self.types, "typedef struct {{ {held} items[{length}]; }} {array};");
                // An item, as the place it is in. The index is checked in every build; where
                // it is a constant, the C compiler sees that the check never fails.
                let _ = writeln!(
                    self.glue,
                    "static inline {held}* sg_at_{name}({array}* v, int64_t at, const char* loc) {{ if (__builtin_expect((uint64_t)at >= (uint64_t){length}, 0)) sg_index_fail(at, {length}, loc); return &v->items[at]; }}"
                );
                let _ = writeln!(
                    self.glue,
                    "static inline {held}* sg_ats_{name}({array}* v, int64_t at, const sg_sum* sum, const char* loc) {{ if (__builtin_expect((uint64_t)at >= (uint64_t){length}, 0)) sg_index_sum_fail(at, {length}, sum, loc); return &v->items[at]; }}"
                );
                if self.droppable(item) {
                    let (drop, forget) = (self.drop_of(item, "v->items[i]"), self.move_of(item, "v->items[i]"));
                    let _ = writeln!(
                        self.glue_protos,
                        "static void sg_drop_from_{name}({array}* v, int64_t from);\nstatic inline void sg_drop_{name}({array}* v);\nstatic inline {array} sg_move_{name}({array}* v);"
                    );
                    // What a `for` that was handed the array leaves behind: the items from `from` on.
                    let _ = writeln!(self.glue, "static void sg_drop_from_{name}({array}* v, int64_t from) {{ for (int64_t i = from; i < {length}; i++) {drop} }}");
                    let _ = writeln!(self.glue, "static inline void sg_drop_{name}({array}* v) {{ sg_drop_from_{name}(v, 0); }}");
                    let _ = writeln!(self.glue, "static inline {array} sg_move_{name}({array}* v) {{ {array} out = *v; for (int64_t i = 0; i < {length}; i++) (void){forget}; return out; }}");
                }
            }
            // A window onto items that something else holds: where they are and how many.
            Ty::Slice(item) => {
                if !self.made.insert(name.clone()) {
                    return Ok(());
                }
                let held = self.ctype(item, span)?;
                let window = format!("sg_{name}");
                let _ = writeln!(self.types, "typedef struct {{ {held}* ptr; int64_t len; }} {window};");
                let _ = writeln!(
                    self.glue,
                    "static inline {held}* sg_at_{name}({window} v, int64_t at, const char* loc) {{ if (__builtin_expect((uint64_t)at >= (uint64_t)v.len, 0)) sg_index_fail(at, v.len, loc); return &v.ptr[at]; }}"
                );
                // A part of the window. An empty list has no block, and nothing is added to
                // a pointer that points nowhere.
                let _ = writeln!(
                    self.glue,
                    "static inline {window} sg_cut_{name}({window} v, int64_t from, int64_t to, const char* loc) {{ if (__builtin_expect(from < 0 || to > v.len || from > to, 0)) sg_span_fail(from, to, v.len, loc); return ({window}){{ from ? v.ptr + from : v.ptr, to - from }}; }}"
                );
                let _ = writeln!(self.glue, "static inline {window} sg_after_{name}({window} v, int64_t from, const char* loc) {{ return sg_cut_{name}(v, from, v.len, loc); }}");
                let _ = writeln!(self.glue, "static inline {window} sg_before_{name}({window} v, int64_t to, const char* loc) {{ return sg_cut_{name}(v, 0, to, loc); }}");
                // The same where an index or an end is `a + c` (see `sg_sum`).
                let _ = writeln!(
                    self.glue,
                    "static inline {held}* sg_ats_{name}({window} v, int64_t at, const sg_sum* sum, const char* loc) {{ if (__builtin_expect((uint64_t)at >= (uint64_t)v.len, 0)) sg_index_sum_fail(at, v.len, sum, loc); return &v.ptr[at]; }}"
                );
                let _ = writeln!(
                    self.glue,
                    "static inline {window} sg_cuts_{name}({window} v, int64_t from, int64_t to, const sg_sum* sf, const sg_sum* st, const char* loc) {{ if (__builtin_expect(from < 0 || to > v.len || from > to, 0)) sg_span_sum_fail(from, to, v.len, sf, st, loc); return ({window}){{ from ? v.ptr + from : v.ptr, to - from }}; }}"
                );
                let _ = writeln!(self.glue, "static inline {window} sg_afters_{name}({window} v, int64_t from, const sg_sum* sf, const char* loc) {{ return sg_cuts_{name}(v, from, v.len, sf, NULL, loc); }}");
                let _ = writeln!(self.glue, "static inline {window} sg_befores_{name}({window} v, int64_t to, const sg_sum* st, const char* loc) {{ return sg_cuts_{name}(v, 0, to, NULL, st, loc); }}");
            }
            // The same shape as an enum with the variants `Ok(T value)` and `Error(E cause)`,
            // so `match` and `is` read both in one way.
            Ty::Result(ok, error) => {
                if !self.made.insert(name.clone()) {
                    return Ok(());
                }
                let (mut members, mut drops) = (String::new(), Vec::new());
                for (index, (field, held)) in [("value", ok), ("cause", error)].into_iter().enumerate() {
                    if **held == Ty::Void {
                        continue;
                    }
                    let _ = write!(members, "struct {{ {} m_{field}; }} v{index}; ", self.ctype(held, span)?);
                    if self.droppable(held) {
                        drops.push(format!("if (v->tag == {index}) {{ {} }}", self.drop_of(held, &format!("v->u.v{index}.m_{field}"))));
                    }
                }
                let _ = writeln!(self.types, "typedef struct {{ uint8_t tag; union {{ {members}}} u; }} sg_{name};");
                if !drops.is_empty() {
                    let _ = writeln!(self.glue_protos, "static inline void sg_drop_{name}(sg_{name}* v);\nstatic inline sg_{name} sg_move_{name}(sg_{name}* v);");
                    let _ = writeln!(self.glue, "static inline void sg_drop_{name}(sg_{name}* v) {{ {} }}", drops.join(" else "));
                    let _ = writeln!(self.glue, "static inline sg_{name} sg_move_{name}(sg_{name}* v) {{ sg_{name} out = *v; v->tag = {MOVED_TAG}; return out; }}");
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// The full name of the type written as `name` in the file at `at`: with its module in
    /// front it is the full name already; otherwise a type of the file's own module, then one
    /// of a module the file names with `use`. The checker has seen that there is exactly one.
    fn full(&self, name: &str, at: Span) -> Option<String> {
        let known = |full: &str| self.structs.contains_key(full) || self.enums.contains_key(full) || self.aliases.contains_key(full);
        let file = self.files.iter().find(|file| at.start <= file.end)?;
        let own = if file.module.is_empty() || name.contains('.') { name.to_string() } else { format!("{}.{name}", file.module) };
        if known(&own) {
            return Some(own);
        }
        // The built-in `TextError` and `NumberError` are seen from every module with nothing
        // written.
        if name == "TextError" || name == "NumberError" {
            return Some(name.to_string());
        }
        let public = |full: &str| {
            let alias = || self.aliases.get(full).map(|decl| decl.public);
            self.structs.get(full).map(|decl| decl.public).or(self.enums.get(full).map(|decl| decl.public)).or_else(alias) == Some(true)
        };
        file.uses.iter().map(|(used, _)| format!("{used}.{name}")).find(|full| public(full))
    }

    /// A type as it is written in a signature or a field.
    fn written(&self, ty: &TypeExpr) -> Res<Ty> {
        let unknown = || not_yet(ty.span, &format!("the type `{}`", self.text(ty.span)));
        if ty.holder.is_some() {
            return Err(not_yet(ty.span, "a `shared` or a `weak` value"));
        }
        let mut found = match (Ty::from_name(&ty.name), ty.args.as_slice()) {
            (None, [ok, error]) if ty.name == "Result" => Ty::Result(Box::new(self.written(ok)?), Box::new(self.written(error)?)),
            (None, [item]) if ty.name == "List" => Ty::List(Box::new(self.written(item)?)),
            (None, [item]) if ty.name == "HashSet" => Ty::Set(Box::new(self.written(item)?)),
            (None, [key, value]) if ty.name == "HashMap" => Ty::Map(Box::new(self.written(key)?), Box::new(self.written(value)?)),
            (None, [params @ .., ret]) if ty.name == "Func" => {
                let params = params.iter().map(|param| self.written(param)).collect::<Res<Vec<Ty>>>()?;
                Ty::Func(params, Box::new(self.written(ret)?))
            }
            (Some(base @ (Ty::Int(_) | Ty::Float(_) | Ty::Bool | Ty::Char | Ty::Void | Ty::Str | Ty::Formatter | Ty::Reader | Ty::Writer)), []) => base,
            (None, []) => match self.full(&ty.name, ty.span) {
                // An alias is the type it names; the checker has seen that it ends.
                Some(full) if self.aliases.contains_key(full.as_str()) => self.written(&self.aliases[full.as_str()].ty)?,
                Some(full) if self.structs.contains_key(full.as_str()) => Ty::Struct(full),
                Some(full) => Ty::Enum(full),
                None => return Err(unknown()),
            },
            _ => return Err(unknown()),
        };
        for suffix in &ty.suffixes {
            found = match suffix {
                Suffix::Pointer => Ty::Ptr(Box::new(found)),
                Suffix::Nullable => Ty::Nullable(Box::new(found)),
                Suffix::Array(length) => Ty::Array(Box::new(found), *length),
                Suffix::Slice => Ty::Slice(Box::new(found)),
            };
            if mangle(&found).is_none() {
                return Err(unknown());
            }
        }
        Ok(match ty.mode {
            Some(Mode::Read) => Ty::Read(Box::new(found)),
            Some(Mode::Ref) => Ty::Ref(Box::new(found)),
            None => found,
        })
    }

    /// Whether a value of this type owns something that is freed when it ends.
    fn droppable(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Str | Ty::Ptr(_) | Ty::List(_) | Ty::Map(..) | Ty::Set(_) | Ty::Reader | Ty::Writer => true,
            Ty::Entry(key, value) => self.droppable(key) || self.droppable(value),
            Ty::Nullable(inner) | Ty::Array(inner, _) => self.droppable(inner),
            Ty::Result(ok, error) => self.droppable(ok) || self.droppable(error),
            Ty::Struct(name) => self.structs[name.as_str()].fields.iter().any(|field| self.written(&field.ty).is_ok_and(|ty| self.droppable(&ty))),
            Ty::Enum(name) => {
                let mut fields = self.enums[name.as_str()].variants.iter().flat_map(|variant| &variant.fields);
                fields.any(|field| self.written(&field.ty).is_ok_and(|ty| self.droppable(&ty)))
            }
            _ => false,
        }
    }

    /// The C statement that drops what is at `place`.
    fn drop_of(&self, ty: &Ty, place: &str) -> String {
        format!("sg_drop_{}(&{place});", mangle(ty).expect("what is dropped has a C type"))
    }

    /// The C expression that takes the value out of `place` and leaves it empty.
    fn move_of(&self, ty: &Ty, place: &str) -> String {
        format!("sg_move_{}(&{place})", mangle(ty).expect("what is moved has a C type"))
    }

    /// What `match` and `is` see of an enum or a `Result`: for each variant, the name and the
    /// type of every value it carries. `None` for any other type. `span`: where to point for
    /// a `Result`, whose variants are written nowhere.
    fn variants(&self, ty: &Ty, span: Span) -> Res<Option<Vec<Carried>>> {
        match ty {
            Ty::Enum(name) => {
                let decl = self.enums[name.as_str()];
                let fields = |variant: &VariantDecl| variant.fields.iter().map(|field| Ok((field.name.clone(), self.written(&field.ty)?, field.ty.span))).collect();
                Ok(Some(decl.variants.iter().map(fields).collect::<Res<_>>()?))
            }
            Ty::Result(ok, error) => {
                let value = if **ok == Ty::Void { Vec::new() } else { vec![("value".to_string(), (**ok).clone(), span)] };
                Ok(Some(vec![value, vec![("cause".to_string(), (**error).clone(), span)]]))
            }
            _ => Ok(None),
        }
    }

    /// Whether a struct or an enum wrote this method itself.
    fn own_method(&self, owner: &str, method: &str) -> bool {
        let methods = match self.structs.get(owner) {
            Some(decl) => &decl.methods,
            None => &self.enums[owner].methods,
        };
        methods.iter().any(|func| func.name == method)
    }

    /// The helper that writes a value of this type as text into a `Formatter`, written once:
    /// a struct field by field (or by its own `format`), an enum as its variant, a list
    /// between brackets. Text inside is quoted.
    fn shower(&mut self, ty: &Ty, span: Span) -> Res<String> {
        if let Ty::Nullable(inner) = ty {
            if matches!(**inner, Ty::Ptr(_)) {
                return self.shower(inner, span);
            }
        }
        let name = mangle(ty).ok_or_else(|| not_yet(span, &format!("writing a `{ty}` as text")))?;
        let helper = format!("sg_show_{name}");
        let ctype = self.ctype(ty, span)?;
        if !self.made.insert(helper.clone()) {
            return Ok(helper);
        }
        let _ = writeln!(self.glue_protos, "static void {helper}(sg_string* out, {ctype} const* v);");
        let mut body = String::new();
        match ty {
            Ty::Int(ty) => {
                let (_, cast, put) = int_shown(*ty);
                let _ = write!(body, "{put}(out, ({cast})*v);");
            }
            Ty::Float(ty) => {
                let _ = write!(body, "{}(out, *v);", float_put(*ty));
            }
            Ty::Bool => body.push_str("if (*v) sg_put(out, \"true\", 4); else sg_put(out, \"false\", 5);"),
            Ty::Char => body.push_str("sg_put_quoted_char(out, *v);"),
            Ty::Str => body.push_str("sg_put_quoted(out, sg_view(v));"),
            // A `T*` and a `T*?` are one C type and share this helper, so it knows `null`.
            Ty::Ptr(inner) => {
                let _ = write!(body, "if (*v) {}(out, *v); else sg_put(out, \"null\", 4);", self.shower(inner, span)?);
            }
            Ty::Nullable(inner) if is_option(ty) => {
                let _ = write!(body, "if (v->has) {}(out, &v->value); else sg_put(out, \"null\", 4);", self.shower(inner, span)?);
            }
            Ty::Nullable(_) => unreachable!("a `T*?` has the name of the `T*`"),
            Ty::Struct(owner) | Ty::Enum(owner) if self.own_method(owner, "format") => {
                let _ = write!(body, "f_{}_format(({ctype}*)v, out);", cname(owner));
            }
            Ty::Struct(owner) => {
                let decl: &'a StructDecl = self.structs[owner.as_str()];
                body.push_str(&put_text(&format!("{} {{ ", short_name(owner))));
                for (position, field) in decl.fields.iter().enumerate() {
                    let field_ty = self.written(&field.ty)?;
                    let text = format!("{}{} = ", if position == 0 { "" } else { ", " }, field.name);
                    let _ = write!(body, " {} {}", put_text(&text), self.show_at(&field_ty, &format!("v->m_{}", field.name), span)?);
                }
                body.push_str(" sg_put(out, \" }\", 2);");
            }
            Ty::Enum(_) | Ty::Result(..) => {
                let variants = self.variants(ty, span)?.expect("an enum or a `Result`");
                let words: Vec<String> = match ty {
                    Ty::Enum(owner) => self.enums[owner.as_str()].variants.iter().map(|variant| variant.name.clone()).collect(),
                    _ => vec!["Ok".to_string(), "Error".to_string()],
                };
                body.push_str("switch (v->tag) {");
                for (index, (word, fields)) in words.iter().zip(&variants).enumerate() {
                    let _ = write!(body, " case {index}: {}", put_text(&format!("{word}{}", if fields.is_empty() { "" } else { "(" })));
                    for (position, (field, field_ty, _)) in fields.iter().enumerate() {
                        if position > 0 {
                            body.push_str(" sg_put(out, \", \", 2);");
                        }
                        let _ = write!(body, " {}", self.show_at(field_ty, &format!("v->u.v{index}.m_{field}"), span)?);
                    }
                    body.push_str(if fields.is_empty() { " break;" } else { " sg_put(out, \")\", 1); break;" });
                }
                body.push_str(" default: break; }");
            }
            Ty::List(item) => {
                let _ = write!(
                    body,
                    "sg_put(out, \"[\", 1); for (int64_t i = 0; i < v->len; i++) {{ if (i) sg_put(out, \", \", 2); {}(out, &v->ptr[i]); }} sg_put(out, \"]\", 1);",
                    self.shower(item, span)?
                );
            }
            // An array and a window are written like a list.
            Ty::Array(item, _) | Ty::Slice(item) => {
                let (count, items) = match ty {
                    Ty::Array(_, length) => (length.to_string(), "items"),
                    _ => ("v->len".to_string(), "ptr"),
                };
                let _ = write!(
                    body,
                    "sg_put(out, \"[\", 1); for (int64_t i = 0; i < {count}; i++) {{ if (i) sg_put(out, \", \", 2); {}(out, &v->{items}[i]); }} sg_put(out, \"]\", 1);",
                    self.shower(item, span)?
                );
            }
            Ty::Map(key, value) => {
                let (key, value) = (self.show_at(key, "v->entries[i].m_key", span)?, self.show_at(value, "v->entries[i].m_value", span)?);
                let _ = write!(body, "sg_put(out, \"{{\", 1); for (int64_t i = 0; i < v->len; i++) {{ if (i) sg_put(out, \", \", 2); {key} sg_put(out, \": \", 2); {value} }} sg_put(out, \"}}\", 1);");
            }
            Ty::Set(key) => {
                let key = self.show_at(key, "v->entries[i].m_key", span)?;
                let _ = write!(body, "sg_put(out, \"{{\", 1); for (int64_t i = 0; i < v->len; i++) {{ if (i) sg_put(out, \", \", 2); {key} }} sg_put(out, \"}}\", 1);");
            }
            other => return Err(not_yet(span, &format!("writing a `{other}` as text"))),
        }
        let _ = writeln!(self.glue, "static void {helper}(sg_string* out, {ctype} const* v) {{ {body} }}");
        Ok(helper)
    }

    /// The C statement that writes what is at `place`: a field of a struct or of a variant.
    /// A field that borrows is written like the value it points at.
    fn show_at(&mut self, ty: &Ty, place: &str, span: Span) -> Res<String> {
        match ty {
            _ if is_text_view(ty) => Ok(format!("sg_put_quoted(out, {place});")),
            // A window is a value of its own: it is written from where it is kept.
            _ if is_window(ty) => Ok(format!("{}(out, &{place});", self.shower(flat(ty).strip_borrow(), span)?)),
            Ty::Read(inner) if is_option(inner) => {
                let Ty::Nullable(held) = &**inner else { unreachable!("an option is a `T?`") };
                let shown = self.show_at(&Ty::Read(held.clone()), place, span)?;
                Ok(format!("if ({}) {{ {shown} }} else sg_put(out, \"null\", 4);", self.lent_holds(inner, place)))
            }
            Ty::Read(inner) | Ty::Ref(inner) => Ok(format!("{}(out, {place});", self.shower(inner, span)?)),
            _ => Ok(format!("{}(out, &{place});", self.shower(ty, span)?)),
        }
    }

    /// The C test that what is at `a` and at `b` is equal; see `show_at`.
    fn same_at(&mut self, ty: &Ty, a: &str, b: &str, span: Span) -> Res<String> {
        match ty {
            _ if is_text_view(ty) => Ok(format!("sg_eq({a}, {b})")),
            _ if is_window(ty) => Ok(format!("{}(&{a}, &{b})", self.equaler(flat(ty).strip_borrow(), span)?)),
            Ty::Read(inner) if is_option(inner) => {
                let Ty::Nullable(held) = &**inner else { unreachable!("an option is a `T?`") };
                let same = self.same_at(&Ty::Read(held.clone()), a, b, span)?;
                let (has_a, has_b) = (self.lent_holds(inner, a), self.lent_holds(inner, b));
                Ok(format!("({has_a} == {has_b} && (!{has_a} || {same}))"))
            }
            Ty::Read(inner) | Ty::Ref(inner) => Ok(format!("{}({a}, {b})", self.equaler(inner, span)?)),
            _ => Ok(format!("{}(&{a}, &{b})", self.equaler(ty, span)?)),
        }
    }

    /// The helper that tells whether two values of this type are equal, written once: a
    /// struct by its own `equals` or field by field, an enum by its variant and what it
    /// carries, a list item by item.
    fn equaler(&mut self, ty: &Ty, span: Span) -> Res<String> {
        if let Ty::Nullable(inner) = ty {
            if matches!(**inner, Ty::Ptr(_)) {
                return self.equaler(inner, span);
            }
        }
        let name = mangle(ty).ok_or_else(|| not_yet(span, &format!("`==` on `{ty}`")))?;
        let helper = format!("sg_same_{name}");
        let ctype = self.ctype(ty, span)?;
        if !self.made.insert(helper.clone()) {
            return Ok(helper);
        }
        let _ = writeln!(self.glue_protos, "static bool {helper}({ctype} const* a, {ctype} const* b);");
        let mut body = String::new();
        match ty {
            Ty::Int(_) | Ty::Bool | Ty::Char => body.push_str("return *a == *b;"),
            Ty::Float(_) => body.push_str("return sg_feq(*a, *b);"),
            Ty::Str => body.push_str("return sg_eq(sg_view(a), sg_view(b));"),
            Ty::Ptr(inner) => {
                let _ = write!(body, "return (*a == NULL) == (*b == NULL) && (*a == NULL || {}(*a, *b));", self.equaler(inner, span)?);
            }
            Ty::Nullable(inner) if is_option(ty) => {
                let _ = write!(body, "return a->has == b->has && (!a->has || {}(&a->value, &b->value));", self.equaler(inner, span)?);
            }
            Ty::Nullable(_) => unreachable!("a `T*?` has the name of the `T*`"),
            Ty::Struct(owner) | Ty::Enum(owner) if self.own_method(owner, "equals") => {
                let _ = write!(body, "return f_{}_equals(({ctype}*)a, ({ctype}*)b);", cname(owner));
            }
            Ty::Struct(owner) => {
                let decl: &'a StructDecl = self.structs[owner.as_str()];
                for field in &decl.fields {
                    let field_ty = self.written(&field.ty)?;
                    let _ = write!(body, "if (!{}) return false; ", self.same_at(&field_ty, &format!("a->m_{}", field.name), &format!("b->m_{}", field.name), span)?);
                }
                body.push_str("return true;");
            }
            Ty::Enum(_) | Ty::Result(..) => {
                let variants = self.variants(ty, span)?.expect("an enum or a `Result`");
                body.push_str("if (a->tag != b->tag) return false; switch (a->tag) {");
                for (index, fields) in variants.iter().enumerate() {
                    if fields.is_empty() {
                        continue;
                    }
                    let _ = write!(body, " case {index}:");
                    for (field, field_ty, _) in fields {
                        let _ = write!(body, " if (!{}) return false;", self.same_at(field_ty, &format!("a->u.v{index}.m_{field}"), &format!("b->u.v{index}.m_{field}"), span)?);
                    }
                    body.push_str(" break;");
                }
                body.push_str(" default: break; } return true;");
            }
            Ty::List(item) => {
                let _ = write!(
                    body,
                    "if (a->len != b->len) return false; for (int64_t i = 0; i < a->len; i++) if (!{}(&a->ptr[i], &b->ptr[i])) return false; return true;",
                    self.equaler(item, span)?
                );
            }
            Ty::Array(item, length) => {
                let _ = write!(body, "for (int64_t i = 0; i < {length}; i++) if (!{}(&a->items[i], &b->items[i])) return false; return true;", self.equaler(item, span)?);
            }
            // Two windows are equal when they show the same items, wherever those are kept.
            Ty::Slice(item) => {
                let _ = write!(
                    body,
                    "if (a->len != b->len) return false; for (int64_t i = 0; i < a->len; i++) if (!{}(&a->ptr[i], &b->ptr[i])) return false; return true;",
                    self.equaler(item, span)?
                );
            }
            // Two tables are equal when they hold the same, in whatever order.
            Ty::Map(_, value) => {
                let same = self.same_at(value, "a->entries[i].m_value", "b->entries[at].m_value", span)?;
                let _ = write!(
                    body,
                    "if (a->len != b->len) return false; for (int64_t i = 0; i < a->len; i++) {{ int64_t at = sg_find_{name}(b, sg_kof_{name}(&a->entries[i].m_key)); if (at < 0 || !{same}) return false; }} return true;"
                );
            }
            Ty::Set(_) => {
                let _ = write!(
                    body,
                    "if (a->len != b->len) return false; for (int64_t i = 0; i < a->len; i++) if (sg_find_{name}(b, sg_kof_{name}(&a->entries[i].m_key)) < 0) return false; return true;"
                );
            }
            other => return Err(not_yet(span, &format!("`==` on `{other}`"))),
        }
        let _ = writeln!(self.glue, "static bool {helper}({ctype} const* a, {ctype} const* b) {{ {body} }}");
        Ok(helper)
    }

    /// A pointer to a value that is only looked at: `read x`, or a value kept for the statement.
    fn pointer(&mut self, expr: &'a Expr) -> Res<String> {
        // A window is not a pointer to its items: it is pointed at where it is kept.
        if expr.ty.is_borrow() && !expr.is_place() && !is_window(&expr.ty) {
            return self.value(expr);
        }
        Ok(format!("(&{})", self.look(expr)?))
    }

    /// The items an expression is, borrows or points at, as a window: where they are and how
    /// many. A window is given as it is; a list or an array gives a window onto all of it.
    fn window(&mut self, expr: &'a Expr) -> Res<String> {
        if is_window(&expr.ty) {
            return self.value(expr);
        }
        let mut core = expr.ty.strip_borrow();
        while let Ty::Ptr(inner) = core {
            core = inner;
        }
        let (Ty::List(item) | Ty::Array(item, _)) = core else { return Err(not_yet(expr.span, &format!("a `{}` used as a slice", expr.ty))) };
        let slice = Ty::Slice(item.clone());
        let ctype = self.ctype(&slice, expr.span)?;
        let from = self.ctype(core, expr.span)?;
        let address = self.list_address(expr)?;
        match core {
            Ty::Array(_, length) => Ok(format!("(({ctype}){{ {address}->items, {length} }})")),
            _ => {
                let helper = format!("sg_all_{}", mangle(core).expect("it has a C type"));
                if self.made.insert(helper.clone()) {
                    let _ = writeln!(self.glue, "static inline {ctype} {helper}(const {from}* v) {{ return ({ctype}){{ v->ptr, v->len }}; }}");
                }
                Ok(format!("{helper}({address})"))
            }
        }
    }

    /// Writes, once, the helpers of a window that are needed only where they are used: `get`
    /// (it gives a look at an item, or `null`) and the search behind `indexOf` and `contains`.
    fn window_helper(&mut self, slice: &Ty, what: &str, span: Span) -> Res<String> {
        let Ty::Slice(item) = slice else { unreachable!("only called for a slice") };
        let missing = || not_yet(span, &format!("`{what}` on a `{slice}`"));
        let name = mangle(slice).ok_or_else(missing)?;
        let helper = format!("sg_{what}_{name}");
        let (window, held) = (self.ctype(slice, span)?, self.ctype(item, span)?);
        if !self.made.insert(helper.clone()) {
            return Ok(helper);
        }
        match what {
            "get" => {
                let option = Ty::Nullable(item.clone());
                if mangle(&option).is_none() {
                    return Err(missing());
                }
                let option_c = self.ctype(&option, span)?;
                let _ = match is_option(&option) {
                    // A look at the item where it is: a pointer to it, or none.
                    true => {
                        let lent_c = self.ctype(&Ty::Read(Box::new(option.clone())), span)?;
                        let at = if **item == Ty::Str { "sg_view(&v.ptr[at])" } else { "&v.ptr[at]" };
                        writeln!(
                            self.glue,
                            "static inline {lent_c} {helper}({window} v, int64_t at) {{ if ((uint64_t)at >= (uint64_t)v.len) return {}; return {at}; }}",
                            self.lent_none(&option)
                        )
                    }
                    false => writeln!(self.glue, "static inline {option_c} {helper}({window} v, int64_t at) {{ if ((uint64_t)at >= (uint64_t)v.len) return NULL; return v.ptr[at]; }}"),
                };
                self.sigs.insert(helper.clone(), vec![Ty::Int(IntTy::Nint)]);
            }
            // The position of the first item that is equal, or -1.
            "index" => {
                let (given, equal) = match &**item {
                    Ty::Int(_) | Ty::Bool | Ty::Char => (held.clone(), "v.ptr[i] == item".to_string()),
                    Ty::Float(_) => (held.clone(), "sg_feq(v.ptr[i], item)".to_string()),
                    Ty::Str => ("sg_str".to_string(), "sg_eq(sg_view(&v.ptr[i]), item)".to_string()),
                    _ => (format!("{held} const*"), format!("{}(&v.ptr[i], item)", self.equaler(item, span)?)),
                };
                let _ = writeln!(
                    self.glue,
                    "static inline int64_t {helper}({window} v, {given} item) {{ for (int64_t i = 0; i < v.len; i++) if ({equal}) return i; return -1; }}"
                );
                let key = if **item == Ty::Str { Ty::Read(item.clone()) } else { (**item).clone() };
                self.sigs.insert(helper.clone(), vec![key]);
            }
            // The items in order where they are, in the list or the array that the window
            // looks at; the same merge sort as a list's.
            "sort" => {
                let compare = self.item_compare(item, slice, span)?;
                let _ = writeln!(self.glue, "static void {helper}({window} v) {{ sg_sort(v.ptr, v.len, sizeof({held}), {compare}); }}");
                self.sigs.insert(helper.clone(), Vec::new());
            }
            "reverse" => {
                let _ = writeln!(
                    self.glue,
                    "static inline void {helper}({window} v) {{ for (int64_t i = 0, j = v.len - 1; i < j; i++, j--) {{ {held} item = v.ptr[i]; v.ptr[i] = v.ptr[j]; v.ptr[j] = item; }} }}"
                );
                self.sigs.insert(helper.clone(), Vec::new());
            }
            _ => unreachable!("the helpers are named here"),
        }
        Ok(helper)
    }

    /// The test `l op r` on two pointers to values of a struct, an enum, a `Result` or a list:
    /// `==` and `!=` by the helper of the type, the others by the `compare` the struct wrote.
    fn compared(&mut self, op: BinOp, ty: &Ty, l: &str, r: &str, span: Span) -> Res<String> {
        match (op, ty) {
            (BinOp::Eq, _) => Ok(format!("{}({l}, {r})", self.equaler(ty, span)?)),
            (BinOp::Ne, _) => Ok(format!("(!{}({l}, {r}))", self.equaler(ty, span)?)),
            (_, Ty::Struct(owner) | Ty::Enum(owner)) if self.own_method(owner, "compare") => Ok(format!("(f_{}_compare({l}, {r}) {} 0)", cname(owner), op.symbol())),
            _ => Err(not_yet(span, &format!("`{}` on `{ty}`", op.symbol()))),
        }
    }

    /// The helper that hashes a key of this type on top of the hash `h` of what came before
    /// it, written once: a struct field after field, an enum its variant and what it carries.
    fn hasher(&mut self, ty: &Ty, span: Span) -> Res<String> {
        let name = mangle(ty).ok_or_else(|| not_yet(span, &format!("a `{ty}` as a key")))?;
        let helper = format!("sg_hash_{name}");
        let ctype = self.ctype(ty, span)?;
        if !self.made.insert(helper.clone()) {
            return Ok(helper);
        }
        let _ = writeln!(self.glue_protos, "static uint64_t {helper}({ctype} const* v, uint64_t h);");
        let mut body = String::new();
        match ty {
            Ty::Int(_) | Ty::Bool | Ty::Char => body.push_str("return sg_hash_word(h, (uint64_t)*v);"),
            Ty::Str => body.push_str("return sg_hash_bytes(h, v->ptr, v->len);"),
            Ty::Nullable(inner) if is_option(ty) => {
                let _ = write!(body, "return v->has ? {}(&v->value, sg_hash_word(h, 1)) : sg_hash_word(h, 0);", self.hasher(inner, span)?);
            }
            Ty::Struct(owner) => {
                let decl: &'a StructDecl = self.structs[owner.as_str()];
                for field in &decl.fields {
                    let field_ty = self.written(&field.ty)?;
                    let _ = write!(body, "h = {}(&v->m_{}, h); ", self.hasher(&field_ty, span)?, field.name);
                }
                body.push_str("return h;");
            }
            Ty::Enum(_) => {
                let variants = self.variants(ty, span)?.expect("an enum");
                body.push_str("h = sg_hash_word(h, v->tag); switch (v->tag) {");
                for (index, fields) in variants.iter().enumerate().filter(|(_, fields)| !fields.is_empty()) {
                    let _ = write!(body, " case {index}:");
                    for (field, field_ty, _) in fields {
                        let _ = write!(body, " h = {}(&v->u.v{index}.m_{field}, h);", self.hasher(field_ty, span)?);
                    }
                    body.push_str(" break;");
                }
                body.push_str(" default: break; } return h;");
            }
            other => return Err(not_yet(span, &format!("a `{other}` as a key"))),
        }
        let _ = writeln!(self.glue, "static uint64_t {helper}({ctype} const* v, uint64_t h) {{ {body} }}");
        Ok(helper)
    }

    /// A key as a table takes it to look something up: a number as it is, text as a look at
    /// it, anything else through a pointer.
    fn key(&mut self, table: &Ty, arg: &'a Expr) -> Res<String> {
        let (Ty::Map(key, _) | Ty::Set(key)) = table else { unreachable!("only called for a table") };
        match &**key {
            Ty::Int(_) | Ty::Bool | Ty::Char => self.value(arg),
            Ty::Str => self.view(arg),
            _ => self.pointer(arg),
        }
    }

    /// Writes, once, the helpers of a table that give a `V?` (`get` and `remove` of a map) and
    /// the one that needs a copy of the key (`getOrAdd`).
    fn table_helper(&mut self, table: &Ty, what: &str, span: Span) -> Res<String> {
        let Ty::Map(key, value) = table else { return Err(not_yet(span, &format!("`{what}` on a `{table}`"))) };
        let name = mangle(table).ok_or_else(|| not_yet(span, &format!("the type `{table}`")))?;
        let helper = format!("sg_{what}_{name}");
        let (key_c, value_c) = (self.ctype(key, span)?, self.ctype(value, span)?);
        if !self.made.insert(helper.clone()) {
            return Ok(helper);
        }
        let given = match &**key {
            Ty::Int(_) | Ty::Bool | Ty::Char => key_c.clone(),
            Ty::Str => "sg_str".to_string(),
            _ => format!("{key_c} const*"),
        };
        if what == "getOrAdd" {
            // The key is only lent; if it has to be kept, the table keeps a copy.
            let copy = match &**key {
                Ty::Int(_) | Ty::Bool | Ty::Char => "key",
                Ty::Str => "sg_clone(key)",
                other if !self.droppable(other) => "*key",
                other => return Err(not_yet(span, &format!("`getOrAdd` with a key of the type `{other}`"))),
            };
            let drop = if self.droppable(value) { self.drop_of(value, "value") } else { String::new() };
            let _ = writeln!(
                self.glue,
                "static inline {value_c}* {helper}(sg_{name}* m, {given} key, {value_c} value) {{ int64_t entry = sg_find_{name}(m, key); \
                 if (entry >= 0) {{ {drop} return &m->entries[entry].m_value; }} sg_put_{name}(m, {copy}, value); return &m->entries[m->len - 1].m_value; }}"
            );
            return Ok(helper);
        }
        let option = Ty::Nullable(value.clone());
        if mangle(&option).is_none() {
            return Err(not_yet(span, &format!("`{what}` on a `{table}`")));
        }
        let option_c = self.ctype(&option, span)?;
        let (some, none) = match is_option(&option) {
            true => (format!("({option_c}){{ true, m->entries[entry].m_value }}"), format!("({option_c}){{ false }}")),
            false => ("m->entries[entry].m_value".to_string(), "NULL".to_string()),
        };
        // The key is freed once the entry is out: taking it out reads the key.
        let drop_key = if self.droppable(key) { self.drop_of(key, "gone") } else { String::new() };
        let _ = match what {
            // A look at the value: the map still owns it.
            // A look at the value where it is: a pointer to it, or none.
            "get" if is_option(&option) => {
                let lent_c = self.ctype(&Ty::Read(Box::new(option.clone())), span)?;
                let at = if **value == Ty::Str { "sg_view(&m->entries[entry].m_value)" } else { "&m->entries[entry].m_value" };
                writeln!(
                    self.glue,
                    "static inline {lent_c} {helper}(const sg_{name}* m, {given} key) {{ int64_t entry = sg_find_{name}(m, key); if (entry < 0) return {}; return {at}; }}",
                    self.lent_none(&option)
                )
            }
            "get" => writeln!(
                self.glue,
                "static inline {option_c} {helper}(const sg_{name}* m, {given} key) {{ int64_t entry = sg_find_{name}(m, key); if (entry < 0) return {none}; return {some}; }}"
            ),
            _ => writeln!(
                self.glue,
                "static inline {option_c} {helper}(sg_{name}* m, {given} key) {{ int64_t entry = sg_find_{name}(m, key); if (entry < 0) return {none}; \
                 {option_c} out = {some}; {key_c} gone = m->entries[entry].m_key; sg_take_{name}(m, entry); {drop_key} (void)gone; return out; }}"
            ),
        };
        Ok(helper)
    }

    /// Writes, once, the helpers of a list that are needed only where they are used: `get`
    /// and `removeLast` (they give a `T?`), and searching and sorting for the items that can
    /// be compared here (integers, `bool` and text). `what`: the helper asked for.
    fn list_helper(&mut self, list: &Ty, what: &str, span: Span) -> Res<String> {
        let Ty::List(item) = list else { unreachable!("only called for a list") };
        let missing = || not_yet(span, &format!("`{what}` on a `{list}`"));
        let name = mangle(list).ok_or_else(missing)?;
        let helper = format!("sg_{what}_{name}");
        let (list_c, held) = (self.ctype(list, span)?, self.ctype(item, span)?);
        if !self.made.insert(helper.clone()) {
            return Ok(helper);
        }
        let nint = Ty::Int(IntTy::Nint);
        match what {
            "get" | "pop" => {
                let option = Ty::Nullable(item.clone());
                if mangle(&option).is_none() {
                    return Err(missing());
                }
                let option_c = self.ctype(&option, span)?;
                let (some, none) = match is_option(&option) {
                    true => (format!("({option_c}){{ true, {{}} }}"), format!("({option_c}){{ false }}")),
                    false => ("{}".to_string(), "NULL".to_string()),
                };
                let _ = match what {
                    // What is given is a look at the item where it is: a pointer to it, or none.
                    "get" if is_option(&option) => {
                        let lent_c = self.ctype(&Ty::Read(Box::new(option.clone())), span)?;
                        let at = if **item == Ty::Str { "sg_view(&v->ptr[at])" } else { "&v->ptr[at]" };
                        writeln!(
                            self.glue,
                            "static inline {lent_c} {helper}(const {list_c}* v, int64_t at) {{ if ((uint64_t)at >= (uint64_t)v->len) return {}; return {at}; }}",
                            self.lent_none(&option)
                        )
                    }
                    "get" => writeln!(
                        self.glue,
                        "static inline {option_c} {helper}(const {list_c}* v, int64_t at) {{ if ((uint64_t)at >= (uint64_t)v->len) return {none}; return {}; }}",
                        some.replace("{}", "v->ptr[at]")
                    ),
                    _ => writeln!(
                        self.glue,
                        "static inline {option_c} {helper}({list_c}* v) {{ if (v->len == 0) return {none}; v->len--; return {}; }}",
                        some.replace("{}", "v->ptr[v->len]")
                    ),
                };
                self.sigs.insert(helper.clone(), if what == "get" { vec![nint] } else { Vec::new() });
            }
            // The position of the first item that is equal, or -1.
            "index" => {
                let (given, equal) = match &**item {
                    Ty::Int(_) | Ty::Bool | Ty::Char => (held.clone(), "v->ptr[i] == item".to_string()),
                    Ty::Float(_) => (held.clone(), "sg_feq(v->ptr[i], item)".to_string()),
                    Ty::Str => ("sg_str".to_string(), "sg_eq(sg_view(&v->ptr[i]), item)".to_string()),
                    // Any other item is compared through a pointer, by the helper of its type.
                    _ => (format!("{held} const*"), format!("{}(&v->ptr[i], item)", self.equaler(item, span)?)),
                };
                let _ = writeln!(
                    self.glue,
                    "static inline int64_t {helper}(const {list_c}* v, {given} item) {{ for (int64_t i = 0; i < v->len; i++) if ({equal}) return i; return -1; }}"
                );
                let key = if **item == Ty::Str { Ty::Read(item.clone()) } else { (**item).clone() };
                self.sigs.insert(helper.clone(), vec![key]);
            }
            "sort" => {
                let compare = self.item_compare(item, list, span)?;
                let _ = writeln!(
                    self.glue,
                    "static void {helper}({list_c}* v) {{ sg_sort(v->ptr, v->len, sizeof({held}), {compare}); }}"
                );
                self.sigs.insert(helper.clone(), Vec::new());
            }
            // The items are `Copyable`, so a copy of their bytes is a copy of them; the checker
            // has seen that they do not lie in the list itself.
            "addAll" => {
                let slice = Ty::Slice(item.clone());
                let window = self.ctype(&slice, span)?;
                let _ = writeln!(
                    self.glue,
                    "static inline void {helper}({list_c}* v, {window} items) {{ if (items.len == 0) return; \
                     if (items.len > v->cap - v->len) v->ptr = sg_reserve(v->ptr, &v->cap, v->len + items.len, sizeof({held})); \
                     memcpy(v->ptr + v->len, items.ptr, (size_t)items.len * sizeof({held})); v->len += items.len; }}"
                );
                self.sigs.insert(helper.clone(), vec![Ty::Read(Box::new(slice))]);
            }
            _ => unreachable!("the helpers are named here"),
        }
        Ok(helper)
    }

    /// The function `sg_sort` compares two items of this type with, written once.
    fn item_compare(&mut self, item: &Ty, holder: &Ty, span: Span) -> Res<String> {
        let name = mangle(item).ok_or_else(|| not_yet(span, &format!("putting the items of a `{holder}` in order")))?;
        let helper = format!("sg_order_{name}");
        if !self.made.insert(helper.clone()) {
            return Ok(helper);
        }
        let held = self.ctype(item, span)?;
        let compare = match item {
            Ty::Int(_) | Ty::Char => format!("{held} x = *(const {held}*)a, y = *(const {held}*)b; return (x > y) - (x < y);"),
            Ty::Float(_) => format!("return sg_fcmp(*(const {held}*)a, *(const {held}*)b);"),
            Ty::Str => "return sg_cmp(sg_view((const sg_string*)a), sg_view((const sg_string*)b));".to_string(),
            Ty::Struct(owner) | Ty::Enum(owner) if self.own_method(owner, "compare") => format!("return f_{}_compare(({held}*)a, ({held}*)b);", cname(owner)),
            _ => return Err(not_yet(span, &format!("putting the items of a `{holder}` in order"))),
        };
        let _ = writeln!(self.glue, "static int {helper}(const void* a, const void* b) {{ {compare} }}");
        Ok(helper)
    }

    /// A pointer to the list, the table or the array that `recv` is, borrows or points at.
    fn list_address(&mut self, recv: &'a Expr) -> Res<String> {
        let mut boxes = 0;
        let mut behind = recv.ty.strip_borrow();
        while let Ty::Ptr(inner) = behind {
            (behind, boxes) = (inner, boxes + 1);
        }
        let mut address = match recv.is_place() {
            true => self.address(recv)?,
            false if recv.ty.is_borrow() => self.value(recv)?,
            false => format!("(&{})", self.look(recv)?),
        };
        for _ in 0..boxes {
            address = format!("(*{address})");
        }
        Ok(address)
    }

    fn struct_decl(&mut self, decl: &'a StructDecl, done: &mut Vec<&'a str>) -> Res<()> {
        if done.contains(&decl.name.as_str()) {
            return Ok(());
        }
        done.push(&decl.name);
        let name = &cname(&decl.name);
        let (mut fields, mut drops, mut forgets) = (String::new(), String::new(), String::new());
        // A field that borrows is a pointer (or, for text, a pointer and a length) to what
        // someone else owns: it is never freed here. The checker has seen that it does not
        // outlive its owner.
        for field in &decl.fields {
            let ty = self.written(&field.ty)?;
            let mut held = Vec::new();
            held_types(&ty, &mut held);
            for inner in held {
                self.type_decl(inner, done)?;
            }
            let _ = writeln!(fields, "    {} m_{};", self.ctype(&ty, field.ty.span)?, field.name);
            if self.droppable(&ty) {
                let place = format!("v->m_{}", field.name);
                let _ = writeln!(drops, "    {}", self.drop_of(&ty, &place));
                let _ = writeln!(forgets, "    (void){};", self.move_of(&ty, &place));
            }
        }
        let _ = writeln!(self.types, "struct S_{name} {{\n{fields}}};");
        // What frees the struct, and what takes it out of a place and leaves that place empty.
        if !drops.is_empty() {
            let _ = writeln!(self.glue_protos, "static inline void sg_drop_S_{name}(S_{name}* v);\nstatic inline S_{name} sg_move_S_{name}(S_{name}* v);");
            let _ = writeln!(self.glue, "static inline void sg_drop_S_{name}(S_{name}* v) {{\n{drops}}}");
            let _ = writeln!(self.glue, "static inline S_{name} sg_move_S_{name}(S_{name}* v) {{\n    S_{name} out = *v;\n{forgets}    return out;\n}}");
        }
        Ok(())
    }

    /// The struct or the enum of this name, written before what holds it.
    fn type_decl(&mut self, name: &str, done: &mut Vec<&'a str>) -> Res<()> {
        if let Some(decl) = self.structs.get(name).copied() {
            return self.struct_decl(decl, done);
        }
        let decl: &'a EnumDecl = self.enums[name];
        self.enum_decl(decl, done)
    }

    /// An enum: a tag that says which variant it is and, for the variants that carry values,
    /// a union with room for the largest of them.
    fn enum_decl(&mut self, decl: &'a EnumDecl, done: &mut Vec<&'a str>) -> Res<()> {
        if done.contains(&decl.name.as_str()) {
            return Ok(());
        }
        done.push(&decl.name);
        if decl.variants.len() >= MOVED_TAG {
            return Err(not_yet(decl.name_span, "an enum with this many variants"));
        }
        let name = &cname(&decl.name);
        let (mut members, mut cases) = (String::new(), String::new());
        for (index, variant) in decl.variants.iter().enumerate() {
            let (mut fields, mut drops) = (String::new(), String::new());
            for field in &variant.fields {
                let ty = self.written(&field.ty)?;
                let mut held = Vec::new();
                held_types(&ty, &mut held);
                for inner in held {
                    self.type_decl(inner, done)?;
                }
                let _ = write!(fields, "{} m_{}; ", self.ctype(&ty, field.ty.span)?, field.name);
                if self.droppable(&ty) {
                    let _ = write!(drops, "{} ", self.drop_of(&ty, &format!("v->u.v{index}.m_{}", field.name)));
                }
            }
            if !fields.is_empty() {
                let _ = writeln!(members, "        struct {{ {fields}}} v{index};");
            }
            if !drops.is_empty() {
                let _ = writeln!(cases, "    case {index}: {drops}break;");
            }
        }
        let union = if members.is_empty() { String::new() } else { format!("    union {{\n{members}    }} u;\n") };
        let _ = writeln!(self.types, "struct E_{name} {{\n    uint8_t tag;\n{union}}};");
        // What frees the values of the variant it is, and what takes the enum out of a place:
        // the place is left with a tag that is no variant.
        if !cases.is_empty() {
            let _ = writeln!(self.glue_protos, "static void sg_drop_E_{name}(E_{name}* v);\nstatic inline E_{name} sg_move_E_{name}(E_{name}* v);");
            let _ = writeln!(self.glue, "static void sg_drop_E_{name}(E_{name}* v) {{\n    switch (v->tag) {{\n{cases}    default: break;\n    }}\n}}");
            let _ = writeln!(self.glue, "static inline E_{name} sg_move_E_{name}(E_{name}* v) {{ E_{name} out = *v; v->tag = {MOVED_TAG}; return out; }}");
        }
        Ok(())
    }

    // ---- functions and statements ----

    /// Where a program starts. An error that `main` ends with is written out, and the program
    /// ends with the exit code 1.
    fn start(&mut self, main: &Func) -> Res<String> {
        // `main(List<string> args)`: the words after the program's name, looked at where the
        // system keeps them (a text of capacity 0 owns nothing), each checked to be UTF-8.
        let (head, call) = match main.params.is_empty() {
            true => ("int main(void) {\n".to_string(), "f_main()"),
            false => {
                let list = self.ctype(&Ty::List(Box::new(Ty::Str)), main.name_span)?;
                (
                    format!(
                        "int main(int argc, char** argv) {{\n    {list} args = {{ malloc(sizeof(sg_string) * (size_t)(argc > 1 ? argc - 1 : 1)), argc > 1 ? argc - 1 : 0, argc > 1 ? argc - 1 : 0 }};\n    \
                         if (!args.ptr) abort();\n    \
                         for (int i = 1; i < argc; i++) {{\n        int64_t n = (int64_t)strlen(argv[i]);\n        \
                         if (sg_utf8_bad((const uint8_t*)argv[i], n) >= 0) {{ fprintf(stderr, \"error: args[%d] is not valid UTF-8\\n\", i - 1); free(args.ptr); return 1; }}\n        \
                         args.ptr[i - 1] = SG_LIT(argv[i], n);\n    }}\n"
                    ),
                    "f_main(args)",
                )
            }
        };
        Ok(match self.written(&main.ret)? {
            ret @ Ty::Result(..) => {
                let Ty::Result(_, error) = &ret else { unreachable!("matched above") };
                let cause = "result.u.v1.m_cause";
                let ctype = self.ctype(&ret, main.ret.span)?;
                let mut drop = if self.droppable(error) { self.drop_of(error, cause) } else { String::new() };
                let mut shown = String::new();
                let (format, values) = match &**error {
                    Ty::Str => ("%.*s", format!("(int){cause}.len, (const char*){cause}.ptr")),
                    Ty::Int(ty) => (int_shown(*ty).0, format!("({}){cause}", int_shown(*ty).1)),
                    Ty::Bool => ("%s", format!("{cause} ? \"true\" : \"false\"")),
                    Ty::Char => {
                        shown = format!("char text[4]; int text_n = sg_utf8(text, {cause});\n        ");
                        ("%.*s", "text_n, text".to_string())
                    }
                    // A struct or an enum is written by the helper of its type.
                    other => {
                        shown = format!("sg_string text = SG_LIT(\"\", 0); {}(&text, &{cause});\n        ", self.shower(other, main.ret.span)?);
                        drop = format!("sg_drop_string(&text); {drop}");
                        ("%.*s", "(int)text.len, (const char*)text.ptr".to_string())
                    }
                };
                format!(
                    "{head}    {ctype} result = {call};\n    if (result.tag != 0) {{\n        fflush(stdout);\n        {shown}\
                     fprintf(stderr, \"error: {format}\\n\", {values});\n        {drop}\n        return 1;\n    }}\n    return 0;\n}}\n"
                )
            }
            _ => format!("{head}    {call};\n    return 0;\n}}\n"),
        })
    }

    /// A `test` block: a function that takes nothing and gives nothing. An error that a `try`
    /// in it meets is written and ends the test (see `ExprKind::Try`).
    fn test(&mut self, test: &'a TestDecl, number: usize, protos: &mut String, bodies: &mut String) -> Res<()> {
        let head = format!("static void t_{number}(void)");
        let _ = writeln!(protos, "{head};");
        let _ = writeln!(bodies, "{head} {{");
        self.ret = Ty::Void;
        self.temps = 0;
        self.scopes = vec![streams()];
        self.in_test = true;
        let written = self.stmts(&test.body, 1, bodies);
        self.in_test = false;
        written?;
        if !ends_by_leaving(&test.body) {
            for line in self.drops(0) {
                let _ = writeln!(bodies, "    {line}");
            }
        }
        self.scopes.clear();
        bodies.push_str("}\n\n");
        Ok(())
    }

    fn func(&mut self, func: &'a Func, owner: Option<&str>, protos: &mut String, bodies: &mut String) -> Res<()> {
        let ret = self.written(&func.ret)?;
        let name = match owner {
            Some(owner) => format!("f_{}_{}", cname(owner), func.name),
            None => format!("f_{}", cname(&func.name)),
        };
        let mut params = Vec::new();
        let mut scope = streams();
        if let (Some(this), Some(owner)) = (func.this, owner) {
            let owner_ty = match self.structs.contains_key(owner) {
                true => Ty::Struct(owner.to_string()),
                false => Ty::Enum(owner.to_string()),
            };
            let ty = match this {
                This::Value => owner_ty,
                This::Read => Ty::Read(Box::new(owner_ty)),
                This::Ref => Ty::Ref(Box::new(owner_ty)),
            };
            params.push(format!("{} v_this", self.ctype(&ty, func.name_span)?));
            scope.push(Local { name: "this".to_string(), cname: "v_this".to_string(), ty, gone: false, drop: None });
        }
        for param in &func.params {
            let ty = self.written(&param.ty)?;
            params.push(format!("{} v_{}", self.ctype(&ty, param.ty.span)?, param.name));
            scope.push(Local { name: param.name.clone(), cname: format!("v_{}", param.name), ty, gone: false, drop: None });
        }
        let params = if params.is_empty() { "void".to_string() } else { params.join(", ") };
        let head = format!("static {} {name}({params})", self.ctype(&ret, func.ret.span)?);
        let _ = writeln!(protos, "{head};");
        let _ = writeln!(bodies, "{head} {{");
        self.ret = ret;
        self.temps = 0;
        self.scopes = vec![scope];
        self.stmts(&func.body, 1, bodies)?;
        // What the function was given by value and still owns at its end.
        if !ends_by_leaving(&func.body) {
            for line in self.drops(0) {
                let _ = writeln!(bodies, "    {line}");
            }
        }
        self.scopes.clear();
        bodies.push_str("}\n\n");
        Ok(())
    }

    /// Holds back what the statement would free at its end (the values in it that have no
    /// name), for a block whose names point into them: `for x in read make()`. They are
    /// freed where `release` writes the line, and on every way out of the block before that.
    fn hold(&mut self) -> Option<String> {
        if self.post.is_empty() {
            return None;
        }
        let line = std::mem::take(&mut self.post).join(" ");
        self.scopes.push(vec![Local { name: String::new(), cname: String::new(), ty: Ty::Void, gone: false, drop: Some(line.clone()) }]);
        Some(line)
    }

    fn release(&mut self, held: Option<String>, pad: &str, out: &mut String) {
        if let Some(line) = held {
            self.scopes.pop();
            let _ = writeln!(out, "{pad}{line}");
        }
    }

    /// A number or a `bool` that is computed only sometimes (the right side of `&&`, `||`,
    /// `??`), together with the lines it needed before and after it: they cannot stand
    /// before the statement, where they would always run.
    fn enclosed(&mut self, before: (usize, usize), value: String, ty: &Ty, span: Span) -> Res<String> {
        if before == (self.pre.len(), self.post.len()) {
            return Ok(value);
        }
        let pre: String = self.pre.drain(before.0..).map(|line| format!("{line} ")).collect();
        let post: String = self.post.drain(before.1..).map(|line| format!("{line} ")).collect();
        let (ctype, temp) = (self.ctype(ty, span)?, self.temp());
        Ok(format!("({{ {pre}{ctype} {temp} = {value}; {post}{temp}; }})"))
    }

    /// The drops for every variable in the scopes from `from` on, innermost and newest first.
    fn drops(&self, from: usize) -> Vec<String> {
        let mut lines = Vec::new();
        for scope in self.scopes[from..].iter().rev() {
            for local in scope.iter().rev() {
                match &local.drop {
                    Some(line) => lines.push(line.clone()),
                    None if !local.gone && self.droppable(&local.ty) => lines.push(self.drop_of(&local.ty, &local.cname)),
                    None => {}
                }
            }
        }
        lines
    }

    /// Which variables are known to be moved out, to come back to after a part of the
    /// program that runs only sometimes.
    fn gone(&self) -> Vec<Vec<bool>> {
        self.scopes.iter().map(|scope| scope.iter().map(|local| local.gone).collect()).collect()
    }

    /// After a block: a move made inside it is no longer certain; a value given inside it
    /// (`x = ...`) may be there.
    fn gone_only_if(&mut self, before: &[Vec<bool>]) {
        for (scope, before) in self.scopes.iter_mut().zip(before) {
            for (local, before) in scope.iter_mut().zip(before) {
                local.gone = local.gone && *before;
            }
        }
    }

    /// Back to what was known before a way out of the function was written.
    fn set_gone(&mut self, before: &[Vec<bool>]) {
        for (scope, before) in self.scopes.iter_mut().zip(before) {
            for (local, before) in scope.iter_mut().zip(before) {
                local.gone = *before;
            }
        }
    }

    /// A loop body runs again: what was moved before it may have a value by then.
    fn nothing_gone(&mut self) {
        for local in self.scopes.iter_mut().flatten() {
            local.gone = false;
        }
    }

    fn local_mut(&mut self, name: &str) -> &mut Local {
        self.scopes.iter_mut().rev().find_map(|scope| scope.iter_mut().rev().find(|local| local.name == name)).expect("names are checked before the back end runs")
    }

    fn local(&self, name: &str) -> &Local {
        self.scopes.iter().rev().find_map(|scope| scope.iter().rev().find(|local| local.name == name)).expect("names are checked before the back end runs")
    }

    fn declare(&mut self, name: &str, ty: Ty) -> String {
        self.temps += 1;
        let cname = format!("v_{name}_{}", self.temps);
        self.scopes.last_mut().expect("an open scope").push(Local { name: name.to_string(), cname: cname.clone(), ty, gone: false, drop: None });
        cname
    }

    /// A block: its own scope, and at its end the drops of what it declared.
    fn block(&mut self, block: &'a Block, depth: usize, out: &mut String) -> Res<()> {
        self.block_with(block, depth, out, Vec::new())
    }

    /// A block that starts with names of its own: those of `if x is Circle(r)`. Each comes
    /// with the C line that declares it.
    fn block_with(&mut self, block: &'a Block, depth: usize, out: &mut String, names: Vec<(Local, String)>) -> Res<()> {
        let before = self.gone();
        self.scopes.push(Vec::new());
        for (local, line) in names {
            let _ = writeln!(out, "{}{line}", "    ".repeat(depth));
            self.scopes.last_mut().expect("an open scope").push(local);
        }
        self.stmts(block, depth, out)?;
        if !ends_by_leaving(block) {
            let pad = "    ".repeat(depth);
            for line in self.drops(self.scopes.len() - 1) {
                let _ = writeln!(out, "{pad}{line}");
            }
        }
        self.scopes.pop();
        self.gone_only_if(&before);
        Ok(())
    }

    fn stmts(&mut self, block: &'a Block, depth: usize, out: &mut String) -> Res<()> {
        for stmt in block {
            self.stmt(stmt, depth, out)?;
        }
        Ok(())
    }

    /// Writes the lines that go before the statement and forgets them.
    fn flush_pre(&mut self, pad: &str, out: &mut String) {
        for line in self.pre.drain(..) {
            let _ = writeln!(out, "{pad}{line}");
        }
    }

    fn flush_post(&mut self, pad: &str, out: &mut String) {
        for line in self.post.drain(..) {
            let _ = writeln!(out, "{pad}{line}");
        }
    }

    fn stmt(&mut self, stmt: &'a Stmt, depth: usize, out: &mut String) -> Res<()> {
        let pad = "    ".repeat(depth);
        match &stmt.kind {
            StmtKind::Let { ty, name, init, .. } => {
                let ty = match ty {
                    Some(written) => self.written(written)?,
                    None => init.ty.clone(),
                };
                let ctype = self.ctype(&ty, init.span)?;
                // A `match` writes its value into the variable; the variable is in scope only
                // after it, so an arm that leaves does not drop it.
                if matches!(init.kind, ExprKind::Match { .. }) {
                    self.temps += 1;
                    let cname = format!("v_{name}_{}", self.temps);
                    let _ = writeln!(out, "{pad}{ctype} {cname};");
                    self.match_stmt(init, &Dest::Place(cname.clone(), ty.clone()), depth, out)?;
                    self.scopes.last_mut().expect("an open scope").push(Local { name: name.clone(), cname, ty, gone: false, drop: None });
                    return Ok(());
                }
                let value = self.head_value(init, &ty)?;
                self.flush_pre(&pad, out);
                let cname = self.declare(name, ty);
                let _ = writeln!(out, "{pad}{ctype} {cname} = {value};");
                self.flush_post(&pad, out);
            }
            StmtKind::Assign { target, op, value, through } => {
                // A `read` variable pointed at something else: the variable itself is assigned.
                let rebound = match &target.kind {
                    ExprKind::Var(name) if !through => Some(self.local(name)).filter(|local| local.ty.is_borrow()).map(|local| (local.cname.clone(), local.ty.clone())),
                    _ => None,
                };
                let rebinds = rebound.is_some();
                // The right side is computed first, then the place is found (Rust's order): the
                // address of an item is taken only once the right side, which may grow the list,
                // is done.
                let held = match (&rebound, &target.kind) {
                    (Some((_, ty)), _) => ty.clone(),
                    (None, ExprKind::Var(name)) => self.local(name).ty.strip_borrow().clone(),
                    (None, _) => target.ty.strip_borrow().clone(),
                };
                let new = if matches!(value.kind, ExprKind::Match { .. }) {
                    let (ctype, temp) = (self.ctype(&held, value.span)?, self.temp());
                    let _ = writeln!(out, "{pad}{ctype} {temp};");
                    self.match_stmt(value, &Dest::Place(temp.clone(), held.clone()), depth, out)?;
                    temp
                } else {
                    let new = self.head_value(value, &held)?;
                    match &target.kind {
                        ExprKind::Var(_) => new,
                        _ => {
                            let (ctype, temp) = (self.ctype(&held, value.span)?, self.temp());
                            self.pre.push(format!("{ctype} {temp} = {new};"));
                            temp
                        }
                    }
                };
                let (place, held) = match rebound {
                    Some(variable) => variable,
                    None => self.slot(target)?,
                };
                self.flush_pre(&pad, out);
                match op {
                    // The new value is ready before the old one is dropped: it may be made from it.
                    None if self.droppable(&held) && !rebinds => {
                        let (ctype, temp) = (self.ctype(&held, target.span)?, self.temp());
                        let _ = writeln!(out, "{pad}{{ {ctype} {temp} = {new}; {} {place} = {temp}; }}", self.drop_of(&held, &place));
                    }
                    None => {
                        let _ = writeln!(out, "{pad}{place} = {new};");
                    }
                    Some(op) => {
                        let operation = match held {
                            // A sum that cannot overflow in this copy of its loop.
                            Ty::Int(ty) if self.unchecked.iter().any(|step| std::ptr::eq(*step, stmt)) => {
                                let checked = std::mem::replace(&mut self.checked, false);
                                let operation = self.arithmetic(*op, ty, &place, &new, stmt.span);
                                self.checked = checked;
                                operation
                            }
                            Ty::Int(ty) => self.arithmetic(*op, ty, &place, &new, stmt.span),
                            Ty::Float(ty) => fractional(*op, ty, &place, &new),
                            _ => return Err(not_yet(stmt.span, "this assignment")),
                        };
                        let _ = writeln!(out, "{pad}{place} = {operation};");
                    }
                }
                self.flush_post(&pad, out);
            }
            StmtKind::If { cond, then, els, after } => {
                let (mut cond, names, chained, decls) = self.condition(cond, !after.is_empty())?;
                for line in decls {
                    let _ = writeln!(out, "{pad}{line}");
                }
                self.flush_pre(&pad, out);
                let mut kept = None;
                if !self.post.is_empty() {
                    let temp = self.temp();
                    let _ = writeln!(out, "{pad}bool {temp} = {cond};");
                    // The names of `x is Circle(r)` point into the value: it stays until
                    // the `if` is done.
                    if names.is_empty() && !chained {
                        self.flush_post(&pad, out);
                    } else {
                        kept = self.hold();
                    }
                    cond = temp;
                }
                let _ = writeln!(out, "{pad}if ({cond}) {{");
                self.block_with(then, depth + 1, out, names)?;
                if let Some(els) = els {
                    let _ = writeln!(out, "{pad}}} else {{");
                    self.block(els, depth + 1, out)?;
                }
                let _ = writeln!(out, "{pad}}}");
                // The value with no name that the names kept after the `if` look at stays
                // until the block around it ends.
                let lasting = match (kept, after.is_empty()) {
                    (Some(_), false) => self.scopes.pop(),
                    (kept, _) => {
                        self.release(kept, &pad, out);
                        None
                    }
                };
                if chained {
                    self.keep_names(after);
                }
                self.scopes.last_mut().expect("an open scope").extend(lasting.into_iter().flatten());
            }
            StmtKind::While { cond, body, after } => {
                self.nothing_gone();
                let (cond, names, chained, decls) = self.condition(cond, !after.is_empty())?;
                // Declared before the loop, so that the code after it sees the names that
                // `after` lists.
                for line in decls {
                    let _ = writeln!(out, "{pad}{line}");
                }
                self.loops.push(self.scopes.len());
                let mut kept = None;
                if self.pre.is_empty() && self.post.is_empty() {
                    let _ = writeln!(out, "{pad}while ({cond}) {{");
                } else {
                    // The condition needs temporaries: they are made and dropped in every round.
                    let (inner, temp) = (format!("{pad}    "), self.temp());
                    let _ = writeln!(out, "{pad}while (1) {{");
                    self.flush_pre(&inner, out);
                    let _ = writeln!(out, "{inner}bool {temp} = {cond};");
                    if names.is_empty() && !chained {
                        self.flush_post(&inner, out);
                        let _ = writeln!(out, "{inner}if (!{temp}) break;");
                    } else {
                        // The names point into the value: it stays for the round.
                        let _ = writeln!(out, "{inner}if (!{temp}) {{ {} break; }}", self.post.join(" "));
                        kept = self.hold();
                    }
                }
                self.block_with(body, depth + 1, out, names)?;
                self.release(kept, &format!("{pad}    "), out);
                self.loops.pop();
                let _ = writeln!(out, "{pad}}}");
                if chained {
                    self.keep_names(after);
                }
            }
            StmtKind::For { var, from, to, body, .. } => {
                // The ends are computed once; the counter stops before the end, so it cannot
                // overflow. The variable is a copy of the counter made in each round: the body
                // may change it without changing the rounds, as in the interpreter.
                let ty = self.ctype(&from.ty, from.span)?;
                let (counter, end) = (self.temp(), self.temp());
                let (from_value, to_value) = (self.value(from)?, self.value(to)?);
                self.flush_pre(&pad, out);
                let _ = writeln!(out, "{pad}{{");
                let _ = writeln!(out, "{pad}{ty} {counter} = {from_value}; {ty} {end} = {to_value};");
                self.flush_post(&pad, out);
                self.nothing_gone();
                let Ty::Int(counter_ty) = from.ty else { return Err(not_yet(from.span, "a range of this type")) };
                self.counted_loop(&pad, out, (&counter, counter_ty), &format!("{counter} < {end}"), var, body, depth, |this| {
                    this.temps += 1;
                    let cname = format!("v_{var}_{}", this.temps);
                    // `(void)`: a body that does not use the variable leaves no warning.
                    let line = format!("{ty} {cname} = {counter}; (void){cname};");
                    Ok(vec![(Local { name: var.clone(), cname, ty: from.ty.clone(), gone: false, drop: None }, line)])
                })?;
                let _ = writeln!(out, "{pad}}}");
            }
            StmtKind::ForEach { var, iter, body, by, .. } if table_behind(&iter.ty).is_some_and(|(_, boxes)| by.is_some() || boxes == 0) => {
                let table = table_behind(&iter.ty).expect("just tested").0.clone();
                let (table_c, name) = (self.ctype(&table, iter.span)?, mangle(&table).expect("it has a C type"));
                // A map gives its entries (a key and a value), a set its items.
                let (item, at_item) = match &table {
                    Ty::Map(key, value) => (Ty::Entry(key.clone(), value.clone()), ""),
                    Ty::Set(key) => ((**key).clone(), ".m_key"),
                    _ => unreachable!("it is a table"),
                };
                let (it, at) = (self.temp(), self.temp());
                let inner = format!("{pad}    ");
                self.nothing_gone();
                match by {
                    Some(mode) => {
                        let address = self.list_address(iter)?;
                        self.flush_pre(&pad, out);
                        let kept = self.hold();
                        let _ = writeln!(out, "{pad}{{");
                        let _ = writeln!(out, "{inner}const {table_c}* {it} = {address}; int64_t {at} = 0;");
                        let ty = if *mode == Mode::Ref { Ty::Ref(Box::new(item)) } else { Ty::Read(Box::new(item)) };
                        let place = format!("{it}->entries[{at}]{at_item}");
                        self.counted_loop(&inner, out, (&at, IntTy::Long), &format!("{at} < {it}->len"), var, body, depth + 1, |this| Ok(vec![this.borrowed_name(var, ty.clone(), &place, iter.span)?]))?;
                        self.release(kept, &inner, out);
                        let _ = writeln!(out, "{pad}}}");
                    }
                    None => {
                        let value = self.value(iter)?;
                        self.flush_pre(&pad, out);
                        let _ = writeln!(out, "{pad}{{");
                        let _ = writeln!(out, "{inner}{table_c} {it} = {value}; int64_t {at} = 0;");
                        self.flush_post(&inner, out);
                        let rest = format!("sg_drop_from_{name}(&{it}, {at} + 1);");
                        self.scopes.push(vec![Local { name: String::new(), cname: it.clone(), ty: table.clone(), gone: false, drop: Some(rest.clone()) }]);
                        let item_c = self.ctype(&item, iter.span)?;
                        self.counted_loop(&inner, out, (&at, IntTy::Long), &format!("{at} < {it}.len"), var, body, depth + 1, |this| {
                            this.temps += 1;
                            let cname = format!("v_{var}_{}", this.temps);
                            let line = format!("{item_c} {cname} = {it}.entries[{at}]{at_item};");
                            Ok(vec![(Local { name: var.clone(), cname, ty: item.clone(), gone: false, drop: None }, line)])
                        })?;
                        self.scopes.pop();
                        let _ = writeln!(out, "{inner}{rest}\n{pad}}}");
                    }
                }
            }
            StmtKind::ForEach { var, iter, body, by, .. } if items_behind(&iter.ty).is_some() => {
                let items = items_behind(&iter.ty).expect("just tested").clone();
                let (Ty::Array(item, _) | Ty::Slice(item)) = &items else { unreachable!("it is an array or a slice") };
                let (it, at) = (self.temp(), self.temp());
                let inner = format!("{pad}    ");
                self.nothing_gone();
                match by {
                    // Borrowed items are gone over through a window, one position after the
                    // other; an array that is lent gives a window onto all of it.
                    Some(mode) => {
                        let window_c = self.ctype(&Ty::Slice(item.clone()), iter.span)?;
                        let window = self.window(iter)?;
                        self.flush_pre(&pad, out);
                        let kept = self.hold();
                        let _ = writeln!(out, "{pad}{{");
                        let _ = writeln!(out, "{inner}{window_c} {it} = {window}; int64_t {at} = 0;");
                        let ty = if *mode == Mode::Ref { Ty::Ref(item.clone()) } else { Ty::Read(item.clone()) };
                        let place = format!("{it}.ptr[{at}]");
                        self.counted_loop(&inner, out, (&at, IntTy::Long), &format!("{at} < {it}.len"), var, body, depth + 1, |this| Ok(vec![this.borrowed_name(var, ty.clone(), &place, iter.span)?]))?;
                        self.release(kept, &inner, out);
                        let _ = writeln!(out, "{pad}}}");
                    }
                    // An array that is handed over gives each item to the round it is for;
                    // the items after the one of this round are freed when the loop is left.
                    None => {
                        let Ty::Array(_, length) = &iter.ty else { return Err(not_yet(iter.span, &format!("a `for` over a `{}`", iter.ty))) };
                        let (array_c, held, name) = (self.ctype(&iter.ty, iter.span)?, self.ctype(item, iter.span)?, mangle(&iter.ty).expect("it has a C type"));
                        let value = self.value(iter)?;
                        self.flush_pre(&pad, out);
                        let _ = writeln!(out, "{pad}{{");
                        let _ = writeln!(out, "{inner}{array_c} {it} = {value}; int64_t {at} = 0;");
                        self.flush_post(&inner, out);
                        let rest = self.droppable(item).then(|| format!("sg_drop_from_{name}(&{it}, {at} + 1);"));
                        self.scopes.push(vec![Local { name: String::new(), cname: it.clone(), ty: Ty::Void, gone: false, drop: rest.clone() }]);
                        self.counted_loop(&inner, out, (&at, IntTy::Long), &format!("{at} < {length}"), var, body, depth + 1, |this| {
                            this.temps += 1;
                            let cname = format!("v_{var}_{}", this.temps);
                            let line = format!("{held} {cname} = {it}.items[{at}];");
                            Ok(vec![(Local { name: var.clone(), cname, ty: (**item).clone(), gone: false, drop: None }, line)])
                        })?;
                        self.scopes.pop();
                        if let Some(rest) = rest {
                            let _ = writeln!(out, "{inner}{rest}");
                        }
                        let _ = writeln!(out, "{pad}}}");
                    }
                }
            }
            // What goes over a text gives one part after the other, until it has none.
            StmtKind::ForEach { var, iter, body, .. } if matches!(iter.ty, Ty::TextIter(_)) => {
                let Ty::TextIter(kind) = iter.ty else { unreachable!("tested above") };
                let item = kind.item();
                if kind == TextIter::ReadLines {
                    self.read_lines_glue(iter.span)?;
                }
                let iter_c = self.ctype(&iter.ty, iter.span)?;
                let (held, it, at) = (self.ctype(&item, iter.span)?, self.temp(), self.temp());
                let inner = format!("{pad}    ");
                self.nothing_gone();
                let value = self.value(iter)?;
                self.flush_pre(&pad, out);
                let _ = writeln!(out, "{pad}{{");
                let _ = writeln!(out, "{inner}{iter_c} {it} = {value}; {held} {at};");
                self.flush_post(&inner, out);
                let _ = writeln!(out, "{inner}while ({}(&{it}, &{at})) {{", next_of(kind));
                self.temps += 1;
                let cname = format!("v_{var}_{}", self.temps);
                let line = format!("{held} {cname} = {at};");
                let local = Local { name: var.clone(), cname, ty: item, gone: false, drop: None };
                self.loops.push(self.scopes.len());
                self.block_with(body, depth + 2, out, vec![(local, line)])?;
                self.loops.pop();
                let _ = writeln!(out, "{inner}}}\n{pad}}}");
            }
            StmtKind::ForEach { var, iter, body, by, .. } => {
                let list = match (by, list_behind(&iter.ty)) {
                    (Some(_), Some((list, _))) => list.clone(),
                    (None, Some((list, 0))) => list.clone(),
                    _ => return Err(not_yet(iter.span, &format!("a `for` over a `{}`", iter.ty))),
                };
                let Ty::List(item) = &list else { unreachable!("it is a list") };
                let (list_c, held, name) = (self.ctype(&list, iter.span)?, self.ctype(item, iter.span)?, mangle(&list).expect("it has a C type"));
                let (it, at) = (self.temp(), self.temp());
                let inner = format!("{pad}    ");
                self.nothing_gone();
                match by {
                    // A borrowed list lends its items, one position after the other. The
                    // checker keeps the list from changing while the loop runs.
                    Some(mode) => {
                        let address = self.list_address(iter)?;
                        self.flush_pre(&pad, out);
                        let kept = self.hold();
                        let _ = writeln!(out, "{pad}{{");
                        let _ = writeln!(out, "{inner}const {list_c}* {it} = {address}; int64_t {at} = 0;");
                        let ty = if *mode == Mode::Ref { Ty::Ref(item.clone()) } else { Ty::Read(item.clone()) };
                        let place = format!("{it}->ptr[{at}]");
                        self.counted_loop(&inner, out, (&at, IntTy::Long), &format!("{at} < {it}->len"), var, body, depth + 1, |this| Ok(vec![this.borrowed_name(var, ty.clone(), &place, iter.span)?]))?;
                        self.release(kept, &inner, out);
                        let _ = writeln!(out, "{pad}}}");
                    }
                    // A list that is handed over gives each item to the round it is for. What
                    // is left of it when the loop ends, or is left early, is freed: the items
                    // after the one of this round, and the block.
                    None => {
                        let value = self.value(iter)?;
                        self.flush_pre(&pad, out);
                        let _ = writeln!(out, "{pad}{{");
                        let _ = writeln!(out, "{inner}{list_c} {it} = {value}; int64_t {at} = 0;");
                        self.flush_post(&inner, out);
                        let rest = format!("sg_drop_from_{name}(&{it}, {at} + 1);");
                        self.scopes.push(vec![Local { name: String::new(), cname: it.clone(), ty: list.clone(), gone: false, drop: Some(rest.clone()) }]);
                        self.counted_loop(&inner, out, (&at, IntTy::Long), &format!("{at} < {it}.len"), var, body, depth + 1, |this| {
                            this.temps += 1;
                            let cname = format!("v_{var}_{}", this.temps);
                            let line = format!("{held} {cname} = {it}.ptr[{at}];");
                            Ok(vec![(Local { name: var.clone(), cname, ty: (**item).clone(), gone: false, drop: None }, line)])
                        })?;
                        self.scopes.pop();
                        let _ = writeln!(out, "{inner}{rest}\n{pad}}}");
                    }
                }
            }
            StmtKind::Return(Some(value)) if matches!(value.kind, ExprKind::Match { .. }) => self.match_stmt(value, &Dest::Return, depth, out)?,
            StmtKind::Return(value) => {
                for line in self.return_lines(value.as_ref(), &[], stmt.span)? {
                    let _ = writeln!(out, "{pad}{line}");
                }
            }
            StmtKind::Break | StmtKind::Continue => {
                // What the loop body declared ends here.
                let from = *self.loops.last().expect("the checker allows these only in a loop");
                for line in self.drops(from) {
                    let _ = writeln!(out, "{pad}{line}");
                }
                self.ways_out += 1;
                match (&stmt.kind, &self.break_to) {
                    (StmtKind::Break, Some((label, _))) => {
                        let _ = writeln!(out, "{pad}goto {label};");
                        self.break_to = Some((label.clone(), true));
                    }
                    (StmtKind::Break, None) => {
                        let _ = writeln!(out, "{pad}break;");
                    }
                    _ => {
                        let _ = writeln!(out, "{pad}continue;");
                    }
                }
            }
            StmtKind::Discard(value) => self.unused(value, &pad, out)?,
            StmtKind::Expr(expr) => self.expr_stmt(expr, depth, out)?,
        }
        Ok(())
    }

    /// A loop that goes by a counter declared before it, `counter` (its C name and type),
    /// while `cond` holds, adding one to the counter after each round. `var` is the name the
    /// loop declares (never the counter); `names` gives what the body sees first in each round. Written inside a block at `pad`; the body is a level
    /// deeper.
    ///
    /// In a checked build, when the body adds to sums that cannot overflow for a number of
    /// rounds (see `sums.rs`), the loop is written twice: the first copy runs those rounds
    /// without the tests of the sums, the second takes over with them, and a `break` in the
    /// first leaves both.
    #[allow(clippy::too_many_arguments)]
    fn counted_loop(
        &mut self,
        pad: &str,
        out: &mut String,
        counter: (&str, IntTy),
        cond: &str,
        var: &str,
        body: &'a Block,
        depth: usize,
        mut names: impl FnMut(&mut Self) -> Res<Vec<(Local, String)>>,
    ) -> Res<()> {
        let (at, at_ty) = counter;
        let found = if self.checked {
            sums::sums(body, var, |name| {
                let local = self.scopes.iter().rev().find_map(|scope| scope.iter().rev().find(|local| local.name == name))?;
                match local.ty {
                    Ty::Int(ty) => Some(ty),
                    _ => None,
                }
            })
        } else {
            Vec::new()
        };
        let mut label = None;
        if !found.is_empty() {
            let mut rounds = String::new();
            for sum in &found {
                let local = self.local(sum.name);
                let Ty::Int(ty) = local.ty else { unreachable!("`sums` takes integers only") };
                let steps = format!("sg_steps_{}({}, {}LL, {}LL)", int_name(ty), local.cname, sum.lo, sum.hi);
                rounds = if rounds.is_empty() { steps } else { format!("sg_fewer({rounds}, {steps})") };
            }
            let (stop, end) = (self.temp(), self.temp());
            let names_list = found.iter().map(|sum| format!("`{}`", sum.name)).collect::<Vec<_>>().join(", ");
            let _ = writeln!(out, "{pad}/* {names_list} cannot overflow for this many rounds: they run without the test. */");
            let _ = writeln!(out, "{pad}{} {stop} = sg_upto_{}({at}, {rounds});", int_ctype(at_ty), int_name(at_ty));
            let _ = writeln!(out, "{pad}for (; {at} < {stop} && {cond}; {at}++) {{");
            let lent = names(self)?;
            let unchecked = std::mem::replace(&mut self.unchecked, found.iter().flat_map(|sum| sum.steps.iter().copied()).collect());
            let outer = self.break_to.replace((end.clone(), false));
            self.loops.push(self.scopes.len());
            self.block_with(body, depth + 1, out, lent)?;
            self.loops.pop();
            let used = std::mem::replace(&mut self.break_to, outer).is_some_and(|(_, used)| used);
            self.unchecked = unchecked;
            let _ = writeln!(out, "{pad}}}");
            label = used.then_some(end);
        }
        let _ = writeln!(out, "{pad}for (; {cond}; {at}++) {{");
        let lent = names(self)?;
        // The body of the second copy is its own loop: a `break` in it is a C `break`.
        let outer = self.break_to.take();
        self.loops.push(self.scopes.len());
        self.block_with(body, depth + 1, out, lent)?;
        self.loops.pop();
        self.break_to = outer;
        let _ = writeln!(out, "{pad}}}");
        if let Some(label) = label {
            let _ = writeln!(out, "{pad}{label}: ;");
        }
        Ok(())
    }

    /// An expression that stands as a statement.
    fn expr_stmt(&mut self, expr: &'a Expr, depth: usize, out: &mut String) -> Res<()> {
        let pad = "    ".repeat(depth);
        match &expr.kind {
            ExprKind::Call { name, args, callee: Callee::Plain, .. } if name == "print" => self.print(&args[0], &pad, out)?,
            ExprKind::Call { name, args, callee: Callee::Plain, .. } if name == "printError" => self.print_error(&args[0], &pad, out)?,
            ExprKind::Call { callee: Callee::Plain, name, .. } if name == "exit" => {
                let stop = self.exit(expr)?;
                self.flush_pre(&pad, out);
                let _ = writeln!(out, "{pad}{stop};");
                self.post.clear();
            }
            ExprKind::Call { name, args, callee: Callee::Plain, .. } if name == "assert" => self.assert(expr, &args[0], &pad, out)?,
            ExprKind::Call { name, args, callee: Callee::Plain, .. } if name == "panic" => {
                let (format, values) = self.format(&args[0])?;
                let loc = self.loc(expr.span);
                self.flush_pre(&pad, out);
                let _ = writeln!(out, "{pad}sg_panicf({loc}, {}{values});", c_string(&format));
                self.post.clear();
            }
            ExprKind::Match { .. } => self.match_stmt(expr, &Dest::Unused, depth, out)?,
            _ => self.unused(expr, &pad, out)?,
        }
        Ok(())
    }

    /// The condition of an `if` or a `while`, and the names it gives to the block: those of
    /// `x is Circle(r)`, declared at the start of the block. When an `is` is part of a chain
    /// of `&&` or `||`, is tested with `not`, or names values that the code after the
    /// statement sees (`keep`), its names are set in the condition instead, and they are in a
    /// scope that the condition opens (the `bool`): the caller closes it after the statement
    /// with `keep_names`. The last part is the lines that declare those names, to be written
    /// before the statement.
    fn condition(&mut self, cond: &'a Expr, keep: bool) -> Res<(String, Names, bool, Vec<String>)> {
        if !keep && matches!(cond.kind, ExprKind::IsVariant { negated: false, .. }) {
            let (test, names) = self.variant_test(cond)?;
            return Ok((test, names, false, Vec::new()));
        }
        if names_in_chain(cond) || names_tested(cond) {
            self.scopes.push(Vec::new());
            let (test, decls) = self.chain(cond, true)?;
            return Ok((test, Vec::new(), true, decls));
        }
        Ok((self.first(cond, true)?, Vec::new(), false, Vec::new()))
    }

    /// Closes the scope that `condition` opened for the names of a chain, and puts the names
    /// that the code after the statement sees in the scope around it.
    fn keep_names(&mut self, after: &[String]) {
        let names = self.scopes.pop().expect("the scope of the condition");
        let around = self.scopes.last_mut().expect("an open scope");
        around.extend(names.into_iter().filter(|local| after.contains(&local.name)));
    }

    /// `a && b && ...` where an `is` names values, or `a || b || ...` where an `is not` does.
    /// Each name is declared before the statement and set where its test holds, so that what
    /// comes after it on the right, the block of an `if` or `while`, `else` and the code after
    /// the statement can use it. The names go in the innermost scope; the lines that declare
    /// them are given back.
    fn chain(&mut self, expr: &'a Expr, head: bool) -> Res<(String, Vec<String>)> {
        let op = match &expr.kind {
            ExprKind::Binary(op @ BinOp::Or, ..) => *op,
            _ => BinOp::And,
        };
        let (mut parts, mut all_decls) = (Vec::new(), Vec::new());
        for (position, part) in links(expr, op).into_iter().enumerate() {
            let named = names_tested(part);
            // What comes after the first part runs only sometimes.
            let before = (self.pre.len(), self.post.len());
            self.maybe += usize::from(position > 0);
            if named && position > 0 {
                self.late = Some((Vec::new(), Vec::new()));
            }
            let value = match (named, position) {
                (true, _) => self.named_test(part),
                (false, 0) => self.first(part, head).map(|value| (value, Vec::new())),
                (false, _) => self.value(part).map(|value| (value, Vec::new())),
            };
            self.maybe -= usize::from(position > 0);
            let (mut value, mut decls) = value?;
            if let Some((made, sets)) = self.late.take() {
                decls.extend(made);
                if !sets.is_empty() {
                    value = format!("({}, {value})", sets.join(", "));
                }
            }
            let value = match position {
                0 => value,
                // The names would point into a value that is dropped before they are used.
                _ if named && before != (self.pre.len(), self.post.len()) => {
                    return Err(not_yet(part.span, &format!("an `is` with names on a value made after the first `{}`", op.symbol())));
                }
                _ => self.enclosed(before, value, &Ty::Bool, part.span)?,
            };
            all_decls.extend(decls);
            parts.push(value);
        }
        Ok((format!("({})", parts.join(&format!(" {} ", op.symbol()))), all_decls))
    }

    /// `x is Circle(r)` in a chain: the test, which sets the names where it holds, and the
    /// lines that declare them. The names are put in the innermost scope. `x is not
    /// Circle(r)` and `!(x is Circle(r))` set them where the test of `x is Circle(r)` holds,
    /// which is where they fail.
    fn named_test(&mut self, expr: &'a Expr) -> Res<(String, Vec<String>)> {
        let (test, flip) = match &expr.kind {
            ExprKind::Unary(UnOp::Not, inner) => (&**inner, true),
            _ => (expr, false),
        };
        let ExprKind::IsVariant { negated, .. } = &test.kind else { unreachable!("only called for an `is` test") };
        let (test, names) = self.positive_test(test)?;
        let (mut decls, mut sets, mut plain) = (Vec::new(), Vec::new(), true);
        for (local, line) in names {
            let cname = &local.cname;
            let alone = line.find(&format!(" {cname};"));
            match alone.filter(|at| line.find(&format!(" {cname} = ")).is_none_or(|set| *at < set)) {
                // Declared alone, and set by the statements after it: an `or` with names.
                Some(at) => {
                    let end = at + cname.len() + 2;
                    decls.push(line[..end].to_string());
                    let rest = line[end..].trim();
                    if !rest.is_empty() {
                        sets.push(rest.to_string());
                        plain = false;
                    }
                }
                None => {
                    decls.push(declaration(&line, cname));
                    sets.push(assignment(&line, cname, cname).trim_end_matches(';').to_string());
                }
            }
            self.scopes.last_mut().expect("an open scope").push(local);
        }
        let set = match plain {
            true => format!("({}, 1)", sets.join(", ")),
            false => {
                let lines: String = sets.iter().map(|line| if line.ends_with(';') || line.ends_with('}') { format!("{line} ") } else { format!("{line}; ") }).collect();
                format!("({{ {lines}1; }})")
            }
        };
        let test = format!("({test} && {set})");
        Ok((if *negated != flip { format!("(!{test})") } else { test }, decls))
    }

    /// `x is Circle(r)`: a test on the tag, and on what the variant carries where patterns
    /// are written inside. The names are borrows of what the variant carries.
    fn variant_test(&mut self, expr: &'a Expr) -> Res<(String, Vec<(Local, String)>)> {
        let ExprKind::IsVariant { negated, .. } = &expr.kind else { unreachable!("only called for an `is` test") };
        let (test, names) = self.positive_test(expr)?;
        Ok((if *negated { format!("(!{test})") } else { test }, names))
    }

    /// The test of `x is Circle(r)` also for `x is not Circle(r)`, and the names.
    fn positive_test(&mut self, expr: &'a Expr) -> Res<(String, Vec<(Local, String)>)> {
        let ExprKind::IsVariant { expr: subject, pattern, by_ref, .. } = &expr.kind else {
            unreachable!("only called for an `is` test");
        };
        let place = self.look(subject)?;
        let ty = subject.ty.strip_borrow();
        if self.variants(behind("", ty).1, subject.span)?.is_none() {
            return Err(not_yet(expr.span, &format!("`is` on `{ty}`")));
        }
        let test = self.pattern_test(pattern, &place, ty, subject.span)?.expect("`is` names a variant");
        let mut names = Vec::new();
        let mode = if *by_ref { Mode::Ref } else { Mode::Read };
        match has_choice(pattern) && !pattern.names().is_empty() {
            true => names = self.take_apart_choices(pattern, &place, ty, Some(mode), subject.span)?.0,
            false => self.take_apart(pattern, &place, ty, Some(mode), &mut names, &mut Vec::new())?,
        }
        Ok((test, names))
    }

    /// The C test that the value at `place`, of type `ty`, matches the pattern; `None` when
    /// every value does. A variant is tested by its tag, then what it carries, left to right.
    fn pattern_test(&mut self, pattern: &Pattern, place: &str, ty: &Ty, span: Span) -> Res<Option<String>> {
        // On a `T?`, `null` tests that nothing is there, any other pattern that something is
        // and then what it is.
        if !matches!(pattern.kind, PatternKind::Wildcard | PatternKind::Bind(_) | PatternKind::Or(_)) {
            if let Some((holds, inner, inner_ty)) = self.nullable_at(place, ty) {
                return Ok(Some(match &pattern.kind {
                    PatternKind::Null => format!("(!{holds})"),
                    _ => match self.pattern_test(pattern, &inner, &inner_ty, span)? {
                        Some(test) => format!("({holds} && {test})"),
                        None => holds,
                    },
                }));
            }
        }
        Ok(match &pattern.kind {
            PatternKind::Wildcard | PatternKind::Bind(_) => None,
            PatternKind::Null => unreachable!("the checker allows `null` only for a `T?`"),
            PatternKind::Or(alternatives) => {
                let mut tests = Vec::new();
                for alternative in alternatives {
                    match self.pattern_test(alternative, place, ty, span)? {
                        Some(test) => tests.push(test),
                        None => return Ok(None),
                    }
                }
                Some(format!("({})", tests.join(" || ")))
            }
            PatternKind::Bool(wanted) => {
                let (at, _) = behind(place, ty);
                Some(if *wanted { at } else { format!("(!{at})") })
            }
            PatternKind::Range(low, high) => {
                let (PatternKind::Int(low), PatternKind::Int(high)) = (&low.kind, &high.kind) else {
                    unreachable!("the checker turns the ends of a range into numbers")
                };
                let (at, core) = behind(place, ty);
                // A character is tested by its code. An end that is the first or the last
                // value of the type is not tested: every value passes it.
                let (int, first, last) = match core {
                    Ty::Int(int) => (*int, int.min(), int.max()),
                    _ => (IntTy::Uint, 0, 0x10ffff),
                };
                let mut tests = Vec::new();
                if *low > first {
                    tests.push(format!("{at} >= {}", int_literal(*low, int)));
                }
                if *high < last {
                    tests.push(format!("{at} <= {}", int_literal(*high, int)));
                }
                match tests.is_empty() {
                    true => None,
                    false => Some(format!("({})", tests.join(" && "))),
                }
            }
            PatternKind::Int(wanted) => {
                let (at, core) = behind(place, ty);
                let int = match core {
                    Ty::Int(int) => *int,
                    // A character is tested by its code.
                    _ => IntTy::Uint,
                };
                Some(format!("({at} == {})", int_literal(*wanted, int)))
            }
            PatternKind::Char(_) => unreachable!("the checker turns a character into a number"),
            PatternKind::Str(text) => {
                let view = match flat(ty) {
                    _ if is_text_view(ty) => place.to_string(),
                    Ty::Ref(_) => format!("sg_view({place})"),
                    _ => format!("sg_view(&{place})"),
                };
                Some(format!("sg_eq({view}, SG_STR({}, {}))", c_string(text), text.len()))
            }
            PatternKind::Variant { index, args, .. } => {
                let (at, core) = behind(place, ty);
                let fields = self.variants(core, span)?.expect("the checker allows a variant only for an enum")[*index].clone();
                let mut tests = vec![format!("{at}.tag == {index}")];
                for (arg, (field, ty, span)) in args.iter().flatten().zip(&fields) {
                    if let Some(test) = self.pattern_test(arg, &format!("{at}.u.v{index}.m_{field}"), ty, *span)? {
                        tests.push(test);
                    }
                }
                Some(match tests.len() {
                    1 => format!("({})", tests[0]),
                    _ => format!("({})", tests.join(" && ")),
                })
            }
        })
    }

    /// What a chosen arm does with the value at `place`, of type `ty`: the names of the
    /// pattern, each with the C line that declares it (not in scope yet), and for a value that
    /// is owned (`mode` is `None`) the lines that drop what the pattern skips and give back the
    /// boxes it looked into. With `mode` the names borrow.
    fn take_apart(&mut self, pattern: &Pattern, place: &str, ty: &Ty, mode: Option<Mode>, names: &mut Vec<(Local, String)>, chosen: &mut Vec<String>) -> Res<()> {
        match &pattern.kind {
            PatternKind::Bind(name) => {
                let span = pattern.span;
                match mode {
                    None => {
                        let ctype = self.ctype(ty, span)?;
                        self.temps += 1;
                        let cname = format!("v_{name}_{}", self.temps);
                        let line = format!("{ctype} {cname} = {place};");
                        names.push((Local { name: name.clone(), cname, ty: ty.clone(), gone: false, drop: None }, line));
                    }
                    Some(mode) => {
                        let held = Box::new(ty.clone());
                        let ty = if mode == Mode::Ref { Ty::Ref(held) } else { Ty::Read(held) };
                        names.push(self.borrowed_name(name, ty, place, span)?);
                    }
                }
            }
            // `null` holds nothing to drop.
            PatternKind::Null => {}
            // An `or` that names nothing takes nothing: what it matched is dropped whole, as for
            // `_`. One that names values is opened by the arm first (`alternatives`).
            PatternKind::Wildcard
            | PatternKind::Int(_)
            | PatternKind::Char(_)
            | PatternKind::Str(_)
            | PatternKind::Bool(_)
            | PatternKind::Range(..)
            | PatternKind::Or(_) => {
                if mode.is_none() && self.droppable(ty) {
                    chosen.push(self.drop_of(ty, place));
                }
            }
            // A variant in a `T?`: what is there, once it is known to be there.
            PatternKind::Variant { .. } if self.nullable_at(place, ty).is_some() => {
                let (_, inner, inner_ty) = self.nullable_at(place, ty).expect("just tested");
                self.take_apart(pattern, &inner, &inner_ty, mode, names, chosen)?;
            }
            PatternKind::Variant { index, args, .. } => {
                // Through a borrow the names borrow too; `read` anywhere makes them `read`.
                let mut inner_mode = mode;
                let mut layer = flat(ty);
                let mut boxes = Vec::new();
                let mut at = place.to_string();
                loop {
                    layer = match layer {
                        Ty::Read(inner) => {
                            inner_mode = Some(Mode::Read);
                            flat(inner)
                        }
                        Ty::Ref(inner) => {
                            inner_mode = inner_mode.or(Some(Mode::Ref));
                            flat(inner)
                        }
                        Ty::Ptr(inner) => {
                            boxes.push(at.clone());
                            flat(inner)
                        }
                        _ => break,
                    };
                    at = format!("(*{at})");
                }
                let fields = self.variants(layer, pattern.span)?.expect("the checker allows a variant only for an enum")[*index].clone();
                for (position, (field, ty, _)) in fields.iter().enumerate() {
                    let value = format!("{at}.u.v{index}.m_{field}");
                    match args.as_ref().map(|args| &args[position]) {
                        Some(arg) => self.take_apart(arg, &value, ty, inner_mode, names, chosen)?,
                        None if inner_mode.is_none() && self.droppable(ty) => chosen.push(self.drop_of(ty, &value)),
                        None => {}
                    }
                }
                // An owned box that was looked into is given back, after what it held.
                if inner_mode.is_none() {
                    for held in boxes.iter().rev() {
                        chosen.push(format!("free({held});"));
                    }
                }
            }
        }
        Ok(())
    }

    /// A name that borrows what is at `at`, and the C line that declares it. The name is not
    /// in scope yet.
    fn borrowed_name(&mut self, name: &str, ty: Ty, at: &str, span: Span) -> Res<(Local, String)> {
        // What is there may be a borrow itself (a field `read string text`): the name is a
        // copy of it, not a borrow of a borrow.
        let (ty, init) = match &ty {
            Ty::Read(held) | Ty::Ref(held) if held.is_borrow() => ((**held).clone(), at.to_string()),
            _ if is_text_view(&ty) => (ty, format!("sg_view(&{at})")),
            // A `T?` that is looked at: tested here, once.
            _ if lent_option(&ty).is_some() => {
                let init = self.lend(&format!("(&{at})"), lent_option(&ty).expect("just tested"), span)?;
                (ty, init)
            }
            _ => (ty, format!("(&{at})")),
        };
        let ctype = self.ctype(&ty, span)?;
        self.temps += 1;
        let cname = format!("v_{name}_{}", self.temps);
        let line = format!("{ctype} {cname} = {init};");
        Ok((Local { name: name.to_string(), cname, ty, gone: false, drop: None }, line))
    }

    /// For a value at `place` of a type that may be `null`: the C test that something is
    /// there, and where that something is, with its type. `None` for any other type.
    fn nullable_at(&self, place: &str, ty: &Ty) -> Option<(String, String, Ty)> {
        let ty = flat(ty);
        let not_null = |inner: &Ty, borrow: fn(Box<Ty>) -> Ty| match inner {
            Ty::Nullable(held) => borrow(held.clone()),
            _ => unreachable!("only called for a `T?`"),
        };
        Some(match ty {
            Ty::Nullable(held) if is_option(ty) => (format!("{place}.has"), format!("{place}.value"), (**held).clone()),
            // A pointer that may be `null` is the pointer.
            Ty::Nullable(held) => (format!("({place} != NULL)"), place.to_string(), (**held).clone()),
            // A `read T?` points at the value, or is null (for text, of length -1).
            Ty::Read(inner) if is_option(inner) => (self.lent_holds(inner, place), place.to_string(), not_null(inner, Ty::Read)),
            Ty::Ref(inner) if is_option(inner) => (format!("{place}->has"), format!("(&{place}->value)"), not_null(inner, Ty::Ref)),
            Ty::Read(inner) if matches!(**inner, Ty::Nullable(_)) => (format!("((*{place}) != NULL)"), place.to_string(), not_null(inner, Ty::Read)),
            Ty::Ref(inner) if matches!(**inner, Ty::Nullable(_)) => (format!("((*{place}) != NULL)"), place.to_string(), not_null(inner, Ty::Ref)),
            _ => return None,
        })
    }

    /// The names of an arm whose pattern holds an `or` with names: the pattern is opened into
    /// one pattern for each way to match, the names are declared once, and the way that
    /// matched sets them. For an owned value, what the way leaves is dropped by the same
    /// choice. Gives the names, each with its C line, and that line of drops.
    fn take_apart_choices(&mut self, pattern: &Pattern, place: &str, ty: &Ty, mode: Option<Mode>, span: Span) -> Res<(Names, Vec<String>)> {
        let ways = alternatives(pattern);
        let mut sets = Vec::new();
        let mut drops = Vec::new();
        let mut names: Names = Vec::new();
        for (position, way) in ways.iter().enumerate() {
            let test = match position + 1 == ways.len() {
                true => None,
                false => self.pattern_test(way, place, ty, span)?,
            };
            let (mut found, mut chosen) = (Vec::new(), Vec::new());
            self.take_apart(way, place, ty, mode, &mut found, &mut chosen)?;
            let assigned: Vec<String> = found
                .iter()
                .map(|(local, line)| {
                    let first = match position {
                        0 => &local.cname,
                        _ => &names.iter().find(|(named, _)| named.name == local.name).expect("each way names the same values").0.cname,
                    };
                    assignment(line, &local.cname, first)
                })
                .collect();
            if position == 0 {
                names = found.into_iter().map(|(local, line)| {
                    let declared = declaration(&line, &local.cname);
                    (local, declared)
                }).collect();
            }
            sets.push((test.clone(), assigned.join(" ")));
            drops.push((test, chosen.join(" ")));
            if sets.last().is_some_and(|(test, _)| test.is_none()) {
                break;
            }
        }
        let chain = |ways: Vec<(Option<String>, String)>| {
            let mut line = String::new();
            for (position, (test, body)) in ways.into_iter().enumerate() {
                let _ = match (test, position) {
                    (Some(test), 0) => write!(line, "if ({test}) {{ {body} }}"),
                    (Some(test), _) => write!(line, " else if ({test}) {{ {body} }}"),
                    (None, 0) => write!(line, "{{ {body} }}"),
                    (None, _) => write!(line, " else {{ {body} }}"),
                };
            }
            line
        };
        if let Some(last) = names.last_mut() {
            last.1 = format!("{} {}", last.1, chain(sets));
        }
        let chosen = match drops.iter().all(|(_, body)| body.is_empty()) {
            true => Vec::new(),
            false => vec![chain(drops)],
        };
        Ok((names, chosen))
    }

    /// `match`, written as statements: a chain of tests on the tag (or on the number), each
    /// arm a block. An arm with `when` that does not hold goes on to the arms below it.
    ///
    /// A subject that is owned is taken apart: the names of an arm own what the variant
    /// carried, what the pattern skips is dropped, and a `T*` subject gives its box back. A
    /// subject that is borrowed is only looked at: the names are borrows.
    fn match_stmt(&mut self, expr: &'a Expr, dest: &Dest, depth: usize, out: &mut String) -> Res<()> {
        let ExprKind::Match { subject, arms, borrowed } = &expr.kind else {
            unreachable!("only called for a `match`");
        };
        let (pad, inner) = ("    ".repeat(depth), "    ".repeat(depth + 1));
        let value = self.first(subject, true)?;
        let ctype = self.ctype(&subject.ty, subject.span)?;
        let held = self.temp();
        self.flush_pre(&pad, out);
        let _ = writeln!(out, "{pad}{{");
        let _ = writeln!(out, "{inner}{ctype} {held} = {value};");
        // The names of a borrowing `match` point into the subject: what it was made from
        // stays until the arms are done.
        let kept = if *borrowed { self.hold() } else { None };
        self.flush_post(&inner, out);

        // What the arms test: a number, a `bool`, a `T?`, or the enum behind the borrow or
        // the pointer.
        let mut core = subject.ty.strip_borrow();
        let mut boxes = 0;
        while let Ty::Ptr(behind) = core {
            (core, boxes) = (behind, boxes + 1);
        }
        let literal = matches!(core, Ty::Int(_) | Ty::Char | Ty::Str | Ty::Bool | Ty::Nullable(_));
        let variants = self.variants(core, subject.span)?;
        if (!literal && variants.is_none()) || (literal && boxes > 0) || boxes > 1 {
            return Err(not_yet(subject.span, &format!("a `match` on `{}`", subject.ty)));
        }
        // `read` if the value is reached through a `read` anywhere, otherwise `ref`.
        let mode = match (*borrowed, &subject.ty) {
            (false, _) => None,
            (true, Ty::Ref(behind)) if !behind.is_borrow() => Some(Mode::Ref),
            (true, _) => Some(Mode::Read),
        };

        let guarded = arms.iter().any(|arm| arm.guard.is_some());
        let end = format!("{held}_end");
        // On a `T?`, once an arm without a condition has taken `null`, a name below it takes
        // what is there (the checker gave it the type without `?`).
        let mut null_taken = false;
        for (position, arm) in arms.iter().enumerate() {
            let last = position + 1 == arms.len();
            let test = self.pattern_test(&arm.pattern, &held, &subject.ty, subject.span)?;
            // Every value has an arm (the checker saw to it), so the last one needs no test.
            let test = test.filter(|_| !(last && arm.guard.is_none()));
            let _ = match (&test, guarded || position == 0) {
                (Some(test), true) => writeln!(out, "{inner}if ({test}) {{"),
                (Some(test), false) => writeln!(out, "{inner}}} else if ({test}) {{"),
                (None, true) => writeln!(out, "{inner}{{"),
                (None, false) => writeln!(out, "{inner}}} else {{"),
            };
            let mut body = depth + 2;
            let mut body_pad = "    ".repeat(body);
            let before = self.gone();
            self.scopes.push(Vec::new());

            // The names of the pattern, and what an owned subject still has to give up once
            // the arm is chosen.
            let mut chosen = Vec::new();
            match &arm.pattern.kind {
                PatternKind::Bind(name) if null_taken => {
                    let (_, inner, inner_ty) = self.nullable_at(&held, &subject.ty).expect("`null` is taken only from a `T?`");
                    let inner_ctype = self.ctype(&inner_ty, subject.span)?;
                    let cname = self.declare(name, inner_ty);
                    let _ = writeln!(out, "{body_pad}{inner_ctype} {cname} = {inner};");
                }
                PatternKind::Bind(name) => {
                    let cname = self.declare(name, subject.ty.clone());
                    let _ = writeln!(out, "{body_pad}{ctype} {cname} = {held};");
                }
                _ if has_choice(&arm.pattern) && !arm.pattern.names().is_empty() => {
                    let (names, drops) = self.take_apart_choices(&arm.pattern, &held, &subject.ty, mode, subject.span)?;
                    for (local, line) in names {
                        let _ = writeln!(out, "{body_pad}{line}");
                        self.scopes.last_mut().expect("an open scope").push(local);
                    }
                    chosen = drops;
                }
                _ => {
                    let mut names = Vec::new();
                    self.take_apart(&arm.pattern, &held, &subject.ty, mode, &mut names, &mut chosen)?;
                    for (local, line) in names {
                        let _ = writeln!(out, "{body_pad}{line}");
                        self.scopes.last_mut().expect("an open scope").push(local);
                    }
                }
            }

            null_taken |= arm.guard.is_none() && takes_null(&arm.pattern);
            if let Some(guard) = &arm.guard {
                let mut cond = self.value(guard)?;
                self.flush_pre(&body_pad, out);
                if !self.post.is_empty() {
                    let temp = self.temp();
                    let _ = writeln!(out, "{body_pad}bool {temp} = {cond};");
                    self.flush_post(&body_pad, out);
                    cond = temp;
                }
                let _ = writeln!(out, "{body_pad}if ({cond}) {{");
                body += 1;
                body_pad = "    ".repeat(body);
            }
            for line in chosen {
                let _ = writeln!(out, "{body_pad}{line}");
            }
            self.stmts(&arm.body, body, out)?;
            if let Some(value) = &arm.value {
                self.arm_value(value, dest, body, out)?;
            }
            // An arm that leaves (or returns its value) has dropped what it had on the way out.
            let leaves = match &arm.value {
                Some(_) => matches!(dest, Dest::Return),
                None => ends_by_leaving(&arm.body),
            };
            if !leaves {
                for line in self.drops(self.scopes.len() - 1) {
                    let _ = writeln!(out, "{body_pad}{line}");
                }
                if guarded {
                    let _ = writeln!(out, "{body_pad}goto {end};");
                }
            }
            self.scopes.pop();
            self.gone_only_if(&before);
            if arm.guard.is_some() {
                let _ = writeln!(out, "{}}}", "    ".repeat(body - 1));
            }
            if guarded {
                let _ = writeln!(out, "{inner}}}");
            }
        }
        if guarded {
            let _ = writeln!(out, "{inner}{end}:;");
        } else {
            let _ = writeln!(out, "{inner}}}");
        }
        self.release(kept, &inner, out);
        let _ = writeln!(out, "{pad}}}");
        Ok(())
    }

    /// The value an arm of a `match` ends with, sent where the value of the `match` goes.
    fn arm_value(&mut self, value: &'a Expr, dest: &Dest, depth: usize, out: &mut String) -> Res<()> {
        let pad = "    ".repeat(depth);
        match dest {
            // A `match` on the last line of an arm gives its value to the same place.
            _ if matches!(value.kind, ExprKind::Match { .. }) => self.match_stmt(value, dest, depth, out)?,
            // `panic(...)` gives no value: the program stops there.
            _ if value.ty == Ty::Never => self.expr_stmt(value, depth, out)?,
            Dest::Unused => self.expr_stmt(value, depth, out)?,
            Dest::Place(place, ty) => {
                let value = self.head_value(value, ty)?;
                self.flush_pre(&pad, out);
                let _ = writeln!(out, "{pad}{place} = {value};");
                self.flush_post(&pad, out);
            }
            Dest::Return => {
                for line in self.return_lines(Some(value), &[], value.span)? {
                    let _ = writeln!(out, "{pad}{line}");
                }
            }
        }
        Ok(())
    }

    /// The lines of a `return`: the value is ready before anything it may be made from is
    /// dropped. `pending`: what the statement around it still has to drop, when the `return`
    /// stands inside an expression (`x ?? return y`). Uses up the lines waiting in `pre` and `post`.
    fn return_lines(&mut self, value: Option<&'a Expr>, pending: &[String], span: Span) -> Res<Vec<String>> {
        self.ways_out += 1;
        let ret = self.ret.clone();
        let value = match value {
            Some(value) => Some(self.head_value(value, &ret)?),
            None => None,
        };
        let mut lines = std::mem::take(&mut self.pre);
        let post = std::mem::take(&mut self.post);
        let drops = self.drops(0);
        match value {
            Some(value) if !drops.is_empty() || !post.is_empty() || !pending.is_empty() => {
                let (ctype, temp) = (self.ctype(&ret, span)?, self.temp());
                lines.push(format!("{ctype} {temp} = {value};"));
                lines.extend(post);
                lines.extend(pending.iter().cloned());
                lines.extend(drops);
                lines.push(format!("return {temp};"));
            }
            Some(value) => lines.push(format!("return {value};")),
            None => {
                lines.extend(post);
                lines.extend(pending.iter().cloned());
                lines.extend(drops);
                lines.push("return;".to_string());
            }
        }
        Ok(lines)
    }

    /// The value of an expression that is the whole right side of a statement, shaped for where
    /// it goes. Nothing else of the statement has been computed yet, so a `try` or a
    /// `?? return` in it can leave the function and leave nothing behind.
    fn head_value(&mut self, expr: &'a Expr, want: &Ty) -> Res<String> {
        self.first_as(expr, want, true)
    }

    /// The value of the part of an expression that is computed first. `head`: whether the
    /// expression itself stood where leaving the function is possible (see `Emitter::head`).
    fn first(&mut self, expr: &'a Expr, head: bool) -> Res<String> {
        self.head = head;
        let value = self.value(expr);
        self.head = false;
        value
    }

    fn first_as(&mut self, expr: &'a Expr, want: &Ty, head: bool) -> Res<String> {
        self.head = head;
        let value = self.value_as(expr, want);
        self.head = false;
        value
    }

    /// The test that the `T?` or `Result` kept in `temp` holds a value, and that value. A
    /// borrowed `T?` (what `get` gives) holds a borrow of the value.
    fn held_in(&self, ty: &Ty, temp: &str) -> Option<(String, String)> {
        // A `read T?` holds a borrow of the value: the pointer itself.
        if let Some(option) = lent_option(ty) {
            return Some((self.lent_holds(option, temp), temp.to_string()));
        }
        Some(match ty {
            // Success that carries nothing gives nothing: the expression is a `void` one.
            Ty::Result(ok, _) if **ok == Ty::Void => (format!("{temp}.tag == 0"), String::new()),
            Ty::Result(..) => (format!("{temp}.tag == 0"), format!("{temp}.u.v0.m_value")),
            Ty::Nullable(_) if is_option(ty) => (format!("{temp}.has"), format!("{temp}.value")),
            Ty::Nullable(_) => (temp.to_string(), temp.to_string()),
            Ty::Read(inner) | Ty::Ref(inner) => match &**inner {
                Ty::Nullable(_) if is_option(inner) => (format!("{temp}->has"), format!("(&{temp}->value)")),
                Ty::Nullable(_) => (format!("(*{temp})"), temp.to_string()),
                _ => return None,
            },
            _ => return None,
        })
    }

    /// Opens a `T?` or a `Result` kept in `temp`: gives what it holds, or runs `exit` (which
    /// leaves the function) when there is no value.
    fn open(&mut self, ty: &Ty, temp: &str, value: String, exit: Vec<String>, span: Span) -> Res<String> {
        let ctype = self.ctype(ty, span)?;
        let Some((test, held)) = self.held_in(ty, temp) else { return Err(not_yet(span, &format!("opening a `{ty}`"))) };
        self.exits += 1;
        self.ways_out += 1;
        Ok(format!("({{ {ctype} {temp} = {value}; if (!({test})) {{ {} }} {held}; }})", exit.join(" ")))
    }

    /// The value of an expression, shaped for where it goes: a `T` that goes where a `T?` is
    /// expected gets its flag, and `null` is written for the type it stands for.
    fn value_as(&mut self, expr: &'a Expr, want: &Ty) -> Res<String> {
        let value = match expr.kind {
            ExprKind::Null => String::new(),
            _ => self.value(expr)?,
        };
        self.lift(value, &expr.ty, want, expr.span)
    }

    fn lift(&mut self, value: String, found: &Ty, want: &Ty, span: Span) -> Res<String> {
        match want {
            Ty::Nullable(inner) if is_option(want) => {
                let ctype = self.ctype(want, span)?;
                match found {
                    Ty::Nullable(_) | Ty::Never => Ok(value),
                    Ty::Null => Ok(format!("(({ctype}){{ false }})")),
                    _ => Ok(format!("(({ctype}){{ true, {} }})", self.lift(value, found, inner, span)?)),
                }
            }
            Ty::Nullable(inner) => match found {
                Ty::Null => Ok("NULL".to_string()),
                Ty::Nullable(_) | Ty::Never => Ok(value),
                _ => self.lift(value, found, inner, span),
            },
            // Where a `read T?` is expected: `null` is no pointer, a borrowed `T` is the pointer
            // it already is, and a `ref T?` is tested for its flag.
            Ty::Read(to) if is_option(to) => match found {
                Ty::Null => Ok(self.lent_none(to).to_string()),
                Ty::Never => Ok(value),
                _ if lent_option(found).is_some() => Ok(value),
                Ty::Ref(inner) if inner == to => self.lend(&value, to, span),
                Ty::Ref(inner) if **inner == Ty::Str => Ok(format!("sg_view({value})")),
                _ if found.is_borrow() && !matches!(found.strip_borrow(), Ty::Nullable(_)) => Ok(value),
                _ => Err(not_yet(span, &format!("a `{found}` lent as `{want}`"))),
            },
            _ if *found == Ty::Null => Err(not_yet(span, "`null` here")),
            // A pointer to a value is not a pointer to a value and a flag.
            Ty::Ref(to) if is_option(to) && found.is_borrow() && !matches!(found.strip_borrow(), Ty::Nullable(_)) => {
                Err(not_yet(span, &format!("a `{found}` lent as `{want}`")))
            }
            _ => Ok(value),
        }
    }

    /// A function called with its arguments, each shaped for its parameter.
    fn call(&mut self, name: String, receiver: Option<Part>, args: &'a [Expr], head: bool, gives: &Ty) -> Res<String> {
        let params = self.sigs.get(&name).cloned().unwrap_or_default();
        // An argument can leave the function for as long as no argument before it owns anything.
        let mut taking = Taking { head: head && receiver.is_none(), handed: Vec::new() };
        let mut parts: Vec<Part> = receiver.into_iter().collect();
        for (position, arg) in args.iter().enumerate() {
            let before = self.mark();
            let (value, ty) = match params.get(position) {
                Some(want) => (self.first_as(arg, want, taking.head)?, want.clone()),
                None => (self.first(arg, taking.head)?, arg.ty.clone()),
            };
            parts.push(self.taken(arg, value, &ty, before, args[position + 1..].iter().any(may_leave), &mut taking)?);
        }
        let (values, names) = self.in_order(parts)?;
        let span = args.first().map_or(Span::new(0, 0), |arg| arg.span);
        let called = self.handed_over(format!("{name}({})", values.join(", ")), gives, &taking.handed, span)?;
        Ok(sequence(&names, called))
    }

    /// A value that owns something and has no name, given a name for the rest of the
    /// statement: a part written after it can then leave the function (`try`, `?? return`),
    /// because what it frees on its way out is what has a name. What is given is the name
    /// (the value is handed over by its bytes), and the line that empties the name once the
    /// value has been handed over, so that the drop at the end of the statement finds nothing.
    fn keep(&mut self, value: String, ty: &Ty, span: Span) -> Res<(String, String)> {
        let (ctype, temp) = (self.ctype(ty, span)?, self.temp());
        self.pre.push(format!("{ctype} {temp} = {value};"));
        self.post.push(self.drop_of(ty, &temp));
        let handed = format!("(void){};", self.move_of(ty, &temp));
        Ok((temp, handed))
    }

    /// What took over the kept values, with their names emptied right after it.
    fn handed_over(&mut self, taker: String, ty: &Ty, handed: &[String], span: Span) -> Res<String> {
        if handed.is_empty() {
            return Ok(taker);
        }
        let handed = handed.join(" ");
        if matches!(ty, Ty::Void | Ty::Never) {
            return Ok(format!("({{ {taker}; {handed} }})"));
        }
        let (ctype, temp) = (self.ctype(ty, span)?, self.temp());
        Ok(format!("({{ {ctype} {temp} = {taker}; {handed} {temp}; }})"))
    }

    /// One of the values that a call, a struct or an enum value takes over, written with
    /// `head` as it stands. If it owns something and a value after it can leave the function,
    /// it is kept under a name first; otherwise nothing after it can leave.
    ///
    /// A kept value stays under its name, where a way out finds it, until all the values are
    /// computed; `handed` gets the line that empties the name after that.
    fn taken(&mut self, expr: &'a Expr, value: String, ty: &Ty, before: (usize, usize), later_leaves: bool, taking: &mut Taking) -> Res<Part> {
        if !self.droppable(ty) {
            return Ok(self.part(expr, ty, value, before));
        }
        if taking.head && later_leaves {
            let (value, line) = self.keep(value, ty, expr.span)?;
            taking.handed.push(line);
            let mut part = self.part(expr, ty, value, before);
            (part.settled, part.effect) = (true, Effect::Reads);
            return Ok(part);
        }
        taking.head = false;
        Ok(self.part(expr, ty, value, before))
    }

    /// What computing the expression can do besides giving its value.
    fn effect(&self, expr: &Expr) -> Effect {
        let of = |inner: &Expr| self.effect(inner);
        let all = |list: &mut dyn Iterator<Item = &Expr>| list.map(|inner| self.effect(inner)).max().unwrap_or(Effect::Reads);
        match &expr.kind {
            ExprKind::Int(..) | ExprKind::Float(..) | ExprKind::Bool(_) | ExprKind::Str(_) | ExprKind::Char(_) | ExprKind::Null | ExprKind::Var(_) | ExprKind::FuncRef(_) | ExprKind::Constant { .. } => Effect::Reads,
            ExprKind::Field { recv: inner, .. }
            | ExprKind::New(inner)
            | ExprKind::NewShared(inner)
            | ExprKind::Share(inner)
            | ExprKind::WeakOf(inner)
            | ExprKind::Load(inner)
            | ExprKind::Unbox(inner)
            | ExprKind::Widen(_, inner)
            | ExprKind::IsNull { expr: inner, .. }
            | ExprKind::IsVariant { expr: inner, .. }
            | ExprKind::Unary(UnOp::Not | UnOp::BitNot, inner) => of(inner),
            // A code that is not a character stops the program in every build.
            ExprKind::Cast { ty: Ty::Char, expr: inner } if *inner.ty.strip_borrow() != Ty::Char => of(inner).max(Effect::Stops),
            ExprKind::Unary(UnOp::Neg, inner) | ExprKind::Cast { expr: inner, .. } => of(inner).max(if self.checked { Effect::Stops } else { Effect::Reads }),
            ExprKind::Binary(op, left, right) => {
                let own = match op {
                    BinOp::Div | BinOp::Rem => Effect::Stops,
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Shl | BinOp::Shr if self.checked => Effect::Stops,
                    _ => Effect::Reads,
                };
                own.max(of(left)).max(of(right))
            }
            ExprKind::Index { recv, index, .. } => Effect::Stops.max(of(recv)).max(of(index)),
            ExprKind::Slice { recv, from, to } => Effect::Stops.max(of(recv)).max(all(&mut from.iter().chain(to).map(|inner| &**inner))),
            ExprKind::Interp(parts) => all(&mut parts.iter().filter_map(|part| match part {
                InterpPart::Expr(inner, _) => Some(inner),
                InterpPart::Text(_) => None,
            })),
            ExprKind::StructLit { fields, .. } => all(&mut fields.iter().map(|field| &field.value)),
            ExprKind::Variant { args, .. } => all(&mut args.iter()),
            ExprKind::Borrow { mutable: true, .. } => Effect::Changes,
            ExprKind::Borrow { expr: inner, .. } => of(inner),
            ExprKind::Coalesce { left, right, .. } => of(left).max(of(right)),
            ExprKind::Call { args, .. } => Effect::Acts.max(all(&mut args.iter())),
            ExprKind::Method { recv, name, args, callee, .. } => {
                let own = match callee {
                    Callee::Method { this: This::Ref, .. } | Callee::Take | Callee::Replace => Effect::Changes,
                    // The built-in methods of numbers and text compute a value and nothing else.
                    // A list is only read by these.
                    Callee::Plain if list_behind(&recv.ty).is_some() && matches!(name.as_str(), "length" | "get" | "indexOf" | "contains") => Effect::Reads,
                    Callee::Plain if list_behind(&recv.ty).is_some() => Effect::Changes,
                    Callee::Plain if items_behind(&recv.ty).is_some() && matches!(name.as_str(), "sort" | "reverse") => Effect::Changes,
                    Callee::Plain if table_behind(&recv.ty).is_some() && matches!(name.as_str(), "length" | "get" | "contains") => Effect::Reads,
                    Callee::Plain if matches!(recv.ty.strip_borrow(), Ty::Map(..) | Ty::Set(_) | Ty::Formatter) => Effect::Changes,
                    Callee::Plain if matches!(name.as_str(), "write" | "clear") && (is_text(&recv.ty) || is_text_behind_pointer(&recv.ty)) => Effect::Changes,
                    // These allocate, and `replaceAll` and `repeat` can stop the program.
                    Callee::Plain if matches!(name.as_str(), "clone" | "toString" | "replaceAll" | "repeat") => Effect::Acts,
                    // `next` moves the iterator on; `split` stops at a separator that is empty.
                    Callee::Plain if matches!(recv.ty.strip_borrow(), Ty::TextIter(_)) => Effect::Changes,
                    Callee::Plain if name == "split" => Effect::Stops,
                    Callee::Plain => Effect::Reads,
                    _ => Effect::Acts,
                };
                own.max(of(recv)).max(all(&mut args.iter()))
            }
            ExprKind::Try { .. } | ExprKind::Return(_) => Effect::Acts,
            ExprKind::Match { .. } | ExprKind::Collection { .. } => Effect::Changes,
            // An array is its items and nothing else: nothing is allocated for it.
            ExprKind::Array { args, items, .. } => all(&mut args.iter().chain(items.iter().flatten())),
            ExprKind::AsSlice(inner) => of(inner),
        }
    }

    /// Where the writing of a part starts: how many lines `pre` has, and how many ways out
    /// of the function were written.
    fn mark(&self) -> (usize, usize) {
        (self.pre.len(), self.exits)
    }

    /// One of the values of a call, an operator or a literal. `before`: the `mark` taken
    /// when the value started to be written.
    fn part(&mut self, expr: &'a Expr, ty: &Ty, value: String, before: (usize, usize)) -> Part {
        let ctype = match (&expr.ty, ty) {
            (Ty::Never | Ty::Void, _) | (_, Ty::Never | Ty::Void | Ty::Null) => String::new(),
            _ => self.ctype(ty, expr.span).unwrap_or_default(),
        };
        Part { ctype, value, effect: self.effect(expr), settled: is_constant(expr), pre: (before.0, self.pre.len()), leaves: self.exits > before.1, span: expr.span }
    }

    /// Makes sure the parts are computed from left to right where the order can be seen: C
    /// leaves the order of the arguments of a call and of the operands of an operator open.
    /// Gives the parts as they are to be written, and the C declarations that have to run
    /// first (see `sequence`), which keep the earlier parts in variables.
    ///
    /// The order can be seen when two parts can each stop the program or call a function, or
    /// when one part can change what another reads. A part that put lines before the statement
    /// (a value kept in a temporary) is already computed there, so the parts before it go
    /// there too.
    ///
    /// A part that can leave the function frees the temporaries made before it on its way
    /// out, so it has to run before the lines of the parts after it; moving it there is the
    /// same step.
    fn in_order(&mut self, mut parts: Vec<Part>) -> Res<(Vec<String>, String)> {
        let values = |parts: &[Part]| parts.iter().map(|part| part.value.clone()).collect::<Vec<_>>();
        if parts.iter().any(|part| part.ctype.is_empty()) {
            // Nothing can be moved here; then nothing may need to be.
            if let Some(leaving) = parts.iter().position(|part| part.leaves) {
                if parts[leaving + 1..].iter().any(|part| part.pre.1 > part.pre.0) {
                    return Err(not_yet(parts[leaving].span, "leaving the function from here").hint("give the value a name first: `var x = try ...`"));
                }
            }
            return Ok((values(&parts), String::new()));
        }
        if let Some(last) = parts.iter().rposition(|part| part.pre.1 > part.pre.0) {
            for (added, part) in parts[..last].iter_mut().filter(|part| !part.settled).enumerate() {
                let temp = self.temp();
                self.pre.insert(part.pre.1 + added, format!("{} {temp} = {};", part.ctype, part.value));
                (part.value, part.settled, part.effect) = (temp, true, Effect::Reads);
            }
        }
        let open: Vec<usize> = (0..parts.len()).filter(|index| !parts[*index].settled).collect();
        let seen = open.iter().filter(|index| parts[**index].effect >= Effect::Stops).count() >= 2
            || (open.len() >= 2 && open.iter().any(|index| parts[*index].effect == Effect::Changes));
        let mut names = String::new();
        if seen {
            let last_effect = open.iter().rposition(|index| parts[*index].effect >= Effect::Stops).expect("an order that can be seen has an effect");
            // The last part can stay where it is: everything before it is already computed.
            let keep = if last_effect + 1 == open.len() { last_effect } else { last_effect + 1 };
            for index in &open[..keep] {
                let temp = self.temp();
                let _ = write!(names, "{} {temp} = {}; ", parts[*index].ctype, parts[*index].value);
                parts[*index].value = temp;
            }
        }
        Ok((values(&parts), names))
    }

    /// An expression whose value is not kept: it is computed, and dropped if it owns something.
    fn unused(&mut self, expr: &'a Expr, pad: &str, out: &mut String) -> Res<()> {
        let value = self.head_value(expr, &expr.ty)?;
        self.flush_pre(pad, out);
        if self.droppable(&expr.ty) {
            let (ctype, temp) = (self.ctype(&expr.ty, expr.span)?, self.temp());
            let _ = writeln!(out, "{pad}{{ {ctype} {temp} = {value}; {} }}", self.drop_of(&expr.ty, &temp));
        } else if expr.ty == Ty::Void || expr.ty == Ty::Never {
            let _ = writeln!(out, "{pad}{value};");
        } else {
            let _ = writeln!(out, "{pad}(void)({value});");
        }
        self.flush_post(pad, out);
        Ok(())
    }

    /// What `printf` is told and what it is given, for `print`, `panic` and `.toString()`: a
    /// number, a `bool`, text, or `$"..."` made of those. The values come with a leading comma.
    fn format(&mut self, arg: &'a Expr) -> Res<(String, String)> {
        match self.pieces(arg, false)?.pop() {
            Some(Piece::Text(format, values)) => Ok((format, values)),
            _ => unreachable!("a line that is not written piece by piece is one format"),
        }
    }

    /// The pieces of a line. With `straight`, the line is added to a text that already
    /// exists (`f.write(...)`), and a struct, an enum or a collection in it is written
    /// there by the helper of its type, as a piece of its own between the others. Without
    /// it there is one piece, a `printf` format, and such a value is written into the
    /// scratch block first.
    fn pieces(&mut self, arg: &'a Expr, straight: bool) -> Res<Vec<Piece>> {
        let mut done = Vec::new();
        let mut format = String::new();
        let mut values: Vec<String> = Vec::new();
        match &arg.kind {
            ExprKind::Str(text) => format.push_str(&text.replace('%', "%%")),
            ExprKind::Interp(parts) => {
                // The values are given to `printf` as arguments, and C computes those in an
                // order of its own. Where the order can be seen, each is computed on a line
                // of its own first.
                let exprs: Vec<&Expr> = parts.iter().filter_map(|part| match part {
                    InterpPart::Expr(expr, _) if !matches!(expr.kind, ExprKind::Str(_)) => Some(expr),
                    _ => None,
                }).collect();
                let effects: Vec<Effect> = exprs.iter().map(|expr| self.effect(expr)).collect();
                let changes = effects.iter().position(|effect| *effect == Effect::Changes);
                let first = effects.iter().filter(|effect| **effect >= Effect::Stops).count() >= 2 || (changes.is_some() && exprs.len() >= 2);
                // A text is looked at where it is; a later value that could change it would
                // leave the look pointing at freed bytes.
                if let Some(changes) = changes {
                    if let Some(text) = exprs[..changes].iter().find(|expr| is_text(&expr.ty) || is_text_behind_pointer(&expr.ty)) {
                        return Err(not_yet(text.span, "a text written before a value that lends with `ref` on the same line")
                            .hint("give the later value a name first"));
                    }
                }
                // The rule of the language: the values of a line are computed first, from
                // left to right, and written after that. A struct, an enum or a collection
                // is written by its helper, which can run the type's own `format`; so the
                // helpers run after every value of the line is there, and a value that does
                // more than read is computed on a line of its own before them.
                let shown = exprs.iter().any(|expr| has_helper(&expr.ty));
                let mut late = Vec::new();
                for part in parts {
                    match part {
                        InterpPart::Text(text) => format.push_str(&text.replace('%', "%%")),
                        InterpPart::Expr(expr, spec) if has_helper(&expr.ty) => {
                            let width = spec.map_or(0, |spec| spec.width);
                            let shower = self.shower(expr.ty.strip_borrow(), expr.span)?;
                            let mut at = self.pointer(expr)?;
                            // Finding the value can stop the program (an index): that is
                            // part of computing it, so it is done in its turn.
                            if self.effect(expr) != Effect::Reads {
                                let temp = self.temp();
                                self.pre.push(format!("const void* {temp} = {at};"));
                                at = temp;
                            }
                            if straight {
                                if !format.is_empty() {
                                    done.push(Piece::Text(std::mem::take(&mut format), values.drain(..).map(|value| format!(", {value}")).collect()));
                                }
                                done.push(Piece::Show(shower, at, width));
                            } else {
                                late.push(self.shown(&shower, &at, &mut format, &mut values));
                                widen(&mut format, &mut values, width, false);
                            }
                        }
                        InterpPart::Expr(expr, spec) => {
                            let from = values.len();
                            self.formatted(expr, *spec, &mut format, &mut values)?;
                            // A number or a `bool`: one value. (A text is already kept before the line.)
                            let text = is_text(&expr.ty) || is_text_behind_pointer(&expr.ty);
                            let before_helpers = shown && self.effect(expr) != Effect::Reads;
                            if (first || before_helpers) && !text && values.len() == from + 1 {
                                let ctype = match &values[from] {
                                    value if value.ends_with("\"false\"") => "const char*",
                                    value if value.starts_with("(unsigned long long)") => "unsigned long long",
                                    _ => "long long",
                                };
                                let temp = self.temp();
                                self.pre.push(format!("{ctype} {temp} = {};", values[from]));
                                values[from] = temp;
                            }
                        }
                    }
                }
                self.pre.append(&mut late);
            }
            _ if straight && has_helper(&arg.ty) => {
                let shower = self.shower(arg.ty.strip_borrow(), arg.span)?;
                done.push(Piece::Show(shower, self.pointer(arg)?, 0));
            }
            _ => self.formatted(arg, None, &mut format, &mut values)?,
        }
        if !format.is_empty() || done.is_empty() {
            done.push(Piece::Text(format, values.iter().map(|value| format!(", {value}")).collect()));
        }
        Ok(done)
    }

    /// The line that writes a value with the helper of its type above what the scratch block
    /// holds; the room is given back after the statement. The block can move while it grows,
    /// so the text is found from its start at the moment it is used.
    fn shown(&mut self, shower: &str, at: &str, format: &mut String, values: &mut Vec<String>) -> String {
        let temp = self.temp();
        format.push_str("%.*s");
        self.post.push(format!("sg_scratch_back({temp});"));
        values.extend([format!("(int){temp}n"), format!("(const char*)sg_scratch.ptr + {temp}")]);
        format!("int64_t {temp} = sg_scratch.len; {shower}(&sg_scratch, {at}); int64_t {temp}n = sg_scratch.len - {temp};")
    }

    /// A fractional number inside a formatted line: written as text on a line of its own,
    /// so it is computed in its turn, and given to `printf` as that text.
    fn float_text(&mut self, ty: FloatTy, value: &str, format: &mut String, values: &mut Vec<String>) {
        let temp = self.temp();
        self.pre.push(format!("char {temp}[40]; int {temp}n = sg_shortest({temp}, {value}, {});", ty == FloatTy::Float));
        format.push_str("%.*s");
        values.push(format!("{temp}n"));
        values.push(temp);
    }

    /// What a text is searched for, as a look at text: the lines that make it ready and the
    /// value. A character becomes its UTF-8 bytes in a buffer named after `temp`, a byte a
    /// text of one byte.
    fn needle(&mut self, arg: &'a Expr, temp: &str) -> Res<(String, String)> {
        match arg.ty.strip_borrow() {
            Ty::Char => {
                let value = self.value(arg)?;
                Ok((format!("uint8_t {temp}b[4]; sg_str {temp}n = sg_char_str({temp}b, {value});"), format!("{temp}n")))
            }
            Ty::Int(_) => {
                let value = self.value(arg)?;
                Ok((format!("uint8_t {temp}b[1] = {{ {value} }}; sg_str {temp}n = {{ {temp}b, 1 }};"), format!("{temp}n")))
            }
            _ => {
                let value = self.view(arg)?;
                Ok((format!("sg_str {temp}n = {value};"), format!("{temp}n")))
            }
        }
    }

    /// A character inside a formatted line: its UTF-8 bytes, on a line of their own.
    fn char_text(&mut self, value: &str, format: &mut String, values: &mut Vec<String>) {
        let temp = self.temp();
        self.pre.push(format!("char {temp}[4]; int {temp}n = sg_utf8({temp}, {value});"));
        format.push_str("%.*s");
        values.push(format!("{temp}n"));
        values.push(temp);
    }

    /// One value inside a formatted line, with what is written after it inside `{ }`.
    fn formatted(&mut self, expr: &'a Expr, spec: Option<Spec>, format: &mut String, values: &mut Vec<String>) -> Res<()> {
        let width = spec.map_or(0, |spec| spec.width);
        if let ExprKind::Str(text) = &expr.kind {
            let fill = " ".repeat((width.unsigned_abs() as usize).saturating_sub(text.chars().count()));
            let text = text.replace('%', "%%");
            format.push_str(&if width > 0 { fill + &text } else { text + &fill });
            return Ok(());
        }
        match spec.and_then(|spec| spec.letter) {
            Some((letter, digits)) => self.lettered(expr, letter, digits.unwrap_or(0), format, values)?,
            None => self.formatted_as_printed(expr, format, values)?,
        }
        let mut ty = expr.ty.strip_borrow();
        while let Ty::Ptr(inner) = ty {
            ty = inner.strip_borrow();
        }
        let ascii = matches!(ty, Ty::Int(_) | Ty::Float(_) | Ty::Bool);
        widen(format, values, width, ascii);
        Ok(())
    }

    /// A number written with the letter after `:`: `{b:X2}`, `{mask:b8}`, `{n:D3}`, `{x:F2}`.
    /// Hex and decimal digits are `printf`'s (its precision is the digits there are at
    /// least); binary and fixed digits are written on a line of their own, as a fractional
    /// number is, and given as text.
    fn lettered(&mut self, expr: &'a Expr, letter: Letter, digits: u32, format: &mut String, values: &mut Vec<String>) -> Res<()> {
        let (value, ty) = match expr.ty.strip_borrow() {
            Ty::Ptr(_) => {
                let mut place = self.look(expr)?;
                let mut ty = expr.ty.strip_borrow();
                while let Ty::Ptr(inner) = ty {
                    place = format!("(*{place})");
                    ty = inner;
                }
                (place, ty)
            }
            ty => {
                let value = self.value(expr)?;
                (if expr.ty.is_borrow() { format!("(*{value})") } else { value }, ty)
            }
        };
        // `printf` writes no digit for 0 with a precision of 0; C# and Rust write `0`.
        let precision = if digits > 0 { format!(".{digits}") } else { String::new() };
        match (letter, ty) {
            (Letter::Hex | Letter::HexUpper, Ty::Int(ty)) => {
                let _ = write!(format, "%{precision}ll{}", if letter == Letter::Hex { 'x' } else { 'X' });
                values.push(format!("(unsigned long long)(uint{}_t)({value})", ty.bits()));
            }
            (Letter::Decimal, Ty::Int(ty)) => {
                let (shown, cast, _) = int_shown(*ty);
                let _ = write!(format, "%{precision}{}", &shown[1..]);
                values.push(format!("({cast})({value})"));
            }
            (Letter::Binary, Ty::Int(ty)) => {
                let temp = self.temp();
                self.pre.push(format!("char {temp}[{}]; int {temp}n = sg_binary({temp}, (unsigned long long)(uint{}_t)({value}), {digits});", digits.max(64), ty.bits()));
                format.push_str("%.*s");
                values.extend([format!("{temp}n"), temp]);
            }
            (Letter::Fixed, Ty::Float(ty)) => {
                let temp = self.temp();
                self.pre.push(format!("char {temp}[{}]; int {temp}n = sg_fixed({temp}, sizeof {temp}, {value}, {digits}, {});", 330 + digits, *ty == FloatTy::Float));
                format.push_str("%.*s");
                values.extend([format!("{temp}n"), temp]);
            }
            _ => unreachable!("the checker lets a letter through only for a number it fits"),
        }
        Ok(())
    }

    /// One value inside a formatted line, as `print` writes it.
    fn formatted_as_printed(&mut self, expr: &'a Expr, format: &mut String, values: &mut Vec<String>) -> Res<()> {
        if let ExprKind::Str(text) = &expr.kind {
            format.push_str(&text.replace('%', "%%"));
            return Ok(());
        }
        // A struct, an enum or a collection (also behind a pointer) is written by the helper
        // of its type.
        if has_helper(&expr.ty) {
            let shower = self.shower(expr.ty.strip_borrow(), expr.span)?;
            let at = self.pointer(expr)?;
            let line = self.shown(&shower, &at, format, values);
            self.pre.push(line);
            return Ok(());
        }
        // A value on the heap is written like the value: `print(count)` with `int* count`.
        if let Ty::Ptr(_) = expr.ty.strip_borrow() {
            let mut place = self.look(expr)?;
            let mut ty = expr.ty.strip_borrow();
            while let Ty::Ptr(inner) = ty {
                place = format!("(*{place})");
                ty = inner;
            }
            match ty {
                Ty::Int(ty) => {
                    let (shown, cast, _) = int_shown(*ty);
                    format.push_str(shown);
                    values.push(format!("({cast})({place})"));
                }
                Ty::Float(ty) => self.float_text(*ty, &place, format, values),
                Ty::Bool => {
                    format.push_str("%s");
                    values.push(format!("({place}) ? \"true\" : \"false\""));
                }
                Ty::Char => self.char_text(&place, format, values),
                Ty::Str => {
                    format.push_str("%.*s");
                    values.extend([format!("(int){place}.len"), format!("(const char*){place}.ptr")]);
                }
                other => return Err(not_yet(expr.span, &format!("writing a `{other}*` as text"))),
            }
            return Ok(());
        }
        if is_text(&expr.ty) {
            // The text is looked at twice (its length and its bytes), so it gets a name.
            let (view, temp) = (self.view(expr)?, self.temp());
            self.pre.push(format!("sg_str {temp} = {view};"));
            format.push_str("%.*s");
            values.extend([format!("(int){temp}.len"), format!("(const char*){temp}.ptr")]);
            return Ok(());
        }
        let value = self.value(expr)?;
        // What is printed may be a borrow of a number: `print(n)` with `read int n`.
        let value = if expr.ty.is_borrow() { format!("(*{value})") } else { value };
        match expr.ty.strip_borrow() {
            Ty::Int(ty) => {
                let (shown, cast, _) = int_shown(*ty);
                format.push_str(shown);
                values.push(format!("({cast})({value})"));
            }
            Ty::Float(ty) => self.float_text(*ty, &value, format, values),
            Ty::Bool => {
                format.push_str("%s");
                values.push(format!("({value}) ? \"true\" : \"false\""));
            }
            Ty::Char => self.char_text(&value, format, values),
            other => return Err(not_yet(expr.span, &format!("writing a `{other}` as text"))),
        }
        Ok(())
    }

    /// `File.readText(path)` and the others: one function each, made where it is first used,
    /// that calls the runtime (`IO_RUNTIME`) and puts what it gives into the `Result`.
    fn io(&mut self, call: IoCall, args: &'a [Expr], head: bool, expr: &'a Expr) -> Res<String> {
        self.made.insert("io".to_string());
        let span = expr.span;
        let gives = self.ctype(&expr.ty, span)?;
        self.io_error(span)?;
        let failed = |cause: &str| format!("return ({gives}){{ .tag = 1, .u.v1 = {{ {cause} }} }};");
        let text = Ty::Read(Box::new(Ty::Str));
        let (fname, params, body) = match call {
            IoCall::OpenText | IoCall::CreateText | IoCall::OpenAppend => {
                let (fname, open, ctype) = match call {
                    IoCall::OpenText => ("sg_file_open_text", "sg_open_reader(a0, ", "sg_reader"),
                    IoCall::CreateText => ("sg_file_create_text", "sg_open_writer(a0, false, ", "sg_writer"),
                    _ => ("sg_file_open_append", "sg_open_writer(a0, true, ", "sg_writer"),
                };
                (fname, vec![text], format!("{ctype} s; int code = {open}&s); if (code) {} return ({gives}){{ .tag = 0, .u.v0 = {{ s }} }};", failed("sg_io_error(code)")))
            }
            _ => return self.io_whole(call, args, head, expr),
        };
        self.io_glue(fname, &params, &body, &gives, span)?;
        self.call(fname.to_string(), None, args, head, &expr.ty)
    }

    /// `sg_io_error(code)` and `sg_io_bad_text(at)`: the `IoError` for a code the system gave,
    /// and for a byte that is not UTF-8.
    fn io_error(&mut self, span: Span) -> Res<()> {
        self.made.insert("io".to_string());
        let error_c = self.ctype(&Ty::Enum("IoError".to_string()), span)?;
        if self.made.insert("sg_io_error".to_string()) {
            let _ = writeln!(
                self.glue,
                "static {error_c} sg_io_error(int code) {{ int kind = sg_io_kind(code); \
                 if (kind == 5) return ({error_c}){{ .tag = 5, .u.v5 = {{ code }} }}; return ({error_c}){{ .tag = (uint8_t)kind }}; }}\n\
                 static {error_c} sg_io_bad_text(int64_t at) {{ return ({error_c}){{ .tag = 4, .u.v4 = {{ at }} }}; }}"
            );
        }
        Ok(())
    }

    /// A function of the glue that the calls of `File` and the methods of the streams go
    /// through, written once.
    fn io_glue(&mut self, fname: &str, params: &[Ty], body: &str, gives: &str, span: Span) -> Res<()> {
        if self.made.insert(fname.to_string()) {
            let mut written = Vec::new();
            for (index, param) in params.iter().enumerate() {
                written.push(format!("{} a{index}", self.ctype(param, span)?));
            }
            let written = if written.is_empty() { "void".to_string() } else { written.join(", ") };
            let _ = writeln!(self.glue, "static {gives} {fname}({written}) {{ {body} }}");
            self.sigs.insert(fname.to_string(), params.to_vec());
        }
        Ok(())
    }

    /// The functions of `File` that read or write a whole file.
    fn io_whole(&mut self, call: IoCall, args: &'a [Expr], head: bool, expr: &'a Expr) -> Res<String> {
        let span = expr.span;
        let gives = self.ctype(&expr.ty, span)?;
        let failed = |cause: &str| format!("return ({gives}){{ .tag = 1, .u.v1 = {{ {cause} }} }};");
        let text = Ty::Read(Box::new(Ty::Str));
        let (fname, params, body) = match call {
            IoCall::ReadText => (
                "sg_file_read_text",
                vec![text],
                format!(
                    "uint8_t* bytes = NULL; int64_t n = 0; int code = sg_read_file(a0, &bytes, &n); if (code) {} \
                     int64_t bad = sg_utf8_bad(bytes, n); if (bad >= 0) {{ free(bytes); {} }} \
                     if (n == 0) {{ free(bytes); return ({gives}){{ .tag = 0, .u.v0 = {{ SG_LIT(\"\", 0) }} }}; }} \
                     return ({gives}){{ .tag = 0, .u.v0 = {{ (sg_string){{ bytes, n, n }} }} }};",
                    failed("sg_io_error(code)"),
                    failed("sg_io_bad_text(bad)")
                ),
            ),
            IoCall::ReadBytes => {
                let list = self.ctype(&Ty::List(Box::new(Ty::Int(IntTy::Byte))), span)?;
                (
                    "sg_file_read_bytes",
                    vec![text],
                    format!(
                        "uint8_t* bytes = NULL; int64_t n = 0; int code = sg_read_file(a0, &bytes, &n); if (code) {} \
                         if (n == 0) {{ free(bytes); return ({gives}){{ .tag = 0, .u.v0 = {{ ({list}){{ 0 }} }} }}; }} \
                         return ({gives}){{ .tag = 0, .u.v0 = {{ ({list}){{ bytes, n, n }} }} }};",
                        failed("sg_io_error(code)")
                    ),
                )
            }
            IoCall::WriteText | IoCall::AppendText | IoCall::WriteBytes => {
                let (fname, second) = match call {
                    IoCall::WriteText => ("sg_file_write_text", text.clone()),
                    IoCall::AppendText => ("sg_file_append_text", text.clone()),
                    _ => ("sg_file_write_bytes", Ty::Read(Box::new(Ty::Slice(Box::new(Ty::Int(IntTy::Byte)))))),
                };
                (
                    fname,
                    vec![text, second],
                    format!(
                        "int code = sg_write_file(a0, a1.ptr, a1.len, {}); if (code) {} return ({gives}){{ .tag = 0 }};",
                        call == IoCall::AppendText,
                        failed("sg_io_error(code)")
                    ),
                )
            }
            IoCall::OpenText | IoCall::CreateText | IoCall::OpenAppend => unreachable!("written by `io`"),
        };
        self.io_glue(fname, &params, &body, &gives, span)?;
        self.call(fname.to_string(), None, args, head, &expr.ty)
    }

    /// The methods of `StreamReader` and `StreamWriter`. Reading and writing go through the
    /// stream's address; `close` takes the writer.
    fn stream(&mut self, call: StreamCall, recv: &'a Expr, args: &'a [Expr], head: bool, expr: &'a Expr) -> Res<String> {
        let span = expr.span;
        self.io_error(span)?;
        let gives = self.ctype(&expr.ty, span)?;
        let failed = |cause: &str| format!("return ({gives}){{ .tag = 1, .u.v1 = {{ {cause} }} }};");
        // `Ok()`, or the error a code stands for.
        let done = |code: &str| format!("({{ int c_ = {code}; c_ ? ({gives}){{ .tag = 1, .u.v1 = {{ sg_io_error(c_) }} }} : ({gives}){{ .tag = 0 }}; }})");
        let before = self.mark();
        match call {
            StreamCall::Close => {
                let value = match recv.is_place() {
                    true => {
                        let place = self.lvalue(recv)?;
                        self.move_of(&Ty::Writer, &place)
                    }
                    false => self.value(recv)?,
                };
                let receiver = self.part(recv, &Ty::Writer, value, before);
                let body = format!("int code = sg_writer_close(&a0); if (code) {} return ({gives}){{ .tag = 0 }};", failed("sg_io_error(code)"));
                self.io_glue("sg_writer_close_r", &[Ty::Writer], &body, &gives, span)?;
                self.call("sg_writer_close_r".to_string(), Some(receiver), args, head, &expr.ty)
            }
            StreamCall::Lines => Ok(format!("((sg_read_lines){{ {}, false }})", self.list_address(recv)?)),
            StreamCall::Flush => Ok(done(&format!("sg_writer_flush({})", self.list_address(recv)?))),
            StreamCall::Write | StreamCall::WriteLine => {
                let temp = self.temp();
                let out = self.list_address(recv)?;
                let written = self.write_into(&format!("(&{temp}->buf)"), &args[0])?;
                let end = if call == StreamCall::WriteLine { format!(" sg_put(&{temp}->buf, \"\\n\", 1);") } else { String::new() };
                Ok(format!("({{ sg_writer* {temp} = {out}; {written};{end} {}; }})", done(&format!("sg_writer_wrote({temp})"))))
            }
            StreamCall::ReadLine | StreamCall::ReadText | StreamCall::ReadBytes | StreamCall::WriteBytes => {
                let reader = match call {
                    StreamCall::WriteBytes => Ty::Ref(Box::new(Ty::Writer)),
                    _ => Ty::Ref(Box::new(Ty::Reader)),
                };
                let receiver = self.list_address(recv)?;
                let receiver = self.part(recv, &reader, receiver, before);
                let bytes = |mode: fn(Box<Ty>) -> Ty| mode(Box::new(Ty::Slice(Box::new(Ty::Int(IntTy::Byte)))));
                let (fname, params, body) = match call {
                    StreamCall::ReadBytes => (
                        "sg_reader_bytes_r",
                        vec![reader, bytes(Ty::Ref)],
                        format!(
                            "int64_t got = 0; int code = sg_read_bytes(a0, a1.ptr, a1.len, &got); if (code) {} \
                             return ({gives}){{ .tag = 0, .u.v0 = {{ got }} }};",
                            failed("sg_io_error(code)")
                        ),
                    ),
                    StreamCall::WriteBytes => (
                        "sg_writer_bytes_r",
                        vec![reader, bytes(Ty::Read)],
                        format!("int code = sg_writer_bytes(a0, a1.ptr, a1.len); if (code) {} return ({gives}){{ .tag = 0 }};", failed("sg_io_error(code)")),
                    ),
                    StreamCall::ReadLine => (
                        "sg_reader_line_r",
                        vec![reader, Ty::Ref(Box::new(Ty::Str))],
                        format!(
                            "bool more = false; int64_t bad = 0; int code = sg_read_line(a0, a1, &more, &bad); \
                             if (code == -1) {} if (code) {} return ({gives}){{ .tag = 0, .u.v0 = {{ more }} }};",
                            failed("sg_io_bad_text(bad)"),
                            failed("sg_io_error(code)")
                        ),
                    ),
                    _ => (
                        "sg_reader_text_r",
                        vec![reader],
                        format!(
                            "int64_t start = a0->done; sg_string text; int code = sg_read_rest(a0, &text); if (code) {} \
                             int64_t bad = sg_utf8_bad(text.ptr, text.len); \
                             if (bad >= 0) {{ sg_drop_string(&text); {} }} return ({gives}){{ .tag = 0, .u.v0 = {{ text }} }};",
                            failed("sg_io_error(code)"),
                            failed("sg_io_bad_text(start + bad)")
                        ),
                    ),
                };
                self.io_glue(fname, &params, &body, &gives, span)?;
                // The receiver is not one of the arguments the signature lists.
                let listed = params[1..].to_vec();
                self.sigs.insert(fname.to_string(), listed);
                self.call(fname.to_string(), Some(receiver), args, head, &expr.ty)
            }
        }
    }

    /// `sg_read_lines_next`: the next line of `reader.lines()` as a text of its own, or the
    /// error that ends it. `item`: the `Result<string, IoError>` it gives.
    fn read_lines_glue(&mut self, span: Span) -> Res<()> {
        if !self.made.insert("sg_read_lines_next".to_string()) {
            return Ok(());
        }
        self.io_error(span)?;
        let item = self.ctype(&TextIter::ReadLines.item(), span)?;
        let _ = writeln!(
            self.glue,
            "static bool sg_read_lines_next(sg_read_lines* it, {item}* out) {{ if (it->done) return false; \
             sg_string line = SG_LIT(\"\", 0); bool more = false; int64_t bad = 0; int code = sg_read_line(it->r, &line, &more, &bad); \
             if (code == 0 && more) {{ *out = ({item}){{ .tag = 0, .u.v0 = {{ line }} }}; return true; }} \
             sg_drop_string(&line); it->done = true; if (code == 0) return false; \
             *out = ({item}){{ .tag = 1, .u.v1 = {{ code == -1 ? sg_io_bad_text(bad) : sg_io_error(code) }} }}; return true; }}"
        );
        Ok(())
    }

    /// `exit(code)`: what was printed goes out and the program ends. In a test it fails the
    /// test instead, as the interpreter does (`sg_exit` looks at `sg_in_tests`).
    fn exit(&mut self, expr: &'a Expr) -> Res<String> {
        let ExprKind::Call { args, .. } = &expr.kind else { unreachable!("`exit` is a call") };
        let code = self.value_as(&args[0], &Ty::Int(IntTy::Byte))?;
        Ok(format!("sg_exit({code}, {})", self.loc(expr.span)))
    }

    /// `printError(x)`: as `print`, to the standard error; what was printed before goes out first.
    fn print_error(&mut self, arg: &'a Expr, pad: &str, out: &mut String) -> Res<()> {
        self.head = !matches!(arg.kind, ExprKind::Interp(_));
        let formatted = self.format(arg);
        self.head = false;
        let (mut format, values) = formatted?;
        format.push('\n');
        self.flush_pre(pad, out);
        let _ = writeln!(out, "{pad}fflush(stdout); fprintf(stderr, {}{values});", c_string(&format));
        self.flush_post(pad, out);
        Ok(())
    }

    fn print(&mut self, arg: &'a Expr, pad: &str, out: &mut String) -> Res<()> {
        // One value is the whole line: it can be `try ...`.
        self.head = !matches!(arg.kind, ExprKind::Interp(_));
        let formatted = self.format(arg);
        self.head = false;
        let (mut format, values) = formatted?;
        format.push('\n');
        self.flush_pre(pad, out);
        let _ = writeln!(out, "{pad}printf({}{values});", c_string(&format));
        self.flush_post(pad, out);
        Ok(())
    }

    /// `assert(cond)`: on failure the text of the condition and, for a comparison, both sides.
    fn assert(&mut self, call: &'a Expr, cond: &'a Expr, pad: &str, out: &mut String) -> Res<()> {
        let text = c_string(self.text(cond.span));
        let loc = self.loc(call.span);
        if let ExprKind::Binary(op, left, right) = &cond.kind {
            // Two structs, enums or lists: on failure each is written as `print` would write it.
            let ty = left.ty.strip_borrow().clone();
            if op.is_comparison() && matches!(ty, Ty::Struct(_) | Ty::Enum(_) | Ty::List(_) | Ty::Map(..) | Ty::Set(_) | Ty::Result(..) | Ty::Array(..) | Ty::Slice(_)) {
                let (l, r, ctype) = (self.temp(), self.temp(), self.ctype(&ty, left.span)?);
                let (left, right) = (self.pointer(left)?, self.pointer(right)?);
                let (test, shower) = (self.compared(*op, &ty, &l, &r, cond.span)?, self.shower(&ty, cond.span)?);
                self.flush_pre(pad, out);
                let _ = writeln!(
                    out,
                    "{pad}{{ {ctype} const* {l} = {left}; {ctype} const* {r} = {right}; if (!({test})) {{ sg_string {l}s = SG_LIT(\"\", 0), {r}s = SG_LIT(\"\", 0); \
                     {shower}(&{l}s, {l}); {shower}(&{r}s, {r}); sg_assert_fail_t({text}, sg_view(&{l}s), sg_view(&{r}s), {loc}); }} }}"
                );
                self.flush_post(pad, out);
                return Ok(());
            }
            let sides = match left.ty.strip_borrow() {
                Ty::Int(_) => Some(("sg_assert_fail_i", "__int128")),
                Ty::Float(FloatTy::Double) => Some(("sg_assert_fail_d", "double")),
                Ty::Float(FloatTy::Float) => Some(("sg_assert_fail_f", "double")),
                Ty::Bool => Some(("sg_assert_fail_b", "bool")),
                Ty::Char => Some(("sg_assert_fail_c", "uint32_t")),
                Ty::Str => Some(("sg_assert_fail_s", "")),
                _ => None,
            };
            if let (true, Some((fail, shown))) = (op.is_comparison(), sides) {
                let (l, r) = (self.temp(), self.temp());
                let line = if shown.is_empty() {
                    let (left, right) = (self.view(left)?, self.view(right)?);
                    let test = match op {
                        BinOp::Eq => format!("sg_eq({l}, {r})"),
                        BinOp::Ne => format!("!sg_eq({l}, {r})"),
                        _ => format!("sg_cmp({l}, {r}) {} 0", op.symbol()),
                    };
                    format!("{{ sg_str {l} = {left}; sg_str {r} = {right}; if (!({test})) {fail}({text}, {l}, {r}, {loc}); }}")
                } else {
                    let ty = self.ctype(left.ty.strip_borrow(), left.span)?;
                    // Each side is a line of its own and a number or a `bool`, which owns
                    // nothing: a `try` can leave from either (`assert(try load() == 42)`).
                    let (left, right) = (self.first(left, true)?, self.first(right, true)?);
                    format!("{{ {ty} {l} = {left}; {ty} {r} = {right}; if (!({l} {} {r})) {fail}({text}, ({shown}){l}, ({shown}){r}, {loc}); }}", op.symbol())
                };
                self.flush_pre(pad, out);
                let _ = writeln!(out, "{pad}{line}");
                self.flush_post(pad, out);
                return Ok(());
            }
        }
        let cond = self.first(cond, true)?;
        self.flush_pre(pad, out);
        let _ = writeln!(out, "{pad}if (!({cond})) sg_assert_fail({text}, {loc});");
        self.flush_post(pad, out);
        Ok(())
    }

    // ---- expressions ----

    /// Whether a variable declared as a value and a flag is used here as the value alone: after
    /// `if x is not null` the checker gives `x` the type without the `?`.
    fn narrowed(&self, name: &str, used_as: &Ty) -> bool {
        is_option(self.local(name).ty.strip_borrow()) && !matches!(used_as.strip_borrow(), Ty::Nullable(_))
    }

    /// A place as C writes it: something that can be assigned to and whose address can be taken.
    /// A variable that is a pointer stands for what it points at.
    fn lvalue(&mut self, expr: &'a Expr) -> Res<String> {
        match &expr.kind {
            ExprKind::Var(name) => {
                let cname = self.local(name).cname.clone();
                // A `read T?` variable is a pointer to the `T`. Known to hold a value, it
                // stands for that value; read as a `T?`, the pair is made for the statement.
                if let Some(option) = lent_option(&self.local(name).ty).cloned() {
                    return match self.narrowed(name, &expr.ty) {
                        true if self.lent_none(&option) == "NULL" => Ok(format!("(*{cname})")),
                        true => Ok(cname),
                        false => self.boxed(&cname, &option, expr.span),
                    };
                }
                let local = &self.local(name).ty;
                let place = if expr.ty.is_borrow() && !is_text_view(local) && !is_window(local) { format!("(*{cname})") } else { cname };
                Ok(if self.narrowed(name, &expr.ty) { format!("{place}.value") } else { place })
            }
            // A field is reached through any number of `T*` on its own.
            ExprKind::Field { recv, name, .. } => {
                // A field of a value that has no name (`longer(a, b).text`): the value is kept
                // for the statement.
                let mut place = self.look(recv)?;
                let mut ty = recv.ty.strip_borrow();
                while let Ty::Ptr(inner) = ty {
                    place = format!("(*{place})");
                    ty = inner;
                }
                // A field that borrows stands for what it points at, as a borrowed variable does.
                let field = format!("{place}.m_{name}");
                if let Some(option) = lent_option(&expr.ty) {
                    return self.boxed(&field, option, expr.span);
                }
                Ok(if expr.ty.is_borrow() && !is_text_view(&expr.ty) && !is_window(&expr.ty) { format!("(*{field})") } else { field })
            }
            // An item of a list: the index is checked, and the item is used where it is.
            ExprKind::Index { recv, index, kind: IndexKind::List } => {
                let Some((list, _)) = list_behind(&recv.ty) else { return Err(not_yet(expr.span, "this place")) };
                let Ty::List(item) = list else { unreachable!("it is a list") };
                let (name, held) = (mangle(list).ok_or_else(|| not_yet(expr.span, &format!("the type `{list}`")))?, self.ctype(item, expr.span)?);
                self.ctype(list, expr.span)?;
                // A place may be written more than once in a statement (`xs[i] += 1`): what
                // finds it is computed once if computing it does more than read.
                let once = self.effect(index).max(self.effect(recv)) >= Effect::Acts;
                let address = self.list_address(recv)?;
                let found = match self.index_sum(index)? {
                    Some((at, sum)) => format!("sg_ats_{name}({address}, {at}, &{sum}, {})", self.loc(expr.span)),
                    None => {
                        let at = self.value(index)?;
                        format!("sg_at_{name}({address}, {at}, {})", self.loc(expr.span))
                    }
                };
                if !once {
                    return Ok(format!("(*{found})"));
                }
                let temp = self.temp();
                self.pre.push(format!("{held}* {temp} = {found};"));
                Ok(format!("(*{temp})"))
            }
            // An item of an array or of a window, where it is: the index is checked.
            ExprKind::Index { recv, index, kind: IndexKind::Array } => {
                let Some(items) = items_behind(&recv.ty) else { return Err(not_yet(expr.span, "this place")) };
                let (Ty::Array(item, _) | Ty::Slice(item)) = items else { unreachable!("it is an array or a slice") };
                let (name, held) = (mangle(items).ok_or_else(|| not_yet(expr.span, &format!("the type `{items}`")))?, self.ctype(item, expr.span)?);
                self.ctype(items, expr.span)?;
                let once = self.effect(index).max(self.effect(recv)) >= Effect::Acts;
                // A window is given as it is, an array through a pointer to it.
                let before = self.mark();
                let (from, from_ty) = match items {
                    Ty::Slice(_) => (self.window(recv)?, Ty::Read(Box::new(items.clone()))),
                    _ => (self.list_address(recv)?, Ty::Ref(Box::new(items.clone()))),
                };
                let mut parts = vec![self.part(recv, &from_ty, from, before)];
                let before = self.mark();
                let (at, sum) = self.index_or_sum(index)?;
                parts.push(self.part(index, &index.ty, at, before));
                let (values, names) = self.in_order(parts)?;
                let loc = self.loc(expr.span);
                let found = match sum {
                    Some(sum) => sequence(&names, format!("sg_ats_{name}({}, {}, &{sum}, {loc})", values[0], values[1])),
                    None => sequence(&names, format!("sg_at_{name}({}, {}, {loc})", values[0], values[1])),
                };
                if !once {
                    return Ok(format!("(*{found})"));
                }
                let temp = self.temp();
                self.pre.push(format!("{held}* {temp} = {found};"));
                Ok(format!("(*{temp})"))
            }
            // What a map keeps under a key: the key has to be there.
            ExprKind::Index { recv, index, kind: IndexKind::Map } => {
                let Some((table, _)) = table_behind(&recv.ty) else { return Err(not_yet(expr.span, "this place")) };
                let table = table.clone();
                let Ty::Map(_, value) = &table else { return Err(not_yet(expr.span, "this place")) };
                let (name, held) = (mangle(&table).ok_or_else(|| not_yet(expr.span, &format!("the type `{table}`")))?, self.ctype(value, expr.span)?);
                self.ctype(&table, expr.span)?;
                let once = self.effect(index).max(self.effect(recv)) >= Effect::Acts;
                let (address, key) = (self.list_address(recv)?, self.key(&table, index)?);
                let loc = self.loc(expr.span);
                let found = format!("sg_at_{name}({address}, {key}, {loc})");
                if !once {
                    return Ok(format!("(*{found})"));
                }
                let temp = self.temp();
                self.pre.push(format!("{held}* {temp} = {found};"));
                Ok(format!("(*{temp})"))
            }
            _ => Err(not_yet(expr.span, "this place")),
        }
    }

    /// A place that is written to as a whole (`x = v`, `x.take()`, `x.replace(v)`) and the type
    /// it was declared with: a variable known to hold a value is still a `T?` slot.
    fn slot(&mut self, expr: &'a Expr) -> Res<(String, Ty)> {
        match &expr.kind {
            ExprKind::Var(name) => {
                // Whatever is written here, the variable holds a value afterwards.
                self.local_mut(name).gone = false;
                let local = self.local(name);
                Ok(match local.ty.is_borrow() {
                    true => (format!("(*{})", local.cname), local.ty.strip_borrow().clone()),
                    false => (local.cname.clone(), local.ty.clone()),
                })
            }
            _ => Ok((self.lvalue(expr)?, expr.ty.strip_borrow().clone())),
        }
    }

    /// A value that is only looked at, as something C can name more than once. A value that
    /// has no name and owns something is kept in a temporary and dropped after the statement.
    fn look(&mut self, expr: &'a Expr) -> Res<String> {
        if matches!(expr.kind, ExprKind::Var(_) | ExprKind::Field { .. } | ExprKind::Index { kind: IndexKind::List | IndexKind::Map | IndexKind::Array, .. }) {
            return self.lvalue(expr);
        }
        let late = self.late.take();
        let before = (self.pre.len(), self.post.len());
        let value = self.value(expr)?;
        if expr.ty.is_borrow() && !is_window(&expr.ty) {
            return Ok(format!("(*{value})"));
        }
        let (ctype, temp) = (self.ctype(&expr.ty, expr.span)?, self.temp());
        // Made only where the test runs, and kept until the block around the statement ends,
        // since the names point into it: a value that owns nothing, made with no line before.
        if let Some((mut decls, mut sets)) = late {
            if before == (self.pre.len(), self.post.len()) && !self.droppable(&expr.ty) {
                decls.push(format!("{ctype} {temp};"));
                sets.push(format!("{temp} = {value}"));
                self.late = Some((decls, sets));
                return Ok(temp);
            }
            self.late = Some((decls, sets));
        }
        self.pre.push(format!("{ctype} {temp} = {value};"));
        if self.droppable(&expr.ty) {
            self.post.push(self.drop_of(&expr.ty, &temp));
        }
        Ok(temp)
    }

    /// What `f.write(x)` and `text.write(x)` become: each piece of `x` added at the end of the
    /// text that `out` points to.
    fn write_into(&mut self, out: &str, arg: &'a Expr) -> Res<String> {
        let mut lines = Vec::new();
        for piece in self.pieces(arg, true)? {
            lines.push(match piece {
                Piece::Text(format, values) if values.is_empty() => {
                    let text = format.replace("%%", "%");
                    format!("sg_put({out}, {}, {})", c_string(&text), text.len())
                }
                Piece::Text(format, values) => format!("sg_putf({out}, {}{values})", c_string(&format)),
                Piece::Show(shower, at, 0) => format!("{shower}({out}, {at})"),
                Piece::Show(shower, at, width) => {
                    let from = self.temp();
                    format!("int64_t {from} = ({out})->len; {shower}({out}, {at}); sg_pad_from({out}, {from}, {width})")
                }
            });
        }
        match lines.len() {
            1 => Ok(lines.concat()),
            _ => Ok(format!("({{ {}; }})", lines.join("; "))),
        }
    }

    /// The address of a place, for `read x`, `ref x` and the receiver of a method.
    fn address(&mut self, place: &'a Expr) -> Res<String> {
        if let ExprKind::Var(name) = &place.kind {
            self.local_mut(name).gone = false;
        }
        match &place.kind {
            ExprKind::Var(name) if place.ty.is_borrow() && !is_text_view(&place.ty) && !self.narrowed(name, &place.ty) => Ok(self.local(name).cname.clone()),
            _ => Ok(format!("(&{})", self.lvalue(place)?)),
        }
    }

    /// Text that is only looked at, as a pointer and a length. An owned text that has no
    /// name (the result of a call) is kept in a temporary and dropped after the statement.
    fn view(&mut self, expr: &'a Expr) -> Res<String> {
        match (&expr.kind, &expr.ty) {
            (ExprKind::Str(text), _) => Ok(format!("SG_STR({}, {})", c_string(text), text.len())),
            (ExprKind::Borrow { expr: inner, .. }, _) if is_text(&inner.ty) => self.view(inner),
            (_, ty) if is_text_view(ty) => self.value(expr),
            (_, Ty::Ref(_)) => Ok(format!("sg_view({})", self.value(expr)?)),
            (ExprKind::Var(_) | ExprKind::Field { .. } | ExprKind::Index { kind: IndexKind::List | IndexKind::Map | IndexKind::Array, .. }, Ty::Str) => {
                Ok(format!("sg_view(&{})", self.lvalue(expr)?))
            }
            (_, Ty::Str) => {
                let (value, temp) = (self.value(expr)?, self.temp());
                self.pre.push(format!("sg_string {temp} = {value};"));
                self.post.push(format!("sg_drop_string(&{temp});"));
                Ok(format!("sg_view(&{temp})"))
            }
            // Text on the heap is looked at where it is: `name.length()` with `string* name`.
            (_, ty) if is_text_behind_pointer(ty) => {
                let mut place = self.look(expr)?;
                let mut ty = ty.strip_borrow();
                while let Ty::Ptr(inner) = ty {
                    place = format!("(*{place})");
                    ty = inner;
                }
                Ok(format!("sg_view(&{place})"))
            }
            (_, other) => Err(not_yet(expr.span, &format!("a `{other}` used as text"))),
        }
    }

    /// An operation on two integers of the same type. In a debug build `+`, `-` and `*` panic
    /// on overflow; in a release build they wrap, written with unsigned arithmetic so that it
    /// is defined in C too. Division checks for zero in every build. A shift count follows
    /// the overflow rule: checked in a debug build, cut to the bits of the type in a release
    /// build (what the shift of the machine does by itself). `&`, `|` and `^` cannot fail.
    fn arithmetic(&mut self, op: BinOp, ty: IntTy, left: &str, right: &str, span: Span) -> String {
        if matches!(op, BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor) {
            return format!("(({})({left} {} {right}))", int_ctype(ty), op.symbol());
        }
        let (name, wrapping) = match op {
            BinOp::Shl => ("shl", Some("wshl")),
            BinOp::Shr => ("shr", Some("wshr")),
            BinOp::Add => ("add", Some("wadd")),
            BinOp::Sub => ("sub", Some("wsub")),
            BinOp::Mul => ("mul", Some("wmul")),
            BinOp::Div => ("div", None),
            _ => ("rem", None),
        };
        match (self.checked, wrapping) {
            (false, Some(wrapping)) => format!("sg_{wrapping}_{}({left}, {right})", int_name(ty)),
            _ => {
                let loc = self.loc(span);
                format!("sg_{name}_{}({left}, {right}, {loc})", int_name(ty))
            }
        }
    }

    /// Two values that are computed one after the other, the left one first.
    fn two(&mut self, left: &'a Expr, right: &'a Expr, mut head: bool) -> Res<(Vec<String>, String)> {
        let mut parts = Vec::new();
        for side in [left, right] {
            let before = self.mark();
            let value = self.first(side, head)?;
            head = head && !self.droppable(&side.ty);
            parts.push(self.part(side, &side.ty, value, before));
        }
        self.in_order(parts)
    }

    /// The value of an expression as C writes it. A use that the checker marked as a move
    /// takes the value out of its place and leaves the place empty.
    fn value(&mut self, expr: &'a Expr) -> Res<String> {
        let head = std::mem::replace(&mut self.head, false);
        match &expr.kind {
            ExprKind::Int(value, ty) => Ok(int_literal(*value, *ty)),
            ExprKind::Char(value) => Ok(format!("((uint32_t){}u)", *value as u32)),
            ExprKind::Float(text, ty) => Ok(float_literal(text, *ty)),
            ExprKind::Bool(value) => Ok(value.to_string()),
            // A constant is a `string` with no capacity: nothing is allocated and nothing freed.
            ExprKind::Str(text) => Ok(format!("SG_LIT({}, {})", c_string(text), text.len())),
            ExprKind::Var(_) | ExprKind::Field { .. } if expr.moved && self.droppable(&expr.ty) => {
                let place = self.lvalue(expr)?;
                // A whole variable moved on a line that always runs: its drop is not written.
                if let (ExprKind::Var(name), 0) = (&expr.kind, self.maybe) {
                    let local = self.local_mut(name);
                    local.gone = !local.ty.is_borrow();
                }
                Ok(self.move_of(&expr.ty, &place))
            }
            // A variable that holds a value and a flag, used as the value alone.
            ExprKind::Var(name) if self.narrowed(name, &expr.ty) => {
                let place = self.lvalue(expr)?;
                Ok(match &expr.ty {
                    // A `read string?` that holds a text is the window itself.
                    ty if is_text_view(ty) && lent_option(&self.local(name).ty).is_some() => place,
                    ty if is_text_view(ty) => format!("sg_view(&{place})"),
                    ty if ty.is_borrow() => format!("(&{place})"),
                    _ => place,
                })
            }
            ExprKind::Var(name) => Ok(self.local(name).cname.clone()),
            // The value of a field that borrows is the pointer it holds.
            ExprKind::Field { recv, name, .. } if lent_option(&expr.ty).is_some() => {
                let mut place = self.look(recv)?;
                let mut ty = recv.ty.strip_borrow();
                while let Ty::Ptr(inner) = ty {
                    place = format!("(*{place})");
                    ty = inner;
                }
                Ok(format!("{place}.m_{name}"))
            }
            ExprKind::Field { .. } if expr.ty.is_borrow() && !is_text_view(&expr.ty) && !is_window(&expr.ty) => Ok(format!("(&{})", self.lvalue(expr)?)),
            ExprKind::Field { .. } => self.lvalue(expr),
            // The name of a function, as a value: its address.
            ExprKind::FuncRef(name) => Ok(format!("f_{}", cname(name))),
            ExprKind::Constant { ty, name } => Ok(constant(ty, name)),
            // A number read out of a pointer that has no name: the pointer is freed after the statement.
            ExprKind::Load(inner) if matches!(inner.ty, Ty::Ptr(_)) => Ok(format!("(*{})", self.look(inner)?)),
            // A `T?` copied out of a `read T?`.
            ExprKind::Load(inner) if lent_option(&inner.ty).is_some() => {
                let value = self.first(inner, head)?;
                self.unlent(&value, lent_option(&inner.ty).expect("just tested"), expr.span)
            }
            ExprKind::Load(inner) => Ok(format!("(*{})", self.first(inner, head)?)),
            // A borrow of a `T*` used as a borrow of the `T`: one step along the pointer.
            ExprKind::Unbox(inner) => Ok(format!("(*{})", self.value(inner)?)),
            ExprKind::NewShared(_) | ExprKind::Share(_) | ExprKind::WeakOf(_) => Err(not_yet(expr.span, "a `shared` or a `weak` value")),
            ExprKind::New(inner) => {
                let Ty::Ptr(target) = &expr.ty else { return Err(not_yet(expr.span, "this `new`")) };
                self.ctype(&expr.ty, expr.span)?;
                let value = self.first_as(inner, target, head)?;
                Ok(format!("sg_new_{}({value})", mangle(&expr.ty).expect("it has a C type")))
            }
            ExprKind::IsNull { expr: inner, negated } => {
                // A `read T?` is asked whether it points at something; a copy of it is not made.
                let inner = match &inner.kind {
                    ExprKind::Load(lent) if lent_option(&lent.ty).is_some() => lent,
                    _ => inner,
                };
                if let Some(option) = lent_option(&inner.ty) {
                    let value = self.value(inner)?;
                    let holds = match option {
                        Ty::Nullable(held) if **held == Ty::Str => format!("({value}.len >= 0)"),
                        _ => format!("({value} != NULL)"),
                    };
                    return Ok(if *negated { holds } else { format!("(!{holds})") });
                }
                let ty = inner.ty.strip_borrow();
                if !matches!(ty, Ty::Nullable(_)) {
                    return Err(not_yet(expr.span, &format!("`is null` on `{ty}`")));
                }
                let subject = self.look(inner)?;
                let holds = if is_option(ty) { format!("{subject}.has") } else { format!("({subject} != NULL)") };
                Ok(if *negated { holds } else { format!("(!{holds})") })
            }
            // `x ?? return y`: with no value the function ends here.
            ExprKind::Coalesce { left, right, on_result } if matches!(right.kind, ExprKind::Return(_)) => {
                let ExprKind::Return(returned) = &right.kind else { unreachable!("matched above") };
                if !head {
                    return Err(not_yet(expr.span, "`?? return` in this part of a statement")
                        .hint("give the value a name first: `var x = ... ?? return ...`"));
                }
                let value = self.first(left, true)?;
                let temp = self.temp();
                // The `return` has its own temporaries; the ones made so far are dropped on its way out.
                let (pre, post) = (std::mem::take(&mut self.pre), std::mem::take(&mut self.post));
                // The error of a `Result` is dropped: the `return` gives one of its own.
                let mut pending = Vec::new();
                if let (true, Ty::Result(_, error)) = (*on_result, &left.ty) {
                    if self.droppable(error) {
                        pending.push(self.drop_of(error, &format!("{temp}.u.v1.m_cause")));
                    }
                }
                pending.extend(post.iter().cloned());
                // What the `return` moves is moved only on that way out.
                let before = self.gone();
                let exit = self.return_lines(returned.as_deref(), &pending, right.span);
                self.set_gone(&before);
                (self.pre, self.post) = (pre, post);
                self.open(&left.ty, &temp, value, exit?, left.span)
            }
            // The right side runs only when there is no value. On a `Result` the error is dropped first.
            ExprKind::Coalesce { left, right, on_result } => {
                let error = match (&left.ty, on_result) {
                    (Ty::Result(_, error), true) => Some(&**error),
                    (_, false) if self.held_in(&left.ty, "").is_some() => None,
                    _ => return Err(not_yet(left.span, &format!("`??` on `{}`", left.ty))),
                };
                let value = self.first(left, head)?;
                let before = (self.pre.len(), self.post.len());
                self.maybe += 1;
                let fallback = self.value(right);
                self.maybe -= 1;
                let fallback = fallback?;
                let fallback = match (before == (self.pre.len(), self.post.len()), &right.ty) {
                    (true, _) => fallback,
                    (false, Ty::Int(_) | Ty::Float(_) | Ty::Bool) => self.enclosed(before, fallback, &right.ty, right.span)?,
                    // Any other value could point at what was made for it.
                    (false, _) => return Err(not_yet(right.span, "a value that needs a temporary on the right of `??`").hint("give it a name first")),
                };
                let (ctype, temp) = (self.ctype(&left.ty, left.span)?, self.temp());
                let (holds, held) = self.held_in(&left.ty, &temp).expect("tested above");
                // A right side that never gives a value (`panic`) still has to have the type in C.
                let fallback = if right.ty == Ty::Never { format!("({fallback}, {held})") } else { fallback };
                let fallback = match error.filter(|error| self.droppable(error)) {
                    Some(error) => format!("({{ {} {fallback}; }})", self.drop_of(error, &format!("{temp}.u.v1.m_cause"))),
                    None => fallback,
                };
                Ok(format!("({{ {ctype} {temp} = {value}; {holds} ? {held} : {fallback}; }})"))
            }
            // `try e`: with no value the function ends here, and gives its caller `null` or the error.
            ExprKind::Try { expr: inner, kind } => {
                if !head {
                    return Err(not_yet(expr.span, "`try` in this part of a statement")
                        .hint("give the value a name first: `var x = try ...`"));
                }
                let value = self.first(inner, true)?;
                let temp = self.temp();
                let ret = self.ret.clone();
                let returned = match kind {
                    TryKind::Null => self.lift(String::new(), &Ty::Null, &ret, expr.span)?,
                    // In a test the error is written and the test ends: there is no caller.
                    TryKind::Result(_) if self.in_test => {
                        let Ty::Result(_, error) = &inner.ty else { return Err(not_yet(expr.span, "`try` on this in a test")) };
                        let cause = format!("{temp}.u.v1.m_cause");
                        let mut exit = vec![match &**error {
                            Ty::Str => format!("sg_test_failed(sg_view(&{cause}));"),
                            other => {
                                let shower = self.shower(other, expr.span)?;
                                format!("{{ sg_string sg_text = SG_LIT(\"\", 0); {shower}(&sg_text, &{cause}); sg_test_failed(sg_view(&sg_text)); sg_drop_string(&sg_text); }}")
                            }
                        }];
                        if self.droppable(error) {
                            exit.push(self.drop_of(error, &cause));
                        }
                        exit.extend(self.post.clone());
                        exit.extend(self.drops(0));
                        exit.push("return;".to_string());
                        return self.open(&inner.ty, &temp, value, exit, inner.span);
                    }
                    TryKind::Result(convert) => {
                        if !matches!(ret, Ty::Result(..)) {
                            return Err(not_yet(expr.span, "`try` on a `Result` here"));
                        }
                        // The error goes to the caller as it is, or inside the variant of
                        // the caller's error type that carries it.
                        let cause = format!("{temp}.u.v1.m_cause");
                        let cause = match convert {
                            Some((owner, index)) => format!("((E_{}){{ .tag = {index}, .u.v{index} = {{ {cause} }} }})", cname(owner)),
                            None => cause,
                        };
                        format!("(({}){{ .tag = 1, .u.v1 = {{ {cause} }} }})", self.ctype(&ret, expr.span)?)
                    }
                };
                // What is returned is made of the opened value alone, so it can be written last.
                let mut exit = self.post.clone();
                exit.extend(self.drops(0));
                exit.push(format!("return {returned};"));
                self.open(&inner.ty, &temp, value, exit, inner.span)
            }
            ExprKind::Widen(ty, inner) => {
                let ctype = self.ctype(ty, expr.span)?;
                Ok(format!("(({ctype}){})", self.first(inner, head)?))
            }
            // `read limit` with `int? limit`: the flag is tested here, and what is lent is a
            // pointer to the number or no pointer.
            ExprKind::Borrow { expr: inner, .. } if lent_option(&expr.ty).is_some() => {
                let option = lent_option(&expr.ty).expect("just tested");
                if lent_option(&inner.ty).is_some() {
                    return self.value(inner);
                }
                let address = match inner.is_place() {
                    true => {
                        if let ExprKind::Var(name) = &inner.kind {
                            self.local_mut(name).gone = false;
                        }
                        format!("(&{})", self.lvalue(inner)?)
                    }
                    false => format!("(&{})", self.look(inner)?),
                };
                self.lend(&address, option, expr.span)
            }
            ExprKind::Borrow { expr: inner, .. } if is_text_view(&expr.ty) => self.view(inner),
            // A window that is lent again is the same window.
            ExprKind::Borrow { expr: inner, .. } if is_window(&expr.ty) => self.window(inner),
            ExprKind::Borrow { expr: inner, .. } if inner.is_place() => self.address(inner),
            // A value that has no name, lent: it is kept until the statement ends.
            ExprKind::Borrow { expr: inner, .. } if !inner.ty.is_borrow() => Ok(format!("(&{})", self.look(inner)?)),
            // A value of an enum, and `Ok(...)` and `Error(...)` of a `Result`.
            ExprKind::Variant { index, args, .. } => {
                let Some(variants) = self.variants(&expr.ty, expr.span)? else { return Err(not_yet(expr.span, &format!("a value of `{}`", expr.ty))) };
                let ctype = self.ctype(&expr.ty, expr.span)?;
                let mut parts = Vec::new();
                let mut taking = Taking { head, handed: Vec::new() };
                for (position, (arg, (_, want, _))) in args.iter().zip(&variants[*index]).enumerate() {
                    let before = self.mark();
                    let value = self.first_as(arg, want, taking.head)?;
                    parts.push(self.taken(arg, value, want, before, args[position + 1..].iter().any(may_leave), &mut taking)?);
                }
                let (values, names) = self.in_order(parts)?;
                let made = match values.is_empty() {
                    true => format!("(({ctype}){{ .tag = {index} }})"),
                    false => format!("(({ctype}){{ .tag = {index}, .u.v{index} = {{ {} }} }})", values.join(", ")),
                };
                let made = self.handed_over(made, &expr.ty, &taking.handed, expr.span)?;
                Ok(sequence(&names, made))
            }
            ExprKind::IsVariant { .. } => Ok(self.variant_test(expr)?.0),
            // A `match` inside a larger expression: it is written before the statement, as the
            // statements it is, and gives its value through a name. The parts of the statement
            // that come before it are computed before it too (see `in_order`).
            ExprKind::Match { .. } if !matches!(expr.ty, Ty::Void | Ty::Never) && self.ctype(&expr.ty, expr.span).is_ok() => {
                let (ctype, temp) = (self.ctype(&expr.ty, expr.span)?, self.temp());
                let (pre, post) = (std::mem::take(&mut self.pre), std::mem::take(&mut self.post));
                // What the statement keeps so far is freed on every way out of an arm.
                let kept = !post.is_empty();
                if kept {
                    self.scopes.push(vec![Local { name: String::new(), cname: String::new(), ty: Ty::Void, gone: false, drop: Some(post.join(" ")) }]);
                }
                let (ways_out, gone) = (self.ways_out, self.gone());
                let mut text = format!("{ctype} {temp};\n");
                let written = self.match_stmt(expr, &Dest::Place(temp.clone(), expr.ty.clone()), 1, &mut text);
                if kept {
                    self.scopes.pop();
                }
                (self.pre, self.post) = (pre, post);
                written?;
                // A value that owns something and has no name yet would be left behind.
                if self.ways_out > ways_out && !head {
                    return Err(not_yet(expr.span, "a `match` that leaves the function, in this part of a statement")
                        .hint("give the value a name first: `var x = match ...`"));
                }
                // On a side that runs only sometimes, what the arms moved is not certain.
                if self.maybe > 0 {
                    self.gone_only_if(&gone);
                }
                self.pre.push(text.trim_end().to_string());
                Ok(temp)
            }
            ExprKind::Match { .. } => Err(not_yet(expr.span, "a `match` inside a larger expression")
                .hint("give the value a name first: `var x = match ...`")),
            ExprKind::Unary(UnOp::Not, operand) => Ok(format!("(!{})", self.first(operand, head)?)),
            // In C a `byte` becomes an `int` before `~`: the result is cut back to its type.
            ExprKind::Unary(UnOp::BitNot, operand) => {
                let Ty::Int(ty) = &expr.ty else { return Err(not_yet(expr.span, "this operator")) };
                Ok(format!("(({})~({}))", int_ctype(*ty), self.first(operand, head)?))
            }
            ExprKind::Unary(UnOp::Neg, operand) if matches!(expr.ty, Ty::Float(_)) => Ok(format!("(-{})", self.first(operand, head)?)),
            ExprKind::Unary(UnOp::Neg, operand) => {
                let Ty::Int(ty) = &expr.ty else { return Err(not_yet(expr.span, "this operator")) };
                let operand = self.first(operand, head)?;
                if !self.checked {
                    return Ok(format!("sg_wsub_{0}(({1})0, {operand})", int_name(*ty), int_ctype(*ty)));
                }
                let loc = self.loc(expr.span);
                Ok(format!("sg_neg_{}({operand}, {loc})", int_name(*ty)))
            }
            // The names of an `is` in the chain last until it ends.
            ExprKind::Binary(BinOp::And | BinOp::Or, ..) if names_in_chain(expr) => {
                self.scopes.push(Vec::new());
                let test = self.chain(expr, head);
                self.scopes.pop();
                let (test, decls) = test?;
                self.pre.extend(decls);
                Ok(test)
            }
            ExprKind::Binary(op @ (BinOp::And | BinOp::Or), left, right) => {
                let left = self.first(left, head)?;
                // The right side runs only sometimes, so it cannot need a line before the statement.
                let before = (self.pre.len(), self.post.len());
                self.maybe += 1;
                let right_value = self.value(right);
                self.maybe -= 1;
                let right_value = self.enclosed(before, right_value?, &Ty::Bool, right.span)?;
                Ok(format!("({left} {} {right_value})", op.symbol()))
            }
            ExprKind::Binary(op, left, right) if is_text(&left.ty) => {
                let view = Ty::Read(Box::new(Ty::Str));
                let mut parts = Vec::new();
                for side in [left, right] {
                    let before = self.mark();
                    let value = self.view(side)?;
                    parts.push(self.part(side, &view, value, before));
                }
                let (values, names) = self.in_order(parts)?;
                let (left, right) = (&values[0], &values[1]);
                Ok(sequence(&names, match op {
                    BinOp::Eq => format!("sg_eq({left}, {right})"),
                    BinOp::Ne => format!("(!sg_eq({left}, {right}))"),
                    _ => format!("(sg_cmp({left}, {right}) {} 0)", op.symbol()),
                }))
            }
            // Structs, enums and lists are compared where they are, through pointers.
            ExprKind::Binary(op, left, right)
                if matches!(left.ty.strip_borrow(), Ty::Struct(_) | Ty::Enum(_) | Ty::List(_) | Ty::Map(..) | Ty::Set(_) | Ty::Result(..) | Ty::Array(..) | Ty::Slice(_)) =>
            {
                let ty = left.ty.strip_borrow().clone();
                let lent = Ty::Read(Box::new(ty.clone()));
                let mut parts = Vec::new();
                for side in [left, right] {
                    let before = self.mark();
                    let value = self.pointer(side)?;
                    let mut part = self.part(side, &lent, value, before);
                    // A window is compared through a pointer to where it is kept.
                    if matches!(ty, Ty::Slice(_)) {
                        part.ctype = format!("{}*", self.ctype(&ty, side.span)?);
                    }
                    parts.push(part);
                }
                let (values, names) = self.in_order(parts)?;
                Ok(sequence(&names, self.compared(*op, &ty, &values[0], &values[1], expr.span)?))
            }
            ExprKind::Binary(op, left, right) => {
                let operands = left.ty.strip_borrow().clone();
                let (values, names) = self.two(left, right, head)?;
                let (left, right) = (&values[0], &values[1]);
                match (&operands, op.is_comparison()) {
                    (Ty::Int(_) | Ty::Float(_) | Ty::Bool | Ty::Char, true) => Ok(sequence(&names, format!("({left} {} {right})", op.symbol()))),
                    (Ty::Int(ty), false) => Ok(sequence(&names, self.arithmetic(*op, *ty, left, right, expr.span))),
                    (Ty::Float(ty), false) => Ok(sequence(&names, fractional(*op, *ty, left, right))),
                    (other, _) => Err(not_yet(expr.span, &format!("`{}` on `{other}`", op.symbol()))),
                }
            }
            // A narrowing conversion follows the overflow rule: checked in a debug build, cut
            // to size in a release build.
            // From a fractional number to an integer: the fraction is dropped, and a value
            // that does not fit (`NaN` too) panics in a debug build; a release build gives the
            // nearest value the type holds, and 0 for `NaN`, as Rust does.
            ExprKind::Cast { ty, expr: inner } => {
                let from = inner.ty.strip_borrow().clone();
                let inner = self.first(inner, head)?;
                match (ty, from) {
                    (Ty::Int(to), Ty::Float(from)) => {
                        if !self.checked {
                            return Ok(format!("sg_sat_{}((double)({inner}))", int_name(*to)));
                        }
                        let loc = self.loc(expr.span);
                        Ok(format!("sg_fcast_{}((double)({inner}), {}, {loc})", int_name(*to), from == FloatTy::Float))
                    }
                    (Ty::Char, Ty::Char) => Ok(inner),
                    (Ty::Char, _) => {
                        let loc = self.loc(expr.span);
                        Ok(format!("sg_to_char((__int128)({inner}), {loc})"))
                    }
                    // The code of a character fits every type of 32 bits or more.
                    (Ty::Int(to), Ty::Char) if !self.checked || to.bits() >= 32 => Ok(format!("(({})({inner}))", int_ctype(*to))),
                    (Ty::Int(to), Ty::Char) => {
                        let (loc, temp) = (self.loc(expr.span), self.temp());
                        Ok(format!(
                            "({{ uint32_t {temp} = {inner}; if (__builtin_expect({temp} > {}u, 0)) sg_code_fail({temp}, \"{}\", {loc}); ({}){temp}; }})",
                            to.max(),
                            to.name(),
                            int_ctype(*to)
                        ))
                    }
                    (Ty::Int(to), _) => {
                        if !self.checked {
                            return Ok(format!("(({})({inner}))", int_ctype(*to)));
                        }
                        let loc = self.loc(expr.span);
                        Ok(format!("sg_cast_{}((__int128)({inner}), {loc})", int_name(*to)))
                    }
                    (to, _) => Ok(format!("(({})({inner}))", self.ctype(to, expr.span)?)),
                }
            }
            ExprKind::Index { kind: IndexKind::List | IndexKind::Map | IndexKind::Array, .. } => self.lvalue(expr),
            // A borrow of a list or of an array, used as a window onto all of it.
            ExprKind::AsSlice(inner) => self.window(inner),
            // A new array. With braces, each item is written; with one value in parentheses,
            // every item gets it; with neither, every item is zero.
            ExprKind::Array { args, items, .. } => {
                let Ty::Array(item, length) = &expr.ty else { return Err(not_yet(expr.span, &format!("a `{}`", expr.ty))) };
                let ctype = self.ctype(&expr.ty, expr.span)?;
                match (items, args.as_slice()) {
                    (Some(items), _) => {
                        let mut parts = Vec::new();
                        let mut taking = Taking { head, handed: Vec::new() };
                        for (position, given) in items.iter().enumerate() {
                            let before = self.mark();
                            let value = self.first_as(given, item, taking.head)?;
                            parts.push(self.taken(given, value, item, before, items[position + 1..].iter().any(may_leave), &mut taking)?);
                        }
                        let (values, names) = self.in_order(parts)?;
                        let made = self.handed_over(format!("(({ctype}){{ {{ {} }} }})", values.join(", ")), &expr.ty, &taking.handed, expr.span)?;
                        Ok(sequence(&names, made))
                    }
                    (None, [value]) => {
                        let (held, temp) = (self.ctype(item, expr.span)?, self.temp());
                        let value = self.first_as(value, item, head)?;
                        Ok(format!("({{ {ctype} {temp}; {held} {temp}v = {value}; for (int64_t {temp}i = 0; {temp}i < {length}; {temp}i++) {temp}.items[{temp}i] = {temp}v; {temp}; }})"))
                    }
                    (None, _) => Ok(format!("(({ctype}){{ 0 }})")),
                }
            }
            // A new list: the items are added one after the other.
            // A new map or set: an entry is added for each item; a key written twice stops the program.
            ExprKind::Collection { items, .. } if matches!(expr.ty, Ty::Map(..) | Ty::Set(_)) => {
                let (ctype, temp) = (self.ctype(&expr.ty, expr.span)?, self.temp());
                let name = mangle(&expr.ty).expect("it has a C type");
                let wanted: Vec<Ty> = match &expr.ty {
                    Ty::Map(key, value) => vec![(**key).clone(), (**value).clone()],
                    Ty::Set(key) => vec![(**key).clone()],
                    _ => unreachable!("matched above"),
                };
                let mut parts = Vec::new();
                let mut spans = Vec::new();
                for given in items.iter().flatten() {
                    spans.push(given.span);
                    for (value, want) in given.values.iter().zip(&wanted) {
                        let before = self.mark();
                        let written = self.value_as(value, want)?;
                        parts.push(self.part(value, want, written, before));
                    }
                }
                let (values, names) = self.in_order(parts)?;
                let mut adds = String::new();
                for (entry, span) in values.chunks(wanted.len()).zip(spans) {
                    let _ = match wanted.len() {
                        2 => write!(adds, "sg_add_{name}(&{temp}, {}, {}, {}); ", entry[0], entry[1], self.loc(span)),
                        _ => write!(adds, "sg_add_{name}(&{temp}, {}); ", entry[0]),
                    };
                }
                Ok(format!("({{ {names}{ctype} {temp} = {{ 0 }}; {adds}{temp}; }})"))
            }
            ExprKind::Collection { items, .. } => {
                let Ty::List(item) = &expr.ty else { return Err(not_yet(expr.span, &format!("a `{}`", expr.ty))) };
                let (ctype, temp) = (self.ctype(&expr.ty, expr.span)?, self.temp());
                let name = mangle(&expr.ty).expect("it has a C type");
                let mut parts = Vec::new();
                for given in items.iter().flatten() {
                    let before = self.mark();
                    let value = self.value_as(&given.values[0], item)?;
                    parts.push(self.part(&given.values[0], item, value, before));
                }
                let (values, names) = self.in_order(parts)?;
                let adds: String = values.iter().map(|value| format!("sg_add_{name}(&{temp}, {value}); ")).collect();
                Ok(format!("({{ {names}{ctype} {temp} = {{ 0 }}; {adds}{temp}; }})"))
            }
            // A byte of text and a range of it: checked in every build.
            ExprKind::Index { recv, index, kind: IndexKind::Text } => {
                let before = self.mark();
                let text = self.view(recv)?;
                let mut parts = vec![self.part(recv, &Ty::Read(Box::new(Ty::Str)), text, before)];
                let before = self.mark();
                let (at, sum) = self.index_or_sum(index)?;
                parts.push(self.part(index, &index.ty, at, before));
                let (values, names) = self.in_order(parts)?;
                let loc = self.loc(expr.span);
                Ok(match sum {
                    Some(sum) => sequence(&names, format!("sg_ats({}, {}, &{sum}, {loc})", values[0], values[1])),
                    None => sequence(&names, format!("sg_at({}, {}, {loc})", values[0], values[1])),
                })
            }
            // A range of items: a window onto a part of them. The ends are checked in every build.
            ExprKind::Slice { recv, from, to } if !is_text_view(&expr.ty) => {
                let slice = flat(&expr.ty).strip_borrow().clone();
                let name = mangle(&slice).ok_or_else(|| not_yet(expr.span, &format!("the type `{slice}`")))?;
                let before = self.mark();
                let whole = self.window(recv)?;
                if from.is_none() && to.is_none() {
                    return Ok(whole);
                }
                let mut parts = vec![self.part(recv, &expr.ty, whole, before)];
                let mut sums = Vec::new();
                for (end, may_sum) in self.range_ends(from, to) {
                    let before = self.mark();
                    let (value, sum) = if may_sum { self.index_or_sum(end)? } else { (self.value(end)?, None) };
                    parts.push(self.part(end, &end.ty, value, before));
                    sums.push(sum);
                }
                let (values, names) = self.in_order(parts)?;
                let helper = match (from, to) {
                    (Some(_), Some(_)) => "cut",
                    (Some(_), None) => "after",
                    _ => "before",
                };
                let loc = self.loc(expr.span);
                if sums.iter().all(Option::is_none) {
                    return Ok(sequence(&names, format!("sg_{helper}_{name}({}, {loc})", values.join(", "))));
                }
                let sums: Vec<String> = sums.into_iter().map(|sum| sum.map_or("NULL".to_string(), |sum| format!("&{sum}"))).collect();
                Ok(sequence(&names, format!("sg_{helper}s_{name}({}, {}, {loc})", values.join(", "), sums.join(", "))))
            }
            ExprKind::Slice { recv, from, to } => {
                let text = self.view(recv)?;
                if from.is_none() && to.is_none() {
                    return Ok(text);
                }
                // The text is named once: its length is one of the ends.
                let temp = self.temp();
                self.pre.push(format!("sg_str {temp} = {text};"));
                let mut parts = Vec::new();
                let mut sums = Vec::new();
                for (end, may_sum) in self.range_ends(from, to) {
                    let before = self.mark();
                    let (value, sum) = if may_sum { self.index_or_sum(end)? } else { (self.value(end)?, None) };
                    parts.push(self.part(end, &end.ty, value, before));
                    sums.push(sum.map_or("NULL".to_string(), |sum| format!("&{sum}")));
                }
                let (values, names) = self.in_order(parts)?;
                let (from, to, sf, st) = match (from, to) {
                    (Some(_), Some(_)) => (values[0].clone(), values[1].clone(), sums[0].clone(), sums[1].clone()),
                    (Some(_), None) => (values[0].clone(), format!("{temp}.len"), sums[0].clone(), "NULL".to_string()),
                    (None, Some(_)) => ("0".to_string(), values[0].clone(), "NULL".to_string(), sums[0].clone()),
                    (None, None) => unreachable!("handled above"),
                };
                let loc = self.loc(expr.span);
                if sf == "NULL" && st == "NULL" {
                    return Ok(sequence(&names, format!("sg_slice({temp}, {from}, {to}, {loc})")));
                }
                Ok(sequence(&names, format!("sg_slices({temp}, {from}, {to}, {sf}, {st}, {loc})")))
            }
            ExprKind::StructLit { name, fields, .. } => {
                let mut parts = Vec::new();
                let decl: &'a StructDecl = self.structs[name.as_str()];
                let mut taking = Taking { head, handed: Vec::new() };
                for (position, field) in fields.iter().enumerate() {
                    let want = self.written(&decl.fields[field.index].ty)?;
                    let before = self.mark();
                    let value = self.first_as(&field.value, &want, taking.head)?;
                    let later = fields[position + 1..].iter().any(|field| may_leave(&field.value));
                    parts.push(self.taken(&field.value, value, &want, before, later, &mut taking)?);
                }
                let (values, names) = self.in_order(parts)?;
                let inits: Vec<String> = fields.iter().zip(values).map(|(field, value)| format!(".m_{} = {value}", field.name)).collect();
                let made = self.handed_over(format!("((S_{}){{ {} }})", cname(name), inits.join(", ")), &expr.ty, &taking.handed, expr.span)?;
                Ok(sequence(&names, made))
            }
            ExprKind::Call { name, args, callee, .. } => match callee {
                // `x ?? panic("...")`: the program stops, so there is no value to shape.
                Callee::Plain if name == "panic" => {
                    let before = (self.pre.len(), self.post.len());
                    let (format, values) = self.format(&args[0])?;
                    let loc = self.loc(expr.span);
                    // What the message needed is made here, where the program stops, and
                    // not before the statement; nothing is left to free.
                    let made: String = self.pre.drain(before.0..).map(|line| format!("{line} ")).collect();
                    self.post.truncate(before.1);
                    let stop = format!("sg_panicf({loc}, {}{values})", c_string(&format));
                    Ok(if made.is_empty() { stop } else { format!("({{ {made}{stop}; }})") })
                }
                // `x ?? exit(2)`: as `panic` above.
                Callee::Plain if name == "exit" => {
                    let before = (self.pre.len(), self.post.len());
                    let stop = self.exit(expr)?;
                    let made: String = self.pre.drain(before.0..).map(|line| format!("{line} ")).collect();
                    self.post.truncate(before.1);
                    Ok(if made.is_empty() { stop } else { format!("({{ {made}{stop}; }})") })
                }
                Callee::Plain if matches!(name.as_str(), "print" | "printError" | "assert") => Err(not_yet(expr.span, &format!("`{name}` used as a value"))),
                Callee::Plain => self.call(format!("f_{}", cname(name)), None, args, head, &expr.ty),
                Callee::Static { owner, name } => self.call(format!("f_{}_{name}", cname(owner)), None, args, head, &expr.ty),
                // A call through a function value kept in a variable.
                Callee::Value => {
                    let local = self.local(name);
                    let (callee, ty) = (if local.ty.is_borrow() { format!("(*{})", local.cname) } else { local.cname.clone() }, local.ty.strip_borrow().clone());
                    let Ty::Func(params, _) = ty else { return Err(not_yet(expr.span, "this call")) };
                    self.sigs.insert(callee.clone(), params);
                    self.call(callee, None, args, false, &expr.ty)
                }
                _ => Err(not_yet(expr.span, "this call")),
            },
            ExprKind::Method { recv, name, args, callee, .. } => match callee {
                Callee::Static { owner, name } => self.call(format!("f_{}_{name}", cname(owner)), None, args, head, &expr.ty),
                Callee::Method { owner, this } => {
                    let held = recv.ty.strip_borrow().clone();
                    let before = self.mark();
                    let receiver = match (this, &held) {
                        // A value that has no name: it is used up, or kept until the statement ends.
                        _ if !recv.is_place() => match (this, &held) {
                            // `this` by value: the value leaves the heap and the box is freed.
                            (This::Value, Ty::Ptr(inner)) if !recv.ty.is_borrow() && !matches!(**inner, Ty::Ptr(_)) => {
                                let (value, ctype, temp) = (self.value(recv)?, self.ctype(inner, recv.span)?, self.temp());
                                format!("({{ {ctype}* {temp} = {value}; {ctype} {temp}v = *{temp}; free({temp}); {temp}v; }})")
                            }
                            (This::Value, Ty::Ptr(_)) => return Err(not_yet(recv.span, "a method that takes `this` called on a pointer that has no name").hint("give it a name first")),
                            // The box is kept until the statement ends; the method goes along the pointer.
                            (_, Ty::Ptr(_)) => {
                                let mut place = self.look(recv)?;
                                let mut ty = &held;
                                while let Ty::Ptr(inner) = ty {
                                    place = format!("(*{place})");
                                    ty = inner;
                                }
                                format!("(&{place})")
                            }
                            (This::Value, _) => self.value(recv)?,
                            _ if recv.ty.is_borrow() => self.value(recv)?,
                            _ => format!("(&{})", self.look(recv)?),
                        },
                        // `this` by value on a `T*`: the value leaves the heap and the box is freed.
                        (This::Value, Ty::Ptr(inner)) if recv.moved => {
                            let (place, ctype, temp) = (self.lvalue(recv)?, self.ctype(inner, recv.span)?, self.temp());
                            format!("({{ {ctype}* {temp} = {}; {ctype} {temp}v = *{temp}; free({temp}); {temp}v; }})", self.move_of(&held, &place))
                        }
                        (This::Value, Ty::Ptr(_)) => format!("(*{})", self.lvalue(recv)?),
                        (This::Value, _) if recv.moved && self.droppable(&held) => {
                            let place = self.lvalue(recv)?;
                            self.move_of(&held, &place)
                        }
                        (This::Value, _) => self.lvalue(recv)?,
                        // A method of `T` called on a `T*`: along the pointer.
                        (This::Read | This::Ref, _) => {
                            let mut address = self.address(recv)?;
                            let mut ty = &held;
                            while let Ty::Ptr(inner) = ty {
                                address = format!("(*{address})");
                                ty = inner;
                            }
                            address
                        }
                    };
                    let this_ty = match this {
                        This::Value => Ty::Struct(owner.clone()),
                        This::Read => Ty::Read(Box::new(Ty::Struct(owner.clone()))),
                        This::Ref => Ty::Ref(Box::new(Ty::Struct(owner.clone()))),
                    };
                    let receiver = self.part(recv, &this_ty, receiver, before);
                    self.call(format!("f_{}_{name}", cname(owner)), Some(receiver), args, false, &expr.ty)
                }
                // A call through a function value kept in a field.
                Callee::FieldValue(_) => {
                    let mut place = self.look(recv)?;
                    let mut ty = recv.ty.strip_borrow();
                    while let Ty::Ptr(inner) = ty {
                        place = format!("(*{place})");
                        ty = inner;
                    }
                    let Ty::Struct(owner) = ty else { return Err(not_yet(expr.span, "this call")) };
                    let decl: &'a StructDecl = self.structs[owner.as_str()];
                    let field = decl.fields.iter().find(|field| field.name == *name).expect("the checker found the field");
                    let Ty::Func(params, _) = self.written(&field.ty)? else { return Err(not_yet(expr.span, "this call")) };
                    let callee = format!("{place}.m_{name}");
                    self.sigs.insert(callee.clone(), params);
                    self.call(callee, None, args, false, &expr.ty)
                }
                // `slot.take()` leaves `null`; `slot.replace(v)` leaves `v`. Both give what was there.
                Callee::Take => {
                    let (place, ty) = self.slot(recv)?;
                    self.ctype(&ty, recv.span)?;
                    Ok(self.move_of(&ty, &place))
                }
                Callee::Replace => {
                    let (place, ty) = self.slot(recv)?;
                    let new = self.value_as(&args[0], &ty)?;
                    let (ctype, temp) = (self.ctype(&ty, recv.span)?, self.temp());
                    Ok(format!("({{ {ctype} {temp}n = {new}; {ctype} {temp} = {place}; {place} = {temp}n; {temp}; }})"))
                }
                Callee::Join => {
                    if self.made.insert("sg_join_list".to_string()) {
                        let list = self.ctype(&Ty::List(Box::new(Ty::Str)), expr.span)?;
                        let _ = writeln!(self.glue, "static sg_string sg_join_list(sg_str separator, const {list}* v) {{ return sg_join(separator, v->ptr, v->len); }}");
                        let text = Ty::Read(Box::new(Ty::Str));
                        self.sigs.insert("sg_join_list".to_string(), vec![text, Ty::Read(Box::new(Ty::List(Box::new(Ty::Str))))]);
                    }
                    self.call("sg_join_list".to_string(), None, args, false, &expr.ty)
                }
                Callee::Plain if table_behind(&recv.ty).is_some() => {
                    let (table, _) = table_behind(&recv.ty).expect("just tested");
                    let table = table.clone();
                    let base = mangle(&table).ok_or_else(|| not_yet(expr.span, &format!("the type `{table}`")))?;
                    self.ctype(&table, expr.span)?;
                    let address = self.list_address(recv)?;
                    let is_map = matches!(table, Ty::Map(..));
                    match (name.as_str(), is_map) {
                        ("length", _) => Ok(format!("{address}->len")),
                        ("clear", _) => Ok(format!("sg_clear_{base}({address})")),
                        ("contains", _) => Ok(format!("sg_contains_{base}({address}, {})", self.key(&table, &args[0])?)),
                        // The key and the value are computed before the table is touched.
                        ("add", true) => {
                            let Ty::Map(key, value) = &table else { unreachable!("it is a map") };
                            let mut parts = Vec::new();
                            for (arg, want) in args.iter().zip([&**key, &**value]) {
                                let before = self.mark();
                                let written = self.value_as(arg, want)?;
                                parts.push(self.part(arg, want, written, before));
                            }
                            let (values, names) = self.in_order(parts)?;
                            let loc = self.loc(expr.span);
                            Ok(sequence(&names, format!("sg_add_{base}({address}, {}, {}, {loc})", values[0], values[1])))
                        }
                        ("add", false) => {
                            let Ty::Set(key) = &table else { unreachable!("it is a set") };
                            Ok(format!("sg_add_{base}({address}, {})", self.value_as(&args[0], key)?))
                        }
                        ("remove", false) => Ok(format!("sg_remove_{base}({address}, {})", self.key(&table, &args[0])?)),
                        ("remove", true) => {
                            let helper = self.table_helper(&table, "remove", expr.span)?;
                            Ok(format!("{helper}({address}, {})", self.key(&table, &args[0])?))
                        }
                        // A look at a value that may not be there; see `get` of a list.
                        ("get", true) => {
                            let Ty::Map(_, value) = &table else { unreachable!("it is a map") };
                            let helper = self.table_helper(&table, "get", expr.span)?;
                            let called = format!("{helper}({address}, {})", self.key(&table, &args[0])?);
                            if is_option(&Ty::Nullable(value.clone())) {
                                return Ok(called);
                            }
                            let (ctype, temp) = (self.ctype(&Ty::Nullable(value.clone()), expr.span)?, self.temp());
                            self.pre.push(format!("{ctype} {temp} = {called};"));
                            Ok(format!("(&{temp})"))
                        }
                        ("getOrAdd", true) => {
                            let Ty::Map(_, value) = &table else { unreachable!("it is a map") };
                            let helper = self.table_helper(&table, "getOrAdd", expr.span)?;
                            let key = self.key(&table, &args[0])?;
                            Ok(format!("{helper}({address}, {key}, {})", self.value_as(&args[1], value)?))
                        }
                        _ => Err(not_yet(expr.span, &format!("the method `{name}`"))),
                    }
                }
                Callee::Plain if list_behind(&recv.ty).is_some() => {
                    let (list, _) = list_behind(&recv.ty).expect("just tested");
                    let list = list.clone();
                    let base = mangle(&list).ok_or_else(|| not_yet(expr.span, &format!("the type `{list}`")))?;
                    self.ctype(&list, expr.span)?;
                    let helper = match name.as_str() {
                        "length" => return Ok(format!("{}->len", self.list_address(recv)?)),
                        "add" | "reverse" | "clear" => format!("sg_{name}_{base}"),
                        "get" => self.list_helper(&list, "get", expr.span)?,
                        "removeLast" => self.list_helper(&list, "pop", expr.span)?,
                        "indexOf" | "contains" => self.list_helper(&list, "index", expr.span)?,
                        "sort" => self.list_helper(&list, "sort", expr.span)?,
                        "addAll" => self.list_helper(&list, "addAll", expr.span)?,
                        _ => return Err(not_yet(expr.span, &format!("the method `{name}`"))),
                    };
                    let before = self.mark();
                    let address = self.list_address(recv)?;
                    let Ty::List(item) = &list else { unreachable!("it is a list") };
                    let by_pointer = matches!(name.as_str(), "indexOf" | "contains") && !matches!(**item, Ty::Int(_) | Ty::Float(_) | Ty::Bool | Ty::Char | Ty::Str);
                    let called = if by_pointer {
                        format!("{helper}({address}, {})", self.pointer(&args[0])?)
                    } else {
                        let receiver = self.part(recv, &Ty::Read(Box::new(list.clone())), address, before);
                        self.call(helper, Some(receiver), args, false, &expr.ty)?
                    };
                    match name.as_str() {
                        // A look at an item that may not be there: it is kept beside the
                        // statement, and what is given is a pointer to it. The list still
                        // owns the item, so this copy of it is never dropped.
                        "get" => {
                            let Ty::List(item) = &list else { unreachable!("it is a list") };
                            if is_option(&Ty::Nullable(item.clone())) {
                                return Ok(called);
                            }
                            let (ctype, temp) = (self.ctype(&Ty::Nullable(item.clone()), expr.span)?, self.temp());
                            self.pre.push(format!("{ctype} {temp} = {called};"));
                            Ok(format!("(&{temp})"))
                        }
                        "indexOf" => {
                            let (ctype, temp) = (self.ctype(&expr.ty, expr.span)?, self.temp());
                            Ok(format!("({{ int64_t {temp} = {called}; (({ctype}){{ {temp} >= 0, {temp} }}); }})"))
                        }
                        "contains" => Ok(format!("({called} >= 0)")),
                        _ => Ok(called),
                    }
                }
                // An array and a window have the methods of a list that only look.
                Callee::Plain if items_behind(&recv.ty).is_some() => {
                    let items = items_behind(&recv.ty).expect("just tested").clone();
                    let (Ty::Array(item, _) | Ty::Slice(item)) = &items else { unreachable!("it is an array or a slice") };
                    let slice = Ty::Slice(item.clone());
                    self.ctype(&slice, expr.span)?;
                    let helper = match (name.as_str(), &items) {
                        // The length of an array is part of its type.
                        ("length", Ty::Array(_, length)) if self.effect(recv) == Effect::Reads => return Ok(format!("((int64_t){length})")),
                        ("length", _) => return Ok(format!("{}.len", self.window(recv)?)),
                        ("get", _) => self.window_helper(&slice, "get", expr.span)?,
                        ("indexOf" | "contains", _) => self.window_helper(&slice, "index", expr.span)?,
                        ("sort" | "reverse", _) => self.window_helper(&slice, name, expr.span)?,
                        _ => return Err(not_yet(expr.span, &format!("the method `{name}`"))),
                    };
                    let before = self.mark();
                    let window = self.window(recv)?;
                    let by_pointer = matches!(name.as_str(), "indexOf" | "contains") && !matches!(**item, Ty::Int(_) | Ty::Float(_) | Ty::Bool | Ty::Char | Ty::Str);
                    let called = if by_pointer {
                        format!("{helper}({window}, {})", self.pointer(&args[0])?)
                    } else {
                        let receiver = self.part(recv, &Ty::Read(Box::new(slice.clone())), window, before);
                        self.call(helper, Some(receiver), args, false, &expr.ty)?
                    };
                    match name.as_str() {
                        // See `get` of a list.
                        "get" if is_option(&Ty::Nullable(item.clone())) => Ok(called),
                        "get" => {
                            let (ctype, temp) = (self.ctype(&Ty::Nullable(item.clone()), expr.span)?, self.temp());
                            self.pre.push(format!("{ctype} {temp} = {called};"));
                            Ok(format!("(&{temp})"))
                        }
                        "indexOf" => {
                            let (ctype, temp) = (self.ctype(&expr.ty, expr.span)?, self.temp());
                            Ok(format!("({{ int64_t {temp} = {called}; (({ctype}){{ {temp} >= 0, {temp} }}); }})"))
                        }
                        "contains" => Ok(format!("({called} >= 0)")),
                        _ => Ok(called),
                    }
                }
                // `string.withCapacity(n)`: a block of `n` bytes, or a panic that says why not.
                Callee::WithCapacity => {
                    let room = self.value_as(&args[0], &Ty::Int(IntTy::Nint))?;
                    let loc = self.loc(expr.span);
                    Ok(format!("sg_with_capacity({room}, {loc})"))
                }
                // `string.fromUtf8(bytes)`: the bytes are looked at as text where they are, if
                // they are text; otherwise the error says where the first bad byte lies.
                Callee::FromUtf8 => {
                    let bytes = Ty::Read(Box::new(Ty::Slice(Box::new(Ty::Int(IntTy::Byte)))));
                    let (given, gives) = (self.ctype(&bytes, expr.span)?, self.ctype(&expr.ty, expr.span)?);
                    if self.made.insert("sg_from_utf8".to_string()) {
                        let _ = writeln!(
                            self.glue,
                            "static {gives} sg_from_utf8({given} bytes) {{ int64_t bad = sg_utf8_bad(bytes.ptr, bytes.len); \
                             if (bad < 0) return ({gives}){{ .tag = 0, .u.v0 = {{ (sg_str){{ bytes.ptr, bytes.len }} }} }}; \
                             return ({gives}){{ .tag = 1, .u.v1 = {{ (S_TextError){{ .m_position = bad }} }} }}; }}"
                        );
                        self.sigs.insert("sg_from_utf8".to_string(), vec![bytes]);
                    }
                    self.call("sg_from_utf8".to_string(), None, args, head, &expr.ty)
                }
                Callee::Io(call) => self.io(*call, args, head, expr),
                Callee::Stream(call) => self.stream(*call, recv, args, head, expr),
                // `ulong.fromLittleEndian(bytes)`: the count is checked, then the bytes are one
                // load. Where the window was cut with a constant width just before, the C
                // compiler sees that the check never fails.
                Callee::FromBytes { ty, big } => {
                    let (bits, size) = (ty.bits(), ty.bits() / 8);
                    let fname = format!("sg_from_{}_{}", if *big { "be" } else { "le" }, int_name(*ty));
                    let bytes = Ty::Read(Box::new(Ty::Slice(Box::new(Ty::Int(IntTy::Byte)))));
                    let (given, int) = (self.ctype(&bytes, expr.span)?, int_ctype(*ty));
                    if self.made.insert(fname.clone()) {
                        let order = if *big { "BE" } else { "LE" };
                        let what = c_string(&format!("{}.{name}", ty.name()));
                        let _ = writeln!(
                            self.glue,
                            "static inline {int} {fname}({given} bytes, const char* loc) {{ \
                             if (__builtin_expect(bytes.len != {size}, 0)) sg_bytes_fail({what}, {size}, bytes.len, loc); \
                             uint{bits}_t v; memcpy(&v, bytes.ptr, {size}); return ({int})SG_{order}{bits}(v); }}"
                        );
                    }
                    let value = self.value_as(&args[0], &bytes)?;
                    let loc = self.loc(expr.span);
                    Ok(format!("{fname}({value}, {loc})"))
                }
                // `int.parse(text)`: one function for each type, made where it is first used.
                Callee::Parse(radix) => {
                    let Ty::Result(ty, _) = &expr.ty else { return Err(not_yet(expr.span, "`parse`")) };
                    let (fname, read) = match &**ty {
                        Ty::Int(ty) => (
                            format!("sg_parse{}_{}", if *radix == 16 { "_hex" } else { "" }, int_name(*ty)),
                            format!(
                                "__int128 v = 0; int64_t at = sg_read_whole(text.ptr, text.len, {radix}, (__int128){}, (__int128){}, &v);",
                                int_literal(ty.min(), if ty.signed() { IntTy::Long } else { IntTy::Ulong }),
                                int_literal(ty.max(), if ty.signed() { IntTy::Long } else { IntTy::Ulong }),
                            ),
                        ),
                        Ty::Float(ty) => (
                            format!("sg_parse_{}", float_ctype(*ty)),
                            format!("double v = 0; int64_t at = sg_read_fraction(text.ptr, text.len, {}, &v);", *ty == FloatTy::Float),
                        ),
                        _ => return Err(not_yet(expr.span, "`parse`")),
                    };
                    let given = Ty::Read(Box::new(Ty::Str));
                    let (text_c, gives) = (self.ctype(&given, expr.span)?, self.ctype(&expr.ty, expr.span)?);
                    let value_c = self.ctype(ty, expr.span)?;
                    let error_c = self.ctype(&Ty::Enum("NumberError".to_string()), expr.span)?;
                    if self.made.insert(fname.clone()) {
                        let _ = writeln!(
                            self.glue,
                            "static {gives} {fname}({text_c} text) {{ {read} \
                             if (at == -1) return ({gives}){{ .tag = 0, .u.v0 = {{ ({value_c})v }} }}; \
                             if (at == -2) return ({gives}){{ .tag = 1, .u.v1 = {{ ({error_c}){{ .tag = 1 }} }} }}; \
                             return ({gives}){{ .tag = 1, .u.v1 = {{ ({error_c}){{ .tag = 0, .u.v0 = {{ at }} }} }} }}; }}"
                        );
                        self.sigs.insert(fname.clone(), vec![given]);
                    }
                    self.call(fname, None, args, head, &expr.ty)
                }
                // The next part of the text, or `null`.
                Callee::Plain if matches!(recv.ty.strip_borrow(), Ty::TextIter(_)) => {
                    let Ty::TextIter(kind) = recv.ty.strip_borrow() else { unreachable!("tested above") };
                    let (step, temp) = (next_of(*kind), self.temp());
                    if *kind == TextIter::ReadLines {
                        self.read_lines_glue(expr.span)?;
                    }
                    let iter_c = self.ctype(recv.ty.strip_borrow(), recv.span)?;
                    let (start, at) = match recv.is_place() {
                        true => (String::new(), format!("&{}", self.lvalue(recv)?)),
                        false => (format!("{iter_c} {temp}i = {}; ", self.value(recv)?), format!("&{temp}i")),
                    };
                    Ok(match kind {
                        TextIter::Chars | TextIter::ReadLines => {
                            let option = self.ctype(&expr.ty, expr.span)?;
                            format!("({{ {start}{option} {temp}; {temp}.has = {step}({at}, &{temp}.value); {temp}; }})")
                        }
                        _ => format!("({{ {start}sg_str {temp}; if (!{step}({at}, &{temp})) {temp} = SG_NO_STR; {temp}; }})"),
                    })
                }
                // `text.write(x)` adds at the end of the text, as `f.write` does; `clear` keeps
                // the block for what is written next.
                Callee::Plain if matches!(name.as_str(), "write" | "clear") && (is_text(&recv.ty) || is_text_behind_pointer(&recv.ty)) => {
                    let out = self.list_address(recv)?;
                    match name.as_str() {
                        "clear" => Ok(format!("(({out})->len = 0)")),
                        _ => {
                            let temp = self.temp();
                            let written = self.write_into(&temp, &args[0])?;
                            Ok(format!("({{ sg_string* {temp} = {out}; {written}; }})"))
                        }
                    }
                }
                Callee::Plain if is_text(&recv.ty) || is_text_behind_pointer(&recv.ty) => match name.as_str() {
                    // The bytes of the text, looked at where they are.
                    "asBytes" => {
                        let window = self.ctype(&expr.ty, expr.span)?;
                        if self.made.insert("sg_bytes".to_string()) {
                            let _ = writeln!(self.glue, "static inline {window} sg_bytes(sg_str s) {{ return ({window}){{ (uint8_t*)s.ptr, s.len }}; }}");
                        }
                        Ok(format!("sg_bytes({})", self.view(recv)?))
                    }
                    // Where the character, the text or the byte first (or last) lies, or `null`.
                    "indexOf" | "lastIndexOf" | "contains" | "startsWith" | "endsWith" | "split" => {
                        let temp = self.temp();
                        let text = self.view(recv)?;
                        // A separator that is one character is kept inside the iterator.
                        if name == "split" && *args[0].ty.strip_borrow() == Ty::Char {
                            let separator = self.value(&args[0])?;
                            return Ok(format!("({{ sg_str {temp}h = {text}; sg_split_char({temp}h, {separator}); }})"));
                        }
                        let (setup, needle) = self.needle(&args[0], &temp)?;
                        let start = format!("sg_str {temp}h = {text}; {setup}");
                        Ok(match name.as_str() {
                            "indexOf" | "lastIndexOf" => {
                                let ctype = self.ctype(&expr.ty, expr.span)?;
                                let find = if name == "indexOf" { "sg_find" } else { "sg_find_last" };
                                format!("({{ {start} int64_t {temp}at = {find}({temp}h, {needle}); (({ctype}){{ {temp}at >= 0, {temp}at >= 0 ? {temp}at : 0 }}); }})")
                            }
                            "contains" => format!("({{ {start} sg_find({temp}h, {needle}) >= 0; }})"),
                            "startsWith" => format!("({{ {start} sg_starts({temp}h, {needle}); }})"),
                            "endsWith" => format!("({{ {start} sg_ends({temp}h, {needle}); }})"),
                            _ => {
                                let loc = self.loc(expr.span);
                                format!("({{ {start} sg_split_str({temp}h, {needle}, {loc}); }})")
                            }
                        })
                    }
                    "replaceAll" => {
                        let (temp, other) = (self.temp(), self.temp());
                        let text = self.view(recv)?;
                        let (find, old) = self.needle(&args[0], &temp)?;
                        let (put, new) = self.needle(&args[1], &other)?;
                        let loc = self.loc(expr.span);
                        Ok(format!("({{ sg_str {temp}h = {text}; {find} {put} sg_replace_all({temp}h, {old}, {new}, {loc}); }})"))
                    }
                    "repeat" => {
                        let temp = self.temp();
                        let text = self.view(recv)?;
                        let times = self.value_as(&args[0], &Ty::Int(IntTy::Nint))?;
                        let loc = self.loc(expr.span);
                        Ok(format!("({{ sg_str {temp}h = {text}; sg_repeat({temp}h, {times}, {loc}); }})"))
                    }
                    "trim" | "trimStart" | "trimEnd" => Ok(format!("sg_{}({})", if name == "trim" { "trim" } else if name == "trimStart" { "trim_start" } else { "trim_end" }, self.view(recv)?)),
                    "equalsIgnoreAsciiCase" => {
                        let temp = self.temp();
                        let (text, other) = (self.view(recv)?, self.view(&args[0])?);
                        Ok(format!("({{ sg_str {temp}a = {text}; sg_str {temp}b = {other}; sg_eq_ascii_case({temp}a, {temp}b); }})"))
                    }
                    "chars" | "splitWhitespace" | "lines" => Ok(format!("sg_iter_of({})", self.view(recv)?)),
                    "length" => Ok(format!("{}.len", self.view(recv)?)),
                    "equals" => Ok(format!("sg_eq({}, {})", self.view(recv)?, self.view(&args[0])?)),
                    // Less than zero, zero or more than zero, as -1, 0 and 1.
                    "compare" => {
                        let temp = self.temp();
                        Ok(format!("({{ int {temp} = sg_cmp({}, {}); (int32_t)(({temp} > 0) - ({temp} < 0)); }})", self.view(recv)?, self.view(&args[0])?))
                    }
                    // A copy is the one place where text is allocated.
                    "clone" => Ok(format!("sg_clone({})", self.view(recv)?)),
                    _ => Err(not_yet(expr.span, &format!("the method `{name}`"))),
                },
                // `f.write(...)` adds to the text a `format` method is writing.
                Callee::Plain if *recv.ty.strip_borrow() == Ty::Formatter => {
                    let out = self.value(recv)?;
                    self.write_into(&out, &args[0])
                }
                Callee::Plain if matches!(name.as_str(), "equals" | "compare") && matches!(recv.ty.strip_borrow(), Ty::Int(_) | Ty::Bool) => {
                    let (values, names) = self.two(recv, &args[0], head)?;
                    let (ctype, temp) = (self.ctype(recv.ty.strip_borrow(), recv.span)?, self.temp());
                    Ok(sequence(&names, match name.as_str() {
                        "equals" => format!("({} == {})", values[0], values[1]),
                        _ => format!("({{ {ctype} {temp}a = {}, {temp}b = {}; (int32_t)(({temp}a > {temp}b) - ({temp}a < {temp}b)); }})", values[0], values[1]),
                    }))
                }
                Callee::Plain if matches!(name.as_str(), "equals" | "compare") && matches!(recv.ty.strip_borrow(), Ty::Float(_)) => {
                    let (values, names) = self.two(recv, &args[0], head)?;
                    let helper = if name == "equals" { "sg_feq" } else { "sg_fcmp" };
                    Ok(sequence(&names, format!("{helper}({}, {})", values[0], values[1])))
                }
                Callee::Plain if matches!(recv.ty, Ty::Float(_)) && matches!(name.as_str(), "abs" | "sqrt" | "floor" | "ceil" | "round" | "truncate" | "isNaN" | "isInfinite") => {
                    let Ty::Float(ty) = recv.ty else { unreachable!("tested above") };
                    let value = self.first(recv, head)?;
                    Ok(match name.as_str() {
                        "isNaN" => format!("(isnan({value}) != 0)"),
                        "isInfinite" => format!("(isinf({value}) != 0)"),
                        _ => format!("{}({value})", float_fn(name, ty)),
                    })
                }
                Callee::Plain if matches!(recv.ty, Ty::Float(_)) && matches!(name.as_str(), "exp" | "log" | "log2" | "log10" | "sin" | "cos" | "tan" | "asin" | "acos" | "atan") => {
                    let Ty::Float(ty) = recv.ty else { unreachable!("tested above") };
                    let value = self.first(recv, head)?;
                    Ok(format!("{}({value})", float_fn(name, ty)))
                }
                Callee::Plain if matches!(recv.ty, Ty::Float(_)) && matches!(name.as_str(), "pow" | "atan2" | "min" | "max") => {
                    let Ty::Float(ty) = recv.ty else { unreachable!("tested above") };
                    let (values, names) = self.two(recv, &args[0], head)?;
                    let made = match name.as_str() {
                        "min" | "max" => format!("(({})sg_f{name}({}, {}))", float_ctype(ty), values[0], values[1]),
                        _ => format!("{}({}, {})", float_fn(name, ty), values[0], values[1]),
                    };
                    Ok(sequence(&names, made))
                }
                Callee::Plain if matches!(recv.ty, Ty::Float(_)) && name == "fusedMultiplyAdd" => {
                    let Ty::Float(ty) = recv.ty else { unreachable!("tested above") };
                    let mut parts = Vec::new();
                    for side in [&**recv, &args[0], &args[1]] {
                        let before = self.mark();
                        let value = self.first(side, head)?;
                        parts.push(self.part(side, &side.ty, value, before));
                    }
                    let (values, names) = self.in_order(parts)?;
                    Ok(sequence(&names, format!("{}({}, {}, {})", float_fn(name, ty), values[0], values[1], values[2])))
                }
                Callee::Plain if matches!(recv.ty, Ty::Int(_)) && matches!(name.as_str(), "min" | "max") => {
                    let Ty::Int(ty) = recv.ty else { unreachable!("tested above") };
                    let (int, temp) = (int_ctype(ty), self.temp());
                    let (values, names) = self.two(recv, &args[0], head)?;
                    let op = if name == "min" { "<" } else { ">" };
                    Ok(sequence(&names, format!("({{ {int} {temp}a = {}, {temp}b = {}; {temp}a {op} {temp}b ? {temp}a : {temp}b; }})", values[0], values[1])))
                }
                // The ASCII tests and changes of a `char` or a `byte`, and what a `char` knows of itself.
                Callee::Plain if matches!(recv.ty, Ty::Char | Ty::Int(_)) && is_char_method(name) => {
                    let ctype = self.ctype(&recv.ty, recv.span)?;
                    let value = self.first(recv, head)?;
                    Ok(match name.as_str() {
                        "isAscii" => format!("((uint32_t)({value}) < 0x80u)"),
                        "isAsciiDigit" => format!("sg_ascii_digit({value})"),
                        "isAsciiLetter" => format!("sg_ascii_letter({value})"),
                        "isAsciiAlphanumeric" => format!("sg_ascii_alnum({value})"),
                        "isAsciiUpper" => format!("sg_ascii_upper_is({value})"),
                        "isAsciiLower" => format!("sg_ascii_lower_is({value})"),
                        "isAsciiWhitespace" => format!("sg_ascii_space({value})"),
                        "toAsciiUpper" => format!("(({ctype})sg_ascii_upper({value}))"),
                        "toAsciiLower" => format!("(({ctype})sg_ascii_lower({value}))"),
                        "isWhitespace" => format!("sg_is_space({value})"),
                        "utf8Length" => {
                            let temp = self.temp();
                            format!("({{ uint32_t {temp} = {value}; (int64_t)({temp} < 0x80u ? 1 : {temp} < 0x800u ? 2 : {temp} < 0x10000u ? 3 : 4); }})")
                        }
                        _ => {
                            let (option, temp) = (self.ctype(&expr.ty, expr.span)?, self.temp());
                            format!("({{ __int128 {temp}v = (__int128)({value}); {option} {temp}; {temp}.has = sg_is_char({temp}v); {temp}.value = (uint32_t){temp}v; {temp}; }})")
                        }
                    })
                }
                Callee::Plain if matches!(name.as_str(), "equals" | "compare") && matches!(recv.ty.strip_borrow(), Ty::Char) => {
                    let (values, names) = self.two(recv, &args[0], head)?;
                    let temp = self.temp();
                    Ok(sequence(&names, match name.as_str() {
                        "equals" => format!("({} == {})", values[0], values[1]),
                        _ => format!("({{ uint32_t {temp}a = {}, {temp}b = {}; (int32_t)(({temp}a > {temp}b) - ({temp}a < {temp}b)); }})", values[0], values[1]),
                    }))
                }
                // `big.tryToInt()`: the value if it fits, `null` if not; `wrappingToInt()` keeps
                // the bits that fit. A fractional number goes toward zero first, as in a cast.
                Callee::Plain if name.starts_with("tryTo") || name.starts_with("wrappingTo") => {
                    let to = match &expr.ty {
                        Ty::Int(to) => *to,
                        Ty::Nullable(inner) => match **inner {
                            Ty::Int(to) => to,
                            _ => return Err(not_yet(expr.span, &format!("the method `{name}`"))),
                        },
                        _ => return Err(not_yet(expr.span, &format!("the method `{name}`"))),
                    };
                    let (int, temp) = (int_ctype(to), self.temp());
                    let value = self.first(recv, head)?;
                    if name.starts_with("wrappingTo") {
                        return Ok(format!("(({int})({value}))"));
                    }
                    let option = self.ctype(&expr.ty, expr.span)?;
                    Ok(match recv.ty {
                        Ty::Float(_) => format!(
                            "({{ double {temp}t = __builtin_trunc({value}); {option} {temp} = {{ false, 0 }}; \
                             if ({temp}t >= SG_LO({int}) && {temp}t < SG_HI({int})) {{ {temp}.has = true; {temp}.value = ({int}){temp}t; }} {temp}; }})"
                        ),
                        _ => format!(
                            "({{ __int128 {temp}v = (__int128)({value}); {option} {temp}; {temp}.value = ({int}){temp}v; \
                             {temp}.has = (__int128){temp}.value == {temp}v; {temp}; }})"
                        ),
                    })
                }
                // `n.toLittleEndian()`: the number is stored into the array as one store.
                Callee::Plain if matches!(name.as_str(), "toLittleEndian" | "toBigEndian") => {
                    let Ty::Int(ty) = recv.ty.strip_borrow() else { return Err(not_yet(expr.span, &format!("the method `{name}`"))) };
                    let (bits, size, order) = (ty.bits(), ty.bits() / 8, if name == "toBigEndian" { "BE" } else { "LE" });
                    let (array, temp) = (self.ctype(&expr.ty, expr.span)?, self.temp());
                    let value = self.first(recv, head)?;
                    Ok(format!(
                        "({{ {array} {temp}; uint{bits}_t {temp}v = SG_{order}{bits}((uint{bits}_t)({value})); memcpy({temp}.items, &{temp}v, {size}); {temp}; }})"
                    ))
                }
                Callee::Plain if name == "toString" => {
                    let (format, values) = self.format(recv)?;
                    Ok(format!("sg_format({}{values})", c_string(&format)))
                }
                // `null` where the plain operator would panic: overflow, and division by zero.
                Callee::Plain if matches!(name.as_str(), "tryAdd" | "trySubtract" | "tryMultiply" | "tryDivide") => {
                    let Ty::Nullable(inner) = &expr.ty else { return Err(not_yet(expr.span, &format!("the method `{name}`"))) };
                    let Ty::Int(ty) = &**inner else { return Err(not_yet(expr.span, &format!("the method `{name}`"))) };
                    let (option, int, temp) = (self.ctype(&expr.ty, expr.span)?, int_ctype(*ty), self.temp());
                    let (values, names) = self.two(recv, &args[0], head)?;
                    let (left, right) = (&values[0], &values[1]);
                    let builtin = match name.as_str() {
                        "tryAdd" => "add",
                        "trySubtract" => "sub",
                        "tryMultiply" => "mul",
                        _ => {
                            return Ok(sequence(&names, format!(
                                "({{ {int} {temp}a = {left}, {temp}b = {right}; {option} {temp} = {{ false, 0 }}; \
                                 if ({temp}b != 0 && !(({int})-1 < 0 && {temp}b == ({int})-1 && __builtin_sub_overflow(({int})0, {temp}a, &{temp}.value))) \
                                 {{ {temp}.has = true; {temp}.value = {temp}a / {temp}b; }} {temp}; }})"
                            )));
                        }
                    };
                    Ok(sequence(&names, format!("({{ {option} {temp}; {temp}.has = !__builtin_{builtin}_overflow({left}, {right}, &{temp}.value); {temp}; }})")))
                }
                Callee::Plain => {
                    let wrapping = match name.as_str() {
                        "wrappingAdd" => "wadd",
                        "wrappingSubtract" => "wsub",
                        "wrappingMultiply" => "wmul",
                        _ => return Err(not_yet(expr.span, &format!("the method `{name}`"))),
                    };
                    let Ty::Int(ty) = &expr.ty else { return Err(not_yet(expr.span, &format!("the method `{name}`"))) };
                    let (values, names) = self.two(recv, &args[0], head)?;
                    Ok(sequence(&names, format!("sg_{wrapping}_{}({}, {})", int_name(*ty), values[0], values[1])))
                }
                _ => Err(not_yet(expr.span, &format!("`{name}`"))),
            },
            _ => Err(not_yet(expr.span, "this expression")),
        }
    }
}

/// `+`, `-`, `*`, `/` and `%` on fractional numbers, as the hardware does them: nothing to
/// check, a division by zero gives an infinity or `NaN`. `%` is C's `fmod`.
fn fractional(op: BinOp, ty: FloatTy, left: &str, right: &str) -> String {
    match op {
        BinOp::Rem => format!("{}({left}, {right})", float_fn("fmod", ty)),
        _ => format!("({left} {} {right})", op.symbol()),
    }
}

/// A C expression that first runs the declarations `names` (the earlier parts, kept in
/// variables) and then gives `value`.
fn sequence(names: &str, value: String) -> String {
    if names.is_empty() { value } else { format!("({{ {names}{value}; }})") }
}

/// Whether the block ends with a statement that leaves it: what follows would never run.
fn ends_by_leaving(block: &Block) -> bool {
    matches!(block.last().map(|stmt| &stmt.kind), Some(StmtKind::Return(_) | StmtKind::Break | StmtKind::Continue))
}

/// The names of a pattern, each with the C line that declares it.
type Names = Vec<(Local, String)>;

/// The ways a pattern can match, each without `or`: `Add(Number(a) or Negate(a), _)` gives
/// `Add(Number(a), _)` and `Add(Negate(a), _)`.
fn alternatives(pattern: &Pattern) -> Vec<Pattern> {
    match &pattern.kind {
        PatternKind::Or(each) => each.iter().flat_map(alternatives).collect(),
        PatternKind::Variant { name, args: Some(args), index } => {
            let mut ways: Vec<Vec<Pattern>> = vec![Vec::new()];
            for arg in args {
                let inner = alternatives(arg);
                ways = ways.into_iter().flat_map(|way| inner.iter().map(move |one| way.iter().cloned().chain(std::iter::once(one.clone())).collect())).collect();
            }
            ways.into_iter()
                .map(|args| Pattern { kind: PatternKind::Variant { name: name.clone(), args: Some(args), index: *index }, span: pattern.span })
                .collect()
        }
        _ => vec![pattern.clone()],
    }
}

/// Whether an `or` is written anywhere in the pattern.
fn has_choice(pattern: &Pattern) -> bool {
    match &pattern.kind {
        PatternKind::Or(_) => true,
        PatternKind::Variant { args: Some(args), .. } => args.iter().any(has_choice),
        _ => false,
    }
}

/// Whether the pattern takes `null` itself, alone or in an `or`.
fn takes_null(pattern: &Pattern) -> bool {
    match &pattern.kind {
        PatternKind::Null => true,
        PatternKind::Or(each) => each.iter().any(takes_null),
        _ => false,
    }
}

/// `T v_x_3 = init;` made into `T v_x_3;`.
/// The parts of a chain of `&&`, left to right.
fn links(expr: &Expr, op: BinOp) -> Vec<&Expr> {
    match &expr.kind {
        ExprKind::Binary(inner, left, right) if *inner == op => {
            let mut parts = links(left, op);
            parts.extend(links(right, op));
            parts
        }
        _ => vec![expr],
    }
}

/// A chain of `&&` or `||` in which an `is` names values.
fn names_in_chain(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Binary(op @ (BinOp::And | BinOp::Or), ..) => links(expr, *op).into_iter().any(names_tested),
        _ => false,
    }
}

/// An `is` test with names, also under `!`.
fn names_tested(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::IsVariant { pattern, .. } => !pattern.names().is_empty(),
        ExprKind::Unary(UnOp::Not, inner) => matches!(&inner.kind, ExprKind::IsVariant { pattern, .. } if !pattern.names().is_empty()),
        _ => false,
    }
}

fn declaration(line: &str, cname: &str) -> String {
    let marker = format!(" {cname} = ");
    let at = line.find(&marker).expect("a name is declared with its value");
    format!("{} {cname};", &line[..at])
}

/// `T v_x_5 = init;` made into `v_x_3 = init;`, setting the name declared first.
fn assignment(line: &str, cname: &str, first: &str) -> String {
    let marker = format!(" {cname} = ");
    let at = line.find(&marker).expect("a name is declared with its value");
    format!("{first} = {}", &line[at + marker.len()..])
}
