---
title: HTTP
description: HttpConnection, the server's side of HTTP/1.1, a request pulled and answered at a time, read where it lies, with the limits a server on the internet needs on from the start.
sidebar_position: 18
---

# HTTP

`HttpConnection` is the server's side of HTTP/1.1 (RFC 9112) on a `TcpStream`, written in SG in the module
`std`. There are no callbacks: the program pulls a request, looks at it, and answers it, as Zig's
`std.http.Server` and Go's `net/textproto` do it, so the connection is an ordinary loop in an ordinary task,
and its errors, its time limits and its cancellation are those of every other task.

A request is read where it lies. The method, the target and each header are places in the connection's block,
found once and never copied, so a request costs nothing to read beyond the bytes that came. `conn.request()`
lends that view, an `HttpRequest`, until the connection reads again; the compiler holds a program to it.

The module does the parts of HTTP/1.1 that a server gets wrong when each writes them again: it keeps
connections open (1.0 and 1.1, `Connection: close`), answers pipelined requests in order and sends their
answers together, reads a body of `Content-Length` bytes or in chunks, sends `100 Continue` once the body is
asked for, lets go of a body nobody read before the next request, and writes `date` and `content-length`
itself. What is malformed, ambiguous or too large it answers itself, and closes the connection.

```sg
async Result<void, IoError> serve(TcpStream stream) {
    var conn = HttpConnection(stream)
    while try await conn.receiveHeadAsync() {
        var req = conn.request()
        if req.method() != "GET" && req.method() != "HEAD" {
            try conn.addHeader("allow", "GET, HEAD")
            try await conn.respondAsync(405, "text/plain", "method not allowed".asBytes())
        } else if req.target() == "/hello" {
            var agent = string.fromUtf8(req.header("user-agent") ?? "someone".asBytes()) ?? "someone"
            var text = $"hello, {agent}\n".toString()
            try await conn.respondAsync(200, "text/plain", text.asBytes())
        } else {
            try await conn.respondAsync(404, "text/plain", "not found\n".asBytes())
        }
    }
    return Ok()
}

// A client in the same program, so that the example ends: it asks twice on one connection.
async Result<void, IoError> main() {
    var listener = try TcpListener.bind("127.0.0.1:0")
    var address = $"127.0.0.1:{listener.port()}".toString()
    var client = try await TcpStream.connectAsync(read address)
    var accepted = try await listener.acceptAsync()
    var server = serve(accepted)
    try await client.sendAsync("GET /hello HTTP/1.1\r\nHost: here\r\nUser-Agent: curl\r\n\r\nDELETE /hello HTTP/1.1\r\nHost: here\r\nConnection: close\r\n\r\n".asBytes())
    var got = List<byte>()
    var buffer = byte[4096](0)
    while true {
        var n = try await client.receiveAsync(ref buffer[..])
        if n == 0 {
            break
        }
        got.addAll(buffer[0..n])
    }
    try await server
    var text = string.fromUtf8(read got) ?? ""
    for line in text.split("\r\n") {
        if !line.startsWith("date: ") {
            print(line)
        }
    }
    return Ok()
}
```

```text
HTTP/1.1 200 OK
content-type: text/plain
content-length: 12

hello, curl
HTTP/1.1 405 Method Not Allowed
content-type: text/plain
allow: GET, HEAD
connection: close
content-length: 18

method not allowed
```

(The `date` line each answer has is left out of what is printed, since it changes.) A server takes
connections with a `TaskGroup` and gives each to a function like `serve`, as the [server of
Tasks](./tasks.md) does; `trials/server/http/sg/http.sg` is the HTTP server of the measurements, written so.

## `HttpConnection`

