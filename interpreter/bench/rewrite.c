// The same as rewrite.sg, written by hand: `memchr` finds the first byte of each place and
// `memcmp` the rest, the pieces are copied into a block that doubles when it is full, and an
// indent is copied into a block of its size. With `memmem` in place of `find` this program
// took 0.70 s on macOS against 0.63 s, so the search written out is the one compared.
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct { char* ptr; int64_t len; int64_t cap; } text;

static void put(text* out, const char* bytes, int64_t n) {
    if (n <= 0) return;
    if (out->len + n > out->cap) {
        int64_t cap = out->cap ? out->cap * 2 : 32;
        while (cap < out->len + n) cap *= 2;
        out->ptr = realloc(out->ptr, (size_t)cap);
        if (!out->ptr) abort();
        out->cap = cap;
    }
    memcpy(out->ptr + out->len, bytes, (size_t)n);
    out->len += n;
}

static const char* find(const char* h, size_t hl, const char* n, size_t nl) {
    if (nl > hl) return NULL;
    const char* end = h + (hl - nl + 1);
    for (const char* p = h; p < end; p++) {
        p = memchr(p, n[0], (size_t)(end - p));
        if (!p) return NULL;
        if (memcmp(p, n, nl) == 0) return p;
    }
    return NULL;
}
static text replace_all(const char* s, int64_t len, const char* old, int64_t old_len, const char* with, int64_t with_len) {
    text out = { malloc((size_t)len), 0, len };
    if (!out.ptr) abort();
    int64_t at = 0;
    for (;;) {
        const char* found = find(s + at, (size_t)(len - at), old, (size_t)old_len);
        if (!found) break;
        put(&out, s + at, found - (s + at));
        put(&out, with, with_len);
        at = (found - s) + old_len;
    }
    put(&out, s + at, len - at);
    return out;
}

static text repeat(const char* s, int64_t len, int64_t n) {
    if (n == 0) return (text){ NULL, 0, 0 };
    int64_t size = len * n;
    text out = { malloc((size_t)size), size, size };
    if (!out.ptr) abort();
    for (int64_t i = 0; i < n; i++) memcpy(out.ptr + i * len, s, (size_t)len);
    return out;
}

int main(void) {
    static const char* table[] = {
        "id\tname\tcount\tprice\tcreated at\tupdated at\towner\tstatus",
        "1042\tnamed range\t17\t3.50\t2026-11-14\t2026-11-14\tada\tactive",
        "no tabs in this line at all, only a name somewhere in the middle of it",
        "\t\t\tleading tabs before the name and trailing ones after it\t\t",
        "short\tline",
    };
    int64_t count = 200000;
    text* lines = malloc(sizeof(text) * (size_t)count);
    int64_t seed = 7;
    for (int64_t i = 0; i < count; i++) {
        seed = (seed * 1103515245 + 12345) % 2147483648;
        const char* t = table[seed % 5];
        int64_t n = (int64_t)strlen(t);
        lines[i] = (text){ malloc((size_t)n), n, n };
        memcpy(lines[i].ptr, t, (size_t)n);
    }
    int64_t total = 0;
    for (int64_t round = 0; round < 20; round++) {
        for (int64_t i = 0; i < count; i++) {
            text wide = replace_all(lines[i].ptr, lines[i].len, "\t", 1, "    ", 4);
            text named = replace_all(wide.ptr, wide.len, "name", 4, "title", 5);
            text indent = repeat("  ", 2, round % 8);
            total += named.len + indent.len + round;
            free(wide.ptr);
            free(named.ptr);
            free(indent.ptr);
        }
    }
    printf("%lld\n", (long long)total);
    for (int64_t i = 0; i < count; i++) free(lines[i].ptr);
    free(lines);
    return 0;
}
