//! The HTTP server of the server lane in Zig (choice 1706): GET /ping answered with "pong" over
//! HTTP/1.1 with connections kept open, by `std.http.Server`, as `../c/http.c` answers it: GET or
//! HEAD of /ping is 200 with `pong`, another target 404, another method 405. `std.http.Server`
//! reads and checks the request itself, keeps or closes the connection as `Connection` says, and
//! sends each answer as it is made.
//!
//!     http PORT
//!
//! Each connection is a task of an `Io.Group` started with `concurrent` on `Io.Threaded`, a thread,
//! as in `../../resp/zig/resp.zig`; its reader holds 8 KB, the most a head may take, and its writer
//! 4 KB. The sockets are given TCP_NODELAY, as a server is.

const std = @import("std");
const Io = std.Io;
const net = Io.net;
const http = std.http;

fn serveConnection(io: Io, conn: net.Stream) !void {
    var in_buffer: [8192]u8 = undefined;
    var out_buffer: [4096]u8 = undefined;
    var reader = conn.reader(io, &in_buffer);
    var writer = conn.writer(io, &out_buffer);
    var server = http.Server.init(&reader.interface, &writer.interface);
    while (true) {
        var request = server.receiveHead() catch |err| switch (err) {
            error.HttpConnectionClosing => return,
            else => return err,
        };
        const get = request.head.method == .GET or request.head.method == .HEAD;
        const ping = std.mem.eql(u8, request.head.target, "/ping");
        const keep = request.head.keep_alive;
        if (!get) {
            try request.respond("method not allowed", .{
                .status = .method_not_allowed,
                .keep_alive = keep,
                .extra_headers = &.{ .{ .name = "content-type", .value = "text/plain" }, .{ .name = "allow", .value = "GET, HEAD" } },
            });
        } else {
            try request.respond(if (ping) "pong" else "not found", .{
                .status = if (ping) .ok else .not_found,
                .keep_alive = keep,
                .extra_headers = &.{.{ .name = "content-type", .value = "text/plain" }},
            });
        }
        if (!keep) return;
    }
}

/// A connection that fails ends alone.
fn connection(io: Io, conn: net.Stream) Io.Cancelable!void {
    defer conn.close(io);
    // `std.http.Server` gives no `Canceled`: a cancel ends the reads and writes it waits in, and
    // the connection ends with them.
    serveConnection(io, conn) catch {};
}

pub fn main(init: std.process.Init) !u8 {
    const io = init.io;
    const arena = init.arena.allocator();
    const args = try init.minimal.args.toSlice(arena);
    const port = if (args.len == 2) std.fmt.parseInt(u16, args[1], 10) catch null else null;
    if (port == null) {
        std.debug.print("usage: http PORT\n", .{});
        return 2;
    }
    var address = try net.IpAddress.parseLiteral("0.0.0.0:0");
    address.setPort(port.?);
    var server = try address.listen(io, .{ .reuse_address = true, .kernel_backlog = 4096 });
    defer server.deinit(io);
    var group: Io.Group = .init;
    defer group.cancel(io);
    const one: c_int = 1;
    while (true) {
        const conn = server.accept(io) catch |err| switch (err) {
            error.Canceled => return err,
            else => {
                // too many open files, say: try again soon
                io.sleep(.fromMilliseconds(10), .awake) catch {};
                continue;
            },
        };
        std.posix.setsockopt(conn.socket.handle, std.posix.IPPROTO.TCP, std.posix.TCP.NODELAY, std.mem.asBytes(&one)) catch {};
        group.concurrent(io, connection, .{ io, conn }) catch {
            conn.close(io);
        };
    }
}
