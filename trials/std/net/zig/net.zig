//! `net`: the network measured against Zig's `std.Io.net` (choices 1553, 1629-1637, 1678-1682).
//! Everything runs in one program, over the loopback: the server and its clients are tasks of it,
//! each started with `io.concurrent`. The `Io` is the one `main` is given, `Io.Threaded`: Zig 0.17
//! has no `Io` that waits for sockets on a loop on macOS (`Io.Evented` there has no network, and
//! `Io.Kqueue` is not finished), so a task is a thread and its sockets block. MODE is taken, `async`
//! or `blocking`, and both run the same code:
//!
//!     net echo MODE SIZE CLIENTS ROUNDS   CLIENTS connections, each sending SIZE bytes and waiting
//!                                          for them to come back, ROUNDS times
//!     net connect MODE COUNT               COUNT connections one after another, each sending a byte,
//!                                          taking it back and seeing the server close
//!     net stream MODE MEGABYTES BUFFER     MEGABYTES megabytes sent one way, BUFFER bytes at a time,
//!                                          and received BUFFER bytes at a time
//!     net lines MODE FILE                  FILE sent over a connection in pieces of 64 KB and read
//!                                          on the other side line by line
//!     net udp MODE SIZE CLIENTS ROUNDS     CLIENTS sockets, each sending datagrams of SIZE bytes to
//!                                          one that sends each back, ROUNDS times
//!     net address FILE                     each line of FILE read as an address with its port and
//!                                          written back into a text kept from one to the next
//!     net print FILE OUT                   the same, each address written into OUT on a line
//!
//! The clients of `echo` connect one after another and then all talk at once. The server closes each
//! connection once it has sent back all that a client sends, and the client sees it close, so the
//! wait of TCP after a close (TIME_WAIT) is on the server's side and the ports of the clients are
//! free again (choice 1684). What is printed is counted from what came back: the last byte of each
//! message is the number of its round, so the sums say that every message came back whole and in
//! order. A wrong use is told on the standard
//! error, with exit code 2; a failure of the network, with exit code 1.

const std = @import("std");
const Io = std.Io;
const net = Io.net;
const Allocator = std.mem.Allocator;

const usage_text =
    \\usage: net echo MODE SIZE CLIENTS ROUNDS | connect MODE COUNT | stream MODE MEGABYTES BUFFER | lines MODE FILE | udp MODE SIZE CLIENTS ROUNDS | address FILE | print FILE OUT
    \\       MODE is async or blocking
    \\
;

const loopback = "127.0.0.1:0";

/// The bytes of `stream`: byte `i` of the buffer, sent again and again.
fn pattern(i: usize) u8 {
    return @truncate(i *% 31 +% 7);
}

/// What has come, into `buffer`: how many bytes, 0 at the end.
fn receive(io: Io, s: net.Stream, buffer: []u8) !usize {
    var parts = [1][]u8{buffer};
    return s.read(io, &parts);
}

/// All of `bytes`, sent.
fn send(io: Io, s: net.Stream, bytes: []const u8) !void {
    var w = s.writer(io, &.{});
    try w.interface.writeAll(bytes);
}

fn listen(io: Io) !net.Server {
    const address = try net.IpAddress.parseLiteral(loopback);
    return address.listen(io, .{});
}

// ---- echo

// What a client sends, `total` bytes, sent back; then the connection is let go.
fn echoBack(io: Io, gpa: Allocator, conn: net.Stream, size: usize, total: usize) !void {
    defer conn.close(io);
    const buffer = try gpa.alloc(u8, size);
    defer gpa.free(buffer);
    var left = total;
    while (left > 0) {
        const n = try receive(io, conn, buffer);
        if (n == 0) return error.ConnectionResetByPeer;
        try send(io, conn, buffer[0..n]);
        left -= n;
    }
}

fn serveEcho(io: Io, gpa: Allocator, server: *net.Server, clients: usize, size: usize, rounds: usize) !void {
    var tasks: std.ArrayList(Io.Future(@typeInfo(@TypeOf(echoBack)).@"fn".return_type.?)) = .empty;
    defer tasks.deinit(gpa);
    for (0..clients) |_| {
        const conn = try server.accept(io);
        try tasks.append(gpa, try io.concurrent(echoBack, .{ io, gpa, conn, size, size * rounds }));
    }
    for (tasks.items) |*t| try t.await(io);
}

