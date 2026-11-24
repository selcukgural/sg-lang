//! `kv`: a store of keys and values held in memory, each change written first to a log in the folder
//! it is given, so that what was answered `OK` is there again when the store starts after a crash.
//!
//!     kv [--sync always|batch|never] DIR
//!
//! The commands come on the input, a line each: `SET key value` (the key ends at the next space, the
//! value is the rest of the line, spaces and all), `GET key`, `DEL key` and `COMPACT`; a key and a
//! value are bytes, UTF-8 or not, and a line ends at `\n` alone. `SET` answers `OK`, `GET` the value or
//! `(nil)`, `DEL` `1` or `0`, `COMPACT` `OK`. A line that is not a command is told on the standard error,
//! and the store ends with 1 once the input is done.
//!
//! The log, `DIR/log`, is records one after another (choice 1845), each
//!
//!     crc u32 | op u8 | key length u32 | value length u32 | key | value
//!
//! little-endian, the CRC-32 that of everything after it; the op is 1 for a set, 2 for a delete, whose
//! value is empty. The input is read in blocks of up to 64 KB; what one block asks is answered after
//! its records are on the disk (`--sync batch`, the default), or with the log only handed to the system
//! (`never`); with `always` each command is answered after its record is on the disk. A record cut off at the end of the log, as a crash in
//! the middle of a write leaves it, is cut away and told; a record whose CRC is wrong with more after
//! it is damage, and the store does not start (choice 1847). When the log is more than twice what the
//! store holds, and more than 1 MB, it is written again with only what is held, keys in the order of
//! their bytes, into `DIR/log.new`, which then takes its place (choice 1848).
//!
//! Zig's `File.sync` is `fsync`, which on macOS leaves the bytes in the disk's own cache; what is on
//! the disk is the algorithm here, so the store asks for it as SG and Rust do (`syncData`).

const std = @import("std");
const builtin = @import("builtin");
const Io = std.Io;

const gpa = std.heap.smp_allocator;
const header_length = 13;
const input_block = 65536;
const op_set = 1;
const op_delete = 2;
const compact_from = 1_048_576;

const Sync = enum { always, batch, never };

const Map = std.StringHashMapUnmanaged([]u8);

const Store = struct {
    io: Io,
    map: Map = .empty,
    dir: Io.Dir,
    log_file: Io.File,
    log_buffer: []u8,
    log: Io.File.Writer,
    sync: Sync,
    /// The bytes of the log, and of the records of what is held.
    log_length: u64 = 0,
    live_length: u64 = 0,
    /// Records written since the log was last put on the disk.
    unsynced: bool = false,
};

/// The file's bytes on the disk, with what is needed to read them back: `fdatasync` on Linux,
/// `F_FULLFSYNC` on macOS (as Rust's `sync_data` and SG's `syncData`), `fsync` elsewhere.
fn syncData(io: Io, file: Io.File) !void {
    switch (builtin.os.tag) {
        .linux => try std.posix.fdatasync(file.handle),
        .macos, .ios, .tvos, .watchos, .visionos => {
            while (true) {
                const rc = std.c.fcntl(file.handle, std.c.F.FULLFSYNC);
                switch (std.posix.errno(rc)) {
                    .SUCCESS => return,
                    .INTR => continue,
                    .IO => return error.InputOutput,
                    .NOSPC => return error.NoSpaceLeft,
                    else => return error.SyncFailed,
                }
            }
        },
        else => try file.sync(io),
    }
}

fn recordLength(key: usize, value: usize) u64 {
    return header_length + key + value;
}

/// One record into the log.
fn writeRecord(w: *Io.Writer, op: u8, key: []const u8, value: []const u8) !void {
    var header: [header_length]u8 = undefined;
    header[4] = op;
    std.mem.writeInt(u32, header[5..9], @intCast(key.len), .little);
    std.mem.writeInt(u32, header[9..13], @intCast(value.len), .little);
    var crc = std.hash.Crc32.init();
    crc.update(header[4..]);
    crc.update(key);
    crc.update(value);
    std.mem.writeInt(u32, header[0..4], crc.final(), .little);
    try w.writeAll(&header);
    try w.writeAll(key);
    try w.writeAll(value);
}

