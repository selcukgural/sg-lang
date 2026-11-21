//! `unicode`: `std`'s categories and normal forms of Unicode measured against ICU4C, called from Zig:
//! Zig's `std` has neither (choices 1553, 1629-1637, 1659, 1661, 1662). Four ways of using them,
//! each on every line of FILE, without its end (`\n` or `\r\n`):
//!
//!     unicode category FILE      the general category of each character; how many characters of
//!                                each category, in the order of SG's `UnicodeCategory`
//!     unicode nfc FILE OUT       each line in NFC, written into OUT with `\n` after it
//!     unicode nfd FILE OUT       each line in NFD, the same way
//!     unicode check FILE         whether each line is in NFC; how many lines, and how many are
//!
//! `nfc` and `nfd` print how many lines they wrote. A file that cannot be read or written is told on
//! the standard error, with exit code 1; a wrong use, with 2.
//!
//! ICU4C's normal forms are of UTF-16, as its C functions take no UTF-8: each line is turned into
//! UTF-16 (`u_strFromUTF8`, which checks it), and what is written back into UTF-8. The categories
//! are taken of the code points of the line, checked to be UTF-8 by Zig.

const std = @import("std");
const Io = std.Io;
const icu = @import("icu");

const usage = "usage: unicode category FILE | nfc FILE OUT | nfd FILE OUT | check FILE\n";

const abbreviations = [29][]const u8{
    "Lu", "Ll", "Lt", "Lm", "Lo", "Mn", "Mc", "Me", "Nd", "Nl", "No", "Pc", "Pd", "Ps", "Pe",
    "Pi", "Pf", "Po", "Sm", "Sc", "Sk", "So", "Zs", "Zl", "Zp", "Cc", "Cf", "Co", "Cn",
};

/// ICU's `UCharCategory`, by its number, to the place in the counts: in the order of SG's
/// `UnicodeCategory`. A surrogate (18) counts as unassigned; no line of UTF-8 holds one.
const places = [30]u8{ 28, 0, 1, 2, 3, 4, 5, 7, 6, 8, 9, 10, 22, 23, 24, 25, 26, 27, 28, 12, 13, 14, 11, 17, 18, 19, 20, 21, 15, 16 };

const Failure = error{ NotUtf8, Icu };

fn fail(name: []const u8, err: anyerror) u8 {
    std.debug.print("unicode: {s}: {s}\n", .{ name, @errorName(err) });
    return 1;
}

fn trimmed(taken: []const u8) []const u8 {
    return if (taken.len > 0 and taken[taken.len - 1] == '\r') taken[0 .. taken.len - 1] else taken;
}

fn category(io: Io, path: []const u8, out: *Io.Writer) !void {
    const file = try Io.Dir.cwd().openFile(io, path, .{});
    defer file.close(io);
    var in_buffer: [65536]u8 = undefined;
    var file_reader = file.readerStreaming(io, &in_buffer);
    const r = &file_reader.interface;
    var counts: [29]u64 = @splat(0);
    while (try r.takeDelimiter('\n')) |taken| {
        const view = std.unicode.Utf8View.init(trimmed(taken)) catch return Failure.NotUtf8;
        var it = view.iterator();
        while (it.nextCodepoint()) |c| {
            counts[places[@intCast(icu.icu_charType(@intCast(c)))]] += 1;
        }
    }
    for (abbreviations, counts) |name, count| try out.print("{s} {d}\n", .{ name, count });
}

/// What ICU keeps from one line to the next: the line in UTF-16, its normal form, the form in UTF-8.
const Buffers = struct {
    gpa: std.mem.Allocator,
    utf16: std.ArrayList(u16) = .empty,
    normal: std.ArrayList(u16) = .empty,
    utf8: std.ArrayList(u8) = .empty,

    /// The line as UTF-16, in `utf16`.
    fn fromUtf8(b: *Buffers, line: []const u8) ![]const u16 {
        try b.utf16.ensureTotalCapacity(b.gpa, line.len + 1);
        var length: i32 = 0;
        var status: icu.UErrorCode = icu.U_ZERO_ERROR;
        _ = icu.icu_strFromUTF8(b.utf16.allocatedSlice().ptr, @intCast(b.utf16.capacity), &length, line.ptr, @intCast(line.len), &status);
        if (status > icu.U_ZERO_ERROR) return Failure.NotUtf8;
        return b.utf16.allocatedSlice()[0..@intCast(length)];
    }

    /// The normal form of `text` by `form`, in UTF-8, ending with `\n`.
    fn normalize(b: *Buffers, form: ?*const icu.UNormalizer2, text: []const u16) ![]const u8 {
        try b.normal.ensureTotalCapacity(b.gpa, text.len * 3 + 1);
        var status: icu.UErrorCode = icu.U_ZERO_ERROR;
        var length = icu.icu_normalize(form, text.ptr, @intCast(text.len), b.normal.allocatedSlice().ptr, @intCast(b.normal.capacity), &status);
        if (status == icu.U_BUFFER_OVERFLOW_ERROR) {
            try b.normal.ensureTotalCapacity(b.gpa, @intCast(length + 1));
            status = icu.U_ZERO_ERROR;
            length = icu.icu_normalize(form, text.ptr, @intCast(text.len), b.normal.allocatedSlice().ptr, @intCast(b.normal.capacity), &status);
        }
        if (status > icu.U_ZERO_ERROR) return Failure.Icu;
        const units: usize = @intCast(length);
        try b.utf8.ensureTotalCapacity(b.gpa, units * 3 + 1);
        var written: i32 = 0;
        _ = icu.icu_strToUTF8(b.utf8.allocatedSlice().ptr, @intCast(b.utf8.capacity), &written, b.normal.allocatedSlice().ptr, length, &status);
        if (status > icu.U_ZERO_ERROR) return Failure.Icu;
        const end: usize = @intCast(written);
        b.utf8.allocatedSlice()[end] = '\n';
        return b.utf8.allocatedSlice()[0 .. end + 1];
    }
};

