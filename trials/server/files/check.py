#!/usr/bin/env python3
"""The file servers held to what they must answer (choice 1870), each against a model of
`c/files.c`'s rules written here in Python, byte for byte where the rules say, over raw sockets so
that every request goes out as it is written.

    python3 trials/server/files/check.py NAME=COMMAND ... [--nginx PATH] [--stand]

COMMAND is the server, run with the port and the root after it, and its words that start with `--`
after those (`sg=trials/target/files-sg`, `sg-interp=sg files.sg`). Each
server is started on a root this script makes, with files of every kind a server meets (empty, large,
with names in UTF-8, with spaces, without a type, a FIFO, one not readable, links in and out), and
asked for:

- paths that try to leave the root (`..`, `%2e%2e`, `%c0%ae`, `%00`, `\\`, `//`, `%2F`), paths that
  are not UTF-8, queries, folders with and without `/`, every method;
- ranges at every edge (`0-0`, `-0`, `5-3`, past the end, a number longer than any file and than a
  `long`, more than one, a space, another unit, with `If-Range`), and a thousand ranges of a large
  file at random, each held to the bytes of the file;
- `If-Modified-Since` in the three forms of RFC 9110, with the day of the week wrong, in another
  case, a second of 60, years past what a date holds;
- HTTP/1.0 with and without `keep-alive`, requests sent together, HEAD of each;
- a file that is cut, that grows and that is removed while it is sent to a client that reads slowly:
  the answer is never longer than it said, a cut one ends the connection, and the server goes on.

With `--stand`, each server then stands clients of a file of 1 GB that never read what they asked
for, that read a kilobyte every 50 ms, and that reset the connection halfway; meanwhile it must answer
another client within a second, and after each it must be back to its descriptors. The rounds are
gone through twice, and the memory after the second must be no more than 32 MB over that after the
first: what a server frees may stay in it (the allocator of macOS keeps it), but a leak grows. What
it held is printed.

Every answer is held to the model in its status, its body and the headers the rules name
(`content-type`, `content-length`, `content-range`, `location`, `last-modified`, `accept-ranges`,
`allow`); `date` and `connection` are let be. With `--nginx`, nginx is asked the same and where it
answers otherwise is printed, without failing: it is measured beside the four, not held to their
rules. Exits 1 when any server answers anything else than the model.
"""

import os
import random
import shutil
import socket
import stat
import subprocess
import sys
import tempfile
import time
from email.utils import formatdate

PIECE = 65536
FAR = 1 << 62
TYPES = {
    "html": "text/html; charset=utf-8", "htm": "text/html; charset=utf-8", "css": "text/css; charset=utf-8",
    "js": "text/javascript; charset=utf-8", "mjs": "text/javascript; charset=utf-8", "json": "application/json",
    "txt": "text/plain; charset=utf-8", "xml": "application/xml", "svg": "image/svg+xml", "png": "image/png",
    "jpg": "image/jpeg", "jpeg": "image/jpeg", "gif": "image/gif", "webp": "image/webp", "ico": "image/x-icon",
    "wasm": "application/wasm", "pdf": "application/pdf", "zip": "application/zip", "gz": "application/gzip",
    "mp4": "video/mp4", "webm": "video/webm", "mp3": "audio/mpeg", "woff2": "font/woff2", "woff": "font/woff",
}
COMPARED = ("content-type", "content-length", "content-range", "location", "last-modified", "accept-ranges", "allow")
ROOT_IS_ROOT = os.geteuid() == 0


# ---- the model


def http_date(seconds):
    return formatdate(seconds, usegmt=True)


DAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
LONG_DAYS = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"]
MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]


def days_from_civil(y, m, d):
    y -= m <= 2
    era = y // 400
    yoe = y - era * 400
    doy = (153 * (m + (-3 if m > 2 else 9)) + 2) // 5 + d - 1
    doe = yoe * 365 + yoe // 4 - yoe // 100 + doy
    return era * 146097 + doe - 719468


