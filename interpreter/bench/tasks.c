/* The same program written by hand in C, with no checks: a thread with a stack of 8 MB for
   each task, and a mutex around the total. */
#include <pthread.h>
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

typedef struct { pthread_mutex_t* lock; int64_t* total; int64_t from, to; } work_t;

static void* work(void* p) {
    work_t* w = p;
    int64_t part = 0;
    for (int64_t n = w->from; n < w->to; n++) {
        part += steps(n);
        if (n % 100 == 99) {
            pthread_mutex_lock(w->lock);
            *w->total += part;
            pthread_mutex_unlock(w->lock);
            part = 0;
        }
    }
    pthread_mutex_lock(w->lock);
    *w->total += part;
    pthread_mutex_unlock(w->lock);
    return NULL;
}

int main(void) {
    pthread_mutex_t lock;
    pthread_mutex_init(&lock, NULL);
    int64_t total = 0;
    pthread_attr_t attr;
    pthread_attr_init(&attr);
    pthread_attr_setstacksize(&attr, (size_t)8 << 20);
    for (int round = 0; round < 10; round++) {
        pthread_t threads[4];
        work_t works[4];
        for (int i = 0; i < 4; i++) {
            works[i] = (work_t){ &lock, &total, i == 0 ? 1 : i * 100000, (i + 1) * 100000 };
            if (pthread_create(&threads[i], &attr, work, &works[i]) != 0) abort();
        }
        for (int i = 0; i < 4; i++) pthread_join(threads[i], NULL);
    }
    pthread_attr_destroy(&attr);
    pthread_mutex_lock(&lock);
    printf("%lld\n", (long long)total);
    pthread_mutex_unlock(&lock);
    pthread_mutex_destroy(&lock);
    return 0;
}
