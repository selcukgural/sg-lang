// The C file of the `varargs` program: functions that take `...` and read it with `va_arg`,
// each as the type it looks for.
#include <stdarg.h>
#include <stdint.h>

static int64_t sum_longs(int32_t count, ...) {
    va_list args;
    va_start(args, count);
    int64_t total = 0;
    for (int32_t i = 0; i < count; i++) total += va_arg(args, int64_t);
    va_end(args);
    return total;
}

static double mean(int32_t count, ...) {
    va_list args;
    va_start(args, count);
    double total = 0;
    for (int32_t i = 0; i < count; i++) total += va_arg(args, double);
    va_end(args);
    return count == 0 ? 0 : total / count;
}

typedef int32_t (*step)(int32_t);

// Pairs of a function and a number: the sum of what each function gives for its number.
static int32_t call_each(int32_t count, ...) {
    va_list args;
    va_start(args, count);
    int32_t total = 0;
    for (int32_t i = 0; i < count; i++) {
        step f = va_arg(args, step);
        int32_t n = va_arg(args, int32_t);
        total += f(n);
    }
    va_end(args);
    return total;
}

// Writes 10, 20, 30, ... into the places it is given.
static void number_each(int32_t count, ...) {
    va_list args;
    va_start(args, count);
    for (int32_t i = 0; i < count; i++) *va_arg(args, int32_t*) = (i + 1) * 10;
    va_end(args);
}

struct point { int32_t x; int32_t y; };

// Moves each point it is given by one, and gives the sum of what they hold after.
static int32_t shift_points(int32_t count, ...) {
    va_list args;
    va_start(args, count);
    int32_t total = 0;
    for (int32_t i = 0; i < count; i++) {
        struct point* p = va_arg(args, struct point*);
        p->x += 1;
        p->y += 1;
        total += p->x + p->y;
    }
    va_end(args);
    return total;
}

// The length of each text, after a number that says how many there are.
static int64_t lengths(int32_t count, ...) {
    va_list args;
    va_start(args, count);
    int64_t total = 0;
    for (int32_t i = 0; i < count; i++) {
        const char* text = va_arg(args, const char*);
        while (*text++) total++;
    }
    va_end(args);
    return total;
}
