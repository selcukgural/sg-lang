/* As pieces.sg, in C: `copy` (memcpy into one buffer, then send) or `writev` of the two. */
#define _GNU_SOURCE
#include <arpa/inet.h>
#include <netinet/in.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/uio.h>
#include <time.h>
#include <unistd.h>
static int lfd;
static void* sink(void* arg) {
    (void)arg;
    int c = accept(lfd, NULL, NULL);
    static char buf[262144];
    while (read(c, buf, sizeof buf) > 0) {}
    close(c);
    return NULL;
}
static void send_all(int fd, const char* p, size_t n) {
    while (n) { ssize_t k = send(fd, p, n, MSG_NOSIGNAL); if (k < 0) { perror("send"); exit(1); } p += k; n -= (size_t)k; }
}
static void writev_all(int fd, struct iovec* v, int cnt) {
    while (cnt) {
        ssize_t k = writev(fd, v, cnt);
        if (k < 0) { perror("writev"); exit(1); }
        while (cnt && (size_t)k >= v->iov_len) { k -= (ssize_t)v->iov_len; v++; cnt--; }
        if (cnt) { v->iov_base = (char*)v->iov_base + k; v->iov_len -= (size_t)k; }
    }
}
int main(int argc, char** argv) {
    int copy = strcmp(argv[1], "copy") == 0;
    size_t size = (size_t)atol(argv[2]);
    long count = atol(argv[3]);
    lfd = socket(AF_INET, SOCK_STREAM, 0);
    struct sockaddr_in a = { .sin_family = AF_INET, .sin_addr.s_addr = htonl(INADDR_LOOPBACK) };
    bind(lfd, (struct sockaddr*)&a, sizeof a);
    listen(lfd, 1);
    socklen_t al = sizeof a;
    getsockname(lfd, (struct sockaddr*)&a, &al);
    pthread_t t;
    pthread_create(&t, NULL, sink, NULL);
    int fd = socket(AF_INET, SOCK_STREAM, 0);
    connect(fd, (struct sockaddr*)&a, sizeof a);
    char* head = malloc(100); memset(head, 72, 100);
    char* body = malloc(size); memset(body, 66, size);
    char* out = malloc(100 + size);
    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    for (long i = 0; i < count; i++) {
        if (copy) {
            memcpy(out, head, 100);
            memcpy(out + 100, body, size);
            send_all(fd, out, 100 + size);
        } else {
            struct iovec v[2] = { { head, 100 }, { body, size } };
            writev_all(fd, v, 2);
        }
    }
    clock_gettime(CLOCK_MONOTONIC, &t1);
    close(fd);
    pthread_join(t, NULL);
    long ns = (t1.tv_sec - t0.tv_sec) * 1000000000L + (t1.tv_nsec - t0.tv_nsec);
    printf("%ld\n", ns / count);
    return 0;
}