def parse_http_date(text):
    """RFC 9110's three forms, as SG's `DateTime.parseHttpDate` reads them; None when it is none."""
    import re
    m = re.fullmatch(r"(Mon|Tue|Wed|Thu|Fri|Sat|Sun), (\d\d) (" + "|".join(MONTHS) + r") (\d{4}) (\d\d):(\d\d):(\d\d) GMT", text)
    if m:
        named, day, month, year = DAYS.index(m[1]), int(m[2]), MONTHS.index(m[3]) + 1, int(m[4])
        hms = m.group(5, 6, 7)
    else:
        m = re.fullmatch(r"(" + "|".join(LONG_DAYS) + r"), (\d\d)-(" + "|".join(MONTHS) + r")-(\d\d) (\d\d):(\d\d):(\d\d) GMT", text)
        if m:
            named, day, month, yy = LONG_DAYS.index(m[1]), int(m[2]), MONTHS.index(m[3]) + 1, int(m[4])
            year = yy + (2000 if yy < 70 else 1900)
            hms = m.group(5, 6, 7)
        else:
            m = re.fullmatch(r"(Mon|Tue|Wed|Thu|Fri|Sat|Sun) (" + "|".join(MONTHS) + r") ( \d|\d\d) (\d\d):(\d\d):(\d\d) (\d{4})", text)
            if not m:
                return None
            named, month, day, year = DAYS.index(m[1]), MONTHS.index(m[2]) + 1, int(m[3]), int(m[7])
            hms = m.group(4, 5, 6)
    if not all(c.isascii() for c in text):
        return None
    hour, minute, second = (int(x) for x in hms)
    leap = year % 4 == 0 and (year % 100 != 0 or year % 400 == 0)
    length = [31, 29 if leap else 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][month - 1]
    if not (1 <= day <= length) or hour > 23 or minute > 59 or second > 59 or not (1000 <= year <= 3000):
        return None
    dn = days_from_civil(year, month, day)
    s = dn * 86400 + hour * 3600 + minute * 60 + second
    if not (-9223372036 <= s <= 9223372036) or (dn + 3) % 7 != named:
        return None
    return s


def decode_path(target):
    raw = target.split(b"?", 1)[0]
    if not raw.startswith(b"/"):
        return None
    out = bytearray()
    i = 0
    while i < len(raw):
        c = raw[i]
        if c == ord("%"):
            if i + 2 >= len(raw):
                return None
            try:
                c = int(raw[i + 1:i + 3].decode("ascii"), 16)
            except ValueError:
                return None
            if not all(chr(x) in "0123456789abcdefABCDEF" for x in raw[i + 1:i + 3]):
                return None
            if c == ord("/"):
                return None
            i += 2
        if c == 0 or c == ord("\\"):
            return None
        out.append(c)
        i += 1
    segments = bytes(out[1:]).split(b"/")
    for k, segment in enumerate(segments):
        if (segment == b"" and k != len(segments) - 1) or segment in (b".", b".."):
            return None
    try:
        return bytes(out).decode("utf-8")
    except UnicodeDecodeError:
        return None


def type_of(path):
    slash, dot = path.rfind("/"), path.rfind(".")
    if dot <= slash:
        return "application/octet-stream"
    return TYPES.get(path[dot + 1:].lower(), "application/octet-stream")


def digits_far(v, at):
    start, x = at, 0
    while at < len(v) and v[at] in b"0123456789":
        x = FAR if x > FAR // 10 else min(x * 10 + v[at] - 48, FAR)
        at += 1
    return (x if at > start else None), at


def range_of(v):
    if not v.startswith(b"bytes="):
        return None
    rest = v[6:]
    first, at = digits_far(rest, 0)
    if at >= len(rest) or rest[at] != ord("-"):
        return None
    last, at = digits_far(rest, at + 1)
    if at != len(rest):
        return None
    if first is None:
        return ("last", last) if last is not None else None
    if last is None:
        return ("from", first, FAR)
    return ("from", first, last) if first <= last else None


def text_answer(status, body, extra=None):
    headers = {"content-type": "text/plain", "content-length": str(len(body))}
    headers.update(extra or {})
    return status, headers, body


def model(root, method, target, headers):
    """(status, headers the rules name, body) for a request."""
    status, out, body = model_get(root, method, target, headers)
    return status, out, b"" if method == b"HEAD" else body


