//! The built-in functions that work on values, the arithmetic, indexing and the paths of
//! values, each as jq 1.8.2 does them. A function reads its input and arguments without taking
//! them (except the arithmetic, which takes both sides so that a side held once can be grown
//! in place) and gives a new value, or raises an error: `error.Raised`, the error's value in
//! the context.

const std = @import("std");
const c = @import("pcre2");
const value = @import("value.zig");
const parse = @import("parse.zig");
const input_mod = @import("input.zig");
const Value = value.Value;
const gpa = value.gpa;
const oom = value.oom;
const retain = value.retain;
const release = value.release;

pub const Raised = error{Raised};

const Regex = struct {
    code: *c.pcre2_code_8,
    data: *c.pcre2_match_data_8,
};

pub const Context = struct {
    /// The value of the error being raised.
    err: Value = .null,
    input: *input_mod.Input,
    /// The regular expressions compiled, by their flags and pattern.
    regexes: std.StringHashMapUnmanaged(Regex) = .empty,

    pub fn raise(ctx: *Context, v: Value) Raised {
        value.replace(&ctx.err, v);
        return error.Raised;
    }

    pub fn fail(ctx: *Context, comptime text: []const u8, args: anytype) Raised {
        var list: std.ArrayList(u8) = .empty;
        list.print(gpa, text, args) catch oom();
        return ctx.raise(value.stringFromList(list));
    }
};

/// A value for an error message: its kind and its JSON, cut short as jq cuts it.
pub const Shown = struct {
    buffer: [48]u8 = undefined,
    len: usize = 0,

    pub fn of(v: Value) Shown {
        var s: Shown = .{};
        var list: std.ArrayList(u8) = .empty;
        defer list.deinit(gpa);
        list.print(gpa, "{s} (", .{value.kindName(v)}) catch oom();
        const start = list.items.len;
        value.appendCompact(&list, v);
        if (list.items.len - start > 11) {
            list.shrinkRetainingCapacity(start + 11);
            list.appendSlice(gpa, "...") catch oom();
        }
        list.append(gpa, ')') catch oom();
        s.len = list.items.len;
        @memcpy(s.buffer[0..s.len], list.items);
        return s;
    }

    pub fn format(s: Shown, w: *std.Io.Writer) std.Io.Writer.Error!void {
        try w.writeAll(s.buffer[0..s.len]);
    }
};

pub fn shown(v: Value) Shown {
    return Shown.of(v);
}

// ---------------------------------------------------------------------------------------------
// Arithmetic

/// `a op b`; takes `a` and `b`.
pub fn arith(ctx: *Context, op: parse.Op, a: Value, b: Value) Raised!Value {
    switch (op) {
        .add => return add(ctx, a, b),
        .eq, .ne => {
            defer release(a);
            defer release(b);
            const same = value.equal(a, b);
            try checkDepth(ctx);
            return value.boolean(same == (op == .eq));
        },
        .lt, .le, .gt, .ge => {
            defer release(a);
            defer release(b);
            const order = value.compare(a, b);
            try checkDepth(ctx);
            return value.boolean(switch (op) {
                .lt => order == .lt,
                .le => order != .gt,
                .gt => order == .gt,
                .ge => order != .lt,
                else => unreachable,
            });
        },
        else => {},
    }
    defer release(b);
    if (value.isNumber(a) and value.isNumber(b)) {
        defer release(a);
        const x = value.toDouble(a);
        const y = value.toDouble(b);
        switch (op) {
            .sub => return value.number(x - y),
            .mul => return value.number(x * y),
            .div => {
                if (y == 0) return ctx.fail("{f} and {f} cannot be divided because the divisor is zero", .{ shown(a), shown(b) });
                return value.number(x / y);
            },
            .mod => {
                if (std.math.isNan(x) or std.math.isNan(y)) return value.number(std.math.nan(f64));
                const yi = toInt(y);
                if (yi == 0) return ctx.fail("{f} and {f} cannot be divided (remainder) because the divisor is zero", .{ shown(a), shown(b) });
                return value.number(if (yi == -1) 0 else @floatFromInt(@rem(toInt(x), yi)));
            },
            else => unreachable,
        }
    }
    switch (op) {
        .sub => if (a == .array and b == .array) {
            var result = a;
            const items = &value.uniqueArray(&result).items;
            var kept: usize = 0;
            for (items.items) |item| {
                const removed = for (b.array.items.items) |other| {
                    if (value.equal(item, other)) break true;
                } else false;
                if (removed) {
                    release(item);
                } else {
                    items.items[kept] = item;
                    kept += 1;
                }
            }
            items.shrinkRetainingCapacity(kept);
            checkDepth(ctx) catch |err| {
                release(result);
                return err;
            };
            return result;
        },
        .mul => {
            if (a == .object and b == .object) return deepMerge(a, b);
            if ((a == .string and value.isNumber(b)) or (value.isNumber(a) and b == .string)) {
                defer release(a);
                const s = if (a == .string) a.string else b.string;
                const d = value.toDouble(if (a == .string) b else a);
                if (d < 0 or std.math.isNan(d)) return .null;
                const n: usize = if (d > std.math.maxInt(i32)) std.math.maxInt(i32) else @intFromFloat(d);
                var list: std.ArrayList(u8) = .empty;
                list.ensureTotalCapacityPrecise(gpa, s.bytes.items.len * n) catch oom();
                for (0..n) |_| list.appendSliceAssumeCapacity(s.bytes.items);
                return value.stringFromList(list);
            }
        },
        .div => if (a == .string and b == .string) {
            defer release(a);
            return splitText(a.string.bytes.items, b.string.bytes.items);
        },
        else => {},
    }
    defer release(a);
    const verb = switch (op) {
        .sub => "subtracted",
        .mul => "multiplied",
        .div => "divided",
        else => "divided (remainder)",
    };
    return ctx.fail("{f} and {f} cannot be {s}", .{ shown(a), shown(b), verb });
}