| Signature | What it does |
|---|---|
| `HttpConnection(TcpStream stream, HttpOptions options = HttpOptions {})` | A connection, with the limits of `options` |
| `async Result<bool, IoError> receiveHeadAsync(ref this, read CancellationToken token = None)` | Reads the head of the next request: `true` when there is one; `false` when the client has gone, a time limit ran out, or the request was refused |
| `HttpRequest request(read this)` | The request being served, where it lies; a panic when none is |
| `async Result<bool, IoError> receiveBodyAsync(ref this, read CancellationToken token = None)` | Reads its body whole into the block, sending `100 Continue` first if the client waits for it; `false` as above, or when the body is over `maxBodyBytes` (answered 413) |
| `Result<void, IoError> addHeader(ref this, read string name, read string value)` | A header of the next answer; `InvalidInput` for a name that is not a token, a value with a line break or another control byte, or `content-length`, `transfer-encoding`, `connection` and `date`, which the module writes |
| `async Result<void, IoError> respondAsync(ref this, int status, read string contentType, read byte[] body, read CancellationToken token = None)` | Answers with a body of known length; `contentType` empty writes none; `InvalidInput` for a type with a line break |
| `async Result<void, IoError> beginAsync(ref this, int status, read string contentType, long? length = null, read CancellationToken token = None)` | Starts an answer sent in parts: of `length` bytes with `content-length`, or, without it, in chunks, or for HTTP/1.0 until the connection closes |
| `async Result<void, IoError> writeAsync(ref this, read byte[] part, read CancellationToken token = None)` | A part of it; an empty part sends nothing |
| `async Result<void, IoError> endAsync(ref this, read CancellationToken token = None)` | Ends it; `InvalidInput` for an answer begun with a length and not all sent, after the connection is closed |
| `async Result<void, IoError> flushAsync(ref this, read CancellationToken token = None)` | Sends the answers that wait for the next request (see below) |
| `void closeAfterAnswer(ref this)` | Closes the connection once the request being served is answered: an answer not begun yet says `connection: close`, and the next `receiveHeadAsync` gives `false`; called after the answer, the connection ends without waiting for another request |
| `int? refused(read this)` | The status the module refused a request with (400, 413, 414, 431, 501, 505), or `null` |

Each has a version that waits on the thread, without `Async` and without a token: `receiveHead`,
`receiveBody`, `respond`, `begin`, `write`, `end`, `flush`. They have no time limits, since a blocking call of
the network has none to give them.

An answer begun with a length (choice 1865) says `content-length`, has no chunks, and keeps the connection,
HTTP/1.0's too: what a file is sent with, its size known before its bytes are read. A part past the length is a
panic, a mistake of the program, which said how much it would send. An end before the length has come closes
the connection once what was sent is out, so that the client sees the answer cut and does not take the next
bytes for a new answer, as nginx does when a file shrinks while it is sent; `endAsync` then gives
`InvalidInput`. For HEAD the parts are counted and not sent, and an end before the length is no error.
A part of 16 KB or more of an answer not in chunks goes out as it lies, after what waits before it, in one send
of pieces (`writev`), without being copied into the connection's block. `trials/server/files/sg/files.sg` is a
file server written so.