fn ask(io: Io, gpa: Allocator, conn: net.Stream, size: usize, rounds: usize) !i64 {
    defer conn.close(io);
    const message = try gpa.alloc(u8, size);
    defer gpa.free(message);
    @memset(message, 7);
    const back = try gpa.alloc(u8, size);
    defer gpa.free(back);
    var sum: i64 = 0;
    for (0..rounds) |round| {
        message[size - 1] = @truncate(round);
        try send(io, conn, message);
        var got: usize = 0;
        while (got < size) {
            const n = try receive(io, conn, back[got..]);
            if (n == 0) return error.ConnectionResetByPeer;
            got += n;
        }
        sum += back[size - 1];
    }
    // The end: the server has let the connection go.
    if (try receive(io, conn, back) != 0) return error.ConnectionResetByPeer;
    return sum;
}

fn echo(io: Io, gpa: Allocator, size: usize, clients: usize, rounds: usize) !i64 {
    var server = try listen(io);
    defer server.deinit(io);
    const address = server.socket.address;
    var serving = try io.concurrent(serveEcho, .{ io, gpa, &server, clients, size, rounds });
    var asking: std.ArrayList(Io.Future(@typeInfo(@TypeOf(ask)).@"fn".return_type.?)) = .empty;
    defer asking.deinit(gpa);
    for (0..clients) |_| {
        const conn = try address.connect(io, .{ .mode = .stream });
        try asking.append(gpa, try io.concurrent(ask, .{ io, gpa, conn, size, rounds }));
    }
    var sum: i64 = 0;
    for (asking.items) |*t| sum += try t.await(io);
    try serving.await(io);
    return sum;
}

// ---- connect: the server takes a connection, sends its byte back and lets it go, so the wait of
// TCP after a close (TIME_WAIT) is on its side and the ports of the clients are free again.

fn serveConnect(io: Io, server: *net.Server, count: usize) !void {
    var buffer: [1]u8 = undefined;
    for (0..count) |_| {
        const conn = try server.accept(io);
        defer conn.close(io);
        const n = try receive(io, conn, &buffer);
        try send(io, conn, buffer[0..n]);
    }
}

fn connect(io: Io, count: usize) !i64 {
    var server = try listen(io);
    defer server.deinit(io);
    const address = server.socket.address;
    var serving = try io.concurrent(serveConnect, .{ io, &server, count });
    var buffer: [1]u8 = undefined;
    var bytes: i64 = 0;
    for (0..count) |_| {
        const conn = try address.connect(io, .{ .mode = .stream });
        defer conn.close(io);
        try send(io, conn, &.{42});
        bytes += @intCast(try receive(io, conn, &buffer));
        bytes += @intCast(try receive(io, conn, &buffer));
    }
    try serving.await(io);
    return bytes;
}

// ---- stream: what was received, and the sum of every 64th byte of it.

const Streamed = struct { bytes: i64, sum: i64 };

fn sendStream(io: Io, gpa: Allocator, address: net.IpAddress, total: usize, size: usize) !void {
    const conn = try address.connect(io, .{ .mode = .stream });
    defer conn.close(io);
    const buffer = try gpa.alloc(u8, size);
    defer gpa.free(buffer);
    for (buffer, 0..) |*b, i| b.* = pattern(i);
    var sent: usize = 0;
    while (sent < total) : (sent += size) try send(io, conn, buffer);
}

fn stream(io: Io, gpa: Allocator, megabytes: usize, size: usize) !Streamed {
    var server = try listen(io);
    defer server.deinit(io);
    var sender = try io.concurrent(sendStream, .{ io, gpa, server.socket.address, megabytes * 1048576, size });
    const conn = try server.accept(io);
    defer conn.close(io);
    const buffer = try gpa.alloc(u8, size);
    defer gpa.free(buffer);
    var bytes: i64 = 0;
    var sum: i64 = 0;
    while (true) {
        const n = try receive(io, conn, buffer);
        if (n == 0) break;
        var at: usize = @intCast(@mod(64 - @mod(bytes, 64), 64));
        while (at < n) : (at += 64) sum += buffer[at];
        bytes += @intCast(n);
    }
    try sender.await(io);
    return .{ .bytes = bytes, .sum = sum };
}

// ---- lines: the file is read whole first, then sent in pieces of 64 KB.

const Lines = struct { lines: i64 = 0, bytes: i64 = 0, longest: i64 = 0 };

