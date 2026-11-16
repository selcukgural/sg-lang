/* The same queue written by hand in C: Vyukov's bounded queue for many producers and many
   consumers, with the `__atomic` builtins and the same orders, four threads that push and four
   that take, each with a stack of 8 MB. */
#include <pthread.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#if defined(__x86_64__) || defined(__i386__)
#define SPIN() __builtin_ia32_pause()
#elif defined(__aarch64__)
#define SPIN() __asm__ __volatile__("yield")
#else
#define SPIN() ((void)0)
#endif

typedef struct { _Alignas(64) size_t value; } position;
typedef struct { size_t turn; int64_t value; } cell;
typedef struct { cell* cells; size_t mask; position head; position tail; } queue;

static bool push(queue* q, int64_t value) {
    size_t at = __atomic_load_n(&q->head.value, __ATOMIC_RELAXED);
    for (;;) {
        cell* c = &q->cells[at & q->mask];
        size_t turn = __atomic_load_n(&c->turn, __ATOMIC_ACQUIRE);
        intptr_t ahead = (intptr_t)turn - (intptr_t)at;
        if (ahead == 0) {
            if (__atomic_compare_exchange_n(&q->head.value, &at, at + 1, true, __ATOMIC_RELAXED, __ATOMIC_RELAXED)) {
                __atomic_store_n(&c->value, value, __ATOMIC_RELAXED);
                __atomic_store_n(&c->turn, at + 1, __ATOMIC_RELEASE);
                return true;
            }
        } else if (ahead < 0) {
            return false;
        } else {
            at = __atomic_load_n(&q->head.value, __ATOMIC_RELAXED);
        }
    }
}

static bool pop(queue* q, int64_t* value) {
    size_t at = __atomic_load_n(&q->tail.value, __ATOMIC_RELAXED);
    for (;;) {
        cell* c = &q->cells[at & q->mask];
        size_t turn = __atomic_load_n(&c->turn, __ATOMIC_ACQUIRE);
        intptr_t ahead = (intptr_t)turn - (intptr_t)(at + 1);
        if (ahead == 0) {
            if (__atomic_compare_exchange_n(&q->tail.value, &at, at + 1, true, __ATOMIC_RELAXED, __ATOMIC_RELAXED)) {
                *value = __atomic_load_n(&c->value, __ATOMIC_RELAXED);
                __atomic_store_n(&c->turn, at + q->mask + 1, __ATOMIC_RELEASE);
                return true;
            }
        } else if (ahead < 0) {
            return false;
        } else {
            at = __atomic_load_n(&q->tail.value, __ATOMIC_RELAXED);
        }
    }
}

typedef struct { queue* q; int64_t count; int64_t* left; int64_t sum; } work_t;

static void* produce(void* p) {
    work_t* w = p;
    for (int64_t n = 1; n <= w->count; n++) {
        while (!push(w->q, n)) SPIN();
    }
    return NULL;
}

static void* consume(void* p) {
    work_t* w = p;
    int64_t sum = 0, value;
    while (__atomic_load_n(w->left, __ATOMIC_RELAXED) > 0) {
        if (!pop(w->q, &value)) {
            SPIN();
            continue;
        }
        sum += value;
        __atomic_fetch_sub(w->left, 1, __ATOMIC_RELAXED);
    }
    w->sum = sum;
    return NULL;
}

int main(void) {
    int64_t count = 500000;
    queue q = { .mask = 1023 };
    q.cells = malloc(sizeof(cell) * 1024);
    if (!q.cells) abort();
    for (size_t at = 0; at < 1024; at++) {
        q.cells[at].turn = at;
        q.cells[at].value = 0;
    }
    int64_t left = 4 * count;
    pthread_attr_t attr;
    pthread_attr_init(&attr);
    pthread_attr_setstacksize(&attr, (size_t)8 << 20);
    pthread_t threads[8];
    work_t work[8];
    for (int i = 0; i < 4; i++) {
        work[2 * i] = (work_t){ &q, count, &left, 0 };
        work[2 * i + 1] = (work_t){ &q, count, &left, 0 };
        pthread_create(&threads[2 * i], &attr, produce, &work[2 * i]);
        pthread_create(&threads[2 * i + 1], &attr, consume, &work[2 * i + 1]);
    }
    int64_t total = 0;
    for (int i = 0; i < 8; i++) {
        pthread_join(threads[i], NULL);
        total += work[i].sum;
    }
    printf("%lld\n", (long long)total);
    free(q.cells);
    return 0;
}
