const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});
    const nanoui = b.dependency("nanoui", .{ .target = target, .optimize = optimize }).module("nanoui");

    const exe = b.addExecutable(.{
        .name = "berth",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/main.zig"),
            .target = target,
            .optimize = optimize,
            .imports = &.{.{ .name = "nanoui", .module = nanoui }},
        }),
    });
    if (target.result.os.tag == .windows) exe.subsystem = .windows;
    b.installArtifact(exe);

    const run = b.addRunArtifact(exe);
    run.addPassthruArgs();
    b.step("run", "Run berth (opens its window)").dependOn(&run.step);

    // Tests: engine protocol, fixtures and views, rendered offscreen.
    const tests = b.addTest(.{
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/tests.zig"),
            .target = target,
            .optimize = optimize,
            .imports = &.{.{ .name = "nanoui", .module = nanoui }},
        }),
    });
    b.step("test", "Run the tests").dependOn(&b.addRunArtifact(tests).step);

    // Every file at most 600 lines, as the framework this is built on requires.
    const check = b.addSystemCommand(&.{ "python3", "tools/lines.py", "src" });
    b.step("lines", "Check that every file is at most 600 lines").dependOn(&check.step);

    // A release binary and a tarball to hand someone. A separate build unit
    // from the default one, so `zig build` stays a debug build.
    const dist_step = b.step("dist", "Build a ReleaseFast binary and pack it as a tarball");
    const dist_exe = b.addExecutable(.{
        .name = "berth",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/main.zig"),
            .target = target,
            .optimize = .ReleaseFast,
            .strip = true,
            .imports = &.{.{ .name = "nanoui", .module = nanoui }},
        }),
    });
    if (target.result.os.tag == .windows) dist_exe.subsystem = .windows;

    const dist_install = b.addInstallArtifact(dist_exe, .{ .dest_sub_path = "berth" });
    dist_step.dependOn(&dist_install.step);

    // Named for the platform it was built on, because a release binary is not
    // portable across operating systems even when the target is.
    const pack = b.addSystemCommand(&.{
        "sh", "-c",
        \\mkdir -p zig-out/dist && tar -czf "zig-out/dist/berth-$(uname -s | tr 'A-Z' 'a-z')-$(uname -m).tar.gz" -C zig-out/bin berth && ls -l zig-out/dist
    });
    pack.step.dependOn(&dist_install.step);
    dist_step.dependOn(&pack.step);
}
