/* The stand of the server lane (choices 1750 to 1753, 1761 to 1763): one program in C that starts a server,
   does to it what clients do that a server must stand (they reset, close half, send a byte at a
   time, never read, send what is not the protocol, outnumber its descriptors, ask it to stop), and
   says whether it stood. `load.c` measures how fast a server is; this checks that it stays up.

       stand resp|http SCENARIO [options] -- SERVER ARGS...

   The server is the program and arguments after `--`; an argument `PORT` is replaced by a port of
   127.0.0.1 the system has just given. The stand starts it, waits for it to listen, warms it up,
   counts its open descriptors (the base), and runs the scenario --rounds times. After each round
   the server must still run, answer a plain client within --sanity milliseconds, and be back to
   the base of descriptors within five seconds. The first round warms the server up: with --rounds
   3 or more, the memory the server holds idle after the last round must be within 10% and 2 MB of
   what it held after the second (choices 1762, 1763: what a C library keeps after the first rounds
   is let be, memory that stays behind after each round, as 100 bytes a connection do, is not; the
   peak, which moves with how the connections of a round meet, is told and not held to it).

   Scenarios (choice 1751; every answer that comes is held to what it must be, byte for byte in
   RESP, status, length and body in HTTP):
     answers    pipelined requests that must be answered (the plain case, and the warm-up)
     fds        the server started with --nofile descriptors (256) and --conns clients at once:
                first they are held open for two seconds, in which the server must use less than
                half a second of the processor (it must not spin on a full table); then each asks
                once and must be answered (choice 1751 a). A connection reset or closed before
                any answer is made again 20 ms later, as a client would: macOS does that to one
                that waited in a full queue of the listener, to the C server's too
     reset      connections reset at random points: before a request, in the middle of one, while
                the answer is awaited, in the middle of an answer of 1 MB (RESP) or of 2,000
                (HTTP) (b)
     half       requests, then the sending half closed: every answer must come, then the close (c)
     split      requests sent a byte at a time and answers read a byte at a time through a small
                receive buffer; in RESP, the first connection of the first round sets a value of
                1 MB sent in pieces of random size and reads it back a byte at a time (in the
                round that warms up: where the allocator of macOS puts blocks of a megabyte moves
                the peak by 3 MB from one round to another) (d)
     trickle    requests sent a byte at a time with up to --gap milliseconds between bytes (f)
     noread     --conns clients each ask for far more than they read (RESP: GET of a value of
                --value bytes, 1 MB if not given, as often as 1 GB of answers takes; HTTP: 100,000
                requests) and never read: the memory of the server may
                grow by --most-growth MB at most (64 + 8 a client), a plain client must still be
                answered, and with --idle MS (the server's --idle-limit) the server must have let
                go of every one of them (its descriptors back to the base) within MS + 3,000 ms (e)
     term       --conns clients each in the middle of a request and ten that send a byte now and
                then, then SIGTERM: the server must end with 0 within --grace + --margin
                milliseconds (g; no rounds)
     fuzz       --conns connections, each sending a request that is not the protocol (lengths too
                long or negative, numbers past 64 bits, halves, random bytes, heads too large; for
                HTTP also the ways of smuggling a request: a length and chunks together, a folded
                line, a space before the colon, a chunk size that is not one),
                drawn from --seed; most then close their sending half and must be answered or
                closed within --deadline milliseconds (1,000), the rest reset; a plain client is
                asked after each thousand (h)

   Options:
     --conns N        connections in all (1000)
     --at-once N      connections open at once (100; scenarios that hold their connections open
                      every one at once)
     --rounds N       (4)
     --seed S         (1)
     --nofile N       the server's limit of descriptors (fds: 256)
     --grace MS       term: the time the server was given to end (its own --grace)
     --margin MS      term: the time past the grace it may take (1000)
     --idle MS        noread: the server's --idle-limit, after which it must cut a client
     --gap MS         trickle: the most time between two bytes (20)
     --value BYTES    noread, RESP: the value the clients ask for (1 MB)
     --deadline MS    fuzz: the time after a request to its answer or close (1000)
     --sanity MS      the time a plain client may wait for its answer (2000)
     --most-growth MB noread: the most the server's memory may grow
     --no-memory      no check of memory (a server built with the address sanitizer)
     --most-rss MB    a server that holds more is told of and stopped, before the machine runs out
                      (looked at every 20 ms)
     --sources K      the connections come from 127.0.0.1 to 127.0.0.K (Linux)
                      (a connect that finds no port of this machine free waits 100 ms and tries
                      again, its deadline moved on as much: macOS keeps the port of a connection the
                      stand closed 30 s, and has 16,384)
     --timeout S      the whole run (600); past it the stand says where it hung, kills the
                      server and ends with 3

   It writes one line of JSON on the standard output when it ends: the scenario, the connections
   made, failed and cut, the descriptors before and after each round, the memory held idle and its peak after each, and
   what the scenario measured. Exit code 0 when the server stood, 1 when it did not (each failure
   on the standard error, the first 20 in full), 2 for a wrong use, 3 when the stand hung.
   Linux (epoll, /proc) and macOS (kqueue, libproc). */

#define _GNU_SOURCE
#include <arpa/inet.h>
#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <inttypes.h>
#include <netinet/in.h>
#include <netinet/tcp.h>
#include <signal.h>
#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>
#include <sys/resource.h>
#include <sys/socket.h>
#include <sys/time.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>
#ifdef __linux__
#include <sys/epoll.h>
#else
#include <libproc.h>
#include <mach/mach_time.h>
#include <sys/event.h>
#endif

#ifndef MSG_NOSIGNAL
#define MSG_NOSIGNAL 0
#endif

enum { RESP, HTTP };
static int kind;
static const char* scenario;
static int total = 1000, at_once = 100, rounds = 4, nofile = 0, grace_ms = 0, margin_ms = 1000, idle_ms = 0;
static int value_size = 0, gap_ms = 20, deadline_ms = 1000, sanity_ms = 2000, sources = 1, timeout_s = 600;
static int64_t most_growth_mb = 0, most_rss_mb = 0;
static uint32_t seed = 1;
static bool memory = true;
static char** server_argv;
static pid_t server = -1;
static int server_status;
static bool server_gone, stopped_by_stand;
static struct sockaddr_in target;
static const char* volatile phase = "starting the server";
static int round_no;

static int64_t now_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (int64_t)ts.tv_sec * 1000000000 + ts.tv_nsec;
}
#define MS 1000000LL

static void stop_server(void) {
    if (server > 0 && !server_gone) {
        kill(server, SIGKILL);
        waitpid(server, &server_status, 0);
        server_gone = true;
    }
}

static void die(const char* what) {
    fprintf(stderr, "stand: %s: %s\n", what, strerror(errno));
    stop_server();
    exit(2);
}

static void usage(const char* why) {
    fprintf(stderr, "stand: %s\nusage: stand resp|http SCENARIO [options] -- SERVER ARGS...\n", why);
    exit(2);
}

static int failures;
static void report(const char* fmt, ...) {
    failures++;
    if (failures > 20) return;
    va_list ap;
    va_start(ap, fmt);
    fprintf(stderr, "stand: %s: ", phase);
    vfprintf(stderr, fmt, ap);
    fputc('\n', stderr);
    va_end(ap);
}

static void on_alarm(int sig) {
    (void)sig;
    static const char a[] = "stand: hung in: ";
    write(2, a, sizeof a - 1);
    write(2, (const char*)phase, strlen((const char*)phase));
    write(2, "\n", 1);
    if (server > 0) kill(server, SIGKILL);
    _exit(3);
}

/* ---- bytes */

typedef struct {
    uint8_t* p;
    size_t n, cap;
} buf;

