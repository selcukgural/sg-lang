//! `time`: `std`'s calendar and clocks measured against Zig (choices 1553, 1629-1637, 1667, 1668).
//! Zig's standard library reads the clocks (`std.Io.Clock`) but has no calendar before 1970, no
//! writing or reading of ISO 8601 and no time zones: the calendar is written here by hand (the
//! days of Howard Hinnant's `civil_from_days` and `days_from_civil`), the text with `std.fmt`, and
//! the time zone is the C library's `localtime_r`. Four ways of using them:
//!
//!     time format COUNT    COUNT moments between 1900 and 2200, each written in ISO 8601 into a
//!                          text kept from one to the next; a quarter of them whole seconds, the
//!                          others with milliseconds, microseconds or nanoseconds
//!     time parse FILE      each line of FILE, a moment in RFC 3339 with an offset, read and taken
//!                          to UTC
//!     time local COUNT     COUNT moments between 1900 and 2200, each seen in the time zone of the
//!                          system (`TZ`)
//!     time clock COUNT     the monotonic clock read COUNT times
//!
//! The moments are made by SplitMix64 from the seed 1. `format` prints how many it wrote, how many
//! characters they had and a sum of one character of each; `parse` how many lines, the sum of their
//! seconds since 1970, of their nanoseconds and of their offsets in seconds; `local` the sum of the
//! offsets and of the year, month, day, hour and minute seen; `clock` how many times it read the
//! clock and whether it never went back. A file that cannot be read is told on the standard error,
//! with exit code 1, as is a line that is not a moment; a wrong use, with 2.

const std = @import("std");
const c = @import("c");
const Io = std.Io;

const usage = "usage: time format COUNT | parse FILE | local COUNT | clock COUNT\n";

const SplitMix = struct {
    state: u64,

    fn next(self: *SplitMix) u64 {
        self.state +%= 0x9E3779B97F4A7C15;
        var z = self.state;
        z = (z ^ (z >> 30)) *% 0xBF58476D1CE4E5B9;
        z = (z ^ (z >> 27)) *% 0x94D049BB133111EB;
        return z ^ (z >> 31);
    }
};

/// 1900-01-01 and 2200-01-01, in seconds since 1970.
const low: i64 = -2208988800;
const span: u64 = 9467107200;

const Moment = struct { seconds: i64, nanoseconds: u32 };

/// A moment between 1900 and 2200: whole seconds, or a fraction of milliseconds, microseconds or
/// nanoseconds, each as likely.
fn moment(random: *SplitMix) Moment {
    const seconds = low + @as(i64, @intCast(random.next() % span));
    const nanoseconds: u32 = switch (random.next() % 4) {
        1 => @as(u32, @intCast(random.next() % 1000)) * 1_000_000,
        2 => @as(u32, @intCast(random.next() % 1_000_000)) * 1000,
        3 => @intCast(random.next() % 1_000_000_000),
        else => 0,
    };
    return .{ .seconds = seconds, .nanoseconds = nanoseconds };
}

const Civil = struct { year: i64, month: u32, day: u32 };

