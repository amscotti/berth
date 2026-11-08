//! Engine-facing types and transport. This crate never depends on GPUI.
//!
//! The GitHub Docker Engine API is the common surface Docker, Podman, Colima
//! and Lima expose. Types default every field so a newer or older engine that
//! adds or omits one does not fail to parse.

pub mod client;
pub mod detail;
pub mod df;
pub mod discovery;
pub mod dto;
pub mod events;
pub mod guards;
pub mod logs;
pub mod network_counts;
pub mod projects;
pub mod prune;
pub mod stats;
pub mod transport;

/// The largest response body berth will buffer for a list call.
pub const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;

/// The default API version used before `/version` has answered.
pub const DEFAULT_API_VERSION: &str = "v1.43";
