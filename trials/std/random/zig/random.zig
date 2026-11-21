//! `random`: `std`'s random numbers measured against Zig's `std.Random` (choices 1553, 1629-1637,
//! 1667, 1669). Zig's generator is `std.Random.DefaultPrng`, xoshiro256++ seeded with SplitMix64:
//! not SG's xoshiro256**, so the numbers are not the same. Four ways of using them:
//!
//!     random numbers COUNT    COUNT numbers of 64 bits from the seed 1
//!     random range COUNT      COUNT numbers from 0 up to 1000, without 1000, counted in a histogram
//!     random fill MEGABYTES   MEGABYTES megabytes of random bytes, 64 KB at a time into one buffer
//!     random tokens COUNT     COUNT tokens of 64 hex digits (32 bytes) from `Io.random`, Zig's
//!                             generator for keys, seeded by the system and kept in the process
//!
//! Nothing printed depends on the numbers drawn, only on whether they look random: how many were
//! drawn, and whether the bits set (`numbers`), each bar of the histogram (`range`) and the mean of
//! every 64th byte (`fill`) lie within six standard deviations of what they should be; `tokens`
//! prints how many characters the tokens had and whether all were hex digits. A wrong use is told
//! on the standard error, with exit code 2.

const std = @import("std");
const Io = std.Io;

const usage = "usage: random numbers COUNT | range COUNT | fill MEGABYTES | tokens COUNT\n";

fn numbers(count: i64, out: *Io.Writer) !void {
    var prng = std.Random.DefaultPrng.init(1);
    const random = prng.random();
    var ones: i64 = 0;
    var i: i64 = 0;
    while (i < count) : (i += 1) ones += @popCount(random.int(u64));
    const off = ones - 32 * count;
    try out.print("numbers {d}\nbits near half {}\n", .{ count, off * off <= 36 * 16 * count });
}

fn range(count: i64, out: *Io.Writer) !void {
    var prng = std.Random.DefaultPrng.init(1);
    const random = prng.random();
    var bars: [1000]i64 = @splat(0);
    var i: i64 = 0;
    while (i < count) : (i += 1) bars[random.uintLessThan(u32, 1000)] += 1;
    const expected = @divTrunc(count, 1000);
    var near = true;
    for (bars) |bar| {
        if ((bar - expected) * (bar - expected) > 36 * expected + 36) near = false;
    }
    try out.print("numbers {d}\nbars 1000\nevery bar near {}\n", .{ count, near });
}

fn fill(megabytes: i64, out: *Io.Writer) !void {
    var prng = std.Random.DefaultPrng.init(1);
    const random = prng.random();
    var buffer: [65536]u8 = undefined;
    const rounds = megabytes * 16;
    var sum: i64 = 0;
    var seen: i64 = 0;
    var i: i64 = 0;
    while (i < rounds) : (i += 1) {
        random.bytes(&buffer);
        var at: usize = 0;
        while (at < buffer.len) : (at += 64) sum += buffer[at];
        seen += 1024;
    }
    const off = 2 * sum - 255 * seen;
    try out.print("bytes {d}\nmean near {}\n", .{ rounds * 65536, off * off <= 786420 * seen });
}

fn tokens(io: Io, count: i64, out: *Io.Writer) !void {
    var bytes: [32]u8 = undefined;
    var characters: i64 = 0;
    var hex = true;
    var i: i64 = 0;
    while (i < count) : (i += 1) {
        io.random(&bytes);
        const text = std.fmt.bytesToHex(bytes, .lower);
        characters += text.len;
        for (text) |ch| {
            if (!std.ascii.isDigit(ch) and !(ch >= 'a' and ch <= 'f')) hex = false;
        }
    }
    try out.print("tokens {d}\ncharacters {d}\nhex {}\n", .{ count, characters, hex });
}

pub fn main(init: std.process.Init) !u8 {
    const io = init.io;
    const arena = init.arena.allocator();
    const args = try init.minimal.args.toSlice(arena);
    const names = [_][]const u8{ "numbers", "range", "fill", "tokens" };
    var known = false;
    if (args.len == 3) {
        for (names) |name| known = known or std.mem.eql(u8, args[1], name);
    }
    const count: i64 = if (known) std.fmt.parseInt(i64, args[2], 10) catch -1 else -1;
    if (count < 0) {
        std.debug.print(usage, .{});
        return 2;
    }
    var out_buffer: [4096]u8 = undefined;
    var out_file: Io.File.Writer = .init(.stdout(), io, &out_buffer);
    const out = &out_file.interface;
    if (std.mem.eql(u8, args[1], "numbers")) {
        try numbers(count, out);
    } else if (std.mem.eql(u8, args[1], "range")) {
        try range(count, out);
    } else if (std.mem.eql(u8, args[1], "fill")) {
        try fill(count, out);
    } else {
        try tokens(io, count, out);
    }
    try out.flush();
    return 0;
}
