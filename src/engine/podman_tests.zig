//! Podman machine config tests kept out of `discovery.zig` for the line rule.
const std = @import("std");
const discovery = @import("discovery.zig");

test "podman 4.5 configs choose the named machine by LastUp" {
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    var path_buf: [std.Io.Dir.max_path_bytes]u8 = undefined;
    const n = try tmp.dir.realPath(std.testing.io, &path_buf);
    const base = path_buf[0..n];
    const io = std.testing.io;
    const gpa = std.testing.allocator;

    const home = try std.fmt.allocPrint(gpa, "{s}/home", .{base});
    defer gpa.free(home);
    try tmp.dir.createDirPath(io, "home/.config/containers/podman/machine/applehv");
    try tmp.dir.writeFile(io, .{
        .sub_path = "home/.config/containers/podman/machine/applehv/podman-machine-default.json",
        .data = "{\"Name\":\"podman-machine-default\",\"LastUp\":\"2024-01-01T00:00:00Z\"}",
    });
    try tmp.dir.writeFile(io, .{
        .sub_path = "home/.config/containers/podman/machine/applehv/work.json",
        .data = "{\"Name\":\"work\",\"LastUp\":\"2025-01-01T00:00:00Z\"}",
    });

    var out: [96]u8 = undefined;
    const socket = "/Users/x/.local/share/containers/podman/machine/applehv/podman.sock";
    try std.testing.expectEqualStrings("work", discovery.podman4MachineNameIn(io, home, "", socket, &out).?);

    // The older config must win when it is the one that was last up.
    try tmp.dir.writeFile(io, .{
        .sub_path = "home/.config/containers/podman/machine/applehv/podman-machine-default.json",
        .data = "{\"Name\":\"podman-machine-default\",\"LastUp\":\"2025-02-01T00:00:00Z\"}",
    });
    try std.testing.expectEqualStrings("podman-machine-default", discovery.podman4MachineNameIn(io, home, "", socket, &out).?);
}

test "podman 4.0-4.4 socket names the machine, config lives under a vm type" {
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    var path_buf: [std.Io.Dir.max_path_bytes]u8 = undefined;
    const n = try tmp.dir.realPath(std.testing.io, &path_buf);
    const base = path_buf[0..n];
    const io = std.testing.io;
    const gpa = std.testing.allocator;

    const home = try std.fmt.allocPrint(gpa, "{s}/home", .{base});
    defer gpa.free(home);
    try tmp.dir.createDirPath(io, "home/.config/containers/podman/machine/applehv");
    try tmp.dir.writeFile(io, .{
        .sub_path = "home/.config/containers/podman/machine/applehv/work.json",
        .data = "{\"Name\":\"work\"}",
    });

    var out: [96]u8 = undefined;
    const socket = "/Users/x/.local/share/containers/podman/machine/work/podman.sock";
    try std.testing.expectEqualStrings("work", discovery.podman4MachineNameIn(io, home, "", socket, &out).?);

    const missing = "/Users/x/.local/share/containers/podman/machine/other/podman.sock";
    try std.testing.expect(discovery.podman4MachineNameIn(io, home, "", missing, &out) == null);
}

extern "c" fn getenv(name: [*:0]const u8) ?[*:0]u8;
extern "c" fn setenv(name: [*:0]const u8, value: [*:0]const u8, overwrite: c_int) c_int;
extern "c" fn unsetenv(name: [*:0]const u8) c_int;

fn snapshotEnv(gpa: std.mem.Allocator, name: [*:0]const u8) ?[]u8 {
    const value = getenv(name) orelse return null;
    return gpa.dupe(u8, std.mem.span(value)) catch null;
}

fn replaceEnv(gpa: std.mem.Allocator, name: [*:0]const u8, value: []const u8) !void {
    const z = try gpa.dupeSentinel(u8, value, 0);
    defer gpa.free(z);
    _ = setenv(name, z.ptr, 1);
}

fn restoreEnv(gpa: std.mem.Allocator, name: [*:0]const u8, old: ?[]u8) void {
    if (old) |value| {
        defer gpa.free(value);
        const z = gpa.dupeSentinel(u8, value, 0) catch {
            _ = unsetenv(name);
            return;
        };
        defer gpa.free(z);
        _ = setenv(name, z.ptr, 1);
    } else {
        _ = unsetenv(name);
    }
}

