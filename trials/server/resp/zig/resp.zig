//! The store of the server lane in Zig (choices 1701, 1702), as `../c/resp.c` describes it: a part
//! of RESP2 (PING, GET, SET, DEL) over 64 maps, each behind a lock of its own; each connection reads
//! into a buffer of 4 KB, which grows when one command does not fit, answers every whole command the
//! buffer holds and sends the answers together; a command of more than 1,024 parts or a part of more
//! than 1 MB closes the connection.
//!
//!     resp PORT
//!
//! The `Io` is the one `main` is given, `Io.Threaded`, and each connection a task of an `Io.Group`
//! started with `concurrent`, which is a thread: Zig 0.17.1's `Io.Evented` (io_uring on Linux) has
//! no `netListenIp` and no `netAccept` yet, so it cannot serve. The maps are Zig's
//! `StringHashMapUnmanaged`, which hashes with Wyhash and no key drawn at the start; the shard is
//! chosen by the FNV-1a of the key, as in every version. The sockets are given TCP_NODELAY, as a
//! server is.

const std = @import("std");
const Io = std.Io;
const net = Io.net;
const Allocator = std.mem.Allocator;

const shard_count = 64;
const first_buffer = 4096;
const most_parts = 1024;
const most_part = 1 << 20;

const Shard = struct {
    mutex: Io.Mutex = .init,
    map: std.StringHashMapUnmanaged([]u8) = .empty,
};

const Store = struct {
    shards: [shard_count]Shard = @splat(.{}),
    gpa: Allocator,
};

fn shardOf(key: []const u8) usize {
    var h: u64 = 14695981039346656037;
    for (key) |b| h = (h ^ b) *% 1099511628211;
    return @intCast(h & (shard_count - 1));
}

fn putNumber(out: *std.ArrayList(u8), gpa: Allocator, kind: u8, n: i64) !void {
    var t: [32]u8 = undefined;
    const text = std.fmt.bufPrint(&t, "{c}{d}\r\n", .{ kind, n }) catch unreachable;
    try out.appendSlice(gpa, text);
}

const Line = union(enum) { whole: struct { value: i64, next: usize }, incomplete, wrong };

fn readNumber(bytes: []const u8, at: usize) Line {
    var i = at;
    var minus = false;
    if (i < bytes.len and bytes[i] == '-') {
        minus = true;
        i += 1;
    }
    const digits = i;
    var value: i64 = 0;
    while (i < bytes.len and bytes[i] >= '0' and bytes[i] <= '9') : (i += 1) {
        if (value > 1000000000) return .wrong;
        value = value * 10 + (bytes[i] - '0');
    }
    if (i + 1 >= bytes.len) return if (bytes.len - at > 24) .wrong else .incomplete;
    if (i == digits or bytes[i] != '\r' or bytes[i + 1] != '\n') return .wrong;
    return .{ .whole = .{ .value = if (minus) -value else value, .next = i + 2 } };
}

fn isWord(part: []const u8, word: []const u8) bool {
    if (part.len != word.len) return false;
    for (part, word) |a, b| if ((a | 0x20) != b) return false;
    return true;
}

fn run(io: Io, store: *Store, parts: []const []const u8, out: *std.ArrayList(u8)) !void {
    const gpa = store.gpa;
    const name = parts[0];
    if (isWord(name, "ping") and parts.len == 1) {
        try out.appendSlice(gpa, "+PONG\r\n");
    } else if (isWord(name, "get") and parts.len == 2) {
        const shard = &store.shards[shardOf(parts[1])];
        shard.mutex.lockUncancelable(io);
        defer shard.mutex.unlock(io);
        if (shard.map.get(parts[1])) |value| {
            try putNumber(out, gpa, '$', @intCast(value.len));
            try out.appendSlice(gpa, value);
            try out.appendSlice(gpa, "\r\n");
        } else {
            try out.appendSlice(gpa, "$-1\r\n");
        }
    } else if (isWord(name, "set") and parts.len == 3) {
        const shard = &store.shards[shardOf(parts[1])];
        shard.mutex.lockUncancelable(io);
        defer shard.mutex.unlock(io);
        const slot = try shard.map.getOrPut(gpa, parts[1]);
        if (slot.found_existing) {
            if (slot.value_ptr.len != parts[2].len) {
                slot.value_ptr.* = try gpa.realloc(slot.value_ptr.*, parts[2].len);
            }
        } else {
            slot.key_ptr.* = try gpa.dupe(u8, parts[1]);
            slot.value_ptr.* = try gpa.alloc(u8, parts[2].len);
        }
        @memcpy(slot.value_ptr.*, parts[2]);
        try out.appendSlice(gpa, "+OK\r\n");
    } else if (isWord(name, "del") and parts.len >= 2) {
        var gone: i64 = 0;
        for (parts[1..]) |key| {
            const shard = &store.shards[shardOf(key)];
            shard.mutex.lockUncancelable(io);
            defer shard.mutex.unlock(io);
            if (shard.map.fetchRemove(key)) |entry| {
                gpa.free(entry.key);
                gpa.free(entry.value);
                gone += 1;
            }
        }
        try putNumber(out, gpa, ':', gone);
    } else {
        try out.appendSlice(gpa, "-ERR unknown command or wrong number of arguments\r\n");
    }
}

