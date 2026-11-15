/* The echo of `echo.sg` written by hand in C the classic way: blocking sockets, a thread that
   accepts and a thread for each connection, and a thread for each client. */
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

enum { CLIENTS = 8, ROUNDS = 20000, SIZE = 64 };
static int port;

static void* echo(void* p) {
    int fd = (int)(long)p;
    char buffer[SIZE];
    for (;;) {
        ssize_t n = recv(fd, buffer, sizeof buffer, 0);
        if (n <= 0) break;
        for (ssize_t done = 0; done < n;) {
            ssize_t put = send(fd, buffer + done, (size_t)(n - done), 0);
            if (put < 0) { perror("send"); exit(1); }
            done += put;
        }
    }
    close(fd);
    return NULL;
}

static void* serve(void* p) {
    int listener = (int)(long)p;
    pthread_t threads[CLIENTS];
    for (int i = 0; i < CLIENTS; i++) {
        int fd = accept(listener, NULL, NULL);
        if (fd < 0) { perror("accept"); exit(1); }
        pthread_create(&threads[i], NULL, echo, (void*)(long)fd);
    }
    for (int i = 0; i < CLIENTS; i++) pthread_join(threads[i], NULL);
    return NULL;
}

static void* client(void* p) {
    long* sum = p;
    int fd = socket(AF_INET, SOCK_STREAM, 0);
    struct sockaddr_in at = { .sin_family = AF_INET, .sin_port = htons((uint16_t)port) };
    at.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    if (connect(fd, (struct sockaddr*)&at, sizeof at) != 0) { perror("connect"); exit(1); }
    unsigned char message[SIZE], buffer[SIZE];
    memset(message, 7, sizeof message);
    for (int r = 0; r < ROUNDS; r++) {
        if (send(fd, message, sizeof message, 0) != SIZE) { perror("send"); exit(1); }
        size_t got = 0;
        while (got < SIZE) {
            ssize_t n = recv(fd, buffer + got, SIZE - got, 0);
            if (n <= 0) { perror("recv"); exit(1); }
            got += (size_t)n;
        }
        *sum += buffer[SIZE - 1];
    }
    close(fd);
    return NULL;
}

int main(void) {
    int listener = socket(AF_INET, SOCK_STREAM, 0);
    int one = 1;
    setsockopt(listener, SOL_SOCKET, SO_REUSEADDR, &one, sizeof one);
    struct sockaddr_in at = { .sin_family = AF_INET, .sin_port = 0 };
    at.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    socklen_t len = sizeof at;
    if (bind(listener, (struct sockaddr*)&at, sizeof at) != 0 || listen(listener, SOMAXCONN) != 0) { perror("bind"); return 1; }
    getsockname(listener, (struct sockaddr*)&at, &len);
    port = ntohs(at.sin_port);
    pthread_t server, clients[CLIENTS];
    long sums[CLIENTS] = { 0 };
    pthread_create(&server, NULL, serve, (void*)(long)listener);
    for (int i = 0; i < CLIENTS; i++) pthread_create(&clients[i], NULL, client, &sums[i]);
    long sum = 0;
    for (int i = 0; i < CLIENTS; i++) {
        pthread_join(clients[i], NULL);
        sum += sums[i];
    }
    pthread_join(server, NULL);
    close(listener);
    printf("%ld bytes came back\n", sum);
    return 0;
}
