//! The values of the program: null, false, true, a number, a text, an array and an object. Texts,
//! arrays and objects live on the heap with a count of their holders, kept by hand: `retain` adds a
//! holder, `release` takes one away and frees the value when none is left. A value held once may
//! be changed in place (jq's copy on write); one held more often is copied first (`unique*`).
//!
//! A number read from the input or written in the filter keeps its literal, as jq 1.8 does with
//! decNumber: the text is made canonical as decNumber's to-scientific-string writes it, and two
//! literals compare exactly, digit by digit. A number computed is a `double`.

const std = @import("std");
const Io = std.Io;

pub const gpa = std.heap.c_allocator;

/// jq stops when memory runs out; so does this program.
pub fn oom() noreturn {
    @panic("out of memory");
}

pub const Value = union(enum) {
    null,
    false,
    true,
    number: f64,
    /// A whole number of at most 15 digits read and not changed: its double, found from its
    /// digits, writes it as it was written (choice 1837).
    exact: f64,
    literal: *Literal,
    string: *String,
    array: *Array,
    object: *Object,
};

/// A number read and not changed: its canonical text, the text's bytes in the same allocation,
/// after the struct, and its value as a `double`, worked out the first time it is wanted (as jq
/// does), as most numbers read are only written out again.
pub const Literal = struct {
    rc: u32 = 1,
    len: u32,
    value: f64,
    ready: bool,
    /// decNumber's infinity: a number read past its largest exponent, or `"infinity"` read by
    /// `tonumber`. Its double is infinite and its text the largest double; it is above every
    /// other literal (below, when negative).
    infinite: bool = false,

    pub fn double(l: *Literal) f64 {
        if (!l.ready) {
            l.value = toDoubleAsJq(l.text());
            l.ready = true;
        }
        return l.value;
    }

    pub fn text(l: *const Literal) []const u8 {
        const bytes: [*]const u8 = @ptrCast(l);
        return bytes[@sizeOf(Literal)..][0..l.len];
    }

    /// A literal of the canonical text `bytes`; its value is `value_` when known.
    fn create(value_: ?f64, bytes: []const u8) *Literal {
        const memory = gpa.alignedAlloc(u8, .of(Literal), @sizeOf(Literal) + bytes.len) catch oom();
        const l: *Literal = @ptrCast(memory.ptr);
        l.* = .{ .len = @intCast(bytes.len), .value = value_ orelse 0, .ready = value_ != null };
        @memcpy(memory[@sizeOf(Literal)..], bytes);
        return l;
    }

    /// decNumber's infinity, with the sign of `d`.
    fn createInfinite(d: f64) *Literal {
        const l = create(d, if (d < 0) "-1.7976931348623157e+308" else "1.7976931348623157e+308");
        l.infinite = true;
        return l;
    }

    fn destroy(l: *Literal) void {
        const memory: [*]align(@alignOf(Literal)) u8 = @ptrCast(l);
        gpa.free(memory[0 .. @sizeOf(Literal) + l.len]);
    }
};

pub const String = struct {
    rc: u32 = 1,
    bytes: std.ArrayList(u8) = .empty,
};

pub const Array = struct {
    rc: u32 = 1,
    items: std.ArrayList(Value) = .empty,
};

pub const Map = std.array_hash_map.String(Value);

/// An object keeps its names in the order they came in; each name is owned by the map.
pub const Object = struct {
    rc: u32 = 1,
    map: Map = .empty,
};

// ---------------------------------------------------------------------------------------------
// Making values

pub fn boolean(b: bool) Value {
    return if (b) .true else .false;
}

pub fn number(d: f64) Value {
    return .{ .number = d };
}

pub fn string(bytes: []const u8) Value {
    const s = gpa.create(String) catch oom();
    s.* = .{};
    s.bytes.ensureTotalCapacityPrecise(gpa, bytes.len) catch oom();
    s.bytes.appendSliceAssumeCapacity(bytes);
    return .{ .string = s };
}

/// A text taking `list`'s bytes as its own; a short one is copied into a block of its size, so
/// that the room a list grows by is not kept for each small text.
pub fn stringFromList(list: std.ArrayList(u8)) Value {
    var bytes = list;
    if (bytes.capacity - bytes.items.len > 16 and bytes.items.len <= 256) {
        defer bytes.deinit(gpa);
        return string(bytes.items);
    }
    const s = gpa.create(String) catch oom();
    s.* = .{ .bytes = bytes };
    return .{ .string = s };
}

pub fn emptyArray() Value {
    const a = gpa.create(Array) catch oom();
    a.* = .{};
    return .{ .array = a };
}

pub fn arrayFromList(list: std.ArrayList(Value)) Value {
    const a = gpa.create(Array) catch oom();
    a.* = .{ .items = list };
    return .{ .array = a };
}

pub fn emptyObject() Value {
    const o = gpa.create(Object) catch oom();
    o.* = .{};
    return .{ .object = o };
}

// ---------------------------------------------------------------------------------------------
// Holders

pub fn retain(v: Value) Value {
    switch (v) {
        .literal => |l| l.rc += 1,
        .string => |s| s.rc += 1,
        .array => |a| a.rc += 1,
        .object => |o| o.rc += 1,
        else => {},
    }
    return v;
}

