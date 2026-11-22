/* The load of the server lane (choices 1700, 1706): one program in C that drives every server the
   same way, so that the servers are measured and not their clients.

       load resp|http HOST:PORT [options]

   Closed loop (the default): each connection keeps PIPELINE requests in flight and sends the next
   as soon as an answer comes; what is measured is how many answers come in a second. Open loop
   (--rate R): the requests go out at R a second in all, spread evenly over the connections, and the
   time of each is counted from when it was due to go, not from when it went, so that a server that
   stalls is charged for every request that waited behind the stall (the mistake of measuring from
   the send is called coordinated omission; wrk2 counts the same way). A connection sends its next
   request when the answer to the last has come, at once if it is late.

   Options:
     --conns N        connections that send (50)
     --threads T      threads of this program, each with its own connections (4)
     --pipeline P     requests in flight on each connection, closed loop only (1)
     --rate R         requests a second in all: open loop (0, closed)
     --duration S     seconds measured (10), after --warmup S seconds not counted (2)
     --value BYTES    the value of a SET (64)
     --keys K         the keys, key:000000000000 and on, 16 bytes each (10000)
     --sets PERCENT   the share of SET among GET and SET (10)
     --close          HTTP: a connection for each request, `Connection: close`
     --idle N         also N connections that are opened first, each asked once, then left silent
     --sources K      the connections come from 127.0.0.1 to 127.0.0.K, for more than one range of ports
     --fill           RESP: SET every key once, then end
     --check          every answer held to what it must be, byte for byte
     --report S       also a line of JSON for each S seconds of the measured time, after the end
     --reconnect N    each connection closed and opened again after N answers (one at a time)
     --trickle MS     RESP: once answered, each idle connection begins a SET of a value of 1 MB and sends
                      one byte of it every MS milliseconds, as a slow client does (slowloris); one the
                      server closes is counted as cut, not as failed (choice 1724)

   It writes one line of JSON on the standard output when it ends, and `ready` on its own line once
   every idle connection is open and has been answered (for whoever measures the server meanwhile).
   Exit code 0, or 1 when a connection failed or an answer was wrong (the line says how many), or 2
   for a wrong use. Linux (epoll) and macOS (kqueue). */

#define _GNU_SOURCE
#include <arpa/inet.h>
#include <errno.h>
#include <fcntl.h>
#include <inttypes.h>
#include <math.h>
#include <netinet/in.h>
#include <netinet/tcp.h>
#include <pthread.h>
#include <signal.h>
#include <stdatomic.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/resource.h>
#include <sys/socket.h>
#include <sys/time.h>
#include <sys/uio.h>
#include <time.h>
#include <unistd.h>
#ifdef __linux__
#include <sys/epoll.h>
#else
#include <sys/event.h>
#endif

/* ---- options */

enum { RESP, HTTP };
static int protocol;
static struct sockaddr_in target;
static int conns = 50, threads = 4, pipeline = 1, value_size = 64, keys = 10000, sets = 10;
static double rate = 0, duration = 10, warmup = 2;
static bool close_each, fill, check;
static int idle = 0, sources = 1, reconnect = 0;
static double trickle = 0;
static double report = 0;

/* ---- time */

static int64_t now_ns(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return (int64_t)t.tv_sec * 1000000000 + t.tv_nsec;
}

/* ---- the histogram: below 512 ns each nanosecond, above it 256 steps for each power of two, so a
   value is kept to 0.4% at worst, up to 2^40 ns. */

#define SUB 512
#define STEPS 256
#define BUCKETS (SUB + 40 * STEPS)
typedef struct {
    uint64_t counts[BUCKETS];
    uint64_t total, max;
    double sum;
} histogram;

static int bucket(uint64_t v) {
    if (v < SUB) return (int)v;
    int e = 63 - __builtin_clzll(v) - 8; /* v >> e lies in [256, 512) */
    int i = SUB + (e - 1) * STEPS + (int)(v >> e) - STEPS;
    return i < BUCKETS ? i : BUCKETS - 1;
}

static uint64_t bucket_value(int i) {
    if (i < SUB) return (uint64_t)i;
    int e = (i - SUB) / STEPS + 1;
    uint64_t m = (uint64_t)((i - SUB) % STEPS + STEPS);
    return m << e;
}

static void record(histogram* h, uint64_t ns) {
    h->counts[bucket(ns)]++;
    h->total++;
    h->sum += (double)ns;
    if (ns > h->max) h->max = ns;
}

static double percentile(const histogram* h, double p) {
    if (h->total == 0) return 0;
    uint64_t want = (uint64_t)ceil(p / 100.0 * (double)h->total);
    if (want == 0) want = 1;
    uint64_t seen = 0;
    for (int i = 0; i < BUCKETS; i++) {
        seen += h->counts[i];
        if (seen >= want) return (double)bucket_value(i) / 1000.0;
    }
    return (double)h->max / 1000.0;
}

