//! `environment`: `std`'s variables of the environment measured against Zig (choices 1553, 1637,
//! 1795, 1799). Zig 0.17 gives `main` the environment as a map (`init.environ_map`); a variable is
//! looked up there, copied into a new text and checked to be UTF-8, the work SG's
//! `Environment.get` and Rust's `env::var` do. (`getEnvVarOwned`, which 1799 names, is gone from
//! Zig 0.17; its heir `Environ.getAlloc` builds the whole map on each call.) Two ways of using them:
//!
//!     environment get NAME...          each variable as text: `NAME value`, `NAME missing`, or
//!                                      `NAME not text` when what it holds is not UTF-8
//!     environment read NAME COUNT      the variable read COUNT times, each time as a new text;
//!                                      prints how many reads and how many bytes they gave
//!
//! A wrong use is told on the standard error, with exit code 2.

const std = @import("std");
const Io = std.Io;

const usage = "usage: environment get NAME... | read NAME COUNT\n";

const Missing = error{ Missing, NotText };

/// The variable as a new text, which the caller frees.
fn variable(gpa: std.mem.Allocator, map: *const std.process.Environ.Map, name: []const u8) ![]u8 {
    const value = map.get(name) orelse return error.Missing;
    if (!std.unicode.utf8ValidateSlice(value)) return error.NotText;
    return gpa.dupe(u8, value);
}

pub fn main(init: std.process.Init) !u8 {
    const io = init.io;
    const gpa = init.gpa;
    const args = try init.minimal.args.toSlice(init.arena.allocator());
    var out_buffer: [4096]u8 = undefined;
    var out_file: Io.File.Writer = .init(.stdout(), io, &out_buffer);
    const out = &out_file.interface;

    if (args.len >= 3 and std.mem.eql(u8, args[1], "get")) {
        for (args[2..]) |name| {
            if (variable(gpa, init.environ_map, name)) |value| {
                defer gpa.free(value);
                try out.print("{s} {s}\n", .{ name, value });
            } else |e| switch (e) {
                error.Missing => try out.print("{s} missing\n", .{name}),
                error.NotText => try out.print("{s} not text\n", .{name}),
                else => return e,
            }
        }
        try out.flush();
        return 0;
    }
    const count: i64 = if (args.len == 4 and std.mem.eql(u8, args[1], "read"))
        std.fmt.parseInt(i64, args[3], 10) catch -1
    else
        -1;
    if (count < 0) {
        std.debug.print(usage, .{});
        return 2;
    }
    var bytes: usize = 0;
    var i: i64 = 0;
    while (i < count) : (i += 1) {
        const value = variable(gpa, init.environ_map, args[2]) catch continue;
        bytes += value.len;
        gpa.free(value);
    }
    try out.print("reads {d}\nbytes {d}\n", .{ count, bytes });
    try out.flush();
    return 0;
}
