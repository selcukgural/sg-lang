# Servers against Rust, Zig and C

SG is meant for servers: stores like Redis, databases, HTTP servers. Since 22 November 2026 the line it holds
there is this: on the network SG is never to be worse than the best rival, in speed or in memory, under load or
idle. This page measures two servers written in SG, in Rust, in Zig and in C, under one load, and shows where
SG stands, including where it does not hold the line yet (choices 1698-1707). The code is in
[`trials/server`](../trials/server).

These are the numbers of one run of each case, taken on 22 November 2026; the median of three runs will replace them. A cell of answers gives the answers a second, the p99 and the memory the server holds at the end; a cell of idle connections gives the memory each connection adds, the memory held, and the processor's time in 10 seconds of silence.

<!-- Linux 7.1.3-200.fc44.aarch64 aarch64, 8 cores; rustc 1.99.1 (58cf8f9c3 2026-10-13); zig 0.17.1; cc (Debian 14.2.0-19) 14.2.0 -->

### The store (RESP)

| Case | SG | Rust | Zig | C |
|---|---|---|---|---|
| 1,000 idle connections | 4.96 KB, 6.3 MB, 0.02 s | 5.00 KB, 7.2 MB, 0.01 s | 272.11 KB, 266.6 MB, 0.01 s | 4.18 KB, 5.4 MB, 0.01 s |
| 10,000 idle connections | 4.97 KB, 50.0 MB, 0.09 s | 5.00 KB, 51.2 MB, 0.07 s | 272.35 KB, 2660.5 MB, 0.08 s | 4.17 KB, 42.1 MB, 0.06 s |
| 100,000 idle connections | 4.96 KB, 486.0 MB, 0.45 s | 5.00 KB, 490.6 MB, 0.36 s | failed: 88952 connections failed | 4.17 KB, 408.8 MB, 0.22 s |
| 1 connection, 1 in flight on each | 27k/s, 43 µs, 3.5 MB | 33k/s, 39 µs, 4.2 MB | 34k/s, 37 µs, 5.2 MB | 33k/s, 38 µs, 3.1 MB |
| 1 connection, 16 in flight on each | 497k/s, 51 µs, 3.5 MB | 580k/s, 36 µs, 4.3 MB | 536k/s, 46 µs, 5.2 MB | 558k/s, 40 µs, 3.1 MB |
| 50 connections, 1 in flight on each | 578k/s, 168 µs, 3.7 MB | 607k/s, 165 µs, 4.5 MB | 900k/s, 124 µs, 16.2 MB | 1,472k/s, 76 µs, 3.3 MB |
| 50 connections, 16 in flight on each | 2,171k/s, 723 µs, 3.9 MB | 2,084k/s, 680 µs, 4.6 MB | 1,884k/s, 801 µs, 16.3 MB | 2,040k/s, 672 µs, 3.4 MB |
| 1,000 connections, 1 in flight on each | 1,208k/s, 1.26 ms, 8.3 MB | 1,325k/s, 2.36 ms, 9.2 MB | 749k/s, 2.70 ms, 268.8 MB | 1,486k/s, 1.53 ms, 7.2 MB |
| 1,000 connections, 16 in flight on each | 2,120k/s, 14.97 ms, 9.6 MB | 2,062k/s, 11.30 ms, 9.7 MB | 1,827k/s, 10.58 ms, 270.9 MB | 2,014k/s, 10.42 ms, 8.8 MB |
| 100 connections, 10% of the slowest's answers a second | p99 1.49 ms, p99.9 1.67 ms, 4.0 MB | p99 1.50 ms, p99.9 2.35 ms, 4.7 MB | p99 1.41 ms, p99.9 1.98 ms, 29.5 MB | p99 1.36 ms, p99.9 1.67 ms, 3.5 MB |
| 100 connections, 40% of the slowest's answers a second | p99 1.13 ms, p99.9 1.28 ms, 4.1 MB | p99 1.18 ms, p99.9 1.30 ms, 4.7 MB | p99 1.20 ms, p99.9 1.37 ms, 29.5 MB | p99 1.20 ms, p99.9 1.34 ms, 3.5 MB |
| 100 connections, 70% of the slowest's answers a second | p99 187 µs, p99.9 1.40 ms, 4.1 MB | p99 183 µs, p99.9 1.10 ms, 4.7 MB | p99 752 µs, p99.9 1.97 ms, 29.5 MB | p99 610 µs, p99.9 1.21 ms, 3.5 MB |
| 10,000 connections, one request a second each | p99 2.32 ms, p99.9 10.22 ms, 53.2 MB | p99 3883.93 ms, p99.9 6912.21 ms, 50.5 MB | p99 1707.08 ms, p99.9 1828.72 ms, 2662.7 MB | p99 3.19 ms, p99.9 10.68 ms, 44.5 MB |
| 10,000 idle connections and 100 at 70% | p99 174 µs, p99.9 1.05 ms, 52.4 MB | p99 584 µs, p99.9 1.17 ms, 53.5 MB | p99 1.01 ms, p99.9 2.74 ms, 2689.4 MB | p99 778 µs, p99.9 1.23 ms, 44.2 MB |
| 50 connections, values of 4 KB | 564k/s, 176 µs, 43.0 MB | 599k/s, 172 µs, 43.4 MB | 787k/s, 145 µs, 54.8 MB | 1,008k/s, 111 µs, 42.2 MB |
| 50 connections, values of 64 KB | 295k/s, 294 µs, 630.7 MB | 308k/s, 287 µs, 634.8 MB | 318k/s, 370 µs, 643.8 MB | 353k/s, 330 µs, 630.1 MB |

