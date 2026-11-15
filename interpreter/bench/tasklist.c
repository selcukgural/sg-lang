/* The same program written by hand in C, without async: each length is a plain call whose
   result is put in a block that is allocated, kept in an array for the list, and freed when it
   is taken. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

static int64_t steps(int64_t start) {
    int64_t n = start;
    int64_t count = 0;
    while (n != 1) {
        if (n % 2 == 0) {
            n = n / 2;
        } else {
            n = 3 * n + 1;
        }
        count += 1;
    }
    return count;
}

typedef struct { int64_t result; } task_t;

static int64_t take_all(int64_t first) {
    task_t** tasks = malloc(8 * sizeof *tasks);
    if (!tasks) abort();
    for (int64_t i = 0; i < 8; i++) {
        tasks[i] = malloc(sizeof **tasks);
        if (!tasks[i]) abort();
        tasks[i]->result = steps(first + i);
    }
    int64_t* values = malloc(8 * sizeof *values);
    if (!values) abort();
    for (int64_t i = 0; i < 8; i++) {
        values[i] = tasks[i]->result;
        free(tasks[i]);
    }
    free(tasks);
    int64_t total = 0;
    for (int64_t i = 0; i < 8; i++) total += values[i];
    free(values);
    return total;
}

static int64_t take_one_by_one(int64_t first) {
    task_t** tasks = malloc(8 * sizeof *tasks);
    if (!tasks) abort();
    int64_t len = 0;
    for (int64_t i = 8; i < 16; i++) {
        tasks[len] = malloc(sizeof **tasks);
        if (!tasks[len]) abort();
        tasks[len++]->result = steps(first + i);
    }
    int64_t total = 0;
    while (len > 0) {
        task_t* one = tasks[0];
        for (int64_t i = 1; i < len; i++) tasks[i - 1] = tasks[i];
        len--;
        total += one->result;
        free(one);
    }
    free(tasks);
    return total;
}

int main(void) {
    int64_t total = 0;
    for (int64_t round = 0; round < 200000; round++) {
        int64_t first = round * 16 + 1;
        total += take_all(first);
        total += take_one_by_one(first);
    }
    printf("%lld\n", (long long)total);
    return 0;
}
