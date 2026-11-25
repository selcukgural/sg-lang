//! The file server of the server lane in Zig (choices 1861-1871): GET and HEAD of the files under a
//! folder, over HTTP/1.1 with connections kept open, by `std.http.Server`, as `../c/files.c`
//! describes it for all four versions. `std.http.Server` reads and checks the request, keeps or
//! closes the connection as `Connection` says, and leaves the body out for HEAD.
//!
//!     files PORT ROOT
//!
//! Each connection is a task of an `Io.Group` started with `concurrent` on `Io.Threaded`, a thread,
//! as in `../../http/zig/http.zig`. [`uring.zig`](uring.zig) runs the same server on `Io.Evented`,
//! Zig's io_uring. The disk is asked through `Io` (`statFile`, `openFile`,
//! `readPositional`) on that thread, and the body is read into a buffer of 64 KB and written, a
//! piece at a time: never with `sendFile`, which the other three do not use (choice 1864).

const std = @import("std");
const Io = std.Io;
const net = Io.net;
const http = std.http;

const piece = 65536;
const far: i64 = 1 << 62;

// ---- HTTP dates

const days = "MonTueWedThuFriSatSun";
const months = "JanFebMarAprMayJunJulAugSepOctNovDec";

fn daysFromCivil(y0: i64, m: i64, d: i64) i64 {
    const y = y0 - @as(i64, @intFromBool(m <= 2));
    const era = @divFloor(y, 400);
    const yoe = y - era * 400;
    const doy = @divTrunc(153 * (m + @as(i64, if (m > 2) -3 else 9)) + 2, 5) + d - 1;
    const doe = yoe * 365 + @divTrunc(yoe, 4) - @divTrunc(yoe, 100) + doy;
    return era * 146097 + doe - 719468;
}

/// `Sun, 06 Nov 1994 08:49:37 GMT` into `out`
fn httpDate(out: *[29]u8, seconds: i64) void {
    const dn = @divFloor(seconds, 86400);
    const rest = @mod(seconds, 86400);
    const z = dn + 719468;
    const era = @divFloor(z, 146097);
    const doe = z - era * 146097;
    const yoe = @divTrunc(doe - @divTrunc(doe, 1460) + @divTrunc(doe, 36524) - @divTrunc(doe, 146096), 365);
    const doy = doe - (365 * yoe + @divTrunc(yoe, 4) - @divTrunc(yoe, 100));
    const mp = @divTrunc(5 * doy + 2, 153);
    const d = doy - @divTrunc(153 * mp + 2, 5) + 1;
    const m = if (mp < 10) mp + 3 else mp - 9;
    const y = yoe + era * 400 + @as(i64, @intFromBool(m <= 2));
    const w: usize = @intCast(@mod(dn + 3, 7));
    const mi: usize = @intCast(m - 1);
    _ = std.fmt.bufPrint(out, "{s}, {d:0>2} {s} {d:0>4} {d:0>2}:{d:0>2}:{d:0>2} GMT", .{
        days[w * 3 .. w * 3 + 3], @as(u64, @intCast(d)), months[mi * 3 .. mi * 3 + 3], @as(u64, @intCast(y)),
        @as(u64, @intCast(@divTrunc(rest, 3600))), @as(u64, @intCast(@mod(@divTrunc(rest, 60), 60))), @as(u64, @intCast(@mod(rest, 60))),
    }) catch unreachable;
}

const Text = struct {
    t: []const u8,
    at: usize = 0,

    fn digits(x: *Text, count: usize) ?i64 {
        var v: i64 = 0;
        for (0..count) |_| {
            if (x.at >= x.t.len or x.t[x.at] < '0' or x.t[x.at] > '9') return null;
            v = v * 10 + (x.t[x.at] - '0');
            x.at += 1;
        }
        return v;
    }

    fn expect(x: *Text, c: u8) ?void {
        if (x.at >= x.t.len or x.t[x.at] != c) return null;
        x.at += 1;
    }

    fn name(x: *Text, names: []const u8, width: usize) ?i64 {
        if (x.at + width > x.t.len) return null;
        var i: usize = 0;
        while (i * width < names.len) : (i += 1) {
            if (std.mem.eql(u8, x.t[x.at .. x.at + width], names[i * width .. i * width + width])) {
                x.at += width;
                return @intCast(i);
            }
        }
        return null;
    }
};