static void put(buf* b, const void* p, size_t n) {
    if (b->n + n > b->cap) {
        size_t cap = b->cap ? b->cap * 2 : 256;
        while (cap < b->n + n) cap *= 2;
        b->p = realloc(b->p, cap);
        if (!b->p) die("realloc");
        b->cap = cap;
    }
    memcpy(b->p + b->n, p, n);
    b->n += n;
}

static void puts_(buf* b, const char* s) { put(b, s, strlen(s)); }

static void putf(buf* b, const char* fmt, ...) {
    char t[256];
    va_list ap;
    va_start(ap, fmt);
    int n = vsnprintf(t, sizeof t, fmt, ap);
    va_end(ap);
    put(b, t, (size_t)n);
}

static uint32_t next_rand(uint32_t* s) {
    uint32_t x = *s;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    return *s = x ? x : 0x9e3779b9u;
}

static void letters(uint32_t* r, uint8_t* to, size_t n) {
    for (size_t i = 0; i < n; i++) to[i] = (uint8_t)('a' + next_rand(r) % 26);
}

/* A command of RESP, its parts given with their lengths. */
static void resp(buf* b, int argc, const char** argv, const size_t* lens) {
    putf(b, "*%d\r\n", argc);
    for (int i = 0; i < argc; i++) {
        size_t n = lens ? lens[i] : strlen(argv[i]);
        putf(b, "$%zu\r\n", n);
        put(b, argv[i], n);
        puts_(b, "\r\n");
    }
}

static const char PING_HTTP[] = "GET /ping HTTP/1.1\r\nHost: stand\r\n\r\n";

/* The value of 1 MB every RESP scenario may GET, and its answer. */
#define BIG (1 << 20)
static uint8_t* big;
static buf big_answer;

/* ---- the server */

static bool server_runs(void) {
    if (server_gone) return false;
    pid_t r = waitpid(server, &server_status, WNOHANG);
    if (r == server) server_gone = true;
    return !server_gone;
}

static void tell_end(void) {
    if (stopped_by_stand) return;
    if (WIFEXITED(server_status))
        report("the server ended with %d", WEXITSTATUS(server_status));
    else if (WIFSIGNALED(server_status))
        report("the server was ended by the signal %d", WTERMSIG(server_status));
}

static void start_server(void) {
    int s = socket(AF_INET, SOCK_STREAM, 0);
    struct sockaddr_in a = {.sin_family = AF_INET};
    a.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    if (s < 0 || bind(s, (struct sockaddr*)&a, sizeof a) != 0) die("a port");
    socklen_t len = sizeof a;
    getsockname(s, (struct sockaddr*)&a, &len);
    close(s);
    target = a;
    char port[16];
    snprintf(port, sizeof port, "%d", ntohs(a.sin_port));
    for (char** p = server_argv; *p; p++)
        if (strcmp(*p, "PORT") == 0) *p = strdup(port);
    server = fork();
    if (server < 0) die("fork");
    if (server == 0) {
        signal(SIGPIPE, SIG_DFL);
        if (nofile > 0) {
            struct rlimit rl = {(rlim_t)nofile, (rlim_t)nofile};
            if (setrlimit(RLIMIT_NOFILE, &rl) != 0) {
                perror("stand: setrlimit");
                _exit(127);
            }
        }
        execvp(server_argv[0], server_argv);
        perror("stand: exec");
        _exit(127);
    }
    int64_t until = now_ns() + 30000 * MS;
    while (true) {
        if (!server_runs()) {
            tell_end();
            exit(1);
        }
        int c = socket(AF_INET, SOCK_STREAM, 0);
        if (c < 0) die("socket");
        if (connect(c, (struct sockaddr*)&target, sizeof target) == 0) {
            close(c);
            return;
        }
        close(c);
        if (now_ns() > until) {
            report("the server does not listen on %s in 30 seconds", port);
            stop_server();
            exit(1);
        }
        usleep(20000);
    }
}

#ifdef __linux__
static int count_fds(void) {
    char path[64];
    snprintf(path, sizeof path, "/proc/%d/fd", (int)server);
    DIR* d = opendir(path);
    if (!d) return -1;
    int n = 0;
    for (struct dirent* e; (e = readdir(d));)
        if (e->d_name[0] != '.') n++;
    closedir(d);
    return n;
}

static int64_t status_kb(const char* field) {
    char path[64], line[256];
    snprintf(path, sizeof path, "/proc/%d/status", (int)server);
    FILE* f = fopen(path, "r");
    if (!f) return -1;
    int64_t v = -1;
    size_t n = strlen(field);
    while (fgets(line, sizeof line, f))
        if (strncmp(line, field, n) == 0) {
            v = strtoll(line + n, NULL, 10);
            break;
        }
    fclose(f);
    return v;
}
static int64_t rss_kb(void) { return status_kb("VmRSS:"); }
static int64_t peak_kb(void) { return status_kb("VmHWM:"); }

static int64_t cpu_ns(void) {
    char path[64], text[1024];
    snprintf(path, sizeof path, "/proc/%d/stat", (int)server);
    FILE* f = fopen(path, "r");
    if (!f) return -1;
    size_t n = fread(text, 1, sizeof text - 1, f);
    fclose(f);
    text[n] = 0;
    char* p = strrchr(text, ')');
    unsigned long user = 0, sys = 0;
    if (!p || sscanf(p + 2, "%*s %*s %*s %*s %*s %*s %*s %*s %*s %*s %*s %lu %lu", &user, &sys) != 2) return -1;
    return (int64_t)(user + sys) * (1000000000 / sysconf(_SC_CLK_TCK));
}
#else
static int count_fds(void) {
    int size = proc_pidinfo(server, PROC_PIDLISTFDS, 0, NULL, 0);
    if (size <= 0) return -1;
    void* b = malloc((size_t)size);
    int got = proc_pidinfo(server, PROC_PIDLISTFDS, 0, b, size);
    free(b);
    return got < 0 ? -1 : got / (int)PROC_PIDLISTFD_SIZE;
}

static bool usage_of(struct rusage_info_v4* ri) {
    return proc_pid_rusage(server, RUSAGE_INFO_V4, (rusage_info_t*)ri) == 0;
}
static int64_t rss_kb(void) {
    struct rusage_info_v4 ri;
    return usage_of(&ri) ? (int64_t)(ri.ri_phys_footprint / 1024) : -1;
}
static int64_t peak_kb(void) {
    struct rusage_info_v4 ri;
    return usage_of(&ri) ? (int64_t)(ri.ri_lifetime_max_phys_footprint / 1024) : -1;
}
static int64_t cpu_ns(void) {
    struct rusage_info_v4 ri;
    if (!usage_of(&ri)) return -1;
    static mach_timebase_info_data_t tb;
    if (tb.denom == 0) mach_timebase_info(&tb);
    return (int64_t)((ri.ri_user_time + ri.ri_system_time) * tb.numer / tb.denom);
}
#endif

/* ---- connections */

enum { G_MAIN, G_HOLD, G_SIDE, G_SANITY, GROUPS };

typedef struct conn {
    int fd, group, index;
    uint32_t gen, rng;
    bool used, connecting, sent_all, shut, eof, hold, no_read, any, want_eof, retry_refused, want_out;
    bool shut_after, send_random;
    uint8_t* out;
    size_t out_len, out_pos, send_most;
    int64_t gap_most, send_at, deadline;
    size_t reset_out, reset_in; /* SIZE_MAX: none */
    int rcvbuf, retries;
    uint8_t* expect; /* RESP: the bytes of every answer, in order */
    size_t expect_len;
    bool own_expect;
    int http_left; /* HTTP: answers still to come, each 200 and "pong" */
    int hstate;    /* 0 the head, 1 the body */
    size_t hlen;
    int64_t body_left;
    size_t body_at;
    char head[480];
    size_t got, read_most;
} conn;

