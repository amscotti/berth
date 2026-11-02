//! berth's colours: warm paper and charcoal, burnt amber for what is chosen,
//! sage and rust for state. The structure follows nanomon's language — a warm
//! neutral page, a slightly raised card, one accent that carries selection —
//! with berth's own values rather than its palette.
//!
//! It is derived from the framework theme each frame, not chosen once, so the
//! window keeps following the system's light/dark switch.
const std = @import("std");
const nanoui = @import("nanoui");
const ui = nanoui.ui;
const Color = ui.Color;

pub fn of(base: ui.Theme) ui.Theme {
    var t = base;
    if (base.dark) {
        t.background = .hex(0x17140f);
        t.surface = .hex(0x211d16);
        t.surface_hover = .hex(0x2d2820);
        t.text = .hex(0xece6d9);
        t.muted = .hex(0x9b9282);
        t.border = .hex(0x3a3327);
        t.accent = .hex(0xe3a44b);
        t.accent_hover = .hex(0xf2b664);
        t.accent_text = .hex(0x17140f);
        t.track = .hex(0x332d23);
        t.knob = .hex(0xece6d9);
        t.danger = .hex(0xd06a4f);
        t.success = .hex(0x83a76d);
        t.shadow = .{ .a = 0.4 };
    } else {
        t.background = .hex(0xf7f3ea);
        t.surface = .hex(0xfffcf5);
        t.surface_hover = .hex(0xefe8da);
        t.text = .hex(0x1f1b14);
        t.muted = .hex(0x6f6759);
        t.border = .hex(0xe1d8c7);
        t.accent = .hex(0xb26a1e);
        t.accent_hover = .hex(0x955716);
        t.accent_text = .hex(0xfffaf0);
        t.track = .hex(0xe7dfd0);
        t.knob = .hex(0xfffcf5);
        t.danger = .hex(0xb3472d);
        t.success = .hex(0x5d7f45);
        t.shadow = .{ .a = 0.12 };
    }
    return t;
}

/// The tint a page's icon carries in the rail and its cards.
pub fn tint(t: ui.Theme, kind: Kind) Color {
    if (t.dark) return switch (kind) {
        .overview, .containers => t.accent,
        .images, .volumes => .hex(0xd9a441),
        .networks, .events => .hex(0x6fb0a8),
        .stacks => .hex(0xbd80e4),
        .disk, .runtimes => .hex(0x8a9bb0),
        .settings => t.muted,
    };
    return switch (kind) {
        .overview, .containers => t.accent,
        .images, .volumes => .hex(0x9a6e1b),
        .networks, .events => .hex(0x2f8f9d),
        .stacks => .hex(0x7a4ea3),
        .disk, .runtimes => .hex(0x3f6079),
        .settings => t.muted,
    };
}

pub const Kind = enum {
    overview,
    containers,
    images,
    volumes,
    networks,
    events,
    stacks,
    disk,
    runtimes,
    settings,
};

test "the palette follows the base theme's light or dark choice" {
    const light = of(ui.Theme.light);
    try std.testing.expect(!light.dark);
    try std.testing.expect(light.accent.r > light.accent.b); // warm amber, not blue
    try std.testing.expect(light.background.r > light.background.b);

    const dark = of(ui.Theme.dark_theme);
    try std.testing.expect(dark.dark);
    try std.testing.expect(dark.background.r < 0.2);
    try std.testing.expect(dark.text.r > dark.background.r);
}

test "each rail section has a distinct tint" {
    const t = of(ui.Theme.light);
    try std.testing.expect(tint(t, .networks).b > tint(t, .networks).r); // teal
    try std.testing.expect(!std.meta.eql(tint(t, .images), tint(t, .stacks)));
}
