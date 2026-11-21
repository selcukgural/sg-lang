//! Builds `unicode` with ICU4C, a library of C: its header translated by Zig, the library linked.
//! `zig build -Doptimize=ReleaseFast` (or `ReleaseSafe`); `-Dicu=/prefix` where ICU4C is
//! installed, `/opt/homebrew/opt/icu4c` by default.
const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});
    const prefix = b.option([]const u8, "icu", "Where ICU4C is installed") orelse "/opt/homebrew/opt/icu4c";
    const include = b.pathJoin(&.{ prefix, "include" });
    const lib = b.pathJoin(&.{ prefix, "lib" });

    const icu = b.addTranslateC(.{
        .root_source_file = b.path("icu.h"),
        .target = target,
        .optimize = optimize,
    });
    icu.addIncludePath(.{ .cwd_relative = include });

    const module = b.createModule(.{
        .root_source_file = b.path("unicode.zig"),
        .target = target,
        .optimize = optimize,
        .link_libc = true,
        .imports = &.{.{ .name = "icu", .module = icu.createModule() }},
    });
    module.addLibraryPath(.{ .cwd_relative = lib });
    module.linkSystemLibrary("icuuc", .{});
    const exe = b.addExecutable(.{ .name = "unicode", .root_module = module });
    b.installArtifact(exe);
}