/// A double as C's conversion to `intmax_t` gives it, held at the ends of its range.
fn toInt(d: f64) i64 {
    if (d <= -9.223372036854775807e18) return std.math.minInt(i64);
    if (d >= 9.223372036854775807e18) return std.math.maxInt(i64);
    return @intFromFloat(d);
}

/// `a + b`: numbers, texts, arrays and objects (the right one's members win), and null.
pub fn add(ctx: *Context, a: Value, b: Value) Raised!Value {
    if (a == .null) return b;
    if (b == .null) return a;
    if (value.isNumber(a) and value.isNumber(b)) {
        defer release(a);
        defer release(b);
        return value.number(value.toDouble(a) + value.toDouble(b));
    }
    defer release(b);
    var result = a;
    switch (a) {
        .string => if (b == .string) {
            value.uniqueString(&result).bytes.appendSlice(gpa, b.string.bytes.items) catch oom();
            return result;
        },
        .array => if (b == .array) {
            const items = &value.uniqueArray(&result).items;
            items.ensureUnusedCapacity(gpa, b.array.items.items.len) catch oom();
            for (b.array.items.items) |item| items.appendAssumeCapacity(retain(item));
            return result;
        },
        .object => if (b == .object) {
            const o = value.uniqueObject(&result);
            for (b.object.map.keys(), b.object.map.values()) |key, item| value.objectPut(o, key, retain(item));
            return result;
        },
        else => {},
    }
    defer release(a);
    return ctx.fail("{f} and {f} cannot be added", .{ shown(a), shown(b) });
}

/// `a * b` of two objects: members that are objects on both sides are merged too. Takes `a`.
fn deepMerge(a: Value, b: Value) Value {
    var result = a;
    const o = value.uniqueObject(&result);
    for (b.object.map.keys(), b.object.map.values()) |key, item| {
        if (o.map.getPtr(key)) |slot| {
            if (slot.* == .object and item == .object) {
                slot.* = deepMerge(slot.*, item);
                continue;
            }
        }
        value.objectPut(o, key, retain(item));
    }
    return result;
}

pub fn negate(ctx: *Context, v: Value) Raised!Value {
    if (!value.isNumber(v)) return ctx.fail("{f} cannot be negated", .{shown(v)});
    return value.negate(v);
}

// ---------------------------------------------------------------------------------------------
// Indexing

/// An index of an array from a double: cut toward zero, held inside `i64`.
fn arrayIndex(d: f64) i64 {
    if (std.math.isNan(d)) return 0;
    return toInt(d);
}

/// `t[key]`.
pub fn index(ctx: *Context, t: Value, key: Value) Raised!Value {
    switch (t) {
        .object => |o| if (key == .string) {
            return retain(o.map.get(key.string.bytes.items) orelse .null);
        },
        .array => |a| {
            if (value.isNumber(key)) {
                const items = a.items.items;
                var i = arrayIndex(value.toDouble(key));
                if (i < 0) i += @intCast(items.len);
                if (i < 0 or i >= items.len) return .null;
                return retain(items[@intCast(i)]);
            }
            if (key == .object) return slice(ctx, t, sliceBound(key, "start"), sliceBound(key, "end"));
            if (key == .array) {
                const found = indices(t, key);
                checkDepth(ctx) catch |err| {
                    release(found);
                    return err;
                };
                return found;
            }
        },
        .null => switch (key) {
            .string, .number, .literal, .object, .null => return .null,
            else => {},
        },
        .string => if (key == .object) return slice(ctx, t, sliceBound(key, "start"), sliceBound(key, "end")),
        else => {},
    }
    if (key == .string) return ctx.fail("Cannot index {s} with \"{s}\"", .{ value.kindName(t), key.string.bytes.items });
    return ctx.fail("Cannot index {s} with {s}", .{ value.kindName(t), value.kindName(key) });
}

fn sliceBound(key: Value, name: []const u8) Value {
    return key.object.map.get(name) orelse .null;
}

/// Where the items of `key` stand in `t` one after another (`.[[1,2]]`).
fn indices(t: Value, key: Value) Value {
    var result: std.ArrayList(Value) = .empty;
    const items = t.array.items.items;
    const want = key.array.items.items;
    if (want.len > 0 and want.len <= items.len) {
        for (0..items.len - want.len + 1) |i| {
            const all = for (want, 0..) |w, k| {
                if (!value.equal(items[i + k], w)) break false;
            } else true;
            if (all) result.append(gpa, value.number(@floatFromInt(i))) catch oom();
        }
    }
    return value.arrayFromList(result);
}

