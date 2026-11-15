/* The same program written by hand in C: `clock_gettime` on the monotonic clock. */
#define _POSIX_C_SOURCE 200809L
#include <stdint.h>
#include <stdio.h>
#include <time.h>

static int64_t now(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return (int64_t)t.tv_sec * 1000000000 + t.tv_nsec;
}

int main(void) {
    int64_t start = now();
    int64_t before = 0;
    int64_t steady = 0;
    for (int i = 0; i < 10000000; i++) {
        int64_t gone = now() - start;
        if (gone >= before) steady += 1;
        before = gone;
    }
    printf("%lld\n", (long long)steady);
    return 0;
}
