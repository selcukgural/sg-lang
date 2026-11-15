/* The same program written by hand in C: a ring buffer over memory from `calloc`. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

typedef struct { int64_t* items; int64_t head; int64_t count; int64_t mask; } ring_t;

static void push(ring_t* ring, int64_t value) {
    ring->items[(ring->head + ring->count) & ring->mask] = value;
    ring->count += 1;
}

static int64_t pop(ring_t* ring) {
    int64_t value = ring->items[ring->head];
    ring->head = (ring->head + 1) & ring->mask;
    ring->count -= 1;
    return value;
}

int main(void) {
    ring_t ring = { calloc(4096, sizeof(int64_t)), 0, 0, 4095 };
    if (!ring.items) abort();
    int64_t sum = 0;
    for (int32_t round = 0; round < 100000; round++) {
        for (int32_t i = 0; i < 4000; i++) {
            push(&ring, (int64_t)i * round);
        }
        for (int32_t i = 0; i < 4000; i++) {
            sum += pop(&ring) ^ i;
        }
    }
    free(ring.items);
    printf("%lld\n", (long long)sum);
    return 0;
}
