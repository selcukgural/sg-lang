/* link: -lz */
/* The same program written by hand in C, calling zlib through its header. */
#include <stdint.h>
#include <stdio.h>
#include <zlib.h>

static uint8_t out[65536];

static int64_t run(z_stream* s, int flush) {
    int64_t made = 0;
    for (;;) {
        s->next_out = out;
        s->avail_out = sizeof out;
        int code = deflate(s, flush);
        if (code < 0 && code != Z_BUF_ERROR) return -1;
        made += (int64_t)(sizeof out - s->avail_out);
        if ((flush == Z_FINISH && code == Z_STREAM_END) || (flush == Z_NO_FLUSH && s->avail_out != 0)) return made;
    }
}

int main(void) {
    static uint8_t block[65536];
    for (int64_t i = 0; i < 65536; i++) {
        block[i] = (uint8_t)(97 + (i * i + i / 7) % 23);
    }
    z_stream s = {0};
    if (deflateInit(&s, 1) != Z_OK) return 1;
    int64_t total = 0;
    for (int32_t round = 0; round < 16384; round++) {
        s.next_in = block;
        s.avail_in = sizeof block;
        int64_t made = run(&s, Z_NO_FLUSH);
        if (made < 0) return 1;
        total += made;
    }
    s.next_in = NULL;
    s.avail_in = 0;
    int64_t made = run(&s, Z_FINISH);
    if (made < 0) return 1;
    total += made;
    deflateEnd(&s);
    printf("%lld\n", (long long)total);
    return 0;
}
