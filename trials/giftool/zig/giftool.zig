//! `giftool`: reads a GIF, changes it and writes it again, or tells what it holds as `gifsicle -I`
//! does (choices 1874-1881). What each option does, and the algorithm the three versions share, is in
//! `trials/giftool/README.md`.
//!
//!     giftool --info [FILE...]
//!     giftool [--unoptimize] [-O1|-O2] [--crop X,Y+WxH] [--flip-horizontal|--flip-vertical]
//!             [--rotate-90|--rotate-180|--rotate-270] [-o OUT] [IN]
//!
//! The input is read whole, and its frames are read, changed and written one after another into the
//! output, which is written at the end, so that a GIF found broken halfway leaves nothing behind.
//! Memory comes from `std.heap.smp_allocator`.

const std = @import("std");
const Io = std.Io;
const eql = std.mem.eql;

const gpa = std.heap.smp_allocator;

const max_pixels: usize = 268435456;
const hash_size = 5003;
const max_codes = 4096;
// The color of a pixel nothing is drawn on; a drawn one has its top byte set.
const clear_color: u32 = 0;
const opaque_color: u32 = 0xFF000000;

const Bytes = std.ArrayList(u8);
const Colors = std.ArrayList(u32);
const ColorMap = std.AutoHashMap(u32, usize);

/// What is wrong with a GIF, or with what was asked of it, as the standard error tells it.
fn messageOf(err: anyerror) []const u8 {
    return switch (err) {
        error.NotAGif => "not a GIF",
        error.EndsTooSoon => "the GIF ends too soon",
        error.NoScreenSize => "the screen has no size",
        error.ScreenTooLarge => "the screen has more pixels than giftool takes",
        error.ControlNotFour => "a graphic control extension is not four bytes",
        error.AppNameNotEleven => "an application extension's name is not eleven bytes",
        error.LoopNotThree => "a loop extension is not three bytes",
        error.FrameNoSize => "a frame has no size",
        error.FramePast65535 => "a frame goes past 65535",
        error.FrameTooLarge => "a frame has more pixels than giftool takes",
        error.BadCodeSize => "a frame's code size is not one GIF allows",
        error.BadBlock => "a block of a kind GIF does not have",
        error.PixelPast255 => "a frame's data has a pixel past 255",
        error.CodeNotInTable => "a frame's data has a code that is not in its table",
        error.DataEndsEarly => "a frame's data ends before its pixels do",
        error.NoFrames => "the GIF has no frames",
        error.CropOutside => "the crop is outside the screen",
        error.TooManyColors => "a frame has more than 256 colors",
        error.OutOfMemory => "out of memory",
        else => @errorName(err),
    };
}

/// A frame as it was read, its pixels not yet: where its table and its data lie in the input.
const Frame = struct {
    left: usize = 0,
    top: usize = 0,
    width: usize = 0,
    height: usize = 0,
    interlace: bool = false,
    local_bits: u4 = 0,
    local_at: usize = 0,
    transparent: ?u8 = null,
    disposal: u3 = 0,
    delay: u16 = 0,
    code_size: u4 = 0,
    data_at: usize = 0,
    comments: std.ArrayList([]const u8) = .empty,
    extensions: usize = 0,

    fn deinit(f: *Frame) void {
        for (f.comments.items) |c| gpa.free(c);
        f.comments.deinit(gpa);
    }
};

const Gif = struct {
    width: usize = 0,
    height: usize = 0,
    global_bits: u4 = 0,
    global_at: usize = 0,
    background: u8 = 0,
    loop: ?u16 = null,
    frames: std.ArrayList(Frame) = .empty,
    end: Frame = .{},

    fn deinit(g: *Gif) void {
        for (g.frames.items) |*f| f.deinit();
        g.frames.deinit(gpa);
        g.end.deinit();
    }
};

/// A frame to be written: its place, its table (none for the screen's) and its indices.
const Out = struct {
    left: usize = 0,
    top: usize = 0,
    width: usize = 1,
    height: usize = 1,
    table_bits: u4 = 0,
    table: Bytes = .empty,
    transparent: ?u8 = null,
    disposal: u3 = 0,
    delay: u16 = 0,
    comments: []const []const u8 = &.{},
    pixels: Bytes = .empty,

    fn deinit(o: *Out) void {
        o.table.deinit(gpa);
        o.pixels.deinit(gpa);
    }
};

const Flip = enum { none, horizontal, vertical };

/// What the options ask for.
const Options = struct {
    unoptimize: bool = false,
    optimize: u2 = 0,
    crop: bool = false,
    crop_x: usize = 0,
    crop_y: usize = 0,
    crop_width: usize = 0,
    crop_height: usize = 0,
    flip: Flip = .none,
    rotate: u16 = 0,
};

/// The tables of the decoder and of the encoder, made once.
const Lzw = struct {
    prefix: [max_codes]u16 = undefined,
    suffix: [max_codes]u8 = undefined,
    first: [max_codes]u8 = undefined,
    length: [max_codes]u16 = undefined,
    joined: Bytes = .empty,
    keys: [hash_size]i32 = undefined,
    codes: [hash_size]u16 = undefined,
};

// ---- reading ----

const Cursor = struct {
    data: []const u8,
    at: usize,

    fn next(c: *Cursor) !u8 {
        if (c.at >= c.data.len) return error.EndsTooSoon;
        const b = c.data[c.at];
        c.at += 1;
        return b;
    }

    fn next16(c: *Cursor) !u16 {
        if (c.at + 2 > c.data.len) return error.EndsTooSoon;
        const v = @as(u16, c.data[c.at]) | @as(u16, c.data[c.at + 1]) << 8;
        c.at += 2;
        return v;
    }

    fn skip(c: *Cursor, n: usize) !void {
        if (c.at + n > c.data.len) return error.EndsTooSoon;
        c.at += n;
    }

    /// Past the sub-blocks, up to the empty one.
    fn skipBlocks(c: *Cursor) !void {
        while (true) {
            const n = try c.next();
            if (n == 0) return;
            try c.skip(n);
        }
    }

    fn joinBlocks(c: *Cursor) ![]const u8 {
        var out: Bytes = .empty;
        errdefer out.deinit(gpa);
        while (true) {
            const n = try c.next();
            if (n == 0) return out.toOwnedSlice(gpa);
            const from = c.at;
            try c.skip(n);
            try out.appendSlice(gpa, c.data[from..c.at]);
        }
    }
};

