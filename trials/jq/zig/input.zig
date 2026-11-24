//! The input: the files named, or the standard input, read in pieces of 64 KB as one stream of
//! bytes, given piece by piece to a `std.json.Scanner`, whose tokens build the program's own
//! values straight away, with no tree of `std.json`'s in between.
//!
//! Each piece is checked to be UTF-8 before the scanner has it, and only a piece with a bad
//! sequence is written again: in each text, each bad sequence becomes U+FFFD as jq's decoder
//! makes it, in the text's bytes once its escapes are read (a sequence cut off by the end of the
//! text is one U+FFFD for all that is left of it). The scanner tells whether a piece starts
//! inside a text. A sequence cut off by the end of a piece waits for the next one.
//!
//! The scanner reads one document; once a value at the top is whole, a new scanner is given what
//! is left of the piece. A number or a word at the top must be followed by a space, the end, or
//! the start of another value, as jq's parser has it. The depth is counted as jq counts it: an
//! array is a level, an object one and its name another while its value is read; a bracket that
//! would open a level past 10,000 is an error. A token longer than a piece (a long text) is put
//! together in a list of its own, which grows by doubling.

const std = @import("std");
const Io = std.Io;
const Scanner = std.json.Scanner;
const value = @import("value.zig");
const Value = value.Value;
const gpa = value.gpa;
const oom = value.oom;

const piece = 64 * 1024;
const max_depth = 10_000;

pub const Error = error{BadInput};

fn isSpace(c: u8) bool {
    return c == ' ' or c == '\t' or c == '\r' or c == '\n';
}

/// The length of a UTF-8 sequence from its first byte; 0 for a byte that is bad alone.
fn sequenceLength(c: u8) usize {
    return switch (c) {
        0x00...0x7F => 1,
        0xC2...0xDF => 2,
        0xE0...0xEF => 3,
        0xF0...0xF4 => 4,
        else => 0,
    };
}

/// Where the bytes stop holding whole sequences: a sequence cut off at the end waits.
fn wholeSequences(bytes: []const u8) usize {
    var back: usize = 1;
    while (back <= 3 and back <= bytes.len) : (back += 1) {
        const c = bytes[bytes.len - back];
        if (c & 0xC0 == 0x80) continue;
        return if (sequenceLength(c) > back) bytes.len - back else bytes.len;
    }
    return bytes.len;
}

/// Where the bytes being written again stand: outside a text, in one, after its `\`, or in the
/// `n` hex digits of a `\u`.
const Place = union(enum) { outside, text, escape, hex: u8 };

/// The place where the scanner stopped.
fn placeOf(s: *const Scanner) Place {
    return switch (s.state) {
        .string, .string_surrogate_half => .text,
        .string_backslash, .string_surrogate_half_backslash => .escape,
        .string_backslash_u, .string_surrogate_half_backslash_u => .{ .hex = 4 },
        .string_backslash_u_1, .string_surrogate_half_backslash_u_1 => .{ .hex = 3 },
        .string_backslash_u_2, .string_surrogate_half_backslash_u_2 => .{ .hex = 2 },
        .string_backslash_u_3, .string_surrogate_half_backslash_u_3 => .{ .hex = 1 },
        else => .outside,
    };
}

