//! The Volumes and Networks pages' create dialogs: one name, one POST, and a
//! refresh. The name is validated here so the engine's JSON body never needs
//! escaping and a bad name is a sentence, not a 400.
const std = @import("std");
const nanoui = @import("nanoui");
const ui = nanoui.ui;
const app = @import("app.zig");
const actions = @import("actions.zig");
const api = @import("engine/api.zig");
const create_ops = @import("engine/create.zig");
const sheets = @import("sheets.zig");
const theme = @import("ui/theme.zig");
const widgets = @import("ui/widgets.zig");

pub const Kind = enum { volume, network };

/// Why a typed name cannot be sent. The form renders the sentence beside the
/// field; a toast is reserved for what the engine says after the attempt.
pub const NameFailure = enum { empty, too_short, too_long, bad_start, bad_char, duplicate };

pub const State = struct {
    open: ?Kind = null,
    name: ui.TextBuffer = .{},
    /// Set by a failed submit; the message is recomputed from the current
    /// text each frame, so correcting the field clears it without another
    /// click.
    attempted: bool = false,
    /// Rises on every open/close so a job can tell whether it still owns the
    /// form it submitted, rather than closing a newer one that looks alike.
    generation: u64 = 0,

    pub fn deinit(self: *State) void {
        self.name.deinit();
    }
};

/// The same rules as `engine/create.zig`, plus the duplicate-volume check
/// Docker would otherwise hide by returning the existing volume as a success.
pub fn validateName(kind: Kind, name: []const u8, volumes: []const api.Volume) ?NameFailure {
    if (name.len == 0) return .empty;
    if (name.len < 2) return .too_short;
    if (name.len > 128) return .too_long;
    if (!std.ascii.isAlphanumeric(name[0])) return .bad_start;
    for (name) |ch| {
        if (!(std.ascii.isAlphanumeric(ch) or ch == '_' or ch == '.' or ch == '-')) return .bad_char;
    }
    if (kind == .volume) {
        for (volumes) |v| if (std.mem.eql(u8, v.Name, name)) return .duplicate;
    }
    return null;
}

pub fn describe(kind: Kind, failure: NameFailure, name: []const u8, buf: []u8) []const u8 {
    return switch (failure) {
        .empty => "Enter a name.",
        .too_short => "A name needs at least 2 characters.",
        .too_long => "A name can be at most 128 characters.",
        .bad_start => "Start the name with a letter or digit.",
        .bad_char => "Use only letters, digits, dot, dash or underscore.",
        .duplicate => std.fmt.bufPrint(buf, "A {s} named {s} already exists.", .{ noun(kind), name }) catch "That name already exists.",
    };
}

pub fn open(s: *app.State, kind: Kind) void {
    // One sheet at a time: a queued stop confirmation or the prune dialog is
    // about a different action and would otherwise stack above this form.
    sheets.dismiss(s);
    s.create_dialog.open = kind;
    s.create_dialog.generation +%= 1;
    s.create_dialog.attempted = false;
    s.create_dialog.name.clear();
    s.create_dialog.name.focus();
    s.redraw();
}

pub fn close(s: *app.State) void {
    s.create_dialog.open = null;
    s.create_dialog.generation +%= 1;
    s.create_dialog.attempted = false;
    s.redraw();
}

pub fn submit(s: *app.State) void {
    const kind = s.create_dialog.open orelse return;
    const name = s.create_dialog.name.text();
    if (validateName(kind, name, s.volumes) != null) {
        // The message is drawn beside the field; do not send it to the rail,
        // where it would be truncated and far from the name being corrected.
        s.create_dialog.attempted = true;
        s.redraw();
        return;
    }
    if (s.client == null) {
        s.notify("no engine is selected", true);
        return;
    }
    // The dialog stays open until the worker reports: a success closes it, a
    // failure leaves the typed name in place to correct or retry.
    s.create_dialog.attempted = false;
    _ = actions.create(s, switch (kind) {
        .volume => .volume,
        .network => .network,
    }, name);
}

