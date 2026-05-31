//! Lifetime core: data models, sync protocol, crypto, local storage.
//!
//! Compiled native on desktop and embedded in mobile clients via UniFFI (iOS)
//! and JNI (Android).

pub mod aggregate;
pub mod lww;
pub mod model;
pub mod storage;
pub mod theme;
