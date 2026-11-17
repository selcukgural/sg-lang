/* The C of `signals`: a handler that writes the signal into a pipe, and the program reading the
   pipe after each `raise`, a hundred thousand times. Windows has no `SIGUSR1` and no `sigaction`:
   there C's own `signal` and `raise` with `SIGTERM`, the handler set again each time, as Windows
   takes it away before it calls it, and a pipe of C's library. */
#if defined(__linux__)
#define _GNU_SOURCE
#endif
#if defined(_WIN32)
#include <fcntl.h>
#include <io.h>
#include <signal.h>
#include <stdio.h>

static int ends[2];

static void handler(int number) {
    unsigned char byte = (unsigned char)number;
    _write(ends[1], &byte, 1);
    signal(SIGTERM, handler);
}

int main(void) {
    if (_pipe(ends, 256, _O_BINARY) != 0) return 1;
    signal(SIGTERM, handler);
    long taken = 0;
    for (int i = 0; i < 100000; i++) {
        raise(SIGTERM);
        unsigned char byte = 0;
        if (_read(ends[0], &byte, 1) == 1 && byte == SIGTERM) taken++;
    }
    printf("%ld\n", taken);
    return 0;
}
#else
#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

static int ends[2];

static void handler(int number) {
    int saved = errno;
    unsigned char byte = (unsigned char)number;
    ssize_t written = write(ends[1], &byte, 1);
    (void)written;
    errno = saved;
}

int main(void) {
    if (pipe(ends) != 0) return 1;
    fcntl(ends[1], F_SETFL, O_NONBLOCK);
    struct sigaction a;
    memset(&a, 0, sizeof a);
    a.sa_handler = handler;
    sigemptyset(&a.sa_mask);
    a.sa_flags = SA_RESTART;
    sigaction(SIGUSR1, &a, NULL);
    long taken = 0;
    for (int i = 0; i < 100000; i++) {
        raise(SIGUSR1);
        unsigned char byte = 0;
        while (read(ends[0], &byte, 1) < 0 && errno == EINTR) {}
        if (byte == SIGUSR1) taken++;
    }
    printf("%ld\n", taken);
    return 0;
}
#endif
