/* The store of the server lane in C (choices 1701, 1702): the program a C programmer writes for
   speed. THREADS threads, each with a listener of its own on the port (SO_REUSEPORT, the kernel
   deals the connections) and a loop of its own (epoll; kqueue on macOS), as nginx is built.

       resp PORT [THREADS]       THREADS: the cores this process may run on, when not given

   The protocol is a part of RESP2, Redis's: a command is an array of bulk strings, and PING, GET,
   SET and DEL are known. What every version of the store does the same way, in C, Rust, Zig and SG:

   - the keys and values in 64 maps, each behind a lock of its own, the map by the hash of the key;
   - each connection reads into a buffer of 4 KB, which grows when one command does not fit, and
     answers every whole command the buffer holds, then sends the answers together;
   - a command of more than 1,024 parts or a part of more than 1 MB is an error that closes the
     connection, so that no client makes the server hold what it likes;
   - the hash of a key is SipHash-1-3 with a key drawn at the start, as Rust's and SG's maps
     hash and as Redis does, so that no client can choose keys that fall together. */

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
#include <sys/resource.h>
#include <sys/socket.h>
#include <unistd.h>
#ifdef __linux__
#include <sys/epoll.h>
#include <sys/random.h>
#else
#include <sys/event.h>
#endif

#define SHARDS 64
#define FIRST_BUFFER 4096
#define MOST_PARTS 1024
#define MOST_PART (1 << 20)

/* ---- SipHash-1-3 */

static uint64_t sip_k0, sip_k1;

#define ROTL(x, b) (uint64_t)(((x) << (b)) | ((x) >> (64 - (b))))
#define SIPROUND                                                                                      \
    do {                                                                                              \
        v0 += v1; v1 = ROTL(v1, 13); v1 ^= v0; v0 = ROTL(v0, 32);                                     \
        v2 += v3; v3 = ROTL(v3, 16); v3 ^= v2;                                                        \
        v0 += v3; v3 = ROTL(v3, 21); v3 ^= v0;                                                        \
        v2 += v1; v1 = ROTL(v1, 17); v1 ^= v2; v2 = ROTL(v2, 32);                                     \
    } while (0)

static uint64_t siphash13(const uint8_t* in, size_t len) {
    uint64_t v0 = 0x736f6d6570736575ULL ^ sip_k0, v1 = 0x646f72616e646f6dULL ^ sip_k1;
    uint64_t v2 = 0x6c7967656e657261ULL ^ sip_k0, v3 = 0x7465646279746573ULL ^ sip_k1;
    uint64_t b = (uint64_t)len << 56;
    const uint8_t* end = in + len - (len % 8);
    for (; in != end; in += 8) {
        uint64_t m;
        memcpy(&m, in, 8);
        v3 ^= m;
        SIPROUND;
        v0 ^= m;
    }
    switch (len & 7) {
    case 7: b |= (uint64_t)in[6] << 48; /* fall through */
    case 6: b |= (uint64_t)in[5] << 40; /* fall through */
    case 5: b |= (uint64_t)in[4] << 32; /* fall through */
    case 4: b |= (uint64_t)in[3] << 24; /* fall through */
    case 3: b |= (uint64_t)in[2] << 16; /* fall through */
    case 2: b |= (uint64_t)in[1] << 8;  /* fall through */
    case 1: b |= (uint64_t)in[0]; break;
    case 0: break;
    }
    v3 ^= b;
    SIPROUND;
    v0 ^= b;
    v2 ^= 0xff;
    SIPROUND;
    SIPROUND;
    SIPROUND;
    return v0 ^ v1 ^ v2 ^ v3;
}

/* ---- a map of bytes to bytes: open addressing, probed in a line, removal by shifting back */

typedef struct {
    uint64_t hash;
    uint8_t* key; /* NULL: empty */
    uint32_t key_len;
    uint8_t* value;
    uint32_t value_len, value_cap;
} entry;

typedef struct {
    pthread_mutex_t lock;
    entry* slots;
    size_t cap, len; /* cap a power of two */
    char pad[64];
} shard;