/// Takes one holder away; true when it was the last.
fn drop(v: Value) bool {
    switch (v) {
        .literal => |l| {
            l.rc -= 1;
            return l.rc == 0;
        },
        .string => |s| {
            s.rc -= 1;
            return s.rc == 0;
        },
        .array => |a| {
            a.rc -= 1;
            return a.rc == 0;
        },
        .object => |o| {
            o.rc -= 1;
            return o.rc == 0;
        },
        else => return false,
    }
}

/// Frees a value whose holders are gone; its texts and numbers at once, the arrays and objects
/// inside it through a list of its own rather than the machine's stack, as a value may be
/// nested deeper than the stack would hold.
fn destroy(v: Value) void {
    var pending: std.ArrayList(Value) = .empty;
    defer pending.deinit(gpa);
    var current = v;
    while (true) {
        switch (current) {
            .literal => |l| l.destroy(),
            .string => |s| {
                s.bytes.deinit(gpa);
                gpa.destroy(s);
            },
            .array => |a| {
                for (a.items.items) |item| {
                    if (drop(item)) pending.append(gpa, item) catch oom();
                }
                a.items.deinit(gpa);
                gpa.destroy(a);
            },
            .object => |o| {
                for (o.map.keys(), o.map.values()) |key, item| {
                    gpa.free(key);
                    if (drop(item)) pending.append(gpa, item) catch oom();
                }
                o.map.deinit(gpa);
                gpa.destroy(o);
            },
            else => {},
        }
        current = pending.pop() orelse return;
    }
}

pub fn release(v: Value) void {
    if (drop(v)) destroy(v);
}

/// Sets `slot` to `v`, releasing what it held.
pub fn replace(slot: *Value, v: Value) void {
    const old = slot.*;
    slot.* = v;
    release(old);
}

/// Takes the value out of `slot`, leaving null.
pub fn take(slot: *Value) Value {
    const v = slot.*;
    slot.* = .null;
    return v;
}

/// The text of `v`, held once, copied first when another holds it too.
pub fn uniqueString(v: *Value) *String {
    const s = v.string;
    if (s.rc == 1) return s;
    const copy = string(s.bytes.items);
    release(v.*);
    v.* = copy;
    return copy.string;
}

pub fn uniqueArray(v: *Value) *Array {
    const a = v.array;
    if (a.rc == 1) return a;
    var items: std.ArrayList(Value) = .empty;
    items.ensureTotalCapacityPrecise(gpa, a.items.items.len) catch oom();
    for (a.items.items) |item| items.appendAssumeCapacity(retain(item));
    const copy = arrayFromList(items);
    release(v.*);
    v.* = copy;
    return copy.array;
}

pub fn uniqueObject(v: *Value) *Object {
    const o = v.object;
    if (o.rc == 1) return o;
    const copy = emptyObject();
    const map = &copy.object.map;
    reserve(map, o.map.count());
    for (o.map.keys(), o.map.values()) |key, item| {
        map.putAssumeCapacity(gpa.dupe(u8, key) catch oom(), retain(item));
    }
    release(v.*);
    v.* = copy;
    return copy.object;
}

/// Gives back the room a small array or object grew by and does not use, once it is whole. A
/// large one keeps it: copying it would hold both at once.
pub fn fitToSize(v: Value) void {
    switch (v) {
        .array => |a| if (a.items.items.len > 4096) return,
        .object => |o| if (o.map.count() > 4096) return,
        else => return,
    }
    switch (v) {
        .array => |a| {
            const items = &a.items;
            if (items.capacity == items.items.len) return;
            var fitted: std.ArrayList(Value) = .empty;
            fitted.ensureTotalCapacityPrecise(gpa, items.items.len) catch oom();
            fitted.appendSliceAssumeCapacity(items.items);
            items.deinit(gpa);
            items.* = fitted;
        },
        .object => |o| if (o.map.entries.capacity > o.map.count()) o.map.shrinkAndFree(gpa, o.map.count()),
        else => {},
    }
}

/// Room for `n` members in a map, and no more.
pub fn reserve(map: *Map, n: usize) void {
    map.entries.setCapacity(gpa, n) catch oom();
    map.ensureTotalCapacity(gpa, n) catch oom();
}

/// Sets a member of an object held once; `value` is taken.
pub fn objectPut(o: *Object, key: []const u8, value: Value) void {
    const entry = o.map.getOrPut(gpa, key) catch oom();
    if (entry.found_existing) {
        replace(entry.value_ptr, value);
    } else {
        entry.key_ptr.* = gpa.dupe(u8, key) catch oom();
        entry.value_ptr.* = value;
    }
}

/// Sets a member of an object held once, taking `key`'s bytes as the name's when it is new;
/// `value` is taken.
pub fn objectPutOwned(o: *Object, key: []u8, v: Value) void {
    const entry = o.map.getOrPut(gpa, key) catch oom();
    if (entry.found_existing) {
        gpa.free(key);
        replace(entry.value_ptr, v);
    } else {
        entry.value_ptr.* = v;
    }
}

pub fn objectRemove(o: *Object, key: []const u8) void {
    if (o.map.fetchOrderedRemove(key)) |kv| {
        gpa.free(kv.key);
        release(kv.value);
    }
}

// ---------------------------------------------------------------------------------------------
// Kinds

pub const Kind = enum(u8) { null, false, true, number, string, array, object };

pub fn kind(v: Value) Kind {
    return switch (v) {
        .null => .null,
        .false => .false,
        .true => .true,
        .number, .exact, .literal => .number,
        .string => .string,
        .array => .array,
        .object => .object,
    };
}