typedef struct {
    int total, started, done, failed, at_once, active;
    void (*make)(conn*);
} group;

static group groups[GROUPS];
static conn* slots;
static int nslots, *free_slots, nfree;
static int pollfd = -1;
static int64_t conn_ms = 60000;
static uint64_t made, retried, cuts, finished_badly, ports_waited;

typedef struct {
    int64_t at;
    int slot;
    uint32_t gen;
    int what; /* 0 send, 1 open again */
} timer;
static timer* heap;
static int heap_n, heap_cap;

static void heap_push(timer t) {
    if (heap_n == heap_cap) {
        heap_cap = heap_cap ? heap_cap * 2 : 1024;
        heap = realloc(heap, (size_t)heap_cap * sizeof *heap);
        if (!heap) die("realloc");
    }
    int i = heap_n++;
    while (i > 0 && heap[(i - 1) / 2].at > t.at) {
        heap[i] = heap[(i - 1) / 2];
        i = (i - 1) / 2;
    }
    heap[i] = t;
}

static timer heap_pop(void) {
    timer top = heap[0], last = heap[--heap_n];
    int i = 0;
    while (true) {
        int k = 2 * i + 1;
        if (k >= heap_n) break;
        if (k + 1 < heap_n && heap[k + 1].at < heap[k].at) k++;
        if (heap[k].at >= last.at) break;
        heap[i] = heap[k];
        i = k;
    }
    if (heap_n > 0) heap[i] = last;
    return top;
}

static int slot_of(conn* c) { return (int)(c - slots); }

static void p_set(conn* c, bool add) {
#ifdef __linux__
    struct epoll_event ev = {.events = (c->no_read ? 0 : EPOLLIN) | EPOLLRDHUP | (c->want_out ? EPOLLOUT : 0)};
    ev.data.u64 = (uint64_t)slot_of(c);
    if (epoll_ctl(pollfd, add ? EPOLL_CTL_ADD : EPOLL_CTL_MOD, c->fd, &ev) != 0) die("epoll_ctl");
#else
    struct kevent ev[2];
    void* tag = (void*)(intptr_t)slot_of(c);
    int n = 0;
    if (add) EV_SET(&ev[n++], c->fd, EVFILT_READ, EV_ADD | (c->no_read ? EV_CLEAR : 0), 0, 0, tag);
    EV_SET(&ev[n++], c->fd, EVFILT_WRITE, (add ? EV_ADD : 0) | (c->want_out ? EV_ENABLE : EV_DISABLE), 0, 0, tag);
    if (kevent(pollfd, ev, n, NULL, 0, NULL) != 0) die("kevent");
#endif
}

static void want_out(conn* c, bool on) {
    if (c->want_out == on) return;
    c->want_out = on;
    p_set(c, false);
}

static void finish(conn* c, bool ok, bool reset) {
    if (c->fd >= 0) {
        if (reset) {
            struct linger l = {1, 0};
            setsockopt(c->fd, SOL_SOCKET, SO_LINGER, &l, sizeof l);
        }
        close(c->fd);
        c->fd = -1;
    }
    free(c->out);
    c->out = NULL;
    if (c->own_expect) free(c->expect);
    c->expect = NULL;
    group* g = &groups[c->group];
    g->active--;
    g->done++;
    if (!ok) {
        g->failed++;
        finished_badly++;
    }
    c->used = false;
    c->gen++;
    free_slots[nfree++] = slot_of(c);
}

static void fail(conn* c, const char* fmt, ...) {
    char t[512];
    va_list ap;
    va_start(ap, fmt);
    vsnprintf(t, sizeof t, fmt, ap);
    va_end(ap);
    report("connection %d: %s", c->index, t);
    finish(c, false, true);
}

static void cut(conn* c) {
    cuts++;
    finish(c, true, true);
}

static void open_conn(conn* c);

static void check_done(conn* c) {
    if (!c->used || c->hold || !c->sent_all || (c->shut_after && !c->shut) || c->any) return;
    bool answered = kind == RESP ? c->got == c->expect_len : c->http_left == 0;
    if (!answered || (c->want_eof && !c->eof)) return;
    finish(c, true, false);
}

static void do_send(conn* c) {
    int64_t now = now_ns();
    if (c->connecting || c->sent_all || c->send_at > now) return;
    while (c->out_pos < c->out_len) {
        size_t n = c->out_len - c->out_pos;
        if (c->send_most && n > c->send_most) {
            n = c->send_random ? 1 + next_rand(&c->rng) % c->send_most : c->send_most;
            if (n > c->out_len - c->out_pos) n = c->out_len - c->out_pos;
        }
        if (c->reset_out != SIZE_MAX && c->out_pos < c->reset_out && n > c->reset_out - c->out_pos)
            n = c->reset_out - c->out_pos;
        ssize_t r = send(c->fd, c->out + c->out_pos, n, MSG_NOSIGNAL);
        if (r < 0) {
            if (errno == EAGAIN || errno == EWOULDBLOCK) {
                want_out(c, true);
                return;
            }
            if (errno == EINTR) continue;
            if (c->no_read) {
                cut(c);
                return;
            }
            if (c->any || c->hold) {
                /* The server closed what was not the protocol: what it said, and its close, are read. */
                c->sent_all = true;
                c->shut = true;
                want_out(c, false);
                return;
            }
            fail(c, "send: %s (%zu of %zu bytes sent)", strerror(errno), c->out_pos, c->out_len);
            return;
        }
        c->out_pos += (size_t)r;
        if (c->out_pos == c->reset_out) {
            finish(c, true, true);
            return;
        }
        if (c->gap_most > 0 && c->out_pos < c->out_len) {
            c->send_at = now + 1 + (int64_t)(next_rand(&c->rng) % (uint64_t)c->gap_most);
            heap_push((timer){c->send_at, slot_of(c), c->gen, 0});
            want_out(c, false);
            return;
        }
    }
    c->sent_all = true;
    want_out(c, false);
    if (c->shut_after && !c->shut) {
        shutdown(c->fd, SHUT_WR);
        c->shut = true;
        if (c->any) c->deadline = now + deadline_ms * MS;
    }
    if (c->reset_in == 0) {
        finish(c, true, true);
        return;
    }
    check_done(c);
}

/* An answer of HTTP: a head with a status of 200 and a length of 4, and the body "pong". */
static bool take_http(conn* c, const uint8_t* p, size_t n) {
    for (size_t i = 0; i < n; i++) {
        if (c->http_left == 0) {
            fail(c, "more bytes than the answers asked for");
            return false;
        }
        if (c->hstate == 0) {
            if (c->hlen == sizeof c->head - 1) {
                fail(c, "a head of an answer longer than %zu bytes", sizeof c->head - 1);
                return false;
            }
            c->head[c->hlen++] = (char)p[i];
            if (c->hlen < 4 || memcmp(c->head + c->hlen - 4, "\r\n\r\n", 4) != 0) continue;
            c->head[c->hlen] = 0;
            if (strncmp(c->head, "HTTP/1.1 200 ", 13) != 0) {
                char* eol = strstr(c->head, "\r\n");
                fail(c, "the answer `%.*s`, not 200", eol ? (int)(eol - c->head) : 40, c->head);
                return false;
            }
            c->body_left = -1;
            for (char* line = strstr(c->head, "\r\n") + 2; *line && line[0] != '\r'; line = strstr(line, "\r\n") + 2)
                if (strncasecmp(line, "content-length:", 15) == 0) c->body_left = strtoll(line + 15, NULL, 10);
            if (c->body_left != 4) {
                fail(c, "an answer of 200 with a length of %lld, not 4", (long long)c->body_left);
                return false;
            }
            c->hstate = 1;
            c->body_at = 0;
            continue;
        }
        if (p[i] != (uint8_t)"pong"[c->body_at]) {
            fail(c, "a body that is not `pong`");
            return false;
        }
        c->body_at++;
        if (--c->body_left == 0) {
            c->hstate = 0;
            c->hlen = 0;
            c->http_left--;
        }
    }
    return true;
}

