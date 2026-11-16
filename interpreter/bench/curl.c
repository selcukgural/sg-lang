/* link: -lcurl */
/* The same program written by hand in C: the server on a thread, the body copied by the
   callback into a buffer that grows. */
#define _GNU_SOURCE
#include <arpa/inet.h>
#include <curl/curl.h>
#include <netinet/in.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <unistd.h>

#define COUNT 1000
#define SIZE 1048576

typedef struct { uint8_t* ptr; size_t len, cap; } Got;

static size_t take(char* data, size_t size, size_t count, void* state) {
    Got* got = state;
    size_t n = size * count;
    if (got->len + n > got->cap) {
        size_t cap = got->cap ? got->cap * 2 : 16;
        while (cap < got->len + n) cap *= 2;
        got->ptr = realloc(got->ptr, cap);
        if (!got->ptr) abort();
        got->cap = cap;
    }
    memcpy(got->ptr + got->len, data, n);
    got->len += n;
    return n;
}

static int listener;

static void send_all(int fd, const void* data, size_t n) {
    const char* at = data;
    while (n) {
        ssize_t sent = send(fd, at, n, 0);
        if (sent <= 0) abort();
        at += sent;
        n -= (size_t)sent;
    }
}

static void* serve(void* unused) {
    (void)unused;
    uint8_t* body = malloc(SIZE);
    for (int i = 0; i < SIZE; i++) body[i] = (uint8_t)(i % 251);
    const char* head = "HTTP/1.1 200 OK\r\nContent-Length: 1048576\r\n\r\n";
    int conn = accept(listener, NULL, NULL);
    char buffer[4096];
    size_t have = 0;
    for (int i = 0; i < COUNT; i++) {
        /* A request ends with an empty line. */
        for (;;) {
            char* end = memmem(buffer, have, "\r\n\r\n", 4);
            if (end) {
                size_t used = (size_t)(end + 4 - buffer);
                memmove(buffer, buffer + used, have - used);
                have -= used;
                break;
            }
            ssize_t got = recv(conn, buffer + have, sizeof buffer - have, 0);
            if (got <= 0) abort();
            have += (size_t)got;
        }
        send_all(conn, head, strlen(head));
        send_all(conn, body, SIZE);
    }
    close(conn);
    free(body);
    return NULL;
}

int main(void) {
    listener = socket(AF_INET, SOCK_STREAM, 0);
    struct sockaddr_in at = { .sin_family = AF_INET, .sin_port = 0 };
    at.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    if (bind(listener, (struct sockaddr*)&at, sizeof at) || listen(listener, 16)) abort();
    socklen_t length = sizeof at;
    getsockname(listener, (struct sockaddr*)&at, &length);
    char url[64];
    snprintf(url, sizeof url, "http://127.0.0.1:%d/", ntohs(at.sin_port));
    pthread_t server;
    pthread_create(&server, NULL, serve, NULL);
    curl_global_init(CURL_GLOBAL_DEFAULT);
    CURL* handle = curl_easy_init();
    Got got = { 0 };
    curl_easy_setopt(handle, CURLOPT_URL, url);
    curl_easy_setopt(handle, CURLOPT_WRITEFUNCTION, take);
    curl_easy_setopt(handle, CURLOPT_WRITEDATA, &got);
    long long total = 0, sum = 0;
    for (int i = 0; i < COUNT; i++) {
        got.len = 0;
        if (curl_easy_perform(handle) != CURLE_OK) abort();
        total += (long long)got.len;
        sum += got.ptr[got.len - 1];
    }
    curl_easy_cleanup(handle);
    curl_global_cleanup();
    pthread_join(server, NULL);
    close(listener);
    free(got.ptr);
    printf("%lld bytes, %lld\n", total, sum);
    return 0;
}
