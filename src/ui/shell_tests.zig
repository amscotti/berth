//! The rail's accessible names and tooltips are frame data, not pixels: a
//! headless PNG cannot show AppKit's native tooltip, so the frame is inspected
//! directly. The status line's narrow reading is checked as drawn text.
const std = @import("std");
const nanoui = @import("nanoui");
const ui = nanoui.ui;
const app = @import("../app.zig");
const api = @import("../engine/api.zig");

test "every narrow rail item has a tooltip and an accessible tab name" {
    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
    };
    defer state.arena.deinit();

    var t: ui.Tester = undefined;
    try t.init(std.testing.allocator, .init(&state, app.State.view), .{ .width = 960, .height = 600 });
    defer t.deinit();

    // The narrow rail still prints the engine and clock beneath the status.
    try std.testing.expect(t.containsText("no engine found"));
    try std.testing.expect(t.containsText("not yet"));

    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var elements: std.ArrayList(ui.access.Element) = .empty;
    defer elements.deinit(arena.allocator());
    try ui.access.tree(&t.ctx, arena.allocator(), &elements);

    var tabs: usize = 0;
    var images = false;
    var status_static = false;
    for (elements.items) |e| {
        if (e.role == .static_text and std.mem.indexOf(u8, e.label, "no engine found") != null) status_static = true;
        if (e.role != .tab) continue;
        tabs += 1;
        if (std.mem.startsWith(u8, e.label, "Images · ")) images = true;
    }
    // Ten rail destinations, including Settings which has no digit shortcut.
    try std.testing.expectEqual(@as(usize, 10), tabs);
    try std.testing.expect(images);
    // The status carries a tooltip but is read as text, not a button.
    try std.testing.expect(status_static);
}

test "a sheet hides the page but keeps its own controls accessible" {
    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
        .prune_open = true,
    };
    defer state.arena.deinit();

    var t: ui.Tester = undefined;
    try t.init(std.testing.allocator, .init(&state, app.State.view), .{ .width = 960, .height = 600 });
    defer t.deinit();

    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var elements: std.ArrayList(ui.access.Element) = .empty;
    defer elements.deinit(arena.allocator());
    try ui.access.tree(&t.ctx, arena.allocator(), &elements);

    var tabs: usize = 0;
    var cancel = false;
    var primary = false;
    for (elements.items) |e| {
        if (e.role == .tab) tabs += 1;
        if (e.role == .button and std.mem.eql(u8, e.label, "Cancel")) cancel = true;
        if (e.role == .button and std.mem.eql(u8, e.label, "Clean up")) primary = true;
    }
    // Background tabs are gone, but the sheet's controls are not swallowed by
    // a panel-level element.
    try std.testing.expectEqual(@as(usize, 0), tabs);
    try std.testing.expect(cancel);
    try std.testing.expect(primary);
}

test "the create sheet keeps its name field and buttons accessible" {
    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
        .create_dialog = .{ .open = .volume },
    };
    defer state.arena.deinit();
    defer state.create_dialog.deinit();

    var t: ui.Tester = undefined;
    try t.init(std.testing.allocator, .init(&state, app.State.view), .{ .width = 960, .height = 600 });
    defer t.deinit();

    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var elements: std.ArrayList(ui.access.Element) = .empty;
    defer elements.deinit(arena.allocator());
    try ui.access.tree(&t.ctx, arena.allocator(), &elements);

    var tabs: usize = 0;
    var name_field = false;
    var cancel = false;
    var create = false;
    for (elements.items) |e| {
        if (e.role == .tab) tabs += 1;
        if (e.role == .text_field and std.mem.eql(u8, e.label, "data")) name_field = true;
        if (e.role == .button and std.mem.eql(u8, e.label, "Cancel")) cancel = true;
        if (e.role == .button and std.mem.eql(u8, e.label, "Create")) create = true;
    }
    try std.testing.expectEqual(@as(usize, 0), tabs);
    try std.testing.expect(name_field);
    try std.testing.expect(cancel);
    try std.testing.expect(create);
}

test "rail tooltips are drawn on a layer, not only AppKit help" {
    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
    };
    defer state.arena.deinit();

    var t: ui.Tester = undefined;
    try t.init(std.testing.allocator, .init(&state, app.State.view), .{ .width = 1280, .height = 760 });
    defer t.deinit();

    const label = t.findText("Images") orelse return error.MissingRailLabel;
    t.moveTo(label.x + label.w / 2, label.y + label.h / 2);
    t.advance(0.8);
    try std.testing.expect(t.containsText("Images · —"));
}

test "a sheet blocks pointer clicks behind it" {
    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
        .prune_open = true,
    };
    defer state.arena.deinit();

    var t: ui.Tester = undefined;
    try t.init(std.testing.allocator, .init(&state, app.State.view), .{ .width = 1280, .height = 760 });
    defer t.deinit();

    t.clickText("Containers") catch return error.MissingRailLabel;
    try std.testing.expectEqual(app.Tab.overview, state.tab);
}

test "wheel input does not reach the page behind a sheet" {
    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
        .prune_open = true,
    };
    defer state.arena.deinit();

    var t: ui.Tester = undefined;
    try t.init(std.testing.allocator, .init(&state, app.State.view), .{ .width = 960, .height = 600 });
    defer t.deinit();

    var page_scroll: ?u64 = null;
    var backdrop: ?u64 = null;
    for (t.ctx.nodes.items) |n| {
        if (!n.style.scroll) continue;
        if (n.rect.w >= 959 and n.rect.h >= 599) backdrop = n.id else if (page_scroll == null) page_scroll = n.id;
    }
    try std.testing.expect(backdrop != null);
    const page_id = page_scroll orelse return error.MissingPageScroll;

    t.scroll(500, 300, -200);
    try std.testing.expectEqual(@as(f32, 0), t.ctx.scroll.get(page_id) orelse 0);
}

