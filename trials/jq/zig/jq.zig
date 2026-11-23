//! `jq`: a filter of JSON, the part of jq 1.8.2's language that `trials/jq/README.md` names, with
//! jq's output byte for byte and its exit codes.
//!
//!     jq [-c] [-r] [-n] [-s] [-e] [-S] [--tab] [--indent n] [--arg name text]
//!        [--argjson name json] FILTER [FILE...]
//!
//! The input is read by `input.zig`, the filter by `parse.zig` into a tree that `eval.zig` runs
//! with a stack of its own; `builtin.zig` holds the functions on values, `value.zig` the values.
//! The output goes through a block of 64 KB of the program's own. Writing and comparing a value
//! call themselves as deep as the value is, which jq holds to 10,000 levels (deeper is written
//! `<skipped: too deep>`, compared with an error), so the machine's own stack is enough.

const std = @import("std");
const Io = std.Io;
const value = @import("value.zig");
const parse = @import("parse.zig");
const eval = @import("eval.zig");
const builtin = @import("builtin.zig");
const input_mod = @import("input.zig");
const Value = value.Value;
const gpa = value.gpa;

const usage_text = "usage: jq [-c] [-r] [-n] [-s] [-e] [-S] [--tab] [--indent n] [--arg name text] [--argjson name json] FILTER [FILE...]\n";

const Options = struct {
    style: value.Style = .{},
    /// The output is a terminal: each line is given to it at once, as the C library does for jq.
    line_buffered: bool = false,
    raw: bool = false,
    null_input: bool = false,
    slurp: bool = false,
    exit_status: bool = false,
};

// jq's results of a run, as its `main.c` names them.
const ok = 0;
const ok_null_kind = -1;
const error_system = 2;
const error_compile = 3;
const ok_no_output = -4;
const error_unknown = 5;

fn usage(comptime format: []const u8, args: anytype) u8 {
    std.debug.print("jq: " ++ format ++ "\n" ++ usage_text, args);
    return error_system;
}

/// Tells an error no `?` stopped, as jq words it.
fn tellError(err: Value) void {
    if (err == .string) {
        std.debug.print("jq: error (at <input>): {s}\n", .{err.string.bytes.items});
    } else {
        const text = value.toJson(err);
        defer value.release(text);
        std.debug.print("jq: error (at <input>) (not a string): {s}\n", .{text.string.bytes.items});
    }
}

pub fn main(init: std.process.Init) !u8 {
    // A closed pipe ends the program at once, killed by SIGPIPE as jq is; Zig's `Io` would
    // otherwise have the write fail.
    std.posix.sigaction(.PIPE, &.{
        .handler = .{ .handler = std.posix.SIG.DFL },
        .mask = std.posix.sigemptyset(),
        .flags = 0,
    }, null);
    const args = try init.minimal.args.toSlice(init.arena.allocator());
    return jq(init.io, args) catch 2;
}

