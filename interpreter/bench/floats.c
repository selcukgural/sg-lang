/* The same program written by hand in C: each number is written with `%.17g`, which always
   gives seventeen digits (the fewest that read back is not something `printf` can do), and read
   back with `strtod`. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

static uint64_t next(uint64_t state) {
    return state * 6364136223846793005ULL + 1442695040888963407ULL;
}

int main(void) {
    size_t cap = 1 << 25, len = 0;
    char* text = malloc(cap);
    for (int pass = 0; pass < 3; pass++) {
        len = 0;
        uint64_t state = 11;
        for (int i = 0; i < 1000000; i++) {
            state = next(state);
            double x = (double)(state >> 44) / 1000.0;
            if (i % 2 == 0) x = (double)(state >> 11) / 9007199254740992.0 * 1000.0;
            if (len + 64 > cap) text = realloc(text, cap *= 2);
            len += (size_t)sprintf(text + len, "%.17g\n", x);
        }
    }
    text[len] = 0;

    double sum = 0;
    int count = 0;
    char* p = text;
    while (*p) {
        char* end;
        sum += strtod(p, &end);
        p = end + 1;
        count++;
    }
    printf("%d\n%lld\n", count, (long long)(sum * 1000));
    free(text);
    return 0;
}
