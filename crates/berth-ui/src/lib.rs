//! berth design tokens, layout geometry and type scale.
//!
//! This crate deliberately paints nothing. It owns the numbers the shell and
//! the views must agree on, and the contrast tests that keep text readable in
//! both themes.

pub mod layout;
pub mod theme;
pub mod typography;

pub use layout::{Density, Widths, container_columns, density, for_window};
pub use theme::{TintKind, Tokens, contrast_ratio, dark, for_appearance, light, tint};
pub use typography::*;
