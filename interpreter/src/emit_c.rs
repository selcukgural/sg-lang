//! The first back end: writes a checked program as C.
//!
//! The checker has already done the work that makes a program safe; what is left to run time
//! are the same data-dependent failures the interpreter reports, with the same messages:
//! overflow, a shift count outside the bits of the type, division by zero, a cast that does
//! not fit, `assert` and `panic`.
//!
//! Only a part of the language is compiled so far: integers, `bool`, text, functions, structs,
//! enums with `match` and `is`, `Result` and `try`, `List`, `HashMap` and `HashSet`, arrays and
//! slices, function values, `T*` and `new`, `T?`, `read` and `ref`, and `print`. Anything else
//! is an error that names what is missing, so nothing is compiled wrongly in silence.
//!
//! How values are represented: `byte`, `int`, `long` and `nint` are fixed-size C integers; a
//! struct is a C struct passed by value; `read T` / `ref T` are pointers; a `string` is its
//! bytes, a length and a capacity, and a `read string` is a pointer and a length. A `T*` is a
//! pointer to one `malloc`ed `T`. A `T*?` is the same pointer, where `NULL` is `null`: it
//! costs nothing. Any other `T?` is the value and a flag beside it. An enum is a tag and,
//! beside it, room for the largest variant (a C union); a `match` is a chain of tests on the tag.
//! A `Result<T, E>` is such an enum with the variants `Ok(T value)` and `Error(E cause)`; `try`
//! is one test on the tag and, on an error, a `return`. A `List<T>` is a pointer to its items,
//! a length and a capacity; `xs[i]` is the item where it is, behind a check of the index.
//!
//! An array `T[N]` is a C array inside a struct, so that it is passed, returned and copied by
//! value like any other value: it lies where its variable or its field lies, and nothing is
//! allocated for it. A slice (`read T[]`, `ref T[]`) is a window: a pointer to the first item
//! and a count, whatever the items are kept in. A range `xs[a..b]` checks its two ends once and
//! gives such a window; an index into a window is checked against the count.
//!
//! Writing and comparing: for each struct, enum and list that is written or compared, one
//! helper is made that writes it into a text (`sg_show_...`) and one that tells whether two
//! are equal (`sg_same_...`). They call the `format`, `equals` and `compare` a struct wrote.
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
use crate::types::{IntTy, Ty};

type Res<T> = Result<T, Diag>;

/// What every compiled program starts with: the checks, written once.
const RUNTIME: &str = r#"#include <stdarg.h>
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

/* A panic is never the usual path: its code is kept out of the way of the code that runs. */
#define SG_COLD __attribute__((cold, noinline))

