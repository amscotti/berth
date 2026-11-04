const std = @import("std");

const zon = @import("build.zig.zon");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});
    const nanoui = b.dependency("nanoui", .{ .target = target, .optimize = optimize }).module("nanoui");

    // The zon version is the one source of truth: the app, the settings
    // subtitle and the macOS bundle all read this value at build time.
    const build_options = b.addOptions();
    build_options.addOption([]const u8, "version", zon.version);
    const version_module = build_options.createModule();

    const exe = b.addExecutable(.{
        .name = "berth",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/main.zig"),
            .target = target,
            .optimize = optimize,
            .imports = &.{
                .{ .name = "nanoui", .module = nanoui },
                .{ .name = "build_options", .module = version_module },
            },
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
            .imports = &.{
                .{ .name = "nanoui", .module = nanoui },
                .{ .name = "build_options", .module = version_module },
            },
        }),
    });
    b.step("test", "Run the tests").dependOn(&b.addRunArtifact(tests).step);

    // Every file at most 600 lines, as the framework this is built on requires.
    const check = b.addSystemCommand(&.{ "python3", "tools/lines.py", "src" });
    b.step("lines", "Check that every file is at most 600 lines").dependOn(&check.step);

    // A release binary and a tarball to hand someone. A separate build unit
    // from the default one, so `zig build` stays a debug build. nanoui is a
    // second dependency instance here: the shared module above carries the
    // caller's optimize mode, which would otherwise leave the "release" tarball
    // with debug framework code.
    const dist_step = b.step("dist", "Build a ReleaseFast binary and pack it as a tarball");
    const dist_nanoui = b.dependency("nanoui", .{ .target = target, .optimize = .ReleaseFast }).module("nanoui");
    const dist_exe = b.addExecutable(.{
        .name = "berth",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/main.zig"),
            .target = target,
            .optimize = .ReleaseFast,
            .strip = true,
            .imports = &.{
                .{ .name = "nanoui", .module = dist_nanoui },
                .{ .name = "build_options", .module = version_module },
            },
        }),
    });
    if (target.result.os.tag == .windows) dist_exe.subsystem = .windows;

    const dist_install = b.addInstallArtifact(dist_exe, .{ .dest_sub_path = "berth" });
    dist_step.dependOn(&dist_install.step);

    // Take the compiler's own output, not a path that might still hold a stale
    // debug build, and name the archive from the target we actually built for
    // rather than the host running the build.
    const archive_name = b.fmt("berth-{s}-{s}.tar.gz", .{
        @tagName(target.result.os.tag),
        @tagName(target.result.cpu.arch),
    });
    const pack = b.addSystemCommand(&.{
        "sh", "-c",
        \\set -eu
        \\out=$1 bin=$2 license=$3 readme=$4 desktop=$5
        \\stage=$(mktemp -d "${TMPDIR:-/tmp}/berth-dist.XXXXXX")
        \\tmp_tar=$(mktemp "${TMPDIR:-/tmp}/berth-dist-tar.XXXXXX")
        \\trap 'rm -rf "$stage" "$tmp_tar"' EXIT
        \\mkdir -p "$stage/bin" "$stage/share/applications"
        \\cp "$bin" "$stage/bin/berth"
        \\chmod +x "$stage/bin/berth"
        \\cp "$license" "$stage/LICENSE"
        \\cp "$readme" "$stage/README.md"
        \\cp "$desktop" "$stage/share/applications/berth.desktop"
        \\# Fixed mtimes and explicit order keep repeated packs comparable.
        \\touch -t 200001010000 "$stage/bin/berth" "$stage/LICENSE" "$stage/README.md" "$stage/share/applications/berth.desktop"
        \\xattr_flag=
        \\if tar --no-xattrs -cf /dev/null -C "$stage" LICENSE 2>/dev/null; then xattr_flag=--no-xattrs; fi
        \\# COPYFILE_DISABLE stops macOS from adding AppleDouble entries; gzip -n
        \\# keeps its header timestamp out of the archive.
        \\COPYFILE_DISABLE=1 tar $xattr_flag -cf "$tmp_tar" -C "$stage" bin/berth LICENSE README.md share/applications/berth.desktop
        \\gzip -n -c "$tmp_tar" > "$out"
        ,
        "sh",
    });
    const archive = pack.addOutputFileArg(archive_name);
    pack.addFileArg(dist_exe.getEmittedBin());
    pack.addFileArg(b.path("LICENSE"));
    pack.addFileArg(b.path("README.md"));
    pack.addFileArg(b.path("packaging/berth.desktop"));
    const install_archive = b.addInstallFileWithDir(archive, .{ .custom = "dist" }, archive_name);
    dist_step.dependOn(&install_archive.step);
}
