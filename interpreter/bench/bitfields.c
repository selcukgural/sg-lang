/* The same program written by hand in C, with no checks. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct { uint16_t red : 5; uint16_t green : 6; uint16_t blue : 5; } pixel;

typedef struct {
    uint8_t header_length : 4;
    uint8_t version : 4;
    uint8_t service;
    uint16_t total_length;
    uint16_t id;
    uint16_t fragment;
    uint8_t time_to_live;
    uint8_t protocol;
    uint16_t checksum;
    uint32_t source;
    uint32_t target;
} ipv4;

int main(void) {
    int64_t count = 4000000;
    pixel* pixels = malloc((size_t)count * sizeof(pixel));
    for (int32_t i = 0; i < count; i++) {
        pixels[i] = (pixel){ .red = (uint16_t)(i & 31), .green = (uint16_t)(i >> 5 & 63), .blue = (uint16_t)(i >> 11 & 31) };
    }
    for (int round = 0; round < 500; round++) {
        for (int64_t i = 0; i < count; i++) {
            pixel p = pixels[i];
            if (p.red < 31) p.red += 1;
            if (p.green < 62) p.green += 2;
            p.blue = p.blue >> 1;
            pixels[i] = p;
        }
    }
    int64_t sum = 0;
    for (int64_t i = 0; i < count; i++) sum += (int64_t)pixels[i].red + pixels[i].green + pixels[i].blue;
    printf("%lld\n", (long long)sum);
    free(pixels);

    uint8_t* packets = malloc(4000 * 20);
    for (int32_t k = 0; k < 4000 * 20; k++) packets[k] = (uint8_t)((k * 7 + k / 20) & 0xff);
    int64_t total = 0;
    for (int round = 0; round < 20000; round++) {
        for (int32_t k = 0; k < 4000; k++) {
            ipv4 ip;
            memcpy(&ip, packets + k * 20, sizeof ip);
            total += (int64_t)ip.version + ip.header_length + ip.time_to_live + (ip.source & 0xff);
        }
    }
    printf("%lld\n", (long long)total);
    free(packets);
    return 0;
}