/// After a first byte at `at - 1`, how many bytes of the text's own (escapes read) come before
/// its end, counted up to `want`; null when the bytes run out first and more may come.
fn bytesLeft(bytes: []const u8, at: usize, want: usize, ended: bool) ?struct { count: usize, text_ends: bool } {
    var i = at;
    var count: usize = 0;
    while (count < want) {
        if (i >= bytes.len) return if (ended) .{ .count = count, .text_ends = false } else null;
        switch (bytes[i]) {
            '"' => return .{ .count = count, .text_ends = true },
            '\\' => {
                if (i + 1 >= bytes.len) return if (ended) .{ .count = count, .text_ends = false } else null;
                if (bytes[i + 1] != 'u') {
                    count += 1;
                    i += 2;
                    continue;
                }
                if (i + 6 > bytes.len) return if (ended) .{ .count = count, .text_ends = false } else null;
                const cp = std.fmt.parseInt(u21, bytes[i + 2 .. i + 6], 16) catch 0;
                i += 6;
                if (cp >= 0xD800 and cp < 0xDC00) {
                    // A pair stands for four bytes; a high half alone is refused later anyway.
                    if (i + 6 > bytes.len and !ended) return null;
                    count += 4;
                    i += 6;
                } else {
                    count += std.unicode.utf8CodepointSequenceLength(cp) catch 3;
                }
            },
            else => {
                count += 1;
                i += 1;
            },
        }
    }
    return .{ .count = count, .text_ends = false };
}

const replacement = "\xEF\xBF\xBD";

/// Writes `bytes` again into `out` with the bad sequences of their texts replaced as jq's
/// decoder replaces them, from `place`; stops early, before a sequence whose end it cannot see
/// yet. Returns how many bytes it took.
fn repair(bytes: []const u8, start: Place, ended: bool, out: *std.ArrayList(u8)) usize {
    var place = start;
    var i: usize = 0;
    while (i < bytes.len) {
        const c = bytes[i];
        switch (place) {
            .outside => {
                if (c == '"') place = .text;
                out.append(gpa, c) catch oom();
                i += 1;
            },
            .escape => {
                place = if (c == 'u') .{ .hex = 4 } else .text;
                out.append(gpa, c) catch oom();
                i += 1;
            },
            .hex => |n| {
                place = if (n == 1) .text else .{ .hex = n - 1 };
                out.append(gpa, c) catch oom();
                i += 1;
            },
            .text => {
                if (c < 0x80) {
                    if (c == '"') place = .outside;
                    if (c == '\\') place = .escape;
                    out.append(gpa, c) catch oom();
                    i += 1;
                    continue;
                }
                const length = sequenceLength(c);
                if (length == 0) {
                    out.appendSlice(gpa, replacement) catch oom();
                    i += 1;
                    continue;
                }
                const left = bytesLeft(bytes, i + 1, length - 1, ended) orelse return i;
                if (left.text_ends and 1 + left.count < length) {
                    // The text ends inside the sequence: one U+FFFD for all that is left, its
                    // escapes too (`\"` is a byte of the text, not its end).
                    out.appendSlice(gpa, replacement) catch oom();
                    while (i < bytes.len and bytes[i] != '"') i = @min(bytes.len, i + @as(usize, if (bytes[i] == '\\') 2 else 1));
                    continue;
                }
                // Escapes never give a continuation byte first, so a sequence is raw bytes.
                var k: usize = 0;
                while (k + 1 < length and i + 1 + k < bytes.len and bytes[i + 1 + k] & 0xC0 == 0x80) k += 1;
                if (k + 1 == length and std.unicode.utf8ValidateSlice(bytes[i .. i + length])) {
                    out.appendSlice(gpa, bytes[i .. i + length]) catch oom();
                } else {
                    out.appendSlice(gpa, replacement) catch oom();
                }
                i += 1 + k;
            },
        }
    }
    return i;
}