/// The whole GIF but its pixels; whatever is not as GIF has it is an error.
fn readGif(data: []const u8) !Gif {
    if (data.len < 6 or !(eql(u8, data[0..6], "GIF87a") or eql(u8, data[0..6], "GIF89a"))) {
        return error.NotAGif;
    }
    var c: Cursor = .{ .data = data, .at = 6 };
    var gif: Gif = .{};
    errdefer gif.deinit();
    gif.width = try c.next16();
    gif.height = try c.next16();
    const flags = try c.next();
    gif.background = try c.next();
    _ = try c.next();
    if (gif.width == 0 or gif.height == 0) return error.NoScreenSize;
    if (gif.width * gif.height > max_pixels) return error.ScreenTooLarge;
    if (flags & 0x80 != 0) {
        gif.global_bits = @intCast((flags & 7) + 1);
        gif.global_at = c.at;
        try c.skip(@as(usize, 3) << gif.global_bits);
    }
    // The frame being read: what comes before its descriptor is its own; after the last, the end's.
    const frame = &gif.end;
    while (true) {
        const block = try c.next();
        if (block == 0x3B) break;
        if (block == 0x21) {
            const kind = try c.next();
            if (kind == 0xF9) {
                if (try c.next() != 4) return error.ControlNotFour;
                const p = try c.next();
                frame.disposal = @intCast((p >> 2) & 7);
                frame.delay = try c.next16();
                const t = try c.next();
                frame.transparent = null;
                if (p & 1 != 0) frame.transparent = t;
                if (try c.next() != 0) return error.ControlNotFour;
            } else if (kind == 0xFE) {
                const comment = try c.joinBlocks();
                errdefer gpa.free(comment);
                try frame.comments.append(gpa, comment);
            } else if (kind == 0xFF) {
                if (try c.next() != 11) return error.AppNameNotEleven;
                const name = c.at;
                try c.skip(11);
                const app = data[name..c.at];
                if (eql(u8, app, "NETSCAPE2.0") or eql(u8, app, "ANIMEXTS1.0")) {
                    if (try c.next() != 3) return error.LoopNotThree;
                    _ = try c.next();
                    gif.loop = try c.next16();
                    if (try c.next() != 0) return error.LoopNotThree;
                } else {
                    try c.skipBlocks();
                    frame.extensions += 1;
                }
            } else {
                try c.skipBlocks();
                frame.extensions += 1;
            }
        } else if (block == 0x2C) {
            frame.left = try c.next16();
            frame.top = try c.next16();
            frame.width = try c.next16();
            frame.height = try c.next16();
            const p = try c.next();
            if (frame.width == 0 or frame.height == 0) return error.FrameNoSize;
            if (frame.left + frame.width > 65535 or frame.top + frame.height > 65535) return error.FramePast65535;
            if (frame.width * frame.height > max_pixels) return error.FrameTooLarge;
            if (p & 0x80 != 0) {
                frame.local_bits = @intCast((p & 7) + 1);
                frame.local_at = c.at;
                try c.skip(@as(usize, 3) << frame.local_bits);
            }
            frame.interlace = p & 0x40 != 0;
            const code_size = try c.next();
            if (code_size < 2 or code_size > 11) return error.BadCodeSize;
            frame.code_size = @intCast(code_size);
            frame.data_at = c.at;
            try c.skipBlocks();
            try gif.frames.append(gpa, frame.*);
            frame.* = .{};
        } else {
            return error.BadBlock;
        }
    }
    return gif;
}

/// The indices of a frame from its LZW data, into `pixels[0..count]`.
fn decode(data: []const u8, frame: *const Frame, lzw: *Lzw, pixels: []u8) !void {
    const count = frame.width * frame.height;
    var at = frame.data_at;
    lzw.joined.clearRetainingCapacity();
    while (true) {
        const n: usize = data[at];
        at += 1;
        if (n == 0) break;
        try lzw.joined.appendSlice(gpa, data[at .. at + n]);
        at += n;
    }
    const joined = lzw.joined.items;
    const code_size: u5 = frame.code_size;
    const clear = @as(usize, 1) << code_size;
    const end = clear + 1;
    for (0..@min(clear, 256)) |c| {
        lzw.suffix[c] = @intCast(c);
        lzw.first[c] = @intCast(c);
        lzw.length[c] = 1;
    }
    var width: u5 = code_size + 1;
    var mask = (@as(usize, 1) << width) - 1;
    var free = clear + 2;
    var previous: ?usize = null;
    var pos: usize = 0;
    var bits: u32 = 0;
    var have: u5 = 0;
    var i: usize = 0;
    const n = joined.len;
    while (pos < count) {
        while (have < width and i < n) {
            bits |= @as(u32, joined[i]) << have;
            have += 8;
            i += 1;
        }
        if (have < width) break;
        const code = @as(usize, bits) & mask;
        bits >>= width;
        have -= width;
        if (code == clear) {
            width = code_size + 1;
            mask = (@as(usize, 1) << width) - 1;
            free = clear + 2;
            previous = null;
            continue;
        }
        if (code == end) break;
        if (code > 255 and code < clear) return error.PixelPast255;
        const prev = previous orelse {
            if (code >= clear) return error.CodeNotInTable;
            pixels[pos] = @intCast(code);
            pos += 1;
            previous = code;
            continue;
        };
        if (code > free or (code == free and free >= max_codes)) return error.CodeNotInTable;
        if (free < max_codes) {
            lzw.prefix[free] = @intCast(prev);
            if (code < free) {
                lzw.suffix[free] = lzw.first[code];
            } else {
                lzw.suffix[free] = lzw.first[prev];
            }
            lzw.first[free] = lzw.first[prev];
            lzw.length[free] = lzw.length[prev] + 1;
            free += 1;
            if (free == (@as(usize, 1) << width) and width < 12) {
                width += 1;
                mask = (@as(usize, 1) << width) - 1;
            }
        }
        // The string of the code, written from its end back.
        const len: usize = lzw.length[code];
        var stop = pos + len;
        var c = code;
        var k = stop - 1;
        if (stop > count) {
            while (k >= count) {
                c = lzw.prefix[c];
                k -= 1;
            }
            stop = count;
        }
        while (k > pos) {
            pixels[k] = lzw.suffix[c];
            c = lzw.prefix[c];
            k -= 1;
        }
        pixels[pos] = lzw.suffix[c];
        pos = stop;
        previous = code;
    }
    if (pos < count) return error.DataEndsEarly;
}

/// The rows of an interlaced frame put in their order.
fn deinterlace(from: []const u8, to: []u8, width: usize, height: usize) void {
    var row: usize = 0;
    const starts = [4]usize{ 0, 4, 2, 1 };
    const steps = [4]usize{ 8, 8, 4, 2 };
    for (starts, steps) |start, step| {
        var y = start;
        while (y < height) : (y += step) {
            @memcpy(to[y * width .. (y + 1) * width], from[row * width .. (row + 1) * width]);
            row += 1;
        }
    }
}

// ---- writing ----

fn put16(out: *Bytes, v: usize) !void {
    try out.append(gpa, @intCast(v & 0xFF));
    try out.append(gpa, @intCast(v >> 8));
}

