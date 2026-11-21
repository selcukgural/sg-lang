//! `zip`: `std`'s zip archives measured against Zig's `std.zip` (choices 1553, 1629-1637,
//! 1643-1645). Three ways of using them:
//!
//!     zip write FILE OUT    FILE cut into parts of 10,000 bytes, `logs/part-00000.txt` and on,
//!                           each deflated at level 6 into the archive OUT
//!     zip read ARCHIVE      every entry of ARCHIVE opened into memory, its CRC-32 checked
//!     zip list ARCHIVE      the name and the size of every entry, as the archive lists them
//!
//! `std.zip` reads archives and does not write them, so `write` writes one here, with the headers
//! of `std.zip` and the DEFLATE of `std.compress.flate` (1644). `std.zip` does not check the CRC-32
//! of what it opens, so `read` does, as the others do (1645). What `write` and `read` print: how
//! many entries and their bytes. An archive that cannot be read is told on the standard error, with
//! exit code 1; a wrong use, with 2.

const std = @import("std");
const Io = std.Io;
const flate = std.compress.flate;
const zip = std.zip;
const Crc32 = std.hash.Crc32;

// 20 November 2026, 13:45:30, as DOS writes a time and a day.
const dos_time: u16 = (13 << 11) | (45 << 5) | (30 / 2);
const dos_date: u16 = ((2026 - 1980) << 9) | (11 << 5) | 20;
const no_flags: @FieldType(zip.LocalFileHeader, "flags") = @bitCast(@as(u16, 0));

const Written = struct { name_at: usize, name_len: u16, crc32: u32, compressed: u32, size: u32, offset: u32 };

fn fail(name: []const u8, err: anyerror) u8 {
    std.debug.print("zip: {s}: {s}\n", .{ name, @errorName(err) });
    return 1;
}

fn write(io: Io, gpa: std.mem.Allocator, path: []const u8, out_path: []const u8, out: *Io.Writer) !void {
    const data = try Io.Dir.cwd().readFileAlloc(io, path, gpa, .unlimited);
    defer gpa.free(data);
    const file = try Io.Dir.cwd().createFile(io, out_path, .{});
    defer file.close(io);
    var buffer: [65536]u8 = undefined;
    var file_writer = file.writer(io, &buffer);
    const w = &file_writer.interface;
    var window: [flate.max_window_len]u8 = undefined;
    var deflated: Io.Writer.Allocating = try .initCapacity(gpa, 16384);
    defer deflated.deinit();
    var names: std.ArrayList(u8) = .empty;
    defer names.deinit(gpa);
    var entries: std.ArrayList(Written) = .empty;
    defer entries.deinit(gpa);
    var offset: u32 = 0;
    var at: usize = 0;
    while (at < data.len) {
        const part = data[at..@min(at + 10000, data.len)];
        at += part.len;
        const name_at = names.items.len;
        try names.print(gpa, "logs/part-{d:0>5}.txt", .{entries.items.len});
        const name = names.items[name_at..];
        deflated.clearRetainingCapacity();
        var compressor = try flate.Compress.init(&deflated.writer, &window, .raw, .level_6);
        try compressor.writer.writeAll(part);
        try compressor.finish();
        const entry: Written = .{
            .name_at = name_at,
            .name_len = @intCast(name.len),
            .crc32 = Crc32.hash(part),
            .compressed = @intCast(deflated.written().len),
            .size = @intCast(part.len),
            .offset = offset,
        };
        try w.writeStruct(zip.LocalFileHeader{
            .signature = zip.local_file_header_sig,
            .version_needed_to_extract = 20,
            .flags = no_flags,
            .compression_method = .deflate,
            .last_modification_time = dos_time,
            .last_modification_date = dos_date,
            .crc32 = entry.crc32,
            .compressed_size = entry.compressed,
            .uncompressed_size = entry.size,
            .filename_len = entry.name_len,
            .extra_len = 0,
        }, .little);
        try w.writeAll(name);
        try w.writeAll(deflated.written());
        offset += @sizeOf(zip.LocalFileHeader) + entry.name_len + entry.compressed;
        try entries.append(gpa, entry);
    }
    const directory_at = offset;
    for (entries.items) |entry| {
        try w.writeStruct(zip.CentralDirectoryFileHeader{
            .signature = zip.central_file_header_sig,
            .version_made_by = 20,
            .version_needed_to_extract = 20,
            .flags = no_flags,
            .compression_method = .deflate,
            .last_modification_time = dos_time,
            .last_modification_date = dos_date,
            .crc32 = entry.crc32,
            .compressed_size = entry.compressed,
            .uncompressed_size = entry.size,
            .filename_len = entry.name_len,
            .extra_len = 0,
            .comment_len = 0,
            .disk_number = 0,
            .internal_file_attributes = 0,
            .external_file_attributes = 0,
            .local_file_header_offset = entry.offset,
        }, .little);
        try w.writeAll(names.items[entry.name_at..][0..entry.name_len]);
        offset += @sizeOf(zip.CentralDirectoryFileHeader) + entry.name_len;
    }
    try w.writeStruct(zip.EndRecord{
        .signature = zip.end_record_sig,
        .disk_number = 0,
        .central_directory_disk_number = 0,
        .record_count_disk = @intCast(entries.items.len),
        .record_count_total = @intCast(entries.items.len),
        .central_directory_size = offset - directory_at,
        .central_directory_offset = directory_at,
        .comment_len = 0,
    }, .little);
    try file_writer.end();
    try out.print("entries {d}\nbytes {d}\n", .{ entries.items.len, data.len });
}