/// The program's own value, built from the scanner's tokens, one at a time.
const Builder = struct {
    /// The arrays and objects open, each with the name of its member being read.
    open: std.ArrayList(struct { container: Value, key: ?[]u8 = null }) = .empty,
    /// The depth as jq counts it.
    depth: usize = 0,
    /// A text or number longer than a piece, put together.
    part: std.ArrayList(u8) = .empty,

    fn reset(b: *Builder) void {
        for (b.open.items) |o| {
            value.release(o.container);
            if (o.key) |k| gpa.free(k);
        }
        b.open.clearRetainingCapacity();
        b.part.clearRetainingCapacity();
        b.depth = 0;
    }

    /// The whole of a text or number token, its parts put together; valid until the next token.
    fn whole(b: *Builder, last: []const u8) []const u8 {
        if (b.part.items.len == 0) return last;
        b.part.appendSlice(gpa, last) catch oom();
        return b.part.items;
    }

    /// Takes a token; gives the value at the top once it is whole. `message` is set on an error.
    fn add(b: *Builder, token: Scanner.Token, message: *[]const u8) Error!?Value {
        const v: Value = switch (token) {
            .object_begin, .array_begin => {
                if (b.depth >= max_depth) {
                    message.* = "Exceeds depth limit for parsing";
                    return error.BadInput;
                }
                b.depth += 1;
                const c = if (token == .object_begin) value.emptyObject() else value.emptyArray();
                b.open.append(gpa, .{ .container = c }) catch oom();
                return null;
            },
            .object_end, .array_end => blk: {
                b.depth -= 1;
                const c = b.open.pop().?.container;
                value.fitToSize(c);
                break :blk c;
            },
            .partial_number, .partial_string => |s| {
                b.part.appendSlice(gpa, s) catch oom();
                return null;
            },
            inline .partial_string_escaped_1, .partial_string_escaped_2, .partial_string_escaped_3, .partial_string_escaped_4 => |s| {
                b.part.appendSlice(gpa, &s) catch oom();
                return null;
            },
            .number => |s| blk: {
                const n = value.numberFromText(b.whole(s), false).?;
                b.part.clearRetainingCapacity();
                break :blk n;
            },
            .string => |s| blk: {
                const top = if (b.open.items.len > 0) &b.open.items[b.open.items.len - 1] else null;
                if (top != null and top.?.container == .object and top.?.key == null) {
                    // A member's name: a level of its own until its value is read.
                    top.?.key = gpa.dupe(u8, b.whole(s)) catch oom();
                    b.part.clearRetainingCapacity();
                    b.depth += 1;
                    return null;
                }
                if (b.part.items.len == 0) break :blk value.string(s);
                b.part.appendSlice(gpa, s) catch oom();
                const t = value.stringFromList(b.part);
                b.part = .empty;
                break :blk t;
            },
            .true => .true,
            .false => .false,
            .null => .null,
            .allocated_number, .allocated_string, .end_of_document => unreachable,
        };
        if (b.open.items.len == 0) return v;
        const top = &b.open.items[b.open.items.len - 1];
        switch (top.container) {
            .array => |a| a.items.append(gpa, v) catch oom(),
            .object => |o| {
                value.objectPutOwned(o, top.key.?, v);
                top.key = null;
                b.depth -= 1;
            },
            else => unreachable,
        }
        return null;
    }
};