/// Answers every whole command in `bytes`; how many bytes it used, or null when the client broke the
/// protocol.
fn serveAll(io: Io, store: *Store, bytes: []const u8, parts: *[most_parts][]const u8, out: *std.ArrayList(u8)) !?usize {
    var used: usize = 0;
    while (used < bytes.len) {
        if (bytes[used] != '*') return null;
        var count: usize = 0;
        var at: usize = 0;
        switch (readNumber(bytes, used + 1)) {
            .whole => |w| {
                if (w.value < 1 or w.value > most_parts) return null;
                count = @intCast(w.value);
                at = w.next;
            },
            .incomplete => return used,
            .wrong => return null,
        }
        for (0..count) |i| {
            if (at >= bytes.len) return used;
            if (bytes[at] != '$') return null;
            switch (readNumber(bytes, at + 1)) {
                .whole => |w| {
                    if (w.value < 0 or w.value > most_part) return null;
                    const n: usize = @intCast(w.value);
                    if (bytes.len - w.next < n + 2) return used;
                    const end = w.next + n;
                    if (bytes[end] != '\r' or bytes[end + 1] != '\n') return null;
                    parts[i] = bytes[w.next..end];
                    at = end + 2;
                },
                .incomplete => return used,
                .wrong => return null,
            }
        }
        try run(io, store, parts[0..count], out);
        used = at;
    }
    return used;
}

fn serveConnection(io: Io, store: *Store, conn: net.Stream) !void {
    const gpa = store.gpa;
    var buffer = try gpa.alloc(u8, first_buffer);
    defer gpa.free(buffer);
    var length: usize = 0;
    var out: std.ArrayList(u8) = .empty;
    defer out.deinit(gpa);
    const parts = try gpa.create([most_parts][]const u8);
    defer gpa.destroy(parts);
    var writer = conn.writer(io, &.{});
    while (true) {
        if (length == buffer.len) buffer = try gpa.realloc(buffer, buffer.len * 2);
        var into = [1][]u8{buffer[length..]};
        const n = try conn.read(io, &into);
        if (n == 0) return;
        length += n;
        out.clearRetainingCapacity();
        const used = try serveAll(io, store, buffer[0..length], parts, &out) orelse {
            try out.appendSlice(gpa, "-ERR Protocol error\r\n");
            try writer.interface.writeAll(out.items);
            return;
        };
        if (out.items.len > 0) try writer.interface.writeAll(out.items);
        std.mem.copyForwards(u8, buffer[0 .. length - used], buffer[used..length]);
        length -= used;
    }
}

/// A connection that fails ends alone.
fn connection(io: Io, store: *Store, conn: net.Stream) Io.Cancelable!void {
    defer conn.close(io);
    serveConnection(io, store, conn) catch |err| switch (err) {
        error.Canceled => return error.Canceled,
        else => {},
    };
}

pub fn main(init: std.process.Init) !u8 {
    const io = init.io;
    const arena = init.arena.allocator();
    const args = try init.minimal.args.toSlice(arena);
    const port = if (args.len == 2) std.fmt.parseInt(u16, args[1], 10) catch null else null;
    if (port == null) {
        std.debug.print("usage: resp PORT\n", .{});
        return 2;
    }
    const store = try init.gpa.create(Store);
    store.* = .{ .gpa = init.gpa };
    var address = try net.IpAddress.parseLiteral("0.0.0.0:0");
    address.setPort(port.?);
    var server = try address.listen(io, .{ .reuse_address = true, .kernel_backlog = 4096 });
    defer server.deinit(io);
    var group: Io.Group = .init;
    defer group.cancel(io);
    const one: c_int = 1;
    while (true) {
        const conn = server.accept(io) catch |err| switch (err) {
            error.Canceled => return err,
            else => {
                // too many open files, say: try again soon
                io.sleep(.fromMilliseconds(10), .awake) catch {};
                continue;
            },
        };
        std.posix.setsockopt(conn.socket.handle, std.posix.IPPROTO.TCP, std.posix.TCP.NODELAY, std.mem.asBytes(&one)) catch {};
        group.concurrent(io, connection, .{ io, store, conn }) catch {
            conn.close(io);
        };
    }
}
