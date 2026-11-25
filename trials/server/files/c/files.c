/* The file server of the server lane in C (choices 1861-1871): GET and HEAD of the files under a
   folder, over HTTP/1.1 with connections kept open. The program a C programmer writes for speed,
   built as `../../http/c/http.c` is: THREADS threads, each with a listener of its own on the port
   (SO_REUSEPORT) and a loop of its own (epoll; kqueue on macOS). The disk is asked from the loop
   (`stat`, `open`, `pread`), as nginx does without `aio threads`.

       files PORT ROOT [THREADS]       THREADS: the cores this process may run on, when not given

   What every version does the same way, in SG, Rust, Zig and C:

   - the request is read and checked as `../../http/c/http.c` says; GET and HEAD are answered,
     another method 405 with `allow: GET, HEAD`;
   - the target is a path from `/`, its query (from `?`) left out; it is percent-decoded, and 400
     answers a target that does not start with `/` or holds a byte that is not visible ASCII, a `%` without two hex digits, a `/` written
     `%2F`, and a decoded path with a zero byte, a `\`, a segment `.` or `..`, an empty segment
     other than the last (`//`), or bytes that are not UTF-8;
   - the file is ROOT followed by the path, `index.html` after it when the path ends with `/`; links
     are followed, as nginx does. What it is is asked first (`stat`): not there 404, not allowed 403,
     a folder 301 to the path written with `/` after it (404 when the path ended with `/` already,
     there is no listing), anything but a file 404 (a FIFO is never opened, it would wait). The
     file is opened next, before its headers are looked at, so one not readable is 403 whatever
     they ask, as nginx answers it (choice 1873);
   - `If-Modified-Since`, read as RFC 9110 reads a date (IMF-fixdate, RFC 850, asctime; the day of
     the week must be the date's; a moment `DateTime` cannot hold is no date), at or after the
     second the file was last written: 304 with `last-modified`;
   - `Range` of one range of bytes, `FIRST-LAST`, `FIRST-` or `-COUNT` (a number longer than 2^62
     is 2^62): 206 with `content-range`; one starting past the end, or `-0`, or any of a file of
     0 bytes, 416 with `content-range: bytes * /SIZE`; anything else (more than one range, a
     FIRST after LAST, another unit, a space) is let be and the whole file sent; with `If-Range`
     the `Range` is let be;
   - the answer has `last-modified`, `accept-ranges: bytes`, a `content-type` from the extension of
     the file's name in any case (`typeOf` below, the same table in each), and the body read and
     sent in pieces of 64 KB, no more read than the answer says; a file found shorter than its
     length ends the answer cut and the connection closed. No file is kept in memory.

   Here the head and the first piece go out in one send, and the next piece is read only once the
   one before is sent, so a client that does not read holds one piece.

   Built with -DSENDFILE (Linux), the body goes from the file to the socket with `sendfile`, which
   copies nothing into the program: not one of the four, measured beside them as what the copy
   costs (choice 1864). */

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
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>
#ifdef __linux__
#include <sys/epoll.h>
#include <sys/sendfile.h>
#else
#include <sys/event.h>
#endif

#define FIRST_BUFFER 4096
#define MOST_HEAD 8192
#define MOST_BODY (1 << 20)
#define PIECE 65536
#define FAR ((int64_t)1 << 62)

static const char* root;
static size_t root_len;

/* ---- connections */