static void take(conn* c, const uint8_t* p, size_t n) {
    size_t before = c->got;
    c->got += n;
    if (!c->any) {
        if (kind == RESP) {
            if (c->got > c->expect_len) {
                fail(c, "%zu bytes of answers, where %zu were asked for", c->got, c->expect_len);
                return;
            }
            if (memcmp(p, c->expect + before, n) != 0) {
                size_t at = 0;
                while (p[at] == c->expect[before + at]) at++;
                fail(c, "byte %zu of the answers is %d, not %d", before + at, p[at], c->expect[before + at]);
                return;
            }
        } else if (!take_http(c, p, n)) {
            return;
        }
    }
    if (c->reset_in != SIZE_MAX && c->got >= c->reset_in) {
        finish(c, true, true);
        return;
    }
    check_done(c);
}

/* The connection closed and opened again 20 ms later, from its start. */
static void again(conn* c) {
    close(c->fd);
    c->fd = -1;
    c->out_pos = c->got = c->hlen = 0;
    c->hstate = 0;
    c->sent_all = c->shut = c->eof = false;
    c->send_at = 0;
    c->retries++;
    retried++;
    heap_push((timer){now_ns() + 20 * MS, slot_of(c), c->gen, 1});
}

static void do_recv(conn* c) {
    static uint8_t b[65536];
    size_t most = c->read_most ? c->read_most : sizeof b;
    while (c->used) {
        ssize_t n = recv(c->fd, b, most, 0);
        if (n > 0) {
            take(c, b, (size_t)n);
            continue;
        }
        if (n < 0 && (errno == EAGAIN || errno == EWOULDBLOCK)) return;
        if (n < 0 && errno == EINTR) continue;
        if (c->any || c->hold) {
            /* What was not the protocol was closed or reset, or a held connection was let go. */
            cuts++;
            finish(c, true, n < 0);
            return;
        }
        if ((n == 0 || errno == ECONNRESET) && c->got == 0 && c->retry_refused && c->retries < 1000) {
            /* macOS resets, or closes, a connection that waited in a full queue of the listener (the
               C server's too); a client tries again. */
            again(c);
            return;
        }
        if (n < 0) {
            fail(c, "recv: %s (%zu bytes of answers)", strerror(errno), c->got);
            return;
        }
        c->eof = true;
        bool answered = kind == RESP ? c->got == c->expect_len : c->http_left == 0;
        if (answered && c->sent_all && c->want_eof) {
            finish(c, true, false);
            return;
        }
        if (kind == RESP)
            fail(c, "closed by the server after %zu of %zu bytes of answers", c->got, c->expect_len);
        else
            fail(c, "closed by the server with %d answers still to come", c->http_left);
        return;
    }
}

static void on_connected(conn* c) {
    c->connecting = false;
    if (c->reset_out == 0) {
        finish(c, true, true);
        return;
    }
    if (c->out_len == 0) {
        c->sent_all = true;
        want_out(c, false);
        check_done(c);
        return;
    }
    do_send(c);
}

static void open_conn(conn* c) {
    int fd = socket(AF_INET, SOCK_STREAM, 0);
    if (fd < 0) {
        fail(c, "socket: %s", strerror(errno));
        return;
    }
    int one = 1;
    setsockopt(fd, IPPROTO_TCP, TCP_NODELAY, &one, sizeof one);
#ifdef SO_NOSIGPIPE
    setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, &one, sizeof one);
#endif
    if (c->rcvbuf) setsockopt(fd, SOL_SOCKET, SO_RCVBUF, &c->rcvbuf, sizeof c->rcvbuf);
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
    c->connecting = true;
    c->want_out = true;
    made++;
    if (connect(fd, (struct sockaddr*)&target, sizeof target) != 0 && errno != EINPROGRESS) {
        int e = errno;
        if (e == EADDRNOTAVAIL && c->retries < 1000) {
            /* No port of this machine is free: this side's limit, not the server's. */
            close(fd);
            c->fd = -1;
            c->retries++;
            ports_waited++;
            if (c->deadline) c->deadline += 100 * MS;
            heap_push((timer){now_ns() + 100 * MS, slot_of(c), c->gen, 1});
            return;
        }
        if (e == ECONNREFUSED && c->retry_refused && c->retries < 1000) {
            close(fd);
            c->fd = -1;
            c->retries++;
            retried++;
            heap_push((timer){now_ns() + 20 * MS, slot_of(c), c->gen, 1});
            return;
        }
        if (c->hold || c->any) {
            close(fd);
            c->fd = -1;
            cuts++;
            finish(c, true, false);
            return;
        }
        fail(c, "connect: %s", strerror(e));
        return;
    }
    p_set(c, true);
}

static void connect_done(conn* c) {
    int err = 0;
    socklen_t len = sizeof err;
    getsockopt(c->fd, SOL_SOCKET, SO_ERROR, &err, &len);
    if (err == 0) {
        on_connected(c);
        return;
    }
    if ((err == ECONNREFUSED || err == ECONNRESET) && c->retry_refused && c->retries < 1000) {
        again(c);
        return;
    }
    if (c->hold || c->any) {
        cuts++;
        finish(c, true, false);
        return;
    }
    fail(c, "connect: %s", strerror(err));
}

static void on_event(conn* c, bool rd, bool wr, bool hup) {
    if (c->connecting) {
        if (wr || hup) connect_done(c);
        return;
    }
    if (c->no_read) {
        if (hup) {
            cut(c);
            return;
        }
        if (wr) do_send(c);
        return;
    }
    if (wr && c->used) do_send(c);
    if ((rd || hup) && c->used) do_recv(c);
}

static void start_conn(int g) {
    if (nfree == 0) {
        int more = nslots ? nslots : 256;
        slots = realloc(slots, (size_t)(nslots + more) * sizeof *slots);
        free_slots = realloc(free_slots, (size_t)(nslots + more) * sizeof *free_slots);
        if (!slots || !free_slots) die("realloc");
        memset(slots + nslots, 0, (size_t)more * sizeof *slots);
        for (int i = nslots + more - 1; i >= nslots; i--) free_slots[nfree++] = i;
        nslots += more;
    }
    conn* c = &slots[free_slots[--nfree]];
    uint32_t gen = c->gen;
    memset(c, 0, sizeof *c);
    c->gen = gen;
    c->used = true;
    c->fd = -1;
    c->group = g;
    group* gr = &groups[g];
    c->index = gr->started++;
    gr->active++;
    c->rng = (seed * 2654435761u) ^ ((uint32_t)c->index * 40503u + 1) ^ ((uint32_t)(round_no + 1) << 24) ^ ((uint32_t)g << 20);
    next_rand(&c->rng);
    c->reset_out = c->reset_in = SIZE_MAX;
    c->deadline = now_ns() + conn_ms * MS;
    gr->make(c);
    if (c->hold) c->deadline = 0;
    open_conn(c);
}

static void fill_groups(void) {
    for (int g = 0; g < GROUPS; g++) {
        group* gr = &groups[g];
        while (gr->make && gr->started < gr->total && gr->active < gr->at_once) start_conn(g);
    }
}

static int64_t last_sweep, last_guard;

/* Handles what comes for up to `ns`, the timers that fall due, and the connections past their
   deadline, then starts the connections the groups still have to start. */
