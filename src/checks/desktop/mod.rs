//! Desktop checks: Wayland session, Hyprland socket, Hyprland binds (hypr),
//! and Waybar scripts (waybar).

pub(crate) mod hypr;
pub(crate) mod waybar;

use crate::config::Config;
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
            results.push(CheckResult::problem(
                "wayland-session",
                GROUP,
                CheckStatus::Warn,
                format!("Session type is '{t}' (not Wayland)"),
                "",
                "XDG_SESSION_TYPE says this is not a Wayland session, so Hyprland and \
Wayland-only tools do not run in it. This is expected on a text console, over SSH or in an X11 \
session; otherwise check how your display manager or login script starts the session.",
            ));
        }
        _ => {
            results.push(CheckResult::problem(
                "wayland-session",
                GROUP,
                CheckStatus::Warn,
                "XDG_SESSION_TYPE not set",
                "",
                "The session type is unknown because XDG_SESSION_TYPE is not set. It is \
normally set at login by systemd-logind or the display manager, and is often missing in cron \
jobs, containers, or when the compositor is started without a login session.",
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
        results.push(CheckResult::problem(
            "hyprland-socket",
            GROUP,
            CheckStatus::Warn,
            "XDG_RUNTIME_DIR not set",
            "",
            "Hyprland seems to be running, but XDG_RUNTIME_DIR is not set, so its socket \
cannot be located and tools such as hyprctl cannot reach it. The variable is normally set at \
login by systemd-logind and is often lost under sudo or in a stripped environment.",
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
        results.push(CheckResult::problem(
            "hyprland-socket",
            GROUP,
            CheckStatus::Warn,
            "Hyprland socket not found",
            path,
            "HYPRLAND_INSTANCE_SIGNATURE is set, but its socket does not exist under \
XDG_RUNTIME_DIR, so hyprctl and other tools cannot talk to Hyprland. Hyprland may have exited \
and left the variable behind, or this shell belongs to another user or Hyprland instance.",
        ));
    }
}

pub fn run(results: &mut Vec<CheckResult>, cfg: &Config) {
    if cfg.enabled("wayland-session") {
        check_wayland(results);
    }
    if cfg.enabled("hyprland-socket") {
        check_hyprland_socket(results);
    }
    if cfg.enabled("hyprland-binds") {
        hypr::check_binds(results);
    }
    if cfg.enabled("waybar-scripts") {
        waybar::check_scripts(results);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run `f` with the given environment variables set (None = unset),
    /// restoring the previous values afterwards.
    fn with_env(
        vars: &[(&str, Option<&str>)],
        f: impl FnOnce() -> Vec<CheckResult>,
    ) -> Vec<CheckResult> {
        let _g = crate::ENV_MUTEX.lock().unwrap();
        let saved: Vec<_> = vars
            .iter()
            .map(|(k, _)| (*k, std::env::var_os(k)))
            .collect();
        for (k, v) in vars {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
        let out = f();
        for (k, v) in saved {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
        out
    }

    fn wayland(value: Option<&str>) -> CheckResult {
        with_env(&[("XDG_SESSION_TYPE", value)], || {
            let mut r = Vec::new();
            check_wayland(&mut r);
            r
        })
        .remove(0)
    }

    fn socket(runtime_dir: Option<&str>) -> CheckResult {
        with_env(
            &[
                ("HYPRLAND_INSTANCE_SIGNATURE", Some("osd_test_sig")),
                ("XDG_RUNTIME_DIR", runtime_dir),
            ],
            || {
                let mut r = Vec::new();
                check_hyprland_socket(&mut r);
                r
            },
        )
        .remove(0)
    }

    #[test]
    fn wayland_situations_are_explained() {
        assert_eq!(wayland(Some("wayland")).explain, "");
        let x11 = wayland(Some("x11"));
        let unset = wayland(None);
        assert_eq!(
            (x11.status, unset.status),
            (CheckStatus::Warn, CheckStatus::Warn)
        );
        assert!(!x11.explain.is_empty() && !unset.explain.is_empty());
        assert_ne!(x11.explain, unset.explain);
    }

    #[test]
    fn socket_situations_are_explained() {
        let no_runtime = socket(None);
        let missing = socket(Some("/nonexistent/osd_runtime"));
        assert_eq!(no_runtime.message, "XDG_RUNTIME_DIR not set");
        assert_eq!(missing.message, "Hyprland socket not found");
        assert_eq!(
            (no_runtime.status, missing.status),
            (CheckStatus::Warn, CheckStatus::Warn)
        );
        assert_ne!(no_runtime.explain, missing.explain);
        assert!(!missing.explain.is_empty());
    }
}
