/* The lines of `netlines.sg` written by hand in C the classic way: blocking sockets, a thread
   that reads the input in pieces of 64 KB and sends each, and the main thread that reads the lines
   with `getline` from a `FILE` opened over the socket, the `\n` and a `\r` before it cut off. The
   lines are not checked to be UTF-8. */
#if defined(__linux__)
#define _GNU_SOURCE
#endif
#include <arpa/inet.h>
#include <netinet/in.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <unistd.h>

static int port;

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
    pthread_t thread;
    pthread_create(&thread, NULL, sender, NULL);
    int fd = accept(listener, NULL, NULL);
    if (fd < 0) { perror("accept"); return 1; }
    FILE* in = fdopen(fd, "r");
    char* line = NULL;
    size_t cap = 0;
    long long lines = 0, bytes = 0, longest = 0, errors = 0;
    ssize_t got;
    while ((got = getline(&line, &cap, in)) >= 0) {
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
    fclose(in);
    pthread_join(thread, NULL);
    close(listener);
    printf("%lld lines, %lld bytes, longest %lld, %lld errors\n", lines, bytes, longest, errors);
    return 0;
}
