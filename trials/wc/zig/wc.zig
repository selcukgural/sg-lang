//! `wc`: the lines, words and bytes of each file named, and their total when there are several; with
//! no file, those of the input. A word is a run of bytes that are not ASCII white space (space, tab,
//! line feed, vertical tab, form feed, carriage return), so a file need not be text. A file that
//! cannot be opened or read is told on the standard error, the others are still counted, and the
//! program ends with exit code 1.

const std = @import("std");
const Io = std.Io;

const Counts = struct {
    lines: u64 = 0,
    words: u64 = 0,
    bytes: u64 = 0,
};

/// The counts of what `file` gives, read in pieces of 64 KB.
fn count(io: Io, file: Io.File) !Counts {
    var buffer: [65536]u8 = undefined;
    var c: Counts = .{};
    var in_word = false;
    while (true) {
        const got = file.readStreaming(io, &.{&buffer}) catch |err| switch (err) {
            error.EndOfStream => break,
            else => |e| return e,
        };
        c.bytes += got;
        for (buffer[0..got]) |b| {
            if (b == '\n') c.lines += 1;
            const space = b == ' ' or (b >= '\t' and b <= '\r');
            if (!space and !in_word) c.words += 1;
            in_word = !space;
        }
    }
    return c;
}

pub fn main(init: std.process.Init) !u8 {
    const io = init.io;
    const args = try init.minimal.args.toSlice(init.arena.allocator());
    var out_buffer: [65536]u8 = undefined;
    var out_file: Io.File.Writer = .init(.stdout(), io, &out_buffer);
    const out = &out_file.interface;
    defer out.flush() catch {};
    if (args.len < 2) {
        const c = count(io, .stdin()) catch |err| {
            std.debug.print("wc: {s}\n", .{@errorName(err)});
            return 1;
        };
        try out.print("{d} {d} {d}\n", .{ c.lines, c.words, c.bytes });
        return 0;
    }
    var total: Counts = .{};
    var code: u8 = 0;
    for (args[1..]) |name| {
        const c = blk: {
            const file = Io.Dir.cwd().openFile(io, name, .{}) catch |err| break :blk err;
            defer file.close(io);
            break :blk count(io, file);
        } catch |err| {
            try out.flush();
            std.debug.print("wc: {s}: {s}\n", .{ name, @errorName(err) });
            code = 1;
            continue;
        };
        try out.print("{d} {d} {d} {s}\n", .{ c.lines, c.words, c.bytes, name });
        total.lines += c.lines;
        total.words += c.words;
        total.bytes += c.bytes;
    }
    if (args.len > 2) try out.print("{d} {d} {d} total\n", .{ total.lines, total.words, total.bytes });
    return code;
}