fn normalize(io: Io, gpa: std.mem.Allocator, path: []const u8, out_path: []const u8, composed: bool, out: *Io.Writer) !void {
    var status: icu.UErrorCode = icu.U_ZERO_ERROR;
    const form = if (composed) icu.icu_nfc(&status) else icu.icu_nfd(&status);
    if (status > icu.U_ZERO_ERROR) return Failure.Icu;
    const file = try Io.Dir.cwd().openFile(io, path, .{});
    defer file.close(io);
    var in_buffer: [65536]u8 = undefined;
    var file_reader = file.readerStreaming(io, &in_buffer);
    const r = &file_reader.interface;
    const out_file = try Io.Dir.cwd().createFile(io, out_path, .{});
    defer out_file.close(io);
    var out_buffer: [65536]u8 = undefined;
    var file_writer = out_file.writer(io, &out_buffer);
    var buffers: Buffers = .{ .gpa = gpa };
    var lines: u64 = 0;
    while (try r.takeDelimiter('\n')) |taken| {
        const text = try buffers.fromUtf8(trimmed(taken));
        try file_writer.interface.writeAll(try buffers.normalize(form, text));
        lines += 1;
    }
    try file_writer.end();
    try out.print("lines {d}\n", .{lines});
}

fn check(io: Io, gpa: std.mem.Allocator, path: []const u8, out: *Io.Writer) !void {
    var status: icu.UErrorCode = icu.U_ZERO_ERROR;
    const form = icu.icu_nfc(&status);
    if (status > icu.U_ZERO_ERROR) return Failure.Icu;
    const file = try Io.Dir.cwd().openFile(io, path, .{});
    defer file.close(io);
    var in_buffer: [65536]u8 = undefined;
    var file_reader = file.readerStreaming(io, &in_buffer);
    const r = &file_reader.interface;
    var buffers: Buffers = .{ .gpa = gpa };
    var lines: u64 = 0;
    var normalized: u64 = 0;
    while (try r.takeDelimiter('\n')) |taken| {
        const text = try buffers.fromUtf8(trimmed(taken));
        if (icu.icu_isNormalized(form, text.ptr, @intCast(text.len), &status) != 0) normalized += 1;
        if (status > icu.U_ZERO_ERROR) return Failure.Icu;
        lines += 1;
    }
    try out.print("lines {d}\nnormalized {d}\n", .{ lines, normalized });
}

pub fn main(init: std.process.Init) !u8 {
    const io = init.io;
    const arena = init.arena.allocator();
    const args = try init.minimal.args.toSlice(arena);
    const one = args.len == 3 and (std.mem.eql(u8, args[1], "category") or std.mem.eql(u8, args[1], "check"));
    const two = args.len == 4 and (std.mem.eql(u8, args[1], "nfc") or std.mem.eql(u8, args[1], "nfd"));
    if (!one and !two) {
        std.debug.print(usage, .{});
        return 2;
    }
    var out_buffer: [4096]u8 = undefined;
    var out_file: Io.File.Writer = .init(.stdout(), io, &out_buffer);
    const out = &out_file.interface;
    const name = args[2];
    if (std.mem.eql(u8, args[1], "category")) {
        category(io, name, out) catch |err| return fail(name, err);
    } else if (std.mem.eql(u8, args[1], "check")) {
        check(io, init.gpa, name, out) catch |err| return fail(name, err);
    } else {
        normalize(io, init.gpa, name, args[3], std.mem.eql(u8, args[1], "nfc"), out) catch |err| return fail(name, err);
    }
    try out.flush();
    return 0;
}
