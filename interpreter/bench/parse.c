/* The same program written by hand in C, with no checks: a function returns an error code
   and gives its value through a pointer. */
#include <stdint.h>
#include <stdio.h>
#include <string.h>

enum { OK, EMPTY, BAD_DIGIT, TOO_LARGE };

static int digit(uint8_t c, int32_t* out, uint8_t* found) {
    if (c < '0' || c > '9') {
        *found = c;
        return BAD_DIGIT;
    }
    *out = c - '0';
    return OK;
}

static int number(const char* text, int64_t len, int32_t* out, uint8_t* found) {
    if (len == 0) {
        return EMPTY;
    }
    int32_t value = 0;
    for (int64_t i = 0; i < len; i++) {
        if (value > 99999999) {
            return TOO_LARGE;
        }
        int32_t d;
        int error = digit((uint8_t)text[i], &d, found);
        if (error != OK) {
            return error;
        }
        value = (int32_t)((uint32_t)value * 10u + (uint32_t)d);
    }
    *out = value;
    return OK;
}

static int sum(const char* line, int64_t len, int32_t* out, uint8_t* found) {
    int32_t total = 0;
    int64_t start = 0;
    for (int64_t i = 0; i < len; i++) {
        if (line[i] == ' ') {
            int32_t n;
            int error = number(line + start, i - start, &n, found);
            if (error != OK) {
                return error;
            }
            total = (int32_t)((uint32_t)total + (uint32_t)n);
            start = i + 1;
        }
    }
    *out = total;
    return OK;
}

int main(void) {
    const char* good = "12 345 6789 0 42 100000 7 99 ";
    const char* bad = "12 345 67x9 0 42 ";
    const char* large = "12 1234567890123 ";
    int64_t good_len = (int64_t)strlen(good), bad_len = (int64_t)strlen(bad), large_len = (int64_t)strlen(large);
    int64_t total = 0;
    int32_t empty = 0;
    int32_t bad_digits = 0;
    for (int32_t round = 0; round < 6000000; round++) {
        int32_t skip = round % 3;
        int32_t value;
        uint8_t found = 0;
        if (sum(good + skip, good_len - skip, &value, &found) == OK) {
            total += value;
        } else {
            empty += 1;
        }
        int error = sum(bad + skip, bad_len - skip, &value, &found);
        if (error == OK) {
            total += value;
        } else if (error == BAD_DIGIT) {
            bad_digits += found - 'a';
        } else {
            empty += 1;
        }
        total += sum(large + skip, large_len - skip, &value, &found) == OK ? value : -1;
    }
    printf("%lld %d %d\n", (long long)total, empty, bad_digits);
    return 0;
}