pub const Input = struct {
    io: Io,
    names: []const []const u8,
    next_name: usize = 0,
    use_stdin: bool,
    file: ?Io.File = null,
    /// No more bytes will come.
    ended: bool = false,
    /// A file could not be opened or read.
    failed: bool = false,
    /// The scanner holds `buffer[0..given]` until it asks for more, when it is done with it;
    /// `buffer[given..end]` are bytes read and kept back (a sequence cut off by the piece).
    buffer: []u8,
    given: usize = 0,
    end: usize = 0,
    /// The scanner has been told there is no more.
    scanner_ended: bool = false,
    /// No byte has been read yet: a byte order mark may come.
    at_start: bool = true,
    scanner: Scanner,
    builder: Builder = .{},
    /// What was wrong with the input, after `error.BadInput`.
    message: []const u8 = "",
    /// `-s`: every value read is given as one array.
    slurp: bool = false,
    slurped: bool = false,

    pub fn init(io: Io, names: []const []const u8) Input {
        return .{
            .io = io,
            .names = names,
            .use_stdin = names.len == 0,
            .buffer = gpa.alloc(u8, 2 * piece) catch oom(),
            .scanner = .initStreaming(gpa),
        };
    }

    /// The file being read, opening the next one when the last has ended; a file that cannot
    /// be opened is told and passed over.
    fn current(self: *Input) ?Io.File {
        if (self.file) |f| return f;
        if (self.use_stdin) {
            self.use_stdin = false;
            self.file = Io.File.stdin();
            return self.file;
        }
        while (self.next_name < self.names.len) {
            const name = self.names[self.next_name];
            self.next_name += 1;
            if (std.mem.eql(u8, name, "-")) {
                self.file = Io.File.stdin();
                return self.file;
            }
            self.file = Io.Dir.cwd().openFile(self.io, name, .{}) catch |err| {
                std.debug.print("jq: error: Could not open {s}: {s}\n", .{ name, @errorName(err) });
                self.failed = true;
                continue;
            };
            return self.file;
        }
        return null;
    }

    fn closeCurrent(self: *Input) void {
        const f = self.file orelse return;
        if (f.handle != Io.File.stdin().handle) f.close(self.io);
        self.file = null;
    }

    /// Reads the next piece after the bytes kept, or notes that the input has ended.
    fn readPiece(self: *Input) void {
        if (self.buffer.len - self.end < piece) {
            var size = self.buffer.len * 2;
            while (size - self.end < piece) size *= 2;
            self.buffer = gpa.realloc(self.buffer, size) catch oom();
        }
        while (true) {
            const file = self.current() orelse {
                self.ended = true;
                return;
            };
            const n = file.readStreaming(self.io, &.{self.buffer[self.end..][0..piece]}) catch |err| {
                if (err != error.EndOfStream) {
                    std.debug.print("jq: error: reading the input: {s}\n", .{@errorName(err)});
                    self.failed = true;
                }
                self.closeCurrent();
                continue;
            };
            if (n == 0) {
                self.closeCurrent();
                continue;
            }
            self.end += n;
            return;
        }
    }

    /// Gives the scanner, which has used all it was given, the next bytes: checked to be UTF-8,
    /// written again where they are not.
    fn feed(self: *Input) void {
        // The bytes the scanner held are done with; those kept back move to the front.
        const kept = self.buffer[self.given..self.end];
        std.mem.copyForwards(u8, self.buffer, kept);
        self.end = kept.len;
        self.given = 0;
        while (true) {
            if (!self.ended) self.readPiece();
            if (self.at_start) {
                if (self.end < 3 and !self.ended) continue;
                self.at_start = false;
                // The mark, or, when the input ends there, the first bytes of one, as jq's
                // parser takes them.
                const bom = "\xEF\xBB\xBF";
                var marked: usize = 0;
                while (marked < 3 and marked < self.end and self.buffer[marked] == bom[marked]) marked += 1;
                if (marked == 3 or (marked > 0 and marked == self.end and self.ended)) {
                    std.mem.copyForwards(u8, self.buffer, self.buffer[marked..self.end]);
                    self.end -= marked;
                }
            }
            const bytes = self.buffer[0..self.end];
            const whole = if (self.ended) bytes.len else wholeSequences(bytes);
            if (std.unicode.utf8ValidateSlice(bytes[0..whole])) {
                self.given = whole;
            } else {
                var fixed: std.ArrayList(u8) = .empty;
                defer fixed.deinit(gpa);
                const taken = repair(bytes, placeOf(&self.scanner), self.ended, &fixed);
                const rest = bytes.len - taken;
                const size = fixed.items.len + rest + piece;
                if (size > self.buffer.len) self.buffer = gpa.realloc(self.buffer, size) catch oom();
                std.mem.copyBackwards(u8, self.buffer[fixed.items.len..][0..rest], self.buffer[taken..self.end]);
                @memcpy(self.buffer[0..fixed.items.len], fixed.items);
                self.end = fixed.items.len + rest;
                self.given = fixed.items.len;
            }
            if (self.given > 0) self.scanner.feedInput(self.buffer[0..self.given]);
            if (self.ended and self.given == self.end) {
                self.scanner.endInput();
                self.scanner_ended = true;
            }
            if (self.given > 0 or self.scanner_ended) return;
        }
    }

    /// A new scanner for what follows a value at the top, given the rest of the current bytes.
    fn restart(self: *Input) void {
        const rest = self.scanner.input[self.scanner.cursor..];
        self.scanner.deinit();
        self.scanner = .initStreaming(gpa);
        if (rest.len > 0) self.scanner.feedInput(rest);
        if (self.scanner_ended) self.scanner.endInput();
    }

    /// The next value of the stream, or null at its end; with `-s`, one array of them all.
    pub fn next(self: *Input) Error!?Value {
        if (!self.slurp) return self.nextValue();
        if (self.slurped) return null;
        self.slurped = true;
        var items: std.ArrayList(Value) = .empty;
        while (self.nextValue() catch |err| {
            for (items.items) |v| value.release(v);
            items.deinit(gpa);
            return err;
        }) |v| items.append(gpa, v) catch oom();
        return value.arrayFromList(items);
    }

    fn nextValue(self: *Input) Error!?Value {
        errdefer self.builder.reset();
        while (true) {
            const token = self.scanner.next() catch |err| switch (err) {
                error.BufferUnderrun => {
                    self.feed();
                    continue;
                },
                error.OutOfMemory => oom(),
                // A new scanner at the end of the input, with only spaces left: no more values.
                error.UnexpectedEndOfInput => if (self.scanner.state == .value and self.builder.open.items.len == 0) return null else {
                    self.message = "Unfinished JSON term at EOF";
                    return error.BadInput;
                },
                else => {
                    self.message = "Invalid JSON text";
                    return error.BadInput;
                },
            };
            if (token == .end_of_document) return null;
            const v = try self.builder.add(token, &self.message) orelse continue;
            switch (token) {
                .number, .true, .false, .null => {
                    // A number or a word at the top: what follows must not belong to it.
                    errdefer value.release(v);
                    while (self.scanner.cursor >= self.scanner.input.len and !self.scanner_ended) self.feed();
                    if (self.scanner.cursor < self.scanner.input.len) switch (self.scanner.input[self.scanner.cursor]) {
                        ' ', '\t', '\r', '\n', '[', '{', '"' => {},
                        else => {
                            self.message = "Invalid literal";
                            return error.BadInput;
                        },
                    };
                },
                else => {},
            }
            self.restart();
            return v;
        }
    }
};