/* ---- connections */

enum { OP_GET, OP_SET, OP_PING, OP_HTTP };
#define MOST_PIPELINE 256

typedef struct {
    int64_t due; /* when it was due to go (open loop) or went (closed) */
    int key;
    uint8_t op;
} pending;

typedef struct conn {
    int fd;
    int index;
    bool is_idle, open, dead, connecting, counted_ready;
    bool trickling; /* --trickle: the head of its SET is sent */
    /* what waits to be sent, when the socket took only part */
    char* out;
    size_t out_len, out_cap, out_at;
    /* requests sent and not answered, in order */
    pending ring[MOST_PIPELINE];
    int head, count;
    /* the answer being read */
    int state;
    char line[8192];
    int line_len;
    int64_t left;       /* bytes of a body still to come */
    int64_t body_at;    /* how far into the body */
    int64_t body_len;
    int status;
    bool saw_close;
    /* open loop: when the next request is due */
    int64_t next_due;
    int64_t interval;
    int64_t opened_at; /* --close: when its connect began, which is when its request was due */
    int heap_at; /* where it is in the heap of the thread, -1 when not there */
    uint64_t served; /* answers on this connection, for --reconnect */
    uint32_t rand;
} conn;

typedef struct {
    int id;
    int poll;
    conn** all;
    int n;
    conn** heap; /* connections waiting to send, by next_due */
    int heap_n;
    histogram* h;
    histogram** intervals; /* --report: one for each interval, made when first used */
    int interval_n;
    uint64_t answers, errors, wrong, late, cut;
    int64_t trickle_next;
    int64_t start, measure_from, measure_to;
    int idle_ready;
    char* scratch;
    size_t scratch_cap;
    char* in;
} worker;

/* Byte j of the value of key k is alphabet[k % 26 + j]: 'a' + (k + j) % 26. */
static char alphabet[26 + 65536 + 64];
static atomic_int idle_answered;
static atomic_int failed_connects;
static int idle_total;
static volatile int stop_all;

static void die(const char* what) {
    fprintf(stderr, "load: %s: %s\n", what, strerror(errno));
    exit(1);
}

static void usage(const char* why) {
    fprintf(stderr, "load: %s\nusage: load resp|http HOST:PORT [--conns N] [--threads T] [--pipeline P] [--rate R]\n"
                    "  [--duration S] [--warmup S] [--value BYTES] [--keys K] [--sets PERCENT] [--close]\n"
                    "  [--idle N] [--trickle MS] [--sources K] [--fill] [--check]\n", why);
    exit(2);
}

static uint32_t next_rand(uint32_t* s) {
    uint32_t x = *s;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    return *s = x;
}

/* ---- the poller */

static int poll_new(void) {
#ifdef __linux__
    int p = epoll_create1(0);
#else
    int p = kqueue();
#endif
    if (p < 0) die("poll");
    return p;
}

static void poll_add(int p, conn* c, bool want_out) {
#ifdef __linux__
    struct epoll_event ev = {.events = EPOLLIN | EPOLLRDHUP | (want_out ? EPOLLOUT : 0), .data.ptr = c};
    if (epoll_ctl(p, EPOLL_CTL_ADD, c->fd, &ev) != 0) die("epoll_ctl");
#else
    struct kevent ev[2];
    EV_SET(&ev[0], c->fd, EVFILT_READ, EV_ADD, 0, 0, c);
    EV_SET(&ev[1], c->fd, EVFILT_WRITE, EV_ADD | (want_out ? 0 : EV_DISABLE), 0, 0, c);
    if (kevent(p, ev, 2, NULL, 0, NULL) != 0) die("kevent");
#endif
}

static void poll_out(int p, conn* c, bool want_out) {
#ifdef __linux__
    struct epoll_event ev = {.events = EPOLLIN | EPOLLRDHUP | (want_out ? EPOLLOUT : 0), .data.ptr = c};
    if (epoll_ctl(p, EPOLL_CTL_MOD, c->fd, &ev) != 0) die("epoll_ctl");
#else
    struct kevent ev;
    EV_SET(&ev, c->fd, EVFILT_WRITE, want_out ? EV_ENABLE : EV_DISABLE, 0, 0, c);
    if (kevent(p, &ev, 1, NULL, 0, NULL) != 0) die("kevent");
#endif
}

/* ---- the heap of connections by when they are due */

static void heap_swap(worker* w, int a, int b) {
    conn* t = w->heap[a];
    w->heap[a] = w->heap[b];
    w->heap[b] = t;
    w->heap[a]->heap_at = a;
    w->heap[b]->heap_at = b;
}

static void heap_push(worker* w, conn* c) {
    int i = w->heap_n++;
    w->heap[i] = c;
    c->heap_at = i;
    while (i > 0 && w->heap[(i - 1) / 2]->next_due > w->heap[i]->next_due) {
        heap_swap(w, i, (i - 1) / 2);
        i = (i - 1) / 2;
    }
}

