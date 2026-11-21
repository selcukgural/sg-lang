//! `checksum`: `std`'s CRC-32 and Adler-32 measured against Zig's `std.hash.Crc32` and
//! `std.hash.Adler32` (choices 1553, 1629-1637, 1653, 1655). Three ways of using them:
//!
//!     checksum crc32 FILE      the CRC-32 of FILE, read a piece of 64 KB at a time
//!     checksum adler32 FILE    the Adler-32 of FILE, read the same way
//!     checksum lines FILE      the CRC-32 of each line of FILE, without its end (`\n` or `\r\n`);
//!                              how many lines, and every CRC-32 put together with XOR
//!
//! The checksums are printed in hexadecimal. A file that cannot be read is told on the standard
//! error, with exit code 1; a wrong use, with 2.

const std = @import("std");
const Io = std.Io;
const Crc32 = std.hash.Crc32;
const Adler32 = std.hash.Adler32;

fn fail(name: []const u8, err: anyerror) u8 {
    std.debug.print("checksum: {s}: {s}\n", .{ name, @errorName(err) });
    return 1;
}

fn whole(io: Io, path: []const u8, crc: bool) !u32 {
    const file = try Io.Dir.cwd().openFile(io, path, .{});
    defer file.close(io);
    var buffer: [65536]u8 = undefined;
    var c = Crc32.init();
    var a: Adler32 = .{};
    while (true) {
        const count = file.readStreaming(io, &.{&buffer}) catch |err| switch (err) {
            error.EndOfStream => break,
            else => return err,
        };
        if (crc) c.update(buffer[0..count]) else a.update(buffer[0..count]);
    }
    return if (crc) c.final() else a.adler;
}

fn lines(io: Io, path: []const u8, out: *Io.Writer) !void {
    const file = try Io.Dir.cwd().openFile(io, path, .{});
    defer file.close(io);
    var in_buffer: [65536]u8 = undefined;
    var file_reader = file.readerStreaming(io, &in_buffer);
    const r = &file_reader.interface;
    var count: u64 = 0;
    var all: u32 = 0;
    while (try r.takeDelimiter('\n')) |taken| {
        const line = if (taken.len > 0 and taken[taken.len - 1] == '\r') taken[0 .. taken.len - 1] else taken;
        all ^= Crc32.hash(line);
        count += 1;
    }
    try out.print("lines {d}\nxor {x:0>8}\n", .{ count, all });
}

pub fn main(init: std.process.Init) !u8 {
    const io = init.io;
    const arena = init.arena.allocator();
    const args = try init.minimal.args.toSlice(arena);
    const known = args.len == 3 and (std.mem.eql(u8, args[1], "crc32") or
        std.mem.eql(u8, args[1], "adler32") or std.mem.eql(u8, args[1], "lines"));
    if (!known) {
        std.debug.print("usage: checksum crc32 FILE | adler32 FILE | lines FILE\n", .{});
        return 2;
    }
    var out_buffer: [4096]u8 = undefined;
    var out_file: Io.File.Writer = .init(.stdout(), io, &out_buffer);
    const out = &out_file.interface;
    const name = args[2];
    if (std.mem.eql(u8, args[1], "lines")) {
        lines(io, name, out) catch |err| return fail(name, err);
    } else {
        const sum = whole(io, name, std.mem.eql(u8, args[1], "crc32")) catch |err| return fail(name, err);
        try out.print("{x:0>8}\n", .{sum});
    }
    try out.flush();
    return 0;
}