pub fn kindName(v: Value) []const u8 {
    return switch (kind(v)) {
        .null => "null",
        .false, .true => "boolean",
        .number => "number",
        .string => "string",
        .array => "array",
        .object => "object",
    };
}

pub fn isNumber(v: Value) bool {
    return v == .number or v == .exact or v == .literal;
}

pub fn toDouble(v: Value) f64 {
    return switch (v) {
        .number, .exact => |d| d,
        .literal => |l| l.double(),
        else => unreachable,
    };
}

pub fn truthy(v: Value) bool {
    return !(v == .null or v == .false);
}

// ---------------------------------------------------------------------------------------------
// Literals of numbers, as decNumber reads and writes them

/// decNumber's limits in jq's context: an adjusted exponent past `max_exponent` is infinite, an
/// exponent below `tiny_exponent` is rounded to it.
const max_exponent: i64 = 999_999_999;
const tiny_exponent: i64 = -1_147_483_646;

/// Reads the text of a number: `[+-]digits[.digits][e[+-]digits]`, `.5` and `1.` too, and, when
/// `words` is set, `nan` and `infinity` as decNumber reads them (for `tonumber`). The text has
/// been checked already when it comes from the input. Null when it is not a number.
pub fn numberFromText(text: []const u8, words: bool) ?Value {
    var i: usize = 0;
    var negative = false;
    if (i < text.len and (text[i] == '-' or text[i] == '+')) {
        negative = text[i] == '-';
        i += 1;
    }
    if (words) {
        const rest = text[i..];
        if (std.ascii.eqlIgnoreCase(rest, "nan") or std.ascii.eqlIgnoreCase(rest, "snan")) return number(std.math.nan(f64));
        if (std.ascii.eqlIgnoreCase(rest, "inf") or std.ascii.eqlIgnoreCase(rest, "infinity")) {
            return .{ .literal = Literal.createInfinite(if (negative) -std.math.inf(f64) else std.math.inf(f64)) };
        }
    }
    const int_start = i;
    while (i < text.len and std.ascii.isDigit(text[i])) i += 1;
    const int_digits = text[int_start..i];
    var frac_digits: []const u8 = "";
    if (i < text.len and text[i] == '.') {
        i += 1;
        const frac_start = i;
        while (i < text.len and std.ascii.isDigit(text[i])) i += 1;
        frac_digits = text[frac_start..i];
    }
    if (int_digits.len == 0 and frac_digits.len == 0) return null;
    var exponent: i64 = 0;
    if (i < text.len and (text[i] == 'e' or text[i] == 'E')) {
        i += 1;
        var exponent_negative = false;
        if (i < text.len and (text[i] == '-' or text[i] == '+')) {
            exponent_negative = text[i] == '-';
            i += 1;
        }
        const exponent_start = i;
        while (i < text.len and std.ascii.isDigit(text[i])) : (i += 1) {
            // Saturates far past decNumber's limits, which then decide.
            if (exponent < 1 << 50) exponent = exponent * 10 + (text[i] - '0');
        }
        if (i == exponent_start) return null;
        if (exponent_negative) exponent = -exponent;
    }
    if (i != text.len) return null;

    // A whole number of at most 15 digits is its double, found from its digits (choice 1837).
    if (i == int_start + int_digits.len and i == text.len and text[0] != '+' and int_digits.len > 0 and
        int_digits.len <= 15 and int_digits[0] != '0')
    {
        var n: i64 = 0;
        for (int_digits) |c| n = n * 10 + (c - '0');
        return .{ .exact = @floatFromInt(if (negative) -n else n) };
    }

    // Most numbers are written as decNumber would write them already: no exponent, no `+`, no
    // leading zero before other digits, and not too small for the plain form.
    if (!words and exponent == 0 and std.mem.indexOfAny(u8, text, "eE+") == null and int_digits.len > 0 and
        (int_digits.len == 1 or int_digits[0] != '0') and (frac_digits.len > 0 or text[text.len - 1] != '.'))
    {
        // A zero is plain with at most six zeros after its point: `0.0000000` is `0E-7`.
        const plain = int_digits[0] != '0' or for (frac_digits, 0..) |c, k| {
            if (c != '0') break k <= 5;
        } else frac_digits.len <= 6;
        if (plain) return .{ .literal = Literal.create(null, text) };
    }

    // The coefficient: the digits without leading zeros, "0" when there are none.
    // The digits of a usual number are put together on the stack.
    var buffer: [256]u8 = undefined;
    var fixed: std.heap.FixedBufferAllocator = .init(&buffer);
    const temporary = if (int_digits.len + frac_digits.len <= 64) fixed.allocator() else gpa;
    var digits: std.ArrayList(u8) = .empty;
    defer digits.deinit(temporary);
    digits.ensureTotalCapacityPrecise(temporary, int_digits.len + frac_digits.len) catch oom();
    digits.appendSlice(temporary, int_digits) catch oom();
    digits.appendSlice(temporary, frac_digits) catch oom();
    exponent -= @intCast(frac_digits.len);
    var first: usize = 0;
    while (first + 1 < digits.items.len and digits.items[first] == '0') first += 1;
    var coefficient = digits.items[first..];
    const zero = coefficient.len == 1 and coefficient[0] == '0';

    if (zero) {
        exponent = std.math.clamp(exponent, tiny_exponent, max_exponent);
    } else {
        if (exponent + @as(i64, @intCast(coefficient.len)) - 1 > max_exponent) {
            return .{ .literal = Literal.createInfinite(if (negative) -std.math.inf(f64) else std.math.inf(f64)) };
        }
        if (exponent < tiny_exponent) {
            // Rounded half up to the smallest exponent, as decNumber's context does.
            const shift: usize = @intCast(tiny_exponent - exponent);
            exponent = tiny_exponent;
            if (shift > coefficient.len) {
                coefficient = digits.items[0..1];
                coefficient[0] = '0';
            } else {
                const kept = coefficient.len - shift;
                const round_up = coefficient[kept] >= '5';
                var rounded: std.ArrayList(u8) = .empty;
                defer rounded.deinit(gpa);
                rounded.appendSlice(gpa, if (kept == 0) "0" else coefficient[0..kept]) catch oom();
                if (round_up) {
                    var at = rounded.items.len;
                    while (at > 0) {
                        at -= 1;
                        if (rounded.items[at] == '9') {
                            rounded.items[at] = '0';
                        } else {
                            rounded.items[at] += 1;
                            break;
                        }
                    } else rounded.insert(gpa, 0, '1') catch oom();
                }
                var start: usize = 0;
                while (start + 1 < rounded.items.len and rounded.items[start] == '0') start += 1;
                return literalFromParts(negative, rounded.items[start..], exponent, gpa);
            }
        }
    }
    return literalFromParts(negative, coefficient, exponent, temporary);
}