test "a long engine name wraps in the sheet at 960" {
    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
    };
    defer state.arena.deinit();
    state.ops.pending = .{ .verb = .stop, .kind = .lima, .running = 12 };
    const name = "Lima · a-very-long-instance-name-that-would-otherwise-clip-the-sentence-at-nine-hundred-and-sixty-points";
    state.ops.pending.?.name_len = @min(name.len, state.ops.pending.?.name.len);
    @memcpy(state.ops.pending.?.name[0..state.ops.pending.?.name_len], name[0..state.ops.pending.?.name_len]);

    var t: ui.Tester = undefined;
    try t.init(std.testing.allocator, .init(&state, app.State.view), .{ .width = 960, .height = 600 });
    defer t.deinit();

    var title_wrapped = false;
    var body_wrapped = false;
    for (t.ctx.nodes.items) |n| switch (n.content) {
        .text => |tx| {
            const within_panel = n.rect.w <= 620;
            if (std.mem.startsWith(u8, tx.str, "Stop Lima") and std.mem.endsWith(u8, tx.str, "?") and n.rect.h > 18 and within_panel) {
                title_wrapped = true;
            }
            if (std.mem.indexOf(u8, tx.str, "will stop with the engine") != null and n.rect.h > 18 and within_panel) {
                body_wrapped = true;
            }
        },
        else => {},
    };
    try std.testing.expect(title_wrapped);
    try std.testing.expect(body_wrapped);
}

test "a focused page field cannot take a sheet's Escape" {
    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
        .tab = .settings,
        .ops = .{ .pending = .{ .verb = .stop, .kind = .lima } },
    };
    defer state.arena.deinit();

    var t: ui.Tester = undefined;
    try t.init(std.testing.allocator, .init(&state, app.State.view), .{ .width = 1280, .height = 760 });
    defer t.deinit();

    state.socket_input.focus();
    _ = t.shortcut(.{ .key = ui.Shortcut.escape });
    try std.testing.expect(state.ops.pending == null);
}

test "digits switch views and F focuses the page's search field" {
    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
        // A finished first pass is what puts the search field on the page.
        .last_update = 1,
    };
    defer state.arena.deinit();

    state.prune_open = true;
    state.create_dialog.open = .volume;
    var t: ui.Tester = undefined;
    try t.init(std.testing.allocator, .init(&state, app.State.view), .{ .width = 1280, .height = 760 });
    defer t.deinit();

    _ = t.shortcut(.{ .key = '2', .cmd = true });
    try std.testing.expectEqual(app.Tab.containers, state.tab);
    // The sheets named the page being left.
    try std.testing.expect(!state.prune_open);
    try std.testing.expect(state.create_dialog.open == null);
    _ = t.shortcut(.{ .key = 'f', .cmd = true });
    try std.testing.expect(t.ctx.focus != 0);
}

test "collapsing the creation card drops stale validation" {
    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
        .tab = .images,
        .creation_card_open = true,
        .last_update = 1,
    };
    defer state.arena.deinit();
    state.image_tasks.pull_attempted = true;
    state.image_tasks.run_attempted = true;

    var t: ui.Tester = undefined;
    try t.init(std.testing.allocator, .init(&state, app.State.view), .{ .width = 960, .height = 600 });
    defer t.deinit();

    t.clickText("Get and run images") catch return error.MissingCardHeader;
    try std.testing.expect(!state.creation_card_open);
    try std.testing.expect(!state.image_tasks.pull_attempted);
    try std.testing.expect(!state.image_tasks.run_attempted);
}

test "Escape closes a bulk confirmation and the cleanup sheet" {
    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
    };
    defer state.arena.deinit();

    state.bulk.enabled = true;
    state.bulk.pending = .{ .route = .stop };
    var t: ui.Tester = undefined;
    try t.init(std.testing.allocator, .init(&state, app.State.view), .{ .width = 960, .height = 600 });
    defer t.deinit();

    _ = t.shortcut(.{ .key = ui.Shortcut.escape });
    try std.testing.expect(state.bulk.pending == null);

    state.prune_open = true;
    _ = t.shortcut(.{ .key = ui.Shortcut.escape });
    try std.testing.expect(!state.prune_open);
}

test "Enter submits the create form and Escape closes it" {
    var state: app.State = .{
        .gpa = std.testing.allocator,
        .io = std.testing.io,
        .app = undefined,
        .arena = .init(std.testing.allocator),
    };
    defer state.arena.deinit();
    defer state.create_dialog.deinit();

    state.create_dialog.open = .volume;
    state.create_dialog.name.set("bad name");
    state.client = api.Client.init(state.io, state.gpa, "/nonexistent/dialog.sock");
    var t: ui.Tester = undefined;
    try t.init(std.testing.allocator, .init(&state, app.State.view), .{ .width = 960, .height = 600 });
    defer t.deinit();

    t.key(.enter);
    try std.testing.expect(state.create_dialog.attempted);
    try std.testing.expect(state.create_dialog.open != null);

    _ = t.shortcut(.{ .key = ui.Shortcut.escape });
    try std.testing.expect(state.create_dialog.open == null);
}
