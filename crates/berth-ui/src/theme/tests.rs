use super::*;

fn themes() -> [(&'static str, Tokens); 2] {
    [("light", light()), ("dark", dark())]
}

/// Text has to stay readable on what it is drawn over.
fn readable(name: &str, what: &str, ink: Hsla, paper: Hsla, floor: f32) {
    let ratio = contrast_ratio(ink, paper);
    assert!(ratio >= floor, "{name} {what} contrast was {ratio:.2}:1");
}

/// Somewhere a wash or an ink is drawn, as a function of the theme.
type Ground = fn(Tokens) -> Hsla;

/// The rows a pill or a label can be drawn on.
const ROWS: [(&str, Ground); 4] = [
    ("plain row", |tokens| tokens.surface),
    ("alt row", Tokens::alt_row),
    ("hovered row", |tokens| tokens.surface_hover),
    ("selected row", Tokens::selected_row),
];

#[test]
fn body_text_clears_its_floor() {
    for (name, tokens) in themes() {
        readable(name, "body", tokens.text, tokens.background, 7.0);
        readable(name, "body on surface", tokens.text, tokens.surface, 7.0);
        readable(name, "muted", tokens.muted, tokens.background, 4.5);
        readable(name, "muted on surface", tokens.muted, tokens.surface, 4.5);
        readable(
            name,
            "faint on surface",
            tokens.faint(),
            tokens.surface,
            3.0,
        );
    }
}

#[test]
fn filled_buttons_keep_their_text_readable_in_every_state() {
    for (name, tokens) in themes() {
        // The palette names the primary button's hover itself; the other
        // fills move toward the text colour.
        for (what, fill, ink, hovered) in [
            (
                "primary",
                tokens.accent,
                tokens.accent_text,
                tokens.accent_hover,
            ),
            (
                "destructive",
                tokens.danger,
                tokens.accent_text,
                tokens.hovered(tokens.danger),
            ),
            (
                "secondary",
                tokens.surface_hover,
                tokens.text,
                tokens.hovered(tokens.surface_hover),
            ),
        ] {
            let what = format!("{what} button");
            readable(name, &what, ink, fill, 4.5);
            readable(name, &format!("{what}, hovered"), ink, hovered, 4.5);
            readable(
                name,
                &format!("{what}, pressed"),
                ink,
                tokens.pressed(fill),
                4.5,
            );
        }
    }
}

#[test]
fn a_wash_control_shows_hover_without_spending_contrast() {
    for (name, tokens) in themes() {
        // Delete reads the same on a card and on every kind of row, because
        // its wash is a solid color and doesn't pick up what is under it.
        readable(
            name,
            "destructive wash",
            tokens.danger_text(),
            tokens.danger_wash(),
            4.5,
        );
        // A selected chip on the page, and a selected tab on a card.
        readable(
            name,
            "selected chip",
            tokens.accent_text_pill(),
            tokens.accent_wash(),
            4.5,
        );
        readable(
            name,
            "selected tab",
            tokens.accent_text_pill(),
            tokens.accent_wash_on(tokens.surface),
            4.5,
        );
        // Hover is an outline, so nothing about the fill changes.
        assert_eq!(
            tokens.hover_outline(tokens.danger_text()).a,
            wash::OUTLINE_HOVER
        );
    }
}

#[test]
fn warning_and_success_text_clear_their_pill_wash() {
    for (name, tokens) in themes() {
        let warn = tokens.warning_text();
        let success = tokens.success_text();
        let warn_ratio = contrast_ratio(warn, tokens.pill_wash(warn));
        let success_ratio = contrast_ratio(success, tokens.pill_wash(success));
        assert!(
            warn_ratio >= 4.5,
            "{name} warning contrast was {warn_ratio:.2}:1"
        );
        assert!(
            success_ratio >= 4.5,
            "{name} success contrast was {success_ratio:.2}:1"
        );
        readable(
            name,
            "accent pill",
            tokens.accent_text_pill(),
            tokens.pill_wash(tokens.accent),
            4.5,
        );
        readable(
            name,
            "muted pill",
            tokens.muted_text_pill(),
            tokens.pill_wash(tokens.muted_text_pill()),
            4.5,
        );
    }
}

#[test]
fn danger_text_clears_error_washes() {
    for (name, tokens) in themes() {
        // Delete lives on cards, not on the page.
        readable(
            name,
            "danger on a card",
            tokens.danger_text(),
            tokens.danger_wash(),
            4.5,
        );
        readable(
            name,
            "danger as text",
            tokens.danger_text(),
            tokens.surface,
            4.5,
        );
    }
}

#[test]
fn every_status_pill_is_readable_on_every_row() {
    for (name, tokens) in themes() {
        for (what, row) in ROWS {
            let row = row(tokens);
            readable(name, what, tokens.text, row, 4.5);
            readable(name, what, tokens.muted, row, 4.5);
            for (pill, ink, wash) in [
                (
                    "running",
                    tokens.success_text(),
                    tokens.pill_wash(tokens.success_text()),
                ),
                (
                    "exited",
                    tokens.muted_text_pill(),
                    tokens.pill_wash(tokens.muted_text_pill()),
                ),
                (
                    "port",
                    tokens.warning_text(),
                    tokens.pill_wash(tokens.warning_fill),
                ),
                ("delete", tokens.danger_text(), tokens.danger_wash()),
            ] {
                // A pill's background is a solid color of its own, so it
                // reads the same whichever row it is on.
                let ink_on_wash = contrast_ratio(ink, wash);
                let ink_as_if_on_row = contrast_ratio(ink, row);
                assert!(
                    ink_on_wash >= 4.5,
                    "{name} {what}, {pill} pill: {ink_on_wash:.2}:1"
                );
                assert!(
                    ink_as_if_on_row >= 3.0 || pill == "port",
                    "{name} {what}: the {pill} ink would vanish if drawn raw on this row"
                );
            }
        }
    }
}

#[test]
fn a_chip_is_readable_on_and_off() {
    for (name, tokens) in themes() {
        readable(name, "chip off", tokens.text, tokens.surface, 4.5);
        readable(
            name,
            "chip off, hovered",
            tokens.text,
            tokens.surface_hover,
            4.5,
        );
        // A quiet control's label strengthens on hover; it isn't muted then.
        readable(name, "quiet label", tokens.muted, tokens.surface, 4.5);
    }
}

#[test]
fn the_theme_is_known_not_guessed_from_a_color() {
    assert!(dark().is_dark());
    assert!(!light().is_dark());
    assert_eq!(
        Theme::for_appearance(WindowAppearance::VibrantDark),
        Theme::Dark
    );
    assert_eq!(Theme::for_appearance(WindowAppearance::Light), Theme::Light);
    assert_ne!(light().danger_text(), dark().danger_text());
    // The veil behind a sheet is heavier in the dark, as the spec has it.
    assert_eq!(light().backdrop().a, wash::BACKDROP_LIGHT);
    assert_eq!(dark().backdrop().a, wash::BACKDROP_DARK);
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
        assert_ne!(tokens.alt_row(), tokens.surface, "{name} stripe invisible");
        assert_ne!(
            tokens.alt_row(),
            tokens.surface_hover,
            "{name} stripe is a hover"
        );
        // Each focus ring shows against what its control is filled with,
        // and the hairline it replaces is invisible.
        for fill in [tokens.background, tokens.surface, tokens.surface_hover] {
            assert!(
                contrast_ratio(tokens.focus_ring(), fill) >= 3.0,
                "{name} ring on {fill:?}"
            );
        }
        for fill in [tokens.accent, tokens.danger] {
            assert!(
                contrast_ratio(tokens.focus_ring_on_fill(), fill) >= 3.0,
                "{name} ring on a fill {fill:?}"
            );
        }
        assert_eq!(tokens.invisible().a, 0.0);
    }
}
