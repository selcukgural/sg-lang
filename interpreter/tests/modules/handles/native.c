// The C side of the `handles` program: it keeps what SG gives it by its address.
#include <stddef.h>

static int64_t count_again(void* tally) {
    return tally_count(tally);
}

static int32_t drive(void) {
    void* tally = tally_new("first");
    tally_add(tally, 4);
    tally_add(tally, 5);
    report("count", (double)tally_count(tally));
    report("twice", (double)tally_twice(tally, count_again, tally));
    tally_free(tally);
    tally_free(NULL);
    double total = 0;
    for (int32_t kind = 0; kind < 3; kind++) {
        void* shape = shape_make(kind);
        if (!shape) {
            report("no shape", kind);
            continue;
        }
        total += shape_area(shape);
        shape_free(shape);
    }
    report("areas", total);
    return 3;
}
