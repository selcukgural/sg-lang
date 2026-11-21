//! `encoding`: `std`'s hex and Base64 measured against Zig's `std.base64` and `std.fmt.bytesToHex`
//! and `hexToBytes` (choices 1553, 1629-1637, 1659, 1660). Four ways of using them:
//!
//!     encoding encode FILE OUT    FILE in Base64, the standard alphabet, padded, written into OUT;
//!                                 FILE is read 48 KB at a time (a multiple of three, so that the
//!                                 pieces written one after another are one text)
//!     encoding decode FILE OUT    FILE, a text in Base64, read back into bytes 64 KB of text at a
//!                                 time and written into OUT
//!     encoding hex COUNT          COUNT values of 32 bytes, each written as hex and read back
//!     encoding tokens COUNT       COUNT tokens of 16 to 48 bytes, each written in the Base64 of URLs
//!                                 and read back
//!
//! `encode` and `decode` print how many bytes they wrote. `hex` and `tokens` make their bytes with
//! SplitMix64 from the seed 1 and print how many they made, how many characters the texts had, a
//! sum of one character of each text and every last 8 bytes read back put together with XOR. A file
//! that cannot be read or written is told on the standard error, with exit code 1, as is a text
//! that is not Base64; a wrong use, with 2.

const std = @import("std");
const Io = std.Io;
const base64 = std.base64;

const usage = "usage: encoding encode FILE OUT | decode FILE OUT | hex COUNT | tokens COUNT\n";

const SplitMix = struct {
    state: u64,

    fn next(self: *SplitMix) u64 {
        self.state +%= 0x9E3779B97F4A7C15;
        var z = self.state;
        z = (z ^ (z >> 30)) *% 0xBF58476D1CE4E5B9;
        z = (z ^ (z >> 27)) *% 0x94D049BB133111EB;
        return z ^ (z >> 31);
    }

    /// `buffer` filled from as many values as it needs, each value's lowest byte first.
    fn fill(self: *SplitMix, buffer: []u8) void {
        var at: usize = 0;
        while (at < buffer.len) : (at += 8) {
            const z: [8]u8 = @bitCast(std.mem.nativeToLittle(u64, self.next()));
            const n = @min(8, buffer.len - at);
            @memcpy(buffer[at .. at + n], z[0..n]);
        }
    }
};

fn lastEight(bytes: []const u8) u64 {
    return std.mem.readInt(u64, bytes[bytes.len - 8 ..][0..8], .little);
}

fn fail(name: []const u8, err: anyerror) u8 {
    std.debug.print("encoding: {s}: {s}\n", .{ name, @errorName(err) });
    return 1;
}

/// Fills `buffer` unless the end comes first; how many bytes.
fn readFull(io: Io, file: Io.File, buffer: []u8) !usize {
    var count: usize = 0;
    while (count < buffer.len) {
        const n = file.readStreaming(io, &.{buffer[count..]}) catch |err| switch (err) {
            error.EndOfStream => break,
            else => return err,
        };
        count += n;
    }
    return count;
}

fn encode(io: Io, path: []const u8, out_path: []const u8) !u64 {
    const file = try Io.Dir.cwd().openFile(io, path, .{});
    defer file.close(io);
    const out_file = try Io.Dir.cwd().createFile(io, out_path, .{});
    defer out_file.close(io);
    var out_buffer: [65536]u8 = undefined;
    var file_writer = out_file.writer(io, &out_buffer);
    var buffer: [49152]u8 = undefined;
    var text: [65536]u8 = undefined;
    var written: u64 = 0;
    while (true) {
        const count = try readFull(io, file, &buffer);
        if (count == 0) break;
        const encoded = base64.standard.Encoder.encode(&text, buffer[0..count]);
        try file_writer.interface.writeAll(encoded);
        written += encoded.len;
    }
    try file_writer.end();
    return written;
}