def model_get(root, method, target, headers):
    if method not in (b"GET", b"HEAD"):
        return text_answer(405, b"method not allowed", {"allow": "GET, HEAD"})
    path = decode_path(target) if all(0x21 <= c <= 0x7e for c in target) else None
    if path is None:
        return text_answer(400, b"bad request")
    full = root + path + ("index.html" if path.endswith("/") else "")
    try:
        info = os.stat(full)
    except PermissionError:
        return text_answer(403, b"forbidden")
    except OSError:
        return text_answer(404, b"not found")
    if stat.S_ISDIR(info.st_mode):
        if path.endswith("/"):
            return text_answer(404, b"not found")
        written = target.split(b"?", 1)[0].decode("ascii")
        return text_answer(301, b"moved permanently", {"location": written + "/"})
    if not stat.S_ISREG(info.st_mode):
        return text_answer(404, b"not found")
    # The file is opened before its headers are looked at (1873): one not readable is 403 whatever
    # they ask, as nginx answers it.
    if not os.access(full, os.R_OK):
        return text_answer(403, b"forbidden")
    mtime = int(info.st_mtime // 1)
    modified = http_date(mtime)
    since = headers.get(b"if-modified-since")
    if since is not None:
        try:
            moment = parse_http_date(since.decode("utf-8"))
        except UnicodeDecodeError:
            moment = None
        if moment is not None and mtime <= moment:
            return 304, {"last-modified": modified}, b""
    size = info.st_size
    start, end, partial = 0, size - 1, False
    wanted = range_of(headers[b"range"]) if b"range" in headers and b"if-range" not in headers else None
    if wanted is not None:
        if wanted[0] == "from" and wanted[1] < size:
            start, end = wanted[1], min(end, wanted[2])
        elif wanted[0] == "last" and wanted[1] > 0 and size > 0:
            start = size - min(wanted[1], size)
        else:
            return text_answer(416, b"range not satisfiable", {"content-range": f"bytes */{size}"})
        partial = True
    length = end - start + 1
    out = {"content-type": type_of(full), "content-length": str(length), "last-modified": modified, "accept-ranges": "bytes"}
    if partial:
        out["content-range"] = f"bytes {start}-{end}/{size}"
    with open(full, "rb") as f:
        f.seek(start)
        body = f.read(length)
    return (206 if partial else 200), out, body


# ---- talking to a server


def read_answer(sock, buffer, head):
    """One answer off the socket: (status, headers, body, cut), `buffer` what came after it."""
    while b"\r\n\r\n" not in buffer:
        more = sock.recv(65536)
        if not more:
            return None, buffer
        buffer += more
    top, buffer = buffer.split(b"\r\n\r\n", 1)
    lines = top.split(b"\r\n")
    status = int(lines[0].split(b" ")[1])
    headers = {}
    for line in lines[1:]:
        name, _, value = line.partition(b":")
        headers.setdefault(name.strip().lower().decode(), value.strip().decode("latin-1"))
    length = 0 if head or status in (204, 304) else int(headers.get("content-length", "0"))
    if "transfer-encoding" in headers:
        raise RuntimeError("a chunked answer, where every answer has a length")
    while len(buffer) < length:
        more = sock.recv(1 << 20)
        if not more:
            return (status, headers, bytes(buffer), True), b""
        buffer += more
    return (status, headers, bytes(buffer[:length]), False), buffer[length:]


def request(port, method, target, extra=b"", version=b"HTTP/1.1"):
    host = b"Host: here\r\n" if version == b"HTTP/1.1" else b""
    return method + b" " + target + b" " + version + b"\r\n" + host + extra + b"\r\n"


def ask(port, raw, count=1, heads=None):
    sock = socket.create_connection(("127.0.0.1", port), timeout=20)
    try:
        sock.sendall(raw)
        answers, buffer = [], b""
        for k in range(count):
            got, buffer = read_answer(sock, buffer, heads[k] if heads else False)
            answers.append(got)
            if got is None:
                break
        return answers
    finally:
        sock.close()


def compare(name, label, got, want, failures):
    if got is None:
        failures.append(f"{name}: {label}: no answer (connection closed)")
        return
    status, headers, body, cut = got
    w_status, w_headers, w_body = want
    problems = []
    if cut:
        problems.append("the body was cut")
    if status != w_status:
        problems.append(f"status {status}, not {w_status}")
    for h in COMPARED:
        # Zig's `std.http.Server` writes `content-length: 0` on a 304 as on every answer it makes.
        if h == "content-length" and status == 304 and headers.get(h) == "0":
            continue
        if headers.get(h) != w_headers.get(h):
            problems.append(f"{h}: {headers.get(h)!r}, not {w_headers.get(h)!r}")
    if body != w_body:
        problems.append(f"body of {len(body)} bytes {body[:40]!r}, not {len(w_body)} {w_body[:40]!r}")
    if problems:
        failures.append(f"{name}: {label}: " + "; ".join(problems))


# ---- the root


def make_root(top):
    root = os.path.join(top, "root")
    os.makedirs(os.path.join(root, "sub"))
    os.makedirs(os.path.join(root, "dir"))
    os.makedirs(os.path.join(root, "deep/er/still"))

    def put(rel, data):
        with open(os.path.join(root, rel), "wb") as f:
            f.write(data)

    put("a.txt", b"hello world\n")
    put("sub/index.html", b"<h1>hi</h1>")
    put("deep/er/still/x.json", b'{"x":1}')
    put("empty.txt", b"")
    put("a b.txt", b"space")
    put("ç.txt", "çay".encode())
    put("UPPER.HTML", b"<p>")
    put("noext", b"plain")
    put(".hidden", b"hidden")
    put("x.tar.gz", b"\x1f\x8b")
    put("weird%41.txt", b"percent")
    put("plus+sign.txt", b"plus")
    for ext in TYPES:
        put(f"t.{ext}", ext.encode())
    rng = random.Random(1870)
    put("big.bin", rng.randbytes(3 * 1024 * 1024 + 17))
    put("piece.bin", rng.randbytes(PIECE))
    put("piece1.bin", rng.randbytes(PIECE + 1))
    os.mkfifo(os.path.join(root, "fifo"))
    put("secret.txt", b"secret")
    os.chmod(os.path.join(root, "secret.txt"), 0)
    os.makedirs(os.path.join(root, "locked"))
    put("locked/in.txt", b"in")
    os.chmod(os.path.join(root, "locked"), 0)
    os.symlink("a.txt", os.path.join(root, "link-in.txt"))
    outside = os.path.join(top, "outside.txt")
    with open(outside, "wb") as f:
        f.write(b"outside")
    os.symlink(outside, os.path.join(root, "link-out.txt"))
    os.symlink("sub", os.path.join(root, "link-dir"))
    os.symlink("nowhere", os.path.join(root, "dangling"))
    os.utime(os.path.join(root, "a.txt"), (784111777, 784111777))
    return root


# ---- the cases


def cases(root):
    """(label, raw request, method, target, headers) for each single request."""
    out = []

    def add(method, target, extra=(), version=b"HTTP/1.1"):
        # What the HTTP of each language refuses itself, before the server sees it, is held to the
        # status only, or to the connection closed without an answer.
        edge = not target.startswith(b"/") or not all(0x21 <= c <= 0x7e for c in target) or not method.isupper()
        headers = {}
        for k, v in extra:
            headers.setdefault(k.lower(), v.strip(b" \t"))
        raw_extra = b"".join(k + b": " + v + b"\r\n" for k, v in extra)
        label = f"{method.decode()} {target!r} {dict(extra)}" + ("" if version == b"HTTP/1.1" else f" {version.decode()}")
        out.append((label, request(0, method, target, raw_extra, version), method, target, headers, edge))

    targets = [
        b"/a.txt", b"/a.txt?x=1", b"/a.txt?", b"/sub/", b"/sub", b"/sub?q=1", b"/dir/", b"/dir", b"/nope", b"/fifo", b"/secret.txt",
        b"/locked/in.txt", b"/locked/", b"/a%20b.txt", b"/%C3%A7.txt", b"/%c3%a7.txt", "/ç.txt".encode(), b"/UPPER.HTML", b"/noext",
        b"/.hidden", b"/x.tar.gz", b"/weird%2541.txt", b"/weird%41.txt", b"/plus+sign.txt", b"/plus%2Bsign.txt", b"/empty.txt",
        b"/link-in.txt", b"/link-out.txt", b"/link-dir/", b"/link-dir", b"/dangling", b"/deep/er/still/x.json", b"/deep/er/",
        b"/", b"//", b"//a.txt", b"/sub//", b"/a.txt/", b"/../a.txt", b"/sub/../a.txt", b"/./a.txt", b"/%2e%2e/a.txt", b"/%2E%2E/a.txt",
        b"/.%2e/a.txt", b"/%2e/a.txt", b"/sub/%2e%2e/a.txt", b"/a%2Fb", b"/sub%2findex.html", b"/a%00.txt", b"/a%00", b"/a%5c.txt",
        b"/a\\b", b"/%c0%ae%c0%ae/a.txt", b"/%ff", b"/%e2%82", b"/%ed%a0%80", b"/%f4%90%80%80", b"/%", b"/%4", b"/%zz", b"/%4g",
        b"/a.txt%", b"a.txt", b"*", b"http://here/a.txt", b"/" + b"a" * 4000, b"/..", b"/.", b"/...", b"/sub/.",
    ] + [f"/t.{ext}".encode() for ext in TYPES] + [f"/t.{ext.upper()}".encode() for ext in ("html", "PNG", "Js")]
    for target in targets:
        add(b"GET", target)
        add(b"HEAD", target)
    for method in (b"POST", b"PUT", b"DELETE", b"OPTIONS", b"PATCH", b"get", b"TRACE"):
        add(method, b"/a.txt")
    ranges = [
        b"bytes=0-4", b"bytes=0-0", b"bytes=-3", b"bytes=6-", b"bytes=11-11", b"bytes=11-", b"bytes=12-", b"bytes=100-", b"bytes=5-3",
        b"bytes=0-1,3-4", b"bytes=0-99999999999999999999999", b"bytes=99999999999999999999999-", b"bytes=-99999999999999999999999",
        b"bytes=-0", b"bytes=-12", b"bytes=-13", b"bytes=0-11", b"bytes=0-12", b"bytes= 0-1", b"bytes=0 -1", b"bytes=0-1 ", b"Bytes=0-1",
        b"items=0-1", b"bytes=", b"bytes=-", b"bytes=--1", b"bytes=0--1", b"bytes=a-b", b"bytes=00000-00004", b"bytes=0-4,",
        b"bytes=9223372036854775807-", b"bytes=4611686018427387904-", b"bytes=4611686018427387903-4611686018427387905",
    ]
    for value in ranges:
        add(b"GET", b"/a.txt", [(b"Range", value)])
        add(b"HEAD", b"/a.txt", [(b"Range", value)])
    for value in (b"bytes=0-0", b"bytes=-1", b"bytes=0-"):
        add(b"GET", b"/empty.txt", [(b"Range", value)])
    for value in (b"bytes=65535-65536", b"bytes=65536-", b"bytes=-65537", b"bytes=0-65535", b"bytes=1-65536", b"bytes=3145728-"):
        add(b"GET", b"/big.bin", [(b"Range", value)])
        add(b"GET", b"/piece1.bin", [(b"Range", value)])
    add(b"GET", b"/a.txt", [(b"Range", b"bytes=0-4"), (b"If-Range", b"\"x\"")])
    add(b"GET", b"/a.txt", [(b"Range", b"bytes=0-4"), (b"Range", b"bytes=5-6")])
    add(b"GET", b"/sub", [(b"Range", b"bytes=0-4")])
    add(b"GET", b"/secret.txt", [(b"Range", b"bytes=100-")])
    dates = [
        b"Sun, 06 Nov 1994 08:49:37 GMT", b"Sun, 06 Nov 1994 08:49:36 GMT", b"Sun, 06 Nov 1994 08:49:38 GMT", b"Sunday, 06-Nov-94 08:49:37 GMT",
        b"Sun Nov  6 08:49:37 1994", b"Sun Nov 06 08:49:37 1994", b"Mon, 06 Nov 1994 08:49:37 GMT", b"sun, 06 Nov 1994 08:49:37 GMT",
        b"Sun, 06 nov 1994 08:49:37 GMT", b"Sun, 06 Nov 1994 08:49:37 gmt", b"Sun, 06 Nov 1994 08:49:37 UTC", b"Sun, 6 Nov 1994 08:49:37 GMT",
        b"Sun, 06 Nov 1994 08:49:37 GMT ", b"Sun, 06 Nov 1994 08:49:60 GMT", b"Sun, 06 Nov 1994 24:00:00 GMT", b"Fri, 31 Dec 9999 23:59:59 GMT",
        b"Fri, 11 Apr 2262 23:47:16 GMT", b"Sat, 12 Apr 2262 00:00:00 GMT", b"Thu, 01 Jan 1970 00:00:00 GMT", b"Wednesday, 01-Jan-69 00:00:00 GMT",
        b"Thursday, 01-Jan-70 00:00:00 GMT", b"Tue, 31 Dec 2069 23:59:59 GMT", b"Sat, 29 Feb 2025 00:00:00 GMT", b"Thu, 29 Feb 2024 00:00:00 GMT",
        b"", b"garbage", b"1994-11-06T08:49:37Z", "Sün, 06 Nov 1994 08:49:37 GMT".encode(), b"\xff\xfe",
    ]
    for value in dates:
        add(b"GET", b"/a.txt", [(b"If-Modified-Since", value)])
    add(b"GET", b"/a.txt", [(b"If-Modified-Since", b"Sun, 06 Nov 1994 08:49:37 GMT"), (b"Range", b"bytes=0-1")])
    add(b"GET", b"/big.bin", [(b"If-Modified-Since", b"Thu, 01 Jan 1970 00:00:00 GMT")])
    add(b"GET", b"/a.txt", version=b"HTTP/1.0")
    add(b"GET", b"/a.txt", [(b"Connection", b"keep-alive")], version=b"HTTP/1.0")
    add(b"GET", b"/a.txt", [(b"Connection", b"close")])
    return out


def run_cases(name, port, root, all_cases, failures, nginx_notes=None):
    for label, raw, method, target, headers, edge in all_cases:
        if ROOT_IS_ROOT and (b"secret" in target or b"locked" in target):
            continue
        want = model(root, method, target, headers)
        got = ask(port, raw, heads=[method == b"HEAD"])[0]
        if edge and (got is None or got[0] == want[0] or got[0] == 400):
            continue
        if nginx_notes is not None:
            before = len(nginx_notes)
            compare(name, label, got, want, nginx_notes)
            del nginx_notes[before + 1:]
        else:
            compare(name, label, got, want, failures)


def run_ranges(name, port, root, failures, rng):
    """A thousand ranges of `big.bin`, at random, and the file whole."""
    with open(os.path.join(root, "big.bin"), "rb") as f:
        data = f.read()
    size = len(data)
    sock = socket.create_connection(("127.0.0.1", port), timeout=20)
    buffer = b""
    try:
        for k in range(1000):
            a = rng.randrange(size)
            b = min(size - 1, a + rng.choice([0, 1, 4095, 4096, PIECE - 1, PIECE, PIECE + 1, rng.randrange(1 << 20)]))
            sock.sendall(request(port, b"GET", b"/big.bin", f"Range: bytes={a}-{b}\r\n".encode()))
            got, buffer = read_answer(sock, buffer, False)
            if got is None or got[0] != 206 or got[2] != data[a:b + 1]:
                failures.append(f"{name}: random range {a}-{b}: {None if got is None else (got[0], len(got[2]))}")
                return
        sock.sendall(request(port, b"GET", b"/big.bin"))
        got, buffer = read_answer(sock, buffer, False)
        if got is None or got[2] != data:
            failures.append(f"{name}: big.bin whole: not its bytes")
    finally:
        sock.close()


def run_pipelined(name, port, root, failures):
    """Requests sent together, answered in order on one connection; then HTTP/1.0 closing."""
    batch = [(b"GET", b"/a.txt", {}), (b"HEAD", b"/big.bin", {}), (b"GET", b"/piece1.bin", {}), (b"GET", b"/nope", {}),
             (b"GET", b"/a.txt", {b"range": b"bytes=1-2"}), (b"POST", b"/a.txt", {}), (b"GET", b"/sub", {}), (b"GET", b"/big.bin", {})]
    raw = b"".join(request(port, m, t, b"".join(k + b": " + v + b"\r\n" for k, v in h.items())) for m, t, h in batch)
    answers = ask(port, raw, count=len(batch), heads=[m == b"HEAD" for m, _, _ in batch])
    for (m, t, h), got in zip(batch, answers):
        compare(name, f"pipelined {m.decode()} {t!r}", got, model(root, m, t, h), failures)
    if len(answers) < len(batch) or answers[-1] is None:
        failures.append(f"{name}: pipelined: {len(answers)} answers of {len(batch)}")
    # HTTP/1.0 without keep-alive: one answer, then the connection closes.
    sock = socket.create_connection(("127.0.0.1", port), timeout=20)
    try:
        sock.sendall(request(port, b"GET", b"/a.txt", version=b"HTTP/1.0"))
        got, rest = read_answer(sock, b"", False)
        compare(name, "HTTP/1.0 closes", got, model(root, b"GET", b"/a.txt", {}), failures)
        more = rest + sock.recv(100)
        if more:
            failures.append(f"{name}: HTTP/1.0 without keep-alive: {len(more)} bytes after the answer")
    except (socket.timeout, ConnectionResetError):
        failures.append(f"{name}: HTTP/1.0 without keep-alive: the connection stayed open")
    finally:
        sock.close()


def run_changing(name, port, root, failures):
    """A file cut, grown and removed while a slow client reads it."""
    size = 32 * 1024 * 1024
    path = os.path.join(root, "changing.bin")
    for change in ("cut", "grow", "remove"):
        data = bytes(range(256)) * (size // 256)
        with open(path, "wb") as f:
            f.write(data)
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 65536)
        sock.settimeout(20)
        sock.connect(("127.0.0.1", port))
        try:
            sock.sendall(request(port, b"GET", b"/changing.bin"))
            buffer = b""
            while b"\r\n\r\n" not in buffer:
                buffer += sock.recv(65536)
            top, body = buffer.split(b"\r\n\r\n", 1)
            length = int([l for l in top.split(b"\r\n") if l.lower().startswith(b"content-length:")][0].split(b":")[1])
            time.sleep(0.3)
            if change == "cut":
                os.truncate(path, 1024 * 1024)
            elif change == "grow":
                with open(path, "ab") as f:
                    f.write(b"x" * (8 * 1024 * 1024))
            else:
                os.remove(path)
            closed = False
            while len(body) <= length:
                more = sock.recv(1 << 20)
                if not more:
                    closed = True
                    break
                body += more
                if len(body) == length:
                    break
            if len(body) > length:
                failures.append(f"{name}: a file that {change}s: {len(body)} bytes where the answer said {length}")
            elif change == "cut" and not closed:
                failures.append(f"{name}: a file cut while sent: the whole length came, {len(body)} bytes")
            elif change == "cut" and body != data[:len(body)]:
                failures.append(f"{name}: a file cut while sent: other bytes than the file's")
            elif change != "cut" and (closed or body != data[:length]):
                failures.append(f"{name}: a file that {change}s while sent: {len(body)} bytes, {'closed' if closed else 'open'}")
        except socket.timeout:
            failures.append(f"{name}: a file that {change}s while sent: no end within 20 s")
        finally:
            sock.close()
        got = ask(port, request(port, b"GET", b"/a.txt"))[0]
        compare(name, f"after a file that {change}s", got, model(root, b"GET", b"/a.txt", {}), failures)
    if os.path.exists(path):
        os.remove(path)


# ---- the stand


def held(pid):
    """(memory in KB, open descriptors) of a process."""
    if sys.platform.startswith("linux"):
        rss = int([l for l in open(f"/proc/{pid}/status") if l.startswith("VmRSS")][0].split()[1])
        return rss, len(os.listdir(f"/proc/{pid}/fd"))
    rss = int(subprocess.run(["ps", "-o", "rss=", "-p", str(pid)], capture_output=True, text=True).stdout.strip() or 0)
    fds = len(subprocess.run(["lsof", "-n", "-P", "-p", str(pid)], capture_output=True, text=True).stdout.splitlines()) - 1
    return rss, fds


def answers_quickly(port, root):
    t = time.time()
    try:
        got = ask(port, request(port, b"GET", b"/a.txt"))[0]
    except OSError:
        return None
    ok = got is not None and got[0] == 200 and got[2] == b"hello world\n"
    return time.time() - t if ok else None


def clients(port, n, path=b"/huge.bin"):
    out = []
    for _ in range(n):
        # The small buffer is set before the connection, so that the window it agrees is that of the
        # buffer: made smaller after it, Linux drops what the server had already sent into the larger
        # window, and the server's resending backs off past any time limit.
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 16384)
        s.settimeout(20)
        s.connect(("127.0.0.1", port))
        s.sendall(request(port, b"GET", path))
        out.append(s)
    return out