A request that is not answered before the next is received is a panic, as is an answer to no request, a
second answer, or a body asked for after the answer: each is a mistake in the program, and the client would
wait for an answer that never comes. A status is 200 to 999 (an answer of 1xx is the module's), and a 204 or
304 has no body.

For a HEAD request the module sends the head of the answer, with the length of the body it was given, and
not the body. An answer to a request that closes the connection says `connection: close`, and the connection
is closed once it is sent: `receiveHeadAsync` then gives `false`. A server that wants to close a
connection itself, to be stopped or to send a client elsewhere, calls `closeAfterAnswer`.

## `HttpRequest`

What `conn.request()` gives: a view of the request in the connection's block, `HttpRequest<from conn>`.

| Signature | What it does |
|---|---|
| `read string method(read this)` | `GET`, `POST`, as written: a token |
| `read string target(read this)` | `/path?query`, as written, not decoded; a whole URL (`http://host/path`) when the client wrote one, as a proxy is asked; `*` for `OPTIONS *` |
| `read byte[] host(read this)` | The host the request is for, with its port when it has one: from the target when it is a whole URL, from `Host` otherwise (RFC 9112 puts the URL's first); empty when there is none, as HTTP/1.0 may send |
| `int minorVersion(read this)` | 0 for HTTP/1.0, 1 for HTTP/1.1 |
| `read byte[]? header(read this, read string name)` | The value of the first header of this name, in any case, without the spaces around it; `null` when there is none |
| `nint headerCount(read this)` | How many headers there are |
| `read string headerName(read this, nint i)`, `read byte[] headerValue(read this, nint i)` | Header `i`, in the order they came |
| `bool hasBody(read this)` | Whether a body comes, of a length or in chunks |
| `read byte[] body(read this)` | The body, once `receiveBodyAsync` has read it; empty for a request without one; a panic before |

A header's value is bytes, not text: HTTP lets it hold bytes that are not UTF-8, and `string.fromUtf8(value)`
makes text of it where the program wants text. The method and the target are text, since the module has
checked that they hold nothing but visible ASCII.

A view lasts until the connection reads again: to read the body, the view is let go, and a new one is taken
after.

```sg nocheck
if conn.request().hasBody() {
    if !try await conn.receiveBodyAsync() {
        return Ok()
    }
}
var req = conn.request()
var size = req.body().length()
```

## What the module refuses

Each is answered, with `connection: close`, and the connection closed; `receiveHeadAsync` (or
`receiveBodyAsync`) gives `false` and `refused()` the status. They are the ways a request is smuggled past a
proxy in front of a server, and the ways a client makes a server hold more than it should:

| Request | Answer |
|---|---|
| A request line or a header that is not HTTP: a space before the colon, a line folded onto the one before, a bare carriage return, a control byte in a value, two spaces in the request line | 400 |
| HTTP/1.1 without `Host`, two `Host` headers, or a `Host` that is not `host[:port]`: a user (`u@a`), a path, a port that is not digits, a port without a host | 400 |
| A target that is not a path (`/...`), a whole `http` or `https` URL with a host, or `*` for `OPTIONS` | 400 |
| `Content-Length` and `Transfer-Encoding` both; two lengths that differ; a length that is not digits | 400 |
| `Transfer-Encoding` that is not `chunked` alone, or in HTTP/1.0 | 400, or 501 for a coding it does not know |
| A chunk size that is not hex, a chunk longer than it said | 400 |
| A request line that has not ended within `maxHeadBytes` (8 KB) | 414 |
| A head over `maxHeadBytes`, or more than `maxHeaders` headers (100) | 431 |
| A body over `maxBodyBytes` (1 MB), when it is asked for | 413 |
| `CONNECT` | 501 |
| A version that is not 1.x | 505 |

A line may end in a line feed alone, and empty lines before a request are let go of, as RFC 9112 asks.
An IPv6 address in brackets is looked at for the bytes it may hold (hex digits, colons, dots), not for its
whole form.
`Upgrade` is let be: the request is answered in HTTP/1.1. A client refused for a head or a body too large may
still be sending (414, 431, 413); were the connection closed on bytes it has not read, the system would send a reset, which
can take the answer away from the client, so the module reads what comes and lets it go until the client stops
for 100 ms (2 s and 1 MB at most), and then closes, as nginx does.

## `HttpOptions`

| Field | Default | What it is |
|---|---|---|
| `int maxHeadBytes` | 8192 | The most bytes of the request line and the headers: 414 past it while the request line goes on, 431 after |
| `int maxHeaders` | 100 | The most headers |
| `long maxBodyBytes` | 1048576 | The most bytes of a body |
| `int headTimeout` | 10000 | Milliseconds a head may take from its first byte; 0 for none |
| `int idleTimeout` | 60000 | Milliseconds a connection may wait for the next request, for more of a body, or for its answers to be taken in; 0 for none |

When a time limit of the module runs out, the connection is closed with no answer: the next call gives
`false`, or an answer that could not be sent is let go of. When the token a call is given is cancelled, the
call gives `IoError.Cancelled`, so the program can tell the server being stopped from a slow client.

A call made without a token keeps the connection's own source of cancellation and sets it again before each
wait, which costs one atomic write; a call given a token links a source to it for that call
([`CancellationSource.linked`](./tasks.md#cancellationsource-and-cancellationtoken)). A server that has one
token for all its connections gives it to `receiveHeadAsync`, where most of the waiting is.

## Answers sent together

When a client sends requests one after another without waiting (pipelining), the next one is often whole in
the block already when an answer is written. The answer then waits, and goes out with the next ones in one
send, up to 64 KB; it goes as soon as the next request is not whole yet. A program that stops reading requests
after an answer calls `closeAfterAnswer`, and the next `receiveHeadAsync` sends what waits before it gives
`false`, or calls `flushAsync`; a connection let go of without either drops those answers, and the client,
which has had no answer to them, asks again.

## How fast

The HTTP server of the measurements is written on `HttpConnection`, with its limits on. With 1,000
connections kept open it answers within 2.2% of the server SG had before, written by hand on `TcpStream`
with the same limits, and faster with 50 ([the library's measurements](https://github.com/selcukgural/sg-lang/blob/main/benchmarks/library.md#http)).

A server of files written on it answers files of 1 KB at 0.66 times the rate of the same server in C, files of
1 MB and of 1 GB at C's rate, and holds what C holds; the difference on small files is the work of each answer,
not the disk ([the server of files](https://github.com/selcukgural/sg-lang/blob/main/benchmarks/server.md#files)).

## Not here yet

A client; HTTP/2 and TLS; a body larger than the block, read as a stream; `sendfile`; compression; cookies,
forms and multipart; a router; WebSocket.