static shard shards[SHARDS];

static entry* find(shard* s, uint64_t h, const uint8_t* key, size_t len) {
    if (s->cap == 0) return NULL;
    for (size_t i = h & (s->cap - 1);; i = (i + 1) & (s->cap - 1)) {
        entry* e = &s->slots[i];
        if (!e->key) return NULL;
        if (e->hash == h && e->key_len == len && memcmp(e->key, key, len) == 0) return e;
    }
}

static void grow(shard* s) {
    size_t cap = s->cap ? s->cap * 2 : 64;
    entry* slots = calloc(cap, sizeof(entry));
    if (!slots) abort();
    for (size_t i = 0; i < s->cap; i++) {
        entry* e = &s->slots[i];
        if (!e->key) continue;
        size_t j = e->hash & (cap - 1);
        while (slots[j].key) j = (j + 1) & (cap - 1);
        slots[j] = *e;
    }
    free(s->slots);
    s->slots = slots;
    s->cap = cap;
}

static void set(shard* s, uint64_t h, const uint8_t* key, size_t len, const uint8_t* value, size_t value_len) {
    entry* e = find(s, h, key, len);
    if (!e) {
        if ((s->len + 1) * 4 > s->cap * 3) grow(s);
        size_t i = h & (s->cap - 1);
        while (s->slots[i].key) i = (i + 1) & (s->cap - 1);
        e = &s->slots[i];
        e->hash = h;
        e->key = malloc(len ? len : 1);
        memcpy(e->key, key, len);
        e->key_len = (uint32_t)len;
        e->value = NULL;
        e->value_cap = 0;
        s->len++;
    }
    if (e->value_cap < value_len) {
        e->value = realloc(e->value, value_len);
        e->value_cap = (uint32_t)value_len;
    }
    memcpy(e->value, value, value_len);
    e->value_len = (uint32_t)value_len;
}

static bool del(shard* s, uint64_t h, const uint8_t* key, size_t len) {
    entry* e = find(s, h, key, len);
    if (!e) return false;
    free(e->key);
    free(e->value);
    size_t i = (size_t)(e - s->slots), mask = s->cap - 1;
    for (size_t j = (i + 1) & mask; s->slots[j].key; j = (j + 1) & mask) {
        size_t home = s->slots[j].hash & mask;
        /* the entry at j may move to i when i lies between its home and j */
        if (((j - home) & mask) >= ((j - i) & mask)) {
            s->slots[i] = s->slots[j];
            i = j;
        }
    }
    memset(&s->slots[i], 0, sizeof(entry));
    s->len--;
    return true;
}

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

static void put_number(conn* c, char kind, int64_t n) {
    char t[32];
    int len = snprintf(t, sizeof t, "%c%lld\r\n", kind, (long long)n);
    put(c, t, (size_t)len);
}

/* A number written in decimal and ended by CRLF, from p; how many bytes it took, 0 when the
   line is not whole yet, -1 when it is wrong. */
static long read_number(const uint8_t* p, const uint8_t* end, long long* n) {
    long long v = 0;
    const uint8_t* q = p;
    bool minus = q < end && *q == '-';
    if (minus) q++;
    const uint8_t* digits = q;
    while (q < end && *q >= '0' && *q <= '9') {
        if (v > 1000000000LL) return -1;
        v = v * 10 + (*q - '0');
        q++;
    }
    if (q + 1 >= end) return end - p > 24 ? -1 : 0;
    if (q == digits || q[0] != '\r' || q[1] != '\n') return -1;
    *n = minus ? -v : v;
    return (long)(q + 2 - p);
}

static bool same(const uint8_t* a, size_t n, const char* word) {
    if (strlen(word) != n) return false;
    for (size_t i = 0; i < n; i++)
        if ((a[i] | 0x20) != word[i]) return false;
    return true;
}