static void pump(int64_t ns) {
    int64_t now = now_ns();
    int64_t wait = ns < 0 ? 0 : ns;
    if (heap_n > 0) {
        int64_t t = heap[0].at - now;
        if (t < wait) wait = t < 0 ? 0 : t;
    }
    if (wait > 50 * MS) wait = 50 * MS;
#ifdef __linux__
    struct epoll_event evs[512];
    int n = epoll_wait(pollfd, evs, 512, (int)((wait + MS - 1) / MS));
    for (int i = 0; i < n; i++) {
        conn* c = &slots[evs[i].data.u64];
        if (!c->used) continue;
        uint32_t e = evs[i].events;
        on_event(c, e & EPOLLIN, e & EPOLLOUT, e & (EPOLLHUP | EPOLLERR | EPOLLRDHUP));
    }
#else
    struct kevent evs[512];
    struct timespec ts = {(time_t)(wait / 1000000000), (long)(wait % 1000000000)};
    int n = kevent(pollfd, NULL, 0, evs, 512, &ts);
    for (int i = 0; i < n; i++) {
        conn* c = &slots[(intptr_t)evs[i].udata];
        if (!c->used || c->fd != (int)evs[i].ident) continue;
        bool rd = evs[i].filter == EVFILT_READ, wr = evs[i].filter == EVFILT_WRITE;
        bool hup = (evs[i].flags & (EV_EOF | EV_ERROR)) != 0;
        /* An end seen by the filter of writing comes again by the filter of reading. */
        on_event(c, rd, wr, hup && (rd || c->connecting));
    }
#endif
    now = now_ns();
    while (heap_n > 0 && heap[0].at <= now) {
        timer t = heap_pop();
        conn* c = &slots[t.slot];
        if (!c->used || c->gen != t.gen) continue;
        if (t.what == 1)
            open_conn(c);
        else if (c->send_at == t.at) {
            c->send_at = 0;
            do_send(c);
        }
    }
    if (most_rss_mb > 0 && now - last_guard > 20 * MS) {
        last_guard = now;
        if (!server_gone) {
            int64_t held = rss_kb();
            if (held > most_rss_mb * 1024) {
                report("the server holds %lld MB, more than --most-rss %lld: stopped", (long long)(held / 1024), (long long)most_rss_mb);
                stopped_by_stand = true;
                stop_server();
            }
        }
    }
    if (now - last_sweep > 100 * MS) {
        last_sweep = now;
        for (int i = 0; i < nslots; i++) {
            conn* c = &slots[i];
            if (!c->used || c->deadline == 0 || now < c->deadline) continue;
            if (c->any)
                fail(c, "neither answered nor closed %d ms after its request", deadline_ms);
            else if (kind == RESP)
                fail(c, "not answered in time: %zu of %zu bytes sent, %zu of %zu bytes of answers", c->out_pos, c->out_len, c->got, c->expect_len);
            else
                fail(c, "not answered in time: %zu of %zu bytes sent, %d answers still to come", c->out_pos, c->out_len, c->http_left);
        }
    }
    fill_groups();
}

static void begin(int g, int n, int once, void (*make)(conn*)) {
    groups[g] = (group){.total = n, .at_once = once, .make = make};
    fill_groups();
}

/* Runs a group to its end; whether every connection of it did what it had to. */
static bool run(int g, int n, int once, void (*make)(conn*), int64_t most_ms) {
    begin(g, n, once, make);
    int64_t until = now_ns() + most_ms * MS;
    while (groups[g].done < groups[g].total) {
        if (now_ns() > until) {
            report("%d of %d connections still open after %lld ms", groups[g].total - groups[g].done, groups[g].total, (long long)most_ms);
            for (int i = 0; i < nslots; i++)
                if (slots[i].used && slots[i].group == g) finish(&slots[i], false, true);
            groups[g].started = groups[g].total;
            break;
        }
        pump(until - now_ns());
    }
    groups[g].make = NULL;
    return groups[g].failed == 0;
}

static void end_group(int g, bool reset) {
    for (int i = 0; i < nslots; i++)
        if (slots[i].used && slots[i].group == g) finish(&slots[i], true, reset);
    groups[g].make = NULL;
}

static void pump_for(int64_t ms) {
    int64_t until = now_ns() + ms * MS;
    while (now_ns() < until) pump(until - now_ns());
}

/* Whether every connection of a group is open and has sent what it can send at once. */
static bool settled(int g) {
    if (groups[g].started < groups[g].total) return false;
    for (int i = 0; i < nslots; i++) {
        conn* c = &slots[i];
        if (c->used && c->group == g && (c->connecting || (!c->sent_all && c->gap_most == 0 && !c->no_read))) return false;
    }
    return true;
}

/* ---- what the connections send */

static void give(conn* c, buf* out, buf* expect) {
    c->out = out->p;
    c->out_len = out->n;
    if (expect) {
        c->expect = expect->p;
        c->expect_len = expect->n;
        c->own_expect = true;
    }
}

/* Requests that must be answered: in RESP, up to `most` keys of this connection set to values of up
   to `longest` bytes, read back and deleted, with a GET of a key no one sets and a PING among them;
   in HTTP, up to 2 × `most` requests of /ping. */
static void requests(conn* c, int most, size_t longest) {
    buf o = {0}, e = {0};
    if (kind == RESP) {
        int k = 1 + (int)(next_rand(&c->rng) % (uint32_t)most);
        char keys[16][48];
        const char* del[17] = {"DEL"};
        for (int j = 0; j < k; j++) {
            snprintf(keys[j], sizeof keys[j], "stand:%d:%d:%d:%d", round_no, c->group, c->index, j);
            del[j + 1] = keys[j];
            size_t len = 1 + next_rand(&c->rng) % longest;
            char* v = malloc(len);
            letters(&c->rng, (uint8_t*)v, len);
            const char* set[3] = {"SET", keys[j], v};
            size_t lens[3] = {3, strlen(keys[j]), len};
            resp(&o, 3, set, lens);
            puts_(&e, "+OK\r\n");
            const char* get[2] = {"GET", keys[j]};
            resp(&o, 2, get, NULL);
            putf(&e, "$%zu\r\n", len);
            put(&e, v, len);
            puts_(&e, "\r\n");
            free(v);
            if (j == 0) {
                const char* none[2] = {"GET", "stand:none"};
                resp(&o, 2, none, NULL);
                puts_(&e, "$-1\r\n");
                const char* ping[1] = {"PING"};
                resp(&o, 1, ping, NULL);
                puts_(&e, "+PONG\r\n");
            }
        }
        resp(&o, k + 1, del, NULL);
        putf(&e, ":%d\r\n", k);
        give(c, &o, &e);
    } else {
        int p = 1 + (int)(next_rand(&c->rng) % (uint32_t)(2 * most));
        for (int j = 0; j < p; j++) puts_(&o, PING_HTTP);
        c->http_left = p;
        give(c, &o, NULL);
    }
}

static void make_answers(conn* c) { requests(c, 8, 1500); }

static void make_one(conn* c) {
    requests(c, 1, 1500);
    c->retry_refused = true;
}

static void make_sanity(conn* c) {
    requests(c, 1, 1500);
    c->deadline = now_ns() + sanity_ms * MS;
}

static void make_half(conn* c) {
    requests(c, 8, 1500);
    c->shut_after = true;
    c->want_eof = true;
}

