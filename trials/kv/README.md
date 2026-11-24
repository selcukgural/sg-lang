# kv

A store of keys and values held in memory, every change written first to a log, so that what it answered
`OK` is there again when it starts after a crash: written in SG, in Rust and in Zig, the same algorithm each
time (choices 1844-1853). Redis's append-only file is the same idea. The three write the same log, byte for
byte, and each reads the others'.

```
kv [--sync always|batch|never] DIR
```

`DIR` is made when it is not there (the folder above it must be). The commands come on the standard input,
one a line:

| Command | Answer |
|---|---|
| `SET key value` | `OK`. The key ends at the next space; the value is the rest of the line, spaces and all |
| `GET key` | The value, or `(nil)` |
| `DEL key` | `1` if the key was there, `0` if not |
| `COMPACT` | `OK`, once the log is written again with only what the store holds |

A key and a value are bytes, UTF-8 or not; a key holds no space, and neither holds a line's end. A line ends
at `\n` alone: a `\r` before it is part of the value, and a last line without its `\n` is read too. A line that
is not one of these (`set`, `SET k` without a value, an empty line) is told on the standard error, nothing is
answered for it, and the store ends with 1 once the input is done.

## The log

`DIR/log` is records one after another, each

```
crc u32 | op u8 | key length u32 | value length u32 | key | value
```

little-endian, the CRC-32 (zlib's) that of everything after it in the record; the op is 1 for a set and 2 for
a delete, whose value is empty. A `DEL` of a key that is not there writes nothing.

**When it is on the disk.** The input is read in blocks of up to 64 KB (one read of the system), and what a block
asks is answered once that block's records are where `--sync` says:

- `batch`, the default: the block's records are put on the disk with one sync, then its answers are written. Under
  load that is one sync for many sets (a group commit), and from a terminal one for each line;
- `always`: each record is put on the disk, and its command answered, before the next command is read from
  the block;
- `never`: the log is only handed to the system at the end of each block. A store killed keeps what it answered,
  a machine that loses its power may not.

A sync is `fdatasync` on Linux and `F_FULLFSYNC` on macOS, in each language: macOS's `fsync` leaves the bytes in
the disk's own cache. SG calls `writer.syncData()` and Rust `File::sync_data`, which are those; Zig's library
has only `File.sync`, `fsync`, so the Zig version asks for the same two itself, since what reaches the disk is
the algorithm here and not a way of writing (choice 1819).

**Starting.** The log is read from its start into the map. A record cut off at the end (a crash in the middle of
a write) is cut away (`setLength`), told on the standard error, and the store starts with what came before it.
So is a last record whose CRC is wrong, which a crash leaves when the file grew before its bytes were written.
A record whose CRC is wrong, or whose op is not known, with more of the log after it is damage: the store says
where it is and ends with 1, and the log is left as it is. A `log.new` left by a compaction that did not end is
removed; the log it was to take the place of is whole.

What this cannot tell: a bit turned in a record's length can make that record seem to reach past the end of the
log, and it is then cut away as a crash would leave it, with what came after it. The record has no CRC of its
own head; the trials' format keeps one CRC a record, as Bitcask's does. And a crash that leaves more than 13 bytes
of zeros after the last whole record is damage, not a cut: the store does not start, and says where.

**Compaction.** When the log is longer than 1 MB and than twice the records of what the store holds, or on
`COMPACT`, it is written again into `DIR/log.new` with one set for each key, keys in the order of their bytes;
that file is synced, takes the place of the log (`File.replace`, a rename), and the folder is synced
(`Directory.sync`), so the new name is on the disk. Before it, the folder is synced once when the log is
first made. With `--sync never` none of these syncs is made.

## How it is written

The same in the three: the map holds a copy of each key and value; a set looks its key up once and copies the
key only when it is new (`getOrAdd` in SG, `get_mut` then `insert` in Rust, `getOrPut` in Zig); the lines of a
block are read where they lie in it, and only a line that goes on into the next block is copied. The answers of
a block go into a buffer of their own, written once the block's records are on the disk, so that no `OK` leaves
before its record is there. A compaction sorts the entries by their keys: Rust sorts references to them, Zig
pointers, SG moves them out of the map into a list and back, a list of keys not being `Comparable`.

SG needed two things it did not have (choices 1852 and 1853): a writer put on the disk and cut to a length
(`syncData`, `sync`, `setLength`, `Directory.sync`), and a `List<byte>` as the key of a map, looked up by a
slice of the buffer the line lies in. Before them the store of the server lane kept text keys only.

## Tests

- `python3 trials/measure.py --check kv` builds the five builds and holds them to each other on the commands at
  their edges, on logs Python wrote (read back, cut in the last record, a bit turned in the middle, zeros after
  the last record, empty, a `log.new` left over), on a folder whose parent is missing and on options it does not
  know: the same output, exit code and log.
- `python3 -I trials/kv/tests/check.py [BINARY...] [--interpreted] [--quick]` holds them to a model of the store
  written in Python (choice 1850): streams of commands drawn at random with keys and values of any bytes, each
  `--sync`, long enough to compact; a log cut at every byte of its last records and at places drawn; a bit
  turned at places drawn; each build's log read by each other; the output closed while it writes (it ends by
  SIGPIPE); and a crash: the store killed with SIGKILL at a moment drawn while it is given sets, in each
  `--sync`, then started again, every set it answered being there with its value. A SIGKILL leaves what was
  handed to the system in its cache, so this tests the program and not the disk: a machine that loses its
  power is not tested here, and with `--sync never` it would lose what was answered.
- `interpreter/tests/cli.rs`, `the_trial_of_kv`, runs the SG version interpreted and compiled on every test run.
