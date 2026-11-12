/* The same program written by hand in C, with no checks. */
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>

static bool is_prime(int32_t n) {
    if (n < 2) {
        return false;
    }
    int32_t d = 2;
    while (d * d <= n) {
        if (n % d == 0) {
            return false;
        }
        d += 1;
    }
    return true;
}

int main(void) {
    int32_t count = 0;
    for (int32_t n = 0; n < 5000000; n++) {
        if (is_prime(n)) {
            count += 1;
        }
    }
    printf("%d\n", count);
    return 0;
}
