//! osdoctor — Linux desktop health checks for Arch/Hyprland systems.
#![allow(clippy::uninlined_format_args)]

pub mod checks;
pub mod cli;
pub mod model;
pub mod output;
pub mod util;

// Helpers shared across module unit tests.

/// Serializes tests that mutate process environment variables (HOME, etc.)
/// against each other and against the tests that read them.
#[cfg(test)]
pub(crate) static ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A unique temp directory named with `tag`, the pid, and a nanosecond
/// timestamp. The caller removes it when finished.
#[cfg(test)]
pub(crate) fn unique_dir(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let mut p = std::env::temp_dir();
    p.push(format!("osdoctor_{}_{}_{}", tag, std::process::id(), nanos));
    std::fs::create_dir_all(&p).unwrap();
    p
}