fn putBlocks(out: *Bytes, data: []const u8) !void {
    var at: usize = 0;
    while (at < data.len) {
        const n = @min(data.len - at, 255);
        try out.append(gpa, @intCast(n));
        try out.appendSlice(gpa, data[at .. at + n]);
        at += n;
    }
    try out.append(gpa, 0);
}

fn bitsFor(n: usize) u4 {
    var bits: u4 = 1;
    while ((@as(usize, 1) << bits) < n) bits += 1;
    return bits;
}

/// The LZW of `pixels` in sub-blocks, after its code size (README, "Writing").
fn encode(out: *Bytes, pixels: []const u8, code_size: u4, lzw: *Lzw) !void {
    try out.append(gpa, code_size);
    const clear = @as(u32, 1) << code_size;
    var width: u5 = @as(u5, code_size) + 1;
    var free: u32 = clear + 2;
    @memset(&lzw.keys, -1);
    // The bits not yet written, and the sub-block being filled: its length byte is at `length_at`.
    var bits: u64 = clear;
    var have: u6 = width;
    var length_at = out.items.len;
    try out.append(gpa, 0);
    var in_block: usize = 0;
    var ent: u32 = pixels[0];
    for (pixels[1..]) |p| {
        const c: u32 = p;
        const key: i32 = @intCast((c << 12) + ent);
        var h: usize = (c << 4) ^ ent;
        if (lzw.keys[h] == key) {
            ent = lzw.codes[h];
            continue;
        }
        if (lzw.keys[h] >= 0) {
            const step: usize = if (h == 0) 1 else hash_size - h;
            var found = false;
            while (true) {
                if (h >= step) h -= step else h = h + hash_size - step;
                if (lzw.keys[h] == key) {
                    found = true;
                    break;
                }
                if (lzw.keys[h] < 0) break;
            }
            if (found) {
                ent = lzw.codes[h];
                continue;
            }
        }
        bits |= @as(u64, ent) << have;
        have += width;
        if (free >= (@as(u32, 1) << width) and width < 12) width += 1;
        if (free < max_codes) {
            lzw.keys[h] = key;
            lzw.codes[h] = @intCast(free);
            free += 1;
        } else {
            bits |= @as(u64, clear) << have;
            have += width;
            @memset(&lzw.keys, -1);
            width = @as(u5, code_size) + 1;
            free = clear + 2;
        }
        while (have >= 8) {
            try out.append(gpa, @truncate(bits));
            bits >>= 8;
            have -= 8;
            in_block += 1;
            if (in_block == 255) {
                out.items[length_at] = 255;
                length_at = out.items.len;
                try out.append(gpa, 0);
                in_block = 0;
            }
        }
        ent = c;
    }
    bits |= @as(u64, ent) << have;
    have += width;
    if (free >= (@as(u32, 1) << width) and width < 12) width += 1;
    bits |= @as(u64, clear + 1) << have;
    have += width;
    while (have > 0) {
        try out.append(gpa, @truncate(bits));
        bits >>= 8;
        have -|= 8;
        in_block += 1;
        if (in_block == 255) {
            out.items[length_at] = 255;
            length_at = out.items.len;
            try out.append(gpa, 0);
            in_block = 0;
        }
    }
    if (in_block > 0) {
        out.items[length_at] = @intCast(in_block);
        try out.append(gpa, 0);
    }
}

fn writeFrame(out: *Bytes, f: *const Out, pixels: []const u8, global_bits: u4, lzw: *Lzw) !void {
    for (f.comments) |comment| {
        try out.appendSlice(gpa, &.{ 0x21, 0xFE });
        try putBlocks(out, comment);
    }
    if (f.disposal != 0 or f.delay != 0 or f.transparent != null) {
        try out.appendSlice(gpa, &.{ 0x21, 0xF9, 4 });
        const packed_disposal = @as(u8, f.disposal) << 2;
        try out.append(gpa, if (f.transparent != null) packed_disposal | 1 else packed_disposal);
        try put16(out, f.delay);
        try out.append(gpa, f.transparent orelse 0);
        try out.append(gpa, 0);
    }
    try out.append(gpa, 0x2C);
    try put16(out, f.left);
    try put16(out, f.top);
    try put16(out, f.width);
    try put16(out, f.height);
    var bits = global_bits;
    if (f.table_bits > 0) {
        try out.append(gpa, 0x80 | @as(u8, f.table_bits - 1));
        try out.appendSlice(gpa, f.table.items);
        bits = f.table_bits;
    } else {
        try out.append(gpa, 0);
    }
    var most: u8 = 0;
    for (pixels) |p| {
        if (p > most) most = p;
    }
    var code_size = bitsFor(@as(usize, most) + 1);
    if (bits > code_size) code_size = bits;
    if (code_size < 2) code_size = 2;
    try encode(out, pixels, code_size, lzw);
}

// ---- frames kept as indices: cut to the screen and the crop, flipped, turned ----

const View = struct {
    // The crop on the screen, and the size of what is written.
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    out_width: usize,
    out_height: usize,
};

const Placed = struct {
    left: usize,
    top: usize,
    width: usize,
    height: usize,
};

/// A frame of `w` by `h` at `left`, `top` of the cropped screen, flipped and turned: its new place
/// and size.
fn place(view: View, o: Options, left: usize, top: usize, w: usize, h: usize) Placed {
    var l = left;
    var t = top;
    switch (o.flip) {
        .horizontal => l = view.width - left - w,
        .vertical => t = view.height - top - h,
        .none => {},
    }
    return switch (o.rotate) {
        90 => .{ .left = view.height - t - h, .top = l, .width = h, .height = w },
        180 => .{ .left = view.width - l - w, .top = view.height - t - h, .width = w, .height = h },
        270 => .{ .left = t, .top = view.width - l - w, .width = h, .height = w },
        else => .{ .left = l, .top = t, .width = w, .height = h },
    };
}

/// Where the pixels of a part of a picture come from once flipped and turned: the pixel at (x, y)
/// of the result is at `base + x * step_x + y * step_y` of the picture.
const Walk = struct {
    base: isize,
    step_x: isize,
    step_y: isize,
    width: usize,
    height: usize,
};