fn fail(comptime format: []const u8, args: anytype) noreturn {
    std.debug.print("kv: " ++ format ++ "\n", args);
    std.process.exit(1);
}

/// A set into the map: the key is copied only when it is new.
fn put(store: *Store, key: []const u8, value: []const u8) !void {
    const copy = try gpa.dupe(u8, value);
    const slot = try store.map.getOrPut(gpa, key);
    if (slot.found_existing) {
        store.live_length -= recordLength(key.len, slot.value_ptr.len);
        gpa.free(slot.value_ptr.*);
    } else {
        slot.key_ptr.* = try gpa.dupe(u8, key);
    }
    slot.value_ptr.* = copy;
    store.live_length += recordLength(key.len, value.len);
}

/// Takes the key out; whether it was there.
fn remove(store: *Store, key: []const u8) bool {
    const entry = store.map.fetchRemove(key) orelse return false;
    store.live_length -= recordLength(key.len, entry.value.len);
    gpa.free(entry.key);
    gpa.free(entry.value);
    return true;
}

/// Reads the log into the map. A record cut off at the end is cut away; one whose CRC or op is wrong
/// with more after it stops the store.
fn recover(store: *Store) !void {
    const io = store.io;
    const file = store.dir.openFile(io, "log", .{}) catch |err| switch (err) {
        error.FileNotFound => return,
        else => |e| return e,
    };
    defer file.close(io);
    const size = try file.length(io);
    var buffer: [input_block]u8 = undefined;
    var file_reader = file.reader(io, &buffer);
    const reader = &file_reader.interface;
    var header: [header_length]u8 = undefined;
    var bytes: std.ArrayList(u8) = .empty;
    defer bytes.deinit(gpa);
    var at: u64 = 0;
    var cut: ?u64 = null;
    while (at < size) {
        if (try reader.readSliceShort(&header) < header_length) {
            cut = at;
            break;
        }
        const key_length: u64 = std.mem.readInt(u32, header[5..9], .little);
        const value_length: u64 = std.mem.readInt(u32, header[9..13], .little);
        const length = header_length + key_length + value_length;
        if (length > size - at) {
            cut = at;
            break;
        }
        const body: usize = @intCast(key_length + value_length);
        try bytes.resize(gpa, body);
        _ = try reader.readSliceShort(bytes.items[0..body]);
        var crc = std.hash.Crc32.init();
        crc.update(header[4..]);
        crc.update(bytes.items[0..body]);
        const right = crc.final() == std.mem.readInt(u32, header[0..4], .little);
        const op = header[4];
        const known = op == op_set or (op == op_delete and value_length == 0);
        if (!right or !known) {
            // The last record of the log, written only in part before a crash.
            if (at + length == size and !right) {
                cut = at;
                break;
            }
            fail("log: a damaged record at {d}", .{at});
        }
        const k: usize = @intCast(key_length);
        if (op == op_set) {
            try put(store, bytes.items[0..k], bytes.items[k..body]);
        } else {
            _ = remove(store, bytes.items[0..k]);
        }
        at += length;
    }
    store.log_length = at;
    if (cut) |from| {
        std.debug.print("kv: log: a record cut off at {d}, {d} bytes cut\n", .{ from, size - from });
        const log = try store.dir.openFile(io, "log", .{ .mode = .write_only });
        defer log.close(io);
        try log.setLength(io, from);
        if (store.sync != .never) try syncData(io, log);
    }
}

/// The log, open to write after what it holds.
fn openLog(store: *Store) !void {
    const io = store.io;
    store.log_file = try store.dir.createFile(io, "log", .{ .truncate = false });
    store.log = store.log_file.writer(io, store.log_buffer);
    try store.log.seekTo(store.log_length);
}

const Entry = struct { key: []const u8, value: []const u8 };

fn keyLess(_: void, a: Entry, b: Entry) bool {
    return std.mem.order(u8, a.key, b.key) == .lt;
}

