// The same as lookup.sg, written by hand.
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

typedef struct {
    int64_t id, a, b, c, d, e, f, g, h, i, j, k, l, m, n, weight;
} Record;

typedef struct {
    Record* ptr;
    int64_t len, cap;
} Records;

static void add(Records* records, Record record) {
    if (records->len == records->cap) {
        records->cap = records->cap ? records->cap * 2 : 4;
        records->ptr = realloc(records->ptr, (size_t)records->cap * sizeof(Record));
        if (!records->ptr) abort();
    }
    records->ptr[records->len++] = record;
}

static const Record* find(const Records* records, int64_t id) {
    if ((uint64_t)id >= (uint64_t)records->len) return NULL;
    return &records->ptr[id];
}

int main(void) {
    Records records = { NULL, 0, 0 };
    for (int32_t id = 0; id < 1000; id++) {
        int64_t x = id;
        add(&records, (Record){ x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x * 3 + 1 });
    }
    int64_t sum = 0, missing = 0, key = 0;
    for (int32_t round = 0; round < 300000000; round++) {
        key = (key * 5 + 7) % 1024;
        const Record* record = find(&records, key);
        if (record != NULL) {
            sum += record->weight + record->id;
        } else {
            missing += 1;
        }
    }
    printf("%lld\n%lld\n", (long long)sum, (long long)missing);
    free(records.ptr);
    return 0;
}
