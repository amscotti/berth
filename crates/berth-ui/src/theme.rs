//! berth's color tokens: warm paper and charcoal, burnt amber for what is
//! chosen, sage and rust for state.
//!
//! The values are the measured light and dark palettes from the unreleased
//! Zig app (`docs/PRODUCT-SPEC.md`). They are stored as [`Hsla`] because that
//! is what GPUI paints; the contrast helper converts back to sRGB for WCAG.
//!
//! A view never mixes its own wash. Every "this colour at some transparency"
//! the design uses is a method here, with the contrast it needs tested below.
//!
//! Text that carries a warning needs the darker `warning` variant. A bright
//! amber is fine as a drawn mark and unreadable as a word.

use gpui::{Hsla, WindowAppearance, rgb};

/// How much of a colour a background wash carries.
pub mod wash {
    /// The row that is open in the inspector. Eight percent, not the ten the
    /// spec first named: at ten, muted text on the selected row reads 4.44:1
    /// in the light theme, and the row is still unmistakable at eight.
    pub const SELECTED: f32 = 0.08;
    /// The active engine's card.
    pub const ACTIVE: f32 = 0.08;
    /// A status pill's background, and the tint behind an icon.
    pub const PILL: f32 = 0.14;
    /// A chip or tab that is on.
    pub const ACCENT: f32 = 0.16;
    /// The wash behind destructive text.
    pub const DANGER: f32 = 0.12;
    /// The alternative row of a table: `surface_hover` at this strength.
    pub const ZEBRA: f32 = 0.35;
    /// Selected text in a field.
    pub const SELECTION: f32 = 0.24;
    /// The quietest text that is still text.
    pub const FAINT: f32 = 0.75;
    /// "Elsewhere" in the host bar.
    pub const ELSEWHERE: f32 = 0.55;
    /// The veil behind a sheet, by theme.
    pub const BACKDROP_LIGHT: f32 = 0.38;
    /// The veil behind a sheet, by theme.
    pub const BACKDROP_DARK: f32 = 0.58;
    /// A graph well's fill and hairline.
    pub const WELL: f32 = 0.28;
    /// A graph well's fill and hairline.
    pub const WELL_BORDER: f32 = 0.55;
    /// How faint a control is drawn while it can't be used.
    pub const DISABLED: f32 = 0.45;
    /// How much a hover moves a filled control's fill toward the text
    /// colour. Only for fills whose text has contrast to spare.
    pub const HOVER: f32 = 0.08;
    /// How much being pressed moves it further.
    pub const PRESSED: f32 = 0.16;
    /// The strength of the hairline a wash-filled control shows on hover.
    pub const OUTLINE_HOVER: f32 = 0.55;
}

/// Which palette is in use. A token never has to be recognised by its colour.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    #[default]
    Light,
    Dark,
}

impl Theme {
    pub fn for_appearance(appearance: WindowAppearance) -> Self {
        match appearance {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Theme::Dark,
            WindowAppearance::Light | WindowAppearance::VibrantLight => Theme::Light,
        }
    }
}

/// The colors shared by the page and its chrome.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub theme: Theme,
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
        theme: Theme::Light,
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
        theme: Theme::Dark,
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

/// Resolve the page tokens for an appearance.
pub fn for_appearance(appearance: WindowAppearance) -> Tokens {
    match Theme::for_appearance(appearance) {
        Theme::Light => light(),
        Theme::Dark => dark(),
    }
}

/// The tokens the window is being painted with, kept as a GPUI global for the
/// entities that render on their own and so can't be handed the root's tokens
/// on each frame: a text field, for instance.
///
/// The root sets it whenever the theme changes, including for an Appearance
/// choice that differs from the system's. Until it has, reading falls back to
/// the window's own appearance.
pub struct ActiveTokens(pub Tokens);

impl gpui::Global for ActiveTokens {}

impl ActiveTokens {
    /// Make `tokens` the active ones.
    pub fn set(tokens: Tokens, cx: &mut gpui::App) {
        if cx
            .try_global::<Self>()
            .is_some_and(|active| active.0 == tokens)
        {
            return;
        }
        cx.set_global(Self(tokens));
    }

    /// The active tokens.
    pub fn get(window: &gpui::Window, cx: &gpui::App) -> Tokens {
        cx.try_global::<Self>()
            .map_or_else(|| for_appearance(window.appearance()), |active| active.0)
    }
}

