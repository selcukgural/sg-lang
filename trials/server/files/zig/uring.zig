//! The file server of `files.zig` on `Io.Evented`, Zig's io_uring on Linux (choice 1872): each
//! connection a fiber, the disk asked with io_uring's own STATX, OPENAT and READ, which the kernel
//! answers at once from its cache and hands to threads of its own otherwise. The fastest form of the
//! Zig version on Linux, measured beside the one on `Io.Threaded`.
//!
//!     files-uring PORT ROOT
//!
//! Zig 0.17.1's library does not compile it: `Io.Uring` names an operation `net_accept` that
//! `Io.Operation` does not have. `measure.py` tries to build it and says so when it cannot.

const std = @import("std");
const files = @import("files.zig");

pub fn main(init: std.process.Init) !u8 {
    const arena = init.arena.allocator();
    const args = try init.minimal.args.toSlice(arena);
    const port = if (args.len == 3) std.fmt.parseInt(u16, args[1], 10) catch null else null;
    if (port == null) {
        std.debug.print("usage: files-uring PORT ROOT\n", .{});
        return 2;
    }
    var ev: std.Io.Evented = undefined;
    try ev.init(std.heap.smp_allocator, .{});
    defer ev.deinit();
    return files.run(ev.io(), args[2], port.?);
}