/* Runs one command; false when the connection is to be closed after the answers so far. */
static void run(conn* c, const uint8_t** part, const size_t* len, int parts) {
    const uint8_t* name = part[0];
    size_t n = len[0];
    if (same(name, n, "ping") && parts == 1) {
        put(c, "+PONG\r\n", 7);
    } else if (same(name, n, "get") && parts == 2) {
        uint64_t h = siphash13(part[1], len[1]);
        shard* s = &shards[h % SHARDS];
        pthread_mutex_lock(&s->lock);
        entry* e = find(s, h, part[1], len[1]);
        if (e) {
            put_number(c, '$', e->value_len);
            put(c, e->value, e->value_len);
            put(c, "\r\n", 2);
        } else {
            put(c, "$-1\r\n", 5);
        }
        pthread_mutex_unlock(&s->lock);
    } else if (same(name, n, "set") && parts == 3) {
        uint64_t h = siphash13(part[1], len[1]);
        shard* s = &shards[h % SHARDS];
        pthread_mutex_lock(&s->lock);
        set(s, h, part[1], len[1], part[2], len[2]);
        pthread_mutex_unlock(&s->lock);
        put(c, "+OK\r\n", 5);
    } else if (same(name, n, "del") && parts >= 2) {
        int64_t gone = 0;
        for (int i = 1; i < parts; i++) {
            uint64_t h = siphash13(part[i], len[i]);
            shard* s = &shards[h % SHARDS];
            pthread_mutex_lock(&s->lock);
            gone += del(s, h, part[i], len[i]);
            pthread_mutex_unlock(&s->lock);
        }
        put_number(c, ':', gone);
    } else {
        put(c, "-ERR unknown command or wrong number of arguments\r\n", 51);
    }
}

/* Answers every whole command in the buffer; false when the client broke the protocol. */
static bool serve(conn* c) {
    static __thread const uint8_t* part[MOST_PARTS];
    static __thread size_t len[MOST_PARTS];
    const uint8_t* p = c->in;
    const uint8_t* end = c->in + c->in_len;
    bool ok = true;
    while (p < end) {
        const uint8_t* q = p;
        if (*q != '*') { ok = false; break; }
        long long parts;
        long k = read_number(q + 1, end, &parts);
        if (k < 0 || (k > 0 && (parts < 1 || parts > MOST_PARTS))) { ok = false; break; }
        if (k == 0) break;
        q += 1 + k;
        int i = 0;
        for (; i < parts; i++) {
            if (q >= end) break;
            if (*q != '$') { ok = false; break; }
            long long n;
            k = read_number(q + 1, end, &n);
            if (k < 0 || (k > 0 && (n < 0 || n > MOST_PART))) { ok = false; break; }
            if (k == 0) break;
            if (end - (q + 1 + k) < n + 2) break;
            const uint8_t* bytes = q + 1 + k;
            if (bytes[n] != '\r' || bytes[n + 1] != '\n') { ok = false; break; }
            part[i] = bytes;
            len[i] = (size_t)n;
            q = bytes + n + 2;
        }
        if (!ok || i < parts) break;
        run(c, part, len, (int)parts);
        p = q;
    }
    size_t used = (size_t)(p - c->in);
    memmove(c->in, p, c->in_len - used);
    c->in_len -= used;
    if (!ok) put(c, "-ERR Protocol error\r\n", 21);
    return ok;
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
        perror("resp: bind");
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
        fprintf(stderr, "usage: resp PORT [THREADS]\n");
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
#ifdef __linux__
    if (getrandom(&sip_k0, 8, 0) != 8 || getrandom(&sip_k1, 8, 0) != 8) abort();
#else
    arc4random_buf(&sip_k0, 8);
    arc4random_buf(&sip_k1, 8);
#endif
    for (int i = 0; i < SHARDS; i++) pthread_mutex_init(&shards[i].lock, NULL);
    pthread_t* ts = calloc((size_t)threads, sizeof(pthread_t));
    for (int i = 1; i < threads; i++) pthread_create(&ts[i], NULL, loop, NULL);
    loop(NULL);
    return 0;
}
