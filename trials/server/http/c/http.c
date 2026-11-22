/* The HTTP server of the server lane in C (choice 1706): GET /ping answered with "pong", over
   HTTP/1.1 with connections kept open. The program a C programmer writes for speed, built as
   `../../resp/c/resp.c` is: THREADS threads, each with a listener of its own on the port
   (SO_REUSEPORT) and a loop of its own (epoll; kqueue on macOS).

       http PORT [THREADS]       THREADS: the cores this process may run on, when not given

   What every hand-written version does the same way, in C and SG (Rust is hyper's, Zig is
   `std.http.Server`'s):

   - the request line and every header are read and checked: a method, a target, HTTP/1.1 or
     HTTP/1.0, headers of `name: value`; `Host` is needed in HTTP/1.1; `Connection` says whether the
     connection is kept (HTTP/1.1 keeps it unless it says `close`, HTTP/1.0 only when it says
     `keep-alive`); a body of `Content-Length` bytes is read and let go; `Transfer-Encoding` is
     answered 501 and the connection closed;
   - a head of more than 8 KB is answered 431, anything else wrong 400, both closing the connection;
   - GET or HEAD of /ping is 200 with `pong`, another target 404, another method 405;
   - the answer has `content-length` and `content-type`, as Zig's has (hyper adds `date`);
   - each connection reads into a buffer of 4 KB and answers every whole request it holds, then
     sends the answers together. */

#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <netinet/in.h>
#include <netinet/tcp.h>
#include <pthread.h>
#include <sched.h>
#include <signal.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>
#include <sys/resource.h>
#include <sys/socket.h>
#include <unistd.h>
#ifdef __linux__
#include <sys/epoll.h>
#else
#include <sys/event.h>
#endif

#define FIRST_BUFFER 4096
#define MOST_HEAD 8192
#define MOST_BODY (1 << 20)

/* ---- connections */

typedef struct {
    int fd;
    uint8_t* in;
    size_t in_cap, in_len;
    uint8_t* out;
    size_t out_cap, out_len, out_at;
    bool waiting_out;
} conn;

static void reserve(uint8_t** p, size_t* cap, size_t need) {
    if (*cap >= need) return;
    size_t c = *cap ? *cap : 64;
    while (c < need) c *= 2;
    *p = realloc(*p, c);
    if (!*p) abort();
    *cap = c;
}

static void put(conn* c, const void* bytes, size_t n) {
    reserve(&c->out, &c->out_cap, c->out_len + n);
    memcpy(c->out + c->out_len, bytes, n);
    c->out_len += n;
}

/* ---- HTTP */

static bool token_char(uint8_t c) {
    return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || strchr("!#$%&'*+-.^_`|~", c) != NULL;
}

static bool has_token(const uint8_t* v, size_t n, const char* word) {
    /* a list of tokens separated by commas, any of them `word`, in any case */
    size_t w = strlen(word), i = 0;
    while (i < n) {
        while (i < n && (v[i] == ' ' || v[i] == '\t' || v[i] == ',')) i++;
        size_t start = i;
        while (i < n && v[i] != ',') i++;
        size_t end = i;
        while (end > start && (v[end - 1] == ' ' || v[end - 1] == '\t')) end--;
        if (end - start == w && strncasecmp((const char*)v + start, word, w) == 0) return true;
    }
    return false;
}

static void answer(conn* c, int status, bool keep, bool head_only) {
    const char* line;
    const char* body;
    const char* extra = "";
    switch (status) {
    case 200: line = "200 OK"; body = "pong"; break;
    case 404: line = "404 Not Found"; body = "not found"; break;
    case 405: line = "405 Method Not Allowed"; body = "method not allowed"; extra = "allow: GET, HEAD\r\n"; break;
    case 431: line = "431 Request Header Fields Too Large"; body = "head too large"; break;
    case 501: line = "501 Not Implemented"; body = "not implemented"; break;
    default: line = "400 Bad Request"; body = "bad request"; break;
    }
    char head[256];
    int n = snprintf(head, sizeof head, "HTTP/1.1 %s\r\n%scontent-length: %zu\r\ncontent-type: text/plain\r\n%s\r\n",
                     line, keep ? "" : "connection: close\r\n", strlen(body), extra);
    put(c, head, (size_t)n);
    if (!head_only) put(c, body, strlen(body));
}

/* Answers every whole request in the buffer; false when the connection is to be closed after the
   answers so far. */
