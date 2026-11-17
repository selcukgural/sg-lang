/* The same program written by hand in C: `gmtime_r` takes each moment apart. Windows' C library
   takes no moment before 1970 (`gmtime_s`), so there the days are turned into a date by hand, as
   Howard Hinnant's `civil_from_days` does. */
#define _POSIX_C_SOURCE 200809L
#include <stdint.h>
#include <stdio.h>
#include <time.h>
#if defined(_WIN32)
static void gmtime_r(const time_t* moment, struct tm* parts) {
    int64_t t = *moment;
    int64_t days = t / 86400, rest = t % 86400;
    if (rest < 0) { rest += 86400; days--; }
    parts->tm_hour = (int)(rest / 3600);
    parts->tm_min = (int)(rest / 60 % 60);
    parts->tm_sec = (int)(rest % 60);
    parts->tm_wday = (int)((days % 7 + 11) % 7);
    int64_t z = days + 719468;
    int64_t era = (z >= 0 ? z : z - 146096) / 146097;
    int64_t doe = z - era * 146097;
    int64_t yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    int64_t doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    int64_t mp = (5 * doy + 2) / 153;
    int64_t d = doy - (153 * mp + 2) / 5 + 1;
    int64_t m = mp < 10 ? mp + 3 : mp - 9;
    int64_t y = yoe + era * 400 + (m <= 2);
    parts->tm_year = (int)(y - 1900);
    parts->tm_mon = (int)(m - 1);
    parts->tm_mday = (int)d;
}
#endif

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
