/* The lines of `netlines.sg` written by hand in C the classic way: blocking sockets, a thread
   that reads the input in pieces of 64 KB and sends each, and the main thread that reads the lines
   with `getline` from a `FILE` opened over the socket, the `\n` and a `\r` before it cut off. The
   lines are not checked to be UTF-8. */
#if defined(__linux__)
#define _GNU_SOURCE
#endif
#if defined(_WIN32)
#include "windows/posix.h"
#else
#include <arpa/inet.h>
#include <netinet/in.h>
#include <pthread.h>
#include <sys/socket.h>
#include <unistd.h>
#endif
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int port;

#if defined(_WIN32)
#define read(fd, to, n) _read(fd, to, (unsigned)(n))

/* Windows' C library cannot open a `FILE` over a socket: a line is cut from a block of 64 KB that
   `recv` fills, as `getline` cuts it from the block of a `FILE`. */
static char block[65536];
static size_t block_at = 0, block_len = 0;

static ssize_t socket_line(int fd, char** line, size_t* cap) {
    size_t len = 0;
    for (;;) {
        if (block_at == block_len) {
            int got = recv(fd, block, sizeof block, 0);
            if (got < 0) { perror("recv"); exit(1); }
            if (got == 0) return len ? (ssize_t)len : -1;
            block_at = 0;
            block_len = (size_t)got;
        }
        char* end = memchr(block + block_at, '\n', block_len - block_at);
        size_t take = end ? (size_t)(end - (block + block_at)) + 1 : block_len - block_at;
        if (len + take + 1 > *cap) {
            while (len + take + 1 > *cap) *cap = *cap ? *cap * 2 : 256;
            *line = realloc(*line, *cap);
            if (!*line) abort();
        }
        memcpy(*line + len, block + block_at, take);
        len += take;
        block_at += take;
        if (end) return (ssize_t)len;
    }
}
#endif

static void* sender(void* p) {
    (void)p;
    int fd = socket(AF_INET, SOCK_STREAM, 0);
    struct sockaddr_in at = { .sin_family = AF_INET, .sin_port = htons((uint16_t)port) };
    at.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    if (connect(fd, (struct sockaddr*)&at, sizeof at) != 0) { perror("connect"); exit(1); }
    static char buffer[65536];
    for (;;) {
        ssize_t n = read(0, buffer, sizeof buffer);
        if (n < 0) { perror("read"); exit(1); }
        if (n == 0) break;
        for (ssize_t done = 0; done < n;) {
            ssize_t put = send(fd, buffer + done, (size_t)(n - done), 0);
            if (put < 0) { perror("send"); exit(1); }
            done += put;
        }
    }
    close(fd);
    return NULL;
}

int main(void) {
    int listener = socket(AF_INET, SOCK_STREAM, 0);
    struct sockaddr_in at = { .sin_family = AF_INET, .sin_port = 0 };
    at.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    if (bind(listener, (struct sockaddr*)&at, sizeof at) != 0 || listen(listener, 128) != 0) { perror("bind"); return 1; }
    socklen_t size = sizeof at;
    getsockname(listener, (struct sockaddr*)&at, &size);
    port = ntohs(at.sin_port);
#if defined(_WIN32)
    _setmode(_fileno(stdin), _O_BINARY);
#endif
    pthread_t thread;
    pthread_create(&thread, NULL, sender, NULL);
    int fd = accept(listener, NULL, NULL);
    if (fd < 0) { perror("accept"); return 1; }
    char* line = NULL;
    size_t cap = 0;
    long long lines = 0, bytes = 0, longest = 0, errors = 0;
    ssize_t got;
#if defined(_WIN32)
    while ((got = socket_line(fd, &line, &cap)) >= 0) {
#else
    FILE* in = fdopen(fd, "r");
    while ((got = getline(&line, &cap, in)) >= 0) {
#endif
        long long length = got;
        if (length > 0 && line[length - 1] == '\n') {
            length--;
            if (length > 0 && line[length - 1] == '\r') length--;
        }
        lines++;
        bytes += length;
        if (length > longest) longest = length;
        if (length >= 5 && memcmp(line, "ERROR", 5) == 0) errors++;
    }
    free(line);
#if defined(_WIN32)
    close(fd);
#else
    fclose(in);
#endif
    pthread_join(thread, NULL);
    close(listener);
    printf("%lld lines, %lld bytes, longest %lld, %lld errors\n", lines, bytes, longest, errors);
    return 0;
}