### HTTP

| Case | SG | Rust | Zig | C |
|---|---|---|---|---|
| 50 connections kept open | 581k/s, 166 µs, 1.7 MB | 536k/s, 184 µs, 3.2 MB | 901k/s, 125 µs, 13.9 MB | 1,442k/s, 76 µs, 1.5 MB |
| 1,000 connections kept open | 1,332k/s, 1.28 ms, 6.2 MB | 867k/s, 3.80 ms, 12.3 MB | 721k/s, 2.87 ms, 266.4 MB | 1,475k/s, 1.56 ms, 5.5 MB |
| 50 at a time, a connection for each request | 149k/s, 502 µs, 2.1 MB | 172k/s, 633 µs, 3.8 MB | 198k/s, 398 µs, 27.3 MB | 202k/s, 333 µs, 2.0 MB |
| 100 connections, 10% of the slowest's answers a second | p99 1.51 ms, p99.9 2.26 ms, 1.9 MB | p99 1.52 ms, p99.9 2.24 ms, 3.9 MB | p99 1.63 ms, p99.9 2.51 ms, 27.2 MB | p99 1.45 ms, p99.9 3.84 ms, 1.7 MB |
| 100 connections, 40% of the slowest's answers a second | p99 1.16 ms, p99.9 1.29 ms, 1.9 MB | p99 1.16 ms, p99.9 1.30 ms, 3.9 MB | p99 1.23 ms, p99.9 1.38 ms, 27.2 MB | p99 1.22 ms, p99.9 1.35 ms, 1.7 MB |
| 100 connections, 70% of the slowest's answers a second | p99 292 µs, p99.9 1.13 ms, 1.9 MB | p99 189 µs, p99.9 1.05 ms, 3.8 MB | p99 756 µs, p99.9 1.47 ms, 27.2 MB | p99 969 µs, p99.9 1.26 ms, 1.7 MB |

## What the numbers say

**Memory.** SG holds the least memory of the three languages in every case but one, close to C's; with 10,000 connections that each ask once a second `tokio` holds 50.5 MB and SG 53.2 MB. An idle connection
costs 4.96 KB in SG, 5.00 KB in `tokio` and 4.17 KB in C, of which 4 KB is the buffer every version gives a
connection; Zig, a thread for each connection, holds 272 KB for each and stops short of 100,000.

**Many connections.** Where many connections wait, SG gives the best tail: 10,000 connections that each ask
once a second are answered within 2.3 ms at p99, against 3.9 s for `tokio` and 1.7 s for Zig, and 10,000 idle
connections beside 100 busy ones leave the busy ones a p99 of 174 µs, against 584 µs for `tokio` and 778 µs for
C. With 1,000 connections kept open, HTTP in SG answers 1.53 times as often as hyper, at a third of its p99.

**Few connections, more than one core.** With 50 connections SG answers 0.64 times as often as Zig's threads
and 0.39 times as often as C, and with one connection 0.79 times as often as Zig. This is the line not held
yet, and the reason was found: on one core SG answers 396,000 requests a second, more than C (349,000) and
Rust (360,000), so the program is not slow; on four cores C answers nearly four times as many and SG and
`tokio` not even one and a half. SG's loop of the network is waited on by one thread, which wakes the tasks of
the sockets onto its ring, and the other threads take them from there: a request is read on one core and
served on another, with a wake between, and each answer costs 2.7 µs in the kernel where it costs 1.4 µs on
one core. C keeps each connection on the thread whose loop took it. Mending that is the next slice
([Slower than C](slower-than-c.md)).

**At a fixed rate.** At 10% and 40% of the slowest's answers a second, every server, C too, answers in the same
time, about 0.6 ms at p50 and 1.2 ms at p99: that is the time the virtual machine takes to wake a core that has
nothing to do, not the servers'. A machine of its own is needed for the latency of a light load; the
comparison between the servers holds.

## The long run

Thirty minutes of the store at half the slowest server's answers a second, 1,000 connections, each opened again
after 1,000 answers (choice 1707); the server looked at every 10 seconds and for 30 seconds after the load
stops. One run, SG and Rust; Zig and C are to come.

