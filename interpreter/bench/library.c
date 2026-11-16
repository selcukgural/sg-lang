// The library of `library.sg`, written in C.
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

typedef struct Stats {
    char *name;
    int64_t count;
    double sum, low, high;
} Stats;

Stats *stats_new(const char *name) {
    Stats *stats = malloc(sizeof *stats);
    if (!stats) abort();
    size_t length = strlen(name) + 1;
    stats->name = malloc(length);
    if (!stats->name) abort();
    memcpy(stats->name, name, length);
    stats->count = 0;
    stats->sum = 0;
    stats->low = 1.0e300;
    stats->high = -1.0e300;
    return stats;
}

void stats_add(Stats *stats, double x) {
    stats->count += 1;
    stats->sum += x;
    if (x < stats->low) stats->low = x;
    if (x > stats->high) stats->high = x;
}

double stats_mean(const Stats *stats) { return stats->sum / (double)stats->count; }

double stats_spread(const Stats *stats) { return stats->high - stats->low; }

void stats_free(Stats *stats) {
    if (!stats) return;
    free(stats->name);
    free(stats);
}

uint64_t fnv(uint8_t *data, uint64_t length) {
    uint64_t hash = 14695981039346656037ULL;
    for (uint64_t i = 0; i < length; i++) hash = (hash ^ data[i]) * 1099511628211ULL;
    return hash;
}
