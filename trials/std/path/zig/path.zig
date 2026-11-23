//! `path`: `std`'s paths measured against Zig's `std.fs.path` (choices 1553, 1637, 1795,
//! 1800-1802). Two ways of using them:
//!
//!     path show PATH...    for each path its name, its extension, its folder and itself joined with
//!                          `more.txt`: `logs/report.txt: report.txt .txt logs logs/report.txt/more.txt`
//!     path many COUNT [WAY]
//!                          COUNT paths made from the seed 1, each joined from one to four folders and
//!                          a name, then taken apart into its name, its extension and its folder;
//!                          prints how many paths and the bytes of each kind of part. WAY (`push`
//!                          or `join`) picks one of SG's two ways; Zig has one, and takes the word
//!                          only so that every program runs the same case
//!
//! The paths are made by MINSTD (`x = x * 48271 % 2147483647`), the same in each language: a fifth
//! start at the root, the names are ASCII and not (`café`, `日本`), and an extension is one of six
//! or none. A path is built as Zig builds one, its parts joined at once by `path.join`. A wrong use
//! is told on the standard error, with exit code 2.

const std = @import("std");
const Io = std.Io;
const path = std.fs.path;

const usage = "usage: path show PATH... | many COUNT [push | join]\n";
const folders = [_][]const u8{ "src", "lib", "docs", "tests", "assets", "build", "images", "2026", "café", "日本" };
const names = [_][]const u8{ "report", "main", "index", "photo", "README", "data", "archive", "notes" };
const endings = [_][]const u8{ "", ".txt", ".tar.gz", ".rs", ".sg", ".json", ".jpeg" };

fn show(gpa: std.mem.Allocator, paths: []const [:0]const u8, out: *Io.Writer) !void {
    for (paths) |p| {
        const joined = try path.join(gpa, &.{ p, "more.txt" });
        defer gpa.free(joined);
        try out.print("{s}: {s} {s} {s} {s}\n", .{ p, path.basename(p), path.extension(p), path.dirname(p) orelse "", joined });
    }
}

fn many(gpa: std.mem.Allocator, count: i64, out: *Io.Writer) !void {
    var x: i64 = 1;
    var bytes: usize = 0;
    var name_bytes: usize = 0;
    var extensions: usize = 0;
    var parents: usize = 0;
    var with_extension: usize = 0;
    var name_buffer: [32]u8 = undefined;
    var i: i64 = 0;
    while (i < count) : (i += 1) {
        x = @mod(x * 48271, 2147483647);
        const depth: usize = @intCast(1 + @mod(x, 4));
        var parts: [6][]const u8 = undefined;
        var n: usize = 0;
        x = @mod(x * 48271, 2147483647);
        if (@mod(x, 5) == 0) {
            parts[n] = "/";
            n += 1;
        }
        for (0..depth) |_| {
            x = @mod(x * 48271, 2147483647);
            parts[n] = folders[@intCast(@mod(x, 10))];
            n += 1;
        }
        x = @mod(x * 48271, 2147483647);
        parts[n] = try std.fmt.bufPrint(&name_buffer, "{s}{s}", .{ names[@intCast(@mod(x, 8))], endings[@intCast(@mod(@divTrunc(x, 8), 7))] });
        n += 1;
        const p = try path.join(gpa, parts[0..n]);
        defer gpa.free(p);
        bytes += p.len;
        name_bytes += path.basename(p).len;
        const extension = path.extension(p);
        extensions += extension.len;
        if (extension.len > 0) with_extension += 1;
        if (path.dirname(p)) |folder| parents += folder.len;
    }
    try out.print("paths {d}\nbytes {d}\nnames {d}\nextensions {d} in {d}\nfolders {d}\n", .{ count, bytes, name_bytes, extensions, with_extension, parents });
}

pub fn main(init: std.process.Init) !u8 {
    const io = init.io;
    const gpa = init.gpa;
    const args = try init.minimal.args.toSlice(init.arena.allocator());
    var out_buffer: [4096]u8 = undefined;
    var out_file: Io.File.Writer = .init(.stdout(), io, &out_buffer);
    const out = &out_file.interface;
    if (args.len >= 3 and std.mem.eql(u8, args[1], "show")) {
        try show(gpa, args[2..], out);
        try out.flush();
        return 0;
    }
    const way_known = args.len == 3 or args.len == 4 and (std.mem.eql(u8, args[3], "push") or std.mem.eql(u8, args[3], "join"));
    const count: i64 = if (way_known and std.mem.eql(u8, args[1], "many"))
        std.fmt.parseInt(i64, args[2], 10) catch -1
    else
        -1;
    if (count < 0) {
        std.debug.print(usage, .{});
        return 2;
    }
    try many(gpa, count, out);
    try out.flush();
    return 0;
}
