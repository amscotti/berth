//! The type scale. One place so a table cell and its header cannot disagree.
//!
//! Seven steps. Text is one of these and nothing else, and a test holds the
//! views to it. The logs keep the monospace size the spec gives them.

/// The quietest text that is still text: "recent", counts beside headings.
pub const META: f32 = 9.5;
/// Table headers, pills, the labels of small controls, inspector labels.
pub const LABEL: f32 = 11.0;
/// Secondary text: page subtitles, captions, the body of a card.
pub const SECONDARY: f32 = 12.5;
/// Table rows, rail items, the labels of controls, inspector values.
pub const BODY: f32 = 13.0;
/// A card or sheet title.
pub const TITLE_SMALL: f32 = 15.0;
/// A page title.
pub const TITLE: f32 = 22.0;
/// The large figure in a metric card.
pub const METRIC: f32 = 26.0;

/// Logs, IDs, socket paths and raw JSON.
pub const MONO: f32 = 11.5;
/// The family monospace text is set in. macOS ships Menlo; Linux doesn't, and
/// asking for it there silently falls back to the proportional default.
pub const MONO_FAMILY: &str = if cfg!(target_os = "macos") {
    "Menlo"
} else {
    "DejaVu Sans Mono"
};

/// Line height for prose.
pub const LEADING: f32 = 1.4;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scale_is_seven_steps_and_a_monospace_size() {
        let steps = [META, LABEL, SECONDARY, BODY, TITLE_SMALL, TITLE, METRIC];
        for pair in steps.windows(2) {
            assert!(pair[0] < pair[1], "{steps:?} must rise");
        }
        assert_eq!(steps.len(), 7);
    }
}