/// The log written again with only what the store holds, in the order of the keys' bytes.
fn compact(store: *Store) !void {
    const io = store.io;
    try store.log.interface.flush();
    store.log_file.close(io);
    var entries: std.ArrayList(Entry) = try .initCapacity(gpa, store.map.count());
    defer entries.deinit(gpa);
    var it = store.map.iterator();
    while (it.next()) |entry| entries.appendAssumeCapacity(.{ .key = entry.key_ptr.*, .value = entry.value_ptr.* });
    std.mem.sort(Entry, entries.items, {}, keyLess);
    const fresh = try store.dir.createFile(io, "log.new", .{});
    var buffer: [input_block]u8 = undefined;
    var fresh_writer = fresh.writer(io, &buffer);
    var length: u64 = 0;
    for (entries.items) |entry| {
        try writeRecord(&fresh_writer.interface, op_set, entry.key, entry.value);
        length += recordLength(entry.key.len, entry.value.len);
    }
    try fresh_writer.interface.flush();
    if (store.sync != .never) try syncData(io, fresh);
    fresh.close(io);
    try store.dir.rename("log.new", store.dir, "log", io);
    if (store.sync != .never) try syncData(io, .{ .handle = store.dir.handle, .flags = .{ .nonblocking = false } });
    store.log_length = length;
    store.live_length = length;
    try openLog(store);
}

/// A record written; after `always`, on the disk.
fn logged(store: *Store, op: u8, key: []const u8, value: []const u8) !void {
    try writeRecord(&store.log.interface, op, key, value);
    store.log_length += recordLength(key.len, value.len);
    if (store.sync == .always) {
        try store.log.interface.flush();
        try syncData(store.io, store.log_file);
    } else {
        store.unsynced = true;
    }
}

fn afterChange(store: *Store) !void {
    if (store.log_length > compact_from and store.log_length > 2 * store.live_length) try compact(store);
}

/// What one line asks, its answer put into `out`; false for a line that is not a command.
fn run(store: *Store, line: []const u8, out: *std.ArrayList(u8)) !bool {
    const s = std.mem.indexOfScalar(u8, line, ' ') orelse {
        if (std.mem.eql(u8, line, "COMPACT")) {
            try compact(store);
            try out.appendSlice(gpa, "OK\n");
            return true;
        }
        return false;
    };
    const word = line[0..s];
    const rest = line[s + 1 ..];
    if (std.mem.eql(u8, word, "SET")) {
        const k = std.mem.indexOfScalar(u8, rest, ' ') orelse return false;
        const key = rest[0..k];
        const value = rest[k + 1 ..];
        try logged(store, op_set, key, value);
        try put(store, key, value);
        try out.appendSlice(gpa, "OK\n");
        try afterChange(store);
    } else if (std.mem.eql(u8, word, "GET")) {
        if (store.map.get(rest)) |value| {
            try out.appendSlice(gpa, value);
            try out.append(gpa, '\n');
        } else {
            try out.appendSlice(gpa, "(nil)\n");
        }
    } else if (std.mem.eql(u8, word, "DEL")) {
        if (!store.map.contains(rest)) {
            try out.appendSlice(gpa, "0\n");
            return true;
        }
        try logged(store, op_delete, rest, "");
        _ = remove(store, rest);
        try out.appendSlice(gpa, "1\n");
        try afterChange(store);
    } else {
        return false;
    }
    return true;
}

/// What a block asked is answered once its records are where `--sync` says.
fn answered(store: *Store, out: *std.ArrayList(u8), stdout: *Io.Writer) !void {
    if (store.unsynced) {
        try store.log.interface.flush();
        if (store.sync == .batch) try syncData(store.io, store.log_file);
        store.unsynced = false;
    }
    try stdout.writeAll(out.items);
    try stdout.flush();
    out.clearRetainingCapacity();
}

fn usage() noreturn {
    std.debug.print("usage: kv [--sync always|batch|never] DIR\n", .{});
    std.process.exit(2);
}