fn jq(io: Io, args: []const []const u8) !u8 {
    var o: Options = .{};
    var filter: ?[]const u8 = null;
    var files: std.ArrayList([]const u8) = .empty;
    var global_names: std.ArrayList([]const u8) = .empty;
    var global_values: std.ArrayList(Value) = .empty;
    var options_end = false;
    var i: usize = 1;
    while (i < args.len) : (i += 1) {
        const arg = args[i];
        // An option is `-` and a letter, or `--`: `-1` and `-.a` are filters.
        if (options_end or arg.len < 2 or arg[0] != '-' or !(std.ascii.isAlphabetic(arg[1]) or arg[1] == '-')) {
            if (filter == null) filter = arg else try files.append(gpa, arg);
            continue;
        }
        if (std.mem.eql(u8, arg, "--")) {
            options_end = true;
        } else if (std.mem.startsWith(u8, arg, "--")) {
            const name = arg[2..];
            if (std.mem.eql(u8, name, "compact-output")) {
                o.style.pretty = false;
                o.style.tab = false;
            } else if (std.mem.eql(u8, name, "raw-output")) {
                o.raw = true;
            } else if (std.mem.eql(u8, name, "null-input")) {
                o.null_input = true;
            } else if (std.mem.eql(u8, name, "slurp")) {
                o.slurp = true;
            } else if (std.mem.eql(u8, name, "exit-status")) {
                o.exit_status = true;
            } else if (std.mem.eql(u8, name, "sort-keys")) {
                o.style.sort_keys = true;
            } else if (std.mem.eql(u8, name, "tab")) {
                o.style.tab = true;
                o.style.pretty = true;
            } else if (std.mem.eql(u8, name, "indent")) {
                if (i + 1 >= args.len) return usage("--indent takes one parameter", .{});
                const n = std.fmt.parseInt(i32, args[i + 1], 10) catch return usage("--indent takes a number", .{});
                if (n < -1 or n > 7) return usage("--indent takes a number between -1 and 7", .{});
                i += 1;
                o.style.tab = false;
                o.style.pretty = true;
                if (n == -1) {
                    o.style.tab = true;
                    o.style.pretty = true;
                } else {
                    o.style.indent = @intCast(n);
                }
            } else if (std.mem.eql(u8, name, "arg") or std.mem.eql(u8, name, "argjson")) {
                if (i + 2 >= args.len) return usage("--{s} takes two parameters (e.g. --{s} varname value)", .{ name, name });
                const v = if (name.len == 3) value.string(args[i + 2]) else blk: {
                    var message: []const u8 = "";
                    break :blk input_mod.parseText(args[i + 2], &message) orelse
                        return usage("Invalid JSON text passed to --argjson", .{});
                };
                try global_names.append(gpa, args[i + 1]);
                try global_values.append(gpa, v);
                i += 2;
            } else {
                return usage("Unknown arguments: {s}", .{arg});
            }
        } else {
            for (arg[1..]) |letter| switch (letter) {
                'c' => {
                    o.style.pretty = false;
                    o.style.tab = false;
                },
                'r' => o.raw = true,
                'n' => o.null_input = true,
                's' => o.slurp = true,
                'e' => o.exit_status = true,
                'S' => o.style.sort_keys = true,
                else => return usage("Unknown arguments: {s}", .{arg}),
            };
        }
    }
    // With no filter, jq runs `.`.
    const source = filter orelse ".";

    var parser = parse.Parser.init(source, global_names.items);
    const program = parser.parseProgram() catch {
        std.debug.print("jq: error: {s}\njq: 1 compile error\n", .{parser.message});
        return error_compile;
    };
    var globals: ?*eval.Env = null;
    for (global_values.items) |v| globals = eval.newEnv(globals, v);

    var out_buffer: [65536]u8 = undefined;
    // Streaming, not at positions of its own: the output may be a file others write to too.
    var out_file: Io.File.Writer = .initStreaming(.stdout(), io, &out_buffer);
    const out = &out_file.interface;
    o.line_buffered = Io.File.stdout().isTty(io) catch false;

    var input = input_mod.Input.init(io, files.items);
    input.slurp = o.slurp;
    var m: eval.Machine = .{ .ctx = .{ .input = &input } };

    var ret: i32 = ok_no_output;
    var last_result: i32 = -1;
    if (o.null_input) {
        ret = try process(&m, program, .null, globals, o, out);
    } else {
        // As jq's main loop, no value is read once a file could not be opened; the value
        // being read when that happened is still worked.
        while (!input.failed) {
            const v = input.next() catch {
                std.debug.print("jq: error (at <input>): {s}\n", .{input.message});
                ret = error_unknown;
                break;
            } orelse break;
            ret = try process(&m, program, v, globals, o, out);
            if (ret <= 0 and ret != ok_no_output) last_result = if (ret != ok_null_kind) 1 else 0;
        }
    }
    if (input.failed) ret = error_system;
    out.flush() catch return error_system;

    if (o.exit_status) {
        if (ret != ok_no_output) return @intCast(@abs(ret));
        return switch (last_result) {
            -1 => 4,
            0 => 1,
            else => 0,
        };
    }
    return if (ret > 0) @intCast(ret) else 0;
}

/// Runs the filter on one input (taken) and writes its outputs; jq's result of the run.
fn process(m: *eval.Machine, program: *parse.Node, v: Value, globals: ?*eval.Env, o: Options, out: *Io.Writer) !i32 {
    var ret: i32 = ok_no_output;
    m.start(program, v, globals);
    while (true) {
        const result = m.next() catch {
            tellError(m.ctx.err);
            value.replace(&m.ctx.err, .null);
            return error_unknown;
        } orelse break;
        defer value.release(result);
        if (o.raw and result == .string) {
            try out.writeAll(result.string.bytes.items);
            ret = ok;
        } else {
            ret = if (result == .null or result == .false) ok_null_kind else ok;
            try value.write(out, result, o.style, 0);
        }
        try out.writeByte('\n');
        if (o.line_buffered) try out.flush();
    }
    return ret;
}
