/* The same program written by hand in C: the table in a plain global, filled before the loop. */
#include <stdint.h>
#include <stdio.h>

static uint32_t crc_table[256];

static void fill_table(void) {
    for (int n = 0; n < 256; n++) {
        uint32_t c = (uint32_t)n;
        for (int k = 0; k < 8; k++) {
            c = (c & 1) ? 0xEDB88320u ^ (c >> 1) : c >> 1;
        }
        crc_table[n] = c;
    }
}

static uint32_t crc(uint32_t start, const uint8_t* data, int64_t length) {
    uint32_t c = start ^ 0xFFFFFFFFu;
    for (int64_t i = 0; i < length; i++) {
        c = crc_table[(c ^ data[i]) & 0xFF] ^ (c >> 8);
    }
    return c ^ 0xFFFFFFFFu;
}

int main(void) {
    static uint8_t block[65536];
    for (int32_t i = 0; i < 65536; i++) {
        block[i] = (uint8_t)((i * 31 + 7) & 255);
    }
    fill_table();
    uint32_t sum = 0;
    for (int32_t round = 0; round < 4096; round++) {
        sum = crc(sum, block, 65536);
    }
    printf("%u\n", sum);
    return 0;
}
