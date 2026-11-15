/* The same program written by hand in C: `readdir`, each name copied, sorted with `qsort` by its
   bytes, and freed. */
#define _DEFAULT_SOURCE
#include <dirent.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct { char* name; size_t len; unsigned char kind; } entry;

static int order(const void* x, const void* y) {
    const entry* a = x;
    const entry* b = y;
    size_t n = a->len < b->len ? a->len : b->len;
    int c = memcmp(a->name, b->name, n);
    return c ? c : (a->len > b->len) - (a->len < b->len);
}

int main(void) {
    long long bytes = 0, files = 0;
    for (int i = 0; i < 100; i++) {
        DIR* folder = opendir("target/bench/listing");
        if (!folder) return 1;
        entry* entries = NULL;
        size_t n = 0, cap = 0;
        struct dirent* d;
        while ((d = readdir(folder))) {
            if (d->d_name[0] == '.' && (d->d_name[1] == 0 || (d->d_name[1] == '.' && d->d_name[2] == 0))) continue;
            if (n == cap) { cap = cap ? cap * 2 : 4; entries = realloc(entries, cap * sizeof *entries); }
            size_t len = strlen(d->d_name);
            char* name = malloc(len + 1);
            memcpy(name, d->d_name, len + 1);
            entries[n++] = (entry){ name, len, d->d_type };
        }
        closedir(folder);
        qsort(entries, n, sizeof *entries, order);
        for (size_t j = 0; j < n; j++) {
            bytes += (long long)entries[j].len;
            if (entries[j].kind == DT_REG) files += 1;
            free(entries[j].name);
        }
        free(entries);
    }
    printf("%lld %lld\n", files, bytes);
    return 0;
}