def reset(socks):
    import struct
    for s in socks:
        try:
            s.setsockopt(socket.SOL_SOCKET, socket.SO_LINGER, struct.pack("ii", 1, 0))
            s.close()
        except OSError:
            pass


def stand(name, proc, port, root, failures, scale):
    """The rounds of the stand against one server; a line for each."""
    with open(os.path.join(root, "huge.bin"), "wb") as f:
        f.truncate(1 << 30)
    try:
        time.sleep(0.3)
        base = held(proc.pid)
        rounds = []

        def after(label, during):
            time.sleep(2)
            quick = answers_quickly(port, root)
            now = held(proc.pid)
            line = (f"{name}: {label}: memory {base[0] / 1024:.1f} MB, {during[0] / 1024:.1f} while, {now[0] / 1024:.1f} after; "
                    f"descriptors {base[1]}, {during[1]} while, {now[1]} after")
            print(line, flush=True)
            if proc.poll() is not None:
                failures.append(f"{name}: {label}: the server ended with {proc.returncode}")
            elif quick is None:
                failures.append(f"{name}: {label}: no answer afterwards")
            if now[1] > base[1] + 2:
                failures.append(f"{name}: {label}: {now[1]} descriptors after, {base[1]} before")
            rounds.append(now[0])

        for _ in range(int(os.environ.get("STAND_ROUNDS", "2"))):
            round_of(proc, port, root, failures, scale, name, after)
        first, second = rounds[2], rounds[-1]
        if second > first + 32 * 1024:
            failures.append(f"{name}: the memory grew from {first / 1024:.1f} MB to {second / 1024:.1f} MB over the rounds again")
    finally:
        os.remove(os.path.join(root, "huge.bin"))


