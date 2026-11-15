/* The same program written by hand in C: `arc4random_buf` on macOS and the BSDs, `getrandom` on
   Linux, as SG reads them. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#if !(defined(__APPLE__) || defined(__FreeBSD__) || defined(__OpenBSD__) || defined(__NetBSD__))
#include <sys/random.h>
#endif

static void fill(uint8_t* p, size_t n) {
#if defined(__APPLE__) || defined(__FreeBSD__) || defined(__OpenBSD__) || defined(__NetBSD__)
    arc4random_buf(p, n);
#else
    while (n > 0) {
        ssize_t got = getrandom(p, n, 0);
        if (got <= 0) abort();
        p += got;
        n -= (size_t)got;
    }
#endif
}

int main(void) {
    static uint8_t block[65536];
    int64_t filled = 0;
    for (int i = 0; i < 16384; i++) {
        fill(block, sizeof block);
        filled += (int64_t)sizeof block;
    }
    printf("%lld\n", (long long)filled);
    return 0;
}
