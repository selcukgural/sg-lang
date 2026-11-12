/* The same program written by hand in C, with no checks. */
#include <stdint.h>
#include <stdio.h>

static int32_t steps(int64_t start) {
    int64_t n = start;
    int32_t count = 0;
    while (n != 1) {
        if (n % 2 == 0) {
            n = n / 2;
        } else {
            n = 3 * n + 1;
        }
        count += 1;
    }
    return count;
}

int main(void) {
    int32_t best = 0;
    int64_t best_start = 0;
    for (int32_t start = 1; start < 3000000; start++) {
        int32_t length = steps(start);
        if (length > best) {
            best = length;
            best_start = start;
        }
    }
    printf("%lld: %d steps\n", (long long)best_start, best);
    return 0;
}