static conn* heap_pop(worker* w) {
    conn* top = w->heap[0];
    w->heap_n--;
    top->heap_at = -1;
    if (w->heap_n > 0) {
        w->heap[0] = w->heap[w->heap_n];
        w->heap[0]->heap_at = 0;
        int i = 0;
        for (;;) {
            int l = 2 * i + 1, r = l + 1, m = i;
            if (l < w->heap_n && w->heap[l]->next_due < w->heap[m]->next_due) m = l;
            if (r < w->heap_n && w->heap[r]->next_due < w->heap[m]->next_due) m = r;
            if (m == i) break;
            heap_swap(w, i, m);
            i = m;
        }
    }
    return top;
}

/* ---- opening */

static void open_conn(worker* w, conn* c) {
    int fd = socket(AF_INET, SOCK_STREAM, 0);
    if (fd < 0) die("socket");
    int one = 1;
    setsockopt(fd, IPPROTO_TCP, TCP_NODELAY, &one, sizeof one);
    if (sources > 1) {
#ifdef IP_BIND_ADDRESS_NO_PORT
        setsockopt(fd, IPPROTO_IP, IP_BIND_ADDRESS_NO_PORT, &one, sizeof one);
#endif
        struct sockaddr_in from = {.sin_family = AF_INET};
        from.sin_addr.s_addr = htonl(0x7f000001u + (uint32_t)(c->index % sources));
        if (bind(fd, (struct sockaddr*)&from, sizeof from) != 0) die("bind");
    }
    fcntl(fd, F_SETFL, fcntl(fd, F_GETFL) | O_NONBLOCK);
    c->fd = fd;
    c->opened_at = now_ns();
    c->open = true;
    c->connecting = true;
    c->state = 0;
    c->line_len = 0;
    c->head = c->count = 0;
    c->out_len = c->out_at = 0;
    c->saw_close = false;
    if (connect(fd, (struct sockaddr*)&target, sizeof target) != 0 && errno != EINPROGRESS) {
        atomic_fetch_add(&failed_connects, 1);
        die("connect");
    }
    poll_add(w->poll, c, true);
}

static void close_conn(worker* w, conn* c) {
    (void)w;
    close(c->fd); /* also takes it off the poller */
    c->fd = -1;
    c->open = false;
}

static void fail_conn(worker* w, conn* c, const char* why) {
    /* a slow client the server closed: what --trickle is there to see */
    if (trickle > 0 && c->is_idle && c->counted_ready && !c->dead) {
        w->cut++;
        c->dead = true;
        if (c->open) close_conn(w, c);
        return;
    }
    if (!c->dead) {
        static atomic_int told;
        if (atomic_fetch_add(&told, 1) < 5) fprintf(stderr, "load: connection %d: %s\n", c->index, why);
    }
    w->errors++;
    c->dead = true;
    if (c->open) close_conn(w, c);
}

/* ---- requests */

static void key_text(char* to, int key) {
    snprintf(to, 17, "key:%012d", key);
}

static size_t make_request(worker* w, uint8_t op, int key) {
    size_t need = (size_t)value_size + 256;
    if (w->scratch_cap < need) {
        w->scratch = realloc(w->scratch, need);
        w->scratch_cap = need;
    }
    char* s = w->scratch;
    char k[17];
    switch (op) {
    case OP_PING:
        memcpy(s, "*1\r\n$4\r\nPING\r\n", 14);
        return 14;
    case OP_GET:
        key_text(k, key);
        return (size_t)sprintf(s, "*2\r\n$3\r\nGET\r\n$16\r\n%s\r\n", k);
    case OP_SET: {
        key_text(k, key);
        int n = sprintf(s, "*3\r\n$3\r\nSET\r\n$16\r\n%s\r\n$%d\r\n", k, value_size);
        memcpy(s + n, alphabet + key % 26, (size_t)value_size);
        memcpy(s + n + value_size, "\r\n", 2);
        return (size_t)n + (size_t)value_size + 2;
    }
    default:
        if (close_each) {
            static const char r[] = "GET /ping HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
            memcpy(s, r, sizeof r - 1);
            return sizeof r - 1;
        } else {
            static const char r[] = "GET /ping HTTP/1.1\r\nHost: localhost\r\n\r\n";
            memcpy(s, r, sizeof r - 1);
            return sizeof r - 1;
        }
    }
}

static void flush_out(worker* w, conn* c) {
    while (c->out_at < c->out_len) {
        ssize_t n = send(c->fd, c->out + c->out_at, c->out_len - c->out_at, 0);
        if (n < 0) {
            if (errno == EAGAIN || errno == EWOULDBLOCK) {
                poll_out(w->poll, c, true);
                return;
            }
            fail_conn(w, c, strerror(errno));
            return;
        }
        c->out_at += (size_t)n;
    }
    c->out_at = c->out_len = 0;
}

