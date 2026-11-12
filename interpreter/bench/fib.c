/* The same program written by hand in C, with no checks. */
#include <stdint.h>
#include <stdio.h>

static int64_t fib(int32_t n) {
    if (n < 2) {
        return n;
    }
    return fib(n - 1) + fib(n - 2);
}

int main(void) {
    printf("%lld\n", (long long)fib(40));
    return 0;
}
