/* What `tests/roots/roots.sg update` does, with a stream of C opened with "r+": the file written
   and read at places a seed gives. The two say the same sum and leave the same file. */
#include <stdio.h>
#include <stdlib.h>

int main(int argc, char** argv) {
    if (argc < 4) return 2;
    FILE* f = fopen(argv[1], "r+b");
    if (!f) return 1;
    long long seed = atoll(argv[2]), ops = atoll(argv[3]);
    fseek(f, 0, SEEK_END);
    long long size = ftell(f);
    fseek(f, 0, SEEK_SET);
    long long state = seed, sum = 0;
    unsigned char data[300], buffer[300];
    for (long long i = 0; i < ops; i++) {
        state = (state * 1103515245 + 12345) % 2147483648LL;
        long long kind = state / 65536 % 4;
        state = (state * 1103515245 + 12345) % 2147483648LL;
        long long place = state % (size + 1);
        state = (state * 1103515245 + 12345) % 2147483648LL;
        long long n = state / 65536 % 300 + 1;
        if (kind == 2) {
            fseek(f, (long)place, SEEK_SET);
            long long got = (long long)fread(buffer, 1, (size_t)n, f);
            for (long long j = 0; j < got; j++) sum = (sum * 31 + buffer[j]) % 1000000007LL;
            sum = (sum * 31 + got) % 1000000007LL;
        } else {
            for (long long j = 0; j < n; j++) data[j] = (unsigned char)((i + j) % 251);
            if (kind == 3) {
                fseek(f, 0, SEEK_END);
                place = ftell(f);
            } else {
                fseek(f, (long)place, SEEK_SET);
            }
            fwrite(data, 1, (size_t)n, f);
            if (place + n > size) size = place + n;
        }
    }
    fclose(f);
    printf("sum %lld size %lld\n", sum, size);
    return 0;
}
