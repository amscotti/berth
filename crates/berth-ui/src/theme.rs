//! berth's color tokens: warm paper and charcoal, burnt amber for what is
//! chosen, sage and rust for state.
//!
//! The values are the measured light and dark palettes from the unreleased
//! Zig app (`docs/PRODUCT-SPEC.md`). They are stored as [`Hsla`] because that
//! is what GPUI paints; the contrast helper converts back to sRGB for WCAG.
//!
//! Text that carries a warning needs the darker `warning` variant. A bright
//! amber is fine as a drawn mark and unreadable as a word.

use gpui::{Hsla, WindowAppearance, rgb};

/// The colors shared by the page and its chrome.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    /// Page background and recessed wells.
    pub background: Hsla,
    /// Rail, cards, inspector.
    pub surface: Hsla,
    /// Hover, secondary buttons, alternate rows.
    pub surface_hover: Hsla,
    /// Primary text.
    pub text: Hsla,
    /// Secondary text and labels.
    pub muted: Hsla,
    /// Hairlines and card edges.
    pub border: Hsla,
    /// Selection and primary controls.
    pub accent: Hsla,
    /// Primary hover.
    pub accent_hover: Hsla,
    /// Text on an accent fill.
    pub accent_text: Hsla,
    /// Bar tracks and inactive checkboxes.
    pub track: Hsla,
    /// Switches.
    pub knob: Hsla,
    /// Destructive fill.
    pub danger: Hsla,
    /// Success fill.
    pub success: Hsla,
    /// Warning fill and the volumes store.
    pub warning_fill: Hsla,
}

/// The warm paper theme.
pub fn light() -> Tokens {
    Tokens {
        background: hex(0xf7f3ea),
        surface: hex(0xfffcf5),
        surface_hover: hex(0xefe8da),
        text: hex(0x1f1b14),
        muted: hex(0x6f6759),
        border: hex(0xe1d8c7),
        accent: hex(0xa35d14),
        accent_hover: hex(0x955716),
        accent_text: hex(0xfffaf0),
        track: hex(0xe7dfd0),
        knob: hex(0xfffcf5),
        danger: hex(0xb3472d),
        success: hex(0x5d7f45),
        warning_fill: hex(0xff9f0a),
    }
}

/// The charcoal theme.
pub fn dark() -> Tokens {
    Tokens {
        background: hex(0x17140f),
        surface: hex(0x211d16),
        surface_hover: hex(0x2d2820),
        text: hex(0xece6d9),
        muted: hex(0x9b9282),
        border: hex(0x3a3327),
        accent: hex(0xe3a44b),
        accent_hover: hex(0xf2b664),
        accent_text: hex(0x17140f),
        track: hex(0x332d23),
        knob: hex(0xece6d9),
        danger: hex(0xd06a4f),
        success: hex(0x83a76d),
        warning_fill: hex(0xff9f0a),
    }
}

/// Resolve the page tokens for the window's current system appearance.
pub fn for_appearance(appearance: WindowAppearance) -> Tokens {
    match appearance {
        WindowAppearance::Dark | WindowAppearance::VibrantDark => dark(),
        WindowAppearance::Light | WindowAppearance::VibrantLight => light(),
    }
}

impl Tokens {
    /// The amber a graph or a store segment is filled with. `accent` itself is
    /// the text-bearing control fill: cream text needs the darker light value,
    /// while a drawn line or segment does not.
    pub fn accent_fill(self) -> Hsla {
        if self.background == dark().background {
            self.accent
        } else {
            hex(0xb26a1e)
        }
    }

    /// Warning text and pills. The bright amber is a dark-theme value; on
    /// paper it lands near 2:1, so the light theme gets a burnt variant.
    pub fn warning_text(self) -> Hsla {
        if self.background == dark().background {
            self.warning_fill
        } else {
            hex(0x7a4700)
        }
    }

    /// Success text and pills, as opposed to the brighter graph fill.
    pub fn success_text(self) -> Hsla {
        if self.background == dark().background {
            self.success
        } else {
            hex(0x3f5c2e)
        }
    }

    /// Danger text and error banners.
    pub fn danger_text(self) -> Hsla {
        if self.background == dark().background {
            // A 12% wash of the fill drops the raw #d06a4f to 4.46:1. A
            // slightly lighter word token keeps error text readable without
            // changing the destructive fill.
            hex(0xd4745a)
        } else {
            hex(0x8f2f1c)
        }
    }

    /// Accent text for status pills ("created").
    pub fn accent_text_pill(self) -> Hsla {
        if self.background == dark().background {
            self.accent
        } else {
            hex(0x7d430f)
        }
    }

    /// Muted text for status pills ("exited").
    pub fn muted_text_pill(self) -> Hsla {
        if self.background == dark().background {
            self.muted
        } else {
            hex(0x5a5245)
        }
    }

    /// The running card's label. Its graph fill is lighter than the warning
    /// tone, so the dark theme keeps that value; on paper the label needs the
    /// readable amber.
    pub fn running_text(self) -> Hsla {
        if self.background == dark().background {
            hex(0xd9a441)
        } else {
            self.warning_text()
        }
    }

    /// The one token for the volumes store, shared by bars and legends.
    pub fn store_volume(self) -> Hsla {
        self.warning_fill
    }
}

