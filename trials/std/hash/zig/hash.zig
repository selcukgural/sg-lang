//! `hash`: `std`'s SHA-256 and HMAC-SHA-256 measured against Zig's `std.crypto` (choices 1553,
//! 1629-1637, 1653-1654). Three ways of using them:
//!
//!     hash sha256 FILE    the SHA-256 of FILE, read a piece of 64 KB at a time, as `sha256sum` does
//!     hash chain COUNT    32 zero bytes hashed COUNT times, each time the hash of the one before
//!     hash hmac FILE      each line of FILE, without its end (`\n` or `\r\n`), signed with
//!                         HMAC-SHA-256 under one key of 32 bytes; how many lines, and every tag
//!                         put together with XOR
//!
//! The hashes are printed in hexadecimal. A file that cannot be read is told on the standard error,
//! with exit code 1; a wrong use, with 2.

const std = @import("std");
const Io = std.Io;
const Sha256 = std.crypto.hash.sha2.Sha256;
const HmacSha256 = std.crypto.auth.hmac.sha2.HmacSha256;

fn fail(name: []const u8, err: anyerror) u8 {
    std.debug.print("hash: {s}: {s}\n", .{ name, @errorName(err) });
    return 1;
}

fn sha256(io: Io, path: []const u8, out: *Io.Writer) !void {
    const file = try Io.Dir.cwd().openFile(io, path, .{});
    defer file.close(io);
    var buffer: [65536]u8 = undefined;
    var h = Sha256.init(.{});
    while (true) {
        const count = file.readStreaming(io, &.{&buffer}) catch |err| switch (err) {
            error.EndOfStream => break,
            else => return err,
        };
        h.update(buffer[0..count]);
    }
    try out.print("{x}\n", .{&h.finalResult()});
}

fn chain(count: u64, out: *Io.Writer) !void {
    var h: [32]u8 = @splat(0);
    var i: u64 = 0;
    while (i < count) : (i += 1) {
        Sha256.hash(&h, &h, .{});
    }
    try out.print("{x}\n", .{&h});
}

fn hmac(io: Io, path: []const u8, out: *Io.Writer) !void {
    var key: [32]u8 = undefined;
    for (&key, 0..) |*k, i| k.* = @intCast(i);
    const file = try Io.Dir.cwd().openFile(io, path, .{});
    defer file.close(io);
    var in_buffer: [65536]u8 = undefined;
    var file_reader = file.readerStreaming(io, &in_buffer);
    const r = &file_reader.interface;
    var lines: u64 = 0;
    var all: [32]u8 = @splat(0);
    while (try r.takeDelimiter('\n')) |taken| {
        const line = if (taken.len > 0 and taken[taken.len - 1] == '\r') taken[0 .. taken.len - 1] else taken;
        var tag: [HmacSha256.mac_length]u8 = undefined;
        HmacSha256.create(&tag, line, &key);
        for (&all, tag) |*a, t| a.* ^= t;
        lines += 1;
    }
    try out.print("lines {d}\nxor {x}\n", .{ lines, &all });
}

pub fn main(init: std.process.Init) !u8 {
    const io = init.io;
    const arena = init.arena.allocator();
    const args = try init.minimal.args.toSlice(arena);
    var out_buffer: [4096]u8 = undefined;
    var out_file: Io.File.Writer = .init(.stdout(), io, &out_buffer);
    const out = &out_file.interface;
    if (args.len == 3 and std.mem.eql(u8, args[1], "chain")) {
        if (std.fmt.parseInt(u64, args[2], 10)) |count| {
            try chain(count, out);
            try out.flush();
            return 0;
        } else |_| {}
    }
    if (args.len != 3 or (!std.mem.eql(u8, args[1], "sha256") and !std.mem.eql(u8, args[1], "hmac"))) {
        std.debug.print("usage: hash sha256 FILE | chain COUNT | hmac FILE\n", .{});
        return 2;
    }
    const name = args[2];
    if (std.mem.eql(u8, args[1], "sha256")) {
        sha256(io, name, out) catch |err| return fail(name, err);
    } else {
        hmac(io, name, out) catch |err| return fail(name, err);
    }
    try out.flush();
    return 0;
}
