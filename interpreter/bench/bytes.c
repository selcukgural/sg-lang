/* The same program written by hand in C: `fread` into a buffer of 4 KB and `fwrite` of each
   piece into a `FILE*` opened to append, after the file is emptied; both `FILE*`s have a
   buffer of 64 KB. */
#include <stdio.h>

int main(void) {
    FILE* empty = fopen("target/bench/bytes-c.out", "wb");
    if (!empty || fclose(empty) != 0) { perror("fopen"); return 1; }
    FILE* in = fopen("target/bench/lines.in", "rb");
    FILE* out = fopen("target/bench/bytes-c.out", "ab");
    if (!in || !out) { perror("fopen"); return 1; }
    setvbuf(in, NULL, _IOFBF, 65536);
    setvbuf(out, NULL, _IOFBF, 65536);
    unsigned char buffer[4096];
    long long total = 0;
    size_t got;
    while ((got = fread(buffer, 1, sizeof buffer, in)) > 0) {
        if (fwrite(buffer, 1, got, out) != got) { perror("fwrite"); return 1; }
        total += (long long)got;
    }
    if (ferror(in)) { perror("fread"); return 1; }
    fclose(in);
    if (fclose(out) != 0) { perror("fclose"); return 1; }
    printf("%lld bytes copied\n", total);
    return 0;
}