fn read(io: Io, gpa: std.mem.Allocator, path: []const u8, out: *Io.Writer) !void {
    const file = try Io.Dir.cwd().openFile(io, path, .{});
    defer file.close(io);
    var buffer: [65536]u8 = undefined;
    var file_reader = file.reader(io, &buffer);
    var it = try zip.Iterator.init(&file_reader);
    var data: Io.Writer.Allocating = .init(gpa);
    defer data.deinit();
    var entries: u64 = 0;
    var bytes: u64 = 0;
    while (try it.next()) |entry| {
        data.clearRetainingCapacity();
        try entry.extractTo(&file_reader, &data.writer);
        if (Crc32.hash(data.written()) != entry.crc32) return error.ZipBadCrc32;
        entries += 1;
        bytes += data.written().len;
    }
    try out.print("entries {d}\nbytes {d}\n", .{ entries, bytes });
}

fn list(io: Io, path: []const u8, out: *Io.Writer) !void {
    const file = try Io.Dir.cwd().openFile(io, path, .{});
    defer file.close(io);
    var buffer: [65536]u8 = undefined;
    var file_reader = file.reader(io, &buffer);
    var it = try zip.Iterator.init(&file_reader);
    var name_buffer: [std.math.maxInt(u16)]u8 = undefined;
    while (try it.next()) |entry| {
        const name = try entry.getFilename(&file_reader, &name_buffer, .{});
        try out.print("{s} {d}\n", .{ name, entry.uncompressed_size });
    }
}

pub fn main(init: std.process.Init) !u8 {
    const io = init.io;
    const arena = init.arena.allocator();
    const args = try init.minimal.args.toSlice(arena);
    const usage = (args.len == 4 and std.mem.eql(u8, args[1], "write")) or
        (args.len == 3 and (std.mem.eql(u8, args[1], "read") or std.mem.eql(u8, args[1], "list")));
    if (!usage) {
        std.debug.print("usage: zip write FILE OUT | read ARCHIVE | list ARCHIVE\n", .{});
        return 2;
    }
    const mode = args[1];
    const name = args[2];
    var out_buffer: [65536]u8 = undefined;
    var out_file: Io.File.Writer = .init(.stdout(), io, &out_buffer);
    const out = &out_file.interface;
    if (std.mem.eql(u8, mode, "write")) {
        write(io, init.gpa, name, args[3], out) catch |err| return fail(name, err);
    } else if (std.mem.eql(u8, mode, "read")) {
        read(io, init.gpa, name, out) catch |err| return fail(name, err);
    } else {
        list(io, name, out) catch |err| return fail(name, err);
    }
    try out.flush();
    return 0;
}