static void make_split(conn* c) {
    c->rcvbuf = 1;
    c->read_most = 1;
    if (kind == RESP && c->index == 0 && round_no == 1) {
        /* A value of 1 MB, sent in pieces of up to 512 bytes and read back a byte at a time. */
        buf o = {0}, e = {0};
        char key[48];
        snprintf(key, sizeof key, "stand:split:%d:%d", round_no, c->index);
        const char* set[3] = {"SET", key, (const char*)big};
        size_t lens[3] = {3, strlen(key), BIG};
        resp(&o, 3, set, lens);
        puts_(&e, "+OK\r\n");
        const char* get[2] = {"GET", key};
        resp(&o, 2, get, NULL);
        put(&e, big_answer.p, big_answer.n);
        const char* del[2] = {"DEL", key};
        resp(&o, 2, del, NULL);
        puts_(&e, ":1\r\n");
        give(c, &o, &e);
        c->send_most = 512;
        c->send_random = true;
        return;
    }
    requests(c, 2, 200);
    c->send_most = 1;
}

static void make_trickle(conn* c) {
    requests(c, 1, 16);
    c->send_most = 1;
    c->gap_most = gap_ms * MS;
}

static void make_reset(conn* c) {
    int how = c->index % 4;
    if (how == 3) {
        buf o = {0};
        if (kind == RESP) {
            const char* get[2] = {"GET", "stand:big"};
            resp(&o, 2, get, NULL);
            c->expect = big_answer.p;
            c->expect_len = big_answer.n;
        } else {
            for (int j = 0; j < 2000; j++) puts_(&o, PING_HTTP);
            c->http_left = 2000;
        }
        c->out = o.p;
        c->out_len = o.n;
        c->reset_in = 1 + next_rand(&c->rng) % (kind == RESP ? (uint32_t)(big_answer.n - 2) : 100000u);
        return;
    }
    requests(c, 8, 1500);
    c->any = true;
    if (how == 0) c->reset_out = 0;
    if (how == 1) c->reset_out = 1 + next_rand(&c->rng) % (c->out_len - 1);
    if (how == 2) c->reset_in = 0;
}

static void make_noread(conn* c) {
    buf o = {0};
    if (kind == RESP) {
        const char* get[2] = {"GET", value_size ? "stand:value" : "stand:big"};
        int n = value_size ? (int)((1 << 30) / value_size) : 1000;
        for (int j = 0; j < n; j++) resp(&o, 2, get, NULL);
    } else {
        for (int j = 0; j < 100000; j++) puts_(&o, PING_HTTP);
    }
    give(c, &o, NULL);
    c->no_read = true;
    c->hold = true;
}

static void make_idle(conn* c) {
    c->hold = true;
    c->any = true;
}

/* The first part of a request, the rest never sent. */
static void make_halfway(conn* c) {
    buf o = {0};
    if (kind == RESP)
        puts_(&o, "*3\r\n$3\r\nSET\r\n$10\r\nstand:half\r\n$10\r\n12345");
    else
        puts_(&o, "GET /ping HTTP/1.1\r\nHost: sta");
    give(c, &o, NULL);
    c->hold = true;
    c->any = true;
}

/* A value of 1 MB sent a byte at a time, one every 50 ms: never whole. */
static void make_slow(conn* c) {
    buf o = {0};
    if (kind == RESP) {
        const char* set[3] = {"SET", "stand:slow", (const char*)big};
        size_t lens[3] = {3, 10, BIG};
        resp(&o, 3, set, lens);
    } else {
        puts_(&o, "GET /ping HTTP/1.1\r\nHost: stand\r\nContent-Length: 1048576\r\n\r\n");
        put(&o, big, BIG);
    }
    give(c, &o, NULL);
    c->send_most = 1;
    c->gap_most = 50 * MS;
    c->hold = true;
    c->any = true;
}

/* What is not the protocol, drawn from the connection's own numbers. */
static void make_fuzz(conn* c) {
    buf o = {0};
    uint32_t* r = &c->rng;
    int what = (int)(next_rand(r) % 16);
    buf valid = {0};
    if (kind == RESP) {
        const char* set[3] = {"SET", "stand:fuzz", "value"};
        resp(&valid, 3, set, NULL);
        switch (what) {
        case 0: puts_(&o, "*2\r\n$99999999999\r\n"); break;
        case 1:
            puts_(&o, "*1000000\r\n");
            for (int j = 0; j < 100; j++) puts_(&o, "$3\r\nGET\r\n");
            break;
        case 2: puts_(&o, "*99999999999999999999999\r\n$3\r\nGET\r\n"); break;
        case 3: puts_(&o, next_rand(r) % 2 ? "*-1\r\n" : "*0\r\n"); break;
        case 4: put(&o, valid.p, 1 + next_rand(r) % (valid.n - 1)); break;
        case 5:
            put(&o, valid.p, valid.n);
            o.p[next_rand(r) % o.n] = (uint8_t)next_rand(r);
            break;
        case 6: {
            size_t n = 1 + next_rand(r) % 2000;
            for (size_t j = 0; j < n; j++) {
                uint8_t b = (uint8_t)next_rand(r);
                put(&o, &b, 1);
            }
            break;
        }
        case 7:
            puts_(&o, "*3\r\n$3\r\nSET\r\n$1\r\nk\r\n$1048577\r\n");
            put(&o, big, next_rand(r) % 65536);
            break;
        case 8:
            puts_(&o, "*");
            for (int j = 0; j < 100000; j++) put(&o, "9", 1);
            break;
        case 9: puts_(&o, "*1\r\n$4\r\nPING"); break;
        case 10:
            for (int j = 0; j < 1000; j++) puts_(&o, "\r\n");
            break;
        case 11: put(&o, "*2\r\n$3\r\nGET\r\n$3\r\n\xff\x00\xfe\r\n", 22); break;
        case 12:
            puts_(&o, "*1025\r\n");
            for (int j = 0; j < 1025; j++) puts_(&o, "$1\r\na\r\n");
            break;
        case 13:
            for (int j = 0; j < 10; j++) put(&o, valid.p, valid.n);
            puts_(&o, "*1\r\n$4\r\nPONG\r\n$$$$");
            break;
        case 14: puts_(&o, "*2\r\n$3\r\nGET\r\n$-5\r\n"); break;
        default:
            puts_(&o, "*3\r\n$3\r\nSET\r\n$1\r\nk\r\n$1048576\r\n");
            put(&o, big, next_rand(r) % 65536);
            break;
        }
    } else {
        puts_(&valid, PING_HTTP);
        switch (what) {
        case 0: puts_(&o, "GET /ping HTTP/1.1\r\nHost: s\r\nContent-Length: 99999999999999999999\r\n\r\n"); break;
        case 1: puts_(&o, "GET /ping HTTP/1.1\r\nHost: s\r\nContent-Length: 9223372036854775808\r\n\r\nabc"); break;
        case 2: puts_(&o, "GET /ping HTTP/1.1\r\nHost: s\r\nContent-Length: 18446744073709551615\r\n\r\nabc"); break;
        case 3: puts_(&o, "GET /ping HTTP/1.1\r\nHost: s\r\nContent-Length: 5\r\nContent-Length: 6\r\n\r\nabcdef"); break;
        case 4: puts_(&o, "GET /ping HTTP/1.1\r\nHost: s\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\n"); break;
        case 5:
            puts_(&o, "GET /ping HTTP/1.1\r\nHost: s\r\nX-A: ");
            for (int j = 0; j < 10000; j++) put(&o, "a", 1);
            puts_(&o, "\r\n\r\n");
            break;
        case 6: puts_(&o, "GET /ping HTTP/1.1\r\n\r\n"); break;
        case 7: put(&o, "\x01\x02\x03 / HTTP/1.1\r\nHost: s\r\n\r\n", 28); break;
        case 8: put(&o, valid.p, 1 + next_rand(r) % (valid.n - 1)); break;
        case 9: {
            size_t n = 1 + next_rand(r) % 2000;
            for (size_t j = 0; j < n; j++) {
                uint8_t b = (uint8_t)next_rand(r);
                put(&o, &b, 1);
            }
            break;
        }
        case 10: puts_(&o, "GET /ping HTTP/1.1\nHost: s\n\n"); break;
        case 11: puts_(&o, next_rand(r) % 2 ? "GET /ping HTTP/9.9\r\nHost: s\r\n\r\n" : "GET /ping HTTP/1.1 more\r\nHost: s\r\n\r\n"); break;
        case 12:
            puts_(&o, "GET /ping HTTP/1.1\r\nHost: s\r\n");
            for (int j = 0; j < 2000; j++) puts_(&o, "a: b\r\n");
            puts_(&o, "\r\n");
            break;
        case 13: puts_(&o, "GET /ping HTTP/1.1\r\nHost s\r\n\r\n"); break;
        case 14:
            puts_(&o, next_rand(r) % 2 ? "GET /ping HTTP/1.1\r\nHost: s\r\nContent-Length: 1048577\r\n\r\n"
                                       : "GET /ping HTTP/1.1\r\nHost: s\r\nContent-Length: -1\r\n\r\n");
            put(&o, big, next_rand(r) % 65536);
            break;
        default:
            /* The last draw is shared by five, so that the cases before it come as often as
               they did: the ways a request is smuggled past a server in front (choice 1772). */
            switch (next_rand(r) % 5) {
            case 0: puts_(&o, "POST /ping HTTP/1.1\r\nHost: s\r\nContent-Length: 4\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\nGET /ping HTTP/1.1\r\nHost: s\r\n\r\n"); break;
            case 1: puts_(&o, "GET /ping HTTP/1.1\r\nHost: s\r\nX-A: one\r\n Transfer-Encoding: chunked\r\n\r\n"); break;
            case 2: puts_(&o, "POST /ping HTTP/1.1\r\nHost: s\r\nTransfer-Encoding : chunked\r\nContent-Length: 3\r\n\r\nabc"); break;
            case 3: puts_(&o, next_rand(r) % 2 ? "POST /ping HTTP/1.1\r\nHost: s\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\nabc\r\n0\r\n\r\n"
                                               : "POST /ping HTTP/1.1\r\nHost: s\r\nTransfer-Encoding: chunked\r\n\r\nffffffffffffffffff\r\nabc\r\n"); break;
            default:
                for (int j = 0; j < 10; j++) put(&o, valid.p, valid.n);
                put(&o, "GET /p\x00ng HTTP/1.1\r\nHost: s\r\nContent-Length: 0000000000000000000000001\r\n\r\nx", 75);
                break;
            }
            break;
        }
    }
    free(valid.p);
    give(c, &o, NULL);
    c->any = true;
    c->deadline = 0;
    if (next_rand(r) % 10 == 0)
        c->reset_out = o.n > 1 ? 1 + next_rand(r) % (o.n - 1) : 0;
    else
        c->shut_after = true;
}