/// The start and end of a slice of `len` items, as jq works them out: from the end when
/// negative, held inside, the start rounded down and the end up.
fn sliceRange(ctx: *Context, len: usize, from: Value, to: Value) Raised![2]usize {
    if (!(from == .null or value.isNumber(from)) or !(to == .null or value.isNumber(to))) {
        return ctx.fail("Start and end indices of an array slice must be numbers", .{});
    }
    const n: f64 = @floatFromInt(len);
    var start: f64 = if (from == .null) 0 else value.toDouble(from);
    var end: f64 = if (to == .null) n else value.toDouble(to);
    if (std.math.isNan(start)) start = 0;
    if (std.math.isNan(end)) end = n;
    if (start < 0) start += n;
    if (end < 0) end += n;
    if (start < 0) start = 0;
    if (start > n) start = n;
    if (end > n) end = n;
    if (end < start) end = start;
    return .{ @intFromFloat(@floor(start)), @intFromFloat(@ceil(end)) };
}

/// `t[from:to]` of an array, a text (by characters) or null.
pub fn slice(ctx: *Context, t: Value, from: Value, to: Value) Raised!Value {
    switch (t) {
        .null => return .null,
        .array => |a| {
            const r = try sliceRange(ctx, a.items.items.len, from, to);
            var items: std.ArrayList(Value) = .empty;
            items.ensureTotalCapacityPrecise(gpa, r[1] - r[0]) catch oom();
            for (a.items.items[r[0]..r[1]]) |item| items.appendAssumeCapacity(retain(item));
            return value.arrayFromList(items);
        },
        .string => |s| {
            const bytes = s.bytes.items;
            const r = try sliceRange(ctx, value.codepointCount(bytes), from, to);
            const begin = value.codepointOffset(bytes, r[0]);
            const end = begin + value.codepointOffset(bytes[begin..], r[1] - r[0]);
            return value.string(bytes[begin..end]);
        },
        else => return ctx.fail("Cannot index {s} with object", .{value.kindName(t)}),
    }
}

// ---------------------------------------------------------------------------------------------
// Paths

/// `getpath(p)`: null where the path leaves the value.
pub fn getPath(ctx: *Context, t: Value, path: Value) Raised!Value {
    if (path != .array) return ctx.fail("Path must be specified as an array", .{});
    var current = retain(t);
    for (path.array.items.items) |key| {
        if (current == .null) return .null;
        const next = index(ctx, current, key) catch |err| {
            release(current);
            return err;
        };
        release(current);
        current = next;
    }
    return current;
}

/// The place of `key` in the container at `slot`, made held once (and made from null when
/// `create`); null when there is no such place and it is not to be made.
fn childSlot(ctx: *Context, slot: *Value, key: Value, create: bool) Raised!?*Value {
    if (slot.* == .null) {
        if (!create) return null;
        if (key == .string) {
            slot.* = value.emptyObject();
        } else if (value.isNumber(key)) {
            slot.* = value.emptyArray();
        } else {
            return ctx.fail("Cannot index null with {s}", .{value.kindName(key)});
        }
    }
    switch (slot.*) {
        .object => if (key == .string) {
            const o = value.uniqueObject(slot);
            if (o.map.getPtr(key.string.bytes.items)) |p| return p;
            if (!create) return null;
            value.objectPut(o, key.string.bytes.items, .null);
            return o.map.getPtr(key.string.bytes.items).?;
        } else return ctx.fail("Cannot index object with {s}", .{value.kindName(key)}),
        .array => if (value.isNumber(key)) {
            const len = slot.array.items.items.len;
            var i = arrayIndex(value.toDouble(key));
            if (i < 0) i += @intCast(len);
            if (i < 0) {
                if (!create) return null;
                return ctx.fail("Out of bounds negative array index", .{});
            }
            const at: usize = @intCast(i);
            if (at >= len and !create) return null;
            if (at > 100_000_000) return ctx.fail("Array index too large", .{});
            const a = value.uniqueArray(slot);
            while (a.items.items.len <= at) a.items.append(gpa, .null) catch oom();
            return &a.items.items[at];
        } else return ctx.fail("Cannot update field at object index of array", .{}),
        else => {
            if (key == .string) return ctx.fail("Cannot index {s} with \"{s}\"", .{ value.kindName(slot.*), key.string.bytes.items });
            return ctx.fail("Cannot index {s} with {s}", .{ value.kindName(slot.*), value.kindName(key) });
        },
    }
}

/// `setpath(p; v)` on the value at `root`, in place where it is held once; takes `v`.
pub fn setPath(ctx: *Context, root: *Value, path: []const Value, v: Value) Raised!void {
    errdefer release(v);
    var slot = root;
    for (path, 0..) |key, i| {
        if (key == .object and (slot.* == .array or slot.* == .null)) {
            // A slice: the part is set as a whole and put back in its place.
            if (slot.* == .null) slot.* = value.emptyArray();
            const len = slot.array.items.items.len;
            const r = try sliceRange(ctx, len, sliceBound(key, "start"), sliceBound(key, "end"));
            var part = try slice(ctx, slot.*, sliceBound(key, "start"), sliceBound(key, "end"));
            setPath(ctx, &part, path[i + 1 ..], v) catch |err| {
                release(part);
                return err;
            };
            if (part != .array) {
                release(part);
                return ctx.fail("A slice of an array can only be assigned another array", .{});
            }
            const a = value.uniqueArray(slot);
            for (a.items.items[r[0]..r[1]]) |old| release(old);
            a.items.replaceRange(gpa, r[0], r[1] - r[0], part.array.items.items) catch oom();
            for (part.array.items.items) |item| _ = retain(item);
            release(part);
            return;
        }
        slot = (try childSlot(ctx, slot, key, true)).?;
    }
    value.replace(slot, v);
}

