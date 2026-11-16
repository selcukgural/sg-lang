// A program of C that uses the calculator written in SG, through `calclib.h`.
//
//   sg build examples/calclib --library -o build/libcalclib.a
//   cc -std=c11 -Ibuild -o calc examples/calclib/main.c build/libcalclib.a -lm -pthread
//
// or with the shared library:
//
//   sg build examples/calclib --shared -o build/libcalclib.so      (.dylib on macOS)
//   cc -std=c11 -Ibuild -o calc examples/calclib/main.c -Lbuild -lcalclib -Wl,-rpath,build
#include <stdio.h>

#include "calclib.h"

// The numbers the functions give, as `CalcError.code` in SG gives them.
enum { CALC_OK = 0, CALC_SYNTAX = 1, CALC_UNKNOWN = 2, CALC_DIVISION = 3, CALC_NOT_TEXT = 4 };

// Works out `text` and prints it under `label`, the text itself when that is NULL.
static void show(Calculator *calc, const char *text, const char *label) {
    double value = 0;
    int32_t code = calc_eval(calc, text, &value);
    if (code == CALC_OK) {
        printf("%-22s = %g\n", label ? label : text, value);
        return;
    }
    char message[64];
    calc_error(calc, (uint8_t *)message, sizeof message);
    printf("%-22s : error %d: %s\n", label ? label : text, (int)code, message);
}

// Called by the library for each variable, with the `data` given to it.
static void print_variable(void *data, uint8_t *name, double value) {
    int *count = data;
    *count += 1;
    printf("  %s = %g\n", (const char *)name, value);
}

// Called by the library when the calculator ends.
static void closed(void *data, int64_t evaluations) {
    printf("%s ended after %lld evaluations\n", (const char *)data, (long long)evaluations);
}

int main(void) {
    Calculator *calc = calc_new();
    calc_on_close(calc, closed, "the calculator");

    // Statements, assignments, functions, and what goes wrong.
    calc_set(calc, "rate", 0.25);
    show(calc, "1 + 2 * 3", NULL);
    show(calc, "price = 80", NULL);
    show(calc, "price * (1 + rate)", NULL);
    show(calc, "2 ^ 10", NULL);
    show(calc, "sqrt(price + 1)", NULL);
    show(calc, "1 / (rate - 0.25)", NULL);
    show(calc, "price +", NULL);
    show(calc, "cost * 2", NULL);
    show(calc, "\xff", "a byte 0xff");

    // A message longer than the buffer is cut, and its whole length is given, as by snprintf.
    char small[8];
    uint64_t length = calc_error(calc, (uint8_t *)small, sizeof small);
    printf("cut to \"%s\", %llu bytes in all\n", small, (unsigned long long)length);

    // A statement read once and run many times, lent to each call.
    Expr *step = calc_compile(calc, "n = n * 2 + 1");
    calc_set(calc, "n", 0);
    double n = 0;
    for (int i = 0; i < 5; i++) calc_run(calc, step, &n);
    printf("n after 5 steps = %g\n", n);
    calc_expr_free(step);
    if (!calc_compile(calc, "(1 + 2")) {
        char message[64];
        calc_error(calc, (uint8_t *)message, sizeof message);
        printf("does not compile: %s\n", message);
    }

    // A sum worked out by four tasks of the library.
    double sum = 0;
    calc_sum(calc, "k * k", "k", 1, 1000, &sum);
    printf("sum of k * k for k in 1..1000 = %.0f\n", sum);

    // Plain data, by value.
    CalcStats stats = calc_stats(calc);
    printf("%lld evaluations, %lld failed, %d variables\n", (long long)stats.evaluations, (long long)stats.failures, stats.variables);

    // Each variable, through a callback.
    int count = 0;
    calc_each_variable(calc, print_variable, &count);
    printf("%d variables seen\n", count);

    calc_free(calc);
    calc_free(NULL);
    return 0;
}