impl Tokens {
    pub fn is_dark(self) -> bool {
        self.theme == Theme::Dark
    }

    /// The amber a graph or a store segment is filled with. `accent` itself is
    /// the text-bearing control fill: cream text needs the darker light value,
    /// while a drawn line or segment does not.
    pub fn accent_fill(self) -> Hsla {
        match self.theme {
            Theme::Dark => self.accent,
            Theme::Light => hex(0xb26a1e),
        }
    }

    /// Warning text and pills. The bright amber is a dark-theme value; on
    /// paper it lands near 2:1, so the light theme gets a burnt variant.
    pub fn warning_text(self) -> Hsla {
        match self.theme {
            Theme::Dark => self.warning_fill,
            Theme::Light => hex(0x7a4700),
        }
    }

    /// Success text and pills, as opposed to the brighter graph fill.
    pub fn success_text(self) -> Hsla {
        match self.theme {
            Theme::Dark => self.success,
            Theme::Light => hex(0x3f5c2e),
        }
    }

    /// Danger text and error banners.
    pub fn danger_text(self) -> Hsla {
        match self.theme {
            // A 12% wash of the fill drops the raw #d06a4f below 4.5:1, and
            // so does the first lighter word token when the wash sits on a
            // card rather than the page, which is where Delete lives. This
            // one clears 4.5:1 on both, without changing the destructive fill.
            Theme::Dark => hex(0xd97b61),
            Theme::Light => hex(0x8f2f1c),
        }
    }

    /// Accent text for status pills ("created").
    pub fn accent_text_pill(self) -> Hsla {
        match self.theme {
            Theme::Dark => self.accent,
            Theme::Light => hex(0x7d430f),
        }
    }

    /// Muted text for status pills ("exited").
    pub fn muted_text_pill(self) -> Hsla {
        match self.theme {
            // A touch lighter than `muted`: at the raw value this pill reads
            // 4.42:1 on its own wash in the dark theme, just under the floor.
            Theme::Dark => hex(0x9f9686),
            Theme::Light => hex(0x5a5245),
        }
    }

    /// The running card's label. Its graph fill is lighter than the warning
    /// tone, so the dark theme keeps that value; on paper the label needs the
    /// readable amber.
    pub fn running_text(self) -> Hsla {
        match self.theme {
            Theme::Dark => hex(0xd9a441),
            Theme::Light => self.warning_text(),
        }
    }

    /// The one token for the volumes store, shared by bars and legends.
    pub fn store_volume(self) -> Hsla {
        self.warning_fill
    }

    // -- washes ---------------------------------------------------------------
    //
    // These are solid colors, mixed over the surface they are normally painted
    // on. A view never stacks one wash on another, so a pill or a row reads the
    // same wherever it is placed, and its contrast is a property of the token.

    /// The row that is open in the inspector.
    pub fn selected_row(self) -> Hsla {
        washed(self.background, self.accent, wash::SELECTED)
    }

    /// Every other row of a table.
    pub fn alt_row(self) -> Hsla {
        washed(self.background, self.surface_hover, wash::ZEBRA)
    }

    /// The background of a status pill in `color`. Mixed as the pill were on a
    /// plain row, so it reads the same on a striped or selected one.
    pub fn pill_wash(self, color: Hsla) -> Hsla {
        washed(self.surface, color, wash::PILL)
    }

    /// The background of a chip or tab that is on, on the page.
    pub fn accent_wash(self) -> Hsla {
        washed(self.background, self.accent, wash::ACCENT)
    }

    /// The background of a chip or tab that is on, over `over`.
    pub fn accent_wash_on(self, over: Hsla) -> Hsla {
        washed(over, self.accent, wash::ACCENT)
    }

    /// The active engine's card.
    pub fn active_wash(self) -> Hsla {
        washed(self.background, self.accent, wash::ACTIVE)
    }

    /// The background behind destructive text. Mixed over a card, where
    /// Delete lives.
    pub fn danger_wash(self) -> Hsla {
        washed(self.surface, self.danger_text(), wash::DANGER)
    }

    /// The veil behind a sheet. Translucent by nature: it covers the page.
    pub fn backdrop(self) -> Hsla {
        self.text.alpha(if self.is_dark() {
            wash::BACKDROP_DARK
        } else {
            wash::BACKDROP_LIGHT
        })
    }