/// The literal of a coefficient and an exponent; its text is put together with `temporary`.
fn literalFromParts(negative: bool, coefficient: []const u8, exponent: i64, temporary: std.mem.Allocator) Value {
    var text: std.ArrayList(u8) = .empty;
    defer text.deinit(temporary);
    text.ensureTotalCapacityPrecise(temporary, coefficient.len + 32) catch oom();
    if (negative) text.append(temporary, '-') catch oom();
    writeScientific(&text, temporary, coefficient, exponent);
    return .{ .literal = Literal.create(null, text.items) };
}

/// decNumber's to-scientific-string: plain when the exponent is 0 or below and the adjusted
/// exponent is -6 or above, else one digit, the rest after a point, and `E+n` or `E-n`.
fn writeScientific(text: *std.ArrayList(u8), allocator: std.mem.Allocator, coefficient: []const u8, exponent: i64) void {
    const n: i64 = @intCast(coefficient.len);
    const adjusted = exponent + n - 1;
    if (exponent <= 0 and adjusted >= -6) {
        if (exponent == 0) {
            text.appendSlice(allocator, coefficient) catch oom();
            return;
        }
        const point = n + exponent;
        if (point > 0) {
            const p: usize = @intCast(point);
            text.appendSlice(allocator, coefficient[0..p]) catch oom();
            text.append(allocator, '.') catch oom();
            text.appendSlice(allocator, coefficient[p..]) catch oom();
        } else {
            text.appendSlice(allocator, "0.") catch oom();
            text.appendNTimes(allocator, '0', @intCast(-point)) catch oom();
            text.appendSlice(allocator, coefficient) catch oom();
        }
        return;
    }
    text.append(allocator, coefficient[0]) catch oom();
    if (coefficient.len > 1) {
        text.append(allocator, '.') catch oom();
        text.appendSlice(allocator, coefficient[1..]) catch oom();
    }
    text.print(allocator, "E{c}{d}", .{ @as(u8, if (adjusted < 0) '-' else '+'), @abs(adjusted) }) catch oom();
}

/// A literal's coefficient and exponent, read back from its canonical text.
const Decimal = struct {
    negative: bool,
    /// The coefficient's digits as they stand in the text, perhaps with a point among them.
    mantissa: []const u8,
    exponent: i64,

    fn read(text: []const u8) Decimal {
        var d: Decimal = .{ .negative = false, .mantissa = text, .exponent = 0 };
        if (d.mantissa[0] == '-') {
            d.negative = true;
            d.mantissa = d.mantissa[1..];
        }
        if (std.mem.indexOfScalar(u8, d.mantissa, 'E')) |e| {
            d.exponent = std.fmt.parseInt(i64, d.mantissa[e + 1 ..], 10) catch 0;
            d.mantissa = d.mantissa[0..e];
        }
        if (std.mem.indexOfScalar(u8, d.mantissa, '.')) |point| {
            d.exponent -= @intCast(d.mantissa.len - point - 1);
        }
        // Leading zeros (of `0.001`) are not digits of the coefficient.
        while (d.mantissa.len > 1 and (d.mantissa[0] == '0' or d.mantissa[0] == '.')) d.mantissa = d.mantissa[1..];
        return d;
    }

    fn digitCount(d: Decimal) i64 {
        const has_point = std.mem.indexOfScalar(u8, d.mantissa, '.') != null;
        return @as(i64, @intCast(d.mantissa.len)) - @intFromBool(has_point);
    }

    fn isZero(d: Decimal) bool {
        for (d.mantissa) |c| if (c != '0' and c != '.') return false;
        return true;
    }

    /// The digit at place `i` from the first, 0 past the last.
    fn digit(d: Decimal, i: usize) u8 {
        var at: usize = 0;
        for (d.mantissa) |c| {
            if (c == '.') continue;
            if (at == i) return c;
            at += 1;
        }
        return '0';
    }
};

