#define _POSIX_C_SOURCE 200809L
/* The same program written by hand in C: `popen` of `cat` and `getline`, which does not check that
   a line is text. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#if defined(_WIN32)
#include "windows/posix.h"
#define READ "rb"
#else
#include <sys/wait.h>
#define READ "r"
#endif

int main(void) {
    FILE* in = popen("cat target/bench/lines.in", READ);
    if (!in) return 1;
    char* line = NULL;
    size_t cap = 0;
    ssize_t got;
    long lines = 0, bytes = 0, errors = 0;
    while ((got = getline(&line, &cap, in)) >= 0) {
        if (got > 0 && line[got - 1] == '\n') got--;
        if (got > 0 && line[got - 1] == '\r') got--;
        lines++;
        bytes += got;
        if (got >= 5 && memcmp(line, "ERROR", 5) == 0) errors++;
    }
    free(line);
    int status = pclose(in);
    printf("%ld lines, %ld bytes, %ld errors, exited with code %d\n", lines, bytes, errors, WEXITSTATUS(status));
    return 0;
}