/// An HTTP date (RFC 9110: IMF-fixdate, RFC 850, asctime) into seconds since 1970, as SG's
/// `DateTime.parseHttpDate` reads it.
fn parseHttpDate(t: []const u8) ?i64 {
    var x: Text = .{ .t = t };
    var named: i64 = undefined;
    var day: i64 = undefined;
    var month: i64 = undefined;
    var year: i64 = 0;
    const asctime = t.len > 3 and t[3] == ' ';
    if (t.len > 3 and t[3] == ',') {
        named = x.name(days, 3) orelse return null;
        x.expect(',') orelse return null;
        x.expect(' ') orelse return null;
        day = x.digits(2) orelse return null;
        x.expect(' ') orelse return null;
        month = (x.name(months, 3) orelse return null) + 1;
        x.expect(' ') orelse return null;
        year = x.digits(4) orelse return null;
        x.expect(' ') orelse return null;
    } else if (asctime) {
        named = x.name(days, 3) orelse return null;
        x.expect(' ') orelse return null;
        month = (x.name(months, 3) orelse return null) + 1;
        x.expect(' ') orelse return null;
        if (x.at < t.len and t[x.at] == ' ') {
            x.at += 1;
            day = x.digits(1) orelse return null;
        } else {
            day = x.digits(2) orelse return null;
        }
        x.expect(' ') orelse return null;
    } else {
        named = x.name(days, 3) orelse return null;
        const rests = [_][]const u8{ "day", "sday", "nesday", "rsday", "day", "urday", "day" };
        const rest = rests[@intCast(named)];
        if (x.at + rest.len > t.len or !std.mem.eql(u8, t[x.at .. x.at + rest.len], rest)) return null;
        x.at += rest.len;
        x.expect(',') orelse return null;
        x.expect(' ') orelse return null;
        day = x.digits(2) orelse return null;
        x.expect('-') orelse return null;
        month = (x.name(months, 3) orelse return null) + 1;
        x.expect('-') orelse return null;
        const yy = x.digits(2) orelse return null;
        year = yy + @as(i64, if (yy < 70) 2000 else 1900);
        x.expect(' ') orelse return null;
    }
    const hour = x.digits(2) orelse return null;
    x.expect(':') orelse return null;
    const minute = x.digits(2) orelse return null;
    x.expect(':') orelse return null;
    const second = x.digits(2) orelse return null;
    x.expect(' ') orelse return null;
    if (asctime) {
        year = x.digits(4) orelse return null;
    } else {
        x.expect('G') orelse return null;
        x.expect('M') orelse return null;
        x.expect('T') orelse return null;
    }
    if (x.at != t.len) return null;
    const leap = @mod(year, 4) == 0 and (@mod(year, 100) != 0 or @mod(year, 400) == 0);
    const lengths = [_]i64{ 31, if (leap) 29 else 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31 };
    if (day < 1 or day > lengths[@intCast(month - 1)] or hour > 23 or minute > 59 or second > 59 or year < 1000 or year > 3000) return null;
    const dn = daysFromCivil(year, month, day);
    const s = dn * 86400 + hour * 3600 + minute * 60 + second;
    if (s < -9223372036 or s > 9223372036 or @mod(dn + 3, 7) != named) return null;
    return s;
}

// ---- what is asked

fn hex(c: u8) ?u8 {
    return switch (c) {
        '0'...'9' => c - '0',
        'a'...'f' => c - 'a' + 10,
        'A'...'F' => c - 'A' + 10,
        else => null,
    };
}

/// The path of `target` percent-decoded into `out` and checked as `../c/files.c` says; `null` for 400.
fn decodePath(target: []const u8, out: []u8) ?[]u8 {
    const raw = target[0 .. std.mem.indexOfScalar(u8, target, '?') orelse target.len];
    if (raw.len == 0 or raw[0] != '/') return null;
    for (target) |c| if (c < 0x21 or c > 0x7e) return null;
    var o: usize = 0;
    var i: usize = 0;
    while (i < raw.len) : (i += 1) {
        var c = raw[i];
        if (c == '%') {
            if (i + 2 >= raw.len) return null;
            c = ((hex(raw[i + 1]) orelse return null) << 4) | (hex(raw[i + 2]) orelse return null);
            if (c == '/') return null;
            i += 2;
        }
        if (c == 0 or c == '\\') return null;
        if (o == out.len) return null;
        out[o] = c;
        o += 1;
    }
    const path = out[0..o];
    var start: usize = 1;
    var k: usize = 1;
    while (k <= o) : (k += 1) {
        if (k < o and path[k] != '/') continue;
        const segment = path[start..k];
        if ((segment.len == 0 and k != o) or std.mem.eql(u8, segment, ".") or std.mem.eql(u8, segment, "..")) return null;
        start = k + 1;
    }
    if (!std.unicode.utf8ValidateSlice(path)) return null;
    return path;
}