/// The `double` jq makes of a literal: decNumber first rounds it to 17 digits, half to even, and
/// the text of that is read as a `double`. A literal of more digits can so end one step away from
/// the `double` nearest to it, and jq is followed in that.
fn toDoubleAsJq(text: []const u8) f64 {
    const d = Decimal.read(text);
    const n = d.digitCount();
    if (n <= 17 or d.isZero()) return std.fmt.parseFloat(f64, text) catch std.math.nan(f64);
    var digits: [18]u8 = undefined;
    for (0..17) |i| digits[i] = d.digit(i);
    // Half to even on what is dropped.
    const first_dropped = d.digit(17);
    var rest_zero = true;
    var k: usize = 18;
    while (k < n) : (k += 1) {
        if (d.digit(k) != '0') {
            rest_zero = false;
            break;
        }
    }
    const up = first_dropped > '5' or (first_dropped == '5' and (!rest_zero or (digits[16] - '0') % 2 == 1));
    var exponent = d.exponent + n - 17;
    var len: usize = 17;
    if (up) {
        var at: usize = 17;
        while (at > 0) {
            at -= 1;
            if (digits[at] == '9') {
                digits[at] = '0';
            } else {
                digits[at] += 1;
                break;
            }
        } else {
            // 99...9 became 100...0: one digit more, the last of which is a zero.
            digits[0] = '1';
            exponent += 1;
            len = 17;
        }
    }
    var buffer: [64]u8 = undefined;
    const rounded = std.fmt.bufPrint(&buffer, "{s}{s}e{d}", .{ if (d.negative) "-" else "", digits[0..len], exponent }) catch unreachable;
    return std.fmt.parseFloat(f64, rounded) catch std.math.nan(f64);
}

/// Two literals compared exactly.
fn compareLiterals(a: []const u8, b: []const u8) std.math.Order {
    const x = Decimal.read(a);
    const y = Decimal.read(b);
    const x_zero = x.isZero();
    const y_zero = y.isZero();
    if (x_zero and y_zero) return .eq;
    const x_sign: i8 = if (x_zero) 0 else if (x.negative) -1 else 1;
    const y_sign: i8 = if (y_zero) 0 else if (y.negative) -1 else 1;
    if (x_sign != y_sign) return std.math.order(x_sign, y_sign);
    // Same sign, neither zero: the adjusted exponents, then the digits.
    var order = std.math.order(x.exponent + x.digitCount(), y.exponent + y.digitCount());
    if (order == .eq) {
        const n: usize = @intCast(@max(x.digitCount(), y.digitCount()));
        for (0..n) |i| {
            order = std.math.order(x.digit(i), y.digit(i));
            if (order != .eq) break;
        }
    }
    return if (x_sign < 0) order.invert() else order;
}

/// `-v`: a literal keeps its digits (decNumber's minus, which gives `0` for `-0`).
pub fn negate(v: Value) Value {
    switch (v) {
        .literal => |l| {
            if (l.infinite) return .{ .literal = Literal.createInfinite(-l.value) };
            const old = l.text();
            const d = Decimal.read(old);
            var text: std.ArrayList(u8) = .empty;
            defer text.deinit(gpa);
            if (!d.negative and !d.isZero()) text.append(gpa, '-') catch oom();
            text.appendSlice(gpa, if (old[0] == '-') old[1..] else old) catch oom();
            return .{ .literal = Literal.create(null, text.items) };
        },
        .number => |d| return number(-d),
        .exact => |d| return .{ .exact = -d },
        else => unreachable,
    }
}

/// Whether a literal's sign is set (`-0` too), as decNumber tells it.
pub fn literalIsNegative(l: *Literal) bool {
    return l.text()[0] == '-';
}

// ---------------------------------------------------------------------------------------------
// Order: null < false < true < numbers < texts < arrays < objects

/// How deep jq compares values: deeper, it stops with "Comparison too deep" (or "Equality check
/// too deep"). A comparison that went past it sets `too_deep`, for its caller to raise.
const max_compare_depth = 10_000;
pub var too_deep: bool = false;

pub fn compare(a: Value, b: Value) std.math.Order {
    return compareAt(a, b, 0);
}

fn compareAt(a: Value, b: Value, depth: usize) std.math.Order {
    if (depth > max_compare_depth) {
        too_deep = true;
        return .eq;
    }
    const ka = kind(a);
    const kb = kind(b);
    if (ka != kb) return std.math.order(@backingInt(ka), @backingInt(kb));
    switch (ka) {
        .null, .false, .true => return .eq,
        .number => return compareNumbers(a, b),
        .string => return std.mem.order(u8, a.string.bytes.items, b.string.bytes.items),
        .array => {
            const x = a.array.items.items;
            const y = b.array.items.items;
            for (0..@min(x.len, y.len)) |i| {
                const order = compareAt(x[i], y[i], depth + 1);
                if (order != .eq) return order;
            }
            return std.math.order(x.len, y.len);
        },
        .object => {
            // The sorted names first (an array of texts, a level and its texts another, as jq
            // compares them), then the values in the order of the names.
            const x = sortedKeys(a.object);
            defer gpa.free(x);
            const y = sortedKeys(b.object);
            defer gpa.free(y);
            // The names are compared one level down, and each pair of names one more.
            if (depth + 1 > max_compare_depth or (x.len > 0 and y.len > 0 and depth + 2 > max_compare_depth)) {
                too_deep = true;
                return .eq;
            }
            for (0..@min(x.len, y.len)) |i| {
                const order = std.mem.order(u8, x[i], y[i]);
                if (order != .eq) return order;
            }
            if (x.len != y.len) return std.math.order(x.len, y.len);
            for (x) |key| {
                const order = compareAt(a.object.map.get(key).?, b.object.map.get(key).?, depth + 1);
                if (order != .eq) return order;
            }
            return .eq;
        },
    }
}

