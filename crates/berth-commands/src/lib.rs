//! berth's action vocabulary, keymaps and menu model.

pub mod app_actions;
pub mod keymap;
pub mod menus;

pub use app_actions::*;
pub use keymap::{Platform, install as install_keymap};
pub use menus::menus;
