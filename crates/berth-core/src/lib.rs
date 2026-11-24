//! Pure berth logic: formatting, sample history, image references,
//! single-line text editing, and form validation. No GPUI, no sockets, no
//! filesystem.

pub mod format;
pub mod history;
pub mod image_ref;
pub mod text;
pub mod validation;

pub use format::{
    ago, bytes, bytes_signed, clock, count, duration, measured, now, percent, started,
};
pub use history::{History, Which};
pub use text::TextBuffer;