fn compareNumbers(a: Value, b: Value) std.math.Order {
    const x = toDouble(a);
    const y = toDouble(b);
    // A NaN is below every number, itself too, as jq has it.
    if (std.math.isNan(x)) return .lt;
    if (std.math.isNan(y)) return .gt;
    // The doubles first: a literal's double is rounded in the order of the literals, so two whose
    // doubles differ are in that order (choice 1837).
    const order = std.math.order(x, y);
    if (order != .eq) return order;
    // decNumber's infinity is above a literal whose double is infinite only by rounding.
    if (x == y and std.math.isInf(x) and a == .literal and b == .literal and a.literal.infinite != b.literal.infinite) {
        const above: std.math.Order = if (a.literal.infinite) .gt else .lt;
        return if (x < 0) above.invert() else above;
    }
    // The same double: two numbers read are compared exactly when one keeps its text, but two of
    // at most 15 digits, whose doubles keep them apart, are equal.
    const read_a = a == .literal or a == .exact;
    const read_b = b == .literal or b == .exact;
    if (read_a and read_b and (a == .literal or b == .literal) and !(orderedAsDouble(a) and orderedAsDouble(b))) {
        var buffer_a: [32]u8 = undefined;
        var buffer_b: [32]u8 = undefined;
        return compareLiterals(readText(a, &buffer_a), readText(b, &buffer_b));
    }
    return .eq;
}

/// Whether a number read is in the order of its double: a whole number of at most 15 digits
/// always is, a literal when it is short. Looked at only when two doubles are the same.
fn orderedAsDouble(v: Value) bool {
    return switch (v) {
        .exact => true,
        .literal => |l| orderedByDouble(l.text()),
        else => false,
    };
}

/// The literal of a number read: its text, or the digits of a whole number held as its double.
fn readText(v: Value, buffer: *[32]u8) []const u8 {
    return switch (v) {
        .literal => |l| l.text(),
        .exact => |d| std.fmt.bufPrint(buffer, "{d}", .{@as(i64, @intFromFloat(d))}) catch unreachable,
        else => unreachable,
    };
}

/// Whether a literal (as decNumber writes it) is ordered exactly by its double: at most 15 digits
/// in its coefficient, and its adjusted exponent within 307 of 0 (choice 1837).
fn orderedByDouble(text: []const u8) bool {
    const b = if (text.len > 0 and text[0] == '-') text[1..] else text;
    const end = std.mem.indexOfScalar(u8, b, 'E') orelse b.len;
    var digits: usize = 0;
    var whole_digits: usize = 0;
    var zeros_after_point: usize = 0;
    var seen_point = false;
    var seen_digit = false;
    for (b[0..end]) |c| {
        if (c == '.') {
            seen_point = true;
        } else if (seen_digit or c != '0') {
            seen_digit = true;
            digits += 1;
            if (!seen_point) whole_digits += 1;
        } else if (seen_point) {
            zeros_after_point += 1;
        }
    }
    if (!seen_digit) return true;
    if (digits > 15) return false;
    if (end < b.len) {
        const adjusted = std.fmt.parseInt(i64, b[end + 1 ..], 10) catch return false;
        return adjusted >= -307 and adjusted <= 307;
    }
    return if (whole_digits > 0) whole_digits <= 308 else zeros_after_point < 307;
}

/// `a == b`, as jq's `jv_equal`: the same value in memory is equal at once; otherwise the items
/// and members, one level deeper each.
pub fn equal(a: Value, b: Value) bool {
    return equalAt(a, b, 0);
}

fn equalAt(a: Value, b: Value, depth: usize) bool {
    if (sameInMemory(a, b)) return true;
    if (depth > max_compare_depth) {
        too_deep = true;
        return false;
    }
    if (kind(a) != kind(b)) return false;
    switch (a) {
        .array => |x| {
            const y = b.array.items.items;
            if (x.items.items.len != y.len) return false;
            for (x.items.items, y) |p, q| if (!equalAt(p, q, depth + 1)) return false;
            return true;
        },
        .object => |x| {
            if (x.map.count() != b.object.map.count()) return false;
            for (x.map.keys(), x.map.values()) |key, item| {
                const other = b.object.map.get(key) orelse return false;
                if (!equalAt(item, other, depth + 1)) return false;
            }
            return true;
        },
        .number, .exact, .literal => return compareNumbers(a, b) == .eq,
        .string => |x| return std.mem.eql(u8, x.bytes.items, b.string.bytes.items),
        else => return true,
    }
}

fn sameInMemory(a: Value, b: Value) bool {
    return switch (a) {
        .literal => |x| b == .literal and x == b.literal,
        .string => |x| b == .string and x == b.string,
        .array => |x| b == .array and x == b.array,
        .object => |x| b == .object and x == b.object,
        else => false,
    };
}

