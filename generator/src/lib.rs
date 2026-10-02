//! Galdr initramfs generator.
//!
//! The binary is a thin CLI over this library so the pieces that decide what
//! ends up in the image (config parsing, CPIO encoding, module resolution) are
//! unit-testable.

pub mod compress;
pub mod config;
pub mod hooks;
pub mod image;
