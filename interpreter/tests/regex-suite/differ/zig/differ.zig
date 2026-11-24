//! The Zig side of the differential test of SG's `Regex` (choice 1841), with PCRE2, Zig having no
//! regular expressions: reads cases, a line each, the hex of a pattern, a space, the hex of a text,
//! and writes for each what `sg/differ.sg` writes. PCRE2 runs in UTF with `$` only at the end and
//! `^` after a last line break too, as SG's are; with `bytes`, with `MATCH_INVALID_UTF`, where a
//! byte that is not UTF-8 matches nothing. The matches one after another are found as SG's and
//! Rust's are: an empty match where the last one ended is passed over, and the search goes on one
//! character further, or one byte where the bytes are not UTF-8.
const std = @import("std");
const Io = std.Io;
const c = @import("pcre2");

const unset = std.math.maxInt(usize);

fn next(t: []const u8, at: usize) usize {
    if (at >= t.len) return at + 1;
    const n = std.unicode.utf8ByteSequenceLength(t[at]) catch return at + 1;
    if (at + n > t.len) return at + 1;
    _ = std.unicode.utf8Decode(t[at .. at + n]) catch return at + 1;
    return at + n;
}

pub fn main(init: std.process.Init) !void {
    const io = init.io;
    const gpa = std.heap.c_allocator;
    const in_buffer = try gpa.alloc(u8, 16 << 20);
    var in_file = Io.File.stdin().readerStreaming(io, in_buffer);
    const r = &in_file.interface;
    var out_buffer: [65536]u8 = undefined;
    var out_file: Io.File.Writer = .init(.stdout(), io, &out_buffer);
    const out = &out_file.interface;
    const args = try init.minimal.args.toSlice(init.arena.allocator());
    const bytes = args.len > 1 and std.mem.eql(u8, args[1], "bytes");
    const options: u32 = c.PCRE2_UTF | c.PCRE2_DOLLAR_ENDONLY | c.PCRE2_ALT_CIRCUMFLEX | (if (bytes) c.PCRE2_MATCH_INVALID_UTF else 0);
    while (try r.takeDelimiter('\n')) |line| {
        const space = std.mem.indexOfScalar(u8, line, ' ').?;
        const p_owned = try std.fmt.hexToBytes(try gpa.alloc(u8, space / 2), line[0..space]);
        defer gpa.free(p_owned);
        const p: []const u8 = if (p_owned.len == 0) "" else p_owned;
        // An empty text is given as a real empty string: PCRE2 is handed the pointer Zig gives an
        // allocation of no bytes otherwise, which it reads.
        const t_owned = try std.fmt.hexToBytes(try gpa.alloc(u8, (line.len - space - 1) / 2), line[space + 1 ..]);
        defer gpa.free(t_owned);
        const t: []const u8 = if (t_owned.len == 0) "" else t_owned;
        var error_code: c_int = 0;
        var error_offset: usize = 0;
        const code = c.pcre2_compile_8(p.ptr, p.len, options, &error_code, &error_offset, null) orelse {
            var message: [256]u8 = undefined;
            const n = c.pcre2_get_error_message_8(error_code, &message, message.len);
            try out.print("E {s}\n", .{message[0..@intCast(@max(n, 0))]});
            continue;
        };
        defer c.pcre2_code_free_8(code);
        const data = c.pcre2_match_data_create_from_pattern_8(code, null).?;
        defer c.pcre2_match_data_free_8(data);
        const ov = c.pcre2_get_ovector_pointer_8(data);
        var groups: u32 = 0;
        _ = c.pcre2_pattern_info_8(code, c.PCRE2_INFO_CAPTURECOUNT, &groups);

        var rc = c.pcre2_match_8(code, t.ptr, t.len, 0, 0, data, null);
        if (rc < 0 and rc != c.PCRE2_ERROR_NOMATCH) {
            try out.print("L {d}\n", .{rc});
            continue;
        }
        try out.writeAll(if (rc >= 0) "y " else "n ");
        var pos: usize = 0;
        var previous_end: usize = unset;
        var first = true;
        var limited = false;
        while (pos <= t.len) {
            rc = c.pcre2_match_8(code, t.ptr, t.len, pos, 0, data, null);
            if (rc == c.PCRE2_ERROR_NOMATCH) break;
            if (rc < 0) {
                limited = true;
                break;
            }
            const s = ov[0];
            const e = ov[1];
            var accept = true;
            if (e == pos) {
                if (s == previous_end) accept = false;
                pos = next(t, pos);
            } else {
                pos = e;
            }
            previous_end = e;
            if (accept) {
                if (!first) try out.writeByte(',');
                first = false;
                try out.print("{d}-{d}", .{ s, e });
            }
        }
        if (limited) try out.writeAll("LIMIT");
        try out.writeAll(" |");
        rc = c.pcre2_match_8(code, t.ptr, t.len, 0, 0, data, null);
        if (rc >= 0) {
            var g: usize = 0;
            while (g <= groups) : (g += 1) {
                if (ov[2 * g] == unset) {
                    try out.writeAll(" -");
                } else {
                    try out.print(" {d}-{d}", .{ ov[2 * g], ov[2 * g + 1] });
                }
            }
        }
        try out.writeByte('\n');
    }
    try out.flush();
}