/// The value at `path`, taken out of `root` (null left in its place) so that whoever updates it
/// holds it once; null when the path leads nowhere.
pub fn takePath(ctx: *Context, root: *Value, path: []const Value) Raised!Value {
    for (path) |key| {
        if (key == .object) return getPath(ctx, root.*, value.arrayFromList(blk: {
            var list: std.ArrayList(Value) = .empty;
            for (path) |k| list.append(gpa, retain(k)) catch oom();
            break :blk list;
        }));
    }
    var slot = root;
    for (path) |key| {
        if (slot.* == .null) return .null;
        slot = (try childSlot(ctx, slot, key, false)) orelse return .null;
    }
    return value.take(slot);
}

fn lessPath(_: void, a: Value, b: Value) bool {
    return value.compare(a, b) == .lt;
}

/// `delpaths(ps)`: the paths sorted, then each deleted from the last to the first.
pub fn delPaths(ctx: *Context, root: *Value, paths: Value) Raised!void {
    if (paths != .array) return ctx.fail("Paths must be specified as an array", .{});
    const list = gpa.dupe(Value, paths.array.items.items) catch oom();
    defer gpa.free(list);
    std.mem.sort(Value, list, {}, lessPath);
    try checkDepth(ctx);
    var i = list.len;
    while (i > 0) {
        i -= 1;
        const path = list[i];
        if (path != .array) return ctx.fail("Path must be specified as an array", .{});
        try delPath(ctx, root, path.array.items.items);
    }
}

fn delPath(ctx: *Context, root: *Value, path: []const Value) Raised!void {
    if (path.len == 0) {
        value.replace(root, .null);
        return;
    }
    var slot = root;
    for (path[0 .. path.len - 1], 0..) |key, i| {
        if (slot.* == .null) return;
        if (key == .object) {
            // Inside a slice: deleted in the part, which is put back.
            if (slot.* != .array) return ctx.fail("Cannot delete field at object index of {s}", .{value.kindName(slot.*)});
            const r = try sliceRange(ctx, slot.array.items.items.len, sliceBound(key, "start"), sliceBound(key, "end"));
            var part = try slice(ctx, slot.*, sliceBound(key, "start"), sliceBound(key, "end"));
            delPath(ctx, &part, path[i + 1 ..]) catch |err| {
                release(part);
                return err;
            };
            const a = value.uniqueArray(slot);
            for (a.items.items[r[0]..r[1]]) |old| release(old);
            a.items.replaceRange(gpa, r[0], r[1] - r[0], part.array.items.items) catch oom();
            for (part.array.items.items) |item| _ = retain(item);
            release(part);
            return;
        }
        slot = (try childSlot(ctx, slot, key, false)) orelse return;
    }
    const key = path[path.len - 1];
    switch (slot.*) {
        .null => {},
        .object => {
            if (key != .string) return ctx.fail("Cannot delete field at index of object", .{});
            value.objectRemove(value.uniqueObject(slot), key.string.bytes.items);
        },
        .array => {
            if (value.isNumber(key)) {
                const len = slot.array.items.items.len;
                var at = arrayIndex(value.toDouble(key));
                if (at < 0) at += @intCast(len);
                if (at < 0) return ctx.fail("Out of bounds negative array index", .{});
                if (at >= len) return;
                const a = value.uniqueArray(slot);
                release(a.items.orderedRemove(@intCast(at)));
            } else if (key == .object) {
                const r = try sliceRange(ctx, slot.array.items.items.len, sliceBound(key, "start"), sliceBound(key, "end"));
                const a = value.uniqueArray(slot);
                for (a.items.items[r[0]..r[1]]) |old| release(old);
                a.items.replaceRange(gpa, r[0], r[1] - r[0], &.{}) catch oom();
            } else return ctx.fail("Cannot delete field at object index of array", .{});
        },
        else => return ctx.fail("Cannot delete field at index of {s}", .{value.kindName(slot.*)}),
    }
}

// ---------------------------------------------------------------------------------------------
// Texts

fn splitText(text: []const u8, separator: []const u8) Value {
    var items: std.ArrayList(Value) = .empty;
    if (text.len > 0) {
        if (separator.len == 0) {
            var i: usize = 0;
            while (i < text.len) {
                const n = std.unicode.utf8ByteSequenceLength(text[i]) catch 1;
                items.append(gpa, value.string(text[i..@min(i + n, text.len)])) catch oom();
                i += n;
            }
        } else {
            var it = std.mem.splitSequence(u8, text, separator);
            while (it.next()) |part| items.append(gpa, value.string(part)) catch oom();
        }
    }
    return value.arrayFromList(items);
}