fn typeOf(path: []const u8) []const u8 {
    const slash = std.mem.lastIndexOfScalar(u8, path, '/') orelse 0;
    const dot = std.mem.lastIndexOfScalar(u8, path, '.') orelse 0;
    if (dot <= slash) return "application/octet-stream";
    const ext = path[dot + 1 ..];
    const table = [_][2][]const u8{
        .{ "html", "text/html; charset=utf-8" },   .{ "htm", "text/html; charset=utf-8" },       .{ "css", "text/css; charset=utf-8" },
        .{ "js", "text/javascript; charset=utf-8" }, .{ "mjs", "text/javascript; charset=utf-8" }, .{ "json", "application/json" },
        .{ "txt", "text/plain; charset=utf-8" },   .{ "xml", "application/xml" },                .{ "svg", "image/svg+xml" },
        .{ "png", "image/png" },                   .{ "jpg", "image/jpeg" },                     .{ "jpeg", "image/jpeg" },
        .{ "gif", "image/gif" },                   .{ "webp", "image/webp" },                    .{ "ico", "image/x-icon" },
        .{ "wasm", "application/wasm" },           .{ "pdf", "application/pdf" },                .{ "zip", "application/zip" },
        .{ "gz", "application/gzip" },             .{ "mp4", "video/mp4" },                      .{ "webm", "video/webm" },
        .{ "mp3", "audio/mpeg" },                  .{ "woff2", "font/woff2" },                   .{ "woff", "font/woff" },
    };
    for (table) |entry| if (std.ascii.eqlIgnoreCase(ext, entry[0])) return entry[1];
    return "application/octet-stream";
}

fn digitsFar(v: []const u8, at: *usize) ?i64 {
    const start = at.*;
    var x: i64 = 0;
    while (at.* < v.len and v[at.*] >= '0' and v[at.*] <= '9') : (at.* += 1) {
        x = if (x > @divTrunc(far, 10)) far else @min(x * 10 + (v[at.*] - '0'), far);
    }
    return if (at.* > start) x else null;
}

const Wanted = union(enum) { whole, from: [2]i64, last: i64 };

fn rangeOf(v: []const u8) Wanted {
    if (!std.mem.startsWith(u8, v, "bytes=")) return .whole;
    const rest = v[6..];
    var at: usize = 0;
    const first = digitsFar(rest, &at);
    if (at >= rest.len or rest[at] != '-') return .whole;
    at += 1;
    const last = digitsFar(rest, &at);
    if (at != rest.len) return .whole;
    if (first == null) return if (last) |count| .{ .last = count } else .whole;
    if (last == null) return .{ .from = .{ first.?, far } };
    if (first.? > last.?) return .whole;
    return .{ .from = .{ first.?, last.? } };
}

// ---- answers

fn simple(request: *http.Server.Request, keep: bool, status: http.Status, body: []const u8, extra: ?http.Header) !void {
    var headers: [2]http.Header = .{ .{ .name = "content-type", .value = "text/plain" }, undefined };
    var count: usize = 1;
    if (extra) |e| {
        headers[1] = e;
        count = 2;
    }
    try request.respond(body, .{ .status = status, .keep_alive = keep, .extra_headers = headers[0..count] });
}

fn failed(request: *http.Server.Request, keep: bool, err: anyerror) !void {
    return switch (err) {
        error.AccessDenied, error.PermissionDenied => simple(request, keep, .forbidden, "forbidden", null),
        else => simple(request, keep, .not_found, "not found", null),
    };
}

fn header(request: *const http.Server.Request, name: []const u8) ?[]const u8 {
    var it = request.iterateHeaders();
    while (it.next()) |h| if (std.ascii.eqlIgnoreCase(h.name, name)) return h.value;
    return null;
}

