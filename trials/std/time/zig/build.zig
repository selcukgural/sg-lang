//! Builds `time` with the C library, whose `localtime_r` sees a moment in the time zone of the
//! system: Zig's standard library has no time zones. `zig build -Doptimize=ReleaseFast` (or
//! `ReleaseSafe`).
const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    const c = b.addTranslateC(.{
        .root_source_file = b.path("c.h"),
        .target = target,
        .optimize = optimize,
        .link_libc = true,
    });

    const module = b.createModule(.{
        .root_source_file = b.path("time.zig"),
        .target = target,
        .optimize = optimize,
        .link_libc = true,
        .imports = &.{.{ .name = "c", .module = c.createModule() }},
    });
    const exe = b.addExecutable(.{ .name = "time", .root_module = module });
    b.installArtifact(exe);
}