fn regex(ctx: *Context, pattern: []const u8, flags: []const u8) Raised!Regex {
    var options: u32 = c.PCRE2_UTF;
    for (flags) |f| switch (f) {
        'g', 'n', 'l', 's' => {},
        'i' => options |= c.PCRE2_CASELESS,
        'x' => options |= c.PCRE2_EXTENDED,
        'p' => options |= c.PCRE2_DOTALL,
        else => return ctx.fail("{s} is not a valid modifier string", .{flags}),
    };
    var key: std.ArrayList(u8) = .empty;
    key.appendSlice(gpa, flags) catch oom();
    key.append(gpa, 0) catch oom();
    key.appendSlice(gpa, pattern) catch oom();
    if (ctx.regexes.get(key.items)) |r| {
        key.deinit(gpa);
        return r;
    }
    var error_code: c_int = 0;
    var error_offset: usize = 0;
    const code = c.pcre2_compile_8(pattern.ptr, pattern.len, options, &error_code, &error_offset, null) orelse {
        key.deinit(gpa);
        var message: [256]u8 = undefined;
        const n = c.pcre2_get_error_message_8(error_code, &message, message.len);
        return ctx.fail("{s} (at offset {d}) is not a valid regex: {s}", .{ pattern, error_offset, message[0..@intCast(@max(n, 0))] });
    };
    _ = c.pcre2_jit_compile_8(code, c.PCRE2_JIT_COMPLETE);
    const data = c.pcre2_match_data_create_from_pattern_8(code, null) orelse oom();
    const r: Regex = .{ .code = code, .data = data };
    ctx.regexes.put(gpa, key.toOwnedSlice(gpa) catch oom(), r) catch oom();
    return r;
}

fn testRegex(ctx: *Context, input: Value, pattern: Value, flags: Value) Raised!Value {
    if (input != .string) return ctx.fail("{f} cannot be matched, as it is not a string", .{shown(input)});
    if (pattern != .string) return ctx.fail("{f} is not a string", .{shown(pattern)});
    if (flags != .null and flags != .string) return ctx.fail("{f} is not a string", .{shown(flags)});
    const flag_text = if (flags == .string) flags.string.bytes.items else "";
    const r = try regex(ctx, pattern.string.bytes.items, flag_text);
    const text = input.string.bytes.items;
    const not_empty: u32 = if (std.mem.indexOfScalar(u8, flag_text, 'n') != null) c.PCRE2_NOTEMPTY else 0;
    const rc = c.pcre2_match_8(r.code, text.ptr, text.len, 0, c.PCRE2_NO_UTF_CHECK | not_empty, r.data, null);
    if (rc >= 0) return .true;
    if (rc == c.PCRE2_ERROR_NOMATCH) return .false;
    var message: [256]u8 = undefined;
    const n = c.pcre2_get_error_message_8(rc, &message, message.len);
    return ctx.fail("Regex failure: {s}", .{message[0..@intCast(@max(n, 0))]});
}

// ---------------------------------------------------------------------------------------------
// Formats

const base64_alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn format(ctx: *Context, f: parse.Format, v: Value) Raised!Value {
    switch (f) {
        .text => return value.toText(retain(v)),
        .json => return value.toJson(v),
        .csv, .tsv => {
            if (v != .array) return ctx.fail("{f} cannot be {s}-formatted, only an array can be", .{ shown(v), @tagName(f) });
            var list: std.ArrayList(u8) = .empty;
            errdefer list.deinit(gpa);
            for (v.array.items.items, 0..) |item, i| {
                if (i > 0) list.append(gpa, if (f == .csv) ',' else '\t') catch oom();
                switch (item) {
                    .null => {},
                    .true, .false, .number, .literal => value.appendCompact(&list, item),
                    .string => |s| if (f == .csv) {
                        list.append(gpa, '"') catch oom();
                        for (s.bytes.items) |ch| {
                            if (ch == '"') list.append(gpa, '"') catch oom();
                            list.append(gpa, ch) catch oom();
                        }
                        list.append(gpa, '"') catch oom();
                    } else {
                        for (s.bytes.items) |ch| switch (ch) {
                            '\\' => list.appendSlice(gpa, "\\\\") catch oom(),
                            '\t' => list.appendSlice(gpa, "\\t") catch oom(),
                            '\n' => list.appendSlice(gpa, "\\n") catch oom(),
                            '\r' => list.appendSlice(gpa, "\\r") catch oom(),
                            else => list.append(gpa, ch) catch oom(),
                        };
                    },
                    else => return ctx.fail("{f} is not valid in a {s} row", .{ shown(item), @tagName(f) }),
                }
            }
            return value.stringFromList(list);
        },
        .base64 => {
            const text = value.toText(retain(v));
            defer release(text);
            const bytes = text.string.bytes.items;
            var list: std.ArrayList(u8) = .empty;
            list.ensureTotalCapacityPrecise(gpa, (bytes.len + 2) / 3 * 4) catch oom();
            var i: usize = 0;
            while (i < bytes.len) : (i += 3) {
                const n = @min(3, bytes.len - i);
                var chunk: u32 = 0;
                for (0..3) |k| chunk = (chunk << 8) | (if (k < n) bytes[i + k] else 0);
                for (0..4) |k| {
                    if (k <= n) {
                        list.appendAssumeCapacity(base64_alphabet[(chunk >> @intCast(18 - 6 * k)) & 63]);
                    } else list.appendAssumeCapacity('=');
                }
            }
            return value.stringFromList(list);
        },
        .base64d => {
            const text = value.toText(retain(v));
            defer release(text);
            // As jq decodes: up to the first `=`, a byte outside the alphabet is an error, and
            // a single byte left over too.
            var list: std.ArrayList(u8) = .empty;
            defer list.deinit(gpa);
            var code: u32 = 0;
            var count: u32 = 0;
            for (text.string.bytes.items) |ch| {
                if (ch == '=') break;
                const d = std.mem.indexOfScalar(u8, base64_alphabet, ch) orelse
                    return ctx.fail("{f} is not valid base64 data", .{shown(text)});
                code = (code << 6) | @as(u32, @intCast(d));
                count += 1;
                if (count == 4) {
                    list.appendSlice(gpa, &.{ @truncate(code >> 16), @truncate(code >> 8), @truncate(code) }) catch oom();
                    code = 0;
                    count = 0;
                }
            }
            switch (count) {
                3 => list.appendSlice(gpa, &.{ @truncate(code >> 10), @truncate(code >> 2) }) catch oom(),
                2 => list.append(gpa, @truncate(code >> 4)) catch oom(),
                1 => return ctx.fail("{f} trailing base64 byte found", .{shown(text)}),
                else => {},
            }
            return value.stringFromBytes(list.items);
        },
        .uri => {
            const text = value.toText(retain(v));
            defer release(text);
            var list: std.ArrayList(u8) = .empty;
            for (text.string.bytes.items) |ch| {
                if (std.ascii.isAlphanumeric(ch) or ch == '-' or ch == '_' or ch == '.' or ch == '~') {
                    list.append(gpa, ch) catch oom();
                } else {
                    list.print(gpa, "%{X:0>2}", .{ch}) catch oom();
                }
            }
            return value.stringFromList(list);
        },
    }
}