static bool serve(conn* c) {
    const uint8_t* p = c->in;
    const uint8_t* end = c->in + c->in_len;
    while (p < end) {
        size_t avail = (size_t)(end - p);
        const uint8_t* stop = memmem(p, avail < MOST_HEAD ? avail : MOST_HEAD, "\r\n\r\n", 4);
        if (!stop) {
            if (avail >= MOST_HEAD) { answer(c, 431, false, false); return false; }
            break;
        }
        const uint8_t* head_end = stop + 4;
        /* the request line */
        const uint8_t* q = p;
        const uint8_t* method = q;
        while (q < stop && token_char(*q)) q++;
        size_t method_len = (size_t)(q - method);
        if (method_len == 0 || q >= stop || *q != ' ') { answer(c, 400, false, false); return false; }
        const uint8_t* target = ++q;
        while (q < stop && *q > ' ' && *q < 0x7f) q++;
        size_t target_len = (size_t)(q - target);
        if (target_len == 0 || q >= stop || *q != ' ') { answer(c, 400, false, false); return false; }
        q++;
        int minor;
        if (stop - q >= 10 && memcmp(q, "HTTP/1.", 7) == 0 && (q[7] == '0' || q[7] == '1') && q[8] == '\r' && q[9] == '\n') {
            minor = q[7] - '0';
            q += 10;
        } else if (stop - q == 8 && memcmp(q, "HTTP/1.", 7) == 0 && (q[7] == '0' || q[7] == '1')) {
            minor = q[7] - '0';
            q += 8; /* the line ends where the head ends */
        } else {
            answer(c, 400, false, false);
            return false;
        }
        /* the headers */
        bool keep = minor == 1, host = false, chunked = false;
        long long length = 0;
        bool has_length = false;
        while (q < stop) {
            const uint8_t* name = q;
            while (q < stop && token_char(*q)) q++;
            size_t name_len = (size_t)(q - name);
            if (name_len == 0 || q >= stop || *q != ':') { answer(c, 400, false, false); return false; }
            q++;
            while (q < stop && (*q == ' ' || *q == '\t')) q++;
            const uint8_t* value = q;
            const uint8_t* eol = memmem(q, (size_t)(stop + 2 - q), "\r\n", 2);
            if (!eol) eol = stop;
            const uint8_t* value_end = eol;
            while (value_end > value && (value_end[-1] == ' ' || value_end[-1] == '\t')) value_end--;
            for (const uint8_t* v = value; v < value_end; v++)
                if ((*v < ' ' && *v != '\t') || *v == 0x7f) { answer(c, 400, false, false); return false; }
            size_t value_len = (size_t)(value_end - value);
            if (name_len == 4 && strncasecmp((const char*)name, "host", 4) == 0) {
                if (host) { answer(c, 400, false, false); return false; }
                host = true;
            } else if (name_len == 10 && strncasecmp((const char*)name, "connection", 10) == 0) {
                if (has_token(value, value_len, "close")) keep = false;
                else if (has_token(value, value_len, "keep-alive")) keep = true;
            } else if (name_len == 14 && strncasecmp((const char*)name, "content-length", 14) == 0) {
                long long v = 0;
                if (value_len == 0 || value_len > 9) { answer(c, 400, false, false); return false; }
                for (size_t i = 0; i < value_len; i++) {
                    if (value[i] < '0' || value[i] > '9') { answer(c, 400, false, false); return false; }
                    v = v * 10 + (value[i] - '0');
                }
                if (has_length && v != length) { answer(c, 400, false, false); return false; }
                has_length = true;
                length = v;
            } else if (name_len == 17 && strncasecmp((const char*)name, "transfer-encoding", 17) == 0) {
                chunked = true;
            }
            q = eol + 2;
        }
        if (chunked) { answer(c, 501, false, false); return false; }
        if (minor == 1 && !host) { answer(c, 400, false, false); return false; }
        if (length > MOST_BODY) { answer(c, 400, false, false); return false; }
        if (end - head_end < length) break; /* the body is not all here yet */
        bool get = method_len == 3 && memcmp(method, "GET", 3) == 0;
        bool head = method_len == 4 && memcmp(method, "HEAD", 4) == 0;
        int status = !(get || head) ? 405 : (target_len == 5 && memcmp(target, "/ping", 5) == 0) ? 200 : 404;
        answer(c, status, keep, head);
        p = head_end + length;
        if (!keep) {
            c->in_len = 0;
            return false;
        }
    }
    size_t used = (size_t)(p - c->in);
    memmove(c->in, p, c->in_len - used);
    c->in_len -= used;
    return true;
}

/* ---- the loop */

static int port_number;

static int listener(void) {
    int fd = socket(AF_INET, SOCK_STREAM, 0);
    int one = 1;
    setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &one, sizeof one);
    setsockopt(fd, SOL_SOCKET, SO_REUSEPORT, &one, sizeof one);
    struct sockaddr_in at = {.sin_family = AF_INET, .sin_port = htons((uint16_t)port_number)};
    at.sin_addr.s_addr = htonl(INADDR_ANY);
    if (bind(fd, (struct sockaddr*)&at, sizeof at) != 0 || listen(fd, SOMAXCONN) != 0) {
        perror("http: bind");
        exit(1);
    }
    fcntl(fd, F_SETFL, fcntl(fd, F_GETFL) | O_NONBLOCK);
    return fd;
}

/* A connection is watched for what comes in, or, while answers wait to be sent, only for room to
   send them: a client that does not read what it asked for is not read either. */