/// The tint a page's icon carries in the rail and its cards.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TintKind {
    Overview,
    Containers,
    Images,
    Volumes,
    Networks,
    Events,
    Stacks,
    Disk,
    Runtimes,
    Settings,
}

pub fn tint(tokens: Tokens, kind: TintKind) -> Hsla {
    if tokens.background == dark().background {
        match kind {
            TintKind::Overview | TintKind::Containers => tokens.accent,
            TintKind::Images | TintKind::Volumes => hex(0xd9a441),
            TintKind::Networks | TintKind::Events => hex(0x6fb0a8),
            TintKind::Stacks => hex(0xbd80e4),
            TintKind::Disk | TintKind::Runtimes => hex(0x8a9bb0),
            TintKind::Settings => tokens.muted,
        }
    } else {
        match kind {
            TintKind::Overview | TintKind::Containers => tokens.accent,
            TintKind::Images | TintKind::Volumes => hex(0x9a6e1b),
            TintKind::Networks | TintKind::Events => hex(0x2f8f9d),
            TintKind::Stacks => hex(0x7a4ea3),
            TintKind::Disk | TintKind::Runtimes => hex(0x3f6079),
            TintKind::Settings => tokens.muted,
        }
    }
}

/// Parse an opaque sRGB integer into the theme's color type.
pub fn hex(value: u32) -> Hsla {
    rgb(value).into()
}

/// Linear RGB blend of two opaque colors. `t` is the weight of `b`.
pub fn mix(a: Hsla, b: Hsla, t: f32) -> Hsla {
    let a = a.to_rgb();
    let b = b.to_rgb();
    let t = t.clamp(0.0, 1.0);
    gpui::Rgba {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: 1.0,
    }
    .into()
}

/// WCAG 2.x relative contrast between two opaque colors.
pub fn contrast_ratio(a: Hsla, b: Hsla) -> f32 {
    let a = relative_luminance(a.to_rgb());
    let b = relative_luminance(b.to_rgb());
    let (lighter, darker) = if a >= b { (a, b) } else { (b, a) };
    (lighter + 0.05) / (darker + 0.05)
}

fn relative_luminance(color: gpui::Rgba) -> f32 {
    fn linear(channel: f32) -> f32 {
        let channel = channel.clamp(0.0, 1.0);
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    }

    0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn themes() -> [(&'static str, Tokens); 2] {
        [("light", light()), ("dark", dark())]
    }

    #[test]
    fn body_text_clears_its_floor() {
        for (name, tokens) in themes() {
            let ratio = contrast_ratio(tokens.text, tokens.background);
            assert!(ratio >= 7.0, "{name} body contrast was {ratio:.2}:1");
        }
    }

    #[test]
    fn on_accent_text_clears_four_and_a_half() {
        for (name, tokens) in themes() {
            let ratio = contrast_ratio(tokens.accent_text, tokens.accent);
            assert!(ratio >= 4.5, "{name} on-accent contrast was {ratio:.2}:1");
        }
    }

    #[test]
    fn warning_and_success_text_clear_their_pill_wash() {
        for (name, tokens) in themes() {
            let warn = tokens.warning_text();
            let success = tokens.success_text();
            let warn_wash = mix(tokens.surface, warn, 0.14);
            let success_wash = mix(tokens.surface, success, 0.14);
            let warn_ratio = contrast_ratio(warn, warn_wash);
            let success_ratio = contrast_ratio(success, success_wash);
            assert!(
                warn_ratio >= 4.5,
                "{name} warning contrast was {warn_ratio:.2}:1"
            );
            assert!(
                success_ratio >= 4.5,
                "{name} success contrast was {success_ratio:.2}:1"
            );
        }
    }

    #[test]
    fn danger_text_clears_error_washes() {
        for (name, tokens) in themes() {
            let danger = tokens.danger_text();
            let wash = mix(tokens.background, danger, 0.12);
            let ratio = contrast_ratio(danger, wash);
            assert!(ratio >= 4.5, "{name} danger contrast was {ratio:.2}:1");
        }
    }

    #[test]
    fn the_palette_follows_the_appearance_choice() {
        let light = for_appearance(WindowAppearance::Light);
        let dark = for_appearance(WindowAppearance::Dark);
        assert_eq!(light, super::light());
        assert_eq!(dark, super::dark());
        assert_ne!(light.background, dark.background);
        let light_accent = light.accent.to_rgb();
        assert!(
            light_accent.r > light_accent.b,
            "light accent is warm amber"
        );
        assert!(dark.text.to_rgb().r > dark.background.to_rgb().r);
    }

    #[test]
    fn every_rail_section_has_a_distinct_tint() {
        let tokens = light();
        assert_ne!(
            tint(tokens, TintKind::Networks),
            tint(tokens, TintKind::Images)
        );
        assert_ne!(tint(tokens, TintKind::Stacks), tint(tokens, TintKind::Disk));
        assert_ne!(
            tint(tokens, TintKind::Overview),
            tint(tokens, TintKind::Settings)
        );
    }

    #[test]
    fn tokens_are_distinct_and_washes_are_visible() {
        for (name, tokens) in themes() {
            assert_ne!(tokens.background, tokens.text, "{name} paper equals ink");
            assert_ne!(
                tokens.background, tokens.surface,
                "{name} surface invisible"
            );
            assert_ne!(tokens.border, tokens.background, "{name} rule invisible");
            assert_ne!(tokens.track, tokens.background, "{name} track invisible");
        }
    }
}