/// The walk over the part `w` by `h` at `origin` of a picture `stride` wide: the flip undone after
/// the turn, each a map of (x, y) that is a sum of x, y and a number.
fn walkOf(o: Options, origin: usize, stride_: usize, w_: usize, h_: usize) Walk {
    const stride: isize = @intCast(stride_);
    const w: isize = @intCast(w_);
    const h: isize = @intCast(h_);
    var ax: isize = 1;
    var bx: isize = 0;
    var cx: isize = 0;
    var ay: isize = 0;
    var by: isize = 1;
    var cy: isize = 0;
    var ow = w_;
    var oh = h_;
    if (o.rotate == 90) {
        ax = 0;
        bx = 1;
        ay = -1;
        by = 0;
        cy = h - 1;
        ow = h_;
        oh = w_;
    } else if (o.rotate == 180) {
        ax = -1;
        cx = w - 1;
        by = -1;
        cy = h - 1;
    } else if (o.rotate == 270) {
        ax = 0;
        bx = -1;
        cx = w - 1;
        ay = 1;
        by = 0;
        ow = h_;
        oh = w_;
    }
    if (o.flip == .horizontal) {
        ax = -ax;
        bx = -bx;
        cx = w - 1 - cx;
    } else if (o.flip == .vertical) {
        ay = -ay;
        by = -by;
        cy = h - 1 - cy;
    }
    return .{
        .base = @as(isize, @intCast(origin)) + cx + cy * stride,
        .step_x = ax + ay * stride,
        .step_y = bx + by * stride,
        .width = ow,
        .height = oh,
    };
}

/// The pixels of `from` along `walk`, into `to`: indices or colors.
fn walkInto(comptime T: type, from: []const T, walk: Walk, to: *std.ArrayList(T)) !void {
    to.clearRetainingCapacity();
    try to.ensureTotalCapacity(gpa, walk.width * walk.height);
    for (0..walk.height) |y| {
        var at = walk.base + @as(isize, @intCast(y)) * walk.step_y;
        if (walk.step_x == 1) {
            const a: usize = @intCast(at);
            to.appendSliceAssumeCapacity(from[a .. a + walk.width]);
            continue;
        }
        for (0..walk.width) |_| {
            to.appendAssumeCapacity(from[@intCast(at)]);
            at += walk.step_x;
        }
    }
}

fn tableCopy(data: []const u8, at: usize, bits: u4) !Bytes {
    var t: Bytes = .empty;
    if (bits > 0) try t.appendSlice(gpa, data[at .. at + (@as(usize, 3) << bits)]);
    return t;
}

/// A frame with its indices: the part of it on the cropped screen, flipped and turned, into
/// `turned`; `whole` when nothing of it changes and its pixels are written as they were read.
fn indexFrame(data: []const u8, f: *const Frame, pixels: []const u8, view: View, o: Options, turned: *Bytes, whole: *bool) !Out {
    var x0 = f.left;
    var y0 = f.top;
    var x1 = f.left + f.width;
    var y1 = f.top + f.height;
    if (x0 < view.x) x0 = view.x;
    if (y0 < view.y) y0 = view.y;
    if (x1 > view.x + view.width) x1 = view.x + view.width;
    if (y1 > view.y + view.height) y1 = view.y + view.height;
    var out: Out = .{
        .left = 0,
        .top = 0,
        .width = 1,
        .height = 1,
        .table_bits = f.local_bits,
        .table = try tableCopy(data, f.local_at, f.local_bits),
        .transparent = 0,
        .disposal = 1,
        .delay = f.delay,
        .comments = f.comments.items,
    };
    whole.* = false;
    if (x0 >= x1 or y0 >= y1) {
        // Nothing of it is on the screen: a pixel that is not drawn.
        turned.clearRetainingCapacity();
        try turned.append(gpa, 0);
        return out;
    }
    const w = x1 - x0;
    const h = y1 - y0;
    const at = place(view, o, x0 - view.x, y0 - view.y, w, h);
    out.left = at.left;
    out.top = at.top;
    out.width = at.width;
    out.height = at.height;
    out.transparent = f.transparent;
    out.disposal = f.disposal;
    whole.* = w == f.width and h == f.height and o.flip == .none and o.rotate == 0;
    if (!whole.*) {
        try walkInto(u8, pixels, walkOf(o, (y0 - f.top) * f.width + x0 - f.left, f.width, w, h), turned);
    }
    return out;
}

// ---- frames put together on the screen ----

/// The colors of a table, `opaque_color` set; an index past it is black.
fn colorsOf(data: []const u8, at: usize, bits: u4, colors: *[256]u32) void {
    @memset(colors, opaque_color);
    if (bits == 0) return;
    for (0..@as(usize, 1) << bits) |i| {
        const p = at + 3 * i;
        colors[i] = opaque_color | @as(u32, data[p]) << 16 | @as(u32, data[p + 1]) << 8 | @as(u32, data[p + 2]);
    }
}

/// Indices and a table for the colors of a frame: the screen's table when it holds every one of
/// them (and an index none of them takes, when one is clear), or a table of their own in the order
/// they come.
fn paletteOf(colors: []const u32, global: *const ColorMap, global_bits: u4, out: *Out) !void {
    out.pixels.clearRetainingCapacity();
    var clear = false;
    var fits = global_bits > 0;
    if (fits) {
        var used: [256]bool = @splat(false);
        var last = clear_color;
        var last_index: usize = 0;
        for (colors) |c| {
            if (c == clear_color) {
                clear = true;
                try out.pixels.append(gpa, 0);
                continue;
            }
            if (c != last) {
                const found = global.get(c) orelse {
                    fits = false;
                    break;
                };
                last = c;
                last_index = found;
                used[last_index] = true;
            }
            try out.pixels.append(gpa, @intCast(last_index));
        }
        if (fits) {
            out.table_bits = 0;
            out.table.clearRetainingCapacity();
            out.transparent = null;
            if (!clear) return;
            var free: ?usize = null;
            for (0..@as(usize, 1) << global_bits) |i| {
                if (!used[i]) {
                    free = i;
                    break;
                }
            }
            if (free) |t| {
                out.transparent = @intCast(t);
                // the clear pixels were written as 0 for now
                for (colors, 0..) |c, k| {
                    if (c == clear_color) out.pixels.items[k] = @intCast(t);
                }
                return;
            }
            fits = false;
        }
    }
    out.pixels.clearRetainingCapacity();
    clear = false;
    var local = ColorMap.init(gpa);
    defer local.deinit();
    var order: Colors = .empty;
    defer order.deinit(gpa);
    var last = clear_color;
    var last_index: usize = 0;
    for (colors) |c| {
        if (c == clear_color) {
            clear = true;
            try out.pixels.append(gpa, 0);
            continue;
        }
        if (c != last) {
            const entry = try local.getOrPut(c);
            if (!entry.found_existing) entry.value_ptr.* = order.items.len;
            const index = entry.value_ptr.*;
            if (index == order.items.len) try order.append(gpa, c);
            last = c;
            last_index = index;
        }
        try out.pixels.append(gpa, @truncate(last_index));
    }
    var count = order.items.len;
    if (clear) count += 1;
    if (count > 256) return error.TooManyColors;
    if (clear) {
        const t: u8 = @intCast(order.items.len);
        for (colors, 0..) |c, k| {
            if (c == clear_color) out.pixels.items[k] = t;
        }
        out.transparent = t;
    } else {
        out.transparent = null;
    }
    const bits = bitsFor(@max(count, 2));
    out.table_bits = bits;
    out.table.clearRetainingCapacity();
    for (order.items) |c| {
        try out.table.appendSlice(gpa, &.{ @truncate(c >> 16), @truncate(c >> 8), @truncate(c) });
    }
    while (out.table.items.len < (@as(usize, 3) << bits)) try out.table.append(gpa, 0);
}

