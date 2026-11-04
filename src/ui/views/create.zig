//! The Images page's creation card. Collapsed by default: the page's job is
//! the list of images, and the form only takes the room when someone asks
//! for it. Expanded, the run options share one grid — two equal columns, a
//! full-width Env row, and the actions anchored to the card's right edge.
const nanoui = @import("nanoui");
const ui = nanoui.ui;
const app = @import("../../app.zig");
const image_tasks = @import("../../image_tasks.zig");
const widgets = @import("../widgets.zig");
const theme = @import("../theme.zig");

pub fn card(s: *app.State, c: *ui.Context) void {
    const box = ui.div(c).column().wFull().gap(8).p(14).rounded(10)
        .bg(c.theme.surface).border(c.theme.border);
    const open = s.creation_card_open;
    const head = ui.div(c).row().wFull().itemsCenter().gap(8).control();
    widgets.chip(c, "sparkles", c.theme.accent, 22);
    ui.text(c, "Get and run images", .{ .size = 13, .weight = .semibold });
    ui.spacer(c);
    if (s.image_tasks.pulling()) {
        ui.text(c, "Pulling…", .{ .size = 11, .weight = .medium, .color = theme.warning(c.theme) });
    } else if (s.image_tasks.running()) {
        ui.text(c, "Starting…", .{ .size = 11, .weight = .medium, .color = theme.warning(c.theme) });
    }
    if (!open) {
        ui.text(c, "Pull an image, or run one", .{ .size = 11.5, .color = c.theme.muted });
    }
    ui.icon(c, if (open) "chevron.up" else "chevron.down", .{ .size = 11, .color = c.theme.muted });
    if (head.clicked()) {
        s.creation_card_open = !open;
        s.redraw();
    }
    head.end();
    if (!open) {
        box.end();
        return;
    }

    // Bottom-align Pull with the Reference field, not with label + field:
    // centre alignment parked the button halfway up the label's height.
    const pull_row = ui.div(c).row().wFull().itemsEnd().gap(8);
    _ = field(c, "Reference", &s.image_tasks.pull_input, "nginx:alpine", .fill, .grow);
    if (button(c, "Pull", !s.image_tasks.pulling())) image_tasks.startPull(s);
    pull_row.end();

    ui.div(c).wFull().h(1).bg(c.theme.border).end();

    // At working widths the run options sit on a two-column grid, so the
    // right edges of Image/Name and Ports/Volumes line up. Stacked, each
    // field takes the full width: `grow` is a main-axis flag and would
    // stretch vertically in the card's column.
    const wide = s.layout.content >= 760;
    // One exact column width for both rows: `grow` hands out leftover space
    // after each child's intrinsic width, which made the two rows split at
    // slightly different points. The grid is (card padding + gap) removed
    // from the content width, halved.
    const col_w = (s.layout.content - 28 - 10) / 2;
    var submitted = false;
    if (wide) {
        const first = ui.div(c).row().wFull().itemsCenter().gap(10);
        submitted = field(c, "Image", &s.image_tasks.run_image, "nginx:alpine", .{ .px = col_w }, .fixed) or submitted;
        submitted = field(c, "Name", &s.image_tasks.run_name, "web", .{ .px = col_w }, .fixed) or submitted;
        first.end();
        const second = ui.div(c).row().wFull().itemsCenter().gap(10);
        submitted = field(c, "Ports", &s.image_tasks.run_ports, "18080:80", .{ .px = col_w }, .fixed) or submitted;
        submitted = field(c, "Volumes", &s.image_tasks.run_volumes, "data:/data", .{ .px = col_w }, .fixed) or submitted;
        second.end();
    } else {
        submitted = field(c, "Image", &s.image_tasks.run_image, "nginx:alpine", .fill, .full) or submitted;
        submitted = field(c, "Name", &s.image_tasks.run_name, "web", .fill, .full) or submitted;
        submitted = field(c, "Ports", &s.image_tasks.run_ports, "18080:80", .fill, .full) or submitted;
        submitted = field(c, "Volumes", &s.image_tasks.run_volumes, "data:/data", .fill, .full) or submitted;
    }
    // Env is a direct child of the card's column, so it takes the full width
    // like the stacked fields: grow here would stretch vertically, not out.
    submitted = field(c, "Env", &s.image_tasks.run_env, "KEY=value", .fill, .full) or submitted;

    // The bottom row carries the how-to on the left and the action on the
    // right, so Run shares the right edge with Pull instead of crowding Env.
    const last = ui.div(c).row().wFull().itemsCenter().gap(12);
    ui.text(c, "host:container ports, source:container volumes, KEY=value env; comma-separated", .{
        .size = 10.5,
        .color = c.theme.muted,
        .wrap = true,
    });
    ui.spacer(c);
    if (button(c, "Run", !s.image_tasks.running()) or submitted) image_tasks.startRun(s);
    last.end();
    box.end();
}

const Stretch = enum { grow, full, fixed };

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
    box = switch (stretch) {
        .grow => box.grow(),
        .full => box.wFull(),
        .fixed => box.w(width.px),
    };
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