static void send_request(worker* w, conn* c, uint8_t op, int key, int64_t due) {
    if (c->count == MOST_PIPELINE) abort();
    size_t len = make_request(w, op, key);
    pending* p = &c->ring[(c->head + c->count) % MOST_PIPELINE];
    p->due = due;
    p->key = key;
    p->op = op;
    c->count++;
    if (c->out_len == 0) {
        ssize_t n = send(c->fd, w->scratch, len, 0);
        if (n == (ssize_t)len) return;
        if (n < 0) {
            if (errno != EAGAIN && errno != EWOULDBLOCK) {
                fail_conn(w, c, strerror(errno));
                return;
            }
            n = 0;
        }
        size_t rest = len - (size_t)n;
        if (c->out_cap < rest) {
            c->out = realloc(c->out, rest);
            c->out_cap = rest;
        }
        memcpy(c->out, w->scratch + n, rest);
        c->out_len = rest;
        c->out_at = 0;
        poll_out(w->poll, c, true);
    } else {
        if (c->out_cap < c->out_len + len) {
            c->out_cap = (c->out_len + len) * 2;
            c->out = realloc(c->out, c->out_cap);
        }
        memcpy(c->out + c->out_len, w->scratch, len);
        c->out_len += len;
    }
}

static void next_request(worker* w, conn* c, int64_t due) {
    uint8_t op;
    int key = 0;
    if (protocol == HTTP) {
        op = OP_HTTP;
    } else if (fill) {
        static atomic_int next_key;
        key = atomic_fetch_add(&next_key, 1);
        if (key >= keys) return;
        op = OP_SET;
    } else {
        key = (int)(next_rand(&c->rand) % (uint32_t)keys);
        op = (int)(next_rand(&c->rand) % 100) < sets ? OP_SET : OP_GET;
    }
    send_request(w, c, op, key, due);
}

/* ---- answers */

static void answered(worker* w, conn* c, bool ok) {
    pending p = c->ring[c->head];
    c->head = (c->head + 1) % MOST_PIPELINE;
    c->count--;
    int64_t t = now_ns();
    if (!ok) {
        w->wrong++;
        static atomic_int told;
        if (atomic_fetch_add(&told, 1) < 5) fprintf(stderr, "load: a wrong answer on connection %d: %.*s\n", c->index, c->line_len, c->line);
    }
    if (c->is_idle) {
        if (!c->counted_ready) {
            c->counted_ready = true;
            atomic_fetch_add(&idle_answered, 1);
        }
        return;
    }
    if (t >= w->measure_from && t < w->measure_to) {
        record(w->h, (uint64_t)(t - p.due));
        w->answers++;
        if (w->intervals) {
            int i = (int)((t - w->measure_from) / (int64_t)(report * 1e9));
            if (i < w->interval_n) {
                if (!w->intervals[i]) w->intervals[i] = calloc(1, sizeof(histogram));
                record(w->intervals[i], (uint64_t)(t - p.due));
            }
        }
    }
    c->served++;
    if (reconnect && c->served % (uint64_t)reconnect == 0 && c->count == 0 && t < w->measure_to) {
        /* a new connection in its place; an open loop's next request waits for it, and is late */
        close_conn(w, c);
        if (rate > 0) c->next_due += c->interval;
        open_conn(w, c);
        return;
    }
    if (fill) {
        next_request(w, c, t);
        return;
    }
    if (close_each) return; /* the end of the connection follows */
    if (t >= w->measure_to) return;
    if (rate > 0) {
        c->next_due += c->interval;
        if (c->next_due <= t) {
            w->late++;
            next_request(w, c, c->next_due);
        } else {
            heap_push(w, c);
        }
    } else {
        next_request(w, c, t);
    }
}

static bool expect_value(int key, const char* bytes, int64_t at, int64_t n) {
    for (int64_t i = 0; i < n; i++)
        if (bytes[i] != alphabet[(key + at + i) % 26]) return false;
    return true;
}

