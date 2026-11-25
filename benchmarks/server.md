# Servers against Rust, Zig and C

SG is meant for servers: stores like Redis, databases, HTTP servers. Since 22 November 2026 the line it holds
there is this: on the network SG is never to be worse than the best rival, in speed or in memory, under load or
idle. This page measures three servers written in SG, in Rust, in Zig and in C, under one load, and shows where
SG stands, including where it does not hold the line yet (choices 1698-1707): a store, an HTTP server, and a
server of files, with nginx beside it ([Files](#files)). The code is in
[`trials/server`](../trials/server).

The numbers of the store and of HTTP are those of one run of each case, taken on 22 November 2026 with the runtime of the 157th slice, a loop of the network on each thread of the pool (choices 1708-1714); the median of three runs will replace them. A cell of answers gives the answers a second, the p99 and the memory the server holds at the end; a cell of idle connections gives the memory each connection adds, the memory held, and the processor's time in 10 seconds of silence.

<!-- Linux 7.1.3-200.fc44.aarch64 aarch64, 8 cores; rustc 1.99.1 (58cf8f9c3 2026-10-13); zig 0.17.1; cc (Debian 14.2.0-19) 14.2.0 -->

### The store (RESP)

| Case | SG | Rust | Zig | C |
|---|---|---|---|---|
| 1,000 idle connections | 4.94 KB, 6.4 MB, 0.01 s | 5.01 KB, 7.2 MB, 0.01 s | 272.11 KB, 266.6 MB, 0.01 s | 4.18 KB, 5.5 MB, 0.01 s |
| 10,000 idle connections | 4.97 KB, 50.1 MB, 0.09 s | 5.00 KB, 51.2 MB, 0.07 s | 272.35 KB, 2660.5 MB, 0.07 s | 4.17 KB, 42.1 MB, 0.05 s |
| 100,000 idle connections | 4.96 KB, 486.1 MB, 0.42 s | 5.00 KB, 490.6 MB, 0.32 s | failed: 88952 connections failed | 4.17 KB, 408.8 MB, 0.21 s |
| 1 connection, 1 in flight on each | 32k/s, 39 µs, 3.6 MB | 32k/s, 40 µs, 4.2 MB | 33k/s, 39 µs, 5.2 MB | 33k/s, 39 µs, 3.1 MB |
| 1 connection, 16 in flight on each | 477k/s, 43 µs, 3.6 MB | 580k/s, 36 µs, 4.3 MB | 562k/s, 37 µs, 5.2 MB | 564k/s, 36 µs, 3.1 MB |
| 50 connections, 1 in flight on each | 1,342k/s, 74 µs, 3.8 MB | 600k/s, 167 µs, 4.5 MB | 903k/s, 123 µs, 16.2 MB | 1,478k/s, 77 µs, 3.3 MB |
| 50 connections, 16 in flight on each | 2,077k/s, 772 µs, 3.9 MB | 2,056k/s, 678 µs, 4.6 MB | 1,867k/s, 805 µs, 16.4 MB | 2,016k/s, 664 µs, 3.4 MB |
| 1,000 connections, 1 in flight on each | 1,331k/s, 1.07 ms, 8.4 MB | 1,279k/s, 2.65 ms, 9.2 MB | 731k/s, 2.88 ms, 268.9 MB | 1,503k/s, 1.53 ms, 7.2 MB |
| 1,000 connections, 16 in flight on each | 2,027k/s, 15.83 ms, 10.0 MB | 2,070k/s, 11.30 ms, 9.8 MB | 1,854k/s, 10.52 ms, 270.8 MB | 1,950k/s, 10.45 ms, 9.0 MB |
| 100 connections, 10% of the slowest's answers a second | p99 1.52 ms, p99.9 1.94 ms, 3.9 MB | p99 1.51 ms, p99.9 3.60 ms, 4.7 MB | p99 1.63 ms, p99.9 2.20 ms, 29.5 MB | p99 1.57 ms, p99.9 4.01 ms, 3.5 MB |
| 100 connections, 40% of the slowest's answers a second | p99 1.23 ms, p99.9 1.43 ms, 4.0 MB | p99 1.27 ms, p99.9 1.46 ms, 4.7 MB | p99 1.25 ms, p99.9 1.48 ms, 29.5 MB | p99 1.27 ms, p99.9 1.47 ms, 3.5 MB |
| 100 connections, 70% of the slowest's answers a second | p99 150 µs, p99.9 1.17 ms, 4.0 MB | p99 176 µs, p99.9 1.05 ms, 4.7 MB | p99 1.33 ms, p99.9 3.64 ms, 29.5 MB | p99 200 µs, p99.9 1.16 ms, 3.5 MB |
| 10,000 connections, one request a second each | p99 6.80 ms, p99.9 12.78 ms, 53.2 MB | p99 2298.48 ms, p99.9 5620.37 ms, 47.8 MB | p99 2.11 ms, p99.9 7.14 ms, 2662.7 MB | p99 2.97 ms, p99.9 11.37 ms, 44.5 MB |
| 10,000 idle connections and 100 at 70% | p99 805 µs, p99.9 1.22 ms, 52.5 MB | p99 339 µs, p99.9 1.24 ms, 53.5 MB | p99 606 µs, p99.9 2.35 ms, 2689.4 MB | p99 668 µs, p99.9 1.22 ms, 44.2 MB |
| 50 connections, values of 4 KB | 1,050k/s, 98 µs, 42.6 MB | 602k/s, 170 µs, 43.4 MB | 789k/s, 145 µs, 54.8 MB | 1,036k/s, 111 µs, 42.3 MB |
| 50 connections, values of 64 KB | 337k/s, 215 µs, 631.2 MB | 305k/s, 290 µs, 636.7 MB | 317k/s, 369 µs, 643.6 MB | 352k/s, 334 µs, 630.7 MB |

### HTTP

SG's column was measured with the server written by hand on `TcpStream`. Since the 162nd slice the lane's server
is written on `std/http` (`HttpConnection`), which stood within 2.2% of the hand-written one with the same time
limits ([library.md](library.md#http)); the next run of the lane, with it, replaces this column.

| Case | SG | Rust | Zig | C |
|---|---|---|---|---|
| 50 connections kept open | 1,386k/s, 73 µs, 1.8 MB | 536k/s, 184 µs, 3.2 MB | 907k/s, 124 µs, 13.9 MB | 1,441k/s, 79 µs, 1.5 MB |
| 1,000 connections kept open | 1,350k/s, 1.10 ms, 6.2 MB | 865k/s, 3.81 ms, 13.6 MB | 718k/s, 2.90 ms, 266.4 MB | 1,481k/s, 1.54 ms, 5.5 MB |
| 50 at a time, a connection for each request | 171k/s, 451 µs, 1.7 MB | 171k/s, 641 µs, 3.7 MB | 200k/s, 371 µs, 17.2 MB | 203k/s, 334 µs, 2.0 MB |
| 100 connections, 10% of the slowest's answers a second | p99 1.60 ms, p99.9 7.98 ms, 2.0 MB | p99 1.51 ms, p99.9 3.08 ms, 3.8 MB | p99 1.63 ms, p99.9 2.26 ms, 27.2 MB | p99 1.61 ms, p99.9 5.14 ms, 1.7 MB |
| 100 connections, 40% of the slowest's answers a second | p99 1.29 ms, p99.9 1.48 ms, 2.0 MB | p99 1.26 ms, p99.9 1.45 ms, 3.8 MB | p99 1.31 ms, p99.9 1.52 ms, 27.2 MB | p99 1.31 ms, p99.9 1.51 ms, 1.7 MB |
| 100 connections, 70% of the slowest's answers a second | p99 1.01 ms, p99.9 1.34 ms, 2.0 MB | p99 195 µs, p99.9 979 µs, 3.8 MB | p99 1.10 ms, p99.9 2.65 ms, 27.2 MB | p99 997 µs, p99.9 1.26 ms, 1.7 MB |

## What the numbers say

**More than one core.** With 50 connections and a request in flight on each, SG's store answers 1,342,000
times a second, 1.49 times Zig's threads and 2.2 times `tokio`, and 0.91 times C, which keeps a thread on each
core with a listener and a loop of its own; HTTP the same (1,386,000, hyper 536,000, C 1,441,000), and values of
4 KB go faster than in C. Before the 157th slice SG got 578,000: its loop of the network was waited on by one
thread, which woke the tasks onto its ring for the others to take, so a request was read on one core and served
on another, and on one core SG already answered more than C. Now each thread of the pool waits on a loop of its
own, a socket goes on the loop of the thread with the fewest and stays there, and a task woken by a loop runs on
that loop's thread; the clock's thread looks at the loop of a thread busy with one task for long, so its sockets
are not left waiting (choices 1708-1714).

**Memory.** SG holds less than `tokio` and Zig in every case but two, close to C's: 10,000 connections that each
ask once a second hold 53.2 MB in SG and 47.8 MB in `tokio`, and 1,000 connections with 16 in flight 10.0 MB
against 9.8 MB. An idle connection costs 4.96 KB in SG, 5.00 KB in `tokio` and 4.17 KB in C, of which 4 KB is
the buffer every version gives a connection; Zig, a thread for each connection, holds 272 KB for each and stops
short of 100,000.

**Many connections.** With 1,000 connections kept open, HTTP in SG answers 1.56 times as often as hyper, at less
than a third of its p99, and 0.91 times as often as C. 10,000 connections that each ask once a second are
answered within 6.8 ms at p99, against 2.3 s for `tokio`, 2.1 ms for Zig and 3.0 ms for C.

**At a fixed rate.** At 10% and 40% of the slowest's answers a second, every server, C too, answers in the same
time, about 0.6 ms at p50 and 1.2 to 1.6 ms at p99: that is the time the virtual machine takes to wake a core
that has nothing to do, not the servers'. At 70% the loop on each thread costs SG its tail: the store's p99 is
150 µs, the best of the four, but HTTP's is 1.01 ms against 195 µs for hyper, and 10,000 idle connections beside
100 busy ones leave these a p99 of 805 µs against 339 µs for `tokio`; with one loop SG had 441 and 364 µs. C,
built the same way, has 997 and 668 µs. A request that comes to the loop of a busy thread, or of one asleep in a
core that wakes slowly, waits, where with one loop any thread awake took it. The p50 is lower than `tokio`'s.
This is not mended yet ([Slower than C](slower-than-c.md)), and a machine of its own is needed to measure it
further.

**Where SG still loses.** One connection with 16 requests in flight gets 477,000 answers a second from SG and
580,000 from `tokio`; a connection for each request 171,000 from SG and `tokio`, 200,000 from Zig and 203,000
from C; and with 1,000 connections and 16 in flight each, SG's p99 is 15.8 ms against 10.5 for C. Each is in
[Slower than C](slower-than-c.md).

## A group of tasks, slow clients and a limit on a request

Since the 158th slice the SG servers keep their connections in a `TaskGroup`, which lets go of each as it
ends, where they kept them in a list emptied with `Task.whenAny`; the store also takes `--idle-limit MS`, the
time a request may take from when its first byte is awaited (choices 1715-1724). Two builds by the same
compiler, the servers as they were and as they are, run in turn four times, give the medians below; single runs
on this virtual machine move by up to 10% from one to the next, so they are not compared.

| Case | A list and `whenAny` | `TaskGroup` | `TaskGroup` and `--idle-limit 5000` |
|---|---|---|---|
| Store, 50 connections | 1,354,979/s, p99 72 µs | 1,361,154/s, 72 µs | 1,318,064/s, 76 µs |
| Store, 1,000 connections | 1,252,276/s, 1.08 ms | 1,265,059/s, 1.08 ms | 1,215,866/s, 1.18 ms |
| HTTP, 50 connections | 1,360,500/s, 73 µs | 1,350,800/s, 74 µs | - |
| 100,000 idle connections | 4.96 KB each | 4.98 KB each | closed after 5 s, as meant |

The group costs nothing. The limit costs 3.2% of the answers with 50 connections and 3.9% with 1,000, and a
tenth on the p99: setting the time again is one atomic write, but every read given a token goes on the token's
list and off it under the one lock of the clock, which four threads share ([Slower than
C](slower-than-c.md)).

What the limit gains: 10,000 slow clients, each of which begins a SET of 1 MB once answered and then sends a
byte of it every second (slowloris), beside 100 connections at 70% of the slowest's answers a second; the
memory and the files the server holds are read halfway through the measured time (medians of three runs).

| Server | p99 of the 100 | p99.9 | Memory | Files open | Slow clients closed |
|---|---|---|---|---|---|
| SG | 2.75 ms | 8.62 ms | 52.7 MB | 10,112 | 0 |
| SG, `--idle-limit 5000` | 807 µs | 1.28 ms | 54.0 MB | 112 | 10,000 |
| Rust (`tokio`) | 2.47 ms | 7.49 ms | 53.5 MB | 10,107 | 0 |
| Zig | 11.37 ms | 15.34 ms | 2,689 MB | 10,104 | 0 |
| C | 5.36 ms | 11.57 ms | 44.3 MB | 10,111 | 0 |

With the limit the busy connections are answered within a third of the time at p99, and the 10,000 files of
the slow ones are given back. The memory the server holds does not fall: the buffers are freed inside the
program, and glibc does not give the pages in the middle of its heap back to the system; new connections take
them again. The other servers have no such limit, as most servers of their kind do not; `tokio` and hyper
would bound a request with `tokio::time::timeout`, Zig and C with a timer of their own.

## A reader that lends its block

Since the 159th slice the SG servers read through `conn.reader()` (`fillAsync`, `buffered`, `consume`), where
they kept a buffer of 4 KB of their own and moved what was left of a request to its start after each read
(choices 1734-1740). The block of the reader of a connection starts at 4 KB, as their buffer did, where it took
64 KB before. The same compiler built both, run in turn four times; medians:

| Case | A buffer of their own | The reader |
|---|---|---|
| Store, 50 connections | 1,366,245/s, p99 73 µs | 1,358,553/s, 72 µs |
| Store, 1,000 connections | 1,262,463/s, 1.08 ms | 1,262,719/s, 1.09 ms |
| HTTP, 50 connections | 1,216,697/s, 75 µs | 1,232,847/s, 75 µs |
| 100,000 idle connections | 4.98 KB each | 4.95 KB each |

The reader costs nothing. A first build lost 7 to 10% of the answers and 0.2 KB for each idle connection:
`fillAsync` tried a receive each time before it waited, where `receiveAsync` waits for the loop at once when the
last receive emptied the connection, and it kept a room of its own in the frame for its wait. Now the reader of
a connection fills its block as `receiveAsync` receives, in the one room every call of the network shares, and
the fields of a reader lie with no room between them (96 bytes before, 72 now).

## The breaking point

The load rises until each server breaks (choice 1760), as Anton Putra's videos do it: each connection asks once
every 20 ms, counted from when the request was due; the load goes up by 25,000 requests a second every 10
seconds (more connections as it rises, up to 100,000 from four addresses of the loopback); an answer later than
100 ms is a failure, and the ramp stops two stages after less than half is answered in time. The server has two
cores (0-1) and the load six (2-7, `load --ramp`), so the server is the limit; `python3 trials/server/measure.py
--ramp`. The knee is where p99 is ten times that of the first stage or 100 ms, the saturation where less than 95%
of what is asked is answered, and "most in time" the most answered in time in one stage.

On Linux, 23 November 2026, after the 160th slice's change of `read` to `recv` (the Zig row is of the run before
it, with SG's earlier numbers beside it):

| | SG | C | Rust | Zig |
|---|---|---|---|---|
| Store: most in time | 448,775/s | 473,775/s | 448,775/s | 248,775/s |
| Store: knee, saturation | 475,000, 500,000 | 475,000, 525,000 | 475,000, 500,000 | 275,000, 275,000 |
| Store: past the breaking point | 350,000/s in time at 775,000 asked, p50 13 ms | 1,534/s at 525,000 | 345,000/s, p50 63 ms | 50,000/s, 1.5 GB held |
| HTTP: most in time | 469,883/s | 498,775/s | 321,068/s (hyper) | 248,621/s |
| HTTP: knee, saturation | 475,000, 500,000 | 525,000, 550,000 | 325,000, 350,000 | 175,000, 275,000 |
| HTTP: past the breaking point | 375,000/s at 800,000 asked, p50 11 ms | 3,328/s at 525,000 | 211,000/s, p50 65 ms | 103,000/s, 1.5 GB held |

Before the change (the run of the same morning) SG's store broke at 403,919 a second and its HTTP server at
438,384: SG read its sockets with `read`, the others with `recv`, which takes a shorter way through Linux with
SELinux on ([Slower than C](slower-than-c.md#tasks-async-and-signals), "The breaking point of the store"). C, the fastest until it breaks,
then answers almost nothing in time: each of its threads keeps reading every request that comes and answers each
too late. SG and Rust keep answering as much as they can, and SG's waits are a fifth of Rust's.

On macOS (the M3 Max, natively, 20,000 more a second every stage, up to 10,000 connections from one address; the
load and the servers share the cores, so these say less):

| | SG | C | Rust | Zig |
|---|---|---|---|---|
| Store: most in time, saturation | 99,020/s, 140,000 | 99,020/s, 120,000 | 99,020/s, 120,000 | 99,020/s, 120,000 |
| HTTP: most in time, saturation | 119,020/s, 140,000 | 119,020/s, 140,000 | 99,020/s, 120,000 | 99,020/s, 120,000 |

The results are in `trials/target/server/ramp-*.json`. Three runs of each, for how far apart runs on the virtual
machine lie, are to be taken at night (`--ramp --runs 3`).

## The long run

Thirty minutes of the store at half the slowest server's answers a second, 1,000 connections, each opened again
after 1,000 answers (choice 1707); the server looked at every 10 seconds and for 30 seconds after the load
stops. One run, SG and Rust, with the runtime before the 157th slice; it is to be run again, and Zig and C are to
come.

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

## Files

The third server of the lane serves the files under a folder over HTTP/1.1: GET and HEAD, `index.html`, ranges,
`If-Modified-Since`, 404 and 403 (choices 1861-1872, [`trials/server/files`](../trials/server/files/README.md)).
SG, Rust, Zig and C send a body in pieces of 64 KB read into the program; C with `sendfile` and nginx, which
sends with `sendfile` as it is usually run, are beside them, as what that copy costs. The files lie on a volume
of xfs in the system's cache, read into it before each case. One run of each case, 24 November 2026; the median
of three will replace them; nginx's column is of 25 November, after its room for connections was set to 4,096
from 100,000, which it lays out when it starts and which had cost it 175 MB and up to a quarter of its answers. A
cell gives the answers a second (or the bytes of the body a second, for one client), the p99 and the most memory
the server held (nginx's is the sum of its five processes' proportional sets, which counts what they share
once).

<!-- Linux 7.1.3-200.fc44.aarch64 aarch64, 8 cores; rustc 1.99.1 (58cf8f9c3 2026-10-13); zig 0.17.1; cc (Debian 14.2.0-19) 14.2.0 -->

| Case | SG | Rust | Zig | C | C, `sendfile` | nginx |
|---|---|---|---|---|---|---|
| 1,000 files of 1 KB, 50 connections | 395k/s, 213 µs, 2.2 MB | 52.5k/s, 1.94 ms, 7.6 MB | 486k/s, 245 µs, 14.6 MB | 598k/s, 220 µs, 1.8 MB | 489k/s, 238 µs, 1.6 MB | 521k/s, 159 µs, 15.4 MB |
| 1,000 files of 1 KB, 1,000 connections | 339k/s, 3.61 ms, 11.9 MB | 52.2k/s, 31.46 ms, 33.8 MB | 409k/s, 4.26 ms, 281.9 MB | 646k/s, 2.09 ms, 7.5 MB | 538k/s, 2.83 ms, 5.7 MB | 498k/s, 2.63 ms, 15.1 MB |
| 100 files of 64 KB, 50 connections | 235k/s, 293 µs, 2.4 MB | 48.7k/s, 1.93 ms, 16.9 MB | 286k/s, 362 µs, 17.6 MB | 333k/s, 322 µs, 5.0 MB | 465k/s, 248 µs, 1.6 MB | 152k/s, 111 µs, 14.3 MB |
| 100 files of 64 KB, 1,000 connections | 196k/s, 48.63 ms, 11.3 MB | 36.6k/s, 58.33 ms, 86.7 MB | 131k/s, 10.16 ms, 340.5 MB | 128k/s, 11.27 ms, 71.9 MB | 371k/s, 7.98 ms, 5.7 MB | 441k/s, 3.81 ms, 14.6 MB |
| 20 files of 1 MB, 50 connections | 22.4k/s, 4.57 ms, 2.4 MB | 5.5k/s, 19.60 ms, 18.9 MB | 21.4k/s, 6.47 ms, 17.6 MB | 21.9k/s, 5.78 ms, 5.0 MB | 37.4k/s, 3.69 ms, 1.6 MB | 40.6k/s, 4.55 ms, 14.1 MB |
| 20 files of 1 MB, 1,000 connections | 22.4k/s, 85.46 ms, 11.3 MB | 5.2k/s, 430.96 ms, 150.1 MB | 19.4k/s, 83.62 ms, 340.5 MB | 18.6k/s, 90.96 ms, 71.9 MB | 37.0k/s, 266.86 ms, 5.7 MB | 36.0k/s, 50.46 ms, 21.3 MB |
| A file of 1 GB, one client | 6,979 MB/s, 1.7 MB | 1,611 MB/s, 3.4 MB | 6,818 MB/s, 0.9 MB | 7,194 MB/s, 1.5 MB | 13,153 MB/s, 1.4 MB | 14,657 MB/s, 13.8 MB |
| 4 KB at random places of the file of 1 GB, 50 connections | 216k/s, 456 µs, 3.0 MB | 36.7k/s, 3.60 ms, 8.4 MB | 393k/s, 375 µs, 14.8 MB | 511k/s, 266 µs, 2.1 MB | 455k/s, 301 µs, 1.6 MB | 530k/s, 155 µs, 13.9 MB |
| 304 Not Modified, 50 connections | 597k/s, 185 µs, 2.1 MB | 130k/s, 989 µs, 6.2 MB | 638k/s, 209 µs, 14.6 MB | 850k/s, 195 µs, 1.6 MB | 874k/s, 163 µs, 1.6 MB | 656k/s, 132 µs, 13.9 MB |
| 10,000 files of 4 KB, 50 connections | 303k/s, 364 µs, 2.4 MB | 44.7k/s, 2.90 ms, 8.5 MB | 368k/s, 347 µs, 14.8 MB | 463k/s, 312 µs, 2.1 MB | 414k/s, 349 µs, 1.6 MB | 531k/s, 172 µs, 13.9 MB |
| 404 Not Found, 50 connections | 667k/s, 170 µs, 2.2 MB | 112k/s, 1.17 ms, 6.3 MB | 649k/s, 201 µs, 14.6 MB | 965k/s, 140 µs, 1.6 MB | 943k/s, 166 µs, 1.6 MB | 800k/s, 107 µs, 13.8 MB |

**Small files.** SG answers 0.66 times C's requests of 1 KB with 50 connections and 0.52 times with 1,000, a 404
0.69 and a 304 0.70 times; ahead of Zig on the 404, behind it on files of 1 KB, and behind nginx on both. The disk is not
the difference: on Linux SG asks the kernel first for the state of a file, its opening and its read with the
promise not to wait, and hands a call to a thread only when the kernel would wait (choice 1872), so a 404 makes
the calls of the system C makes. What is left is the processor for each answer, 7.7 ms for a thousand in SG
against 4.8 in C, spread over the parsing in `std/http`, the head of the answer, the task of a connection and
its wakes ([Slower than C](slower-than-c.md#tasks-async-and-signals)). Every call of the disk handed to a
thread, as SG did at first, gave SG 60,400 answers a second here; Rust, which hands every call of `tokio::fs` to
its pool, gives 52,000.

**After `File.openRegular` (25 November, SG and C only).** The server opened each file twice over: `File.info` and
then `File.openText` each looked at the path through a descriptor of its own, so that no FIFO would be opened.
Opened once with `File.openRegular`, which opens only a regular file, and its size and time asked of the open file
(`reader.info()`, choice 1873), and measured again for SG and C (one run of each, the whole table at night):

| Case | SG | C | SG / C | Processor, SG and C, for 1,000 answers |
|---|---|---|---|---|
| 1,000 files of 1 KB, 50 connections | 452k/s, 183 µs, 2.2 MB | 637k/s, 196 µs, 1.8 MB | 0.71 (was 0.66) | 6.6 ms, 4.5 ms |
| 1,000 files of 1 KB, 1,000 connections | 409k/s, 3.04 ms, 11.7 MB | 649k/s, 2.10 ms, 7.5 MB | 0.63 (was 0.52) | 7.3 ms, 4.7 ms |
| 304 Not Modified, 50 connections | 514k/s, 169 µs, 2.1 MB | 713k/s, 160 µs, 1.6 MB | 0.72 (was 0.70) | 5.9 ms, 4.0 ms |
| 404 Not Found, 50 connections | 885k/s, 112 µs, 2.1 MB | 1,259k/s, 89 µs, 1.6 MB | 0.70 (was 0.69) | 3.1 ms, 2.1 ms |

The machine gave more today (C's 404 is 30% above the table's), so the ratio is what compares. Every version now
opens the file before it looks at `If-Modified-Since`, so that a file that cannot be read is 403 whatever is
asked, as nginx answers it: a 304 costs an open and a close it did not, and both SG and C lost a seventh there.

**Large files.** Files of 1 MB go at C's speed in SG, Zig and C (22,400, 21,400 and 21,900 a second), and a
file of 1 GB at 6,979 MB/s against C's 7,194; `sendfile` nearly doubles both, which is the copy into the
program that the four make and nginx does not. With files of 64 KB and 1,000 connections SG answers the most of
the four, but its p99 is 48.6 ms against C's 11.3. nginx's 152,000 with 50 connections and files of 64 KB are not
its speed: about one answer in a few hundred waits some 40 ms, the time of a delayed acknowledgement (its p99.9
is 41.7 ms), which with 1,000 connections does not show.

**A range.** 4 KB at random places of the file of 1 GB: SG 216,000 a second, C 511,000. A reader of a file fills
its block of 64 KB after a seek, so each range reads 64 KB; the reader keeps that rule
([Slower than C](slower-than-c.md#tasks-async-and-signals)).

**Memory.** SG holds 2.2 to 11.9 MB in every case, about what C holds, and less than C's 71.6 MB with 1,000
connections asking for 64 KB or more; nginx's five processes hold 14 to 21 MB. Zig, a thread for each
connection, holds 282 to 340 MB with 1,000.

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
  with `-O2`. The server of files is described in [its folder](../trials/server/files/README.md), and what each
  version does the same way at the top of [`files/c/files.c`](../trials/server/files/c/files.c). Rust and SG hash the keys with SipHash-1-3 and a key drawn at the start, as C does; Zig's map uses
  Wyhash, with no key. Rust, Zig and C set `TCP_NODELAY` on each connection, as servers do; SG has no way to ask
  for it yet. hyper adds a `date` to every answer. SG's keys must be UTF-8, the others take any bytes.
- **Waiting to the nanosecond.** Since the 160th slice the load waits with `epoll_pwait2` on Linux (to the
  nanosecond; `epoll_wait`, to the millisecond, where the kernel lacks it) and `kevent` on macOS, so an open
  loop sends on time at rates where a wait rounded up to a millisecond would bunch the requests; it changes every
  measurement of an open loop, of every server alike.
- **The load.** [`load/load.c`](../trials/server/load/load.c), four threads. In a closed loop each connection
  sends its next request when an answer comes (the answers a second); in an open loop (`--rate`) the requests
  go out at a fixed rate and the time of each answer is counted from when its request was due, not from when it
  went, so a server that stalls is charged for everything that waited behind it. A histogram gives p50, p99,
  p99.9 and the most. Each case starts its server afresh; the store is first given 10,000 keys, not measured;
  2 seconds go by before 10 are measured. The rates of the open loop are shares of the slowest server's
  answers a second with 50 connections. Idle connections come from four addresses of the loopback, and each is
  answered once before the memory is taken.
- **The load held against `redis-benchmark` and `wrk`**, once, on 22 November 2026, before the 157th slice, with 50 connections and a
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