fn civilFromDays(z0: i64) Civil {
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

fn daysFromCivil(year0: i64, month: u32, day: u32) i64 {
    const year = if (month <= 2) year0 - 1 else year0;
    const era = @divFloor(year, 400);
    const yoe = year - era * 400;
    const m: i64 = month;
    const doy = @divFloor(153 * (if (m > 2) m - 3 else m + 9) + 2, 5) + day - 1;
    const doe = yoe * 365 + @divFloor(yoe, 4) - @divFloor(yoe, 100) + doy;
    return era * 146097 + doe - 719468;
}

/// ISO 8601 in `buffer`: the fraction in 3, 6 or 9 digits when there is one.
fn writeIso(buffer: []u8, t: Moment) ![]u8 {
    const date = civilFromDays(@divFloor(t.seconds, 86400));
    const time: u32 = @intCast(@mod(t.seconds, 86400));
    var w: Io.Writer = .fixed(buffer);
    try w.print("{d:0>4}-{d:0>2}-{d:0>2}T{d:0>2}:{d:0>2}:{d:0>2}", .{
        @as(u64, @intCast(date.year)), date.month, date.day, time / 3600, time / 60 % 60, time % 60,
    });
    const n = t.nanoseconds;
    if (n == 0) {
        try w.writeByte('Z');
    } else if (n % 1_000_000 == 0) {
        try w.print(".{d:0>3}Z", .{n / 1_000_000});
    } else if (n % 1000 == 0) {
        try w.print(".{d:0>6}Z", .{n / 1000});
    } else {
        try w.print(".{d:0>9}Z", .{n});
    }
    return w.buffered();
}

fn format(count: u64, out: *Io.Writer) !void {
    var random: SplitMix = .{ .state = 1 };
    var buffer: [64]u8 = undefined;
    var characters: u64 = 0;
    var sum: u64 = 0;
    var i: u64 = 0;
    while (i < count) : (i += 1) {
        const text = try writeIso(&buffer, moment(&random));
        characters += text.len;
        sum += text[@intCast(i % text.len)];
    }
    try out.print("moments {d}\ncharacters {d}\nsum {d}\n", .{ count, characters, sum });
}

const NotAMoment = error{NotAMoment};

fn digits(text: []const u8, at: *usize, count: usize) NotAMoment!u32 {
    if (at.* + count > text.len) return error.NotAMoment;
    var value: u32 = 0;
    for (text[at.* .. at.* + count]) |ch| {
        if (ch < '0' or ch > '9') return error.NotAMoment;
        value = value * 10 + (ch - '0');
    }
    at.* += count;
    return value;
}

fn expect(text: []const u8, at: *usize, ch: u8) NotAMoment!void {
    if (at.* >= text.len or text[at.*] != ch) return error.NotAMoment;
    at.* += 1;
}

fn daysIn(year: u32, month: u32) u32 {
    return switch (month) {
        2 => if (year % 4 == 0 and (year % 100 != 0 or year % 400 == 0)) 29 else 28,
        4, 6, 9, 11 => 30,
        else => 31,
    };
}

const Parsed = struct { seconds: i64, nanoseconds: u32, offset: i64 };

/// RFC 3339: `YYYY-MM-DDTHH:MM:SS`, a fraction of 1 to 9 digits, then `Z` or `+hh:mm` / `-hh:mm`.
fn parseRfc3339(text: []const u8) NotAMoment!Parsed {
    var at: usize = 0;
    const year = try digits(text, &at, 4);
    try expect(text, &at, '-');
    const month = try digits(text, &at, 2);
    try expect(text, &at, '-');
    const day = try digits(text, &at, 2);
    try expect(text, &at, 'T');
    const hour = try digits(text, &at, 2);
    try expect(text, &at, ':');
    const minute = try digits(text, &at, 2);
    try expect(text, &at, ':');
    const second = try digits(text, &at, 2);
    var nanoseconds: u32 = 0;
    if (at < text.len and text[at] == '.') {
        at += 1;
        var count: usize = 0;
        while (at < text.len and text[at] >= '0' and text[at] <= '9') : (at += 1) {
            if (count == 9) return error.NotAMoment;
            nanoseconds = nanoseconds * 10 + (text[at] - '0');
            count += 1;
        }
        if (count == 0) return error.NotAMoment;
        while (count < 9) : (count += 1) nanoseconds *= 10;
    }
    var offset: i64 = 0;
    if (at < text.len and text[at] == 'Z') {
        at += 1;
    } else {
        if (at >= text.len or (text[at] != '+' and text[at] != '-')) return error.NotAMoment;
        const negative = text[at] == '-';
        at += 1;
        const hours = try digits(text, &at, 2);
        try expect(text, &at, ':');
        const minutes = try digits(text, &at, 2);
        if (hours > 23 or minutes > 59) return error.NotAMoment;
        offset = @as(i64, hours) * 3600 + @as(i64, minutes) * 60;
        if (negative) offset = -offset;
    }
    if (at != text.len) return error.NotAMoment;
    if (month < 1 or month > 12 or day < 1 or day > daysIn(year, month)) return error.NotAMoment;
    if (hour > 23 or minute > 59 or second > 59) return error.NotAMoment;
    const days = daysFromCivil(year, month, day);
    const seconds = days * 86400 + @as(i64, hour) * 3600 + @as(i64, minute) * 60 + second - offset;
    return .{ .seconds = seconds, .nanoseconds = nanoseconds, .offset = offset };
}

fn trimmed(taken: []const u8) []const u8 {
    return if (taken.len > 0 and taken[taken.len - 1] == '\r') taken[0 .. taken.len - 1] else taken;
}

fn parse(io: Io, path: []const u8, out: *Io.Writer) !void {
    const file = try Io.Dir.cwd().openFile(io, path, .{});
    defer file.close(io);
    var in_buffer: [65536]u8 = undefined;
    var file_reader = file.readerStreaming(io, &in_buffer);
    const r = &file_reader.interface;
    var count: i64 = 0;
    var seconds: i64 = 0;
    var nanoseconds: i64 = 0;
    var offsets: i64 = 0;
    while (try r.takeDelimiter('\n')) |taken| {
        count += 1;
        const seen = parseRfc3339(trimmed(taken)) catch {
            std.debug.print("time: {s}: line {d}: not a moment\n", .{ path, count });
            std.process.exit(1);
        };
        seconds += seen.seconds;
        nanoseconds += seen.nanoseconds;
        offsets += seen.offset;
    }
    try out.print("lines {d}\nseconds {d}\nnanoseconds {d}\noffsets {d}\n", .{ count, seconds, nanoseconds, offsets });
}

fn local(count: u64, out: *Io.Writer) !void {
    var random: SplitMix = .{ .state = 1 };
    var offsets: i64 = 0;
    var parts: i64 = 0;
    var i: u64 = 0;
    while (i < count) : (i += 1) {
        const t = moment(&random);
        const seconds: c.time_t = @intCast(t.seconds);
        var seen: c.struct_tm = undefined;
        _ = c.localtime_r(&seconds, &seen) orelse return error.NoLocalTime;
        offsets += seen.tm_gmtoff;
        parts += @as(i64, seen.tm_year) + 1900 + seen.tm_mon + 1 + seen.tm_mday + seen.tm_hour + seen.tm_min;
    }
    try out.print("moments {d}\noffsets {d}\nparts {d}\n", .{ count, offsets, parts });
}

fn clock(io: Io, count: u64, out: *Io.Writer) !void {
    var last = Io.Clock.awake.now(io);
    var forward = true;
    var i: u64 = 0;
    while (i < count) : (i += 1) {
        const now = Io.Clock.awake.now(io);
        if (now.nanoseconds < last.nanoseconds) forward = false;
        last = now;
    }
    try out.print("reads {d}\nforward {}\n", .{ count, forward });
}

pub fn main(init: std.process.Init) !u8 {
    const io = init.io;
    const arena = init.arena.allocator();
    const args = try init.minimal.args.toSlice(arena);
    const file = args.len == 3 and std.mem.eql(u8, args[1], "parse");
    const counts = args.len == 3 and (std.mem.eql(u8, args[1], "format") or
        std.mem.eql(u8, args[1], "local") or std.mem.eql(u8, args[1], "clock"));
    if (!file and !counts) {
        std.debug.print(usage, .{});
        return 2;
    }
    var out_buffer: [4096]u8 = undefined;
    var out_file: Io.File.Writer = .init(.stdout(), io, &out_buffer);
    const out = &out_file.interface;
    if (file) {
        parse(io, args[2], out) catch |err| {
            std.debug.print("time: {s}: {s}\n", .{ args[2], @errorName(err) });
            return 1;
        };
    } else {
        const count = std.fmt.parseInt(u64, args[2], 10) catch {
            std.debug.print(usage, .{});
            return 2;
        };
        if (std.mem.eql(u8, args[1], "format")) {
            try format(count, out);
        } else if (std.mem.eql(u8, args[1], "local")) {
            try local(count, out);
        } else {
            try clock(io, count, out);
        }
    }
    try out.flush();
    return 0;
}
