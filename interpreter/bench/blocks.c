/* The same program written by hand in C: a block that doubles when it is full, and a
   `memcpy` of each piece to its end. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct { uint8_t* ptr; int64_t len; int64_t cap; } Buffer;

static void add_all(Buffer* b, const uint8_t* items, int64_t count) {
    if (count > b->cap - b->len) {
        int64_t room = b->cap ? b->cap * 2 : 4;
        if (room < b->len + count) room = b->len + count;
        b->ptr = realloc(b->ptr, (size_t)room);
        if (!b->ptr) abort();
        b->cap = room;
    }
    memcpy(b->ptr + b->len, items, (size_t)count);
    b->len += count;
}

int main(void) {
    uint8_t block[255];
    for (int i = 0; i < 255; i++) block[i] = (uint8_t)i;
    Buffer buffer = { 0 };
    long long total = 0, check = 0;
    for (long long round = 0; round < 8000000; round++) {
        int64_t size = 1 + round % 255;
        block[0] = (uint8_t)(round & 0xff);
        add_all(&buffer, block, size);
        if (buffer.len > (64 << 20)) {
            total += buffer.len;
            check += buffer.ptr[buffer.len / 2];
            buffer.len = 0;
        }
    }
    total += buffer.len;
    printf("%lld bytes, check %lld\n", total, check);
    free(buffer.ptr);
    return 0;
}
