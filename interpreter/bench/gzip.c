/* link: -lz */
/* link on windows: -lzlib */
/* The same program written by hand in C, calling zlib through its header. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <zlib.h>

static const char* words[] = {"the", "quick", "brown", "fox", "jumps", "over", "lazy", "dog", "compression", "of", "text", "is", "a", "matter", "repeated", "words", "and", "their", "places", "window"};

static size_t made(uint8_t* data) {
    size_t n = 0;
    uint32_t state = 12345;
    while (n < 16000000) {
        state = state * 1103515245u + 12345u;
        const char* word = words[(state >> 16) % 20];
        size_t length = strlen(word);
        memcpy(data + n, word, length);
        n += length;
        data[n++] = ((state >> 8) & 15) == 0 ? 10 : 32;
    }
    return n;
}

int main(void) {
    uint8_t* data = malloc(16000100);
    size_t n = made(data);
    z_stream s;
    memset(&s, 0, sizeof s);
    deflateInit2(&s, 6, Z_DEFLATED, 31, 8, Z_DEFAULT_STRATEGY);
    gz_header header;
    memset(&header, 0, sizeof header);
    header.os = 255;
    deflateSetHeader(&s, &header);
    uLong cap = deflateBound(&s, n);
    uint8_t* packed = malloc(cap);
    s.next_in = data;
    s.avail_in = (uInt)n;
    s.next_out = packed;
    s.avail_out = (uInt)cap;
    if (deflate(&s, Z_FINISH) != Z_STREAM_END) return 1;
    printf("%lu %lu\n", (unsigned long)s.total_out, (unsigned long)crc32(0, packed, (uInt)s.total_out));
    return 0;
}
