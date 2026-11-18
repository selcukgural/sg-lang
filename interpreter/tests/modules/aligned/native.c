// The C side of the `aligned` program: a `Slot*` lies at a multiple of 64, as its type says.
#include <stdio.h>
#include <stdlib.h>

static int64_t at64(const void* given) {
    return (int64_t)((uintptr_t)given % 64);
}

static int64_t offBy(const void* s) {
    return at64(s);
}

static int64_t offByItems(void* items, int64_t count) {
    (void)count;
    return at64(items);
}

static int64_t bump(void* given) {
    // What C may do with a `Slot*`: the compiler is told its address is a multiple of 64.
    if (at64(given) != 0) {
        fprintf(stderr, "native.c: a Slot* at %p is not aligned to 64\n", given);
        abort();
    }
    int64_t* count = given;
    *count += 1;
    return *count;
}