/* Reads what the bytes [p, end) say, for as many answers as they hold. */
static void take(worker* w, conn* c, const char* p, const char* end) {
    while (p < end && c->count > 0) {
        pending* want = &c->ring[c->head];
        if (c->state == 0) { /* the first line (RESP) or the head (HTTP) */
            if (protocol == RESP) {
                const char* nl = memchr(p, '\n', (size_t)(end - p));
                size_t n = nl ? (size_t)(nl - p + 1) : (size_t)(end - p);
                if (c->line_len + n > sizeof c->line) { fail_conn(w, c, "a line too long"); return; }
                memcpy(c->line + c->line_len, p, n);
                c->line_len += (int)n;
                p += n;
                if (!nl) return;
                char* l = c->line;
                int len = c->line_len;
                if (len < 3 || l[len - 2] != '\r') { fail_conn(w, c, "not RESP"); return; }
                switch (want->op) {
                case OP_PING:
                    c->line_len = 0;
                    answered(w, c, len == 7 && memcmp(l, "+PONG\r\n", 7) == 0);
                    break;
                case OP_SET:
                    c->line_len = 0;
                    answered(w, c, len == 5 && memcmp(l, "+OK\r\n", 5) == 0);
                    break;
                case OP_GET:
                    if (l[0] != '$') { answered(w, c, false); c->line_len = 0; break; }
                    c->body_len = strtoll(l + 1, NULL, 10);
                    if (c->body_len < 0) { /* not there: fine before a fill */
                        c->line_len = 0;
                        answered(w, c, !check);
                        break;
                    }
                    c->left = c->body_len + 2;
                    c->body_at = 0;
                    c->state = 1;
                    break;
                }
            } else {
                /* HTTP: the head ends with an empty line */
                size_t room = sizeof c->line - (size_t)c->line_len;
                size_t n = (size_t)(end - p) < room ? (size_t)(end - p) : room;
                memcpy(c->line + c->line_len, p, n);
                int before = c->line_len;
                c->line_len += (int)n;
                char* hit = memmem(c->line + (before > 3 ? before - 3 : 0), (size_t)(c->line_len - (before > 3 ? before - 3 : 0)), "\r\n\r\n", 4);
                if (!hit) {
                    if (c->line_len == (int)sizeof c->line) { fail_conn(w, c, "a head too long"); return; }
                    p += n;
                    return;
                }
                int head_len = (int)(hit - c->line) + 4;
                p += head_len - before;
                c->status = 0;
                if (sscanf(c->line, "HTTP/1.1 %d", &c->status) != 1) { fail_conn(w, c, "not HTTP/1.1"); return; }
                c->body_len = -1;
                for (char* h = memchr(c->line, '\n', (size_t)head_len); h && h < c->line + head_len; h = memchr(h, '\n', (size_t)(c->line + head_len - h))) {
                    h++;
                    if (strncasecmp(h, "content-length:", 15) == 0) c->body_len = strtoll(h + 15, NULL, 10);
                }
                if (c->body_len < 0) { fail_conn(w, c, "no Content-Length"); return; }
                c->line_len = head_len; /* kept for a message */
                c->left = c->body_len;
                c->body_at = 0;
                c->state = 1;
                if (c->left == 0) goto body_done;
            }
            continue;
        }
        /* state 1: the body */
        {
            int64_t n = end - p < c->left ? end - p : c->left;
            if (check) {
                if (protocol == RESP) {
                    int64_t value_part = c->body_len - c->body_at;
                    if (value_part > n) value_part = n;
                    if (value_part > 0 && !expect_value(want->key, p, c->body_at, value_part)) c->status = -1;
                } else if (c->body_at + n <= 4 && memcmp(p, &"pong"[c->body_at], (size_t)n) != 0) {
                    c->status = -1;
                }
            }
            c->body_at += n;
            c->left -= n;
            p += n;
            if (c->left > 0) return;
        }
    body_done:
        c->state = 0;
        if (protocol == RESP) {
            bool ok = c->status != -1 && c->body_len == value_size;
            c->status = 0;
            c->line_len = 0;
            answered(w, c, ok || (!check && c->body_len >= 0));
        } else {
            bool ok = c->status == 200 && c->body_len == 4;
            c->line_len = 0;
            answered(w, c, ok);
        }
    }
    if (p < end && c->count == 0) fail_conn(w, c, "bytes that answer nothing");
}

/* ---- the loop of a thread */

static void start_conn(worker* w, conn* c, int64_t t) {
    /* the socket is connected: the first request */
    c->connecting = false;
    poll_out(w->poll, c, c->out_len > 0);
    if (c->is_idle) {
        send_request(w, c, protocol == RESP ? OP_PING : OP_HTTP, 0, t);
    } else if (rate > 0) {
        heap_push(w, c);
    } else if (close_each) {
        next_request(w, c, c->opened_at);
    } else {
        for (int i = 0; i < pipeline; i++) next_request(w, c, t);
    }
}

static void on_event(worker* w, conn* c, bool readable, bool writable, bool hangup) {
    if (c->dead || !c->open) return;
    int64_t t = now_ns();
    if (c->connecting) {
        if (!writable && !hangup && !readable) return;
        int err = 0;
        socklen_t len = sizeof err;
        getsockopt(c->fd, SOL_SOCKET, SO_ERROR, &err, &len);
        if (err) {
            errno = err;
            fail_conn(w, c, strerror(err));
            return;
        }
        start_conn(w, c, t);
        if (c->dead) return;
    }
    if (writable && c->out_len > 0) {
        flush_out(w, c);
        if (c->dead) return;
        if (c->out_len == 0) poll_out(w->poll, c, false);
    }
    if (readable || hangup) {
        for (;;) {
            ssize_t n = recv(c->fd, w->in, 256 * 1024, 0);
            if (n > 0) {
                take(w, c, w->in, w->in + n);
                if (c->dead) return;
                if (n < 256 * 1024) break;
                continue;
            }
            if (n == 0) {
                if (close_each && c->count == 0) {
                    /* the server let it go after its answer: the next request on a new one */
                    close_conn(w, c);
                    if (t < w->measure_to) open_conn(w, c);
                    return;
                }
                fail_conn(w, c, "the server closed it");
                return;
            }
            if (errno == EAGAIN || errno == EWOULDBLOCK) break;
            if (errno == EINTR) continue;
            fail_conn(w, c, strerror(errno));
            return;
        }
    }
}