static void watch(int poll, int fd, void* data, bool out, bool add) {
#ifdef __linux__
    struct epoll_event ev = {.events = out ? EPOLLOUT : EPOLLIN, .data.ptr = data};
    epoll_ctl(poll, add ? EPOLL_CTL_ADD : EPOLL_CTL_MOD, fd, &ev);
#else
    (void)add;
    struct kevent ev[2];
    EV_SET(&ev[0], fd, EVFILT_READ, EV_ADD | (out ? EV_DISABLE : EV_ENABLE), 0, 0, data);
    EV_SET(&ev[1], fd, EVFILT_WRITE, EV_ADD | (out ? EV_ENABLE : EV_DISABLE), 0, 0, data);
    kevent(poll, ev, 2, NULL, 0, NULL);
#endif
}

static void drop(conn* c) {
    close(c->fd);
    free(c->in);
    free(c->out);
    free(c);
}

/* Sends what is waiting; false when the connection failed. */
static bool flush(int poll, conn* c) {
    while (c->out_at < c->out_len) {
        ssize_t n = send(c->fd, c->out + c->out_at, c->out_len - c->out_at, 0);
        if (n < 0) {
            if (errno == EAGAIN || errno == EWOULDBLOCK) {
                if (!c->waiting_out) watch(poll, c->fd, c, true, false);
                c->waiting_out = true;
                return true;
            }
            if (errno == EINTR) continue;
            return false;
        }
        c->out_at += (size_t)n;
    }
    c->out_at = c->out_len = 0;
    if (c->waiting_out) watch(poll, c->fd, c, false, false);
    c->waiting_out = false;
    return true;
}

static void readable(int poll, conn* c) {
    if (c->in_len == c->in_cap) reserve(&c->in, &c->in_cap, c->in_cap * 2);
    ssize_t n = recv(c->fd, c->in + c->in_len, c->in_cap - c->in_len, 0);
    if (n == 0 || (n < 0 && errno != EAGAIN && errno != EWOULDBLOCK && errno != EINTR)) {
        drop(c);
        return;
    }
    if (n < 0) return;
    c->in_len += (size_t)n;
    if (!serve(c)) {
        flush(poll, c);
        drop(c);
        return;
    }
    if (!flush(poll, c)) drop(c);
}

static void* loop(void* arg) {
    (void)arg;
    int lfd = listener();
#ifdef __linux__
    int poll = epoll_create1(0);
    struct epoll_event evs[256];
#else
    int poll = kqueue();
    struct kevent evs[256];
#endif
    static char the_listener;
    watch(poll, lfd, &the_listener, false, true);
    for (;;) {
#ifdef __linux__
        int k = epoll_wait(poll, evs, 256, -1);
#else
        int k = kevent(poll, NULL, 0, evs, 256, NULL);
#endif
        for (int i = 0; i < k; i++) {
#ifdef __linux__
            void* data = evs[i].data.ptr;
            bool in = evs[i].events & (EPOLLIN | EPOLLHUP | EPOLLERR), out = evs[i].events & EPOLLOUT;
#else
            void* data = evs[i].udata;
            bool in = evs[i].filter == EVFILT_READ, out = evs[i].filter == EVFILT_WRITE;
#endif
            if (data == &the_listener) {
                for (;;) {
                    int fd = accept(lfd, NULL, NULL);
                    if (fd < 0) break;
                    fcntl(fd, F_SETFL, fcntl(fd, F_GETFL) | O_NONBLOCK);
                    int one = 1;
                    setsockopt(fd, IPPROTO_TCP, TCP_NODELAY, &one, sizeof one);
                    conn* c = calloc(1, sizeof(conn));
                    c->fd = fd;
                    c->in = malloc(FIRST_BUFFER);
                    c->in_cap = FIRST_BUFFER;
                    watch(poll, fd, c, false, true);
                }
                continue;
            }
            conn* c = data;
            if (out) {
                if (!flush(poll, c)) drop(c);
            } else if (in) {
                readable(poll, c);
            }
        }
    }
    return NULL;
}

static int cores(void) {
#ifdef __linux__
    cpu_set_t set;
    if (sched_getaffinity(0, sizeof set, &set) == 0) return CPU_COUNT(&set);
#endif
    long n = sysconf(_SC_NPROCESSORS_ONLN);
    return n > 0 ? (int)n : 1;
}

int main(int argc, char** argv) {
    if (argc < 2 || argc > 3) {
        fprintf(stderr, "usage: http PORT [THREADS]\n");
        return 2;
    }
    port_number = atoi(argv[1]);
    int threads = argc == 3 ? atoi(argv[2]) : cores();
    signal(SIGPIPE, SIG_IGN);
    struct rlimit r;
    if (getrlimit(RLIMIT_NOFILE, &r) == 0) {
        r.rlim_cur = r.rlim_max;
#ifdef __APPLE__
        if (r.rlim_cur > 10240) r.rlim_cur = 10240;
#endif
        setrlimit(RLIMIT_NOFILE, &r);
    }
    pthread_t* ts = calloc((size_t)threads, sizeof(pthread_t));
    for (int i = 1; i < threads; i++) pthread_create(&ts[i], NULL, loop, NULL);
    loop(NULL);
    return 0;
}
