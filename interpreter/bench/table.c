/* The same table written by hand in C: one `printf` a line, with its widths, `%08llx`,
   `%06X` and `%.3f`; the binary digits are put together by a small function first, since
   `printf` has no conversion for them. */
#include <stdint.h>
#include <stdio.h>

static uint64_t next(uint64_t state) {
    return state * 6364136223846793005ULL + 1442695040888963407ULL;
}

static void binary(char* out, unsigned v, int digits) {
    for (int i = 0; i < digits; i++) out[digits - 1 - i] = (v >> i & 1) ? '1' : '0';
    out[digits] = 0;
}

int main(void) {
    const char* names[] = { "ada", "grace", "linus", "ken", "barbara", "dennis", "edsger" };
    uint64_t state = 5;
    char bits[17];
    for (long long i = 0; i < 2000000; i++) {
        state = next(state);
        long long a = (long long)((state >> 40) & 0xffff) - 32768;
        double x = (double)(state >> 44) / 1000.0;
        unsigned mask = (unsigned)(state >> 48);
        unsigned code = (unsigned)(state >> 8) & 0xffffff;
        binary(bits, mask, 16);
        printf("%08llx %10lld  %-8s|%10.3f  %s %06X\n", i, a, names[i % 7], x, bits, code);
    }
    return 0;
}
