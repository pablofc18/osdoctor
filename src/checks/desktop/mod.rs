//! Desktop checks (replaces check_desktop.c): Wayland session, Hyprland socket,
//! Hyprland binds (hypr), and Waybar scripts (waybar).

pub(crate) mod hypr;
pub(crate) mod waybar;

use crate::model::{CheckResult, CheckStatus};
use crate::util::fs::file_exists;

const GROUP: &str = "Desktop";

fn check_wayland(results: &mut Vec<CheckResult>) {
    match std::env::var("XDG_SESSION_TYPE") {
        Ok(t) if t == "wayland" => {
            results.push(CheckResult::new(
                "wayland-session",
                GROUP,
                CheckStatus::Ok,
                "Wayland session detected",
                "",
            ));
        }
        Ok(t) if !t.is_empty() => {
            results.push(CheckResult::new(
                "wayland-session",
                GROUP,
                CheckStatus::Warn,
                format!("Session type is '{t}' (not Wayland)"),
                "",
            ));
        }
        _ => {
            results.push(CheckResult::new(
                "wayland-session",
                GROUP,
                CheckStatus::Warn,
                "XDG_SESSION_TYPE not set",
                "",
            ));
        }
    }
}

fn check_hyprland_socket(results: &mut Vec<CheckResult>) {
    let sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").unwrap_or_default();
    if sig.is_empty() {
        results.push(CheckResult::new(
            "hyprland-socket",
            GROUP,
            CheckStatus::Skip,
            "Hyprland not running (no instance signature)",
            "",
        ));
        return;
    }
    let xdg = std::env::var("XDG_RUNTIME_DIR").unwrap_or_default();
    if xdg.is_empty() {
        results.push(CheckResult::new(
            "hyprland-socket",
            GROUP,
            CheckStatus::Warn,
            "XDG_RUNTIME_DIR not set",
            "",
        ));
        return;
    }
    let path = format!("{xdg}/hypr/{sig}/.socket.sock");
    if file_exists(&path) {
        results.push(CheckResult::new(
            "hyprland-socket",
            GROUP,
            CheckStatus::Ok,
            "Hyprland socket detected",
            "",
        ));
    } else {
        results.push(CheckResult::new(
            "hyprland-socket",
            GROUP,
            CheckStatus::Warn,
            "Hyprland socket not found",
            path,
        ));
    }
}

pub fn run(results: &mut Vec<CheckResult>) {
    check_wayland(results);
    check_hyprland_socket(results);
    hypr::check_binds(results);
    waybar::check_scripts(results);
}
