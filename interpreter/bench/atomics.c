/* The same counters written by hand in C: four threads with a stack of 8 MB each, and the
   `__atomic` builtins with the same orders. */
#if defined(_WIN32)
#include "windows/posix.h"
#else
#include <pthread.h>
#endif
#include <stdint.h>
#include <stdio.h>

typedef struct { int64_t ordered; int64_t relaxed; } counts_t;
typedef struct { counts_t* counts; int64_t times; } work_t;

static void* count(void* p) {
    work_t* w = p;
    for (int64_t i = 0; i < w->times; i++) {
        __atomic_fetch_add(&w->counts->ordered, 1, __ATOMIC_SEQ_CST);
        __atomic_fetch_add(&w->counts->relaxed, 2, __ATOMIC_RELAXED);
    }
    return NULL;
}

int main(void) {
    counts_t counts = { 0, 0 };
    pthread_attr_t attr;
    pthread_attr_init(&attr);
    pthread_attr_setstacksize(&attr, (size_t)8 << 20);
    pthread_t threads[4];
    work_t work = { &counts, 5000000 };
    for (int i = 0; i < 4; i++) pthread_create(&threads[i], &attr, count, &work);
    for (int i = 0; i < 4; i++) pthread_join(threads[i], NULL);
    printf("%lld\n", (long long)(__atomic_load_n(&counts.ordered, __ATOMIC_SEQ_CST) + __atomic_load_n(&counts.relaxed, __ATOMIC_SEQ_CST)));
    return 0;
}