static void* run(void* arg) {
    worker* w = arg;
    /* idle connections first, at most a few hundred being opened at once so that the backlog of
       the listener is not what is measured */
    int idle_n = 0;
    for (int i = 0; i < w->n; i++) if (w->all[i]->is_idle) idle_n++;
    int opened = 0;
#ifdef __linux__
    struct epoll_event evs[512];
#else
    struct kevent evs[512];
#endif
    while (opened < idle_n || w->idle_ready < idle_n) {
        int connecting = 0;
        for (int i = 0; i < opened; i++) if (w->all[i]->is_idle && !w->all[i]->counted_ready && !w->all[i]->dead) connecting++;
        while (opened < idle_n && connecting < 256) {
            open_conn(w, w->all[opened]);
            opened++;
            connecting++;
        }
#ifdef __linux__
        int k = epoll_wait(w->poll, evs, 512, 100);
        for (int i = 0; i < k; i++) {
            conn* c = evs[i].data.ptr;
            on_event(w, c, evs[i].events & EPOLLIN, evs[i].events & EPOLLOUT, evs[i].events & (EPOLLHUP | EPOLLERR | EPOLLRDHUP));
        }
#else
        struct timespec ts = {0, 100000000};
        int k = kevent(w->poll, NULL, 0, evs, 512, &ts);
        for (int i = 0; i < k; i++) {
            conn* c = evs[i].udata;
            on_event(w, c, evs[i].filter == EVFILT_READ, evs[i].filter == EVFILT_WRITE, evs[i].flags & (EV_EOF | EV_ERROR));
        }
#endif
        w->idle_ready = 0;
        for (int i = 0; i < idle_n; i++) if (w->all[i]->counted_ready || w->all[i]->dead) w->idle_ready++;
        if (stop_all) break;
    }
    return NULL;
}

static void* drive(void* arg) {
    worker* w = arg;
    int64_t t = now_ns();
    w->start = t;
    w->measure_from = t + (int64_t)(warmup * 1e9);
    w->measure_to = w->measure_from + (int64_t)(duration * 1e9);
    int active = 0;
    for (int i = 0; i < w->n; i++) {
        conn* c = w->all[i];
        if (c->is_idle) continue;
        active++;
        if (rate > 0) {
            int64_t total = conns;
            c->interval = (int64_t)(1e9 * (double)total / rate);
            c->next_due = t + c->interval * c->index / total;
        }
        open_conn(w, c);
    }
#ifdef __linux__
    struct epoll_event evs[512];
#else
    struct kevent evs[512];
#endif
    for (;;) {
        int64_t now = now_ns();
        if (now >= w->measure_to + 200000000) break; /* answers still on the way are not counted */
        if (fill) {
            bool busy = false;
            for (int i = 0; i < w->n; i++) if (!w->all[i]->is_idle && !w->all[i]->dead && w->all[i]->count > 0) busy = true;
            if (!busy && now > w->start + 100000000) break;
        }
        while (w->heap_n > 0 && w->heap[0]->next_due <= now) {
            conn* c = heap_pop(w);
            if (c->dead) continue;
            if (c->next_due < w->measure_to) next_request(w, c, c->next_due);
        }
        /* --trickle: the next byte of every slow client */
        if (trickle > 0 && now >= w->trickle_next) {
            static const char head[] = "*3\r\n$3\r\nSET\r\n$7\r\ntrickle\r\n$1048576\r\n";
            for (int i = 0; i < w->n; i++) {
                conn* c = w->all[i];
                if (!c->is_idle || c->dead || !c->open || !c->counted_ready) continue;
                const char* bytes = c->trickling ? "x" : head;
                size_t len = c->trickling ? 1 : sizeof head - 1;
                ssize_t n = send(c->fd, bytes, len, 0);
                if (n < 0 && errno != EAGAIN && errno != EWOULDBLOCK) fail_conn(w, c, strerror(errno));
                else if (n == (ssize_t)len) c->trickling = true;
            }
            w->trickle_next = now + (int64_t)(trickle * 1e6);
        }
        int wait_ms = 50;
        if (w->heap_n > 0) {
            int64_t d = w->heap[0]->next_due - now;
            wait_ms = d <= 0 ? 0 : (int)((d + 999999) / 1000000);
            if (wait_ms > 50) wait_ms = 50;
        }
#ifdef __linux__
        int k = epoll_wait(w->poll, evs, 512, wait_ms);
        for (int i = 0; i < k; i++) {
            conn* c = evs[i].data.ptr;
            on_event(w, c, evs[i].events & EPOLLIN, evs[i].events & EPOLLOUT, evs[i].events & (EPOLLHUP | EPOLLERR | EPOLLRDHUP));
        }
#else
        struct timespec ts = {0, (long)wait_ms * 1000000};
        int k = kevent(w->poll, NULL, 0, evs, 512, &ts);
        for (int i = 0; i < k; i++) {
            conn* c = evs[i].udata;
            on_event(w, c, evs[i].filter == EVFILT_READ, evs[i].filter == EVFILT_WRITE, evs[i].flags & (EV_EOF | EV_ERROR));
        }
#endif
    }
    (void)active;
    return NULL;
}

