/* The machine of the programs in `tests/freestanding`: a program of this computer, linked with
   the object of one of them, so that it can be run here. It gives the memory, writes what the
   program writes at once, and starts it; `board_stop` ends the process with the code given. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

void start(void);

static _Alignas(16) uint8_t memory[1 << 20];

uint8_t *board_memory(void) { return memory; }
uint64_t board_memory_size(void) { return sizeof memory; }

void board_write(const uint8_t *text, uint64_t length) {
    fwrite(text, 1, (size_t)length, stdout);
    fflush(stdout);
}

void board_stop(int32_t code) {
    fflush(stdout);
    exit(code);
}

int main(void) {
    start();
    return 0;
}