const Rect = struct {
    x0: usize = 0,
    y0: usize = 0,
    x1: usize = 0,
    y1: usize = 0,

    fn isEmpty(r: Rect) bool {
        return r.x0 >= r.x1 or r.y0 >= r.y1;
    }
};

fn joinedRect(a: Rect, b: Rect) Rect {
    if (a.isEmpty()) return b;
    if (b.isEmpty()) return a;
    return .{ .x0 = @min(a.x0, b.x0), .y0 = @min(a.y0, b.y0), .x1 = @max(a.x1, b.x1), .y1 = @max(a.y1, b.y1) };
}

/// The smallest rectangle holding the pixels where `before` and `after` differ; with `cleared` only
/// those drawn before and clear after.
fn differing(before: []const u32, after: []const u32, w: usize, h: usize, cleared: bool) Rect {
    var x0 = w;
    var y0 = h;
    var x1: usize = 0;
    var y1: usize = 0;
    for (0..h) |y| {
        const row = y * w;
        for (0..w) |x| {
            const a = before[row + x];
            const b = after[row + x];
            var differs = a != b;
            if (cleared) differs = a != clear_color and b == clear_color;
            if (differs) {
                if (x < x0) x0 = x;
                if (x >= x1) x1 = x + 1;
                if (y < y0) y0 = y;
                y1 = y + 1;
            }
        }
    }
    if (x0 >= x1) return .{};
    return .{ .x0 = x0, .y0 = y0, .x1 = x1, .y1 = y1 };
}

/// A frame of `-O` not yet written: the next frame may still make it larger and have it cleared.
const Pending = struct {
    there: bool = false,
    rect: Rect = .{},
    colors: Colors = .empty,
    delay: u16 = 0,
    disposal: u3 = 1,
    comments: []const []const u8 = &.{},
};

fn writePending(out: *Bytes, p: *Pending, global: *const ColorMap, global_bits: u4, lzw: *Lzw) !void {
    var f: Out = .{
        .left = p.rect.x0,
        .top = p.rect.y0,
        .width = p.rect.x1 - p.rect.x0,
        .height = p.rect.y1 - p.rect.y0,
        .disposal = p.disposal,
        .delay = p.delay,
        .comments = p.comments,
    };
    defer f.deinit();
    try paletteOf(p.colors.items, global, global_bits, &f);
    try writeFrame(out, &f, f.pixels.items, global_bits, lzw);
    p.there = false;
}

/// The colors of `rect` of a screen `w` wide: of `now`, or clear where `before` shows the same, for
/// -O2.
fn cutColors(now: []const u32, before: []const u32, w: usize, rect: Rect, same_clear: bool, to: *Colors) !void {
    to.clearRetainingCapacity();
    for (rect.y0..rect.y1) |y| {
        const row = y * w;
        for (rect.x0..rect.x1) |x| {
            const c = now[row + x];
            if (same_clear and before[row + x] == c) {
                try to.append(gpa, clear_color);
            } else {
                try to.append(gpa, c);
            }
        }
    }
}

// ---- the program ----

