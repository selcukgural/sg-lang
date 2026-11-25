# The server lane

Three servers, each written in SG, Rust, Zig and C, and one load that drives them all the same way
(choices 1698-1707). The numbers, and what they say, are in
[Servers against Rust, Zig and C](../../benchmarks/server.md).

| Folder | What it is |
|---|---|
| [load/stand.c](load/stand.c) | The stand, in C (choices 1750-1763): starts a server and does to it what it must stand, resets, halves closed, a byte at a time, clients that do not read, requests that are not the protocol, more clients than descriptors, SIGTERM; after each round the server must answer, be back to its descriptors and hold no more memory idle. What each scenario does is written at its top; the results are in [What the servers stand](../../benchmarks/robustness.md) |
| [load](load) | The load, in C (`resp`, `http` and `files`, whose answers it checks byte for byte against what each file holds): closed loop (answers a second) or open loop (`--rate`, the time of each answer counted from when it was due, so a server that stalls is charged for what waited behind it), idle connections from several source addresses, a check of every answer byte for byte, a line for each interval of a long run (`--report`), connections opened again (`--reconnect`) |
| [resp](resp) | A store that speaks a part of RESP2, Redis's protocol: PING, GET, SET, DEL, over 64 maps each behind a lock |
| [http](http) | An HTTP/1.1 server that answers GET /ping with `pong`, connections kept open; SG's is on `std/http` (`sg`), with the one written by hand on `TcpStream` before it beside it (`sg-by-hand`) |
| [files](files) | A file server: GET and HEAD of the files under a folder, ranges, `If-Modified-Since`, folders, with nginx measured beside the four (choices 1861-1871); its own `measure.py` and `check.py` |
| [http/conformance](http/conformance) | Requests on the edges of HTTP/1.1 sent to `std/http` and to hyper, the answers set side by side (choice 1794); run by hand, the table is in [library.md](../../benchmarks/library.md#against-hyper) |
| [pieces](pieces) | A head of 100 bytes and a body of 4 KB or 64 KB sent over the loopback, copied into one buffer, with SG's `sendPieces` and with C's `writev` (choice 1740): `pieces copy\|pieces SIZE COUNT` prints the nanoseconds of one message |
| [measure.py](measure.py) | Builds everything and runs every case on Linux in the image of [Containerfile](Containerfile) |

What every version does the same way is written at the top of `resp/c/resp.c` and `http/c/http.c`.
Rust is `tokio` (and hyper for HTTP), Zig is `std.Io` on `Io.Threaded` (and `std.http.Server`), C
is the program a C programmer writes for speed: a thread on each core, each with a listener of its
own (`SO_REUSEPORT`) and a loop of its own on epoll.

```sh
podman build -t sg-server -f trials/server/Containerfile trials/server   # once
python3 trials/server/measure.py              # every case, on Linux in the image
python3 trials/server/measure.py --soak       # and the long runs, 30 minutes each
python3 trials/server/measure.py --check      # each server held to what it must answer, natively
python3 trials/server/measure.py --ramp       # the breaking point, each server and protocol
python3 trials/server/measure.py --robust     # the stand, heavy level, each server (ROBUST_SCALE=0.05 to try)
python3 trials/server/measure.py --languages sg,sg-limit,rust,zig,c --only resp-trickle-10k --runs 3
                                              # slow clients, and SG's store with --idle-limit 5000 beside the others
```

`sg-limit` is SG's store started with `--idle-limit 5000`, the time a request may take; it is measured only
when named, since the other servers have no such limit (choice 1724). The load's `--trickle MS` makes its idle
connections slow clients, each sending a byte of an unending SET every MS milliseconds; one the server closes is
counted as cut.

The SG servers take `--idle-limit MS` (a connection that goes that long without a whole request, or with
its answers not taken in, is closed) and `--grace MS` (the first SIGTERM or Ctrl-C stops them taking
connections, gives those open that long, and they end with 0); the others have neither.

`interpreter/tests/cli.rs` (`the_servers_of_the_server_lane`) builds the SG servers and the load and
runs the check on every test run, on Linux and macOS; `the_servers_stand` runs the stand's quick level
against them, built for release and under the sanitizers (10,000 connections on Linux, a tenth on macOS).
