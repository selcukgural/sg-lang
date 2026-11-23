#!/usr/bin/env python3
"""The requests on the edges of HTTP/1.1 sent to SG's `HttpConnection` and to hyper, the answers
set side by side (choice 1794).

Each case is the bytes of `interpreter/tests/cases/ok_http_conformance.sg`, and a few more, sent
on a connection of its own to `echo.sg` and to `hyper/`, which answer every request alike; what
comes back until the server closes, or goes quiet for a second, is written as the statuses of the
answers, the bodies of the 200s, and whether the server closed. The table goes to standard output
in Markdown, the cases that differ marked; why each differs is written in
benchmarks/library.md, "Against hyper".

    python3 trials/server/http/conformance/compare.py [--only NAME]

It builds both servers (cargo, and the `sg` of interpreter/target/release) and runs them on the
loopback. It is run by hand, not by the tests: hyper is not a dependency of the language.
"""

import argparse
import os
import re
import socket
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", "..", ".."))
OUT = os.path.join(ROOT, "trials", "target", "conformance")
SG = os.path.join(ROOT, "interpreter", "target", "release", "sg")
QUIET = 1.0


def cases():
    many = b"GET / HTTP/1.1\r\nhost: a\r\n" + b"".join(b"x-%d: %d\r\n" % (i, i) for i in range(101)) + b"connection: close\r\n\r\n"
    long_target = b"GET /" + b"a" * 8200 + b" HTTP/1.1\r\nhost: a\r\n\r\n"
    wide = b"GET / HTTP/1.1\r\nhost: a\r\nx-wide: " + b"a" * 8200 + b"\r\n\r\n"
    return [
        ("a request", b"GET /ping HTTP/1.1\r\nHost: a\r\nConnection: close\r\n\r\n"),
        ("HEAD", b"HEAD /ping HTTP/1.1\r\nHost: a\r\nConnection: close\r\n\r\n"),
        ("kept open, then closed", b"GET /1 HTTP/1.1\r\nHost: a\r\n\r\nGET /2 HTTP/1.1\r\nHost: a\r\nConnection: close\r\n\r\n"),
        ("HTTP/1.0 closes", b"GET /old HTTP/1.0\r\n\r\nGET /never HTTP/1.0\r\n\r\n"),
        ("HTTP/1.0 kept alive", b"GET /a HTTP/1.0\r\nConnection: keep-alive\r\n\r\nGET /b HTTP/1.0\r\n\r\n"),
        ("lines ending in LF alone", b"GET /lf HTTP/1.1\nHost: a\nConnection: close\n\n"),
        ("empty lines first", b"\r\n\r\nGET /late HTTP/1.1\r\nHost: a\r\nConnection: close\r\n\r\n"),
        ("a body read", b"POST /body HTTP/1.1\r\nHost: a\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello"),
        ("a body not read, then the next", b"POST /skip HTTP/1.1\r\nHost: a\r\nContent-Length: 5\r\n\r\nhelloGET /next HTTP/1.1\r\nHost: a\r\nConnection: close\r\n\r\n"),
        ("chunks", b"POST /body HTTP/1.1\r\nHost: a\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n3\r\nabc\r\n2;name=value\r\nde\r\n0\r\nx-trailer: 1\r\n\r\n"),
        ("chunks not read, then the next", b"POST /skip HTTP/1.1\r\nHost: a\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n0\r\n\r\nGET /next HTTP/1.1\r\nHost: a\r\nConnection: close\r\n\r\n"),
        ("two equal lengths", b"POST /body HTTP/1.1\r\nHost: a\r\nContent-Length: 2\r\nContent-Length: 2\r\nConnection: close\r\n\r\nhi"),
        ("Expect: 100-continue", b"POST /body HTTP/1.1\r\nHost: a\r\nExpect: 100-continue\r\nContent-Length: 2\r\nConnection: close\r\n\r\nhi"),
        ("pipelined", b"GET /1 HTTP/1.1\r\nHost: a\r\n\r\nGET /2 HTTP/1.1\r\nHost: a\r\n\r\nGET /3 HTTP/1.1\r\nHost: a\r\nConnection: close\r\n\r\n"),
        ("Upgrade, let be", b"GET /up HTTP/1.1\r\nHost: a\r\nUpgrade: h2c\r\nConnection: Upgrade, HTTP2-Settings, close\r\nHTTP2-Settings: AAMAAABkAAQAoAAAAAIAAAAA\r\n\r\n"),
        ("a Host with a port", b"GET / HTTP/1.1\r\nHost: a.example:8080\r\nConnection: close\r\n\r\n"),
        ("a Host of an IPv6 address", b"GET / HTTP/1.1\r\nHost: [::1]:8080\r\nConnection: close\r\n\r\n"),
        ("an empty Host", b"GET / HTTP/1.1\r\nHost:\r\nConnection: close\r\n\r\n"),
        ("a whole URL as the target", b"GET http://b.example:81/p?q HTTP/1.1\r\nHost: a\r\nConnection: close\r\n\r\n"),
        ("OPTIONS *", b"OPTIONS * HTTP/1.1\r\nHost: a\r\nConnection: close\r\n\r\n"),
        ("a Host with a user", b"GET / HTTP/1.1\r\nHost: u@a\r\n\r\n"),
        ("a Host with a path", b"GET / HTTP/1.1\r\nHost: a/b\r\n\r\n"),
        ("a Host with a port not of digits", b"GET / HTTP/1.1\r\nHost: a:8x\r\n\r\n"),
        ("a Host of a port alone", b"GET / HTTP/1.1\r\nHost: :80\r\n\r\n"),
        ("a target that is not a path", b"GET p HTTP/1.1\r\nHost: a\r\n\r\n"),
        ("* for GET", b"GET * HTTP/1.1\r\nHost: a\r\n\r\n"),
        ("a URL of another scheme", b"GET ftp://a/p HTTP/1.1\r\nHost: a\r\n\r\n"),
        ("a URL without a host", b"GET http:///p HTTP/1.1\r\nHost: a\r\n\r\n"),
        ("a URL with a user", b"GET http://u@a/p HTTP/1.1\r\nHost: a\r\n\r\n"),
        ("no Host", b"GET / HTTP/1.1\r\n\r\n"),
        ("two Hosts", b"GET / HTTP/1.1\r\nHost: a\r\nHost: b\r\n\r\n"),
        ("Content-Length and Transfer-Encoding", b"POST /body HTTP/1.1\r\nHost: a\r\nContent-Length: 3\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n"),
        ("two lengths that differ", b"POST /body HTTP/1.1\r\nHost: a\r\nContent-Length: 2\r\nContent-Length: 3\r\n\r\nabc"),
        ("a length with a sign", b"POST /body HTTP/1.1\r\nHost: a\r\nContent-Length: +2\r\n\r\nhi"),
        ("a length as a list", b"POST /body HTTP/1.1\r\nHost: a\r\nContent-Length: 2, 2\r\n\r\nhi"),
        ("a coding other than chunked", b"POST /body HTTP/1.1\r\nHost: a\r\nTransfer-Encoding: gzip, chunked\r\n\r\n"),
        ("chunked not last", b"POST /body HTTP/1.1\r\nHost: a\r\nTransfer-Encoding: chunked, chunked\r\n\r\n"),
        ("chunked in HTTP/1.0", b"POST /body HTTP/1.0\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n"),
        ("a folded line", b"GET / HTTP/1.1\r\nHost: a\r\nX-Long: one\r\n two\r\n\r\n"),
        ("a space before the colon", b"GET / HTTP/1.1\r\nHost : a\r\n\r\n"),
        ("a bare CR", b"GET / HTTP/1.1\r\nHost: a\rb\r\n\r\n"),
        ("a zero byte in a value", b"GET / HTTP/1.1\r\nHost: a\x00b\r\n\r\n"),
        ("two spaces in the request line", b"GET  / HTTP/1.1\r\nHost: a\r\n\r\n"),
        ("a version not 1", b"GET / HTTP/2.0\r\nHost: a\r\n\r\n"),
        ("a version that is no version", b"GET / HTTQ/1.1\r\nHost: a\r\n\r\n"),
        ("CONNECT", b"CONNECT a:443 HTTP/1.1\r\nHost: a:443\r\n\r\n"),
        ("a target too long", long_target),
        ("a header too long", wide),
        ("too many headers", many),
        ("a body too large", b"POST /body HTTP/1.1\r\nHost: a\r\nContent-Length: 2000000\r\n\r\n"),
        ("chunks too large", b"POST /body HTTP/1.1\r\nHost: a\r\nTransfer-Encoding: chunked\r\n\r\n100001\r\n"),
        ("a chunk size that is not hex", b"POST /body HTTP/1.1\r\nHost: a\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\n"),
        ("a chunk longer than it said", b"POST /body HTTP/1.1\r\nHost: a\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nabc\r\n0\r\n\r\n"),
        # More than the golden test has: ways requests are smuggled.
        ("Transfer-Encoding with a space before the colon", b"POST /body HTTP/1.1\r\nHost: a\r\nTransfer-Encoding : chunked\r\nContent-Length: 3\r\n\r\nabc"),
        ("Transfer-Encoding of another case", b"POST /body HTTP/1.1\r\nHost: a\r\nTransfer-Encoding: CHUNKED\r\nConnection: close\r\n\r\n2\r\nhi\r\n0\r\n\r\n"),
        ("a chunk size with a sign", b"POST /body HTTP/1.1\r\nHost: a\r\nTransfer-Encoding: chunked\r\n\r\n+2\r\nhi\r\n0\r\n\r\n"),
        ("a chunk size with zeros in front", b"POST /body HTTP/1.1\r\nHost: a\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n00000000000000000002\r\nhi\r\n0\r\n\r\n"),
        ("a chunk size too large to hold", b"POST /body HTTP/1.1\r\nHost: a\r\nTransfer-Encoding: chunked\r\n\r\nffffffffffffffffff1\r\nhi\r\n0\r\n\r\n"),
        ("a length too large to hold", b"POST /body HTTP/1.1\r\nHost: a\r\nContent-Length: 99999999999999999999\r\n\r\nhi"),
        ("a method that is not a token", b"G(T / HTTP/1.1\r\nHost: a\r\n\r\n"),
        ("a header name with a slash", b"GET / HTTP/1.1\r\nHost: a\r\nx/y: 1\r\n\r\n"),
        ("a request line ending in LF, a header in CR LF", b"GET /mixed HTTP/1.1\nHost: a\r\nConnection: close\r\n\r\n"),
    ]


