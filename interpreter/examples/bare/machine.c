/* A pretend machine for the tests of `examples/bare`: a program of a computer that has an
   operating system, linked with the object `sg build examples/bare --freestanding` gives, so
   that the program can be run here. It gives the program its memory, shows what it writes, and
   calls it as a kernel's start code would: `start`, then `report`, then `readingAt` past the
   end of the readings, which panics; `onPanic` writes the message and stops the machine with
   the code 101. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

void start(void);
void report(void);
int32_t readingAt(int32_t index);

static _Alignas(16) uint8_t memory[1 << 16];

uint8_t *board_memory(void) { return memory; }
uint64_t board_memory_size(void) { return sizeof memory; }

void board_write(const uint8_t *text, uint64_t length) { fwrite(text, 1, (size_t)length, stdout); }

void board_stop(int32_t code) {
    fflush(stdout);
    exit(code);
}

int main(void) {
    start();
    report();
    printf("the third reading: %d\n", (int)readingAt(2));
    report();
    printf("the eighth reading: %d\n", (int)readingAt(7));
    printf("not reached\n");
    return 0;
}
