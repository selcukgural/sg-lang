/* The same program written by hand in C, without async: the two lengths are plain calls, and
   the one the task would give is put in a block that is allocated and freed. */
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

static int64_t both(int64_t n) {
    task_t* task = malloc(sizeof *task);
    if (!task) abort();
    task->result = steps(n);
    int64_t next = steps(n + 1);
    int64_t total = next + task->result;
    free(task);
    return total;
}

int main(void) {
    int64_t total = 0;
    for (int64_t n = 1; n < 1000000; n++) {
        total += both(n);
    }
    printf("%lld\n", (long long)total);
    return 0;
}
