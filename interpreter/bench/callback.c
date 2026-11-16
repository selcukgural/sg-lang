// The same sort as `callback.sg`, with the comparator written in C.
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

static int compare_ints(const void* a, const void* b) {
    int32_t x = *(const int32_t*)a;
    int32_t y = *(const int32_t*)b;
    if (x < y) return -1;
    if (x > y) return 1;
    return 0;
}

int main(void) {
    int64_t count = 10000000;
    int32_t* items = calloc((size_t)count, sizeof(int32_t));
    if (!items) return 1;
    uint32_t state = 2463534242u;
    for (int64_t i = 0; i < count; i++) {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        items[i] = (int32_t)(state >> 1);
    }
    qsort(items, (size_t)count, 4, compare_ints);
    printf("%d %d %d\n", items[0], items[count / 2], items[count - 1]);
    free(items);
    return 0;
}