static double number(const char* s, const char* name) {
    char* end;
    double v = strtod(s, &end);
    if (*end || v < 0) {
        char why[128];
        snprintf(why, sizeof why, "%s takes a number, not `%s`", name, s);
        usage(why);
    }
    return v;
}

int main(int argc, char** argv) {
    if (argc < 3) usage("a protocol and an address are needed");
    if (strcmp(argv[1], "resp") == 0) protocol = RESP;
    else if (strcmp(argv[1], "http") == 0) protocol = HTTP;
    else usage("the protocol is resp or http");
    {
        char host[64];
        const char* colon = strrchr(argv[2], ':');
        if (!colon || colon - argv[2] >= (long)sizeof host) usage("the address is HOST:PORT");
        memcpy(host, argv[2], (size_t)(colon - argv[2]));
        host[colon - argv[2]] = 0;
        target.sin_family = AF_INET;
        target.sin_port = htons((uint16_t)atoi(colon + 1));
        if (strcmp(host, "localhost") == 0) strcpy(host, "127.0.0.1");
        if (inet_pton(AF_INET, host, &target.sin_addr) != 1) usage("the host is an IPv4 address");
    }
    for (int i = 3; i < argc; i++) {
        const char* a = argv[i];
        const char* v = i + 1 < argc ? argv[i + 1] : NULL;
#define TAKES(name) (strcmp(a, name) == 0 && (v ? (i++, true) : (usage(name " takes a value"), false)))
        if (TAKES("--conns")) conns = (int)number(v, a);
        else if (TAKES("--threads")) threads = (int)number(v, a);
        else if (TAKES("--pipeline")) pipeline = (int)number(v, a);
        else if (TAKES("--rate")) rate = number(v, a);
        else if (TAKES("--duration")) duration = number(v, a);
        else if (TAKES("--warmup")) warmup = number(v, a);
        else if (TAKES("--value")) value_size = (int)number(v, a);
        else if (TAKES("--keys")) keys = (int)number(v, a);
        else if (TAKES("--sets")) sets = (int)number(v, a);
        else if (TAKES("--idle")) idle = (int)number(v, a);
        else if (TAKES("--sources")) sources = (int)number(v, a);
        else if (TAKES("--report")) report = number(v, a);
        else if (TAKES("--reconnect")) reconnect = (int)number(v, a);
        else if (TAKES("--trickle")) trickle = number(v, a);
        else if (strcmp(a, "--close") == 0) close_each = true;
        else if (strcmp(a, "--fill") == 0) fill = true;
        else if (strcmp(a, "--check") == 0) check = true;
        else {
            char why[128];
            snprintf(why, sizeof why, "`%s` is not an option", a);
            usage(why);
        }
    }
    if (threads < 1 || pipeline < 1 || pipeline > MOST_PIPELINE || keys < 1 || value_size > 65536 || sources < 1 || sources > 250) usage("a value out of range");
    if (close_each && (protocol != HTTP || pipeline != 1 || rate > 0)) usage("--close is for HTTP, one request at a time, closed loop");
    if (rate > 0 && pipeline != 1) usage("--rate sends one request at a time on each connection");
    if (fill && protocol != RESP) usage("--fill is for RESP");
    if (trickle > 0 && (protocol != RESP || idle == 0)) usage("--trickle is for RESP, with --idle");
    if (reconnect && (pipeline != 1 || close_each)) usage("--reconnect is for one request at a time on kept connections");
    if (fill) { warmup = 0; duration = 1e6; idle = 0; }
    signal(SIGPIPE, SIG_IGN);
    struct rlimit r;
    if (getrlimit(RLIMIT_NOFILE, &r) == 0) {
        r.rlim_cur = r.rlim_max;
#ifdef __APPLE__
        if (r.rlim_cur > 10240) r.rlim_cur = 10240;
#endif
        setrlimit(RLIMIT_NOFILE, &r);
    }
    for (int i = 0; i < (int)sizeof alphabet; i++) alphabet[i] = (char)('a' + i % 26);

    /* the connections, dealt to the threads in turn */
    idle_total = idle;
    int all_n = conns + idle;
    worker* ws = calloc((size_t)threads, sizeof(worker));
    for (int t = 0; t < threads; t++) {
        ws[t].id = t;
        ws[t].poll = poll_new();
        ws[t].all = calloc((size_t)all_n / (size_t)threads + 2, sizeof(conn*));
        ws[t].heap = calloc((size_t)all_n / (size_t)threads + 2, sizeof(conn*));
        ws[t].h = calloc(1, sizeof(histogram));
        ws[t].measure_to = INT64_MAX;
        ws[t].in = malloc(256 * 1024);
        if (report > 0) {
            ws[t].interval_n = (int)ceil(duration / report);
            ws[t].intervals = calloc((size_t)ws[t].interval_n, sizeof(histogram*));
        }
    }
    for (int i = 0; i < all_n; i++) {
        conn* c = calloc(1, sizeof(conn));
        c->fd = -1;
        c->heap_at = -1;
        c->is_idle = i < idle;
        c->index = c->is_idle ? i : i - idle;
        c->rand = 0x9e3779b9u * (uint32_t)(i + 1);
        worker* w = &ws[i % threads];
        w->all[w->n++] = c;
    }
    /* the idle ones opened and answered once */
    if (idle > 0) {
        pthread_t* ts = calloc((size_t)threads, sizeof(pthread_t));
        for (int t = 0; t < threads; t++) pthread_create(&ts[t], NULL, run, &ws[t]);
        for (int t = 0; t < threads; t++) pthread_join(ts[t], NULL);
        free(ts);
        uint64_t errs = 0;
        for (int t = 0; t < threads; t++) errs += ws[t].errors;
        printf("ready %d idle connections, %" PRIu64 " failed\n", atomic_load(&idle_answered), errs);
        fflush(stdout);
    }
    /* then the load, with the idle ones still open (and watched: a server that drops one is an error) */
    pthread_t* ts = calloc((size_t)threads, sizeof(pthread_t));
    int64_t began = now_ns();
    for (int t = 0; t < threads; t++) pthread_create(&ts[t], NULL, drive, &ws[t]);
    for (int t = 0; t < threads; t++) pthread_join(ts[t], NULL);
    int64_t ended = now_ns();

    histogram* all = calloc(1, sizeof(histogram));
    uint64_t answers = 0, errors = 0, wrong = 0, late = 0, cut = 0;
    for (int t = 0; t < threads; t++) {
        for (int i = 0; i < BUCKETS; i++) all->counts[i] += ws[t].h->counts[i];
        all->total += ws[t].h->total;
        all->sum += ws[t].h->sum;
        if (ws[t].h->max > all->max) all->max = ws[t].h->max;
        answers += ws[t].answers;
        errors += ws[t].errors;
        wrong += ws[t].wrong;
        late += ws[t].late;
        cut += ws[t].cut;
    }
    if (report > 0) {
        for (int i = 0; i < ws[0].interval_n; i++) {
            histogram* h = calloc(1, sizeof(histogram));
            for (int t = 0; t < threads; t++) {
                histogram* x = ws[t].intervals[i];
                if (!x) continue;
                for (int b = 0; b < BUCKETS; b++) h->counts[b] += x->counts[b];
                h->total += x->total;
                h->sum += x->sum;
                if (x->max > h->max) h->max = x->max;
            }
            printf("{\"interval\":%d,\"from\":%.1f,\"answers\":%" PRIu64 ",\"per_second\":%.0f,\"p50_us\":%.1f,\"p99_us\":%.1f,\"p999_us\":%.1f,\"max_us\":%.1f}\n",
                   i, i * report, h->total, (double)h->total / report, percentile(h, 50), percentile(h, 99), percentile(h, 99.9), (double)h->max / 1000.0);
            free(h);
        }
    }
    double seconds = fill ? (double)(ended - began) / 1e9 : duration;
    printf("{\"protocol\":\"%s\",\"conns\":%d,\"idle\":%d,\"pipeline\":%d,\"rate\":%.0f,\"answers\":%" PRIu64
           ",\"seconds\":%.3f,\"per_second\":%.0f,\"p50_us\":%.1f,\"p90_us\":%.1f,\"p99_us\":%.1f,\"p999_us\":%.1f"
           ",\"max_us\":%.1f,\"mean_us\":%.1f,\"late\":%" PRIu64 ",\"errors\":%" PRIu64 ",\"wrong\":%" PRIu64 ",\"cut\":%" PRIu64 "}\n",
           protocol == RESP ? "resp" : "http", conns, idle, pipeline, rate, answers, seconds, (double)answers / seconds,
           percentile(all, 50), percentile(all, 90), percentile(all, 99), percentile(all, 99.9),
           (double)all->max / 1000.0, all->total ? all->sum / (double)all->total / 1000.0 : 0, late, errors, wrong, cut);
    return errors || wrong ? 1 : 0;
}
