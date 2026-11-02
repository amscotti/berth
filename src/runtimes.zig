//! Runtime records outlive a refresh and are rebuilt only on discovery: the
//! strings belong to the state, not a snapshot arena, so they are freed here.
const std = @import("std");
const app = @import("app.zig");
const api = @import("engine/api.zig");

pub fn drop(s: *app.State) void {
    if (s.runtimes.len == 0) return;
    for (s.runtimes) |r| {
        s.gpa.free(r.name);
        s.gpa.free(r.socket);
        if (r.error_text.len > 0) s.gpa.free(r.error_text);
        freeVersion(s.gpa, r.version);
        freeInfo(s.gpa, r.info);
    }
    s.gpa.free(s.runtimes);
    s.runtimes = &.{};
}

fn freeVersion(gpa: std.mem.Allocator, v: api.Version) void {
    gpa.free(v.Version);
    gpa.free(v.ApiVersion);
    gpa.free(v.MinAPIVersion);
    gpa.free(v.Os);
    gpa.free(v.Arch);
    gpa.free(v.GoVersion);
}

fn freeInfo(gpa: std.mem.Allocator, i: api.Info) void {
    gpa.free(i.ID);
    gpa.free(i.Name);
    gpa.free(i.ServerVersion);
    gpa.free(i.OperatingSystem);
    gpa.free(i.OSType);
    gpa.free(i.Architecture);
    gpa.free(i.DockerRootDir);
    gpa.free(i.Driver);
    gpa.free(i.LoggingDriver);
    gpa.free(i.CgroupVersion);
}
