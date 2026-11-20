//! `grep`: the lines of each file named, or of the input, that a regular expression matches, with
//! PCRE2's syntax (a C library, compiled once with its JIT; Zig's standard library has no regular
//! expressions). `-i` ignores case, `-v` gives the lines that do not match, `-n` writes each line's
//! number in front of it, `-c` gives only how many lines there were. With more than one file, each
//! line starts with its file's name. A line is read as UTF-8 text without its `\n` or `\r\n`, and
//! may be up to 1 MB long. The exit code is 0 when a line was given, 1 when none was, 2 when a file
//! could not be read or the pattern is not one (as GNU grep's).

const std = @import("std");
const Io = std.Io;
const c = @import("pcre2");

const Options = struct {
    count: bool = false,
    invert: bool = false,
    numbers: bool = false,
    names: bool = false,
};

const Pattern = struct {
    code: *c.pcre2_code_8,
    data: *c.pcre2_match_data_8,

    fn isMatch(p: Pattern, line: []const u8) bool {
        const rc = c.pcre2_match_8(p.code, line.ptr, line.len, 0, c.PCRE2_NO_UTF_CHECK, p.data, null);
        return rc >= 0;
    }
};

/// Writes the lines of `file` that are wanted; how many there were.
fn search(io: Io, file: Io.File, p: Pattern, o: Options, name: []const u8, out: *Io.Writer, buffer: []u8) !u64 {
    var file_reader = file.readerStreaming(io, buffer);
    const r = &file_reader.interface;
    var number: u64 = 0;
    var found: u64 = 0;
    while (try r.takeDelimiter('\n')) |taken| {
        var line = taken;
        if (line.len > 0 and line[line.len - 1] == '\r') line = line[0 .. line.len - 1];
        number += 1;
        if (!std.unicode.utf8ValidateSlice(line)) return error.InvalidText;
        if (p.isMatch(line) == o.invert) continue;
        found += 1;
        if (o.count) continue;
        if (o.names) {
            try out.writeAll(name);
            try out.writeByte(':');
        }
        if (o.numbers) try out.print("{d}:", .{number});
        try out.writeAll(line);
        try out.writeByte('\n');
    }
    if (o.count) {
        if (o.names) {
            try out.print("{s}:{d}\n", .{ name, found });
        } else {
            try out.print("{d}\n", .{found});
        }
    }
    return found;
}

fn usage(message: []const u8, letter: u8) u8 {
    if (letter != 0) {
        std.debug.print("grep: {s} -{c}\n", .{ message, letter });
    } else {
        std.debug.print("grep: {s}\n", .{message});
    }
    std.debug.print("usage: grep [-c] [-i] [-n] [-v] PATTERN [FILE...]\n", .{});
    return 2;
}

pub fn main(init: std.process.Init) !u8 {
    const io = init.io;
    const arena = init.arena.allocator();
    const args = try init.minimal.args.toSlice(arena);
    var o: Options = .{};
    var ignore_case = false;
    var at: usize = 1;
    while (at < args.len) : (at += 1) {
        const arg = args[at];
        if (std.mem.eql(u8, arg, "--")) {
            at += 1;
            break;
        } else if (std.mem.eql(u8, arg, "--count")) {
            o.count = true;
        } else if (std.mem.eql(u8, arg, "--ignore-case")) {
            ignore_case = true;
        } else if (std.mem.eql(u8, arg, "--line-number")) {
            o.numbers = true;
        } else if (std.mem.eql(u8, arg, "--invert-match")) {
            o.invert = true;
        } else if (arg.len > 1 and arg[0] == '-') {
            for (arg[1..]) |letter| switch (letter) {
                'c' => o.count = true,
                'i' => ignore_case = true,
                'n' => o.numbers = true,
                'v' => o.invert = true,
                else => return usage("unknown option", letter),
            };
        } else break;
    }
    if (at >= args.len) return usage("no pattern", 0);
    const pattern = if (ignore_case) try std.mem.concat(arena, u8, &.{ "(?i)", args[at] }) else args[at];
    const files = args[at + 1 ..];
    o.names = files.len > 1;

    var error_code: c_int = 0;
    var error_offset: usize = 0;
    const code = c.pcre2_compile_8(pattern.ptr, pattern.len, c.PCRE2_UTF, &error_code, &error_offset, null) orelse {
        var message: [256]u8 = undefined;
        const n = c.pcre2_get_error_message_8(error_code, &message, message.len);
        std.debug.print("grep: column {d}: {s}\n", .{ error_offset + 1, message[0..@intCast(@max(n, 0))] });
        return 2;
    };
    defer c.pcre2_code_free_8(code);
    _ = c.pcre2_jit_compile_8(code, c.PCRE2_JIT_COMPLETE);
    const data = c.pcre2_match_data_create_from_pattern_8(code, null) orelse return error.OutOfMemory;
    defer c.pcre2_match_data_free_8(data);
    const p: Pattern = .{ .code = code, .data = data };

    const buffer = try arena.alloc(u8, 1 << 20);
    var out_buffer: [65536]u8 = undefined;
    var out_file: Io.File.Writer = .init(.stdout(), io, &out_buffer);
    const out = &out_file.interface;
    var found: u64 = 0;
    var failed = false;
    if (files.len == 0) {
        if (search(io, .stdin(), p, o, "", out, buffer)) |n| {
            found += n;
        } else |err| {
            try out.flush();
            std.debug.print("grep: {s}\n", .{@errorName(err)});
            failed = true;
        }
    }
    for (files) |name| {
        const result = blk: {
            const file = Io.Dir.cwd().openFile(io, name, .{}) catch |err| break :blk err;
            defer file.close(io);
            break :blk search(io, file, p, o, name, out, buffer);
        };
        if (result) |n| {
            found += n;
        } else |err| {
            try out.flush();
            std.debug.print("grep: {s}: {s}\n", .{ name, @errorName(err) });
            failed = true;
        }
    }
    out.flush() catch return 2;
    if (failed) return 2;
    if (found == 0) return 1;
    return 0;
}