def build():
    os.makedirs(OUT, exist_ok=True)
    sg = os.path.join(OUT, "echo-sg")
    subprocess.run([SG, "build", os.path.join(HERE, "echo.sg"), "-o", sg], check=True, timeout=600)
    hyper = os.path.join(HERE, "hyper")
    subprocess.run(["cargo", "build", "--release", "--quiet"], cwd=hyper, check=True, timeout=900)
    return {"SG": sg, "hyper": os.path.join(hyper, "target", "release", "echo")}


def free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def start(binary):
    port = free_port()
    proc = subprocess.Popen([binary, str(port)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        try:
            socket.create_connection(("127.0.0.1", port), timeout=1).close()
            return proc, port
        except OSError:
            time.sleep(0.05)
    proc.kill()
    sys.exit(f"compare: {binary} did not start")


def exchange(port, request):
    """What comes back to `request`, and whether the server closed: `closed`, `reset` or `open`."""
    got = b""
    how = "open"
    with socket.create_connection(("127.0.0.1", port), timeout=5) as s:
        try:
            s.sendall(request)
        except OSError:
            how = "reset"
        s.settimeout(QUIET)
        while how == "open":
            try:
                part = s.recv(65536)
            except socket.timeout:
                break
            except ConnectionResetError:
                how = "reset"
                break
            if not part:
                how = "closed"
                break
            got += part
    return got, how


def answers(got):
    """The answers in `got`: the status of each, with the body of a 200."""
    out = []
    while got:
        end = got.find(b"\r\n\r\n")
        if end < 0:
            out.append("(cut)")
            break
        head, got = got[:end].decode("latin-1"), got[end + 4:]
        status = head.split("\r\n")[0].split(" ")
        code = status[1] if len(status) > 1 else "?"
        if code == "100":
            out.append("100")
            continue
        headers = {}
        for line in head.split("\r\n")[1:]:
            name, _, value = line.partition(":")
            headers[name.strip().lower()] = value.strip()
        if "content-length" in headers:
            n = int(headers["content-length"])
            body, got = got[:n], got[n:]
        elif headers.get("transfer-encoding", "").lower() == "chunked":
            body = b""
            while True:
                line_end = got.find(b"\r\n")
                size = int(got[:line_end].split(b";")[0], 16)
                got = got[line_end + 2:]
                if size == 0:
                    got = got[got.find(b"\r\n") + 2:]
                    break
                body, got = body + got[:size], got[size + 2:]
        else:
            body, got = got, b""
        out.append(f"{code} `{body.decode('latin-1')}`" if code == "200" else code)
    return out


def summary(got, how):
    return " + ".join(answers(got) or ["nothing"]) + f", {how}"


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--only", help="the one case to run")
    args = parser.parse_args()
    servers = build()
    running = {name: start(binary) for name, binary in servers.items()}
    try:
        rows = []
        for name, request in cases():
            if args.only and name != args.only:
                continue
            seen = {server: summary(*exchange(port, request)) for server, (_, port) in running.items()}
            rows.append((name, seen["SG"], seen["hyper"]))
    finally:
        for proc, _ in running.values():
            proc.kill()
    print("| Request | SG | hyper | |")
    print("|---|---|---|---|")
    differ = 0
    for name, sg, hyper in rows:
        mark = "" if sg == hyper else "differs"
        differ += sg != hyper
        print(f"| {name} | {sg} | {hyper} | {mark} |")
    print(f"\n{len(rows)} cases, {differ} differ", file=sys.stderr)


if __name__ == "__main__":
    main()
