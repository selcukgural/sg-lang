/* The same program written by hand in C: `readdir`, each name copied, sorted with `qsort` by its
   bytes, and freed. On Windows `FindFirstFileExW` and `FindNextFileW` (as SG lists a folder there),
   each name turned into UTF-8. */
#define _DEFAULT_SOURCE
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#if defined(_WIN32)
#include <windows.h>
#else
#include <dirent.h>
#endif

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
#if defined(_WIN32)
        WIN32_FIND_DATAW d;
        HANDLE folder = FindFirstFileExW(L"target\\bench\\listing\\*", FindExInfoBasic, &d, FindExSearchNameMatch, NULL, FIND_FIRST_EX_LARGE_FETCH);
        if (folder == INVALID_HANDLE_VALUE) return 1;
        entry* entries = NULL;
        size_t n = 0, cap = 0;
        do {
            const wchar_t* w = d.cFileName;
            if (w[0] == L'.' && (w[1] == 0 || (w[1] == L'.' && w[2] == 0))) continue;
            if (n == cap) { cap = cap ? cap * 2 : 4; entries = realloc(entries, cap * sizeof *entries); }
            int len = WideCharToMultiByte(CP_UTF8, 0, w, -1, NULL, 0, NULL, NULL);
            char* name = malloc((size_t)len);
            WideCharToMultiByte(CP_UTF8, 0, w, -1, name, len, NULL, NULL);
            unsigned char plain = !(d.dwFileAttributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT));
            entries[n++] = (entry){ name, (size_t)len - 1, plain };
        } while (FindNextFileW(folder, &d));
        FindClose(folder);
        const unsigned char regular = 1;
#else
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
        const unsigned char regular = DT_REG;
#endif
        qsort(entries, n, sizeof *entries, order);
        for (size_t j = 0; j < n; j++) {
            bytes += (long long)entries[j].len;
            if (entries[j].kind == regular) files += 1;
            free(entries[j].name);
        }
        free(entries);
    }
    printf("%lld %lld\n", files, bytes);
    return 0;
}