SG_COLD static _Noreturn void sg_die(const char* loc) {
    fputs(loc, stderr);
    exit(1);
}
SG_COLD static _Noreturn void sg_overflow(long long a, const char* op, long long b, const char* ty, const char* loc) {
    fflush(stdout);
    fprintf(stderr, "panic: overflow: %lld %s %lld does not fit in %s\n", a, op, b, ty);
    sg_die(loc);
}
SG_COLD static _Noreturn void sg_neg_overflow(long long a, const char* ty, const char* loc) {
    fflush(stdout);
    fprintf(stderr, "panic: overflow: -(%lld) does not fit in %s\n", a, ty);
    sg_die(loc);
}
SG_COLD static _Noreturn void sg_shift_fail(long long n, const char* ty, int bits, const char* loc) {
    fflush(stdout);
    fprintf(stderr, "panic: shift by %lld is out of range: %s has %d bits\n", n, ty, bits);
    sg_die(loc);
}
SG_COLD static _Noreturn void sg_div_zero(long long a, const char* op, const char* loc) {
    fflush(stdout);
    fprintf(stderr, "panic: division by zero: %lld %s 0\n", a, op);
    sg_die(loc);
}
SG_COLD static _Noreturn void sg_cast_fail(long long v, const char* ty, const char* loc) {
    fflush(stdout);
    fprintf(stderr, "panic: value %lld does not fit in %s\n", v, ty);
    sg_die(loc);
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
SG_COLD static _Noreturn void sg_assert_fail_i(const char* text, long long l, long long r, const char* loc) {
    fflush(stdout);
    fprintf(stderr, "panic: assert failed: %s\n  left: %lld\n  right: %lld\n", text, l, r);
    sg_die(loc);
}
SG_COLD static _Noreturn void sg_assert_fail_b(const char* text, bool l, bool r, const char* loc) {
    fflush(stdout);
    fprintf(stderr, "panic: assert failed: %s\n  left: %s\n  right: %s\n", text, l ? "true" : "false", r ? "true" : "false");
    sg_die(loc);
}

/* One set of checked operations for each integer type. Overflow is caught by the compiler's
   own overflow builtins: one branch that is never taken in a correct program. */
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
static inline T sg_cast_##N(int64_t v, const char* loc) { T r = (T)v; if (__builtin_expect((int64_t)r != v, 0)) sg_cast_fail(v, NAME, loc); return r; } \
static inline T sg_wadd_##N(T a, T b) { return (T)((uint64_t)a + (uint64_t)b); } \
static inline T sg_wsub_##N(T a, T b) { return (T)((uint64_t)a - (uint64_t)b); } \
static inline T sg_wmul_##N(T a, T b) { return (T)((uint64_t)a * (uint64_t)b); } \
static inline T sg_shl_##N(T a, int64_t n, const char* loc) { if (__builtin_expect((uint64_t)n >= sizeof(T) * 8, 0)) sg_shift_fail(n, NAME, (int)sizeof(T) * 8, loc); return (T)((uint64_t)a << n); } \
static inline T sg_shr_##N(T a, int64_t n, const char* loc) { if (__builtin_expect((uint64_t)n >= sizeof(T) * 8, 0)) sg_shift_fail(n, NAME, (int)sizeof(T) * 8, loc); return (T)(a >> n); } \
static inline T sg_wshl_##N(T a, int64_t n) { return (T)((uint64_t)a << (n & (int64_t)(sizeof(T) * 8 - 1))); } \
static inline T sg_wshr_##N(T a, int64_t n) { return (T)(a >> (n & (int64_t)(sizeof(T) * 8 - 1))); }

SG_INT(uint8_t, byte, "byte")
SG_INT(int32_t, int, "int")
SG_INT(int64_t, long, "long")
SG_INT(int64_t, nint, "nint")

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
/* Where the first byte lies that is not part of valid UTF-8, or -1 when the bytes are text. */
static int64_t sg_utf8_bad(const uint8_t* bytes, int64_t len) {
    int64_t at = 0;
    while (at < len) {
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
static sg_string sg_format(const char* format, ...) {
    va_list count, write;
    va_start(count, format);
    va_copy(write, count);
    int n = vsnprintf(NULL, 0, format, count);
    va_end(count);
    if (n <= 0) { va_end(write); return SG_LIT("", 0); }
    uint8_t* bytes = malloc((size_t)n + 1);
    if (!bytes) abort();
    vsnprintf((char*)bytes, (size_t)n + 1, format, write);
    va_end(write);
    return (sg_string){ bytes, n, n };
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
/* Text that is put together piece by piece: what a `Formatter` writes into. */
static void sg_put(sg_string* out, const void* bytes, int64_t count) {
    if (count <= 0) return;
    if (out->len + count > out->cap) {
        int64_t room = out->cap ? out->cap * 2 : 32;
        while (room < out->len + count) room *= 2;
        uint8_t* grown = out->cap ? realloc(out->ptr, (size_t)room) : malloc((size_t)room);
        if (!grown) abort();
        out->ptr = grown;
        out->cap = room;
    }
    memcpy(out->ptr + out->len, bytes, (size_t)count);
    out->len += count;
}
static void sg_puts(sg_string* out, const char* text) { sg_put(out, text, (int64_t)strlen(text)); }
static void sg_putf(sg_string* out, const char* format, ...) {
    char small[128];
    va_list count, write;
    va_start(count, format);
    va_copy(write, count);
    int n = vsnprintf(small, sizeof small, format, count);
    va_end(count);
    if (n < (int)sizeof small) {
        sg_put(out, small, n);
    } else {
        char* large = malloc((size_t)n + 1);
        if (!large) abort();
        vsnprintf(large, (size_t)n + 1, format, write);
        sg_put(out, large, n);
        free(large);
    }
    va_end(write);
}
static void sg_put_int(sg_string* out, long long value) {
    char digits[24];
    char* at = digits + sizeof digits;
    unsigned long long left = value < 0 ? 0ULL - (unsigned long long)value : (unsigned long long)value;
    do { *--at = (char)('0' + left % 10); left /= 10; } while (left);
    if (value < 0) *--at = '-';
    sg_put(out, at, digits + sizeof digits - at);
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
        checked,
        locs: Vec::new(),
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
        head: false,
        exits: 0,
        ways_out: 0,
        in_test: false,
        late_types: String::new(),
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
    for decl in &prog.structs {
        for method in &decl.methods {
            let params = method.params.iter().map(|param| emitter.written(&param.ty)).collect::<Res<Vec<Ty>>>()?;
            emitter.sigs.insert(format!("f_{}_{}", cname(&decl.name), method.name), params);
        }
    }

    let mut protos = String::new();
    let mut bodies = String::new();
    for func in &prog.funcs {
        emitter.func(func, None, &mut protos, &mut bodies)?;
    }
    for decl in &prog.structs {
        for method in &decl.methods {
            emitter.func(method, Some(&decl.name), &mut protos, &mut bodies)?;
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
        IntTy::Byte => "uint8_t",
        IntTy::Int => "int32_t",
        IntTy::Long | IntTy::Nint => "int64_t",
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
        Ty::Bool => "bool".to_string(),
        Ty::Str => "string".to_string(),
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
        ExprKind::Int(..) | ExprKind::Bool(_) | ExprKind::Str(_) | ExprKind::Char(_) | ExprKind::Null | ExprKind::Var(_) | ExprKind::FuncRef(_) => false,
        ExprKind::Array { args, items, .. } => any(&mut args.iter().chain(items.iter().flatten())),
        ExprKind::AsSlice(inner) => may_leave(inner),
        ExprKind::Field { recv: inner, .. }
        | ExprKind::New(inner)
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
            InterpPart::Expr(inner) => Some(inner),
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

/// One piece of a line that is being written.
enum Piece {
    /// A `printf` format and the values for it, each with its comma in front.
    Text(String, String),
    /// A value written by the helper of its type: the helper, and a pointer to the value.
    Show(String, String),
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
    /// A debug build: overflow panics. Otherwise it wraps.
    checked: bool,
    /// The place of each line that can panic, as it is printed under the message.
    locs: Vec<String>,
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
}

impl<'a> Emitter<'a> {
    /// The name of the C constant that holds where `span` is in the source.
    fn loc(&mut self, span: Span) -> String {
        self.locs.push(self.sources.snippet(span));
        format!("sg_loc_{}", self.locs.len() - 1)
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
            Ty::Bool => "bool".to_string(),
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
                    Ty::Int(_) | Ty::Bool => (
                        key_c.clone(),
                        "sg_hash_word(0, (uint64_t)key)".to_string(),
                        "*held == key".to_string(),
                        "*held".to_string(),
                        if **key == Ty::Bool { "sg_puts(out, key ? \"true\" : \"false\");".to_string() } else { "sg_put_int(out, (long long)key);".to_string() },
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
        let known = |full: &str| self.structs.contains_key(full) || self.enums.contains_key(full);
        let file = self.files.iter().find(|file| at.start <= file.end)?;
        let own = if file.module.is_empty() || name.contains('.') { name.to_string() } else { format!("{}.{name}", file.module) };
        if known(&own) {
            return Some(own);
        }
        // The built-in `TextError` is seen from every module with nothing written.
        if name == "TextError" {
            return Some(name.to_string());
        }
        let public = |full: &str| self.structs.get(full).map(|decl| decl.public).or(self.enums.get(full).map(|decl| decl.public)) == Some(true);
        file.uses.iter().map(|(used, _)| format!("{used}.{name}")).find(|full| public(full))
    }

    /// A type as it is written in a signature or a field.
    fn written(&self, ty: &TypeExpr) -> Res<Ty> {
        let unknown = || not_yet(ty.span, &format!("the type `{}`", self.text(ty.span)));
        let mut found = match (Ty::from_name(&ty.name), ty.args.as_slice()) {
            (None, [ok, error]) if ty.name == "Result" => Ty::Result(Box::new(self.written(ok)?), Box::new(self.written(error)?)),
            (None, [item]) if ty.name == "List" => Ty::List(Box::new(self.written(item)?)),
            (None, [item]) if ty.name == "HashSet" => Ty::Set(Box::new(self.written(item)?)),
            (None, [key, value]) if ty.name == "HashMap" => Ty::Map(Box::new(self.written(key)?), Box::new(self.written(value)?)),
            (None, [params @ .., ret]) if ty.name == "Func" => {
                let params = params.iter().map(|param| self.written(param)).collect::<Res<Vec<Ty>>>()?;
                Ty::Func(params, Box::new(self.written(ret)?))
            }
            (Some(base @ (Ty::Int(_) | Ty::Bool | Ty::Void | Ty::Str | Ty::Formatter)), []) => base,
            (None, []) => match self.full(&ty.name, ty.span) {
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
            Ty::Str | Ty::Ptr(_) | Ty::List(_) | Ty::Map(..) | Ty::Set(_) => true,
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

    /// Whether a struct wrote this method itself.
    fn own_method(&self, owner: &str, method: &str) -> bool {
        self.structs[owner].methods.iter().any(|func| func.name == method)
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
            Ty::Int(_) => body.push_str("sg_put_int(out, (long long)*v);"),
            Ty::Bool => body.push_str("if (*v) sg_put(out, \"true\", 4); else sg_put(out, \"false\", 5);"),
            Ty::Str => body.push_str("sg_put_quoted(out, sg_view(v));"),
            // A `T*` and a `T*?` are one C type and share this helper, so it knows `null`.
            Ty::Ptr(inner) => {
                let _ = write!(body, "if (*v) {}(out, *v); else sg_put(out, \"null\", 4);", self.shower(inner, span)?);
            }
            Ty::Nullable(inner) if is_option(ty) => {
                let _ = write!(body, "if (v->has) {}(out, &v->value); else sg_put(out, \"null\", 4);", self.shower(inner, span)?);
            }
            Ty::Nullable(_) => unreachable!("a `T*?` has the name of the `T*`"),
            Ty::Struct(owner) if self.own_method(owner, "format") => {
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
            Ty::Int(_) | Ty::Bool => body.push_str("return *a == *b;"),
            Ty::Str => body.push_str("return sg_eq(sg_view(a), sg_view(b));"),
            Ty::Ptr(inner) => {
                let _ = write!(body, "return (*a == NULL) == (*b == NULL) && (*a == NULL || {}(*a, *b));", self.equaler(inner, span)?);
            }
            Ty::Nullable(inner) if is_option(ty) => {
                let _ = write!(body, "return a->has == b->has && (!a->has || {}(&a->value, &b->value));", self.equaler(inner, span)?);
            }
            Ty::Nullable(_) => unreachable!("a `T*?` has the name of the `T*`"),
            Ty::Struct(owner) if self.own_method(owner, "equals") => {
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
                    Ty::Int(_) | Ty::Bool => (held.clone(), "v.ptr[i] == item".to_string()),
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
            (_, Ty::Struct(owner)) if self.own_method(owner, "compare") => Ok(format!("(f_{}_compare({l}, {r}) {} 0)", cname(owner), op.symbol())),
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
            Ty::Int(_) | Ty::Bool => body.push_str("return sg_hash_word(h, (uint64_t)*v);"),
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
            Ty::Int(_) | Ty::Bool => self.value(arg),
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
            Ty::Int(_) | Ty::Bool => key_c.clone(),
            Ty::Str => "sg_str".to_string(),
            _ => format!("{key_c} const*"),
        };
        if what == "getOrAdd" {
            // The key is only lent; if it has to be kept, the table keeps a copy.
            let copy = match &**key {
                Ty::Int(_) | Ty::Bool => "key",
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
                    Ty::Int(_) | Ty::Bool => (held.clone(), "v->ptr[i] == item".to_string()),
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
                let compare = match &**item {
                    Ty::Int(_) => format!("{held} x = *(const {held}*)a, y = *(const {held}*)b; return (x > y) - (x < y);"),
                    Ty::Str => "return sg_cmp(sg_view((const sg_string*)a), sg_view((const sg_string*)b));".to_string(),
                    Ty::Struct(owner) if self.own_method(owner, "compare") => format!("return f_{}_compare(({held}*)a, ({held}*)b);", cname(owner)),
                    _ => return Err(not_yet(span, &format!("putting the items of a `{list}` in order"))),
                };
                let _ = writeln!(self.glue, "static int {helper}_compare(const void* a, const void* b) {{ {compare} }}");
                let _ = writeln!(
                    self.glue,
                    "static void {helper}({list_c}* v) {{ sg_sort(v->ptr, v->len, sizeof({held}), {helper}_compare); }}"
                );
                self.sigs.insert(helper.clone(), Vec::new());
            }
            _ => unreachable!("the helpers are named here"),
        }
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
        Ok(match self.written(&main.ret)? {
            ret @ Ty::Result(..) => {
                let Ty::Result(_, error) = &ret else { unreachable!("matched above") };
                let cause = "result.u.v1.m_cause";
                let ctype = self.ctype(&ret, main.ret.span)?;
                let mut drop = if self.droppable(error) { self.drop_of(error, cause) } else { String::new() };
                let mut shown = String::new();
                let (format, values) = match &**error {
                    Ty::Str => ("%.*s", format!("(int){cause}.len, (const char*){cause}.ptr")),
                    Ty::Int(_) => ("%lld", format!("(long long){cause}")),
                    Ty::Bool => ("%s", format!("{cause} ? \"true\" : \"false\"")),
                    // A struct or an enum is written by the helper of its type.
                    other => {
                        shown = format!("sg_string text = SG_LIT(\"\", 0); {}(&text, &{cause});\n        ", self.shower(other, main.ret.span)?);
                        drop = format!("sg_drop_string(&text); {drop}");
                        ("%.*s", "(int)text.len, (const char*)text.ptr".to_string())
                    }
                };
                format!(
                    "int main(void) {{\n    {ctype} result = f_main();\n    if (result.tag != 0) {{\n        fflush(stdout);\n        {shown}\
                     fprintf(stderr, \"error: {format}\\n\", {values});\n        {drop}\n        return 1;\n    }}\n    return 0;\n}}\n"
                )
            }
            _ => "int main(void) {\n    f_main();\n    return 0;\n}\n".to_string(),
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
        self.scopes = vec![Vec::new()];
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
        let mut scope = Vec::new();
        if let (Some(this), Some(owner)) = (func.this, owner) {
            let owner_ty = Ty::Struct(owner.to_string());
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
                let (place, held) = match rebound {
                    Some(variable) => variable,
                    None => self.slot(target)?,
                };
                let new = if matches!(value.kind, ExprKind::Match { .. }) {
                    let (ctype, temp) = (self.ctype(&held, value.span)?, self.temp());
                    let _ = writeln!(out, "{pad}{ctype} {temp};");
                    self.match_stmt(value, &Dest::Place(temp.clone(), held.clone()), depth, out)?;
                    temp
                } else {
                    self.head_value(value, &held)?
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
                        let Ty::Int(ty) = held else { return Err(not_yet(stmt.span, "this assignment")) };
                        let operation = self.arithmetic(*op, ty, &place, &new, stmt.span);
                        let _ = writeln!(out, "{pad}{place} = {operation};");
                    }
                }
                self.flush_post(&pad, out);
            }
            StmtKind::If { cond, then, els } => {
                let (mut cond, names) = self.condition(cond)?;
                self.flush_pre(&pad, out);
                let mut kept = None;
                if !self.post.is_empty() {
                    let temp = self.temp();
                    let _ = writeln!(out, "{pad}bool {temp} = {cond};");
                    // The names of `x is Circle(r)` point into the value: it stays until
                    // the `if` is done.
                    if names.is_empty() {
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
                self.release(kept, &pad, out);
            }
            StmtKind::While { cond, body } => {
                self.nothing_gone();
                let (cond, names) = self.condition(cond)?;
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
                    if names.is_empty() {
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
            }
            StmtKind::For { var, from, to, body, .. } => {
                // The ends are computed once; the counter stops before the end, so it cannot overflow.
                let ty = self.ctype(&from.ty, from.span)?;
                let end = self.temp();
                let (from_value, to_value) = (self.value(from)?, self.value(to)?);
                self.flush_pre(&pad, out);
                self.scopes.push(Vec::new());
                let cname = self.declare(var, from.ty.clone());
                let _ = writeln!(out, "{pad}{{");
                let _ = writeln!(out, "{pad}{ty} {cname} = {from_value}; {ty} {end} = {to_value};");
                self.flush_post(&pad, out);
                self.nothing_gone();
                self.loops.push(self.scopes.len());
                let _ = writeln!(out, "{pad}for (; {cname} < {end}; {cname}++) {{");
                self.block(body, depth + 1, out)?;
                self.loops.pop();
                self.scopes.pop();
                let _ = writeln!(out, "{pad}}}\n{pad}}}");
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
                        let _ = writeln!(out, "{inner}const {table_c}* {it} = {address};");
                        let _ = writeln!(out, "{inner}for (int64_t {at} = 0; {at} < {it}->len; {at}++) {{");
                        let ty = if *mode == Mode::Ref { Ty::Ref(Box::new(item)) } else { Ty::Read(Box::new(item)) };
                        let lent = self.borrowed_name(var, ty, &format!("{it}->entries[{at}]{at_item}"), iter.span)?;
                        self.loops.push(self.scopes.len());
                        self.block_with(body, depth + 2, out, vec![lent])?;
                        self.loops.pop();
                        let _ = writeln!(out, "{inner}}}");
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
                        let _ = writeln!(out, "{inner}for (; {at} < {it}.len; {at}++) {{");
                        self.temps += 1;
                        let cname = format!("v_{var}_{}", self.temps);
                        let line = format!("{} {cname} = {it}.entries[{at}]{at_item};", self.ctype(&item, iter.span)?);
                        let owned = Local { name: var.clone(), cname, ty: item, gone: false, drop: None };
                        self.loops.push(self.scopes.len());
                        self.block_with(body, depth + 2, out, vec![(owned, line)])?;
                        self.loops.pop();
                        self.scopes.pop();
                        let _ = writeln!(out, "{inner}}}\n{inner}{rest}\n{pad}}}");
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
                        let _ = writeln!(out, "{inner}{window_c} {it} = {window};");
                        let _ = writeln!(out, "{inner}for (int64_t {at} = 0; {at} < {it}.len; {at}++) {{");
                        let ty = if *mode == Mode::Ref { Ty::Ref(item.clone()) } else { Ty::Read(item.clone()) };
                        let lent = self.borrowed_name(var, ty, &format!("{it}.ptr[{at}]"), iter.span)?;
                        self.loops.push(self.scopes.len());
                        self.block_with(body, depth + 2, out, vec![lent])?;
                        self.loops.pop();
                        let _ = writeln!(out, "{inner}}}");
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
                        let _ = writeln!(out, "{inner}for (; {at} < {length}; {at}++) {{");
                        self.temps += 1;
                        let cname = format!("v_{var}_{}", self.temps);
                        let line = format!("{held} {cname} = {it}.items[{at}];");
                        let owned = Local { name: var.clone(), cname, ty: (**item).clone(), gone: false, drop: None };
                        self.loops.push(self.scopes.len());
                        self.block_with(body, depth + 2, out, vec![(owned, line)])?;
                        self.loops.pop();
                        self.scopes.pop();
                        let _ = writeln!(out, "{inner}}}");
                        if let Some(rest) = rest {
                            let _ = writeln!(out, "{inner}{rest}");
                        }
                        let _ = writeln!(out, "{pad}}}");
                    }
                }
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
                        let _ = writeln!(out, "{inner}const {list_c}* {it} = {address};");
                        let _ = writeln!(out, "{inner}for (int64_t {at} = 0; {at} < {it}->len; {at}++) {{");
                        let ty = if *mode == Mode::Ref { Ty::Ref(item.clone()) } else { Ty::Read(item.clone()) };
                        let lent = self.borrowed_name(var, ty, &format!("{it}->ptr[{at}]"), iter.span)?;
                        self.loops.push(self.scopes.len());
                        self.block_with(body, depth + 2, out, vec![lent])?;
                        self.loops.pop();
                        let _ = writeln!(out, "{inner}}}");
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
                        let _ = writeln!(out, "{inner}for (; {at} < {it}.len; {at}++) {{");
                        self.temps += 1;
                        let cname = format!("v_{var}_{}", self.temps);
                        let line = format!("{held} {cname} = {it}.ptr[{at}];");
                        let owned = Local { name: var.clone(), cname, ty: (**item).clone(), gone: false, drop: None };
                        self.loops.push(self.scopes.len());
                        self.block_with(body, depth + 2, out, vec![(owned, line)])?;
                        self.loops.pop();
                        self.scopes.pop();
                        let _ = writeln!(out, "{inner}}}\n{inner}{rest}\n{pad}}}");
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
                let word = if matches!(stmt.kind, StmtKind::Break) { "break" } else { "continue" };
                let _ = writeln!(out, "{pad}{word};");
            }
            StmtKind::Discard(value) => self.unused(value, &pad, out)?,
            StmtKind::Expr(expr) => self.expr_stmt(expr, depth, out)?,
        }
        Ok(())
    }

    /// An expression that stands as a statement.
    fn expr_stmt(&mut self, expr: &'a Expr, depth: usize, out: &mut String) -> Res<()> {
        let pad = "    ".repeat(depth);
        match &expr.kind {
            ExprKind::Call { name, args, callee: Callee::Plain, .. } if name == "print" => self.print(&args[0], &pad, out)?,
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
    /// `x is Circle(r)`.
    fn condition(&mut self, cond: &'a Expr) -> Res<(String, Vec<(Local, String)>)> {
        if !matches!(cond.kind, ExprKind::IsVariant { .. }) {
            return Ok((self.first(cond, true)?, Vec::new()));
        }
        self.variant_test(cond)
    }

    /// `x is Circle(r)`: a test on the tag. The names are borrows of what the variant carries.
    fn variant_test(&mut self, expr: &'a Expr) -> Res<(String, Vec<(Local, String)>)> {
        let ExprKind::IsVariant { expr: subject, negated, pattern, by_ref } = &expr.kind else {
            unreachable!("only called for an `is` test");
        };
        let PatternKind::Variant { index, args, .. } = &pattern.kind else {
            unreachable!("`is` only tests for a variant");
        };
        let mut place = self.look(subject)?;
        let mut ty = subject.ty.strip_borrow();
        while let Ty::Ptr(inner) = ty {
            place = format!("(*{place})");
            ty = inner;
        }
        let Some(variants) = self.variants(ty, subject.span)? else { return Err(not_yet(expr.span, &format!("`is` on `{ty}`"))) };
        let mut names = Vec::new();
        for (position, (name, _)) in args.iter().flatten().enumerate() {
            let Some(name) = name else { continue };
            let (field, held, span) = variants[*index][position].clone();
            let held = Box::new(held);
            let ty = if *by_ref { Ty::Ref(held) } else { Ty::Read(held) };
            let at = format!("{place}.u.v{index}.m_{field}");
            names.push(self.borrowed_name(name, ty, &at, span)?);
        }
        let op = if *negated { "!=" } else { "==" };
        Ok((format!("({place}.tag {op} {index})"), names))
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

        // What the arms test: a number, or the enum behind the borrow or the pointer.
        let mut core = subject.ty.strip_borrow();
        let mut boxes = 0;
        while let Ty::Ptr(behind) = core {
            (core, boxes) = (behind, boxes + 1);
        }
        let number = match core {
            Ty::Int(ty) => Some((if subject.ty.is_borrow() { format!("(*{held})") } else { held.clone() }, *ty)),
            _ => None,
        };
        let variants = self.variants(core, subject.span)?;
        if (number.is_none() && variants.is_none()) || (number.is_some() && boxes > 0) {
            return Err(not_yet(subject.span, &format!("a `match` on `{}`", subject.ty)));
        }
        // The enum as C reaches it: through a pointer in every case.
        let at = match (*borrowed, boxes) {
            (true, 0) | (false, 1) => held.clone(),
            (true, 1) => format!("(*{held})"),
            (false, 0) => format!("(&{held})"),
            _ => return Err(not_yet(subject.span, &format!("a `match` on `{}`", subject.ty))),
        };
        // `read` if the value is reached through a `read` anywhere, otherwise `ref`.
        let mode = match (*borrowed, &subject.ty) {
            (false, _) => None,
            (true, Ty::Ref(behind)) if !behind.is_borrow() => Some(Mode::Ref),
            (true, _) => Some(Mode::Read),
        };

        let guarded = arms.iter().any(|arm| arm.guard.is_some());
        let end = format!("{held}_end");
        for (position, arm) in arms.iter().enumerate() {
            let last = position + 1 == arms.len();
            let test = match (&arm.pattern.kind, &number) {
                (PatternKind::Variant { index, .. }, _) => Some(format!("{at}->tag == {index}")),
                (PatternKind::Int(wanted), Some((number, ty))) => Some(match *wanted {
                    i64::MIN => format!("{number} == (({})(-9223372036854775807LL - 1))", int_ctype(*ty)),
                    wanted => format!("{number} == (({}){wanted}LL)", int_ctype(*ty)),
                }),
                _ => None,
            };
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
                PatternKind::Bind(name) => {
                    let cname = self.declare(name, subject.ty.clone());
                    let _ = writeln!(out, "{body_pad}{ctype} {cname} = {held};");
                }
                PatternKind::Wildcard if !*borrowed && self.droppable(&subject.ty) => chosen.push(self.drop_of(&subject.ty, &held)),
                PatternKind::Variant { index, args, .. } => {
                    let fields = &variants.as_ref().expect("the checker allows a variant only for an enum")[*index];
                    for (place, (field, ty, span)) in fields.iter().cloned().enumerate() {
                        let value = format!("{at}->u.v{index}.m_{field}");
                        let name = args.as_ref().and_then(|args| args[place].0.as_ref());
                        match (name, mode) {
                            (Some(name), None) => {
                                let ctype = self.ctype(&ty, span)?;
                                let cname = self.declare(name, ty);
                                let _ = writeln!(out, "{body_pad}{ctype} {cname} = {value};");
                            }
                            (Some(name), Some(mode)) => {
                                let ty = if mode == Mode::Ref { Ty::Ref(Box::new(ty)) } else { Ty::Read(Box::new(ty)) };
                                let (local, line) = self.borrowed_name(name, ty, &value, span)?;
                                let _ = writeln!(out, "{body_pad}{line}");
                                self.scopes.last_mut().expect("an open scope").push(local);
                            }
                            (None, None) if self.droppable(&ty) => chosen.push(self.drop_of(&ty, &value)),
                            (None, _) => {}
                        }
                    }
                    if !*borrowed && boxes == 1 {
                        chosen.push(format!("free({held});"));
                    }
                }
                _ => {}
            }

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
            ExprKind::Int(..) | ExprKind::Bool(_) | ExprKind::Str(_) | ExprKind::Char(_) | ExprKind::Null | ExprKind::Var(_) | ExprKind::FuncRef(_) => Effect::Reads,
            ExprKind::Field { recv: inner, .. }
            | ExprKind::New(inner)
            | ExprKind::Load(inner)
            | ExprKind::Unbox(inner)
            | ExprKind::Widen(_, inner)
            | ExprKind::IsNull { expr: inner, .. }
            | ExprKind::IsVariant { expr: inner, .. }
            | ExprKind::Unary(UnOp::Not | UnOp::BitNot, inner) => of(inner),
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
                InterpPart::Expr(inner) => Some(inner),
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
                    Callee::Plain if table_behind(&recv.ty).is_some() && matches!(name.as_str(), "length" | "get" | "contains") => Effect::Reads,
                    Callee::Plain if matches!(recv.ty.strip_borrow(), Ty::Map(..) | Ty::Set(_) | Ty::Formatter) => Effect::Changes,
                    Callee::Plain if name == "clone" || name == "toString" => Effect::Acts,
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
                    InterpPart::Expr(expr) if !matches!(expr.kind, ExprKind::Str(_)) => Some(expr),
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
                        InterpPart::Expr(expr) if has_helper(&expr.ty) => {
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
                                done.push(Piece::Show(shower, at));
                            } else {
                                late.push(self.shown(&shower, &at, &mut format, &mut values));
                            }
                        }
                        InterpPart::Expr(expr) => {
                            let from = values.len();
                            self.formatted(expr, &mut format, &mut values)?;
                            // A number or a `bool`: one value. (A text is already kept before the line.)
                            let text = is_text(&expr.ty) || is_text_behind_pointer(&expr.ty);
                            let before_helpers = shown && self.effect(expr) != Effect::Reads;
                            if (first || before_helpers) && !text && values.len() == from + 1 {
                                let (ctype, temp) = (if values[from].ends_with("\"false\"") { "const char*" } else { "long long" }, self.temp());
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
                done.push(Piece::Show(shower, self.pointer(arg)?));
            }
            _ => self.formatted(arg, &mut format, &mut values)?,
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
        values.push(format!("(int){temp}n, (const char*)sg_scratch.ptr + {temp}"));
        format!("int64_t {temp} = sg_scratch.len; {shower}(&sg_scratch, {at}); int64_t {temp}n = sg_scratch.len - {temp};")
    }

    /// One value inside a formatted line.
    fn formatted(&mut self, expr: &'a Expr, format: &mut String, values: &mut Vec<String>) -> Res<()> {
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
                Ty::Int(_) => {
                    format.push_str("%lld");
                    values.push(format!("(long long)({place})"));
                }
                Ty::Bool => {
                    format.push_str("%s");
                    values.push(format!("({place}) ? \"true\" : \"false\""));
                }
                Ty::Str => {
                    format.push_str("%.*s");
                    values.push(format!("(int){place}.len, (const char*){place}.ptr"));
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
            values.push(format!("(int){temp}.len, (const char*){temp}.ptr"));
            return Ok(());
        }
        let value = self.value(expr)?;
        // What is printed may be a borrow of a number: `print(n)` with `read int n`.
        let value = if expr.ty.is_borrow() { format!("(*{value})") } else { value };
        match expr.ty.strip_borrow() {
            Ty::Int(_) => {
                format.push_str("%lld");
                values.push(format!("(long long)({value})"));
            }
            Ty::Bool => {
                format.push_str("%s");
                values.push(format!("({value}) ? \"true\" : \"false\""));
            }
            other => return Err(not_yet(expr.span, &format!("writing a `{other}` as text"))),
        }
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
                Ty::Int(_) => Some(("sg_assert_fail_i", "long long")),
                Ty::Bool => Some(("sg_assert_fail_b", "bool")),
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
                let (address, at) = (self.list_address(recv)?, self.value(index)?);
                let loc = self.loc(expr.span);
                let found = format!("sg_at_{name}({address}, {at}, {loc})");
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
                let at = self.value(index)?;
                parts.push(self.part(index, &index.ty, at, before));
                let (values, names) = self.in_order(parts)?;
                let loc = self.loc(expr.span);
                let found = sequence(&names, format!("sg_at_{name}({}, {}, {loc})", values[0], values[1]));
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
        let value = self.value(expr)?;
        if expr.ty.is_borrow() && !is_window(&expr.ty) {
            return Ok(format!("(*{value})"));
        }
        let (ctype, temp) = (self.ctype(&expr.ty, expr.span)?, self.temp());
        self.pre.push(format!("{ctype} {temp} = {value};"));
        if self.droppable(&expr.ty) {
            self.post.push(self.drop_of(&expr.ty, &temp));
        }
        Ok(temp)
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
            ExprKind::Int(value, ty) => Ok(match *value {
                i64::MIN => format!("(({})(-9223372036854775807LL - 1))", int_ctype(*ty)),
                value => format!("(({}){value}LL)", int_ctype(*ty)),
            }),
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
                    (false, Ty::Int(_) | Ty::Bool) => self.enclosed(before, fallback, &right.ty, right.span)?,
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
            ExprKind::Widen(ty, inner) => Ok(format!("(({}){})", int_ctype(*ty), self.first(inner, head)?)),
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
            ExprKind::Unary(UnOp::Neg, operand) => {
                let Ty::Int(ty) = &expr.ty else { return Err(not_yet(expr.span, "this operator")) };
                let operand = self.first(operand, head)?;
                if !self.checked {
                    return Ok(format!("sg_wsub_{0}(({1})0, {operand})", int_name(*ty), int_ctype(*ty)));
                }
                let loc = self.loc(expr.span);
                Ok(format!("sg_neg_{}({operand}, {loc})", int_name(*ty)))
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
                    (Ty::Int(_) | Ty::Bool, true) => Ok(sequence(&names, format!("({left} {} {right})", op.symbol()))),
                    (Ty::Int(ty), false) => Ok(sequence(&names, self.arithmetic(*op, *ty, left, right, expr.span))),
                    (other, _) => Err(not_yet(expr.span, &format!("`{}` on `{other}`", op.symbol()))),
                }
            }
            // A narrowing conversion follows the overflow rule: checked in a debug build, cut
            // to size in a release build.
            ExprKind::Cast { ty, expr: inner } => {
                let inner = self.first(inner, head)?;
                if !self.checked {
                    return Ok(format!("(({})({inner}))", int_ctype(*ty)));
                }
                let loc = self.loc(expr.span);
                Ok(format!("sg_cast_{}((int64_t)({inner}), {loc})", int_name(*ty)))
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
                let at = self.value(index)?;
                parts.push(self.part(index, &index.ty, at, before));
                let (values, names) = self.in_order(parts)?;
                let loc = self.loc(expr.span);
                Ok(sequence(&names, format!("sg_at({}, {}, {loc})", values[0], values[1])))
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
                for end in from.iter().chain(to) {
                    let before = self.mark();
                    let value = self.value(end)?;
                    parts.push(self.part(end, &end.ty, value, before));
                }
                let (values, names) = self.in_order(parts)?;
                let helper = match (from, to) {
                    (Some(_), Some(_)) => "cut",
                    (Some(_), None) => "after",
                    _ => "before",
                };
                let loc = self.loc(expr.span);
                Ok(sequence(&names, format!("sg_{helper}_{name}({}, {loc})", values.join(", "))))
            }
            ExprKind::Slice { recv, from, to } => {
                let text = self.view(recv)?;
                if from.is_none() && to.is_none() {
                    return Ok(text);
                }
                // The text is named once: its length is one of the ends.
                let temp = self.temp();
                self.pre.push(format!("sg_str {temp} = {text};"));
                let (from, to, names) = match (from, to) {
                    (Some(from), Some(to)) => {
                        let (values, names) = self.two(from, to, false)?;
                        (values[0].clone(), values[1].clone(), names)
                    }
                    (Some(from), None) => (self.value(from)?, format!("{temp}.len"), String::new()),
                    (None, Some(to)) => ("0".to_string(), self.value(to)?, String::new()),
                    (None, None) => unreachable!("handled above"),
                };
                let loc = self.loc(expr.span);
                Ok(sequence(&names, format!("sg_slice({temp}, {from}, {to}, {loc})")))
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
                Callee::Plain if matches!(name.as_str(), "print" | "assert") => Err(not_yet(expr.span, &format!("`{name}` used as a value"))),
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
                        _ => return Err(not_yet(expr.span, &format!("the method `{name}`"))),
                    };
                    let before = self.mark();
                    let address = self.list_address(recv)?;
                    let Ty::List(item) = &list else { unreachable!("it is a list") };
                    let by_pointer = matches!(name.as_str(), "indexOf" | "contains") && !matches!(**item, Ty::Int(_) | Ty::Bool | Ty::Str);
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
                        _ => return Err(not_yet(expr.span, &format!("the method `{name}`"))),
                    };
                    let before = self.mark();
                    let window = self.window(recv)?;
                    let by_pointer = matches!(name.as_str(), "indexOf" | "contains") && !matches!(**item, Ty::Int(_) | Ty::Bool | Ty::Str);
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
                        _ => Ok(format!("({called} >= 0)")),
                    }
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
                Callee::Plain if is_text(&recv.ty) || is_text_behind_pointer(&recv.ty) => match name.as_str() {
                    // The bytes of the text, looked at where they are.
                    "asBytes" => {
                        let window = self.ctype(&expr.ty, expr.span)?;
                        if self.made.insert("sg_bytes".to_string()) {
                            let _ = writeln!(self.glue, "static inline {window} sg_bytes(sg_str s) {{ return ({window}){{ (uint8_t*)s.ptr, s.len }}; }}");
                        }
                        Ok(format!("sg_bytes({})", self.view(recv)?))
                    }
                    // The position of the first such byte, or `null`.
                    "indexOf" => {
                        let (ctype, temp) = (self.ctype(&expr.ty, expr.span)?, self.temp());
                        let (text, byte) = (self.view(recv)?, self.value(&args[0])?);
                        Ok(format!(
                            "({{ sg_str {temp} = {text}; const uint8_t* {temp}p = {temp}.len ? memchr({temp}.ptr, {byte}, (size_t){temp}.len) : NULL; \
                             (({ctype}){{ {temp}p != NULL, {temp}p ? {temp}p - {temp}.ptr : 0 }}); }})"
                        ))
                    }
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
                    let lines: Vec<String> = self.pieces(&args[0], true)?.into_iter().map(|piece| match piece {
                        Piece::Text(format, values) if values.is_empty() => {
                            let text = format.replace("%%", "%");
                            format!("sg_put({out}, {}, {})", c_string(&text), text.len())
                        }
                        Piece::Text(format, values) => format!("sg_putf({out}, {}{values})", c_string(&format)),
                        Piece::Show(shower, at) => format!("{shower}({out}, {at})"),
                    }).collect();
                    match lines.len() {
                        1 => Ok(lines.concat()),
                        _ => Ok(format!("({{ {}; }})", lines.join("; "))),
                    }
                }
                Callee::Plain if matches!(name.as_str(), "equals" | "compare") && matches!(recv.ty.strip_borrow(), Ty::Int(_) | Ty::Bool) => {
                    let (values, names) = self.two(recv, &args[0], head)?;
                    let (ctype, temp) = (self.ctype(recv.ty.strip_borrow(), recv.span)?, self.temp());
                    Ok(sequence(&names, match name.as_str() {
                        "equals" => format!("({} == {})", values[0], values[1]),
                        _ => format!("({{ {ctype} {temp}a = {}, {temp}b = {}; (int32_t)(({temp}a > {temp}b) - ({temp}a < {temp}b)); }})", values[0], values[1]),
                    }))
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

/// A C expression that first runs the declarations `names` (the earlier parts, kept in
/// variables) and then gives `value`.
fn sequence(names: &str, value: String) -> String {
    if names.is_empty() { value } else { format!("({{ {names}{value}; }})") }
}

/// Whether the block ends with a statement that leaves it: what follows would never run.
fn ends_by_leaving(block: &Block) -> bool {
    matches!(block.last().map(|stmt| &stmt.kind), Some(StmtKind::Return(_) | StmtKind::Break | StmtKind::Continue))
}