fn serve(io: Io, sync: Sync, folder: []const u8) !u8 {
    const cwd = Io.Dir.cwd();
    const dir = cwd.openDir(io, folder, .{}) catch |err| switch (err) {
        error.FileNotFound => blk: {
            try cwd.createDir(io, folder, .default_dir);
            break :blk try cwd.openDir(io, folder, .{});
        },
        else => |e| return e,
    };
    dir.deleteFile(io, "log.new") catch |err| switch (err) {
        error.FileNotFound => {},
        else => |e| return e,
    };
    const made = blk: {
        dir.access(io, "log", .{}) catch break :blk true;
        break :blk false;
    };
    var store: Store = .{
        .io = io,
        .dir = dir,
        .log_file = undefined,
        .log_buffer = try gpa.alloc(u8, input_block),
        .log = undefined,
        .sync = sync,
    };
    try recover(&store);
    try openLog(&store);
    if (made and sync != .never) try syncData(io, .{ .handle = dir.handle, .flags = .{ .nonblocking = false } });

    var out_buffer: [input_block]u8 = undefined;
    var stdout_writer = Io.File.stdout().writerStreaming(io, &out_buffer);
    const stdout = &stdout_writer.interface;
    var block: [input_block]u8 = undefined;
    // A line that goes on past the block it started in.
    var carried: std.ArrayList(u8) = .empty;
    var out: std.ArrayList(u8) = .empty;
    var line_number: u64 = 0;
    var bad = false;
    const stdin = Io.File.stdin();
    while (true) {
        const got = stdin.readStreaming(io, &.{&block}) catch |err| switch (err) {
            error.EndOfStream => break,
            else => |e| return e,
        };
        if (got == 0) break;
        var start: usize = 0;
        while (start < got) {
            const end = std.mem.indexOfScalar(u8, block[start..got], '\n') orelse {
                try carried.appendSlice(gpa, block[start..got]);
                break;
            };
            const e = start + end;
            line_number += 1;
            const ok = if (carried.items.len == 0)
                try run(&store, block[start..e], &out)
            else blk: {
                try carried.appendSlice(gpa, block[start..e]);
                const ok = try run(&store, carried.items, &out);
                carried.clearRetainingCapacity();
                break :blk ok;
            };
            if (!ok) {
                std.debug.print("kv: line {d}: not a command\n", .{line_number});
                bad = true;
            }
            // Each answer as soon as its record is on the disk.
            if (store.sync == .always) try answered(&store, &out, stdout);
            start = e + 1;
        }
        try answered(&store, &out, stdout);
    }
    // The last line, without its `\n`.
    if (carried.items.len > 0) {
        line_number += 1;
        if (!try run(&store, carried.items, &out)) {
            std.debug.print("kv: line {d}: not a command\n", .{line_number});
            bad = true;
        }
    }
    try answered(&store, &out, stdout);
    try store.log.interface.flush();
    store.log_file.close(io);
    return if (bad) 1 else 0;
}

pub fn main(init: std.process.Init) !u8 {
    // A closed pipe ends the program at once, killed by SIGPIPE as a C program is; Zig's `Io` would
    // otherwise have the write fail.
    std.posix.sigaction(.PIPE, &.{
        .handler = .{ .handler = std.posix.SIG.DFL },
        .mask = std.posix.sigemptyset(),
        .flags = 0,
    }, null);
    const args = try init.minimal.args.toSlice(init.arena.allocator());
    var sync: Sync = .batch;
    var folder: ?[]const u8 = null;
    var i: usize = 1;
    while (i < args.len) : (i += 1) {
        const arg = args[i];
        if (std.mem.eql(u8, arg, "--sync") and i + 1 < args.len) {
            i += 1;
            sync = std.meta.stringToEnum(Sync, args[i]) orelse usage();
        } else if (folder == null and !std.mem.startsWith(u8, arg, "-")) {
            folder = arg;
        } else {
            usage();
        }
    }
    return serve(init.io, sync, folder orelse usage()) catch |err| fail("{s}", .{@errorName(err)});
}