/* ---- the checks of every round */

static bool sane(void) {
    const char* was = phase;
    phase = "a plain client";
    bool ok = run(G_SANITY, 1, 1, make_sanity, sanity_ms + 1000);
    phase = was;
    return ok;
}

static int fds_now(void) {
    return count_fds();
}

/* The descriptors of the server, once they come back to `base` or after five seconds. */
static int fds_back(int base) {
    int64_t until = now_ns() + 5000 * MS;
    int n = fds_now();
    while (n > base && now_ns() < until) {
        pump_for(50);
        n = fds_now();
    }
    return n;
}

/* ---- the scenarios */

static int64_t measured_cpu_ms = -1, growth_kb = -1, term_ms = -1;
static int base_fds;

/* SET stand:big, the value of 1 MB the RESP scenarios read, and stand:value, of --value bytes. */
static void make_set_big(conn* c) {
    buf o = {0}, e = {0};
    const char* set[3] = {"SET", "stand:big", (const char*)big};
    size_t lens[3] = {3, 9, BIG};
    resp(&o, 3, set, lens);
    puts_(&e, "+OK\r\n");
    if (value_size) {
        const char* value[3] = {"SET", "stand:value", (const char*)big};
        size_t sizes[3] = {3, 11, (size_t)value_size};
        resp(&o, 3, value, sizes);
        puts_(&e, "+OK\r\n");
    }
    give(c, &o, &e);
}

static void scenario_round(void) {
    if (strcmp(scenario, "answers") == 0) {
        run(G_MAIN, total, at_once, make_answers, 300000);
    } else if (strcmp(scenario, "half") == 0) {
        run(G_MAIN, total, at_once, make_half, 300000);
    } else if (strcmp(scenario, "split") == 0) {
        run(G_MAIN, total, at_once, make_split, 300000);
    } else if (strcmp(scenario, "trickle") == 0) {
        run(G_MAIN, total, at_once, make_trickle, 300000);
    } else if (strcmp(scenario, "reset") == 0) {
        run(G_MAIN, total, at_once, make_reset, 300000);
    } else if (strcmp(scenario, "fuzz") == 0) {
        for (int done = 0; done < total; done += 1000) {
            int n = total - done < 1000 ? total - done : 1000;
            run(G_MAIN, n, at_once, make_fuzz, 120000);
            if (!server_runs()) return;
            if (!sane()) report("a plain client was not answered after %d requests that are not the protocol", done + n);
        }
    } else if (strcmp(scenario, "fds") == 0) {
        phase = "holding more connections than the server has descriptors";
        begin(G_HOLD, total, total, make_idle);
        pump_for(2000);
        int64_t before = cpu_ns();
        pump_for(1000);
        measured_cpu_ms = (cpu_ns() - before) / MS;
        if (measured_cpu_ms > 500) report("the server spins: %lld ms of the processor in a second with its descriptors taken", (long long)measured_cpu_ms);
        end_group(G_HOLD, false);
        phase = "a request on each, as many at once as the server has descriptors and more";
        run(G_MAIN, total, total, make_one, 120000);
    } else if (strcmp(scenario, "noread") == 0) {
        phase = "clients that do not read";
        int64_t base = peak_kb();
        begin(G_HOLD, total, total, make_noread);
        pump_for(3000);
        int64_t grew = peak_kb() - base;
        if (grew > growth_kb) growth_kb = grew;
        int64_t most = most_growth_mb ? most_growth_mb * 1024 : (64 + 8 * (int64_t)total) * 1024;
        if (memory && grew > most) report("the memory of the server grew by %lld KB, more than %lld", (long long)grew, (long long)most);
        if (!sane()) report("a plain client was not answered beside %d clients that do not read", total);
        if (idle_ms > 0) {
            /* Cut is told by the server's descriptors: the end of a connection that it closes
               with answers still unsent waits behind them, and a client that does not read may
               never see it. */
            phase = "waiting for the clients that do not read to be cut";
            int64_t until = now_ns() + (idle_ms + 3000) * MS;
            int held = fds_now();
            while (held > base_fds && now_ns() < until) {
                pump_for(50);
                held = fds_now();
            }
            if (held > base_fds) report("the server still holds %d descriptors 3000 ms after the idle limit, %d before the clients that do not read", held, base_fds);
        }
        end_group(G_HOLD, true);
    }
}

static void term(void) {
    phase = "connections in the middle of a request";
    begin(G_HOLD, total, total, make_halfway);
    begin(G_SIDE, 10, 10, make_slow);
    int64_t until = now_ns() + 60000 * MS;
    while (!settled(G_HOLD) && now_ns() < until) pump(until - now_ns());
    if (!settled(G_HOLD)) report("the connections were not all open in 60 seconds");
    pump_for(200);
    phase = "SIGTERM";
    int64_t t0 = now_ns();
    kill(server, SIGTERM);
    int64_t most = (int64_t)(grace_ms + margin_ms) * MS;
    while (server_runs() && now_ns() - t0 < most) pump(5 * MS);
    term_ms = (now_ns() - t0) / MS;
    if (server_runs()) {
        report("the server still runs %lld ms after SIGTERM", (long long)term_ms);
        stop_server();
    } else if (!WIFEXITED(server_status) || WEXITSTATUS(server_status) != 0) {
        tell_end();
    }
    end_group(G_HOLD, false);
    end_group(G_SIDE, false);
}