fn noun(kind: Kind) []const u8 {
    return switch (kind) {
        .volume => "volume",
        .network => "network",
    };
}

pub fn dialog(s: *app.State, c: *ui.Context) void {
    const kind = s.create_dialog.open orelse return;
    // The sheet's own field may keep the keyboard; a field left behind on the
    // page may not. Without this, a focused socket field could eat Enter.
    const keys = c.focus == 0 or c.focus == s.create_dialog.name.last_id;
    if (keys and c.shortcut(.{ .key = ui.Shortcut.escape })) {
        close(s);
        return;
    }
    // Plain Enter is not a modifier shortcut; the field reports it, and this
    // covers the case where the dialog has focus but no field does.
    const enter = keys and c.input.key == .enter;
    const failure = if (s.create_dialog.attempted)
        validateName(kind, s.create_dialog.name.text(), s.volumes)
    else
        null;

    const box = ui.div(c).column().wFull().gap(8).p(12).rounded(10).bg(c.theme.surface).border(c.theme.accent);
    defer box.end();
    const title = c.print("New {s}", .{noun(kind)});
    const title_at: u32 = @intCast(c.nodes.items.len);
    ui.text(c, title, .{ .size = 13, .weight = .semibold });
    ui.access.describe(c, title_at, .{ .role = .static_text, .label = title, .header = true });
    ui.text(c, "Name", .{ .size = 11, .color = c.theme.muted });
    const field = ui.textField(c, &s.create_dialog.name, .{
        .placeholder = if (kind == .volume) "data" else "backend",
        .width = .fill,
        .size = 12,
    });
    if (field.submitted or enter) submit(s);
    if (failure) |f| {
        var buf: [160]u8 = undefined;
        ui.text(c, c.print("{s}", .{describe(kind, f, s.create_dialog.name.text(), &buf)}), .{
            .size = 11,
            .color = theme.dangerText(c.theme),
            .wrap = true,
        });
    }
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

test "opening and closing a form advances its generation" {
    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
    };
    defer state.arena.deinit();
    defer state.create_dialog.deinit();

    open(&state, .volume);
    const first = state.create_dialog.generation;
    close(&state);
    try std.testing.expectEqual(first + 1, state.create_dialog.generation);
    open(&state, .network);
    try std.testing.expectEqual(first + 2, state.create_dialog.generation);
}

test "create names fail with a sentence for the field, not a toast" {
    const volumes = [_]api.Volume{.{ .Name = "data" }};
    try std.testing.expectEqual(NameFailure.empty, validateName(.volume, "", volumes[0..]).?);
    try std.testing.expectEqual(NameFailure.too_short, validateName(.volume, "a", volumes[0..]).?);
    try std.testing.expectEqual(NameFailure.bad_start, validateName(.network, "-net", volumes[0..]).?);
    try std.testing.expectEqual(NameFailure.bad_char, validateName(.network, "two words", volumes[0..]).?);
    try std.testing.expectEqual(NameFailure.duplicate, validateName(.volume, "data", volumes[0..]).?);
    // A duplicate network is the engine's 409, not a local rule.
    try std.testing.expect(validateName(.network, "data", volumes[0..]) == null);

    var buf: [160]u8 = undefined;
    try std.testing.expectEqualStrings(
        "A volume named data already exists.",
        describe(.volume, .duplicate, "data", &buf),
    );
    try std.testing.expectEqualStrings(
        "Use only letters, digits, dot, dash or underscore.",
        describe(.network, .bad_char, "x", &buf),
    );
}

test "opening a form dismisses the other sheets and the last mistake" {
    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
    };
    defer state.arena.deinit();
    defer state.create_dialog.deinit();

    state.prune_open = true;
    state.bulk.pending = .{ .route = .stop };
    state.create_dialog.attempted = true;
    open(&state, .volume);
    try std.testing.expect(!state.prune_open);
    try std.testing.expect(state.bulk.pending == null);
    try std.testing.expect(!state.create_dialog.attempted);
    try std.testing.expectEqual(Kind.volume, state.create_dialog.open.?);
}
