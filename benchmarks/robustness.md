# What the servers stand

A server is not only as fast as it answers. It has to stay up while its clients reset, close half a
connection, send a byte at a time, stop reading, send what is not the protocol, outnumber its descriptors,
and while it is asked to stop. This page holds the two servers of the [server lane](server.md), the store
that speaks a part of Redis's protocol and the HTTP server, each written in SG, Rust, Zig and C, to those
clients (choices 1750 to 1763). The code is [`trials/server/load/stand.c`](../trials/server/load/stand.c).

**These are the numbers of a short try**: the heavy level of choice 1759 with every count at 2% of what it is
to be (`ROBUST_SCALE=0.02`), taken on 23 November 2026 after the 160th slice. The heavy level itself (a
million resets, 100,000 clients beside 1,024 descriptors, 10,000 that do not read, SIGTERM with 50,000 open, a
million requests that are not the protocol) runs for hours and is to be run at night; its numbers will replace
these.

<!-- Linux 7.1.3-200.fc44.aarch64 aarch64, 8 cores; rustc 1.99.1 (58cf8f9c3 2026-10-13); zig 0.17.1; cc (Debian 14.2.0-19) 14.2.0 -->

## Standing

A server stands a scenario when, after each round of it:

- it still runs, and has not panicked or crashed;
- every answer that came was right: byte for byte in RESP, status, length and body in HTTP;
- a plain client is answered within 5 seconds;
- it holds no more descriptors than before the round, within five seconds of its clients leaving;
- the memory it holds idle after the last round is within 10% and 2 MB of what it held after the second.
  The first round warms the server up, since a C library keeps what it was given, and the peak is told but not
  held to the rule: it moves with how the connections of a round happen to meet (choices 1762, 1763). What
  leaks stays behind in the memory held idle.

## The scenarios

| Scenario | What the clients do | Also held to |
|---|---|---|
| answers | Requests in flight, every answer checked | |
| fds | The server is started with 1,024 descriptors (`RLIMIT_NOFILE`), and 100 times as many clients hold connections open for two seconds; then each asks once, trying again when refused | Less than half a second of the processor in a second while its descriptors are taken: it must not spin |
| reset | Connections reset (`SO_LINGER` 0) before a request, in the middle of one, while the answer is awaited, or in the middle of an answer of 1 MB (RESP) or of 2,000 answers (HTTP) | |
| half | Requests, then the sending half closed | Every answer, then the close |
| split | A byte at a time, answers read a byte at a time through the smallest receive buffer; in RESP a value of 1 MB sent in pieces of random size and read back a byte at a time | |
| trickle | A byte at a time with up to 20 ms between bytes | |
| noread | Clients ask for far more than they read and never read (RESP: GET of a value of 4 KB, as often as a gigabyte of answers takes; HTTP: 100,000 requests) | The memory it grows by stays bounded, and a plain client is answered beside them |
| noread-limit | As noread, against SG's servers started with `--idle-limit 5000` | The server lets go of every one of them within 8 seconds |
| term | Connections in the middle of a request, and ten that send a byte every 50 ms, then SIGTERM, against SG's servers started with `--grace 2000` | It ends with 0 within the grace and 3 seconds; under the sanitizers, with no leak |
| fuzz | Requests that are not the protocol, drawn from a fixed seed: lengths too long, negative or past 64 bits, `*1000000`, halves, random bytes, heads too large, bare line feeds, a `Content-Length` twice; most then close their sending half, the rest reset | Each answered or closed within a second of its request; a plain client answered after every thousand |

The scenarios marked SG only are those the other servers have no flag for (a limit on a request, a grace to
stop in); they are not compared there (choice 1751). Each scenario that ends with "-sanitized" is SG's server
built under the address and undefined-behaviour sanitizers, with the leak check of the address sanitizer when
the server ends.

## The results of the short try

A cell says whether the server stood, and the memory it held idle after the last round. The counts are those
of the short try: 20,000 connections for answers, reset and half; 2,000 a byte at a time, 1,500 slow, 600 that
do not read, 12,000 against 1,024 descriptors, 1,000 at SIGTERM, 20,000 requests that are not the protocol.

### The store (RESP)

| Scenario | SG | Rust | Zig | C |
|---|---|---|---|---|
| answers | stood, 4.5 MB | stood, 5.7 MB | stood, 50.9 MB | stood, 4.0 MB |
| fds | stood, 12.6 MB, 10 ms of the processor | stood, 12.1 MB, 10 ms | stood, 487.8 MB, 0 ms | **fell**: it spins, 4,010 ms of the processor in a second |
| reset | stood, 575.3 MB | **fell**: held 218.5 MB idle after the second round, 418.2 MB after the last | stood, 750.3 MB | stood, 693.1 MB |
| half | stood, 4.5 MB | stood, 6.1 MB | stood, 56.0 MB | stood, 4.4 MB |
| split | stood, 6.0 MB | stood, 5.9 MB | stood, 12.6 MB | stood, 3.1 MB |
| trickle | stood, 3.2 MB | stood, 4.3 MB | stood, 28.7 MB | stood, 3.2 MB |
| noread: grew by | stood, 13.6 MB | **fell**: idle 41.3 MB after the second round, 79.6 MB after the last; grew by 104.1 MB | stood, 154.2 MB | stood, 104.3 MB |
| noread-limit | stood, every client let go | | | |
| term | stood: ended with 0 in 2,019 ms | | | |
| fuzz | stood | stood | stood | stood |
| fuzz, reset, term sanitized | stood, stood, ended with 0 in 2,018 ms with no leak | | | |

### HTTP