fn lessKey(_: void, a: []const u8, b: []const u8) bool {
    return std.mem.order(u8, a, b) == .lt;
}

/// The names of an object sorted by their bytes; the slice is the caller's to free.
pub fn sortedKeys(o: *Object) [][]const u8 {
    const keys = gpa.alloc([]const u8, o.map.count()) catch oom();
    for (o.map.keys(), 0..) |key, i| keys[i] = key;
    std.mem.sort([]const u8, keys, {}, lessKey);
    return keys;
}

// ---------------------------------------------------------------------------------------------
// Writing values

pub const Style = struct {
    /// Lines broken and members indented; otherwise all on one line with no spaces.
    pretty: bool = true,
    indent: u8 = 2,
    tab: bool = false,
    sort_keys: bool = false,
};

/// A `double` as jq writes it: the fewest digits that read back the same, with an exponent when
/// the point would stand 4 or more places left of the first digit or more than 15 right of the
/// last one; NaN as null, the infinities as the largest finite doubles.
pub fn writeDouble(w: *Io.Writer, d_: f64) Io.Writer.Error!void {
    if (std.math.isNan(d_)) return w.writeAll("null");
    const d = std.math.clamp(d_, -std.math.floatMax(f64), std.math.floatMax(f64));
    // A whole number below 10^16 is written as an integer, as jq writes it (choice 1825); a zero
    // goes on below, which keeps the sign of `-0`.
    if (d != 0 and @abs(d) < 1e16 and d == @trunc(d)) return w.print("{d}", .{@as(i64, @intFromFloat(d))});
    var buffer: [64]u8 = undefined;
    // Ryu's shortest digits, as `d.ddde±n`.
    const sci = std.fmt.float.render(&buffer, d, .{ .mode = .scientific }) catch unreachable;
    var digits: [24]u8 = undefined;
    var n: usize = 0;
    var at: usize = 0;
    if (sci[0] == '-') {
        try w.writeByte('-');
        at = 1;
    }
    while (sci[at] != 'e') : (at += 1) {
        if (sci[at] == '.') continue;
        digits[n] = sci[at];
        n += 1;
    }
    const exponent = std.fmt.parseInt(i32, sci[at + 1 ..], 10) catch unreachable;
    // The place of the point counted from the first digit, as dtoa gives it.
    const point: i32 = exponent + 1;
    const count: i32 = @intCast(n);
    if (point <= -4 or point > count + 15) {
        try w.writeByte(digits[0]);
        if (n > 1) {
            try w.writeByte('.');
            try w.writeAll(digits[1..n]);
        }
        const e = point - 1;
        try w.writeByte('e');
        try w.writeByte(if (e < 0) '-' else '+');
        const magnitude: u32 = @abs(e);
        if (magnitude < 10) try w.writeByte('0');
        try w.print("{d}", .{magnitude});
    } else if (point <= 0) {
        try w.writeAll("0.");
        for (0..@intCast(-point)) |_| try w.writeByte('0');
        try w.writeAll(digits[0..n]);
    } else if (point >= count) {
        try w.writeAll(digits[0..n]);
        for (0..@intCast(point - count)) |_| try w.writeByte('0');
    } else {
        const p: usize = @intCast(point);
        try w.writeAll(digits[0..p]);
        try w.writeByte('.');
        try w.writeAll(digits[p..n]);
    }
}

const hex = "0123456789abcdef";

/// A text in quotes: `"`, `\`, the control characters and DEL escaped, nothing else.
pub fn writeString(w: *Io.Writer, bytes: []const u8) Io.Writer.Error!void {
    try w.writeByte('"');
    var start: usize = 0;
    for (bytes, 0..) |c, i| {
        if (c >= 0x20 and c != '"' and c != '\\' and c != 0x7f) continue;
        try w.writeAll(bytes[start..i]);
        start = i + 1;
        switch (c) {
            '"' => try w.writeAll("\\\""),
            '\\' => try w.writeAll("\\\\"),
            '\n' => try w.writeAll("\\n"),
            '\t' => try w.writeAll("\\t"),
            '\r' => try w.writeAll("\\r"),
            0x08 => try w.writeAll("\\b"),
            0x0c => try w.writeAll("\\f"),
            else => try w.writeAll(&.{ '\\', 'u', '0', '0', hex[c >> 4], hex[c & 15] }),
        }
    }
    try w.writeAll(bytes[start..]);
    try w.writeByte('"');
}

fn writeIndent(w: *Io.Writer, style: Style, level: usize) Io.Writer.Error!void {
    try w.writeByte('\n');
    if (style.tab) {
        for (0..level) |_| try w.writeByte('\t');
    } else {
        for (0..level * style.indent) |_| try w.writeByte(' ');
    }
}

