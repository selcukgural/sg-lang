# The server lane

Two servers, each written in SG, Rust, Zig and C, and one load that drives them all the same way
(choices 1698-1707). The numbers, and what they say, are in
[Servers against Rust, Zig and C](../../benchmarks/server.md).

| Folder | What it is |
|---|---|
| [load](load) | The load, in C: closed loop (answers a second) or open loop (`--rate`, the time of each answer counted from when it was due, so a server that stalls is charged for what waited behind it), idle connections from several source addresses, a check of every answer byte for byte, a line for each interval of a long run (`--report`), connections opened again (`--reconnect`) |
| [resp](resp) | A store that speaks a part of RESP2, Redis's protocol: PING, GET, SET, DEL, over 64 maps each behind a lock |
| [http](http) | An HTTP/1.1 server that answers GET /ping with `pong`, connections kept open |
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
python3 trials/server/measure.py --languages sg,sg-limit,rust,zig,c --only resp-trickle-10k --runs 3
                                              # slow clients, and SG's store with --idle-limit 5000 beside the others
```

`sg-limit` is SG's store started with `--idle-limit 5000`, the time a request may take; it is measured only
when named, since the other servers have no such limit (choice 1724). The load's `--trickle MS` makes its idle
connections slow clients, each sending a byte of an unending SET every MS milliseconds; one the server closes is
counted as cut.

`interpreter/tests/cli.rs` (`the_servers_of_the_server_lane`) builds the SG servers and the load and
runs the check on every test run, on Linux and macOS.
