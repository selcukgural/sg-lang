/* The same program written by hand in C as for `lines`, since C has no async read to compare
   with: `getline` into one buffer that is used again for every line, the `\n` and a `\r`
   before it cut off. The lines are not checked to be UTF-8. */
#define _POSIX_C_SOURCE 200809L
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int main(void) {
    char* line = NULL;
    size_t cap = 0;
    long long lines = 0, bytes = 0, longest = 0, errors = 0;
    ssize_t got;
    while ((got = getline(&line, &cap, stdin)) >= 0) {
        long long length = got;
        if (length > 0 && line[length - 1] == '\n') {
            length--;
            if (length > 0 && line[length - 1] == '\r') length--;
        }
        lines++;
        bytes += length;
        if (length > longest) longest = length;
        if (length >= 5 && memcmp(line, "ERROR", 5) == 0) errors++;
    }
    free(line);
    printf("%lld lines, %lld bytes, longest %lld, %lld errors\n", lines, bytes, longest, errors);
    return 0;
}
