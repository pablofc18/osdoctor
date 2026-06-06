//! osdoctor — Linux desktop health checks for Arch/Hyprland systems.
#![allow(clippy::uninlined_format_args)]

pub mod checks;
pub mod cli;
pub mod model;
pub mod output;
pub mod util;

// Serializes tests that mutate process environment variables (HOME, etc.).
// First used by util::fs and checks::desktop::hypr tests in later tasks.
#[cfg(test)]
#[allow(dead_code)]
pub(crate) static ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());