def round_of(proc, port, root, failures, scale, name, after):
    """One round of each kind of client, each followed by `after`."""
    # clients of 1 GB that never read: the server holds what it sent, not the file
    socks = clients(port, int(500 * scale))
    time.sleep(3)
    during = held(proc.pid)
    quick = answers_quickly(port, root)
    if quick is None or quick > 1:
        failures.append(f"{name}: with clients that do not read, another waited {quick} s")
    for s in socks:
        s.close()
    after(f"{len(socks)} clients of 1 GB that do not read", during)
    # clients that read a kilobyte every 50 ms
    socks = clients(port, int(50 * scale))
    for s in socks:
        s.setblocking(False)
    until = time.time() + 5
    while time.time() < until:
        for s in socks:
            try:
                s.recv(1024)
            except BlockingIOError:
                pass
        time.sleep(0.05)
    during = held(proc.pid)
    quick = answers_quickly(port, root)
    if quick is None or quick > 1:
        failures.append(f"{name}: with slow clients, another waited {quick} s")
    for s in socks:
        s.close()
    after(f"{len(socks)} clients that read 1 KB every 50 ms", during)
    # clients that reset the connection after up to a megabyte
    rng = random.Random(1870)
    peak = (0, 0)
    for _ in range(int(20 * scale)):
        socks = clients(port, 100)
        for s in socks:
            want = rng.randrange(1 << 20)
            got = 0
            while got < want:
                more = s.recv(min(65536, want - got))
                if not more:
                    break
                got += len(more)
        peak = max(peak, held(proc.pid))
        reset(socks)
    after(f"{int(20 * scale) * 100} clients that reset after up to 1 MB", peak)


