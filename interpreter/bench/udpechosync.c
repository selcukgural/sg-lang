/* The C of `udpechosync`, the same as that of `udpecho`: blocking datagram sockets, a thread for the server and a
   thread for each of eight clients; each client sends 64 bytes and waits for them to come back,
   20,000 times. */
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

#define CLIENTS 8
#define ROUNDS 20000

static struct sockaddr_in server_at;

static int bound(void) {
    int fd = socket(AF_INET, SOCK_DGRAM, 0);
    struct sockaddr_in at;
    memset(&at, 0, sizeof at);
    at.sin_family = AF_INET;
    at.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    if (fd < 0 || bind(fd, (struct sockaddr*)&at, sizeof at) != 0) abort();
    return fd;
}

static void* serve(void* arg) {
    int fd = *(int*)arg;
    unsigned char buffer[64];
    for (int i = 0; i < CLIENTS * ROUNDS; i++) {
        struct sockaddr_storage from;
        socklen_t len = sizeof from;
        ssize_t n = recvfrom(fd, buffer, sizeof buffer, 0, (struct sockaddr*)&from, &len);
        if (n < 0 || sendto(fd, buffer, (size_t)n, 0, (struct sockaddr*)&from, len) != n) abort();
    }
    return NULL;
}

static void* client(void* arg) {
    long* sum = arg;
    int fd = bound();
    if (connect(fd, (struct sockaddr*)&server_at, sizeof server_at) != 0) abort();
    unsigned char message[64], buffer[64];
    memset(message, 7, sizeof message);
    for (int i = 0; i < ROUNDS; i++) {
        if (send(fd, message, sizeof message, 0) != 64) abort();
        ssize_t n = recv(fd, buffer, sizeof buffer, 0);
        if (n <= 0) abort();
        *sum += buffer[n - 1];
    }
    close(fd);
    return NULL;
}

int main(void) {
    int fd = bound();
    socklen_t len = sizeof server_at;
    getsockname(fd, (struct sockaddr*)&server_at, &len);
    pthread_t server, clients[CLIENTS];
    long sums[CLIENTS] = { 0 };
    pthread_create(&server, NULL, serve, &fd);
    for (int i = 0; i < CLIENTS; i++) pthread_create(&clients[i], NULL, client, &sums[i]);
    long sum = 0;
    for (int i = 0; i < CLIENTS; i++) {
        pthread_join(clients[i], NULL);
        sum += sums[i];
    }
    pthread_join(server, NULL);
    close(fd);
    printf("%ld bytes came back\n", sum);
    return 0;
}
