//! Cross-platform tracking primitives.
//!
//! [`Sampler`] implementations produce observation samples from OS-specific
//! APIs. Each desktop and mobile client selects the appropriate sampler at
//! compile time via `target_os`.

pub mod sampler;

#[cfg(target_os = "macos")]
pub mod macos;

pub use sampler::Sampler;
