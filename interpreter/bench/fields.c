/* The same program written by hand in C: the lines and the fields are found with `memchr`,
   white space around a name is skipped by hand, each number is read with `strtoll` (which
   skips the spaces in front and stops at the ones after), and the characters of a name are
   counted as the bytes that do not continue a character. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static uint64_t next(uint64_t state) {
    return state * 6364136223846793005ULL + 1442695040888963407ULL;
}

static int space(unsigned char c) { return c == ' ' || (c >= '\t' && c <= '\r'); }

int main(void) {
    size_t cap = 1 << 25, len = 0;
    char* text = malloc(cap);
    uint64_t state = 11;
    for (int i = 0; i < 1000000; i++) {
        state = next(state);
        long long a = (long long)(state >> 54);
        long long b = (long long)((state >> 40) & 0x3fff) - 8192;
        long long c = (long long)((state >> 20) & 0xfffff);
        if (len + 96 > cap) text = realloc(text, cap *= 2);
        len += (size_t)sprintf(text + len, " %s%d , %lld, %lld ,%lld\r\n", i % 7 == 0 ? "Ren\xc3\xa9" "e" : "Ada", i % 1000, a, b, c);
    }

    long long total = 0, letters = 0, lines = 0;
    for (int pass = 0; pass < 20; pass++) {
        const char* p = text;
        const char* end = text + len;
        while (p < end) {
            const char* nl = memchr(p, '\n', (size_t)(end - p));
            const char* stop = nl ? nl : end;
            const char* line_end = stop > p && stop[-1] == '\r' ? stop - 1 : stop;
            const char* comma = memchr(p, ',', (size_t)(line_end - p));
            const char* name_end = comma ? comma : line_end;
            const char* s = p;
            while (s < name_end && space((unsigned char)*s)) s++;
            const char* e = name_end;
            while (e > s && space((unsigned char)e[-1])) e--;
            for (; s < e; s++) letters += ((unsigned char)*s & 0xC0) != 0x80;
            while (comma) {
                const char* field = comma + 1;
                comma = memchr(field, ',', (size_t)(line_end - field));
                total += strtoll(field, NULL, 10);
            }
            lines++;
            p = nl ? nl + 1 : end;
        }
    }
    printf("%lld\n%lld\n%lld\n", total, letters, lines);
    free(text);
    return 0;
}
