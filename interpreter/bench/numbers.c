/* The same program written by hand in C: the text is built the same way, and each number is
   read with `strtoll` or `strtod`, which also find where it ends. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

static uint64_t next(uint64_t state) {
    return state * 6364136223846793005ULL + 1442695040888963407ULL;
}

int main(void) {
    size_t cap = 1 << 24, len = 0;
    char* text = malloc(cap);
    uint64_t state = 7;
    for (int i = 0; i < 1000000; i++) {
        state = next(state);
        if (len + 64 > cap) text = realloc(text, cap *= 2);
        if (i % 2 == 0) {
            long long whole = (long long)(state >> 33) - 1073741824;
            len += (size_t)sprintf(text + len, "%lld\n", whole);
        } else {
            /* A fractional number, written from two whole numbers: `12.1345`. */
            uint64_t k = state >> 44;
            len += (size_t)sprintf(text + len, "%llu.%llu\n", (unsigned long long)(k / 1000), (unsigned long long)(k % 1000 + 1000));
        }
    }
    text[len] = 0;

    long long total = 0;
    double sum = 0;
    for (int pass = 0; pass < 10; pass++) {
        char* p = text;
        int line = 0;
        while (*p) {
            char* end;
            if (line % 2 == 0) total += strtoll(p, &end, 10);
            else sum += strtod(p, &end);
            p = end + 1;
            line++;
        }
    }
    printf("%lld\n%lld\n", total, (long long)sum);
    free(text);
    return 0;
}