fn answer(io: Io, root: []const u8, request: *http.Server.Request, keep: bool) !void {
    const head = request.head.method == .HEAD;
    if (request.head.method != .GET and !head) {
        return simple(request, keep, .method_not_allowed, "method not allowed", .{ .name = "allow", .value = "GET, HEAD" });
    }
    var full_buffer: [4096 + 8192 + 16]u8 = undefined;
    @memcpy(full_buffer[0..root.len], root);
    const target = request.head.target;
    const path = decodePath(target, full_buffer[root.len .. full_buffer.len - 11]) orelse
        return simple(request, keep, .bad_request, "bad request", null);
    var len = root.len + path.len;
    const slash = path[path.len - 1] == '/';
    if (slash) {
        @memcpy(full_buffer[len .. len + 10], "index.html");
        len += 10;
    }
    const full = full_buffer[0..len];
    const info = Io.Dir.cwd().statFile(io, full, .{}) catch |err| return failed(request, keep, err);
    if (info.kind == .directory) {
        if (slash) return simple(request, keep, .not_found, "not found", null);
        var location_buffer: [8200]u8 = undefined;
        const written = target[0 .. std.mem.indexOfScalar(u8, target, '?') orelse target.len];
        const location = try std.fmt.bufPrint(&location_buffer, "{s}/", .{written});
        return simple(request, keep, .moved_permanently, "moved permanently", .{ .name = "location", .value = location });
    }
    if (info.kind != .file) return simple(request, keep, .not_found, "not found", null);
    const mtime: i64 = @intCast(@divFloor(info.mtime.toNanoseconds(), std.time.ns_per_s));
    var modified: [29]u8 = undefined;
    httpDate(&modified, mtime);
    if (header(request, "if-modified-since")) |value| {
        if (parseHttpDate(value)) |since| if (mtime <= since) {
            return request.respond("", .{ .status = .not_modified, .keep_alive = keep, .extra_headers = &.{.{ .name = "last-modified", .value = &modified }} });
        };
    }
    const size: i64 = @intCast(info.size);
    var start: i64 = 0;
    var end: i64 = size - 1;
    const wanted: Wanted = if (header(request, "if-range") == null) (if (header(request, "range")) |v| rangeOf(v) else .whole) else .whole;
    const partial = wanted != .whole;
    switch (wanted) {
        .whole => {},
        .from => |r| if (r[0] < size) {
            start = r[0];
            end = @min(end, r[1]);
        },
        .last => |count| if (count > 0 and size > 0) {
            start = size - @min(count, size);
        },
    }
    const satisfiable = switch (wanted) {
        .whole => true,
        .from => |r| r[0] < size,
        .last => |count| count > 0 and size > 0,
    };
    if (!satisfiable) {
        var range_buffer: [48]u8 = undefined;
        const range = try std.fmt.bufPrint(&range_buffer, "bytes */{d}", .{size});
        return simple(request, keep, .range_not_satisfiable, "range not satisfiable", .{ .name = "content-range", .value = range });
    }
    const file = Io.Dir.cwd().openFile(io, full, .{}) catch |err| return failed(request, keep, err);
    defer file.close(io);
    const length = end - start + 1;
    var range_buffer: [80]u8 = undefined;
    var headers = [_]http.Header{
        .{ .name = "content-type", .value = typeOf(full) },
        .{ .name = "last-modified", .value = &modified },
        .{ .name = "accept-ranges", .value = "bytes" },
        undefined,
    };
    var count: usize = 3;
    if (partial) {
        headers[3] = .{ .name = "content-range", .value = try std.fmt.bufPrint(&range_buffer, "bytes {d}-{d}/{d}", .{ start, end, size }) };
        count = 4;
    }
    var body = try request.respondStreaming(&.{}, .{
        .content_length = @intCast(length),
        .respond_options = .{ .status = if (partial) .partial_content else .ok, .keep_alive = keep, .extra_headers = headers[0..count] },
    });
    if (!head) {
        var buffer: [piece]u8 = undefined;
        var at: u64 = @intCast(start);
        var left: u64 = @intCast(length);
        while (left > 0) {
            const want: usize = @intCast(@min(left, piece));
            const n = try file.readPositional(io, &.{buffer[0..want]}, at);
            // A file found shorter than its length: the answer is cut, and the connection closed.
            if (n == 0) return error.FileShorter;
            try body.writer.writeAll(buffer[0..n]);
            at += n;
            left -= n;
        }
    }
    try body.end();
}

fn serveConnection(io: Io, root: []const u8, conn: net.Stream) !void {
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
        const keep = request.head.keep_alive;
        try answer(io, root, &request, keep);
        if (!keep) return;
    }
}

/// A connection that fails ends alone.
fn connection(io: Io, root: []const u8, conn: net.Stream) Io.Cancelable!void {
    defer conn.close(io);
    serveConnection(io, root, conn) catch {};
}

pub fn main(init: std.process.Init) !u8 {
    const arena = init.arena.allocator();
    const args = try init.minimal.args.toSlice(arena);
    const port = if (args.len == 3) std.fmt.parseInt(u16, args[1], 10) catch null else null;
    if (port == null) {
        std.debug.print("usage: files PORT ROOT\n", .{});
        return 2;
    }
    return run(init.io, args[2], port.?);
}

/// The server on `io`, until it is stopped.
pub fn run(io: Io, root_arg: []const u8, port: u16) !u8 {
    var root: []const u8 = root_arg;
    while (root.len > 1 and root[root.len - 1] == '/') root = root[0 .. root.len - 1];
    const kind = (Io.Dir.cwd().statFile(io, root, .{}) catch null);
    if (root.len > 4096 or kind == null or kind.?.kind != .directory) {
        std.debug.print("files: {s} is not a folder\n", .{root});
        return 2;
    }
    var address = try net.IpAddress.parseLiteral("0.0.0.0:0");
    address.setPort(port);
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
        group.concurrent(io, connection, .{ io, root, conn }) catch {
            conn.close(io);
        };
    }
}
