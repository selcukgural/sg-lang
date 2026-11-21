//! `json`: `std`'s JSON measured against Zig's `std.json` (choices 1553, 1629-1637). Three ways of
//! using it:
//!
//!     json sum FILE      reads the document and walks it: what it holds, counted and added up
//!     json copy FILE     reads the document and writes it back on one line
//!     json lines FILE    reads a document on each line, and walks each, as `sum` does
//!
//! What `sum` and `lines` print: how many values of each kind there are, the bytes of the texts
//! and of the names of members, and the sums of the whole numbers and of the others, added in the
//! order they come. A file that cannot be read or is not JSON is told on the standard error, with
//! exit code 1; a wrong use, with 2.

const std = @import("std");
const Io = std.Io;
const Value = std.json.Value;

const Counts = struct {
    values: i64 = 0,
    nulls: i64 = 0,
    bools: i64 = 0,
    integers: i64 = 0,
    numbers: i64 = 0,
    strings: i64 = 0,
    arrays: i64 = 0,
    objects: i64 = 0,
    string_bytes: i64 = 0,
    name_bytes: i64 = 0,
    integer_sum: i64 = 0,
    number_sum: f64 = 0,
};

fn walk(value: Value, c: *Counts) void {
    c.values += 1;
    switch (value) {
        .null => c.nulls += 1,
        .bool => c.bools += 1,
        .integer => |n| {
            c.integers += 1;
            c.integer_sum += n;
        },
        .float => |d| {
            c.numbers += 1;
            c.number_sum += d;
        },
        // A whole number beyond an `i64`: the input has none.
        .number_string => c.numbers += 1,
        .string => |text| {
            c.strings += 1;
            c.string_bytes += @intCast(text.len);
        },
        .array => |items| {
            c.arrays += 1;
            for (items.items) |item| walk(item, c);
        },
        .object => |members| {
            c.objects += 1;
            var it = members.iterator();
            while (it.next()) |member| {
                c.name_bytes += @intCast(member.key_ptr.len);
                walk(member.value_ptr.*, c);
            }
        },
    }
}

fn show(c: Counts, out: *Io.Writer) !void {
    try out.print("values {d}\nnull {d}\nbool {d}\ninteger {d}\nnumber {d}\n", .{ c.values, c.nulls, c.bools, c.integers, c.numbers });
    try out.print("string {d}\narray {d}\nobject {d}\n", .{ c.strings, c.arrays, c.objects });
    try out.print("string bytes {d}\nname bytes {d}\n", .{ c.string_bytes, c.name_bytes });
    try out.print("integer sum {d}\nnumber sum {d}\n", .{ c.integer_sum, c.number_sum });
}

fn fail(comptime format: []const u8, args: anytype) u8 {
    std.debug.print("json: " ++ format ++ "\n", args);
    return 1;
}

pub fn main(init: std.process.Init) !u8 {
    const io = init.io;
    const arena = init.arena.allocator();
    const args = try init.minimal.args.toSlice(arena);
    const usage = args.len != 3 or !(std.mem.eql(u8, args[1], "sum") or
        std.mem.eql(u8, args[1], "copy") or std.mem.eql(u8, args[1], "lines"));
    if (usage) {
        std.debug.print("usage: json sum|copy|lines FILE\n", .{});
        return 2;
    }
    const mode = args[1];
    const name = args[2];
    var out_buffer: [65536]u8 = undefined;
    var out_file: Io.File.Writer = .init(.stdout(), io, &out_buffer);
    const out = &out_file.interface;
    var c: Counts = .{};
    if (std.mem.eql(u8, mode, "lines")) {
        const file = Io.Dir.cwd().openFile(io, name, .{}) catch |err| return fail("{s}: {s}", .{ name, @errorName(err) });
        defer file.close(io);
        var buffer: [65536]u8 = undefined;
        var file_reader = file.readerStreaming(io, &buffer);
        const r = &file_reader.interface;
        // One arena for the values of a line, emptied before the next.
        var lines_arena: std.heap.ArenaAllocator = .init(init.gpa);
        defer lines_arena.deinit();
        var number: u64 = 0;
        while (r.takeDelimiter('\n') catch |err| return fail("{s}: {s}", .{ name, @errorName(err) })) |line| {
            number += 1;
            _ = lines_arena.reset(.retain_capacity);
            const value = std.json.parseFromSliceLeaky(Value, lines_arena.allocator(), line, .{}) catch |err|
                return fail("{s}, line {d}: {s}", .{ name, number, @errorName(err) });
            walk(value, &c);
        }
        try out.print("documents {d}\n", .{number});
        try show(c, out);
        try out.flush();
        return 0;
    }
    const text = Io.Dir.cwd().readFileAlloc(io, name, arena, .unlimited) catch |err|
        return fail("{s}: {s}", .{ name, @errorName(err) });
    const value = std.json.parseFromSliceLeaky(Value, arena, text, .{}) catch |err|
        return fail("{s}: {s}", .{ name, @errorName(err) });
    if (std.mem.eql(u8, mode, "sum")) {
        walk(value, &c);
        try show(c, out);
    } else {
        try std.json.Stringify.value(value, .{}, out);
        try out.writeByte('\n');
    }
    try out.flush();
    return 0;
}