# ---- the servers


def free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def wait_for(port, proc, what):
    for _ in range(200):
        if proc.poll() is not None:
            raise RuntimeError(f"{what} ended with {proc.returncode}: {proc.stderr.read().decode()[-2000:]}")
        try:
            socket.create_connection(("127.0.0.1", port), timeout=1).close()
            return
        except OSError:
            time.sleep(0.05)
    raise RuntimeError(f"{what} did not listen on {port}")


def start(command, port, root):
    # the words of COMMAND that start with `--` go after the port and the root (`zig=files-zig --uring`)
    words = command.split()
    flags = [w for w in words if w.startswith("--")]
    words = [w for w in words if not w.startswith("--")]
    proc = subprocess.Popen(words + [str(port), root] + flags, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    wait_for(port, proc, command)
    return proc


def start_nginx(nginx, port, root, top):
    conf = os.path.join(top, "nginx.conf")
    with open(conf, "w") as f:
        f.write(f"""daemon off; worker_processes 1; error_log {top}/nginx-error.log; pid {top}/nginx.pid;
events {{ worker_connections 1024; }}
http {{ access_log off; types {{ {' '.join(f'{t} {e};' for e, t in TYPES.items())} }} default_type application/octet-stream;
  server {{ listen 127.0.0.1:{port}; root {root}; index index.html; autoindex off; etag off; }} }}
""")
    proc = subprocess.Popen([nginx, "-c", conf, "-p", top], stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    wait_for(port, proc, "nginx")
    return proc


def main():
    args = sys.argv[1:]
    nginx = None
    if "--nginx" in args:
        k = args.index("--nginx")
        nginx = args[k + 1]
        del args[k:k + 2]
    stand_too = "--stand" in args
    servers = [a.split("=", 1) for a in args if "=" in a]
    if not servers:
        print(__doc__)
        return 2
    top = tempfile.mkdtemp(prefix="files-check-")
    failures = []
    try:
        root = make_root(top)
        all_cases = cases(root)
        for name, command in servers:
            port = free_port()
            proc = start(command, port, root)
            try:
                before = len(failures)
                run_cases(name, port, root, all_cases, failures)
                run_ranges(name, port, root, failures, random.Random(1870))
                run_pipelined(name, port, root, failures)
                run_changing(name, port, root, failures)
                if stand_too:
                    stand(name, proc, port, root, failures, float(os.environ.get("STAND_SCALE", "1")))
                if proc.poll() is not None:
                    failures.append(f"{name}: the server ended with {proc.returncode}: {proc.stderr.read().decode()[-2000:]}")
                print(f"{name}: {len(all_cases)} requests, 1,001 ranges, pipelined, changing files: {len(failures) - before} wrong")
            finally:
                proc.kill()
                proc.wait()
        if nginx:
            port = free_port()
            proc = start_nginx(nginx, port, root, top)
            notes = []
            try:
                run_cases("nginx", port, root, all_cases, failures, notes)
            finally:
                proc.kill()
                proc.wait()
            print(f"nginx answers {len(notes)} of {len(all_cases)} requests otherwise than the four (not a failure):")
            for note in notes:
                print("  " + note)
    finally:
        for dirpath, dirnames, _ in os.walk(top):
            for d in dirnames:
                try:
                    os.chmod(os.path.join(dirpath, d), 0o755)
                except OSError:
                    pass
        try:
            os.chmod(os.path.join(top, "root", "secret.txt"), 0o644)
        except OSError:
            pass
        shutil.rmtree(top, ignore_errors=True)
    for failure in failures:
        print(failure)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
