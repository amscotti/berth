//! The Images page's creation card: pull a reference, and run one with the
//! options people actually reuse — name, ports, volumes, environment.
const nanoui = @import("nanoui");
const ui = nanoui.ui;
const app = @import("../../app.zig");
const image_tasks = @import("../../image_tasks.zig");
const widgets = @import("../widgets.zig");

pub fn card(s: *app.State, c: *ui.Context) void {
    const box = ui.div(c).column().wFull().gap(8).p(14).rounded(10)
        .bg(c.theme.surface).border(c.theme.border);
    const head = ui.div(c).row().wFull().itemsCenter().gap(8);
    widgets.chip(c, "sparkles", c.theme.accent, 22);
    ui.text(c, "Get and run images", .{ .size = 13, .weight = .semibold });
    ui.spacer(c);
    if (s.image_tasks.pulling()) {
        ui.text(c, "Pulling…", .{ .size = 11, .weight = .medium, .color = .hex(0xff9f0a) });
    } else if (s.image_tasks.running()) {
        ui.text(c, "Starting…", .{ .size = 11, .weight = .medium, .color = .hex(0xff9f0a) });
    }
    head.end();

    const pull_row = ui.div(c).row().wFull().itemsCenter().gap(8);
    _ = field(c, "Reference", &s.image_tasks.pull_input, "nginx:alpine", .fill, .grow);
    if (button(c, "Pull", !s.image_tasks.pulling())) image_tasks.startPull(s);
    pull_row.end();

    ui.div(c).wFull().h(1).bg(c.theme.border).end();

    // At working widths the run options share rows; stacked, each takes the
    // full width. `grow` is a main-axis flag: in the card's column it would
    // stretch vertically, so the narrow branch uses wFull instead.
    const wide = s.layout.content >= 760;
    var submitted = false;
    if (wide) {
        const first = ui.div(c).row().wFull().itemsCenter().gap(8);
        submitted = field(c, "Image", &s.image_tasks.run_image, "nginx:alpine", .fill, .grow) or submitted;
        submitted = field(c, "Name", &s.image_tasks.run_name, "web", .{ .px = 150 }, .grow) or submitted;
        first.end();
        const second = ui.div(c).row().wFull().itemsCenter().gap(8);
        submitted = field(c, "Ports", &s.image_tasks.run_ports, "18080:80", .fill, .grow) or submitted;
        submitted = field(c, "Volumes", &s.image_tasks.run_volumes, "data:/data", .fill, .grow) or submitted;
        second.end();
        const third = ui.div(c).row().wFull().itemsCenter().gap(8);
        submitted = field(c, "Env", &s.image_tasks.run_env, "KEY=value", .fill, .grow) or submitted;
        if (button(c, "Run", !s.image_tasks.running()) or submitted) image_tasks.startRun(s);
        third.end();
    } else {
        submitted = field(c, "Image", &s.image_tasks.run_image, "nginx:alpine", .fill, .full) or submitted;
        submitted = field(c, "Name", &s.image_tasks.run_name, "web", .fill, .full) or submitted;
        submitted = field(c, "Ports", &s.image_tasks.run_ports, "18080:80", .fill, .full) or submitted;
        submitted = field(c, "Volumes", &s.image_tasks.run_volumes, "data:/data", .fill, .full) or submitted;
        submitted = field(c, "Env", &s.image_tasks.run_env, "KEY=value", .fill, .full) or submitted;
        const last = ui.div(c).row().wFull().justifyEnd();
        if (button(c, "Run", !s.image_tasks.running()) or submitted) image_tasks.startRun(s);
        last.end();
    }

    ui.text(c, "Ports are host:container; volumes source:container; environment KEY=value. Separate entries with commas.", .{
        .size = 10.5,
        .color = c.theme.muted,
        .wrap = true,
    });
    box.end();
}

const Stretch = enum { grow, full };

/// One labelled text field; returns whether Enter was pressed in it, so the
/// card can submit the same action its button would.
fn field(
    c: *ui.Context,
    label: []const u8,
    buffer: *ui.TextBuffer,
    placeholder: []const u8,
    width: ui.Length,
    stretch: Stretch,
) bool {
    var box = ui.div(c).column().gap(3);
    box = if (stretch == .grow) box.grow() else box.wFull();
    ui.text(c, label, .{ .size = 10.5, .color = c.theme.muted });
    const input = ui.textField(c, buffer, .{
        .placeholder = placeholder,
        .width = width,
        .size = 12,
    });
    box.end();
    return input.submitted;
}

fn button(c: *ui.Context, label: []const u8, enabled: bool) bool {
    const bg = if (enabled) c.theme.accent else c.theme.surface_hover;
    const fg = if (enabled) c.theme.accent_text else c.theme.muted;
    const box = ui.div(c).row().h(28).px(12).itemsCenter().rounded(14).bg(bg).control();
    ui.text(c, label, .{ .size = 12, .weight = .medium, .color = fg });
    const clicked = enabled and box.clicked();
    box.end();
    return clicked;
}