fn rewrite(data: []const u8, gif: *const Gif, o: Options) !Bytes {
    if (gif.frames.items.len == 0) return error.NoFrames;
    var view: View = .{
        .x = 0,
        .y = 0,
        .width = gif.width,
        .height = gif.height,
        .out_width = gif.width,
        .out_height = gif.height,
    };
    if (o.crop) {
        if (o.crop_x >= gif.width or o.crop_y >= gif.height) return error.CropOutside;
        view.x = o.crop_x;
        view.y = o.crop_y;
        view.width = @min(o.crop_width, gif.width - o.crop_x);
        view.height = @min(o.crop_height, gif.height - o.crop_y);
    }
    const sideways = o.rotate == 90 or o.rotate == 270;
    if (sideways) {
        view.out_width = view.height;
        view.out_height = view.width;
    } else {
        view.out_width = view.width;
        view.out_height = view.height;
    }

    var out: Bytes = try .initCapacity(gpa, data.len + 1024);
    errdefer out.deinit(gpa);
    try out.appendSlice(gpa, "GIF89a");
    try put16(&out, view.out_width);
    try put16(&out, view.out_height);
    if (gif.global_bits > 0) {
        try out.append(gpa, 0x80 | 0x70 | @as(u8, gif.global_bits - 1));
    } else {
        try out.append(gpa, 0);
    }
    try out.append(gpa, gif.background);
    try out.append(gpa, 0);
    if (gif.global_bits > 0) {
        try out.appendSlice(gpa, data[gif.global_at .. gif.global_at + (@as(usize, 3) << gif.global_bits)]);
    }
    if (gif.loop) |loop| {
        try out.appendSlice(gpa, &.{ 0x21, 0xFF, 11 });
        try out.appendSlice(gpa, "NETSCAPE2.0");
        try out.appendSlice(gpa, &.{ 3, 1 });
        try put16(&out, loop);
        try out.append(gpa, 0);
    }

    const lzw = try gpa.create(Lzw);
    defer gpa.destroy(lzw);
    lzw.* = .{};
    defer lzw.joined.deinit(gpa);
    var indices: []u8 = &.{};
    defer gpa.free(indices);
    var rows: []u8 = &.{};
    defer gpa.free(rows);
    const together = o.unoptimize or o.optimize > 0;
    // The screen as drawn so far, what a frame of `previous` puts back, and the views of the screen.
    var screen: usize = 0;
    var seen: usize = 0;
    if (together) {
        screen = gif.width * gif.height;
        seen = view.out_width * view.out_height;
    }
    const canvas = try gpa.alloc(u32, screen);
    defer gpa.free(canvas);
    @memset(canvas, clear_color);
    var saved: Colors = .empty;
    defer saved.deinit(gpa);
    var colors: [256]u32 = @splat(opaque_color);
    var global = ColorMap.init(gpa);
    defer global.deinit();
    if (gif.global_bits > 0) {
        colorsOf(data, gif.global_at, gif.global_bits, &colors);
        for (0..@as(usize, 1) << gif.global_bits) |i| {
            const entry = try global.getOrPut(colors[i]);
            if (!entry.found_existing) entry.value_ptr.* = i;
        }
    }
    var turned: Bytes = .empty;
    defer turned.deinit(gpa);
    var whole = false;
    const screen_walk = walkOf(o, view.y * gif.width + view.x, gif.width, view.width, view.height);
    var now: Colors = try .initCapacity(gpa, seen);
    defer now.deinit(gpa);
    now.appendNTimesAssumeCapacity(clear_color, seen);
    const before = try gpa.alloc(u32, seen);
    defer gpa.free(before);
    @memset(before, clear_color);
    const base = try gpa.alloc(u32, seen);
    defer gpa.free(base);
    @memset(base, clear_color);
    var pending: Pending = .{};
    defer pending.colors.deinit(gpa);
    for (gif.frames.items) |*frame| {
        const count = frame.width * frame.height;
        if (indices.len < count) {
            gpa.free(indices);
            indices = try gpa.alloc(u8, count);
            @memset(indices, 0);
        }
        try decode(data, frame, lzw, indices);
        if (frame.interlace) {
            if (rows.len < count) {
                gpa.free(rows);
                rows = try gpa.alloc(u8, count);
                @memset(rows, 0);
            }
            deinterlace(indices[0..count], rows, frame.width, frame.height);
            @memcpy(indices[0..count], rows[0..count]);
        }
        if (!together) {
            var f = try indexFrame(data, frame, indices[0..count], view, o, &turned, &whole);
            defer f.deinit();
            if (whole) {
                try writeFrame(&out, &f, indices[0..count], gif.global_bits, lzw);
            } else {
                try writeFrame(&out, &f, turned.items, gif.global_bits, lzw);
            }
            continue;
        }
        // Drawn on the screen.
        if (frame.local_bits > 0) {
            colorsOf(data, frame.local_at, frame.local_bits, &colors);
        } else {
            colorsOf(data, gif.global_at, gif.global_bits, &colors);
        }
        const x0 = frame.left;
        const y0 = frame.top;
        const x1 = @min(frame.left + frame.width, gif.width);
        const y1 = @min(frame.top + frame.height, gif.height);
        // A frame wholly past the screen draws nothing, saves nothing and clears nothing.
        const drawn = x0 < x1 and y0 < y1;
        if (frame.disposal == 3) {
            saved.clearRetainingCapacity();
            if (drawn) {
                for (y0..y1) |y| try saved.appendSlice(gpa, canvas[y * gif.width + x0 .. y * gif.width + x1]);
            }
        }
        if (drawn) {
            for (y0..y1) |y| {
                const row = (y - frame.top) * frame.width;
                for (x0..x1) |x| {
                    const p = indices[row + x - frame.left];
                    if (frame.transparent) |t| {
                        if (p == t) continue;
                    }
                    canvas[y * gif.width + x] = colors[p];
                }
            }
        }
        try walkInto(u32, canvas, screen_walk, &now);
        if (drawn and frame.disposal == 2) {
            for (y0..y1) |y| {
                for (x0..x1) |x| canvas[y * gif.width + x] = clear_color;
            }
        } else if (drawn and frame.disposal == 3) {
            var k: usize = 0;
            for (y0..y1) |y| {
                for (x0..x1) |x| {
                    canvas[y * gif.width + x] = saved.items[k];
                    k += 1;
                }
            }
        }
        const vw = view.out_width;
        const vh = view.out_height;
        if (o.optimize == 0) {
            var f: Out = .{
                .left = 0,
                .top = 0,
                .width = vw,
                .height = vh,
                .disposal = 2,
                .delay = frame.delay,
                .comments = frame.comments.items,
            };
            defer f.deinit();
            try paletteOf(now.items, &global, gif.global_bits, &f);
            try writeFrame(&out, &f, f.pixels.items, gif.global_bits, lzw);
            continue;
        }
        // -O: what was drawn and is clear now is cleared by the frame before, made larger to hold it.
        @memcpy(base, before);
        if (pending.there) {
            const gone = differing(before, now.items, vw, vh, true);
            if (!gone.isEmpty()) {
                const grown = joinedRect(pending.rect, gone);
                var colors_before: Colors = .empty;
                errdefer colors_before.deinit(gpa);
                for (grown.y0..grown.y1) |y| {
                    for (grown.x0..grown.x1) |x| {
                        const r = pending.rect;
                        const inside = x >= r.x0 and x < r.x1 and y >= r.y0 and y < r.y1;
                        if (inside) {
                            const w = r.x1 - r.x0;
                            try colors_before.append(gpa, pending.colors.items[(y - r.y0) * w + x - r.x0]);
                        } else if (o.optimize == 2) {
                            try colors_before.append(gpa, clear_color);
                        } else {
                            try colors_before.append(gpa, before[y * vw + x]);
                        }
                    }
                }
                pending.rect = grown;
                pending.colors.deinit(gpa);
                pending.colors = colors_before;
                pending.disposal = 2;
                for (grown.y0..grown.y1) |y| {
                    for (grown.x0..grown.x1) |x| base[y * vw + x] = clear_color;
                }
            }
            try writePending(&out, &pending, &global, gif.global_bits, lzw);
        }
        var changed = differing(base, now.items, vw, vh, false);
        if (changed.isEmpty()) changed = .{ .x0 = 0, .y0 = 0, .x1 = 1, .y1 = 1 };
        pending.there = true;
        pending.rect = changed;
        try cutColors(now.items, base, vw, changed, o.optimize == 2, &pending.colors);
        pending.delay = frame.delay;
        pending.disposal = 1;
        pending.comments = frame.comments.items;
        @memcpy(before, now.items);
    }
    if (pending.there) try writePending(&out, &pending, &global, gif.global_bits, lzw);
    for (gif.end.comments.items) |comment| {
        try out.appendSlice(gpa, &.{ 0x21, 0xFE });
        try putBlocks(&out, comment);
    }
    try out.append(gpa, 0x3B);
    return out;
}

// ---- --info, as gifsicle -I writes it ----

fn safeText(s: *Bytes, data: []const u8) !void {
    for (data, 0..) |b, i| {
        if (b < 0x20 or b >= 0x7F or b == '\\') {
            switch (b) {
                7 => try s.appendSlice(gpa, "\\a"),
                8 => try s.appendSlice(gpa, "\\b"),
                12 => try s.appendSlice(gpa, "\\f"),
                10 => try s.appendSlice(gpa, "\\n"),
                13 => try s.appendSlice(gpa, "\\r"),
                9 => try s.appendSlice(gpa, "\\t"),
                11 => try s.appendSlice(gpa, "\\v"),
                '\\' => try s.appendSlice(gpa, "\\\\"),
                0 => if (data.len - i > 1) try s.appendSlice(gpa, "\\000"),
                else => try s.print(gpa, "\\{d}{d}{d}", .{ (b >> 6) & 7, (b >> 3) & 7, b & 7 }),
            }
        } else {
            try s.append(gpa, b);
        }
    }
}

