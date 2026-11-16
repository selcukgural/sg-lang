/* The headers as C declares them, to check that C reads what SG writes into memory. */
#include <stddef.h>
#include <stdint.h>

#pragma pack(push, 1)
struct bmp_file_header {
    uint16_t kind;
    uint32_t file_size;
    uint16_t reserved1;
    uint16_t reserved2;
    uint32_t data_offset;
};
#pragma pack(pop)

/* As Linux declares `struct iphdr` on a machine whose bits start at the lowest one. */
struct ipv4_header {
    uint8_t header_length : 4;
    uint8_t version : 4;
    uint8_t service;
    uint16_t total_length;
    uint16_t id;
    uint16_t fragment;
    uint8_t time_to_live;
    uint8_t protocol;
    uint16_t checksum;
    union { uint32_t value; uint8_t octets[4]; } source;
    union { uint32_t value; uint8_t octets[4]; } target;
};

static intptr_t c_size(int32_t which) {
    return which == 0 ? (intptr_t)sizeof(struct bmp_file_header) : (intptr_t)sizeof(struct ipv4_header);
}

static uint32_t c_bmp_offset(const void* header) {
    return ((const struct bmp_file_header*)header)->data_offset;
}

static int32_t c_ip_version(const void* header) {
    return ((const struct ipv4_header*)header)->version;
}

static uint32_t c_ip_source(const void* header) {
    return ((const struct ipv4_header*)header)->source.value;
}