    /// Selected text in a field, over the field's own text.
    pub fn text_selection(self) -> Hsla {
        self.accent.alpha(wash::SELECTION)
    }

    /// A graph well's fill, inside a card.
    pub fn well(self) -> Hsla {
        washed(self.surface, self.track, wash::WELL)
    }

    /// A graph well's hairline.
    pub fn well_border(self) -> Hsla {
        washed(self.surface, self.track, wash::WELL_BORDER)
    }

    /// "Elsewhere" in the host bar: what is neither containers nor engine.
    pub fn elsewhere(self) -> Hsla {
        washed(self.track, self.muted, wash::ELSEWHERE)
    }

    // -- text -----------------------------------------------------------------

    /// The quietest text that is still text.
    pub fn faint(self) -> Hsla {
        self.muted.alpha(wash::FAINT)
    }

    /// The tint behind a rail icon, or a card's chip.
    pub fn tint_wash(self, tint: Hsla) -> Hsla {
        tint.alpha(wash::PILL)
    }

    // -- controls ---------------------------------------------------------------

    /// What a filled control is filled with while the pointer is over it.
    ///
    /// The fill moves toward the text colour, which is what a person expects
    /// and which costs a little contrast. So it is only for fills whose text
    /// has contrast to spare; a control that is an ink on its own wash shows
    /// [`Tokens::hover_outline`] instead.
    pub fn hovered(self, fill: Hsla) -> Hsla {
        mix(fill, self.text, wash::HOVER)
    }

    /// What it is filled with while it is pressed.
    pub fn pressed(self, fill: Hsla) -> Hsla {
        mix(fill, self.text, wash::PRESSED)
    }

    /// The hairline a wash-filled control shows while the pointer is over it.
    /// Its fill doesn't change, so its text keeps exactly the contrast it had.
    pub fn hover_outline(self, ink: Hsla) -> Hsla {
        ink.alpha(wash::OUTLINE_HOVER)
    }

    /// The ring around the quiet control that has keyboard focus: one drawn
    /// in the text colour, so it shows against the page and against a
    /// secondary fill.
    pub fn focus_ring(self) -> Hsla {
        self.text
    }

    /// The ring around a filled control that has keyboard focus. It is the
    /// text colour of that fill: in the dark theme the ordinary text colour is
    /// as light as the amber and would vanish against it.
    pub fn focus_ring_on_fill(self) -> Hsla {
        self.accent_text
    }

    /// Nothing at all: the hairline every control keeps, so that drawing the
    /// focus ring in its place never moves the control by a pixel.
    pub fn invisible(self) -> Hsla {
        self.text.alpha(0.0)
    }

    /// How faint a control is while it can't be used.
    pub fn disabled_opacity(self) -> f32 {
        wash::DISABLED
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
    match tokens.theme {
        Theme::Dark => match kind {
            TintKind::Overview | TintKind::Containers => tokens.accent,
            TintKind::Images | TintKind::Volumes => hex(0xd9a441),
            TintKind::Networks | TintKind::Events => hex(0x6fb0a8),
            TintKind::Stacks => hex(0xbd80e4),
            TintKind::Disk | TintKind::Runtimes => hex(0x8a9bb0),
            TintKind::Settings => tokens.muted,
        },
        Theme::Light => match kind {
            TintKind::Overview | TintKind::Containers => tokens.accent,
            TintKind::Images | TintKind::Volumes => hex(0x9a6e1b),
            TintKind::Networks | TintKind::Events => hex(0x2f8f9d),
            TintKind::Stacks => hex(0x7a4ea3),
            TintKind::Disk | TintKind::Runtimes => hex(0x3f6079),
            TintKind::Settings => tokens.muted,
        },
    }
}

/// Parse an opaque sRGB integer into the theme's color type.
pub fn hex(value: u32) -> Hsla {
    rgb(value).into()
}

/// Blend two opaque colors in sRGB. `t` is the weight of `b`. The result is
/// what a wash over a solid background amounts to, to the eye; the contrast
/// tests are computed on washes the same way.
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

/// What a wash of `color` looks like over `over`: the colour a contrast ratio
/// has to be measured against.
pub fn washed(over: Hsla, color: Hsla, strength: f32) -> Hsla {
    mix(over, color, strength)
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
mod tests;
