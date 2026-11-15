/* The same program written by hand in C: `gmtime_r` takes each moment apart. */
#define _POSIX_C_SOURCE 200809L
#include <stdint.h>
#include <stdio.h>
#include <time.h>

int main(void) {
    uint64_t state = 7;
    int64_t sum = 0;
    for (int i = 0; i < 10000000; i++) {
        state = state * 6364136223846793005ULL + 1442695040888963407ULL;
        time_t seconds = (int64_t)(state >> 20) % 9467280000LL - 2208988800LL;
        struct tm parts;
        gmtime_r(&seconds, &parts);
        sum += parts.tm_year + 1900 + parts.tm_mon + 1 + parts.tm_mday + parts.tm_hour + parts.tm_min + parts.tm_sec;
        if (parts.tm_wday == 0) sum += 1;
    }
    printf("%lld\n", (long long)sum);
    return 0;
}
