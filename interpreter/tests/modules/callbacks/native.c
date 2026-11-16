// The C file of the `callbacks` program: a library that keeps a callback and its state, and
// calls the functions of SG it is given, and some by their names.
#include <stdint.h>
#include <string.h>

typedef void (*on_word)(void* data, int32_t number, int32_t length);

static on_word listener = NULL;
static void* listener_data = NULL;

static void listen(on_word callback, void* data) {
    listener = callback;
    listener_data = data;
}

// Calls the listener kept earlier, once for each word of the text.
static int32_t feed(const char* text) {
    int32_t words = 0;
    const char* at = text;
    while (*at) {
        while (*at == ' ') at++;
        if (!*at) break;
        const char* start = at;
        while (*at && *at != ' ') at++;
        words += 1;
        if (listener) listener(listener_data, words, (int32_t)(at - start));
    }
    return words;
}

static int32_t triple(int32_t n) {
    return 3 * n;
}

// Calls `scaled`, an `export` function of SG, by its name: SG declares it before this file.
static int32_t sum_scaled(int32_t n) {
    int32_t total = 0;
    for (int32_t i = 1; i <= n; i++) {
        total += scaled(i);
    }
    return total;
}

// Through the pointer it is given, and the one SG gave back.
static int32_t apply_twice(int32_t (*f)(int32_t), int32_t n) {
    return f(f(n));
}