// ---------------------------------------------------------------------------------------------
// Sorting

fn lessValue(_: void, a: Value, b: Value) bool {
    return value.compare(a, b) == .lt;
}

const Keyed = struct { key: Value, item: Value };

fn lessKeyed(_: void, a: Keyed, b: Keyed) bool {
    return value.compare(a.key, b.key) == .lt;
}

/// The items of `input` with their keys, sorted by them and stable.
fn sortedByKeys(ctx: *Context, input: Value, keys: Value, what: []const u8) Raised![]Keyed {
    if (input != .array) return ctx.fail("Cannot {s} {f}, as it is not an array", .{ what, shown(input) });
    const items = input.array.items.items;
    const list = gpa.alloc(Keyed, items.len) catch oom();
    for (items, 0..) |item, i| list[i] = .{ .key = keys.array.items.items[i], .item = item };
    std.mem.sort(Keyed, list, {}, lessKeyed);
    return list;
}

fn groups(ctx: *Context, input: Value, keys: Value, first_only: bool) Raised!Value {
    const list = try sortedByKeys(ctx, input, keys, "group");
    defer gpa.free(list);
    var result: std.ArrayList(Value) = .empty;
    var i: usize = 0;
    while (i < list.len) {
        var j = i + 1;
        while (j < list.len and value.equal(list[j].key, list[i].key)) j += 1;
        if (first_only) {
            result.append(gpa, retain(list[i].item)) catch oom();
        } else {
            var group: std.ArrayList(Value) = .empty;
            group.ensureTotalCapacityPrecise(gpa, j - i) catch oom();
            for (list[i..j]) |k| group.appendAssumeCapacity(retain(k.item));
            result.append(gpa, value.arrayFromList(group)) catch oom();
        }
        i = j;
    }
    return value.arrayFromList(result);
}

/// `min_by` and `max_by`: the first smallest, the last largest.
fn minMax(ctx: *Context, input: Value, keys: ?Value, is_min: bool) Raised!Value {
    if (input != .array) return ctx.fail("Cannot find {s} of {f}, as it is not an array", .{ if (is_min) "min" else "max", shown(input) });
    const items = input.array.items.items;
    if (items.len == 0) return .null;
    var best: usize = 0;
    for (1..items.len) |i| {
        const a = if (keys) |k| k.array.items.items[i] else items[i];
        const b = if (keys) |k| k.array.items.items[best] else items[best];
        const order = value.compare(a, b);
        if ((order == .lt) == is_min) best = i;
    }
    return retain(items[best]);
}

// ---------------------------------------------------------------------------------------------
// The functions

fn requireString(ctx: *Context, v: Value, comptime name: []const u8) Raised![]const u8 {
    if (v != .string) return ctx.fail(name ++ " input must be a string", .{});
    return v.string.bytes.items;
}

fn iterateValues(ctx: *Context, v: Value) Raised![]const Value {
    return switch (v) {
        .array => |a| a.items.items,
        .object => |o| o.map.values(),
        else => ctx.fail("Cannot iterate over {f}", .{shown(v)}),
    };
}

fn keysOf(ctx: *Context, v: Value, sorted: bool) Raised!Value {
    var items: std.ArrayList(Value) = .empty;
    switch (v) {
        .object => |o| {
            if (sorted) {
                const keys = value.sortedKeys(o);
                defer gpa.free(keys);
                for (keys) |k| items.append(gpa, value.string(k)) catch oom();
            } else {
                for (o.map.keys()) |k| items.append(gpa, value.string(k)) catch oom();
            }
        },
        .array => |a| for (0..a.items.items.len) |i| items.append(gpa, value.number(@floatFromInt(i))) catch oom(),
        else => return ctx.fail("{f} has no keys", .{shown(v)}),
    }
    return value.arrayFromList(items);
}

fn has(ctx: *Context, v: Value, key: Value) Raised!Value {
    switch (v) {
        .object => |o| if (key == .string) return value.boolean(o.map.contains(key.string.bytes.items)),
        .array => |a| if (value.isNumber(key)) {
            const d = value.toDouble(key);
            return value.boolean(d >= 0 and d < @as(f64, @floatFromInt(a.items.items.len)));
        },
        else => {},
    }
    return ctx.fail("Cannot check whether {s} has a {s} key", .{ value.kindName(v), value.kindName(key) });
}

