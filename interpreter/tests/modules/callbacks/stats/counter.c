// How many times `scaled` ran.
#include <stdint.h>

static int32_t calls = 0;

static int32_t bump(void) {
    return ++calls;
}

static int32_t count(void) {
    return calls;
}
