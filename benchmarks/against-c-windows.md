# SG against C on Windows

The programs of [SG against C](against-c.md), measured again on Windows. How the numbers are taken is in
[How it is measured](method.md).

## How it was run

Measured on Windows (`python3 bench/run.py`, the hundred-and-second slice): Windows 11 on 64-bit Arm in a
virtual machine on the Mac of [SG against C](against-c.md), Clang 23 with C's library in the program (`/MT`, as `sg build`
links), best of five after a run that is not timed, which also leaves out the first start of a new program
(which on Windows took seconds now and then; why was not looked into); `spawn`, `spawnasync`, `capture`,
`pipelines` and `pipesasync` are one timed run each (`--runs 1`), since five thousand starts take over twenty
seconds there. The last column is the same ratio on macOS, from [SG against C](against-c.md) or from the
program's choice. Windows on x86-64 is not measured: there is no such machine here, and the machines of the CI are
shared and give no steady numbers.

## The table

| Program | SG, release | SG, overflow checked | C | release / C | on macOS |
|---|---|---|---|---|---|
| `fib` | 0.276 s | 0.393 s | 0.273 s | 1.01 | 0.98 |
| `primes` | 0.488 s | 0.475 s | 0.471 s | 1.04 | 0.99 |
| `collatz` | 0.339 s | 0.540 s | 0.340 s | 1.00 | 1.00 |
| `words` | 1.155 s | 1.094 s | 1.100 s | 1.05 | 1.02 |
| `list` | 0.433 s | 0.444 s | 0.431 s | 1.00 | 1.01 |
| `expr` | 0.342 s | 0.346 s | 0.338 s | 1.01 | 1.02 |
| `parse` | 0.286 s | 0.315 s | 0.284 s | 1.01 | 0.99 |
| `vector` | 2.117 s | 2.371 s | 2.114 s | 1.00 | 1.02 |
| `counts` | 0.408 s | 0.404 s | 0.343 s | 1.19 | 1.20 |
| `show` | 2.876 s | 3.025 s | 8.363 s | 0.34 | 0.47 |
| `lookup` | 0.501 s | 0.488 s | 0.483 s | 1.04 | 1.01 |
| `buffer` | 0.287 s | 0.300 s | 0.287 s | 1.00 | 0.99 |
| `bits` | 0.339 s | 0.303 s | 0.369 s | 0.92 | 0.79 |
| `hash` | 0.132 s | 0.134 s | 0.131 s | 1.01 | 1.00 |
| `nbody` | 0.669 s | 0.669 s | 0.673 s | 0.99 | 0.97 |
| `numbers` | 0.222 s | 0.225 s | 0.498 s | 0.45 | 0.95 |
| `floats` | 0.182 s | 0.195 s | 0.735 s | 0.25 | 0.23 |
| `fields` | 1.349 s | 1.358 s | 1.487 s | 0.91 | 1.02 |
| `lines` | 0.072 s | 0.098 s | 0.454 s | 0.16 | 0.82 |
| `copy` | 0.160 s | 0.187 s | 0.910 s | 0.18 | 0.54 |
| `bytes` | 0.061 s | 0.059 s | 0.064 s | 0.95 | 0.94 |
| `table` | 2.524 s | 2.476 s | 3.325 s | 0.76 | 0.52 |
| `blocks` | 0.107 s | 0.120 s | 0.117 s | 0.91 | 1.00 |
| `keywords` | 0.233 s | 0.234 s | 0.216 s | 1.08 | 1.03 |
| `classes` | 0.209 s | 0.269 s | 0.234 s | 0.89 | 0.87 |
| `rewrite` | 0.575 s | 0.625 s | 0.720 s | 0.80 | 0.94 |
| `tree` | 0.194 s | 0.244 s | 0.114 s | 1.71 | 1.53 |
| `tasks` | 0.121 s | 0.189 s | 0.123 s | 0.99 | 1.00 |
| `async` | 0.236 s | 0.354 s | 0.208 s | 1.14 | 1.08 |
| `asynclines` | 0.153 s | 0.171 s | 0.458 s | 0.33 | 1.28 |
| `tasklist` | 0.534 s | 0.746 s | 0.445 s | 1.20 | 1.24 |
| `asynccopy` | 0.384 s | 0.434 s | 0.920 s | 0.42 | 1.04 |
| `echo` | 0.861 s | 0.850 s | 0.769 s | 1.12 | 0.89 |
| `echosync` | 0.873 s | 0.778 s | 0.769 s | 1.13 | 0.99 |
| `echowork` | 0.456 s | 0.457 s | 0.551 s | 0.83 | 1.02 |
| `netlines` | 0.096 s | 0.132 s | 0.111 s | 0.86 | 1.14 |
| `ring` | 0.343 s | 0.342 s | 0.343 s | 1.00 | 1.00 |
| `crc` | 2.800 s | 2.798 s | 2.798 s | 1.00 | 0.99 |
| `deflate` | 1.312 s | 1.311 s | 1.279 s | 1.03 | 1.04 |
| `clock` | 0.237 s | 0.228 s | 0.236 s | 1.01 | 1.01 |
| `dates` | 0.123 s | 0.303 s | 0.106 s | 1.16 | 0.29 |
| `rng` | 0.115 s | 0.115 s | 0.115 s | 1.00 | 1.00 |
| `secure` | 0.137 s | 0.137 s | 0.139 s | 0.98 | 1.00 |
| `listing` | 0.316 s | 0.316 s | 0.276 s | 1.14 | 0.99 |
| `status` | 11.688 s | 11.795 s | 15.677 s | 0.75 | 1.00 |
| `spawn` | 22.829 s | 22.798 s | 22.761 s | 1.00 | 0.70 |
| `capture` | 1.096 s | 1.279 s | 1.337 s | 0.82 | 1.49 |
| `pipelines` | 0.091 s | 0.113 s | 0.502 s | 0.18 | 0.85 |
| `pipesasync` | 0.153 s | 0.166 s | 0.507 s | 0.30 | 1.03 |
| `spawnasync` | 22.644 s | 23.007 s | 22.968 s | 0.99 | 0.71 |
| `case` | 0.176 s | 0.179 s | 5.273 s | 0.03 | 0.23 |
| `letters` | 0.295 s | 0.376 s | 4.692 s | 0.06 | 0.10 |
| `udpecho` | 1.089 s | 1.080 s | 1.365 s | 0.80 | 1.19 |
| `udpechosync` | 1.299 s | 1.306 s | 1.284 s | 1.01 | 1.00 |
| `inline` | 1.015 s | 1.012 s | 1.012 s | 1.00 | 1.03 |
| `callback` | 0.914 s | 0.925 s | 0.913 s | 1.00 | 1.00 |
| `sqlite` | 0.300 s | 0.301 s | 0.301 s | 1.00 | 1.00 |
| `curl` | 0.366 s | 0.367 s | 0.365 s | 1.00 | 1.02 |
| `signals` | 0.008 s | 0.007 s | 0.246 s | 0.03 | 1.27 |
| `bitfields` | 0.371 s | 0.335 s | 0.352 s | 1.05 | 0.91 |
| `atomics` | 0.226 s | 0.212 s | 0.260 s | 0.87 | 1.02 |
| `queue` | 0.247 s | 0.250 s | 0.255 s | 0.97 | 1.39 |
| `library` | 0.582 s | 0.576 s | 0.568 s | 1.02 | 1.01 |
| `kernel` (size) | 5168 B | | 2236 B | 2.31 | 2.31 |