| Scenario | SG | Rust (hyper) | Zig | C |
|---|---|---|---|---|
| answers | stood, 2.4 MB | stood, 4.8 MB | stood, 34.1 MB | stood, 2.5 MB |
| fds | stood, 8.3 MB, 0 ms of the processor | stood, 20.7 MB, 10 ms | stood, 290.7 MB, 0 ms | **fell**: it spins, 4,020 ms of the processor in a second |
| reset | stood, 16.1 MB | **fell**: 22 descriptors after the round where it had 8, still 19 after the last | stood, 760.6 MB | stood, 11.0 MB |
| half | stood, 1.9 MB | **fell**: the connection closed with an answer still to come | stood, 57.2 MB | stood, 2.6 MB |
| split | stood, 1.9 MB | stood, 3.7 MB | stood, 11.2 MB | stood, 1.7 MB |
| trickle | stood, 2.1 MB | stood, 5.1 MB | stood, 28.0 MB | stood, 1.9 MB |
| noread: grew by | stood, 2.6 MB | **fell**: idle 10.8 MB after the second round, 16.8 MB after the last; grew by 58.4 MB | stood, 49.7 MB | stood, 2.3 MB |
| noread-limit | stood, every client let go | | | |
| term | stood: ended with 0 in 2,016 ms | | | |
| fuzz | stood | stood | stood | stood |
| fuzz, reset, term sanitized | stood, stood, ended with 0 in 2,021 ms with no leak | | | |

## What the stand found

**In SG's store, mended in the same slice.** Clients that ask for much and read nothing made the store hold
everything they asked for: it answered every whole command in its block of 4 KB, about 150 GETs, and kept the
answers until they were sent, so 20 clients asking for a value of 1 MB held 3 GB of the server. The C store
does the same and is measured as it is. Now a connection holds at most 64 KB of answers and the last one whole,
and runs the commands after them once they are sent. `--idle-limit` covered only the wait for a request and
was set again only while no part of one was in the block: it never cut a client that does not read, and could
cut a busy client whose block never emptied. Now it is set again each time a whole request is answered, and
covers the sending of the answers. Both servers took `--grace` for the scenario of SIGTERM, and the HTTP server
`--idle-limit`; none of this changes what the measured cases of [the server lane](server.md) do.

**In the others, as they are.**
- C's two servers spin when their descriptors are taken: the listener is on each thread's epoll without
  `EPOLLET`, its `accept` fails with `EMFILE` and leaves the connection waiting, so epoll tells of it again at
  once and four cores go round for nothing until a descriptor is free. SG waits 10 ms after a failed accept;
  `tokio` and Zig spent 0 to 10 ms of the processor in that second.
- `tokio`'s store held more memory idle after each round of resets and of clients that do not read
  (glibc's arenas for its threads, not looked into); hyper let connections go late after resets, and
  closes a connection whose sending half the client closed without answering what was in it (hyper's
  `half_close` is off by default).
- Every server holds hundreds of megabytes idle after the resets in the middle of answers of 1 MB, which a C
  library keeps for the next: SG 575 MB, C 693 MB, Zig 750 MB, Rust 418 MB and growing. Zig's servers, a thread
  for each connection, hold 290 to 490 MB after 12,000 connections.

**Not understood yet, on macOS only.** When the stand ends many connections at once with a reset, some of
SG's sockets stay `ESTABLISHED` in the kernel of the server, with nothing left on the client's side: in three
runs of four of the scenario split, none of C's in four, and on Linux never. The kernel has the socket
open, so no program could tell; the test on macOS ends its connections with a close instead. Without
permission to watch the packets it was left here.

## A client out of local ports

The other side of the stand (choice 1751 i): an SG client on a machine given 100 local ports connects 300
times to a server of its own and keeps every connection. Past the ports, `TcpStream.connect` and
`connectAsync` give `IoError.AddressNotAvailable` (choice 1764; `EADDRNOTAVAIL`) and the program goes on; once its
connections are let go, a new one connects and sends. On Linux 99 connections were made interpreted and 98
compiled (the listener holds one of the 100 ports). It is the test `a_client_out_of_local_ports` of
[`tests/cli.rs`](../interpreter/tests/cli.rs), which runs only on a machine so narrowed, since it would starve
the tests beside it: a last step of the job on Linux narrows the range with `sysctl` and runs it alone, and on
this machine `podman run --sysctl net.ipv4.ip_local_port_range="40000 40099"` does.

## What is not tried

A limit of memory by cgroup on the server, TLS, a load over IPv6, Windows, a network that loses or delays
packets and io_uring. The long run of 30 minutes is
[on the page of the servers](server.md#the-long-run) and stays a run at night on this machine (choice 1757).

## How it is measured

`python3 trials/server/measure.py --robust` builds every server and the stand and runs each scenario on Linux in
the image of [`trials/server/Containerfile`](../trials/server/Containerfile), with podman: the server on cores
0-3, the stand on cores 4-7, connections from four addresses of the loopback, the image held to 7 GB of memory
and the stand stopping a server that holds more than 6 GB. `ROBUST_SCALE=0.02` gives the short try. The results
are written to `trials/target/server/robust-linux-*.json`.

The quick level runs in every `cargo test` on Linux and macOS (`interpreter/tests/cli.rs`,
`the_servers_stand`) and in the continuous integration on Linux: SG's two servers, built for release and under
the sanitizers, against 10,000 connections on Linux and a tenth of them on macOS, which keeps the port of a
connection the client closed 30 seconds and has 16,384 of them. It holds the servers to everything above but
the rule of rounds for memory, which needs a quiet machine: beside the other tests the arenas of glibc warm up
round by round for ten rounds and more (the store's memory held idle went from 11.6 to 14.3 MB in ten rounds
with the cores busy, 6.6 to 6.7 MB with them free); the bound on clients that do not read is kept.
