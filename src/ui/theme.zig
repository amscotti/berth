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
        t.accent = .hex(0xa35d14);
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

/// The amber a graph or a store segment is filled with. Text that carries a
/// warning needs the darker `warning` below; a 2:1 amber is fine as a drawn
/// mark and unreadable as a word.
pub const warning_fill: Color = .hex(0xff9f0a);

/// One token for the volumes store, shared by the bars and their legends: the
/// legend used to pick a different amber from the segment, which made the
/// legend describe a colour that was not on screen.
pub const store_volume: Color = warning_fill;

/// The bright accent for graphs, bars and legends. `accent` itself is the
/// text-bearing control fill: cream text needs the darker light-theme value,
/// while a drawn line or segment does not.
pub fn accentFill(t: ui.Theme) Color {
    return if (t.dark) t.accent else .hex(0xb26a1e);
}

/// Warning text and pills. The bright amber is a dark-theme value; on paper
/// it lands at 2:1, so the light theme gets a burnt variant instead.
pub fn warning(t: ui.Theme) Color {
    return if (t.dark) warning_fill else .hex(0x7a4700);
}

/// Success text and pills, as opposed to the brighter graph fill. The light
/// theme's sage was only just over 4:1 and a pill's own tint lowered it
/// further; the darker green keeps the word readable on both surfaces.
pub fn successText(t: ui.Theme) Color {
    return if (t.dark) t.success else .hex(0x3f5c2e);
}

/// Danger text and error banners. The light warning uses the same rule: the
/// raw rust is a fill, not a word, once a pill or a row wash sits under it.
pub fn dangerText(t: ui.Theme) Color {
    return if (t.dark) t.danger else .hex(0x8f2f1c);
}

/// Accent text for status pills ("created"). The selector accent is a fill;
/// its light-theme text needs more weight to survive a selected row.
pub fn accentText(t: ui.Theme) Color {
    return if (t.dark) t.accent else .hex(0x7d430f);
}

/// Muted text for status pills ("exited"). General secondary prose can stay
/// at `muted`; a pill on a tinted row cannot.
pub fn mutedText(t: ui.Theme) Color {
    return if (t.dark) t.muted else .hex(0x5a5245);
}

/// The running card's label. Its graph fill is a lighter amber than the
/// warning tone, so the dark theme keeps that value; on paper the label needs
/// the same readable amber the other warning text uses.
pub fn runningText(t: ui.Theme) Color {
    return if (t.dark) .hex(0xd9a441) else warning(t);
}

/// The contrast ratio between two opaque colours, for the palette tests.
pub fn contrastRatio(a: Color, b: Color) f32 {
    const la = luminance(a);
    const lb = luminance(b);
    const hi = @max(la, lb);
    const lo = @min(la, lb);
    return (hi + 0.05) / (lo + 0.05);
}

fn luminance(c: Color) f32 {
    const lin = struct {
        fn f(v: f32) f32 {
            return if (v <= 0.04045) v / 12.92 else std.math.pow(f32, (v + 0.055) / 1.055, 2.4);
        }
    }.f;
    return 0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b);
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

test "on-accent text and the full-opacity rail figure clear 4.5:1" {
    const light = of(ui.Theme.light);
    try std.testing.expect(contrastRatio(light.accent_text, light.accent) >= 4.5);
    // The rail figure is this token at full opacity. The old 80% blend is the
    // 3.19:1 value that must not come back while the fill is this dark.
    const faded = light.accent.mix(light.accent_text, 0.8);
    try std.testing.expect(contrastRatio(faded, light.accent) < 4.5);
    // Drawn fills keep the brighter accent; only the text-bearing control
    // fill is darkened.
    try std.testing.expectEqual(ui.Color.hex(0xb26a1e), accentFill(light));

    const dark = of(ui.Theme.dark_theme);
    try std.testing.expect(contrastRatio(dark.accent_text, dark.accent) >= 4.5);
    try std.testing.expectEqual(ui.Color.hex(0xe3a44b), dark.accent);
    try std.testing.expectEqual(dark.accent, accentFill(dark));
}

test "warning and success text clear 4.5:1 against their own pill tint" {
    for ([_]ui.Theme{ of(ui.Theme.light), of(ui.Theme.dark_theme) }) |t| {
        const warn = warning(t);
        const success = successText(t);
        // A pill sits on a 14% wash of its own colour, not on bare surface;
        // that wash is what pushed the light ambers under the threshold.
        try std.testing.expect(contrastRatio(warn, t.surface.mix(warn, 0.14)) >= 4.5);
        try std.testing.expect(contrastRatio(success, t.surface.mix(success, 0.14)) >= 4.5);
    }
    // The dark theme keeps the colours it had; graph fills stay bright.
    const dark = of(ui.Theme.dark_theme);
    try std.testing.expectEqual(warning_fill, warning(dark));
    try std.testing.expectEqual(dark.success, successText(dark));
    try std.testing.expectEqual(ui.Color.hex(0xd9a441), runningText(dark));
    try std.testing.expectEqual(store_volume, warning_fill);
    const light = of(ui.Theme.light);
    try std.testing.expect(contrastRatio(runningText(light), light.surface) >= 4.5);
    try std.testing.expect(contrastRatio(dangerText(light), light.background.mix(dangerText(light), 0.12)) >= 4.7);
    try std.testing.expectEqual(dark.danger, dangerText(dark));
    try std.testing.expectEqual(dark.accent, accentText(dark));
    try std.testing.expectEqual(dark.muted, mutedText(dark));
}