test "a DOCKER_HOST alias collapses to the same socket" {
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    var path_buf: [std.Io.Dir.max_path_bytes]u8 = undefined;
    const n = try tmp.dir.realPath(std.testing.io, &path_buf);
    const base = path_buf[0..n];
    const io = std.testing.io;
    const gpa = std.testing.allocator;

    try tmp.dir.createDirPath(io, "real");
    try tmp.dir.createDirPath(io, "home");
    try tmp.dir.createDirPath(io, "tmp");
    try tmp.dir.createDirPath(io, "xdg");
    try tmp.dir.writeFile(io, .{ .sub_path = "real/docker.sock", .data = "" });
    const socket = try std.fmt.allocPrint(gpa, "{s}/real/docker.sock", .{base});
    defer gpa.free(socket);
    const alias = try std.fmt.allocPrint(gpa, "{s}/alias.sock", .{base});
    defer gpa.free(alias);
    try tmp.dir.symLink(io, socket, "alias.sock", .{});

    const old_docker = snapshotEnv(gpa, "DOCKER_HOST");
    defer restoreEnv(gpa, "DOCKER_HOST", old_docker);
    const old_home = snapshotEnv(gpa, "HOME");
    defer restoreEnv(gpa, "HOME", old_home);
    const old_tmp = snapshotEnv(gpa, "TMPDIR");
    defer restoreEnv(gpa, "TMPDIR", old_tmp);
    const old_xdg = snapshotEnv(gpa, "XDG_RUNTIME_DIR");
    defer restoreEnv(gpa, "XDG_RUNTIME_DIR", old_xdg);

    const host = try std.fmt.allocPrint(gpa, "unix://{s}", .{alias});
    defer gpa.free(host);
    const home = try std.fmt.allocPrint(gpa, "{s}/home", .{base});
    defer gpa.free(home);
    const tmpdir = try std.fmt.allocPrint(gpa, "{s}/tmp", .{base});
    defer gpa.free(tmpdir);
    const xdg = try std.fmt.allocPrint(gpa, "{s}/xdg", .{base});
    defer gpa.free(xdg);
    try replaceEnv(gpa, "DOCKER_HOST", host);
    try replaceEnv(gpa, "HOME", home);
    try replaceEnv(gpa, "TMPDIR", tmpdir);
    try replaceEnv(gpa, "XDG_RUNTIME_DIR", xdg);

    const found = try discovery.candidates(io, gpa, "");
    defer {
        for (found) |path| gpa.free(path);
        gpa.free(found);
    }
    var hits: usize = 0;
    var aliases: usize = 0;
    for (found) |path| {
        if (std.mem.eql(u8, path, socket)) hits += 1;
        if (std.mem.eql(u8, path, alias)) aliases += 1;
    }
    try std.testing.expectEqual(@as(usize, 1), hits);
    try std.testing.expectEqual(@as(usize, 0), aliases);
}