| | SG | Rust |
|---|---|---|
| Answers a second | 283,557 | 283,557 |
| p99, the most | 1.00 ms, 8.38 ms | 738.20 ms, 2,058.16 ms |
| p99 in the 5th minute, the last | 1.06 ms, 1.10 ms | 950.01 ms, 870.32 ms |
| Memory at 5 minutes, the last, the most | 22.2 MB, 22.2 MB, 23.0 MB | 13.1 MB, 13.3 MB, 14.2 MB |
| After the load: processor, files open, memory | 0.0 s in 30 s, 7 (7 at the start), 22.0 MB | 0.0 s in 30 s, 7 (7 at the start), 12.4 MB |

Neither grows or leaks: the memory of the last minute is that of the fifth, the files open after the load are
those before it, and both go quiet. SG keeps its tail for the whole half hour; `tokio` keeps the rate but its
p99 is near a second, for a reason not looked into (the rival's, not SG's to mend). SG holds 9 MB more than
`tokio` here, which goes into [Slower than C](slower-than-c.md).

## How it is measured

`python3 trials/server/measure.py` builds every server and the load and runs each case on Linux, in the image
of [`trials/server/Containerfile`](../trials/server/Containerfile), which it starts with podman; `--soak` adds
the long runs, `--runs 3` takes the median of three, and `--table` prints these tables from the results.

- **The machine.** Apple M3 Max; Linux 7.1 for arm64 in podman's virtual machine, 8 cores and 9.3 GB;
  rustc 1.99.1, Zig 0.17.1, GCC 14.2 for SG's C and for C. The server runs on cores 0-3 and the load on cores
  4-7 (`taskset`), so the two do not take each other's time.
- **The servers.** What each version does the same way is written at the top of
  [`resp/c/resp.c`](../trials/server/resp/c/resp.c) and [`http/c/http.c`](../trials/server/http/c/http.c).
  The store: a part of RESP2 (PING, GET, SET, DEL), 64 maps each behind a lock, chosen by the FNV-1a of the
  key; a buffer of 4 KB for each connection that grows when one command does not fit; every whole command in
  the buffer answered and the answers sent together. HTTP: GET /ping answered with `pong`, the request line and
  every header read. SG is built with `sg build --release`; Rust is `tokio` 1.54 with the runtime
  `#[tokio::main]` makes, and hyper 1.12 for HTTP; Zig is `std.Io` on `Io.Threaded`, a thread for each connection,
  and `std.http.Server`, built `ReleaseFast` (Zig 0.17.1's `Io.Evented` on io_uring has no `netAccept` yet);
  C is a thread on each core, each with a listener of its own (`SO_REUSEPORT`) and its own loop on epoll, built
  with `-O2`. Rust and SG hash the keys with SipHash-1-3 and a key drawn at the start, as C does; Zig's map uses
  Wyhash, with no key. Rust, Zig and C set `TCP_NODELAY` on each connection, as servers do; SG has no way to ask
  for it yet. hyper adds a `date` to every answer. SG's keys must be UTF-8, the others take any bytes.
- **The load.** [`load/load.c`](../trials/server/load/load.c), four threads. In a closed loop each connection
  sends its next request when an answer comes (the answers a second); in an open loop (`--rate`) the requests
  go out at a fixed rate and the time of each answer is counted from when its request was due, not from when it
  went, so a server that stalls is charged for everything that waited behind it. A histogram gives p50, p99,
  p99.9 and the most. Each case starts its server afresh; the store is first given 10,000 keys, not measured;
  2 seconds go by before 10 are measured. The rates of the open loop are shares of the slowest server's
  answers a second with 50 connections. Idle connections come from four addresses of the loopback, and each is
  answered once before the memory is taken.
- **The load held against `redis-benchmark` and `wrk`**, once, on 22 November 2026, with 50 connections and a
  request in flight on each (`redis-benchmark -t get -c 50 --threads 4`, `wrk -t4 -c50`), the clients on the
  same cores as the load. Where the server is the limit the three agree within 3%: the store in SG 557,000 a
  second by the load and 545,000 by `redis-benchmark`, in Rust 594,000 and 571,000; HTTP in SG 580,000 and
  565,000 by `wrk`, in Rust 535,000 and 545,000. With the faster servers the other two clients reach their own
  limit first (`redis-benchmark` 545,000 for Zig and 666,000 for C, where the load gets 897,000 and 1,467,000;
  `wrk` 756,000 and 1,052,000, the load 895,000 and 1,436,000), so they would show SG nearer to Zig and C than
  the load does, never further.
- **The server, through `/proc`.** The memory it holds at the end of a case (VmRSS), its threads, and the time
  of the processor while measured. An idle connection is the memory it adds over the server just started,
  and the time is the processor's in 10 seconds with every connection silent. A case where a connection failed
  or an answer was wrong is shown as failed, never with numbers.
