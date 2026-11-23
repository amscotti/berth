//! Tests that read the app's own source. The rules a review would check with
//! a grep fail a build here instead: a view that draws its own button, a text
//! size off the scale, or a wash mixed by hand.

use std::path::{Path, PathBuf};

/// Every Rust file of the app's library: its path and its source, in order.
fn sources() -> Vec<(PathBuf, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    let mut pending = vec![root];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).expect("the app's source is where it was built") {
            let path = entry.expect("a readable entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path
                .file_name()
                .is_some_and(|name| name == "contract_tests.rs")
            {
                // This file names what it forbids, in order to look for it.
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let source = std::fs::read_to_string(&path).expect("source is UTF-8");
                files.push((path, source));
            }
        }
    }
    files.sort();
    files
}

/// Where a line is, for a failure message: relative to the crate, 1-based.
fn place(path: &Path, line: usize) -> String {
    let shown = path
        .strip_prefix(Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap_or(path);
    format!("{}:{}", shown.display(), line + 1)
}

/// The text from `at`, which is just inside an opening parenthesis, up to
/// the parenthesis that closes it.
fn argument(source: &str, at: usize) -> &str {
    let mut depth = 1usize;
    for (offset, character) in source[at..].char_indices() {
        match character {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return source[at..at + offset].trim();
                }
            }
            _ => {}
        }
    }
    &source[at..]
}

/// Whether `text` writes a number with a fraction in it: `9.5`, not `self.x`.
fn has_decimal(text: &str) -> bool {
    text.bytes()
        .zip(text.bytes().skip(1))
        .any(|(first, second)| first.is_ascii_digit() && second == b'.')
}

/// A button is the kit's, or it isn't a button. No view rolls its own.
#[test]
fn no_view_draws_its_own_button() {
    for (path, source) in sources() {
        let in_kit = path.to_string_lossy().contains("src/ui/kit/");
        for (line, text) in source.lines().enumerate() {
            let own = text.contains("Role::Button") || text.contains("Role::CheckBox");
            assert!(
                !own || in_kit,
                "{} draws a button outside the kit",
                place(&path, line)
            );
        }
    }
}

/// Text is one of the scale's steps and nothing else.
#[test]
fn text_comes_from_the_scale() {
    for (path, source) in sources() {
        let mut from = 0;
        while let Some(at) = source[from..].find(".text_size(px(") {
            let start = from + at + ".text_size(px(".len();
            let size = argument(&source, start);
            let on_scale = size.starts_with("type_scale::")
                || size.starts_with("typography::")
                // A choice between two steps is still on the scale, as long
                // as neither is a number.
                || (size.starts_with("if ") && !has_decimal(size));
            assert!(
                on_scale,
                "{} sets text to {}, which isn't a step of the scale",
                place(&path, source[..start].lines().count() - 1),
                size.replace('\n', " ")
            );
            from = start;
        }
    }
}

/// A wash is a method on the tokens. A view never mixes its own.
#[test]
fn every_wash_is_a_token() {
    for (path, source) in sources() {
        for (line, text) in source.lines().enumerate() {
            assert!(
                !text.contains(".alpha(0."),
                "{} mixes its own wash",
                place(&path, line)
            );
        }
    }
}

/// The monospace family is chosen once, for the platform, in `berth-ui`.
#[test]
fn monospace_is_named_once() {
    for (path, source) in sources() {
        for (line, text) in source.lines().enumerate() {
            assert!(
                !text.contains("Menlo"),
                "{} names a font the platform may not have",
                place(&path, line)
            );
        }
    }
}

/// Dead code is deleted, not forgiven. The one exception is the tray's stub
/// for the platforms that have no tray.
#[test]
fn dead_code_is_deleted_not_forgiven() {
    for (path, source) in sources() {
        for (line, text) in source.lines().enumerate() {
            if text.contains("allow(dead_code)") {
                assert!(
                    path.ends_with("tray.rs"),
                    "{} forgives dead code",
                    place(&path, line)
                );
            }
        }
    }
}

/// The page and the inspector redraw apart (`subviews`), which is only
/// honest while each draws from its own state: the page never shows the
/// inspector's, and the inspector never shows the tables'. A scoped redraw
/// (`redraw_page`, `redraw_inspector`) would leave what broke this stale.
#[test]
fn each_corner_draws_from_its_own_state() {
    const PAGE: [&str; 6] = [
        "src/ui/table.rs",
        "src/ui/table_cells.rs",
        "src/ui/rail.rs",
        "src/ui/row_actions.rs",
        "src/ui/toast.rs",
        "src/ui/views/",
    ];
    const PANE: [&str; 4] = [
        "src/ui/inspector.rs",
        "src/ui/inspect_tabs.rs",
        "src/ui/inspect_objects.rs",
        "src/ui/inspect_shared.rs",
    ];
    // What the tables are searched, sorted and selected by.
    const TABLES: [&str; 7] = [
        ".lists[",
        ".search(",
        ".query(",
        ".sort_for(",
        ".visible_rows(",
        ".bulk_",
        ".view.filter",
    ];
    for (path, source) in sources() {
        let shown = place(&path, 0);
        let on_page = PAGE.iter().any(|file| shown.starts_with(file));
        let in_pane = PANE.iter().any(|file| shown.starts_with(file));
        for (line, text) in source.lines().enumerate() {
            if on_page {
                assert!(
                    !text.contains(".inspector."),
                    "{} draws the inspector's state on the page",
                    place(&path, line)
                );
            }
            if in_pane {
                for read in TABLES {
                    assert!(
                        !text.contains(read),
                        "{} draws the tables' state in the inspector: {read}",
                        place(&path, line)
                    );
                }
            }
        }
    }
}

/// An icon that nothing draws is a file in the bundle for nothing.
#[test]
fn every_icon_is_drawn() {
    // The repository's `assets/icons`, two levels above the crate.
    let icons = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("assets")
        .join("icons");
    let drawn: String = sources().into_iter().map(|(_, source)| source).collect();
    for entry in std::fs::read_dir(&icons).expect("the icons are in the repository") {
        let path = entry.expect("a readable entry").path();
        let name = path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        assert!(
            drawn.contains(&format!("\"{name}\"")),
            "{} is bundled, and nothing names it",
            path.display()
        );
    }
}