fn fromEntries(ctx: *Context, v: Value) Raised!Value {
    const items = try iterateValues(ctx, v);
    var result = value.emptyObject();
    errdefer release(result);
    for (items) |entry| {
        // The name: `key`, `Key`, `name` or `Name`, the first that is not null or false; the
        // value: `value`, else `Value`.
        var key: Value = .null;
        var val: Value = .null;
        if (entry == .object) {
            const m = entry.object.map;
            for ([_][]const u8{ "key", "Key", "name", "Name" }) |name| {
                key = m.get(name) orelse .null;
                if (value.truthy(key)) break;
            }
            val = m.get("value") orelse m.get("Value") orelse .null;
        } else if (entry != .null) {
            return ctx.fail("Cannot index {s} with \"key\"", .{value.kindName(entry)});
        }
        if (key != .string) return ctx.fail("Cannot use {f} as object key", .{shown(key)});
        value.objectPut(value.uniqueObject(&result), key.string.bytes.items, retain(val));
    }
    return result;
}

fn join(ctx: *Context, v: Value, separator: Value) Raised!Value {
    const items = try iterateValues(ctx, v);
    var result: Value = .null;
    errdefer release(result);
    for (items) |item| {
        // `add` takes what it adds, also when it fails.
        const so_far = result;
        result = .null;
        result = if (so_far == .null) value.string("") else try add(ctx, so_far, retain(separator));
        const part: Value = switch (item) {
            .null => value.string(""),
            .string => retain(item),
            .true, .false, .number, .literal => value.toJson(item),
            else => return ctx.fail("Cannot join with {s}", .{value.kindName(item)}),
        };
        const joined = result;
        result = .null;
        result = try add(ctx, joined, part);
    }
    if (result == .null) return value.string("");
    return result;
}

fn length(ctx: *Context, v: Value) Raised!Value {
    return switch (v) {
        .null => value.number(0),
        .true, .false => ctx.fail("{f} has no length", .{shown(v)}),
        .number => |d| value.number(@abs(d)),
        .literal => |l| if (value.literalIsNegative(l)) value.negate(v) else retain(v),
        .string => |s| value.number(@floatFromInt(value.codepointCount(s.bytes.items))),
        .array => |a| value.number(@floatFromInt(a.items.items.len)),
        .object => |o| value.number(@floatFromInt(o.map.count())),
    };
}

/// `reverse` as jq 1.8.2 writes it: `[.[length - 1 - range(0; length)]]`.
fn reverse(ctx: *Context, v: Value) Raised!Value {
    switch (v) {
        .array => |a| {
            var items: std.ArrayList(Value) = .empty;
            items.ensureTotalCapacityPrecise(gpa, a.items.items.len) catch oom();
            var i = a.items.items.len;
            while (i > 0) {
                i -= 1;
                items.appendAssumeCapacity(retain(a.items.items[i]));
            }
            return value.arrayFromList(items);
        },
        else => {
            const n = try length(ctx, v);
            defer release(n);
            if (value.toDouble(n) <= 0) return value.emptyArray();
            return ctx.fail("Cannot index {s} with number", .{value.kindName(v)});
        },
    }
}

fn caseOf(ctx: *Context, v: Value, upper: bool) Raised!Value {
    if (v != .string) return ctx.fail("{f} cannot be {s}", .{ shown(v), if (upper) "uppercased" else "lowercased" });
    var list: std.ArrayList(u8) = .empty;
    list.appendSlice(gpa, v.string.bytes.items) catch oom();
    for (list.items) |*ch| ch.* = if (upper) std.ascii.toUpper(ch.*) else std.ascii.toLower(ch.*);
    return value.stringFromList(list);
}

fn tonumber(ctx: *Context, v: Value) Raised!Value {
    if (value.isNumber(v)) return retain(v);
    if (v == .string) {
        if (value.numberFromText(v.string.bytes.items, true)) |n| return n;
        return ctx.fail("Cannot parse '{s}' as JSON", .{v.string.bytes.items});
    }
    return ctx.fail("{f} cannot be parsed as a number", .{shown(v)});
}

/// Raises when a comparison just made went deeper than jq lets one go.
pub fn checkDepth(ctx: *Context) Raised!void {
    if (!value.too_deep) return;
    value.too_deep = false;
    return ctx.fail("Comparison too deep", .{});
}

/// The built-in function `id` on `input` with `args`, read without taking them; an error when
/// the comparisons it made went too deep.
pub fn call(ctx: *Context, id: parse.Native, f: parse.Format, input: Value, args: []const Value) Raised!Value {
    const result = try callFunction(ctx, id, f, input, args);
    checkDepth(ctx) catch |err| {
        release(result);
        return err;
    };
    return result;
}