/* ---- main */

static int arg_int(char** argv, int* i, int argc) {
    if (*i + 1 >= argc) usage("a number is missing");
    char* end;
    long v = strtol(argv[++*i], &end, 10);
    if (*end || v < 0) usage("not a number");
    return (int)v;
}

int main(int argc, char** argv) {
    if (argc < 4) usage("too few arguments");
    if (strcmp(argv[1], "resp") == 0)
        kind = RESP;
    else if (strcmp(argv[1], "http") == 0)
        kind = HTTP;
    else
        usage("resp or http");
    scenario = argv[2];
    const char* known[] = {"answers", "fds", "reset", "half", "split", "trickle", "noread", "term", "fuzz"};
    bool found = false;
    for (size_t i = 0; i < sizeof known / sizeof *known; i++) found |= strcmp(scenario, known[i]) == 0;
    if (!found) usage("no such scenario");
    bool at_once_given = false;
    int i = 3;
    for (; i < argc && strcmp(argv[i], "--") != 0; i++) {
        const char* a = argv[i];
        if (strcmp(a, "--conns") == 0) total = arg_int(argv, &i, argc);
        else if (strcmp(a, "--at-once") == 0) at_once = arg_int(argv, &i, argc), at_once_given = true;
        else if (strcmp(a, "--rounds") == 0) rounds = arg_int(argv, &i, argc);
        else if (strcmp(a, "--seed") == 0) seed = (uint32_t)arg_int(argv, &i, argc);
        else if (strcmp(a, "--nofile") == 0) nofile = arg_int(argv, &i, argc);
        else if (strcmp(a, "--grace") == 0) grace_ms = arg_int(argv, &i, argc);
        else if (strcmp(a, "--margin") == 0) margin_ms = arg_int(argv, &i, argc);
        else if (strcmp(a, "--idle") == 0) idle_ms = arg_int(argv, &i, argc);
        else if (strcmp(a, "--gap") == 0) gap_ms = arg_int(argv, &i, argc);
        else if (strcmp(a, "--value") == 0) value_size = arg_int(argv, &i, argc);
        else if (strcmp(a, "--deadline") == 0) deadline_ms = arg_int(argv, &i, argc);
        else if (strcmp(a, "--sanity") == 0) sanity_ms = arg_int(argv, &i, argc);
        else if (strcmp(a, "--most-growth") == 0) most_growth_mb = arg_int(argv, &i, argc);
        else if (strcmp(a, "--sources") == 0) sources = arg_int(argv, &i, argc);
        else if (strcmp(a, "--timeout") == 0) timeout_s = arg_int(argv, &i, argc);
        else if (strcmp(a, "--no-memory") == 0) memory = false;
        else if (strcmp(a, "--most-rss") == 0) most_rss_mb = arg_int(argv, &i, argc);
        else usage("an unknown option");
    }
    if (i + 1 >= argc) usage("no server after --");
    server_argv = argv + i + 1;
    if (total < 1 || at_once < 1 || rounds < 1 || sources < 1 || sources > 254 || value_size > BIG) usage("a number out of range");
    if (!at_once_given && at_once > total) at_once = total;
    if (strcmp(scenario, "fds") == 0 && nofile == 0) nofile = 256;
    if (strcmp(scenario, "term") == 0 && grace_ms == 0) usage("term needs --grace, the server's own");
    if (strcmp(scenario, "term") == 0) rounds = 1;

    signal(SIGPIPE, SIG_IGN);
    signal(SIGALRM, on_alarm);
    alarm((unsigned)timeout_s);
    struct rlimit rl;
    if (getrlimit(RLIMIT_NOFILE, &rl) == 0) {
        rlim_t want = rl.rlim_max;
        if (want == RLIM_INFINITY || want > 1048576) want = 1048576;
        for (rl.rlim_cur = want; rl.rlim_cur > 1024 && setrlimit(RLIMIT_NOFILE, &rl) != 0; rl.rlim_cur /= 2) {
        }
    }
    uint32_t r = seed * 2246822519u + 1;
    big = malloc(BIG);
    letters(&r, big, BIG);
#ifdef __linux__
    pollfd = epoll_create1(0);
#else
    pollfd = kqueue();
#endif
    if (pollfd < 0) die("poll");

    start_server();
    int64_t started = now_ns();
    putf(&big_answer, "$%d\r\n", BIG);
    put(&big_answer, big, BIG);
    puts_(&big_answer, "\r\n");
    if (kind == RESP) {
        phase = "setting the value of 1 MB";
        if (!run(G_MAIN, 1, 1, make_set_big, 10000)) {
            stop_server();
            return 1;
        }
    }
    phase = "warming up";
    run(G_MAIN, 64, 16, make_answers, 30000);
    if (!sane() || !server_runs()) {
        if (server_gone) tell_end();
        stop_server();
        return 1;
    }
    pump_for(200);
    int base = base_fds = fds_now();
    int fds_after[64];
    int64_t peaks[64], idle[64];
    int done_rounds = 0;
    if (strcmp(scenario, "term") == 0) {
        term();
    } else {
        for (round_no = 1; round_no <= rounds && round_no <= 64; round_no++) {
            char name[64];
            snprintf(name, sizeof name, "%s, round %d", scenario, round_no);
            phase = strdup(name);
            scenario_round();
            if (!server_runs()) {
                tell_end();
                break;
            }
            if (!sane()) report("a plain client was not answered after the round");
            fds_after[done_rounds] = fds_back(base);
            if (fds_after[done_rounds] > base) report("the server holds %d descriptors, %d before", fds_after[done_rounds], base);
            peaks[done_rounds] = peak_kb();
            idle[done_rounds] = rss_kb();
            done_rounds++;
        }
        if (memory && done_rounds >= 3 && idle[done_rounds - 1] > idle[1] + idle[1] / 10 + 2048)
            report("the server held %lld KB idle after the second round and %lld KB after the last", (long long)idle[1], (long long)idle[done_rounds - 1]);
    }
    double seconds = (double)(now_ns() - started) / 1e9;
    stop_server();
    printf("{\"kind\":\"%s\",\"scenario\":\"%s\",\"stood\":%s,\"failures\":%d,\"connections\":%" PRIu64 ",\"failed\":%" PRIu64 ",\"retried\":%" PRIu64
           ",\"cut\":%" PRIu64 ",\"ports_waited\":%" PRIu64 ",\"fds_base\":%d,\"fds_after\":[",
           kind == RESP ? "resp" : "http", scenario, failures == 0 ? "true" : "false", failures, made, finished_badly, retried, cuts, ports_waited, base);
    for (int k = 0; k < done_rounds; k++) printf("%s%d", k ? "," : "", fds_after[k]);
    printf("],\"idle_kb\":[");
    for (int k = 0; k < done_rounds; k++) printf("%s%lld", k ? "," : "", (long long)idle[k]);
    printf("],\"peak_kb\":[");
    for (int k = 0; k < done_rounds; k++) printf("%s%lld", k ? "," : "", (long long)peaks[k]);
    printf("],\"cpu_held_ms\":%lld,\"growth_kb\":%lld,\"term_ms\":%lld,\"seconds\":%.2f}\n", (long long)measured_cpu_ms,
           (long long)growth_kb, (long long)term_ms, seconds);
    return failures == 0 ? 0 : 1;
}