fn infoOf(gif: *const Gif, name: []const u8, s: *Bytes) !void {
    const n = gif.frames.items.len;
    if (n == 0) return;
    if (n == 1) {
        try s.print(gpa, "* {s} 1 image\n", .{name});
    } else {
        try s.print(gpa, "* {s} {d} images\n", .{ name, n });
    }
    try s.print(gpa, "  logical screen {d}x{d}\n", .{ gif.width, gif.height });
    if (gif.global_bits > 0) {
        try s.print(gpa, "  global color table [{d}]\n", .{@as(usize, 1) << gif.global_bits});
        try s.print(gpa, "  background {d}\n", .{gif.background});
    }
    if (gif.loop) |loop| {
        if (loop == 0) {
            try s.appendSlice(gpa, "  loop forever\n");
        } else {
            try s.print(gpa, "  loop count {d}\n", .{loop});
        }
    }
    for (gif.end.comments.items) |c| {
        try s.appendSlice(gpa, "  end comment ");
        try safeText(s, c);
        try s.append(gpa, '\n');
    }
    if (gif.end.extensions > 0) try s.print(gpa, "  end extensions {d}\n", .{gif.end.extensions});
    for (gif.frames.items, 0..) |*f, i| {
        try s.print(gpa, "  + image #{d} {d}x{d}", .{ i, f.width, f.height });
        if (f.left != 0 or f.top != 0) try s.print(gpa, " at {d},{d}", .{ f.left, f.top });
        if (f.interlace) try s.appendSlice(gpa, " interlaced");
        if (f.transparent) |t| try s.print(gpa, " transparent {d}", .{t});
        try s.append(gpa, '\n');
        for (f.comments.items) |c| {
            try s.appendSlice(gpa, "    comment ");
            try safeText(s, c);
            try s.append(gpa, '\n');
        }
        if (f.extensions > 0) try s.print(gpa, "    extensions {d}\n", .{f.extensions});
        if (f.local_bits > 0) try s.print(gpa, "    local color table [{d}]\n", .{@as(usize, 1) << f.local_bits});
        if (f.disposal != 0 or f.delay != 0) {
            try s.appendSlice(gpa, "   ");
            if (f.disposal != 0) {
                try s.appendSlice(gpa, " disposal ");
                switch (f.disposal) {
                    1 => try s.appendSlice(gpa, "asis"),
                    2 => try s.appendSlice(gpa, "background"),
                    3 => try s.appendSlice(gpa, "previous"),
                    else => try s.print(gpa, "{d}", .{f.disposal}),
                }
            }
            if (f.delay != 0) try s.print(gpa, " delay {d}.{d:0>2}s", .{ f.delay / 100, f.delay % 100 });
            try s.append(gpa, '\n');
        }
    }
}

fn readInput(io: Io, name: []const u8) ![]u8 {
    if (!eql(u8, name, "-")) return Io.Dir.cwd().readFileAlloc(io, name, gpa, .unlimited);
    var buffer: [65536]u8 = undefined;
    var reader = Io.File.stdin().readerStreaming(io, &buffer);
    return reader.interface.allocRemaining(gpa, .unlimited) catch |err| switch (err) {
        error.ReadFailed => reader.err.?,
        else => |e| e,
    };
}

fn fail(name: []const u8, message: []const u8) noreturn {
    std.debug.print("giftool: {s}: {s}\n", .{ name, message });
    std.process.exit(1);
}

fn usage(comptime format: []const u8, args: anytype) noreturn {
    std.debug.print("giftool: " ++ format ++ "\nTry 'giftool --help'.\n", args);
    std.process.exit(2);
}

const help_text =
    \\Reads a GIF, changes it and writes it again, or tells what it holds.
    \\
    \\Usage: giftool [options] [files...]
    \\
    \\Arguments:
    \\  [files...]               The GIF to read, the input when none; with --info, any number
    \\
    \\Options:
    \\  -I, --info               Tell what each GIF holds, as gifsicle -I does
    \\      --unoptimize         Write each frame whole, as the screen shows it
    \\  -O, --optimize LEVEL     1: each frame only where the screen changed; 2: also clear what did not
    \\      --crop X,Y+WxH       Keep only this part of the screen
    \\      --flip-horizontal    Mirror left to right
    \\      --flip-vertical      Mirror top to bottom
    \\      --rotate-90          Turn a quarter clockwise
    \\      --rotate-180         Turn half round
    \\      --rotate-270         Turn a quarter the other way
    \\  -o, --output OUT         Where to write the GIF; the output when not given
    \\  -h, --help               Print this help
    \\
;

/// `X,Y+WxH`, digits only.
fn number(s: []const u8) ?usize {
    if (s.len == 0 or s.len > 9) return null;
    for (s) |c| {
        if (c < '0' or c > '9') return null;
    }
    return std.fmt.parseInt(usize, s, 10) catch null;
}

/// The flags and options, GNU's way: `--name value`, `--name=value`, `-I`, `-O2`, `-o OUT`; they may
/// come after the files, `--` makes every word after it a file, a lone `-` is the input.
const Spec = struct { name: []const u8, letter: u8, takes_value: bool };
const specs = [_]Spec{
    .{ .name = "info", .letter = 'I', .takes_value = false },
    .{ .name = "unoptimize", .letter = 0, .takes_value = false },
    .{ .name = "optimize", .letter = 'O', .takes_value = true },
    .{ .name = "crop", .letter = 0, .takes_value = true },
    .{ .name = "flip-horizontal", .letter = 0, .takes_value = false },
    .{ .name = "flip-vertical", .letter = 0, .takes_value = false },
    .{ .name = "rotate-90", .letter = 0, .takes_value = false },
    .{ .name = "rotate-180", .letter = 0, .takes_value = false },
    .{ .name = "rotate-270", .letter = 0, .takes_value = false },
    .{ .name = "output", .letter = 'o', .takes_value = true },
};

fn specIndex(name: []const u8) ?usize {
    for (specs, 0..) |s, k| {
        if (eql(u8, s.name, name)) return k;
    }
    return null;
}

const Arguments = struct {
    given: [specs.len]bool = @splat(false),
    // The last value of each option.
    values: [specs.len]?[]const u8 = @splat(null),
    files: std.ArrayList([]const u8) = .empty,

    fn flag(a: *const Arguments, name: []const u8) bool {
        return a.given[specIndex(name).?];
    }

    fn option(a: *const Arguments, name: []const u8) ?[]const u8 {
        return a.values[specIndex(name).?];
    }
};

fn printHelp(io: Io) noreturn {
    Io.File.stdout().writeStreamingAll(io, help_text) catch std.process.exit(1);
    std.process.exit(0);
}