## How the C differs on Windows

The C is the same program wherever it can be. Where POSIX has a function Windows has not, the C takes
Windows' own (`bench/windows/posix.h`): threads of `_beginthreadex` and locks of `SRWLOCK`, as SG's runtime
has them there; Winsock; `getline` written over `fgets`, and the input read as bytes; `popen` through
`cmd.exe`. `clock` reads `QueryPerformanceCounter`, `secure` calls `BCryptGenRandom`, `listing` lists with
`FindFirstFileExW`, `spawn` and `capture` start `CreateProcessW`, `netlines` cuts its lines out of what
`recv` gives (Windows' C library cannot open a `FILE` over a socket), and `signals` is C's `signal` and
`raise` with `SIGTERM` and a pipe of C's library, since Windows has no `SIGUSR1`. `dates` takes the moments
apart by hand (Howard Hinnant's `civil_from_days`), since `gmtime_s` refuses a moment before 1970: so on
Windows it is the same algorithm in both, where on macOS it is the C library's `gmtime_r`. Windows has no
`cat` and no `true`, so `run.py` builds both from `bench/windows` and puts them first on `PATH`, for SG and
C alike. `case` is compared by the size of its text only: its C maps a character to one by the tables of
the C library, and Windows' writes three bytes more each line.

## What the numbers say

Most of the rest is the time of C, as on macOS. Where SG is much faster, the difference is Windows' C
library: `printf` (`show`, `numbers`, `floats`), `fgets` (`lines`, `copy`, `asynccopy`, `pipelines`,
`pipesasync`) and the functions of wide characters (`case`, `letters`) are several times slower there than
on macOS, and SG uses none of them. `signals` is 0.03 because `Signals.raise` on Windows hands the signal on
inside the program, where C's `raise` calls the handler, which writes into a pipe. Where SG is slower:
`tree`, `counts`, `async` and `tasklist` as on macOS; `dates`, `listing`, `echo` and `echosync` on Windows
only, in [Slower than C](slower-than-c.md).
