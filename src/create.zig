//! The Volumes and Networks pages' create dialogs: one name, one POST, and a
//! refresh. The name is validated here so the engine's JSON body never needs
//! escaping and a bad name is a sentence, not a 400.
const std = @import("std");
const nanoui = @import("nanoui");
const ui = nanoui.ui;
const app = @import("app.zig");
const create_ops = @import("engine/create.zig");
const widgets = @import("ui/widgets.zig");

pub const Kind = enum { volume, network };

pub const State = struct {
    open: ?Kind = null,
    name: ui.TextBuffer = .{},

    pub fn deinit(self: *State) void {
        self.name.deinit();
    }
};

pub fn open(s: *app.State, kind: Kind) void {
    // One sheet at a time: a queued stop confirmation or the prune dialog is
    // about a different action and would otherwise stack above this form.
    s.bulk.pending = null;
    s.bulk.project_len = 0;
    s.prune_open = false;
    s.create_dialog.open = kind;
    s.create_dialog.name.clear();
    s.create_dialog.name.focus();
    s.redraw();
}

pub fn close(s: *app.State) void {
    s.create_dialog.open = null;
    s.redraw();
}

pub fn submit(s: *app.State) void {
    const kind = s.create_dialog.open orelse return;
    var client = s.client orelse {
        s.notify("no engine is selected", true);
        return;
    };
    const name = s.create_dialog.name.text();
    if (!create_ops.validName(name)) {
        s.notifyFmt("a {s} name is 2–128 letters, digits, dot, dash or underscore, starting with a letter or digit", .{noun(kind)}, true);
        return;
    }
    // A duplicate volume is not an error to Docker: /volumes/create returns
    // the existing one, which would read here as "created".
    if (kind == .volume) {
        for (s.volumes) |v| {
            if (std.mem.eql(u8, v.Name, name)) {
                s.notifyFmt("a volume named {s} already exists", .{name}, true);
                return;
            }
        }
    }
    const result = switch (kind) {
        .volume => create_ops.volume(&client, name),
        .network => create_ops.network(&client, name),
    };
    result catch {
        s.notifyFmt("could not create the {s}: {s}", .{
            noun(kind),
            if (client.err_body.len > 0) client.err_body else "the engine did not answer",
        }, true);
        return;
    };
    s.create_dialog.open = null;
    s.notifyFmt("{s} {s} created", .{ if (kind == .volume) "Volume" else "Network", name }, false);
    s.refresh();
    s.redraw();
}

fn noun(kind: Kind) []const u8 {
    return switch (kind) {
        .volume => "volume",
        .network => "network",
    };
}

pub fn dialog(s: *app.State, c: *ui.Context) void {
    const kind = s.create_dialog.open orelse return;
    if (c.shortcut(.{ .key = ui.Shortcut.escape })) {
        close(s);
        return;
    }
    const box = ui.div(c).column().wFull().gap(8).p(12).rounded(10).bg(c.theme.surface).border(c.theme.accent);
    defer box.end();
    ui.text(c, c.print("New {s}", .{noun(kind)}), .{ .size = 13, .weight = .semibold });
    ui.text(c, "Name", .{ .size = 11, .color = c.theme.muted });
    const field = ui.textField(c, &s.create_dialog.name, .{
        .placeholder = if (kind == .volume) "data" else "backend",
        .width = .fill,
        .size = 12,
    });
    if (field.submitted) submit(s);
    ui.text(c, "2–128 letters, digits, dot, dash or underscore; the name starts with a letter or digit.", .{
        .size = 11,
        .color = c.theme.muted,
        .wrap = true,
    });
    const buttons = ui.div(c).row().wFull().justifyEnd().gap(8).pt(2);
    if (widgets.dialogButton(c, "Cancel", false, false)) close(s);
    if (widgets.dialogButton(c, "Create", false, true)) submit(s);
    buttons.end();
}


test "the dialog names its target" {
    try std.testing.expectEqualStrings("volume", noun(.volume));
    try std.testing.expectEqualStrings("network", noun(.network));
}
