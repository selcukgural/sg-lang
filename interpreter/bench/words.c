/* The same program written by hand in C, with no checks. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int32_t matches(const char* text, int64_t len, const char* wanted, int64_t wanted_len) {
    int32_t count = 0;
    int64_t start = 0;
    for (int64_t i = 0; i < len; i++) {
        if (text[i] == ' ') {
            if (i - start == wanted_len && memcmp(text + start, wanted, (size_t)wanted_len) == 0) {
                count += 1;
            }
            start = i + 1;
        }
    }
    return count;
}

static int32_t matches_copied(const char* text, int64_t len, const char* wanted, int64_t wanted_len) {
    int32_t count = 0;
    int64_t start = 0;
    for (int64_t i = 0; i < len; i++) {
        if (text[i] == ' ') {
            int64_t word_len = i - start;
            char* word = malloc((size_t)word_len);
            memcpy(word, text + start, (size_t)word_len);
            if (word_len == wanted_len && memcmp(word, wanted, (size_t)wanted_len) == 0) {
                count += 1;
            }
            free(word);
            start = i + 1;
        }
    }
    return count;
}

int main(void) {
    const char* text = "the quick brown fox jumps over the lazy dog and then the cat sleeps ";
    int64_t len = (int64_t)strlen(text);
    int32_t found = 0;
    int32_t found_in_copies = 0;
    for (int32_t round = 0; round < 4000000; round++) {
        found += matches(text, len, "the", 3);
        found_in_copies += matches_copied(text, len, "the", 3);
    }
    printf("%d matches, %d in copies\n", found, found_in_copies);
    return 0;
}
