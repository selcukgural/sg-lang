/* The same program written by hand in C as for `copy`, since C has no async write to compare
   with: `getline` into one buffer that is used again for every line, the `\n` and a `\r`
   before it cut off, and `fprintf` into a `FILE*` with a buffer of 64 KB. The lines are not
   checked to be UTF-8. */
#define _POSIX_C_SOURCE 200809L
#include <stdio.h>
#include <stdlib.h>
#if defined(_WIN32)
#include "windows/posix.h"
#endif

int main(void) {
    FILE* in = fopen("target/bench/lines.in", "rb");
    FILE* out = fopen("target/bench/asynccopy-c.out", "wb");
    if (!in || !out) { perror("fopen"); return 1; }
    setvbuf(out, NULL, _IOFBF, 65536);
    char* line = NULL;
    size_t cap = 0;
    long long lines = 0;
    ssize_t got;
    while ((got = getline(&line, &cap, in)) >= 0) {
        if (got > 0 && line[got - 1] == '\n') {
            got--;
            if (got > 0 && line[got - 1] == '\r') got--;
        }
        line[got] = 0;
        lines++;
        if (fprintf(out, "%lld: %s\n", lines, line) < 0) { perror("fprintf"); return 1; }
    }
    free(line);
    fclose(in);
    if (fclose(out) != 0) { perror("fclose"); return 1; }
    printf("%lld lines copied\n", lines);
    return 0;
}
