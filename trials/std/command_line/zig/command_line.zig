//! `command_line`: `std`'s command line measured against Zig (choices 1553, 1637, 1795-1798). Zig's
//! library has no parser of command lines, so the arguments are read from `init.minimal.args` and
//! taken apart by hand, as the Zig compiler and most programs in Zig do (1797). The program is the
//! front of a packer of files, `pack [OPTIONS] FORMAT FOLDER [FILES]...`: four flags, three options
//! (`--exclude` may be given again), two words in their places and the rest, GNU's way of writing
//! (`--name value`, `--name=value`, `-vn`, `-ofile`, `--`). It packs nothing; it prints what it
//! read, one line for each name:
//!
//!     verbose true
//!     output out.zip           (`none` when it was not given)
//!     exclude 2 *.tmp *.log    (how many, then each)
//!     files 3 22 notes.txt -   (how many, their bytes, the first and the last)
//!
//! `-h` and `--help` print the help. A wrong use is told on the standard error, with exit code 2.

const std = @import("std");
const Io = std.Io;
const eql = std.mem.eql;

const help =
    \\Packs files into an archive.
    \\
    \\Usage: pack [OPTIONS] FORMAT FOLDER [FILES]...
    \\
    \\Arguments:
    \\  FORMAT                 The kind of archive: zip or tar
    \\  FOLDER                 The folder to pack
    \\  FILES                  Files to pack besides the folder
    \\
    \\Options:
    \\  -v, --verbose          Print each file as it is packed
    \\  -q, --quiet            Print nothing but errors
    \\  -f, --force            Write over an archive that exists
    \\  -n, --dry-run          Show what would be packed, and pack nothing
    \\  -o, --output FILE      Write the archive to FILE
    \\  -l, --level N          Compress at level N
    \\  -x, --exclude PATTERN  Leave out the files that match PATTERN; may be given again
    \\  -h, --help             Print this help
    \\
;

const Read = struct {
    verbose: bool = false,
    quiet: bool = false,
    force: bool = false,
    dry_run: bool = false,
    output: ?[]const u8 = null,
    level: ?[]const u8 = null,
    exclude: std.ArrayList([]const u8) = .empty,
    words: std.ArrayList([]const u8) = .empty,
};

const Wrong = error{ Unknown, NoValue, FlagValue, Help };

/// The flag named by a long name or a letter; false when it is none.
fn setFlag(r: *Read, long: []const u8, letter: u8) bool {
    if (eql(u8, long, "verbose") or letter == 'v') {
        r.verbose = true;
    } else if (eql(u8, long, "quiet") or letter == 'q') {
        r.quiet = true;
    } else if (eql(u8, long, "force") or letter == 'f') {
        r.force = true;
    } else if (eql(u8, long, "dry-run") or letter == 'n') {
        r.dry_run = true;
    } else return false;
    return true;
}

/// Sets the option named by a long name or a letter to `value`; false when it is none.
fn setOption(gpa: std.mem.Allocator, r: *Read, long: []const u8, letter: u8, value: []const u8) !bool {
    if (eql(u8, long, "output") or letter == 'o') {
        r.output = value;
    } else if (eql(u8, long, "level") or letter == 'l') {
        r.level = value;
    } else if (eql(u8, long, "exclude") or letter == 'x') {
        try r.exclude.append(gpa, value);
    } else return false;
    return true;
}

fn isOption(long: []const u8, letter: u8) bool {
    return eql(u8, long, "output") or eql(u8, long, "level") or eql(u8, long, "exclude") or
        letter == 'o' or letter == 'l' or letter == 'x';
}

fn parse(gpa: std.mem.Allocator, args: []const [:0]const u8, r: *Read, bad: *[]const u8) !void {
    var i: usize = 1;
    var words_only = false;
    while (i < args.len) : (i += 1) {
        const arg: []const u8 = args[i];
        if (words_only or arg.len < 2 or arg[0] != '-') {
            try r.words.append(gpa, arg);
        } else if (eql(u8, arg, "--")) {
            words_only = true;
        } else if (arg[1] == '-') {
            const body = arg[2..];
            const equals = std.mem.findScalar(u8, body, '=');
            const name = if (equals) |at| body[0..at] else body;
            bad.* = arg;
            if (eql(u8, name, "help")) return error.Help;
            if (isOption(name, 0)) {
                const value = if (equals) |at| body[at + 1 ..] else blk: {
                    i += 1;
                    if (i == args.len) return error.NoValue;
                    break :blk @as([]const u8, args[i]);
                };
                _ = try setOption(gpa, r, name, 0, value);
            } else if (equals != null) {
                return if (setFlag(r, name, 0)) error.FlagValue else error.Unknown;
            } else if (!setFlag(r, name, 0)) return error.Unknown;
        } else {
            var k: usize = 1;
            while (k < arg.len) : (k += 1) {
                bad.* = arg;
                if (arg[k] == 'h') return error.Help;
                if (isOption("", arg[k])) {
                    const value = if (k + 1 < arg.len) arg[k + 1 ..] else blk: {
                        i += 1;
                        if (i == args.len) return error.NoValue;
                        break :blk @as([]const u8, args[i]);
                    };
                    _ = try setOption(gpa, r, "", arg[k], value);
                    break;
                }
                if (!setFlag(r, "", arg[k])) return error.Unknown;
            }
        }
    }
}

pub fn main(init: std.process.Init) !u8 {
    const io = init.io;
    const gpa = init.gpa;
    const args = try init.minimal.args.toSlice(init.arena.allocator());
    var out_buffer: [4096]u8 = undefined;
    var out_file: Io.File.Writer = .init(.stdout(), io, &out_buffer);
    const out = &out_file.interface;

    var r: Read = .{};
    defer r.exclude.deinit(gpa);
    defer r.words.deinit(gpa);
    var bad: []const u8 = "";
    parse(gpa, args, &r, &bad) catch |e| {
        switch (e) {
            error.Help => {
                try out.writeAll(help);
                try out.flush();
                return 0;
            },
            error.Unknown => std.debug.print("pack: unknown option '{s}'\n", .{bad}),
            error.NoValue => std.debug.print("pack: option '{s}' needs a value\n", .{bad}),
            error.FlagValue => std.debug.print("pack: option '{s}' takes no value\n", .{bad}),
            else => return e,
        }
        std.debug.print("Try 'pack --help'.\n", .{});
        return 2;
    };
    if (r.words.items.len < 2) {
        std.debug.print("pack: missing <{s}>\nTry 'pack --help'.\n", .{if (r.words.items.len == 0) "format" else "folder"});
        return 2;
    }

    try out.print("verbose {}\nquiet {}\nforce {}\ndry-run {}\n", .{ r.verbose, r.quiet, r.force, r.dry_run });
    try out.print("output {s}\nlevel {s}\n", .{ r.output orelse "none", r.level orelse "none" });
    try out.print("exclude {d}", .{r.exclude.items.len});
    for (r.exclude.items) |pattern| try out.print(" {s}", .{pattern});
    const files = r.words.items[2..];
    try out.print("\nformat {s}\nfolder {s}\n", .{ r.words.items[0], r.words.items[1] });
    var bytes: usize = 0;
    for (files) |file| bytes += file.len;
    try out.print("files {d} {d}", .{ files.len, bytes });
    if (files.len > 0) try out.print(" {s} {s}", .{ files[0], files[files.len - 1] });
    try out.writeAll("\n");
    try out.flush();
    return 0;
}
