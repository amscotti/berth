//! Stacks: containers grouped by their Compose project labels. Nothing is
//! parsed from YAML — Compose puts the project, service and config paths on
//! every container it creates, so grouping is a label lookup.
const std = @import("std");
const nanoui = @import("nanoui");
const ui = nanoui.ui;
const app = @import("../../app.zig");
const api = @import("../../engine/api.zig");
const bulk = @import("../../bulk.zig");
const projects_mod = @import("../../projects.zig");
const widgets = @import("../widgets.zig");
const theme = @import("../theme.zig");

pub fn view(s: *app.State, c: *ui.Context) void {
    // Reading lasts until the first snapshot commits; an empty stack list
    // would claim the engine has no compose projects before it has been read.
    const first = widgets.firstRead(s.last_update, s.error_text);
    if (first == .reading) {
        widgets.pageHeader(c, "Stacks", "Reading engine…");
        widgets.loadingRows(c, 4);
        return;
    }
    if (first == .failed) {
        widgets.pageHeader(c, "Stacks", "engine not answering");
        widgets.errorBanner(c, s.error_text);
        widgets.emptyState(c, "code", "Engine not answering", widgets.engine_help);
        return;
    }

    const projects = s.projects.len;
    const subtitle = if (s.error_text.len > 0 and projects == 0)
        "engine not answering"
    else
        widgets.count(c, projects, "project", "projects");
    widgets.pageHeader(c, "Stacks", subtitle);
    if (s.error_text.len > 0) widgets.errorBanner(c, s.error_text);

    // The bulk worker owns start/stop across projects; while it runs, every
    // card's actions are shown disabled rather than pretending a second batch
    // can join it. A single-container mutation or an open confirmation blocks
    // the same buttons for the same reason.
    const busy = s.bulk.running();
    const actions_enabled = !s.mutating() and s.bulk.pending == null;
    if (busy) {
        const line = ui.div(c).row().wFull().itemsCenter().gap(6).pb(8);
        ui.icon(c, "hourglass", .{ .size = 11, .color = theme.warning(c.theme) });
        ui.text(c, if (s.bulk.canceling) "Canceling…" else c.print("{s} containers…", .{bulk.runningLabel(&s.bulk)}), .{
            .size = 11.5,
            .weight = .medium,
            .color = theme.warning(c.theme),
        });
        line.end();
    }

    if (projects == 0) {
        if (s.error_text.len > 0) {
            widgets.emptyState(c, "code", "Engine not answering", widgets.engine_help);
        } else {
            widgets.emptyState(c, "code", "No Compose stacks", "Containers started by docker compose appear here, grouped by project");
        }
        return;
    }

    const list = ui.div(c).column().wFull().gap(10);
    defer list.end();

    for (s.projects) |summary| {
        projectCard(s, c, summary, actions_enabled);
    }
}

fn stateLabel(ct: api.Container) []const u8 {
    return ct.health() orelse ct.State;
}

fn projectCard(s: *app.State, c: *ui.Context, summary: projects_mod.Summary, enabled: bool) void {
    const card = ui.div(c).column().wFull().gap(6).p(14).rounded(10).bg(c.theme.surface).border(c.theme.border);
    defer card.end();

    const head = ui.div(c).row().wFull().itemsCenter().gap(10);
    ui.icon(c, "code", .{ .size = 16, .color = c.theme.accent });
    ui.text(c, summary.name, .{ .size = 14, .weight = .semibold, .truncate = .ellipsis });
    ui.text(c, c.print("{d}/{d} running", .{ summary.running, summary.total }), .{ .size = 12, .color = c.theme.muted });
    ui.spacer(c);

    if (s.layout.content >= 760) {
        if (summary.first < s.containers.len) {
            const first = s.containers[summary.first];
            if (first.label("com.docker.compose.project.working_dir")) |dir| {
                ui.text(c, dir, .{ .size = 11, .color = c.theme.muted, .truncate = .middle });
            }
        }
    }
    // The one action a Compose project needs from a monitor: start everything
    // that is down, or stop everything that is up, through the bulk worker.
    // Start all is the accent primary, like the creation card's Pull and Run.
    if (summary.running < summary.total and projectButton(c, "Start all", "play.fill", true, enabled)) bulk.startProject(s, summary.name);
    if (summary.running > 0 and projectButton(c, "Stop all", "stop.fill", false, enabled)) bulk.stopProject(s, summary.name);
    head.end();

    for (summary.members) |index| {
        if (index >= s.containers.len) continue;
        const ct = s.containers[index];
        const is_selected = s.selected != null and std.mem.eql(u8, s.selected.?, ct.Id);
        var row = ui.div(c).row().wFull().h(28).itemsCenter().gap(8).px(4).rounded(4)
            .bg(if (is_selected) c.theme.accent.alpha(0.10) else c.theme.surface)
            .control();
        if (!is_selected) row = row.hover(c.theme.surface_hover);
        if (row.clicked()) s.select(&ct);
        const svc = ct.service() orelse ct.name();
        ui.text(c, svc, .{ .size = 12, .weight = .medium, .truncate = .ellipsis });
        ui.text(c, ct.name(), .{ .size = 11, .color = c.theme.muted, .truncate = .ellipsis });
        ui.spacer(c);
        // A service whose healthcheck is failing is not "running" in the sense
        // the row means; the health verdict is the status.
        widgets.pill(c, stateLabel(ct));
        row.end();
    }
}

fn projectButton(c: *ui.Context, label: []const u8, icon_name: []const u8, primary: bool, enabled: bool) bool {
    const bg = if (!enabled)
        c.theme.surface_hover.alpha(0.6)
    else if (primary)
        c.theme.accent
    else
        c.theme.surface_hover;
    const fg = if (!enabled)
        c.theme.muted
    else if (primary)
        c.theme.accent_text
    else
        c.theme.text;
    const box = ui.div(c).row().h(24).px(10).itemsCenter().gap(5).rounded(12).bg(bg).control();
    ui.icon(c, icon_name, .{ .size = 9, .color = fg });
    ui.text(c, label, .{ .size = 11.5, .color = fg });
    const clicked = if (enabled) box.clicked() else false;
    box.end();
    return clicked;
}

test "a service row shows its health verdict instead of green running" {
    const unhealthy: api.Container = .{ .State = "running", .Status = "Up 2 minutes (unhealthy)" };
    const plain: api.Container = .{ .State = "running", .Status = "Up 2 minutes" };
    try std.testing.expectEqualStrings("unhealthy", stateLabel(unhealthy));
    try std.testing.expectEqualStrings("running", stateLabel(plain));
}
