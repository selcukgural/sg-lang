//! Builds `differ` with PCRE2, a library of C: its header translated by Zig, the library linked.
//! `zig build -Doptimize=ReleaseFast` (or `ReleaseSafe`); `-Dpcre2=/prefix` where PCRE2 is
//! installed, `/opt/homebrew/opt/pcre2` by default.
const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});
    const prefix = b.option([]const u8, "pcre2", "Where PCRE2 is installed") orelse "/opt/homebrew/opt/pcre2";
    const include = b.pathJoin(&.{ prefix, "include" });
    const lib = b.pathJoin(&.{ prefix, "lib" });

    const pcre2 = b.addTranslateC(.{
        .root_source_file = b.path("pcre2.h"),
        .target = target,
        .optimize = optimize,
    });
    pcre2.addIncludePath(.{ .cwd_relative = include });

    const module = b.createModule(.{
        .root_source_file = b.path("differ.zig"),
        .target = target,
        .optimize = optimize,
        .link_libc = true,
        .imports = &.{.{ .name = "pcre2", .module = pcre2.createModule() }},
    });
    module.addLibraryPath(.{ .cwd_relative = lib });
    module.linkSystemLibrary("pcre2-8", .{});
    const exe = b.addExecutable(.{ .name = "differ", .root_module = module });
    b.installArtifact(exe);
}
