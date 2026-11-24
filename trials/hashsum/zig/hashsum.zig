//! `hashsum`: the SHA-256 of each file it is given, worked out by a pool of workers, written in the
//! order the files were given, as GNU's `sha256sum` writes them (choice 1854).
//!
//!     hashsum [-j N] [--] [FILE...]
//!
//! Each line is the hash in hex, two spaces and the name; a name with `\`, a line's end or a CR in it
//! is written with them as `\\`, `\n` and `\r`, and a `\` before the hash. `-`, or no file at all, is
//! the standard input. A file that cannot be read is told on the standard error, the others go on, and
//! the program ends with 1. `-j N` is the number of workers, by default the processors the program may
//! run on (`std.Thread.getCpuCount`).
//!
//! The pool (choice 1855): the files go to the workers on a queue with room for twice as many as there
//! are workers (`std.Io.Queue`, the library's channel), each worker reads a file in blocks of 64 KB into
//! the hash and puts what it found on a second queue, and the main thread writes the answers in the
//! order of the files, holding those that come early. It never gives out more than that room past the
//! first file not yet written, so neither queue is ever full and what waits is bounded. The standard
//! input is read by the main thread itself, when its turn comes: a second `-` finds it at its end, as
//! `sha256sum` does.

const std = @import("std");
const Io = std.Io;
const Sha256 = std.crypto.hash.sha2.Sha256;

const gpa = std.heap.smp_allocator;
const block_size = 65536;
const output_block = 65536;

const Job = struct {
    index: usize,
    path: []const u8,
};

/// What a worker found: the line of the output, or the message of the error.
const Done = struct {
    index: usize,
    failed: bool,
    text: []u8,
};

/// The line `sha256sum` writes for a hash and a name.
fn lineOf(digest: *const [32]u8, name: []const u8) ![]u8 {
    const escaped = std.mem.indexOfAny(u8, name, "\\\n\r") != null;
    var line: std.ArrayList(u8) = try .initCapacity(gpa, 68 + name.len);
    if (escaped) line.appendAssumeCapacity('\\');
    const hex = std.fmt.bytesToHex(digest, .lower);
    line.appendSliceAssumeCapacity(&hex);
    line.appendSliceAssumeCapacity("  ");
    if (!escaped) {
        line.appendSliceAssumeCapacity(name);
    } else {
        for (name) |c| switch (c) {
            '\\' => try line.appendSlice(gpa, "\\\\"),
            '\n' => try line.appendSlice(gpa, "\\n"),
            '\r' => try line.appendSlice(gpa, "\\r"),
            else => try line.append(gpa, c),
        };
    }
    try line.append(gpa, '\n');
    return line.toOwnedSlice(gpa);
}

fn hashFile(io: Io, file: Io.File, block: []u8) ![32]u8 {
    var hash = Sha256.init(.{});
    while (true) {
        const got = file.readStreaming(io, &.{block}) catch |err| switch (err) {
            error.EndOfStream => break,
            else => |e| return e,
        };
        if (got == 0) break;
        hash.update(block[0..got]);
    }
    return hash.finalResult();
}

fn hashPath(io: Io, path: []const u8, block: []u8) ![32]u8 {
    const file = try Io.Dir.cwd().openFile(io, path, .{});
    defer file.close(io);
    return hashFile(io, file, block);
}

fn doneOf(index: usize, path: []const u8, hashed: anyerror![32]u8) !Done {
    if (hashed) |digest| {
        return .{ .index = index, .failed = false, .text = try lineOf(&digest, path) };
    } else |err| {
        return .{ .index = index, .failed = true, .text = try std.fmt.allocPrint(gpa, "hashsum: {s}: {s}", .{ path, @errorName(err) }) };
    }
}

fn work(io: Io, jobs: *Io.Queue(Job), done: *Io.Queue(Done)) void {
    const block = gpa.alloc(u8, block_size) catch @panic("out of memory");
    defer gpa.free(block);
    while (true) {
        const job = jobs.getOneUncancelable(io) catch return;
        const found = doneOf(job.index, job.path, hashPath(io, job.path, block)) catch @panic("out of memory");
        done.putOneUncancelable(io, found) catch return;
    }
}