/// A whole text read as one JSON value (`fromjson`, `--argjson`); null with `message` set when
/// it is not one.
pub fn parseText(bytes: []const u8, message: *[]const u8) ?Value {
    var fixed: std.ArrayList(u8) = .empty;
    defer fixed.deinit(gpa);
    const text = if (std.unicode.utf8ValidateSlice(bytes)) bytes else blk: {
        _ = repair(bytes, .outside, true, &fixed);
        break :blk fixed.items;
    };
    var scanner: Scanner = .initCompleteInput(gpa, text);
    defer scanner.deinit();
    var builder: Builder = .{};
    defer builder.open.deinit(gpa);
    defer builder.part.deinit(gpa);
    while (true) {
        const token = scanner.next() catch |err| {
            builder.reset();
            message.* = if (err == error.UnexpectedEndOfInput and scanner.state == .value and builder.open.items.len == 0) "Expected JSON value" else "Invalid JSON text";
            return null;
        };
        if (token == .end_of_document) {
            message.* = "Expected JSON value";
            return null;
        }
        const v = (builder.add(token, message) catch {
            builder.reset();
            return null;
        }) orelse continue;
        for (text[scanner.cursor..]) |c| {
            if (!isSpace(c)) {
                value.release(v);
                message.* = "Unexpected extra JSON values";
                return null;
            }
        }
        return v;
    }
}
