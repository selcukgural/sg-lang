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
    deflateInit2(&s, 1, Z_DEFLATED, 31, 8, Z_DEFAULT_STRATEGY);
    uLong cap = deflateBound(&s, n);
    uint8_t* packed = malloc(cap);
    s.next_in = data;
    s.avail_in = (uInt)n;
    s.next_out = packed;
    s.avail_out = (uInt)cap;
    if (deflate(&s, Z_FINISH) != Z_STREAM_END) return 1;
    uLong packedSize = s.total_out;
    deflateEnd(&s);
    uint8_t* plain = malloc(n);
    uLong size = 0;
    uLong check = 0;
    for (int i = 0; i < 20; i++) {
        z_stream d;
        memset(&d, 0, sizeof d);
        inflateInit2(&d, 31);
        d.next_in = packed;
        d.avail_in = (uInt)packedSize;
        d.next_out = plain;
        d.avail_out = (uInt)n;
        if (inflate(&d, Z_FINISH) != Z_STREAM_END) return 1;
        size = d.total_out;
        check = crc32(0, plain, (uInt)size);
        inflateEnd(&d);
    }
    printf("%lu %lu\n", (unsigned long)size, (unsigned long)check);
    return 0;
}
