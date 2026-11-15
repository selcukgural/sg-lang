/* The same program written by hand in C: `stat`. */
#include <stdio.h>
#include <sys/stat.h>

int main(void) {
    long long size = 0;
    for (int i = 0; i < 1000000; i++) {
        struct stat info;
        if (stat("target/bench/listing/file-04242.txt", &info) != 0) return 1;
        size += (long long)info.st_size;
    }
    printf("%lld\n", size);
    return 0;
}