fn parseArguments(io: Io, args: []const [:0]const u8) !Arguments {
    var a: Arguments = .{};
    var words_only = false;
    var i: usize = 1;
    while (i < args.len) {
        const word: []const u8 = args[i];
        i += 1;
        if (words_only or eql(u8, word, "-") or word.len == 0 or word[0] != '-') {
            try a.files.append(gpa, word);
            continue;
        }
        if (eql(u8, word, "--")) {
            words_only = true;
            continue;
        }
        if (std.mem.startsWith(u8, word, "--")) {
            const body = word[2..];
            const equals = std.mem.indexOfScalar(u8, body, '=');
            const name = if (equals) |e| body[0..e] else body;
            if (eql(u8, name, "help")) {
                if (equals == null) printHelp(io);
                usage("option '--help' takes no value", .{});
            }
            const k = specIndex(name) orelse usage("unknown option '--{s}'", .{name});
            if (!specs[k].takes_value) {
                if (equals != null) usage("option '--{s}' takes no value", .{name});
                a.given[k] = true;
            } else if (equals) |e| {
                a.values[k] = body[e + 1 ..];
            } else if (i < args.len) {
                a.values[k] = args[i];
                i += 1;
            } else {
                usage("option '--{s}' needs a value", .{name});
            }
            continue;
        }
        var at: usize = 1;
        while (at < word.len) {
            const code = word[at];
            if (!std.ascii.isAlphanumeric(code)) usage("unknown option '{s}'", .{word});
            if (code == 'h') printHelp(io);
            const k = for (specs, 0..) |s, n| {
                if (s.letter == code) break n;
            } else usage("unknown option '-{c}'", .{code});
            at += 1;
            if (!specs[k].takes_value) {
                a.given[k] = true;
                continue;
            }
            if (at < word.len) {
                a.values[k] = word[at..];
            } else if (i < args.len) {
                a.values[k] = args[i];
                i += 1;
            } else {
                usage("option '-{c}' needs a value", .{code});
            }
            break;
        }
    }
    return a;
}

fn info(io: Io, files: []const []const u8) !u8 {
    var failed = false;
    const names: []const []const u8 = if (files.len == 0) &.{"-"} else files;
    var buffer: [65536]u8 = undefined;
    var stdout_writer = Io.File.stdout().writerStreaming(io, &buffer);
    const stdout = &stdout_writer.interface;
    var text: Bytes = .empty;
    defer text.deinit(gpa);
    for (names) |name| {
        const shown = if (eql(u8, name, "-")) "<stdin>" else name;
        const data = readInput(io, name) catch |err| {
            std.debug.print("giftool: {s}: {s}\n", .{ shown, messageOf(err) });
            failed = true;
            continue;
        };
        defer gpa.free(data);
        var gif = readGif(data) catch |err| {
            std.debug.print("giftool: {s}: {s}\n", .{ shown, messageOf(err) });
            failed = true;
            continue;
        };
        defer gif.deinit();
        text.clearRetainingCapacity();
        try infoOf(&gif, shown, &text);
        stdout.writeAll(text.items) catch fail("<stdout>", messageOf(stdout_writer.err orelse error.WriteFailed));
    }
    stdout.flush() catch fail("<stdout>", messageOf(stdout_writer.err orelse error.WriteFailed));
    return if (failed) 1 else 0;
}

fn run(io: Io, args: []const [:0]const u8) !u8 {
    const a = try parseArguments(io, args);
    const files = a.files.items;
    if (a.flag("info")) return info(io, files);

    var o: Options = .{ .unoptimize = a.flag("unoptimize") };
    if (a.option("optimize")) |level| {
        if (eql(u8, level, "1")) {
            o.optimize = 1;
        } else if (eql(u8, level, "2")) {
            o.optimize = 2;
        } else {
            usage("-O takes 1 or 2, not '{s}'", .{level});
        }
    }
    if (a.option("crop")) |text| {
        const plus = std.mem.indexOfScalar(u8, text, '+');
        const comma = std.mem.indexOfScalar(u8, text, ',');
        const times = std.mem.indexOfScalar(u8, text, 'x');
        var ok = false;
        if (plus != null and comma != null and times != null) {
            const p = plus.?;
            const c = comma.?;
            const t = times.?;
            if (c < p and p < t) {
                const x = number(text[0..c]);
                const y = number(text[c + 1 .. p]);
                const w = number(text[p + 1 .. t]);
                const h = number(text[t + 1 ..]);
                if (x != null and y != null and w != null and h != null) {
                    o.crop = true;
                    o.crop_x = x.?;
                    o.crop_y = y.?;
                    o.crop_width = w.?;
                    o.crop_height = h.?;
                    ok = o.crop_width > 0 and o.crop_height > 0;
                }
            }
        }
        if (!ok) usage("--crop takes X,Y+WxH, not '{s}'", .{text});
    }
    if (a.flag("flip-horizontal") and a.flag("flip-vertical")) usage("one flip at most", .{});
    if (a.flag("flip-horizontal")) {
        o.flip = .horizontal;
    } else if (a.flag("flip-vertical")) {
        o.flip = .vertical;
    }
    var turns: usize = 0;
    if (a.flag("rotate-90")) {
        o.rotate = 90;
        turns += 1;
    }
    if (a.flag("rotate-180")) {
        o.rotate = 180;
        turns += 1;
    }
    if (a.flag("rotate-270")) {
        o.rotate = 270;
        turns += 1;
    }
    if (turns > 1) usage("one rotation at most", .{});
    if (files.len > 1) usage("one GIF at a time", .{});
    const name = if (files.len == 1) files[0] else "-";
    const shown = if (eql(u8, name, "-")) "<stdin>" else name;
    const data = readInput(io, name) catch |err| fail(shown, messageOf(err));
    var gif = readGif(data) catch |err| fail(shown, messageOf(err));
    const written = rewrite(data, &gif, o) catch |err| fail(shown, messageOf(err));
    const target = a.option("output") orelse "-";
    if (eql(u8, target, "-")) {
        Io.File.stdout().writeStreamingAll(io, written.items) catch |err| fail("<stdout>", messageOf(err));
    } else {
        Io.Dir.cwd().writeFile(io, .{ .sub_path = target, .data = written.items }) catch |err| fail(target, messageOf(err));
    }
    return 0;
}

pub fn main(init: std.process.Init) !u8 {
    // A closed pipe ends the program at once, killed by SIGPIPE as a C program is; Zig's `Io` would
    // otherwise have the write fail.
    std.posix.sigaction(.PIPE, &.{
        .handler = .{ .handler = std.posix.SIG.DFL },
        .mask = std.posix.sigemptyset(),
        .flags = 0,
    }, null);
    const args = try init.minimal.args.toSlice(init.arena.allocator());
    return run(init.io, args) catch |err| fail("giftool", messageOf(err));
}
