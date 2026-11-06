//! Platform-independent core of *That's it*.
//!
//! Everything that can be tested without AppKit lives here; the `thatsit-app` crate is a
//! thin native shell around it.

pub mod conversion;
pub mod engines;
pub mod geometry;
pub mod menu;
pub mod output;
pub mod pipeline;
pub mod placeholder;
pub mod tools;

/// Re-exported so engine implementations outside the core use the same `toml` version.
pub use toml;
