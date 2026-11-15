/* link: -lz */
/* The same program written by hand in C, calling zlib through its header. */
#include <stdint.h>
#include <stdio.h>
#include <zlib.h>

int main(void) {
    static uint8_t block[65536];
    for (int32_t i = 0; i < 65536; i++) {
        block[i] = (uint8_t)((i * 31 + 7) & 255);
    }
    uLong crc = 0;
    for (int32_t round = 0; round < 262144; round++) {
        crc = crc32(crc, block, 65536);
    }
    printf("%lu\n", (unsigned long)crc);
    return 0;
}
