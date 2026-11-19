/* link: -lcrypto */
/* The same SHA-256 computed by OpenSSL, called through its header. */
#include <stdint.h>
#include <stdio.h>
#include <openssl/evp.h>

int main(void) {
    static uint8_t block[65536];
    for (int32_t i = 0; i < 65536; i++) {
        block[i] = (uint8_t)((i * 31 + 7) & 255);
    }
    EVP_MD_CTX *context = EVP_MD_CTX_new();
    EVP_DigestInit_ex(context, EVP_sha256(), NULL);
    for (int32_t round = 0; round < 16384; round++) {
        EVP_DigestUpdate(context, block, sizeof block);
    }
    unsigned char hash[32];
    unsigned int length = 0;
    EVP_DigestFinal_ex(context, hash, &length);
    EVP_MD_CTX_free(context);
    for (unsigned int i = 0; i < length; i++) {
        printf("%02x", hash[i]);
    }
    printf("\n");
    return 0;
}