fn decode(io: Io, path: []const u8, out_path: []const u8) !u64 {
    const file = try Io.Dir.cwd().openFile(io, path, .{});
    defer file.close(io);
    const out_file = try Io.Dir.cwd().createFile(io, out_path, .{});
    defer out_file.close(io);
    var out_buffer: [65536]u8 = undefined;
    var file_writer = out_file.writer(io, &out_buffer);
    var buffer: [65536]u8 = undefined;
    var bytes: [49152]u8 = undefined;
    var written: u64 = 0;
    while (true) {
        const count = try readFull(io, file, &buffer);
        if (count == 0) break;
        const decoder = base64.standard.Decoder;
        const length = try decoder.calcSizeForSlice(buffer[0..count]);
        try decoder.decode(bytes[0..length], buffer[0..count]);
        try file_writer.interface.writeAll(bytes[0..length]);
        written += length;
    }
    try file_writer.end();
    return written;
}

fn hex(count: u64, out: *Io.Writer) !void {
    var random: SplitMix = .{ .state = 1 };
    var value: [32]u8 = undefined;
    var back: [32]u8 = undefined;
    var digits: u64 = 0;
    var sum: u64 = 0;
    var all: u64 = 0;
    var i: u64 = 0;
    while (i < count) : (i += 1) {
        random.fill(&value);
        const text = std.fmt.bytesToHex(value, .lower);
        _ = std.fmt.hexToBytes(&back, &text) catch unreachable;
        digits += text.len;
        sum += text[@intCast(i % text.len)];
        all ^= lastEight(&back);
    }
    try out.print("values {d}\ndigits {d}\nsum {d}\nxor {x:0>16}\n", .{ count, digits, sum, all });
}

fn tokens(count: u64, out: *Io.Writer) !void {
    var random: SplitMix = .{ .state = 1 };
    var token: [48]u8 = undefined;
    var text: [64]u8 = undefined;
    var back: [48]u8 = undefined;
    var characters: u64 = 0;
    var sum: u64 = 0;
    var all: u64 = 0;
    const codec = base64.url_safe_no_pad;
    var i: u64 = 0;
    while (i < count) : (i += 1) {
        const length: usize = 16 + @as(usize, @intCast(random.next() % 33));
        random.fill(token[0..length]);
        const encoded = codec.Encoder.encode(&text, token[0..length]);
        const read = codec.Decoder.calcSizeForSlice(encoded) catch unreachable;
        codec.Decoder.decode(back[0..read], encoded) catch unreachable;
        characters += encoded.len;
        sum += encoded[@intCast(i % encoded.len)];
        all ^= lastEight(back[0..read]);
    }
    try out.print("tokens {d}\ncharacters {d}\nsum {d}\nxor {x:0>16}\n", .{ count, characters, sum, all });
}

pub fn main(init: std.process.Init) !u8 {
    const io = init.io;
    const arena = init.arena.allocator();
    const args = try init.minimal.args.toSlice(arena);
    const files = args.len == 4 and (std.mem.eql(u8, args[1], "encode") or std.mem.eql(u8, args[1], "decode"));
    const counts = args.len == 3 and (std.mem.eql(u8, args[1], "hex") or std.mem.eql(u8, args[1], "tokens"));
    if (!files and !counts) {
        std.debug.print(usage, .{});
        return 2;
    }
    var out_buffer: [4096]u8 = undefined;
    var out_file: Io.File.Writer = .init(.stdout(), io, &out_buffer);
    const out = &out_file.interface;
    if (counts) {
        const count = std.fmt.parseInt(u64, args[2], 10) catch {
            std.debug.print(usage, .{});
            return 2;
        };
        if (std.mem.eql(u8, args[1], "hex")) try hex(count, out) else try tokens(count, out);
    } else {
        const name = args[2];
        const written = if (std.mem.eql(u8, args[1], "encode"))
            encode(io, name, args[3]) catch |err| return fail(name, err)
        else
            decode(io, name, args[3]) catch |err| return fail(name, err);
        try out.print("bytes {d}\n", .{written});
    }
    try out.flush();
    return 0;
}