fn sendLines(io: Io, address: net.IpAddress, text: []const u8) !void {
    const conn = try address.connect(io, .{ .mode = .stream });
    defer conn.close(io);
    var at: usize = 0;
    while (at < text.len) {
        const end = @min(at + 65536, text.len);
        try send(io, conn, text[at..end]);
        at = end;
    }
}

fn lines(io: Io, text: []const u8) !Lines {
    var server = try listen(io);
    defer server.deinit(io);
    var sender = try io.concurrent(sendLines, .{ io, server.socket.address, text });
    const conn = try server.accept(io);
    defer conn.close(io);
    var buffer: [65536]u8 = undefined;
    var stream_reader = conn.reader(io, &buffer);
    const r = &stream_reader.interface;
    var counts: Lines = .{};
    while (try r.takeDelimiter('\n')) |taken| {
        const line = if (taken.len > 0 and taken[taken.len - 1] == '\r') taken[0 .. taken.len - 1] else taken;
        counts.lines += 1;
        const length: i64 = @intCast(line.len);
        counts.bytes += length;
        counts.longest = @max(counts.longest, length);
    }
    try sender.await(io);
    return counts;
}

// ---- udp: one socket sends each datagram back where it came from.

fn bindUdp(io: Io) !net.Socket {
    const address = try net.IpAddress.parseLiteral(loopback);
    return address.bind(io, .{ .mode = .dgram });
}

fn serveUdp(io: Io, gpa: Allocator, socket: net.Socket, count: usize, size: usize) !void {
    const buffer = try gpa.alloc(u8, size);
    defer gpa.free(buffer);
    for (0..count) |_| {
        const got = try socket.receive(io, buffer);
        try socket.send(io, &got.from, got.data);
    }
}

fn askUdp(io: Io, gpa: Allocator, to: net.IpAddress, size: usize, rounds: usize) !i64 {
    const socket = try bindUdp(io);
    defer socket.close(io);
    const message = try gpa.alloc(u8, size);
    defer gpa.free(message);
    @memset(message, 7);
    const back = try gpa.alloc(u8, size);
    defer gpa.free(back);
    var sum: i64 = 0;
    for (0..rounds) |round| {
        message[size - 1] = @truncate(round);
        try socket.send(io, &to, message);
        const got = try socket.receive(io, back);
        sum += got.data[got.data.len - 1];
    }
    return sum;
}

fn udp(io: Io, gpa: Allocator, size: usize, clients: usize, rounds: usize) !i64 {
    const socket = try bindUdp(io);
    defer socket.close(io);
    var serving = try io.concurrent(serveUdp, .{ io, gpa, socket, clients * rounds, size });
    var asking: std.ArrayList(Io.Future(@typeInfo(@TypeOf(askUdp)).@"fn".return_type.?)) = .empty;
    defer asking.deinit(gpa);
    for (0..clients) |_| {
        try asking.append(gpa, try io.concurrent(askUdp, .{ io, gpa, socket.address, size, rounds }));
    }
    var sum: i64 = 0;
    for (asking.items) |*t| sum += try t.await(io);
    try serving.await(io);
    return sum;
}

// ---- addresses

fn addresses(io: Io, path: []const u8, out: *Io.Writer) !void {
    const file = try Io.Dir.cwd().openFile(io, path, .{});
    defer file.close(io);
    var in_buffer: [65536]u8 = undefined;
    var file_reader = file.readerStreaming(io, &in_buffer);
    const r = &file_reader.interface;
    var text_buffer: [64]u8 = undefined;
    var count: i64 = 0;
    var v6: i64 = 0;
    var ports: i64 = 0;
    var characters: i64 = 0;
    var sum: u64 = 0;
    var invalid: i64 = 0;
    while (try r.takeDelimiter('\n')) |taken| {
        const line = if (taken.len > 0 and taken[taken.len - 1] == '\r') taken[0 .. taken.len - 1] else taken;
        const address = net.IpAddress.parseLiteral(line) catch {
            invalid += 1;
            continue;
        };
        var w: Io.Writer = .fixed(&text_buffer);
        try address.format(&w);
        const text = w.buffered();
        if (address == .ip6) v6 += 1;
        ports += address.getPort();
        characters += @intCast(text.len);
        sum += text[@as(usize, @intCast(count)) % text.len];
        count += 1;
    }
    try out.print("addresses {d}\nv6 {d}\nports {d}\ncharacters {d}\nsum {d}\ninvalid {d}\n", .{ count, v6, ports, characters, sum, invalid });
}

