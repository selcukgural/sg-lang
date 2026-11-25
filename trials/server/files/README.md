# The file server

GET and HEAD of the files under a folder over HTTP/1.1, in SG, Rust, Zig and C (choices 1861-1873), under
the lane's load, with nginx measured beside them. What every version does the same way is written at the top
of [`c/files.c`](c/files.c); the numbers are in [Servers against Rust, Zig and C](../../../benchmarks/server.md#files).

| Folder | What it is |
|---|---|
| [sg](sg/files.sg) | On `HttpConnection` of `std/http`: the disk asked as tasks (`File.openRegularAsync`, which opens only a regular file, and `readSomeAsync`), the size and the time asked of the open file (`reader.info()`), each connection a task, a body begun with its length (`beginAsync(status, type, length)`) and sent in pieces of 64 KB |
| [rust](rust) | hyper 1 on tokio with `tokio::fs` and `tokio-util`'s `ReaderStream`, written by hand and not with `tower-http`'s `ServeDir`, whose own ways (a folder listed, its own 404s and ranges) would keep the four from being held to one another |
| [zig](zig/files.zig) | `std.http.Server` on `Io.Threaded`, a thread for each connection; the file read with `readPositional` into a buffer of 64 KB and written, never with `sendFile` |
| [c](c/files.c) | epoll, a loop on each core, `stat`, `open` and `pread` in the loop as nginx does without `aio threads`; with `-DSENDFILE` the body goes out with `sendfile`, measured beside the four as what the copy of the other four costs (1864) |
| [check.py](check.py) | Every server held to a model of the rules in Python, over raw sockets: 320 requests at every edge, 1,001 ranges at random, requests sent together, files cut, grown and removed while they are sent; with `--stand` clients of 1 GB that do not read, read slowly and reset halfway; with `--nginx` where nginx answers otherwise, printed and not held |
| [measure.py](measure.py) | The cases, on Linux in the lane's image, on the machinery of `../measure.py` |

```sh
python3 trials/server/files/measure.py              # every case, on Linux in the image
python3 trials/server/files/measure.py --check      # check.py and the load's own check, natively
python3 trials/server/files/check.py --stand sg=trials/target/server-bin-darwin/files-sg ...
```

## Fairness

- No version keeps a file in memory; each reads the system's cache, which every file is read into before a
  case, so that the disk is measured in none. The files lie on a volume of the machine's own file system (xfs),
  as a server in a container serves them: the layers of the image (overlayfs) and tmpfs have no reads told not
  to wait (`RWF_NOWAIT`), which SG asks for first (choice 1872), and on them every read of SG's goes to a thread.
- The body goes in pieces of 64 KB read into the program and written in all four, `sendfile` in none: it is a
  way of working, which goes into all or none (choice 1819), and Rust's hyper and tokio do not have it. What
  it would give is shown by C with `sendfile` and by nginx, which uses it as it is usually run.
- The disk is asked as each language's server asks it: Rust hands every call to a pool of threads and waits, as
  `tokio::fs` does; SG asks first where the kernel promises not to wait and hands the call over only when it
  would (choices 1871, 1872: SG's rule is that nothing holds the thread in async code); Zig blocks the thread of
  its connection; C blocks its loop, as nginx does by default. Zig's io_uring (`Io.Evented`, `zig/uring.zig`)
  was to be measured beside its default and does not compile in Zig 0.17.1, which `measure.py` says.
- Each version opens the file before it looks at `If-Modified-Since` and `Range`, so a file that cannot be read
  is 403 whatever they ask, as nginx answers it (1873). C, Rust and Zig ask what the path is first (`stat`) and
  then open it, two calls; SG opens with `File.openRegular`, which on Linux looks at the path through a
  descriptor of the path alone and at the open file again, since nothing opens only a regular file in one call
  before Linux 7.2 (`OPENAT2_REGULAR`): four calls, two of them walking the path.
- What each language's HTTP refuses before the server sees it (a lower-case method, a byte that is not ASCII
  in the target) is held to its status only, or to the connection closed; Zig's `std.http.Server` writes
  `content-length: 0` on a 304, which `check.py` lets be.

## What it found

- `std/http` could not send an answer whose length was known before its body: an answer begun was always in
  chunks. `beginAsync` takes the length now (1865).
- `std` had no HTTP dates and no percent-encoding (1866, 1867).
- `File.info` was called as it is in an async function, holding the thread while the disk answered (1871).
- Measured first, every call of the disk handed to a thread, SG gave a sixth to an eighth of C's answers, level
  with Rust; asked first where the kernel will not wait (1872), with a body's large parts sent without a copy
  and a buffer the size of a small file, it gives what the tables show.
- `check.py` itself: a client's small buffer set after the connection made Linux drop what the server had sent
  into the larger window, and the resending backed off past 20 seconds for every server; it is set before.
- The scan of which function holds its thread missed a static function written `Type.f(...)`, so a function
  calling `File.exists` or `SocketAddress.lookup` was not found holding; and a task started inside `std` by
  another function of `std` was placed where the program called it, which C could not take as a constant.
- For a file of 1 KB SG made twice C's calls of the system: `File.infoAsync` and then `File.openTextAsync` each
  looked at the path through a descriptor of its own before the file was opened, so that no FIFO would be.
  `File.openRegular` and `reader.info()` (1873) take one walk of the path and one close away: 0.71 times C's answers of 1 KB with 50 connections where it was 0.66, and 0.63 with 1,000 where it was 0.52.
- Rust and Zig served a target with a byte that is not ASCII, which HTTP does not allow; SG accepted the last
  second a `DateTime` holds, where the three others stopped a second early. The four agree now.