typedef struct {
    int fd;
    uint8_t* in;
    size_t in_cap, in_len;
    uint8_t* out;
    size_t out_cap, out_len, out_at;
    bool waiting_out;
    /* the file whose body is being sent, -1 for none */
    int file;
    int64_t file_at, file_left;
    /* closed once what waits is sent */
    bool close_after;
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

static void puts_(conn* c, const char* text) { put(c, text, strlen(text)); }

/* ---- HTTP dates */

static const char days[] = "MonTueWedThuFriSatSun";
static const char months[] = "JanFebMarAprMayJunJulAugSepOctNovDec";

static int64_t floor_div(int64_t a, int64_t b) { int64_t q = a / b; if ((a % b != 0) && ((a < 0) != (b < 0))) q--; return q; }
static int64_t floor_mod(int64_t a, int64_t b) { int64_t r = a % b; if (r != 0 && ((r < 0) != (b < 0))) r += b; return r; }

/* days since 1970 of a date of the Gregorian calendar (Howard Hinnant's days_from_civil) */
static int64_t days_from_civil(int64_t y, int64_t m, int64_t d) {
    y -= m <= 2;
    int64_t era = floor_div(y, 400);
    int64_t yoe = y - era * 400;
    int64_t doy = (153 * (m + (m > 2 ? -3 : 9)) + 2) / 5 + d - 1;
    int64_t doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    return era * 146097 + doe - 719468;
}

static void civil_from_days(int64_t z, int64_t* y, int* m, int* d) {
    z += 719468;
    int64_t era = floor_div(z, 146097);
    int64_t doe = z - era * 146097;
    int64_t yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    int64_t doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    int64_t mp = (5 * doy + 2) / 153;
    *d = (int)(doy - (153 * mp + 2) / 5 + 1);
    *m = (int)(mp < 10 ? mp + 3 : mp - 9);
    *y = yoe + era * 400 + (*m <= 2);
}

/* `Sun, 06 Nov 1994 08:49:37 GMT` into `out`, which has room for 64 bytes: 29 and a zero for
   every year a date holds, the rest so that no compiler takes a year of more digits to cut. */
static void http_date(char* out, int64_t seconds) {
    int64_t dn = floor_div(seconds, 86400), rest = floor_mod(seconds, 86400);
    int64_t y;
    int m, d;
    civil_from_days(dn, &y, &m, &d);
    int w = (int)floor_mod(dn + 3, 7);
    snprintf(out, 64, "%.3s, %02d %.3s %04lld %02d:%02d:%02d GMT", days + 3 * w, d, months + 3 * (m - 1), (long long)y,
             (int)(rest / 3600), (int)(rest / 60 % 60), (int)(rest % 60));
}

static bool is_leap(int64_t y) { return y % 4 == 0 && (y % 100 != 0 || y % 400 == 0); }

static int days_in(int64_t y, int m) {
    static const int n[] = {31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31};
    return m == 2 && is_leap(y) ? 29 : n[m - 1];
}

static bool digits_at(const uint8_t* t, size_t n, size_t* at, int count, int* value) {
    int v = 0;
    for (int i = 0; i < count; i++) {
        if (*at >= n || t[*at] < '0' || t[*at] > '9') return false;
        v = v * 10 + (t[*at] - '0');
        (*at)++;
    }
    *value = v;
    return true;
}

static bool expect(const uint8_t* t, size_t n, size_t* at, char c) {
    if (*at >= n || t[*at] != (uint8_t)c) return false;
    (*at)++;
    return true;
}

/* which of the names of `width` letters packed in `names` is at `at` */
static int name_at(const uint8_t* t, size_t n, size_t* at, const char* names, int count, size_t width) {
    if (*at + width > n) return -1;
    for (int i = 0; i < count; i++)
        if (memcmp(t + *at, names + (size_t)i * width, width) == 0) { *at += width; return i; }
    return -1;
}

/* An HTTP date (RFC 9110: IMF-fixdate, RFC 850, asctime) into seconds since 1970; false for one that
   is not, or a moment `DateTime` cannot hold, as SG's `DateTime.parseHttpDate`. */
static bool parse_http_date(const uint8_t* t, size_t n, int64_t* out) {
    size_t at = 0;
    int named, day = 0, month, year = 0, hour, minute, second;
    if (n > 3 && t[3] == ',') {
        if ((named = name_at(t, n, &at, days, 7, 3)) < 0 || !expect(t, n, &at, ',') || !expect(t, n, &at, ' ')) return false;
        if (!digits_at(t, n, &at, 2, &day) || !expect(t, n, &at, ' ')) return false;
        if ((month = name_at(t, n, &at, months, 12, 3)) < 0 || !expect(t, n, &at, ' ')) return false;
        if (!digits_at(t, n, &at, 4, &year) || !expect(t, n, &at, ' ')) return false;
    } else if (n > 3 && t[3] == ' ') {
        if ((named = name_at(t, n, &at, days, 7, 3)) < 0 || !expect(t, n, &at, ' ')) return false;
        if ((month = name_at(t, n, &at, months, 12, 3)) < 0 || !expect(t, n, &at, ' ')) return false;
        if (at < n && t[at] == ' ') {
            at++;
            if (!digits_at(t, n, &at, 1, &day)) return false;
        } else if (!digits_at(t, n, &at, 2, &day)) {
            return false;
        }
        if (!expect(t, n, &at, ' ')) return false;
    } else {
        static const char* rest[] = {"day", "sday", "nesday", "rsday", "day", "urday", "day"};
        if ((named = name_at(t, n, &at, days, 7, 3)) < 0) return false;
        size_t r = strlen(rest[named]);
        if (at + r > n || memcmp(t + at, rest[named], r) != 0) return false;
        at += r;
        if (!expect(t, n, &at, ',') || !expect(t, n, &at, ' ') || !digits_at(t, n, &at, 2, &day) || !expect(t, n, &at, '-')) return false;
        if ((month = name_at(t, n, &at, months, 12, 3)) < 0 || !expect(t, n, &at, '-')) return false;
        if (!digits_at(t, n, &at, 2, &year) || !expect(t, n, &at, ' ')) return false;
        year += year < 70 ? 2000 : 1900;
    }
    month += 1;
    if (!digits_at(t, n, &at, 2, &hour) || !expect(t, n, &at, ':') || !digits_at(t, n, &at, 2, &minute) || !expect(t, n, &at, ':') ||
        !digits_at(t, n, &at, 2, &second) || !expect(t, n, &at, ' '))
        return false;
    if (t[3] == ' ') {
        if (!digits_at(t, n, &at, 4, &year)) return false;
    } else if (!expect(t, n, &at, 'G') || !expect(t, n, &at, 'M') || !expect(t, n, &at, 'T')) {
        return false;
    }
    if (at != n) return false;
    if (day < 1 || day > days_in(year, month) || hour > 23 || minute > 59 || second > 59 || year < 1000 || year > 3000) return false;
    int64_t dn = days_from_civil(year, month, day);
    int64_t s = dn * 86400 + hour * 3600 + minute * 60 + second;
    if (s < -9223372036LL || s > 9223372036LL) return false;
    if (floor_mod(dn + 3, 7) != named) return false;
    *out = s;
    return true;
}

/* ---- what is asked */

static bool token_char(uint8_t c) {
    return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || strchr("!#$%&'*+-.^_`|~", c) != NULL;
}

static bool has_token(const uint8_t* v, size_t n, const char* word) {
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

static int hex(uint8_t c) {
    if (c >= '0' && c <= '9') return c - '0';
    if (c >= 'a' && c <= 'f') return c - 'a' + 10;
    if (c >= 'A' && c <= 'F') return c - 'A' + 10;
    return -1;
}

static bool utf8(const uint8_t* s, size_t n) {
    size_t i = 0;
    while (i < n) {
        uint8_t c = s[i];
        if (c < 0x80) { i++; continue; }
        size_t len;
        uint32_t cp;
        if (c >= 0xC2 && c <= 0xDF) { len = 2; cp = c & 0x1F; }
        else if (c >= 0xE0 && c <= 0xEF) { len = 3; cp = c & 0x0F; }
        else if (c >= 0xF0 && c <= 0xF4) { len = 4; cp = c & 0x07; }
        else return false;
        if (i + len > n) return false;
        for (size_t k = 1; k < len; k++) {
            if ((s[i + k] & 0xC0) != 0x80) return false;
            cp = cp << 6 | (s[i + k] & 0x3F);
        }
        if ((len == 3 && (cp < 0x800 || (cp >= 0xD800 && cp <= 0xDFFF))) || (len == 4 && (cp < 0x10000 || cp > 0x10FFFF))) return false;
        i += len;
    }
    return true;
}

/* Decodes the path of `target` into `path` (with room for the root, `index.html` and a zero),
   checked as the top of this file says; false for 400. */
static bool decode_path(const uint8_t* target, size_t n, char* path, size_t* len) {
    if (n == 0 || target[0] != '/') return false;
    const uint8_t* q = memchr(target, '?', n);
    if (q) n = (size_t)(q - target);
    size_t o = 0;
    for (size_t i = 0; i < n; i++) {
        uint8_t c = target[i];
        if (c == '%') {
            if (i + 2 >= n) return false;
            int h = hex(target[i + 1]), l = hex(target[i + 2]);
            if (h < 0 || l < 0) return false;
            c = (uint8_t)(h << 4 | l);
            if (c == '/') return false;
            i += 2;
        }
        if (c == 0 || c == '\\') return false;
        path[o++] = (char)c;
    }
    size_t start = 1;
    for (size_t i = 1; i <= o; i++) {
        if (i < o && path[i] != '/') continue;
        size_t seg = i - start;
        if ((seg == 0 && i != o) || (seg == 1 && path[start] == '.') || (seg == 2 && path[start] == '.' && path[start + 1] == '.')) return false;
        start = i + 1;
    }
    if (!utf8((const uint8_t*)path, o)) return false;
    *len = o;
    return true;
}

static const char* type_of(const char* path, size_t n) {
    static const char* const table[][2] = {
        {"html", "text/html; charset=utf-8"}, {"htm", "text/html; charset=utf-8"}, {"css", "text/css; charset=utf-8"},
        {"js", "text/javascript; charset=utf-8"}, {"mjs", "text/javascript; charset=utf-8"}, {"json", "application/json"},
        {"txt", "text/plain; charset=utf-8"}, {"xml", "application/xml"}, {"svg", "image/svg+xml"}, {"png", "image/png"},
        {"jpg", "image/jpeg"}, {"jpeg", "image/jpeg"}, {"gif", "image/gif"}, {"webp", "image/webp"}, {"ico", "image/x-icon"},
        {"wasm", "application/wasm"}, {"pdf", "application/pdf"}, {"zip", "application/zip"}, {"gz", "application/gzip"},
        {"mp4", "video/mp4"}, {"webm", "video/webm"}, {"mp3", "audio/mpeg"}, {"woff2", "font/woff2"}, {"woff", "font/woff"},
    };
    size_t slash = 0, dot = 0;
    for (size_t i = 0; i < n; i++) {
        if (path[i] == '/') slash = i;
        if (path[i] == '.') dot = i;
    }
    if (dot <= slash) return "application/octet-stream";
    const char* ext = path + dot + 1;
    size_t len = n - dot - 1;
    for (size_t i = 0; i < sizeof table / sizeof table[0]; i++)
        if (strlen(table[i][0]) == len && strncasecmp(ext, table[i][0], len) == 0) return table[i][1];
    return "application/octet-stream";
}

static int64_t digits_far(const uint8_t* v, size_t n, size_t* at, bool* some) {
    int64_t x = 0;
    size_t start = *at;
    while (*at < n && v[*at] >= '0' && v[*at] <= '9') {
        if (x > FAR / 10) x = FAR;
        else {
            x = x * 10 + (v[*at] - '0');
            if (x > FAR) x = FAR;
        }
        (*at)++;
    }
    *some = *at > start;
    return x;
}

/* `bytes=...` of one range: 0 none, 1 FIRST-LAST, 2 the last LAST bytes */
static int range_of(const uint8_t* v, size_t n, int64_t* first, int64_t* last) {
    if (n < 6 || memcmp(v, "bytes=", 6) != 0) return 0;
    size_t at = 6;
    bool has_first, has_last;
    int64_t f = digits_far(v, n, &at, &has_first);
    if (at >= n || v[at] != '-') return 0;
    at++;
    int64_t l = digits_far(v, n, &at, &has_last);
    if (at != n) return 0;
    if (!has_first) {
        if (!has_last) return 0;
        *last = l;
        return 2;
    }
    if (!has_last) { *first = f; *last = FAR; return 1; }
    if (f > l) return 0;
    *first = f;
    *last = l;
    return 1;
}

/* ---- answers */

static __thread char date_now[64];
static __thread time_t date_second = -1;

static void head(conn* c, const char* line, bool keep, const char* type, int64_t length) {
    time_t now = time(NULL);
    if (now != date_second) {
        date_second = now;
        http_date(date_now, (int64_t)now);
    }
    char h[512];
    int n = snprintf(h, sizeof h, "HTTP/1.1 %s\r\ndate: %s\r\n%s", line, date_now, keep ? "" : "connection: close\r\n");
    put(c, h, (size_t)n);
    if (type) {
        puts_(c, "content-type: ");
        puts_(c, type);
        puts_(c, "\r\n");
    }
    if (length >= 0) {
        n = snprintf(h, sizeof h, "content-length: %lld\r\n", (long long)length);
        put(c, h, (size_t)n);
    }
}

static void simple(conn* c, int status, bool keep, bool head_only, const char* extra) {
    const char* line;
    const char* body;
    switch (status) {
    case 301: line = "301 Moved Permanently"; body = "moved permanently"; break;
    case 403: line = "403 Forbidden"; body = "forbidden"; break;
    case 404: line = "404 Not Found"; body = "not found"; break;
    case 405: line = "405 Method Not Allowed"; body = "method not allowed"; extra = "allow: GET, HEAD\r\n"; break;
    case 416: line = "416 Range Not Satisfiable"; body = "range not satisfiable"; break;
    case 431: line = "431 Request Header Fields Too Large"; body = "head too large"; break;
    case 501: line = "501 Not Implemented"; body = "not implemented"; break;
    default: line = "400 Bad Request"; body = "bad request"; break;
    }
    head(c, line, keep, "text/plain", (int64_t)strlen(body));
    if (extra) puts_(c, extra);
    puts_(c, "\r\n");
    if (!head_only) puts_(c, body);
}

/* Answers a GET or HEAD of `target`; false when the connection closes after it. */
static void answer_file(conn* c, const uint8_t* target, size_t target_len, bool keep, bool head_only, const uint8_t* since_v, size_t since_n,
                        const uint8_t* range_v, size_t range_n, bool if_range) {
    char full[8192 + 4096 + 16];
    size_t len;
    memcpy(full, root, root_len);
    if (!decode_path(target, target_len, full + root_len, &len)) { simple(c, 400, keep, head_only, NULL); return; }
    bool slash = full[root_len + len - 1] == '/';
    len += root_len;
    if (slash) { memcpy(full + len, "index.html", 10); len += 10; }
    full[len] = 0;
    struct stat st;
    if (stat(full, &st) != 0) { simple(c, (errno == EACCES || errno == EPERM) ? 403 : 404, keep, head_only, NULL); return; }
    if (S_ISDIR(st.st_mode)) {
        if (slash) { simple(c, 404, keep, head_only, NULL); return; }
        const uint8_t* q = memchr(target, '?', target_len);
        size_t written = q ? (size_t)(q - target) : target_len;
        char extra[8300];
        snprintf(extra, sizeof extra, "location: %.*s/\r\n", (int)written, (const char*)target);
        simple(c, 301, keep, head_only, extra);
        return;
    }
    if (!S_ISREG(st.st_mode)) { simple(c, 404, keep, head_only, NULL); return; }
    int fd = open(full, O_RDONLY | O_CLOEXEC);
    if (fd < 0) { simple(c, (errno == EACCES || errno == EPERM) ? 403 : 404, keep, head_only, NULL); return; }
#ifdef __APPLE__
    int64_t mtime = (int64_t)st.st_mtimespec.tv_sec;
#else
    int64_t mtime = (int64_t)st.st_mtim.tv_sec;
#endif
    char modified[64];
    http_date(modified, mtime);
    int64_t since;
    if (since_v && parse_http_date(since_v, since_n, &since) && mtime <= since) {
        close(fd);
        head(c, "304 Not Modified", keep, NULL, -1);
        puts_(c, "last-modified: ");
        puts_(c, modified);
        puts_(c, "\r\n\r\n");
        return;
    }
    int64_t size = (int64_t)st.st_size, start = 0, end = size - 1, first = 0, last = 0;
    int kind = range_v && !if_range ? range_of(range_v, range_n, &first, &last) : 0;
    if (kind != 0) {
        bool whole = (kind == 1 && first < size) || (kind == 2 && last > 0 && size > 0);
        if (!whole) {
            char extra[64];
            snprintf(extra, sizeof extra, "content-range: bytes */%lld\r\n", (long long)size);
            close(fd);
            simple(c, 416, keep, head_only, extra);
            return;
        }
        if (kind == 1) {
            start = first;
            if (last < end) end = last;
        } else if (last < size) {
            start = size - last;
        }
    }
    int64_t length = end - start + 1;
    head(c, kind ? "206 Partial Content" : "200 OK", keep, type_of(full, len), length);
    puts_(c, "last-modified: ");
    puts_(c, modified);
    puts_(c, "\r\naccept-ranges: bytes\r\n");
    if (kind) {
        char h[96];
        int n = snprintf(h, sizeof h, "content-range: bytes %lld-%lld/%lld\r\n", (long long)start, (long long)end, (long long)size);
        put(c, h, (size_t)n);
    }
    puts_(c, "\r\n");
    if (head_only || length == 0) {
        close(fd);
        return;
    }
    c->file = fd;
    c->file_at = start;
    c->file_left = length;
}

/* Reads the next piece of the file being sent behind what waits; false when the file ended short. */
static bool piece(conn* c) {
    size_t want = c->file_left < PIECE ? (size_t)c->file_left : PIECE;
    reserve(&c->out, &c->out_cap, c->out_len + want);
    ssize_t n;
    do n = pread(c->file, c->out + c->out_len, want, (off_t)c->file_at);
    while (n < 0 && errno == EINTR);
    if (n <= 0) return false;
    c->out_len += (size_t)n;
    c->file_at += n;
    c->file_left -= n;
    if (c->file_left == 0) {
        close(c->file);
        c->file = -1;
    }
    return true;
}

/* Answers the whole requests in the buffer until one begins a file: 0 when none was whole, 1 when
   some were answered, -1 when the connection closes after the answers. */
static int serve(conn* c) {
    const uint8_t* p = c->in;
    const uint8_t* end = c->in + c->in_len;
    int answered = 0;
    while (p < end && c->file < 0) {
        size_t avail = (size_t)(end - p);
        const uint8_t* stop = memmem(p, avail < MOST_HEAD ? avail : MOST_HEAD, "\r\n\r\n", 4);
        if (!stop) {
            if (avail >= MOST_HEAD) { simple(c, 431, false, false, NULL); return -1; }
            break;
        }
        const uint8_t* head_end = stop + 4;
        const uint8_t* q = p;
        const uint8_t* method = q;
        while (q < stop && token_char(*q)) q++;
        size_t method_len = (size_t)(q - method);
        if (method_len == 0 || q >= stop || *q != ' ') { simple(c, 400, false, false, NULL); return -1; }
        const uint8_t* target = ++q;
        while (q < stop && *q > ' ' && *q < 0x7f) q++;
        size_t target_len = (size_t)(q - target);
        if (target_len == 0 || q >= stop || *q != ' ') { simple(c, 400, false, false, NULL); return -1; }
        q++;
        int minor;
        if (stop - q >= 10 && memcmp(q, "HTTP/1.", 7) == 0 && (q[7] == '0' || q[7] == '1') && q[8] == '\r' && q[9] == '\n') {
            minor = q[7] - '0';
            q += 10;
        } else if (stop - q == 8 && memcmp(q, "HTTP/1.", 7) == 0 && (q[7] == '0' || q[7] == '1')) {
            minor = q[7] - '0';
            q += 8;
        } else {
            simple(c, 400, false, false, NULL);
            return -1;
        }
        bool keep = minor == 1, host = false, chunked = false, if_range = false;
        long long length = 0;
        bool has_length = false;
        const uint8_t *since_v = NULL, *range_v = NULL;
        size_t since_n = 0, range_n = 0;
        while (q < stop) {
            const uint8_t* name = q;
            while (q < stop && token_char(*q)) q++;
            size_t name_len = (size_t)(q - name);
            if (name_len == 0 || q >= stop || *q != ':') { simple(c, 400, false, false, NULL); return -1; }
            q++;
            while (q < stop && (*q == ' ' || *q == '\t')) q++;
            const uint8_t* value = q;
            const uint8_t* eol = memmem(q, (size_t)(stop + 2 - q), "\r\n", 2);
            if (!eol) eol = stop;
            const uint8_t* value_end = eol;
            while (value_end > value && (value_end[-1] == ' ' || value_end[-1] == '\t')) value_end--;
            for (const uint8_t* v = value; v < value_end; v++)
                if ((*v < ' ' && *v != '\t') || *v == 0x7f) { simple(c, 400, false, false, NULL); return -1; }
            size_t value_len = (size_t)(value_end - value);
            if (name_len == 4 && strncasecmp((const char*)name, "host", 4) == 0) {
                if (host) { simple(c, 400, false, false, NULL); return -1; }
                host = true;
            } else if (name_len == 10 && strncasecmp((const char*)name, "connection", 10) == 0) {
                if (has_token(value, value_len, "close")) keep = false;
                else if (has_token(value, value_len, "keep-alive")) keep = true;
            } else if (name_len == 14 && strncasecmp((const char*)name, "content-length", 14) == 0) {
                long long v = 0;
                if (value_len == 0 || value_len > 9) { simple(c, 400, false, false, NULL); return -1; }
                for (size_t i = 0; i < value_len; i++) {
                    if (value[i] < '0' || value[i] > '9') { simple(c, 400, false, false, NULL); return -1; }
                    v = v * 10 + (value[i] - '0');
                }
                if (has_length && v != length) { simple(c, 400, false, false, NULL); return -1; }
                has_length = true;
                length = v;
            } else if (name_len == 17 && strncasecmp((const char*)name, "transfer-encoding", 17) == 0) {
                chunked = true;
            } else if (name_len == 17 && strncasecmp((const char*)name, "if-modified-since", 17) == 0) {
                if (!since_v) { since_v = value; since_n = value_len; }
            } else if (name_len == 5 && strncasecmp((const char*)name, "range", 5) == 0) {
                if (!range_v) { range_v = value; range_n = value_len; }
            } else if (name_len == 8 && strncasecmp((const char*)name, "if-range", 8) == 0) {
                if_range = true;
            }
            q = eol + 2;
        }
        if (chunked) { simple(c, 501, false, false, NULL); return -1; }
        if (minor == 1 && !host) { simple(c, 400, false, false, NULL); return -1; }
        if (length > MOST_BODY) { simple(c, 400, false, false, NULL); return -1; }
        if (end - head_end < length) break;
        bool get = method_len == 3 && memcmp(method, "GET", 3) == 0;
        bool head_only = method_len == 4 && memcmp(method, "HEAD", 4) == 0;
        if (!get && !head_only) simple(c, 405, keep, head_only, NULL);
        else answer_file(c, target, target_len, keep, head_only, since_v, since_n, range_v, range_n, if_range);
        answered = 1;
        p = head_end + length;
        if (!keep) {
            c->in_len = 0;
            return -1;
        }
    }
    size_t used = (size_t)(p - c->in);
    memmove(c->in, p, c->in_len - used);
    c->in_len -= used;
    return answered;
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
        perror("files: bind");
        exit(1);
    }
    fcntl(fd, F_SETFL, fcntl(fd, F_GETFL) | O_NONBLOCK);
    return fd;
}

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
    if (c->file >= 0) close(c->file);
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

#if defined(SENDFILE) && defined(__linux__)
/* Sends the file being sent straight from it, once the head is out; false when the file ended short
   or the connection failed. */
static bool send_file(int poll, conn* c) {
    while (c->file_left > 0) {
        off_t at = (off_t)c->file_at;
        ssize_t n = sendfile(c->fd, c->file, &at, c->file_left < (1 << 20) ? (size_t)c->file_left : (1 << 20));
        if (n < 0) {
            if (errno == EAGAIN) {
                if (!c->waiting_out) watch(poll, c->fd, c, true, false);
                c->waiting_out = true;
                return true;
            }
            if (errno == EINTR) continue;
            return false;
        }
        if (n == 0) return false;
        c->file_at += n;
        c->file_left -= n;
    }
    close(c->file);
    c->file = -1;
    if (c->waiting_out) watch(poll, c->fd, c, false, false);
    c->waiting_out = false;
    return true;
}
#endif

/* Answers, reads the file being sent and sends, until it must wait; false when the connection is
   done with. */
static bool go_on(int poll, conn* c) {
    for (;;) {
#if defined(SENDFILE) && defined(__linux__)
        if (c->file >= 0) {
            if (!flush(poll, c)) return false;
            if (c->waiting_out) return true;
            if (!send_file(poll, c)) return false;
            if (c->waiting_out) return true;
            continue;
        }
#endif
        if (c->file >= 0 && c->out_len - c->out_at < PIECE && !piece(c)) {
            flush(poll, c);
            return false;
        }
        if (!flush(poll, c)) return false;
        if (c->waiting_out) return true;
        if (c->file >= 0) continue;
        if (c->close_after) return false;
        int r = serve(c);
        if (r < 0) c->close_after = true;
        else if (r == 0) return true;
    }
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
    if (!go_on(poll, c)) drop(c);
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
                    c->file = -1;
                    c->in = malloc(FIRST_BUFFER);
                    c->in_cap = FIRST_BUFFER;
                    watch(poll, fd, c, false, true);
                }
                continue;
            }
            conn* c = data;
            if (out) {
                if (!go_on(poll, c)) drop(c);
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
    if (argc < 3 || argc > 4) {
        fprintf(stderr, "usage: files PORT ROOT [THREADS]\n");
        return 2;
    }
    port_number = atoi(argv[1]);
    size_t n = strlen(argv[2]);
    while (n > 1 && argv[2][n - 1] == '/') n--;
    if (n > 4096) {
        fprintf(stderr, "files: the root is too long\n");
        return 2;
    }
    char* r = malloc(n + 1);
    memcpy(r, argv[2], n);
    r[n] = 0;
    struct stat st;
    if (stat(r, &st) != 0 || !S_ISDIR(st.st_mode)) {
        fprintf(stderr, "files: %s is not a folder\n", r);
        return 2;
    }
    root = r;
    root_len = n;
    int threads = argc == 4 ? atoi(argv[3]) : cores();
    signal(SIGPIPE, SIG_IGN);
    struct rlimit rl;
    if (getrlimit(RLIMIT_NOFILE, &rl) == 0) {
        rl.rlim_cur = rl.rlim_max;
#ifdef __APPLE__
        if (rl.rlim_cur > 10240) rl.rlim_cur = 10240;
#endif
        setrlimit(RLIMIT_NOFILE, &rl);
    }
    pthread_t* ts = calloc((size_t)threads, sizeof(pthread_t));
    for (int i = 1; i < threads; i++) pthread_create(&ts[i], NULL, loop, NULL);
    loop(NULL);
    return 0;
}
