//! `compression`: `std`'s compression measured against Zig's `std.compress.flate` (choices 1553,
//! 1629-1637, 1642). Three ways of using it, all at level 6, zlib's default:
//!
//!     compression compress FILE OUT  FILE in gzip into OUT, read and written a piece at a time
//!     compression decompress FILE    the gzip FILE opened on the standard output, a piece at a time
//!     compression records FILE       each line of FILE, without its `\n`, compressed in zlib and
//!                                    opened again in memory
//!
//! What `compress` prints: the bytes it read. What `records` prints: how many lines, their bytes,
//! and how many came back the same; the bytes they took compressed go to the standard error, as
//! the size of what `compress` writes is not compared: each compressor finds its own matches. A
//! file that cannot be read or opened is told on the standard error, with exit code 1; a wrong use,
//! with 2.

const std = @import("std");
const Io = std.Io;
const flate = std.compress.flate;

fn fail(name: []const u8, err: anyerror) u8 {
    std.debug.print("compression: {s}: {s}\n", .{ name, @errorName(err) });
    return 1;
}

fn compress(io: Io, path: []const u8, out_path: []const u8) !u64 {
    const file = try Io.Dir.cwd().openFile(io, path, .{});
    defer file.close(io);
    var in_buffer: [65536]u8 = undefined;
    var file_reader = file.readerStreaming(io, &in_buffer);
    const out_file = try Io.Dir.cwd().createFile(io, out_path, .{});
    defer out_file.close(io);
    var out_buffer: [65536]u8 = undefined;
    var file_writer = out_file.writer(io, &out_buffer);
    var window: [flate.max_window_len]u8 = undefined;
    var compressor = try flate.Compress.init(&file_writer.interface, &window, .gzip, .level_6);
    const total = try file_reader.interface.streamRemaining(&compressor.writer);
    try compressor.finish();
    try file_writer.end();
    return total;
}

fn decompress(io: Io, path: []const u8, out: *Io.Writer) !void {
    const file = try Io.Dir.cwd().openFile(io, path, .{});
    defer file.close(io);
    var in_buffer: [65536]u8 = undefined;
    var file_reader = file.readerStreaming(io, &in_buffer);
    var window: [flate.max_window_len]u8 = undefined;
    var decompressor: flate.Decompress = .init(&file_reader.interface, .gzip, &window);
    _ = try decompressor.reader.streamRemaining(out);
}

fn records(io: Io, gpa: std.mem.Allocator, path: []const u8, out: *Io.Writer) !void {
    const file = try Io.Dir.cwd().openFile(io, path, .{});
    defer file.close(io);
    var in_buffer: [65536]u8 = undefined;
    var file_reader = file.readerStreaming(io, &in_buffer);
    const r = &file_reader.interface;
    var window: [flate.max_window_len]u8 = undefined;
    var open_window: [flate.max_window_len]u8 = undefined;
    var packed_bytes: Io.Writer.Allocating = try .initCapacity(gpa, 4096);
    defer packed_bytes.deinit();
    var opened: Io.Writer.Allocating = try .initCapacity(gpa, 4096);
    defer opened.deinit();
    var lines: u64 = 0;
    var bytes: u64 = 0;
    var same: u64 = 0;
    var compressed: u64 = 0;
    while (try r.takeDelimiter('\n')) |taken| {
        const line = if (taken.len > 0 and taken[taken.len - 1] == '\r') taken[0 .. taken.len - 1] else taken;
        packed_bytes.clearRetainingCapacity();
        var compressor = try flate.Compress.init(&packed_bytes.writer, &window, .zlib, .level_6);
        try compressor.writer.writeAll(line);
        try compressor.finish();
        const data = packed_bytes.written();
        opened.clearRetainingCapacity();
        var source: Io.Reader = .fixed(data);
        var decompressor: flate.Decompress = .init(&source, .zlib, &open_window);
        _ = try decompressor.reader.streamRemaining(&opened.writer);
        lines += 1;
        bytes += line.len;
        compressed += data.len;
        if (std.mem.eql(u8, opened.written(), line)) same += 1;
    }
    try out.print("records {d}\nbytes {d}\nsame {d}\n", .{ lines, bytes, same });
    std.debug.print("compressed {d}\n", .{compressed});
}

pub fn main(init: std.process.Init) !u8 {
    const io = init.io;
    const arena = init.arena.allocator();
    const args = try init.minimal.args.toSlice(arena);
    const usage = (args.len == 4 and std.mem.eql(u8, args[1], "compress")) or
        (args.len == 3 and (std.mem.eql(u8, args[1], "decompress") or std.mem.eql(u8, args[1], "records")));
    if (!usage) {
        std.debug.print("usage: compression compress FILE OUT | decompress FILE | records FILE\n", .{});
        return 2;
    }
    const mode = args[1];
    const name = args[2];
    var out_buffer: [65536]u8 = undefined;
    var out_file: Io.File.Writer = .init(.stdout(), io, &out_buffer);
    const out = &out_file.interface;
    if (std.mem.eql(u8, mode, "compress")) {
        const total = compress(io, name, args[3]) catch |err| return fail(name, err);
        try out.print("bytes {d}\n", .{total});
    } else if (std.mem.eql(u8, mode, "decompress")) {
        decompress(io, name, out) catch |err| return fail(name, err);
    } else {
        records(io, init.gpa, name, out) catch |err| return fail(name, err);
    }
    try out.flush();
    return 0;
}
