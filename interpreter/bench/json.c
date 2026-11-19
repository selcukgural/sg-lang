/* link: -lcjson */
/* The same document read five times with cJSON 1.7 and walked once. cJSON keeps every number as
   a `double`: a whole one is told by its value, and the input writes every fractional one with a
   fraction that is not zero. */
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <cjson/cJSON.h>

typedef struct {
    int64_t values, strings, bytes, numbers, whole_sum;
} Counts;

static void walk(const cJSON *value, Counts *counts) {
    counts->values += 1;
    if (cJSON_IsString(value)) {
        counts->strings += 1;
        counts->bytes += (int64_t)strlen(value->valuestring);
    } else if (cJSON_IsNumber(value)) {
        counts->numbers += 1;
        double d = value->valuedouble;
        if (d == floor(d)) {
            counts->whole_sum += (int64_t)d;
        }
    } else if (cJSON_IsArray(value)) {
        for (const cJSON *item = value->child; item != NULL; item = item->next) {
            walk(item, counts);
        }
    } else if (cJSON_IsObject(value)) {
        for (const cJSON *member = value->child; member != NULL; member = member->next) {
            counts->strings += 1;
            counts->bytes += (int64_t)strlen(member->string);
            walk(member, counts);
        }
    }
}

int main(void) {
    size_t capacity = 1 << 20, length = 0;
    char *text = malloc(capacity);
    size_t got;
    while ((got = fread(text + length, 1, capacity - length, stdin)) > 0) {
        length += got;
        if (length == capacity) {
            capacity *= 2;
            text = realloc(text, capacity);
        }
    }
    Counts counts = {0, 0, 0, 0, 0};
    for (int round = 0; round < 5; round++) {
        cJSON *value = cJSON_ParseWithLength(text, length);
        if (value == NULL) {
            printf("not JSON\n");
            return 0;
        }
        if (round == 4) {
            walk(value, &counts);
        }
        cJSON_Delete(value);
    }
    printf("%lld values, %lld strings of %lld bytes, %lld numbers, whole ones add up to %lld\n", (long long)counts.values,
           (long long)counts.strings, (long long)counts.bytes, (long long)counts.numbers, (long long)counts.whole_sum);
    free(text);
    return 0;
}
