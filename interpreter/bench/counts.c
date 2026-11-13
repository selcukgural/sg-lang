/* The same program written by hand in C, with no checks: an open-addressing table of the
   kind a C programmer writes, with a multiplicative hash that has no secret key. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct { int32_t key; int32_t value; } entry_t;
typedef struct { entry_t* entries; uint32_t* slots; int64_t len; int64_t cap; int64_t mask; } table_t;

static inline uint64_t hash(int32_t key) { return ((uint64_t)(uint32_t)key * 0x9E3779B97F4A7C15ULL) >> 32; }

static int64_t find(const table_t* t, int32_t key) {
    if (t->len == 0) return -1;
    uint64_t at = hash(key) & (uint64_t)t->mask;
    for (;;) {
        uint32_t slot = t->slots[at];
        if (slot == 0) return -1;
        if (t->entries[slot - 1].key == key) return (int64_t)slot - 1;
        at = (at + 1) & (uint64_t)t->mask;
    }
}

static void place(table_t* t, int64_t entry) {
    uint64_t at = hash(t->entries[entry].key) & (uint64_t)t->mask;
    while (t->slots[at] != 0) at = (at + 1) & (uint64_t)t->mask;
    t->slots[at] = (uint32_t)entry + 1;
}

static int32_t* get_or_add(table_t* t, int32_t key, int32_t value) {
    int64_t entry = find(t, key);
    if (entry >= 0) return &t->entries[entry].value;
    if (t->len == t->cap) {
        t->cap = t->cap ? t->cap * 2 : 4;
        t->entries = realloc(t->entries, (size_t)t->cap * sizeof(entry_t));
    }
    if (!t->slots || (t->len + 1) * 4 > (t->mask + 1) * 3) {
        int64_t size = t->slots ? (t->mask + 1) * 2 : 8;
        free(t->slots);
        t->slots = calloc((size_t)size, sizeof(uint32_t));
        t->mask = size - 1;
        for (int64_t i = 0; i < t->len; i++) place(t, i);
    }
    t->entries[t->len].key = key;
    t->entries[t->len].value = value;
    t->len++;
    place(t, t->len - 1);
    return &t->entries[t->len - 1].value;
}

static int32_t take(table_t* t, int32_t key) {
    int64_t entry = find(t, key);
    if (entry < 0) return 0;
    int32_t value = t->entries[entry].value;
    uint64_t mask = (uint64_t)t->mask;
    uint64_t hole = hash(key) & mask;
    while (t->slots[hole] != (uint32_t)entry + 1) hole = (hole + 1) & mask;
    for (uint64_t next = (hole + 1) & mask; t->slots[next] != 0; next = (next + 1) & mask) {
        uint64_t home = hash(t->entries[t->slots[next] - 1].key) & mask;
        if (((next - home) & mask) >= ((next - hole) & mask)) {
            t->slots[hole] = t->slots[next];
            hole = next;
        }
    }
    t->slots[hole] = 0;
    int64_t last = t->len - 1;
    if (entry != last) {
        uint64_t at = hash(t->entries[last].key) & mask;
        while (t->slots[at] != (uint32_t)last + 1) at = (at + 1) & mask;
        t->slots[at] = (uint32_t)entry + 1;
        t->entries[entry] = t->entries[last];
    }
    t->len = last;
    return value;
}

int main(void) {
    int64_t total = 0;
    for (int32_t round = 0; round < 20; round++) {
        table_t counts = { 0 };
        int64_t x = 12345 + round;
        for (int32_t i = 0; i < 1000000; i++) {
            x = (x * 1103515245 + 12345) % 2147483648;
            int32_t* count = get_or_add(&counts, (int32_t)(x % 50000), 0);
            *count += 1;
        }
        for (int64_t i = 0; i < counts.len; i++) {
            total += counts.entries[i].key % 7 * counts.entries[i].value;
        }
        for (int32_t key = 0; key < 50000; key++) {
            if (key % 3 == 0) {
                total += take(&counts, key);
            }
        }
        total += counts.len;
        free(counts.entries);
        free(counts.slots);
    }
    printf("%lld\n", (long long)total);
    return 0;
}