/// Gives the files out and writes the answers in order; whether one could not be read. The queue of
/// jobs is closed at the end, so the workers end once they have the last.
fn serve(io: Io, paths: []const []const u8, jobs: *Io.Queue(Job), results: *Io.Queue(Done), room: usize) !bool {
    defer jobs.close(io);
    const total = paths.len;
    const held = try gpa.alloc(?Done, room);
    defer gpa.free(held);
    @memset(held, null);
    var block: []u8 = &.{};
    defer gpa.free(block);
    var out_buffer: [output_block]u8 = undefined;
    var stdout_writer = Io.File.stdout().writerStreaming(io, &out_buffer);
    const stdout = &stdout_writer.interface;
    var given: usize = 0;
    var written: usize = 0;
    var failed = false;
    while (written < total) {
        // What is there in order is written.
        while (written < given) {
            const done = held[written % room] orelse break;
            held[written % room] = null;
            if (done.failed) {
                try stdout.flush();
                std.debug.print("{s}\n", .{done.text});
                failed = true;
            } else {
                try stdout.writeAll(done.text);
            }
            gpa.free(done.text);
            written += 1;
        }
        if (written == total) break;
        if (given < total and given - written < room) {
            if (std.mem.eql(u8, paths[given], "-")) {
                if (block.len == 0) block = try gpa.alloc(u8, block_size);
                held[given % room] = try doneOf(given, "-", hashFile(io, Io.File.stdin(), block));
            } else {
                jobs.putOneUncancelable(io, .{ .index = given, .path = paths[given] }) catch break;
            }
            given += 1;
            continue;
        }
        const done = try results.getOneUncancelable(io);
        held[done.index % room] = done;
    }
    try stdout.flush();
    return failed;
}

fn usage() noreturn {
    std.debug.print("usage: hashsum [-j N] [--] [FILE...]\n", .{});
    std.process.exit(2);
}

pub fn main(init: std.process.Init) !u8 {
    // A closed pipe ends the program at once, killed by SIGPIPE as a C program is; Zig's `Io` would
    // otherwise have the write fail.
    std.posix.sigaction(.PIPE, &.{
        .handler = .{ .handler = std.posix.SIG.DFL },
        .mask = std.posix.sigemptyset(),
        .flags = 0,
    }, null);
    const io = init.io;
    const args = try init.minimal.args.toSlice(init.arena.allocator());
    var workers: usize = std.Thread.getCpuCount() catch 1;
    var paths: std.ArrayList([]const u8) = .empty;
    var options = true;
    var i: usize = 1;
    while (i < args.len) : (i += 1) {
        const arg = args[i];
        if (options and std.mem.eql(u8, arg, "--")) {
            options = false;
        } else if (options and std.mem.eql(u8, arg, "-j")) {
            i += 1;
            if (i >= args.len) usage();
            workers = std.fmt.parseInt(usize, args[i], 10) catch usage();
        } else if (options and arg.len > 1 and arg[0] == '-') {
            usage();
        } else {
            try paths.append(gpa, arg);
        }
    }
    if (workers < 1) usage();
    if (paths.items.len == 0) try paths.append(gpa, "-");
    const room = 2 * workers;
    const job_buffer = try gpa.alloc(Job, room);
    const done_buffer = try gpa.alloc(Done, room);
    var jobs: Io.Queue(Job) = .init(job_buffer);
    var results: Io.Queue(Done) = .init(done_buffer);
    const pool = try gpa.alloc(std.Thread, workers);
    for (pool) |*thread| thread.* = try std.Thread.spawn(.{}, work, .{ io, &jobs, &results });
    const served = serve(io, paths.items, &jobs, &results, room);
    for (pool) |thread| thread.join();
    const failed = served catch |err| {
        std.debug.print("hashsum: {s}\n", .{@errorName(err)});
        return 1;
    };
    return if (failed) 1 else 0;
}
