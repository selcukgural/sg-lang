//! `log`: `std`'s log measured against Zig's `std.log` (choices 1553, 1629-1637, 1667, 1670).
//! Three ways of using it:
//!
//!     log lines COUNT    COUNT lines at the level Info, `request 12 took 84 ms`
//!     log off COUNT      COUNT lines at the level Debug while the log writes from Info: none is
//!                        written, and the text of none is made
//!     log tasks COUNT    four threads at once, each writing a quarter of COUNT lines at the level
//!                        Info, `task 2 request 12 took 84 ms`
//!
//! The lines go to the standard error, each `2026-11-21T20:15:03.123Z INFO  ` and the message, one
//! whole line from one call: `std.log`'s own lines have no time (`info: request 12 took 84 ms`), so
//! `logFn` is given one that writes SG's lines, under the lock of the standard error as `defaultLog`
//! writes, with the calendar written by hand (Zig's `std.time.epoch` has the days but no text).
//! The level is Zig's, fixed when the program is compiled: a line of a level that is off is not in
//! the program at all. Each prints on the standard output how many lines it wrote (`off`, how many
//! it did not, and the sum of the numbers it would have written). A wrong use is told on the
//! standard error, with exit code 2.

const std = @import("std");
const Io = std.Io;
const log = std.log;

const usage = "usage: log lines COUNT | off COUNT | tasks COUNT\n";

pub const std_options: std.Options = .{
    .log_level = .info,
    .logFn = logLine,
};

/// The year, month and day of a count of days since 1970 (Howard Hinnant's `civil_from_days`).
fn civilFromDays(z0: i64) struct { year: i64, month: u32, day: u32 } {
    const z = z0 + 719468;
    const era = @divFloor(z, 146097);
    const doe = z - era * 146097;
    const yoe = @divFloor(doe - @divFloor(doe, 1460) + @divFloor(doe, 36524) - @divFloor(doe, 146096), 365);
    const doy = doe - (365 * yoe + @divFloor(yoe, 4) - @divFloor(yoe, 100));
    const mp = @divFloor(5 * doy + 2, 153);
    const day: u32 = @intCast(doy - @divFloor(153 * mp + 2, 5) + 1);
    const month: u32 = @intCast(if (mp < 10) mp + 3 else mp - 9);
    return .{ .year = yoe + era * 400 + @intFromBool(month <= 2), .month = month, .day = day };
}

fn logLine(
    comptime level: log.Level,
    comptime scope: @EnumLiteral(),
    comptime format: []const u8,
    args: anytype,
) void {
    _ = scope;
    const io = std.Options.debug_io;
    const nanoseconds: i64 = @intCast(Io.Clock.real.now(io).nanoseconds);
    const milliseconds = @divFloor(nanoseconds, 1_000_000);
    const seconds = @divFloor(milliseconds, 1000);
    const date = civilFromDays(@divFloor(seconds, 86400));
    const time: u64 = @intCast(@mod(seconds, 86400));
    const name = switch (level) {
        .err => "ERROR",
        .warn => "WARN ",
        .info => "INFO ",
        .debug => "DEBUG",
    };
    // The whole line in the buffer, written at once.
    var buffer: [512]u8 = undefined;
    const stderr = std.debug.lockStderr(&buffer);
    defer std.debug.unlockStderr();
    const w = &stderr.file_writer.interface;
    w.print("{d:0>4}-{d:0>2}-{d:0>2}T{d:0>2}:{d:0>2}:{d:0>2}.{d:0>3}Z {s} " ++ format ++ "\n", .{
        @as(u64, @intCast(date.year)), date.month, date.day, time / 3600, time / 60 % 60, time % 60,
        @as(u64, @intCast(@mod(milliseconds, 1000))), name,
    } ++ args) catch {};
}

fn lines(count: i64, out: *Io.Writer) !void {
    var i: i64 = 0;
    while (i < count) : (i += 1) log.info("request {d} took {d} ms", .{ i, @mod(i * 7, 1000) });
    try out.print("lines {d}\n", .{count});
}

fn off(count: i64, out: *Io.Writer) !void {
    var sum: i64 = 0;
    var i: i64 = 0;
    while (i < count) : (i += 1) {
        log.debug("request {d} took {d} ms", .{ i, @mod(i * 7, 1000) });
        sum += @mod(i * 7, 1000);
    }
    try out.print("lines 0 of {d}\nsum {d}\n", .{ count, sum });
}

fn work(task: i64, count: i64) void {
    var i: i64 = 0;
    while (i < count) : (i += 1) log.info("task {d} request {d} took {d} ms", .{ task, i, @mod(i * 7, 1000) });
}

fn tasks(count: i64, out: *Io.Writer) !void {
    const shares = [4]i64{ @divTrunc(count, 4), @divTrunc(count, 4), @divTrunc(count, 4), count - @divTrunc(count, 4) * 3 };
    var threads: [4]std.Thread = undefined;
    for (&threads, shares, 0..) |*thread, share, task| thread.* = try std.Thread.spawn(.{}, work, .{ @as(i64, @intCast(task)), share });
    for (threads) |thread| thread.join();
    try out.print("lines {d}\n", .{shares[0] + shares[1] + shares[2] + shares[3]});
}

pub fn main(init: std.process.Init) !u8 {
    const io = init.io;
    const arena = init.arena.allocator();
    const args = try init.minimal.args.toSlice(arena);
    const names = [_][]const u8{ "lines", "off", "tasks" };
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
    if (std.mem.eql(u8, args[1], "lines")) {
        try lines(count, out);
    } else if (std.mem.eql(u8, args[1], "off")) {
        try off(count, out);
    } else {
        try tasks(count, out);
    }
    try out.flush();
    return 0;
}