fn printAddresses(io: Io, path: []const u8, out_path: []const u8) !void {
    const file = try Io.Dir.cwd().openFile(io, path, .{});
    defer file.close(io);
    var in_buffer: [65536]u8 = undefined;
    var file_reader = file.readerStreaming(io, &in_buffer);
    const r = &file_reader.interface;
    const out_file = try Io.Dir.cwd().createFile(io, out_path, .{});
    defer out_file.close(io);
    var out_buffer: [65536]u8 = undefined;
    var file_writer = out_file.writer(io, &out_buffer);
    const w = &file_writer.interface;
    while (try r.takeDelimiter('\n')) |taken| {
        const line = if (taken.len > 0 and taken[taken.len - 1] == '\r') taken[0 .. taken.len - 1] else taken;
        if (net.IpAddress.parseLiteral(line)) |address| {
            try w.print("{f}\n", .{address});
        } else |_| {
            try w.writeAll("invalid\n");
        }
    }
    try file_writer.end();
}

// ---- the command line

fn usage() u8 {
    std.debug.print(usage_text, .{});
    return 2;
}

/// The number at `args[at]`, from 1 up to `most`, or null.
fn number(args: []const []const u8, at: usize, most: usize) ?usize {
    const value = std.fmt.parseInt(usize, args[at], 10) catch return null;
    if (value < 1 or value > most) return null;
    return value;
}

fn fail(err: anyerror) u8 {
    std.debug.print("net: {s}\n", .{@errorName(err)});
    return 1;
}

pub fn main(init: std.process.Init) !u8 {
    const io = init.io;
    const gpa = init.gpa;
    const arena = init.arena.allocator();
    const args = try init.minimal.args.toSlice(arena);
    if (args.len < 3) return usage();
    var out_buffer: [4096]u8 = undefined;
    var out_file: Io.File.Writer = .init(.stdout(), io, &out_buffer);
    const out = &out_file.interface;
    const command = args[1];
    if (std.mem.eql(u8, command, "address") and args.len == 3) {
        addresses(io, args[2], out) catch |err| return fail(err);
        try out.flush();
        return 0;
    }
    if (std.mem.eql(u8, command, "print") and args.len == 4) {
        printAddresses(io, args[2], args[3]) catch |err| return fail(err);
        return 0;
    }
    if (!std.mem.eql(u8, args[2], "async") and !std.mem.eql(u8, args[2], "blocking")) return usage();
    const is_echo = std.mem.eql(u8, command, "echo");
    if ((is_echo or std.mem.eql(u8, command, "udp")) and args.len == 6) {
        const size = number(args, 3, 65536) orelse return usage();
        const clients = number(args, 4, 100000) orelse return usage();
        const rounds = number(args, 5, 1000000000) orelse return usage();
        if (!is_echo and size > 8192) return usage();
        const sum = (if (is_echo) echo(io, gpa, size, clients, rounds) else udp(io, gpa, size, clients, rounds)) catch |err| return fail(err);
        const messages = clients * rounds;
        try out.print("{s} {d}\nmessages {d}\nbytes {d}\nsum {d}\n", .{ if (is_echo) "connections" else "sockets", clients, messages, messages * size, sum });
    } else if (std.mem.eql(u8, command, "connect") and args.len == 4) {
        const count = number(args, 3, 1000000000) orelse return usage();
        const bytes = connect(io, count) catch |err| return fail(err);
        try out.print("connections {d}\nbytes {d}\n", .{ count, bytes });
    } else if (std.mem.eql(u8, command, "stream") and args.len == 5) {
        const megabytes = number(args, 3, 1000000) orelse return usage();
        const size = number(args, 4, 16777216) orelse return usage();
        if (1048576 % size != 0) return usage();
        const streamed = stream(io, gpa, megabytes, size) catch |err| return fail(err);
        try out.print("bytes {d}\nsum {d}\n", .{ streamed.bytes, streamed.sum });
    } else if (std.mem.eql(u8, command, "lines") and args.len == 4) {
        const text = Io.Dir.cwd().readFileAlloc(io, args[3], gpa, .unlimited) catch |err| {
            std.debug.print("net: {s}: {s}\n", .{ args[3], @errorName(err) });
            return 1;
        };
        defer gpa.free(text);
        const counts = lines(io, text) catch |err| return fail(err);
        try out.print("lines {d}\nbytes {d}\nlongest {d}\n", .{ counts.lines, counts.bytes, counts.longest });
    } else {
        return usage();
    }
    try out.flush();
    return 0;
}