/// Writes `v` as JSON. It calls itself for what `v` holds, no deeper than 10,000 levels.
pub fn write(w: *Io.Writer, v: Value, style: Style, level: usize) Io.Writer.Error!void {
    // As jq, a value nested deeper than 10,000 levels is not written.
    if (level > 10_000) return w.writeAll("<skipped: too deep>");
    switch (v) {
        .null => try w.writeAll("null"),
        .false => try w.writeAll("false"),
        .true => try w.writeAll("true"),
        .number, .exact => |d| try writeDouble(w, d),
        .literal => |l| {
            // A literal too large for a double still writes its digits; NaN cannot be one.
            try w.writeAll(l.text());
        },
        .string => |s| try writeString(w, s.bytes.items),
        .array => |a| {
            const items = a.items.items;
            if (items.len == 0) return w.writeAll("[]");
            try w.writeByte('[');
            for (items, 0..) |item, i| {
                if (i > 0) try w.writeByte(',');
                if (style.pretty) try writeIndent(w, style, level + 1);
                try write(w, item, style, level + 1);
            }
            if (style.pretty) try writeIndent(w, style, level);
            try w.writeByte(']');
        },
        .object => |o| {
            if (o.map.count() == 0) return w.writeAll("{}");
            try w.writeByte('{');
            if (style.sort_keys) {
                const keys = sortedKeys(o);
                defer gpa.free(keys);
                for (keys, 0..) |key, i| try writeMember(w, key, o.map.get(key).?, style, level, i);
            } else {
                for (o.map.keys(), o.map.values(), 0..) |key, item, i| try writeMember(w, key, item, style, level, i);
            }
            if (style.pretty) try writeIndent(w, style, level);
            try w.writeByte('}');
        },
    }
}

fn writeMember(w: *Io.Writer, key: []const u8, item: Value, style: Style, level: usize, i: usize) Io.Writer.Error!void {
    if (i > 0) try w.writeByte(',');
    if (style.pretty) try writeIndent(w, style, level + 1);
    try writeString(w, key);
    try w.writeAll(if (style.pretty) ": " else ":");
    try write(w, item, style, level + 1);
}

/// Appends `v` on one line, as `tojson` writes it, to `list`.
pub fn appendCompact(list: *std.ArrayList(u8), v: Value) void {
    var w: Io.Writer.Allocating = .fromArrayList(gpa, list);
    write(&w.writer, v, .{ .pretty = false }, 0) catch oom();
    list.* = w.toArrayList();
}

/// `v` on one line, as `tojson` writes it.
pub fn toJson(v: Value) Value {
    var list: std.ArrayList(u8) = .empty;
    appendCompact(&list, v);
    return stringFromList(list);
}

/// `tostring`: a text as it is, anything else as `tojson` writes it. Takes `v`.
pub fn toText(v: Value) Value {
    if (v == .string) return v;
    defer release(v);
    return toJson(v);
}

// ---------------------------------------------------------------------------------------------
// UTF-8

/// The number of characters of a valid UTF-8 text.
pub fn codepointCount(bytes: []const u8) usize {
    var n: usize = 0;
    for (bytes) |c| {
        if (c & 0xC0 != 0x80) n += 1;
    }
    return n;
}

/// The offset of the character `index` of a valid UTF-8 text, or its length past the end.
pub fn codepointOffset(bytes: []const u8, index: usize) usize {
    var n: usize = 0;
    for (bytes, 0..) |c, i| {
        if (c & 0xC0 != 0x80) {
            if (n == index) return i;
            n += 1;
        }
    }
    return bytes.len;
}

const replacement = "\xEF\xBF\xBD";

/// Appends `bytes` to `out` with each bad sequence replaced by U+FFFD, as jq's decoder
/// (`jvp_utf8_next`) reads them: the first byte gives the length; a byte inside that is not a
/// continuation ends the sequence there and is read again; a sequence the text ends inside is
/// one U+FFFD; an overlong form, a surrogate or a value past U+10FFFF is one for the whole.
pub fn appendReplacingBadUtf8(out: *std.ArrayList(u8), bytes: []const u8) void {
    var i: usize = 0;
    while (i < bytes.len) {
        const first = bytes[i];
        if (first < 0x80) {
            out.append(gpa, first) catch oom();
            i += 1;
            continue;
        }
        const length: usize = switch (first) {
            0xC2...0xDF => 2,
            0xE0...0xEF => 3,
            0xF0...0xF4 => 4,
            else => 0,
        };
        if (length == 0) {
            out.appendSlice(gpa, replacement) catch oom();
            i += 1;
            continue;
        }
        if (i + length > bytes.len) {
            // A sequence the text ends inside: one U+FFFD for the rest.
            out.appendSlice(gpa, replacement) catch oom();
            i = bytes.len;
            continue;
        }
        var codepoint: u32 = first & (@as(u8, 0xFF) >> @intCast(length + 1));
        var taken: usize = 1;
        var good = true;
        while (taken < length) : (taken += 1) {
            const c = bytes[i + taken];
            if (c & 0xC0 != 0x80) {
                good = false;
                break;
            }
            codepoint = (codepoint << 6) | (c & 0x3F);
        }
        const smallest: u32 = switch (length) {
            2 => 0x80,
            3 => 0x800,
            else => 0x10000,
        };
        if (good and codepoint >= smallest and !(codepoint >= 0xD800 and codepoint <= 0xDFFF) and codepoint <= 0x10FFFF) {
            out.appendSlice(gpa, bytes[i .. i + length]) catch oom();
        } else {
            out.appendSlice(gpa, replacement) catch oom();
        }
        i += taken;
    }
}

/// A text made of any bytes: kept when they are valid UTF-8, else with the bad ones replaced.
pub fn stringFromBytes(bytes: []const u8) Value {
    if (std.unicode.utf8ValidateSlice(bytes)) return string(bytes);
    var list: std.ArrayList(u8) = .empty;
    appendReplacingBadUtf8(&list, bytes);
    return stringFromList(list);
}