test "podman honors XDG config and data roots" {
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    var path_buf: [std.Io.Dir.max_path_bytes]u8 = undefined;
    const n = try tmp.dir.realPath(std.testing.io, &path_buf);
    const base = path_buf[0..n];
    const io = std.testing.io;
    const gpa = std.testing.allocator;

    try tmp.dir.createDirPath(io, "config/containers/podman/machine/applehv");
    try tmp.dir.createDirPath(io, "data/containers/podman/machine/applehv");
    try tmp.dir.createDirPath(io, "home");
    try tmp.dir.createDirPath(io, "tmp");
    try tmp.dir.createDirPath(io, "runtime");
    try tmp.dir.writeFile(io, .{
        .sub_path = "config/containers/podman/machine/applehv/work.json",
        .data = "{\"Name\":\"work\",\"LastUp\":\"2025-01-01T00:00:00Z\"}",
    });
    try tmp.dir.writeFile(io, .{ .sub_path = "data/containers/podman/machine/applehv/podman.sock", .data = "" });

    const old_home = snapshotEnv(gpa, "HOME");
    defer restoreEnv(gpa, "HOME", old_home);
    const old_config = snapshotEnv(gpa, "XDG_CONFIG_HOME");
    defer restoreEnv(gpa, "XDG_CONFIG_HOME", old_config);
    const old_data = snapshotEnv(gpa, "XDG_DATA_HOME");
    defer restoreEnv(gpa, "XDG_DATA_HOME", old_data);
    const old_tmp = snapshotEnv(gpa, "TMPDIR");
    defer restoreEnv(gpa, "TMPDIR", old_tmp);
    const old_runtime = snapshotEnv(gpa, "XDG_RUNTIME_DIR");
    defer restoreEnv(gpa, "XDG_RUNTIME_DIR", old_runtime);

    const home = try std.fmt.allocPrint(gpa, "{s}/home", .{base});
    defer gpa.free(home);
    const config = try std.fmt.allocPrint(gpa, "{s}/config", .{base});
    defer gpa.free(config);
    const data = try std.fmt.allocPrint(gpa, "{s}/data", .{base});
    defer gpa.free(data);
    const tmpdir = try std.fmt.allocPrint(gpa, "{s}/tmp", .{base});
    defer gpa.free(tmpdir);
    const runtime = try std.fmt.allocPrint(gpa, "{s}/runtime", .{base});
    defer gpa.free(runtime);
    try replaceEnv(gpa, "HOME", home);
    try replaceEnv(gpa, "XDG_CONFIG_HOME", config);
    try replaceEnv(gpa, "XDG_DATA_HOME", data);
    try replaceEnv(gpa, "TMPDIR", tmpdir);
    try replaceEnv(gpa, "XDG_RUNTIME_DIR", runtime);

    const socket = try std.fmt.allocPrint(gpa, "{s}/containers/podman/machine/applehv/podman.sock", .{data});
    defer gpa.free(socket);
    const found = try discovery.candidates(io, gpa, "");
    defer {
        for (found) |path| gpa.free(path);
        gpa.free(found);
    }
    var hits: usize = 0;
    for (found) |path| {
        if (std.mem.eql(u8, path, socket)) hits += 1;
    }
    try std.testing.expectEqual(@as(usize, 1), hits);

    var out: [96]u8 = undefined;
    const machine_socket = "/Users/x/.local/share/containers/podman/machine/applehv/podman.sock";
    try std.testing.expectEqualStrings("work", discovery.podman4MachineName(io, machine_socket, &out).?);
}

test "a two-hop alias collapses to one named runtime socket" {
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    var path_buf: [std.Io.Dir.max_path_bytes]u8 = undefined;
    const n = try tmp.dir.realPath(std.testing.io, &path_buf);
    const base = path_buf[0..n];
    const io = std.testing.io;
    const gpa = std.testing.allocator;

    try tmp.dir.createDirPath(io, "runtime/podman");
    try tmp.dir.createDirPath(io, "home");
    try tmp.dir.writeFile(io, .{ .sub_path = "runtime/podman/work-api.sock", .data = "" });
    const target = try std.fmt.allocPrint(gpa, "{s}/runtime/podman/work-api.sock", .{base});
    defer gpa.free(target);
    const hop1 = try std.fmt.allocPrint(gpa, "{s}/hop1.sock", .{base});
    defer gpa.free(hop1);
    const hop2 = try std.fmt.allocPrint(gpa, "{s}/hop2.sock", .{base});
    defer gpa.free(hop2);
    try tmp.dir.symLink(io, target, "hop1.sock", .{});
    try tmp.dir.symLink(io, hop1, "hop2.sock", .{});

    const old_home = snapshotEnv(gpa, "HOME");
    defer restoreEnv(gpa, "HOME", old_home);
    const old_docker = snapshotEnv(gpa, "DOCKER_HOST");
    defer restoreEnv(gpa, "DOCKER_HOST", old_docker);
    const old_tmp = snapshotEnv(gpa, "TMPDIR");
    defer restoreEnv(gpa, "TMPDIR", old_tmp);
    const old_runtime = snapshotEnv(gpa, "XDG_RUNTIME_DIR");
    defer restoreEnv(gpa, "XDG_RUNTIME_DIR", old_runtime);

    const home = try std.fmt.allocPrint(gpa, "{s}/home", .{base});
    defer gpa.free(home);
    const runtime = try std.fmt.allocPrint(gpa, "{s}/runtime", .{base});
    defer gpa.free(runtime);
    const host = try std.fmt.allocPrint(gpa, "unix://{s}", .{hop2});
    defer gpa.free(host);
    try replaceEnv(gpa, "HOME", home);
    try replaceEnv(gpa, "DOCKER_HOST", host);
    try replaceEnv(gpa, "TMPDIR", runtime);
    try replaceEnv(gpa, "XDG_RUNTIME_DIR", runtime);

    const found = try discovery.candidates(io, gpa, "");
    defer {
        for (found) |path| gpa.free(path);
        gpa.free(found);
    }
    var hits: usize = 0;
    var aliases: usize = 0;
    for (found) |path| {
        if (std.mem.eql(u8, path, target)) hits += 1;
        if (std.mem.eql(u8, path, hop1) or std.mem.eql(u8, path, hop2)) aliases += 1;
    }
    try std.testing.expectEqual(@as(usize, 1), hits);
    try std.testing.expectEqual(@as(usize, 0), aliases);
    try std.testing.expectEqualStrings("work", discovery.podmanMachineName(target).?);
}

