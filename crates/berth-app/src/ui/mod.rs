//! Views and view fragments. The shell owns state; these modules own drawing.

pub mod graph;
pub mod icon;
pub mod input;
pub mod inspect_objects;
pub mod inspect_shared;
pub mod inspect_tabs;
pub mod inspector;
pub mod rail;
pub mod sheets;
pub mod table;
pub mod table_cells;
pub mod toast;
pub mod views;

use std::sync::Arc;

use gpui::FontFeatures;

/// Tabular figures for every number, clock and byte count.
pub(crate) fn tabular() -> FontFeatures {
    FontFeatures(Arc::new(vec![("tnum".to_owned(), 1)]))
}
