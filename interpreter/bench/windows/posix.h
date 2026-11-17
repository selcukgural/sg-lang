/* What the programs of C written by hand take of POSIX, on Windows, where they include this
   instead of POSIX's headers: threads of `_beginthreadex` with the stack they ask for and locks of
   `SRWLOCK`, as SG's runtime has them there; sockets of Winsock, started before `main`; and
   `getline`, `memmem` and `popen`, which Windows' C library has not or has under other names. */
#ifndef BENCH_POSIX_ON_WINDOWS
#define BENCH_POSIX_ON_WINDOWS

#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <winsock2.h>
#include <ws2tcpip.h>
#include <windows.h>
#include <fcntl.h>
#include <io.h>
#include <process.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#pragma comment(lib, "ws2_32")

typedef long long ssize_t;

/* Threads and locks. */
typedef HANDLE pthread_t;
typedef struct { size_t stack; } pthread_attr_t;
typedef SRWLOCK pthread_mutex_t;

static inline int pthread_attr_init(pthread_attr_t* attr) { attr->stack = 0; return 0; }
static inline int pthread_attr_destroy(pthread_attr_t* attr) { (void)attr; return 0; }
static inline int pthread_attr_setstacksize(pthread_attr_t* attr, size_t stack) { attr->stack = stack; return 0; }

typedef struct { void* (*body)(void*); void* arg; } posix_start;

static unsigned __stdcall posix_thread(void* p) {
    posix_start start = *(posix_start*)p;
    free(p);
    start.body(start.arg);
    return 0;
}

static inline int pthread_create(pthread_t* thread, const pthread_attr_t* attr, void* (*body)(void*), void* arg) {
    posix_start* start = malloc(sizeof *start);
    if (!start) return 1;
    *start = (posix_start){ body, arg };
    unsigned stack = attr ? (unsigned)attr->stack : 0;
    uintptr_t handle = _beginthreadex(NULL, stack, posix_thread, start, stack ? STACK_SIZE_PARAM_IS_A_RESERVATION : 0, NULL);
    if (!handle) {
        free(start);
        return 1;
    }
    *thread = (HANDLE)handle;
    return 0;
}

static inline int pthread_join(pthread_t thread, void** result) {
    (void)result;
    WaitForSingleObject(thread, INFINITE);
    CloseHandle(thread);
    return 0;
}

static inline int pthread_mutex_init(pthread_mutex_t* lock, const void* attr) { (void)attr; InitializeSRWLock(lock); return 0; }
static inline int pthread_mutex_destroy(pthread_mutex_t* lock) { (void)lock; return 0; }
static inline int pthread_mutex_lock(pthread_mutex_t* lock) { AcquireSRWLockExclusive(lock); return 0; }
static inline int pthread_mutex_unlock(pthread_mutex_t* lock) { ReleaseSRWLockExclusive(lock); return 0; }

/* Sockets: Winsock is started before `main`, and a socket is closed with `closesocket`. */
__attribute__((constructor)) static void posix_sockets(void) {
    WSADATA data;
    if (WSAStartup(MAKEWORD(2, 2), &data) != 0) abort();
}
#define close(fd) closesocket(fd)

/* `getline`, from `fgets` into a buffer that grows. */
static inline ssize_t getline(char** line, size_t* cap, FILE* in) {
    if (!*line) {
        *cap = 256;
        *line = malloc(*cap);
        if (!*line) abort();
    }
    size_t len = 0;
    while (fgets(*line + len, (int)(*cap - len), in)) {
        len += strlen(*line + len);
        if ((*line)[len - 1] == '\n') return (ssize_t)len;
        if (len + 1 == *cap) {
            *cap *= 2;
            *line = realloc(*line, *cap);
            if (!*line) abort();
        }
    }
    return len ? (ssize_t)len : -1;
}

static inline void* memmem(const void* haystack, size_t n, const void* needle, size_t m) {
    const char* at = haystack;
    for (size_t i = 0; m <= n && i <= n - m; i++) {
        if (memcmp(at + i, needle, m) == 0) return (void*)(at + i);
    }
    return NULL;
}

/* `popen` through `cmd.exe`, as POSIX's goes through `sh`; `pclose` gives the code itself. */
#define popen _popen
#define pclose _pclose
#define WEXITSTATUS(status) (status)

#endif