test "a typed symlink matches its canonical runtime socket" {
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    var path_buf: [std.Io.Dir.max_path_bytes]u8 = undefined;
    const n = try tmp.dir.realPath(std.testing.io, &path_buf);
    const base = path_buf[0..n];
    const io = std.testing.io;
    const gpa = std.testing.allocator;

    try tmp.dir.createDirPath(io, "runtime");
    try tmp.dir.writeFile(io, .{ .sub_path = "runtime/work-api.sock", .data = "" });
    const target = try std.fmt.allocPrint(gpa, "{s}/runtime/work-api.sock", .{base});
    defer gpa.free(target);
    const typed = try std.fmt.allocPrint(gpa, "{s}/typed.sock", .{base});
    defer gpa.free(typed);
    try tmp.dir.symLink(io, target, "typed.sock", .{});
    try std.testing.expect(discovery.sameSocket(io, gpa, typed, target));
}

test "DOCKER_HOST relative and empty paths do not panic" {
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    var path_buf: [std.Io.Dir.max_path_bytes]u8 = undefined;
    const n = try tmp.dir.realPath(std.testing.io, &path_buf);
    const base = path_buf[0..n];
    const io = std.testing.io;
    const gpa = std.testing.allocator;

    try tmp.dir.createDirPath(io, "home");
    try tmp.dir.createDirPath(io, "tmp");
    try tmp.dir.createDirPath(io, "runtime");

    const old_home = snapshotEnv(gpa, "HOME");
    defer restoreEnv(gpa, "HOME", old_home);
    const old_docker = snapshotEnv(gpa, "DOCKER_HOST");
    defer restoreEnv(gpa, "DOCKER_HOST", old_docker);
    const old_tmp = snapshotEnv(gpa, "TMPDIR");
    defer restoreEnv(gpa, "TMPDIR", old_tmp);
    const old_runtime = snapshotEnv(gpa, "XDG_RUNTIME_DIR");
    defer restoreEnv(gpa, "XDG_RUNTIME_DIR", old_runtime);

    const home = try std.fmt.allocPrint(gpa, "{s}/home", .{base});
    defer gpa.free(home);
    const tmpdir = try std.fmt.allocPrint(gpa, "{s}/tmp", .{base});
    defer gpa.free(tmpdir);
    const runtime = try std.fmt.allocPrint(gpa, "{s}/runtime", .{base});
    defer gpa.free(runtime);
    try replaceEnv(gpa, "HOME", home);
    try replaceEnv(gpa, "TMPDIR", tmpdir);
    try replaceEnv(gpa, "XDG_RUNTIME_DIR", runtime);

    try replaceEnv(gpa, "DOCKER_HOST", "unix://relative.sock");
    const cwd = try std.process.currentPathAlloc(io, gpa);
    defer gpa.free(cwd);
    const expected = try std.fmt.allocPrint(gpa, "{s}/relative.sock", .{cwd});
    defer gpa.free(expected);
    const found = try discovery.candidates(io, gpa, "");
    defer {
        for (found) |path| gpa.free(path);
        gpa.free(found);
    }
    var hits: usize = 0;
    for (found) |path| {
        if (std.mem.eql(u8, path, expected)) hits += 1;
    }
    try std.testing.expectEqual(@as(usize, 1), hits);

    try replaceEnv(gpa, "DOCKER_HOST", "unix://");
    const empty = try discovery.candidates(io, gpa, "");
    defer {
        for (empty) |path| gpa.free(path);
        gpa.free(empty);
    }
    for (empty) |path| try std.testing.expect(path.len > 0);
}
