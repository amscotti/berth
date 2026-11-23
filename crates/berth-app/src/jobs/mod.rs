//! Background work. Jobs own their socket strings and return owned data; they
//! never touch the view.

pub mod detail;
pub mod engine_ops;
pub mod mutation;
pub mod refresh;