fn callFunction(ctx: *Context, id: parse.Native, f: parse.Format, input: Value, args: []const Value) Raised!Value {
    switch (id) {
        .not_ => return value.boolean(!value.truthy(input)),
        .length => return length(ctx, input),
        .utf8bytelength => {
            if (input != .string) return ctx.fail("{f} only strings have UTF-8 byte length", .{shown(input)});
            return value.number(@floatFromInt(input.string.bytes.items.len));
        },
        .keys => return keysOf(ctx, input, true),
        .keys_unsorted => return keysOf(ctx, input, false),
        .has => return has(ctx, input, args[0]),
        .in_ => return has(ctx, args[0], input),
        .to_entries => {
            const keys = try keysOf(ctx, input, false);
            defer release(keys);
            var items: std.ArrayList(Value) = .empty;
            items.ensureTotalCapacityPrecise(gpa, keys.array.items.items.len) catch oom();
            for (keys.array.items.items) |k| {
                const entry = value.emptyObject();
                value.objectPut(entry.object, "key", retain(k));
                value.objectPut(entry.object, "value", try index(ctx, input, k));
                items.appendAssumeCapacity(entry);
            }
            return value.arrayFromList(items);
        },
        .from_entries => return fromEntries(ctx, input),
        .add => {
            var result: Value = .null;
            for (try iterateValues(ctx, input)) |item| result = try add(ctx, result, retain(item));
            return result;
        },
        .any, .all => {
            const want_all = id == .all;
            for (try iterateValues(ctx, input)) |item| {
                if (value.truthy(item) != want_all) return value.boolean(!want_all);
            }
            return value.boolean(want_all);
        },
        .sort => {
            if (input != .array) return ctx.fail("{f} cannot be sorted, as it is not an array", .{shown(input)});
            var items: std.ArrayList(Value) = .empty;
            items.appendSlice(gpa, input.array.items.items) catch oom();
            for (items.items) |item| _ = retain(item);
            std.mem.sort(Value, items.items, {}, lessValue);
            return value.arrayFromList(items);
        },
        .sort_by => {
            const list = try sortedByKeys(ctx, input, args[0], "sort");
            defer gpa.free(list);
            var items: std.ArrayList(Value) = .empty;
            items.ensureTotalCapacityPrecise(gpa, list.len) catch oom();
            for (list) |k| items.appendAssumeCapacity(retain(k.item));
            return value.arrayFromList(items);
        },
        .group_by => return groups(ctx, input, args[0], false),
        .unique => {
            if (input != .array) return ctx.fail("{f} cannot be sorted, as it is not an array", .{shown(input)});
            return groups(ctx, input, input, true);
        },
        .unique_by => return groups(ctx, input, args[0], true),
        .min => return minMax(ctx, input, null, true),
        .max => return minMax(ctx, input, null, false),
        .min_by => return minMax(ctx, input, args[0], true),
        .max_by => return minMax(ctx, input, args[0], false),
        .reverse => return reverse(ctx, input),
        .tostring => return value.toText(retain(input)),
        .tonumber => return tonumber(ctx, input),
        .tojson => return value.toJson(input),
        .fromjson => {
            if (input != .string) return ctx.fail("{f} only strings can be parsed", .{shown(input)});
            var message: []const u8 = "";
            return input_mod.parseText(input.string.bytes.items, &message) orelse
                ctx.fail("{s} (while parsing '{s}')", .{ message, input.string.bytes.items });
        },
        .type_ => return value.string(value.kindName(input)),
        .split => {
            if (input != .string or args[0] != .string) return ctx.fail("split input and separator must be strings", .{});
            return splitText(input.string.bytes.items, args[0].string.bytes.items);
        },
        .join => return join(ctx, input, args[0]),
        .test_ => return testRegex(ctx, input, args[0], if (args.len > 1) args[1] else .null),
        .ascii_downcase => return caseOf(ctx, input, false),
        .ascii_upcase => return caseOf(ctx, input, true),
        .startswith, .endswith => {
            if (input != .string or args[0] != .string) {
                return ctx.fail("{s}() requires string inputs", .{if (id == .startswith) "startswith" else "endswith"});
            }
            const text = input.string.bytes.items;
            const part = args[0].string.bytes.items;
            return value.boolean(if (id == .startswith) std.mem.startsWith(u8, text, part) else std.mem.endsWith(u8, text, part));
        },
        .ltrimstr, .rtrimstr => {
            // jq 1.8.2 asks `startswith` (`endswith`) first, which wants two texts.
            if (input != .string or args[0] != .string) {
                return ctx.fail("{s}() requires string inputs", .{if (id == .ltrimstr) "startswith" else "endswith"});
            }
            {
                const text = input.string.bytes.items;
                const part = args[0].string.bytes.items;
                if (id == .ltrimstr and std.mem.startsWith(u8, text, part)) return value.string(text[part.len..]);
                if (id == .rtrimstr and std.mem.endsWith(u8, text, part)) return value.string(text[0 .. text.len - part.len]);
            }
            return retain(input);
        },
        .floor, .sqrt => {
            if (!value.isNumber(input)) return ctx.fail("{f} number required", .{shown(input)});
            const d = value.toDouble(input);
            return value.number(if (id == .floor) @floor(d) else @sqrt(d));
        },
        .setpath => {
            if (args[0] != .array) return ctx.fail("Path must be specified as an array", .{});
            var result = retain(input);
            setPath(ctx, &result, args[0].array.items.items, retain(args[1])) catch |err| {
                release(result);
                return err;
            };
            return result;
        },
        .delpaths => {
            var result = retain(input);
            delPaths(ctx, &result, args[0]) catch |err| {
                release(result);
                return err;
            };
            return result;
        },
        .error_ => return ctx.raise(retain(if (args.len > 0) args[0] else input)),
        .input => {
            const next = ctx.input.next() catch return ctx.fail("{s}", .{ctx.input.message});
            return next orelse ctx.fail("No more inputs", .{});
        },
        .format => return format(ctx, f, input),
    }
}
