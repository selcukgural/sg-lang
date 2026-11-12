/* The same program written by hand in C, with no checks: an array that doubles when it is full. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

typedef struct { int32_t* ptr; int64_t len; int64_t cap; } list_t;

static void add(list_t* xs, int32_t item) {
    if (xs->len == xs->cap) {
        xs->cap = xs->cap ? xs->cap * 2 : 4;
        xs->ptr = realloc(xs->ptr, (size_t)xs->cap * sizeof(int32_t));
        if (!xs->ptr) abort();
    }
    xs->ptr[xs->len++] = item;
}

static int64_t total(const list_t* xs) {
    int64_t sum = 0;
    for (int64_t i = 0; i < xs->len; i++) {
        sum += xs->ptr[i];
    }
    return sum;
}

int main(void) {
    int64_t sum = 0;
    for (int32_t round = 0; round < 600; round++) {
        list_t xs = { 0 };
        for (int32_t i = 0; i < 1000000; i++) {
            add(&xs, i % 1000);
        }
        for (int64_t i = 0; i < xs.len; i++) {
            xs.ptr[i] = xs.ptr[i] * 3 + 1;
        }
        sum += total(&xs);
        for (int64_t i = 0, j = xs.len - 1; i < j; i++, j--) {
            int32_t item = xs.ptr[i];
            xs.ptr[i] = xs.ptr[j];
            xs.ptr[j] = item;
        }
        sum += xs.ptr[round];
        free(xs.ptr);
    }
    printf("%lld\n", (long long)sum);
    return 0;
}
