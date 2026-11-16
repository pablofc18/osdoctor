//! Check group runners and the full-scan orchestration.

pub mod desktop;
pub mod packages;
#[cfg(feature = "libalpm")]
mod packages_alpm;
pub mod services;
#[cfg(feature = "libsystemd")]
mod services_sdbus;
pub mod system;

use crate::config::Config;
use crate::model::CheckResult;

/// Every check ID, in report order.
pub const CHECK_IDS: [&str; 13] = [
    "kernel-version",
    "root-disk-space",
    "tmp-writable",
    "memory-info",
    "failed-system-services",
    "failed-user-services",
    "pacman-lock",
    "pacman-log",
    "orphan-packages",
    "wayland-session",
    "hyprland-socket",
    "hyprland-binds",
    "waybar-scripts",
];

pub fn run_system(results: &mut Vec<CheckResult>, cfg: &Config) {
    system::run(results, cfg);
}

pub fn run_services(results: &mut Vec<CheckResult>, cfg: &Config) {
    services::run(results, cfg);
}

pub fn run_packages(results: &mut Vec<CheckResult>, cfg: &Config) {
    packages::run(results, cfg);
}

pub fn run_desktop(results: &mut Vec<CheckResult>, cfg: &Config) {
    desktop::run(results, cfg);
}

pub fn run_all(results: &mut Vec<CheckResult>, cfg: &Config) {
    system::run(results, cfg);
    services::run(results, cfg);
    packages::run(results, cfg);
    desktop::run(results, cfg);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Mode;

    #[test]
    fn check_ids_are_unique() {
        let set: std::collections::BTreeSet<&str> = CHECK_IDS.iter().copied().collect();
        assert_eq!(set.len(), 13);
    }

    // Orchestration contract: all four groups run, contiguously, in
    // System -> Services -> Packages -> Desktop order. The fixed-shape groups
    // always emit a known number of rows (System=4, Services=2, Packages=3);
    // the Desktop group emits the two env checks plus host-dependent
    // bind/script rows (>=1 each), so its total is host-dependent and only
    // bounded below. (The plan's original exact "== 13" assertion only holds on
    // a host with no Hyprland/Waybar config; this machine has both, so we assert
    // the host-independent contract instead.)
    #[test]
    fn run_all_emits_four_groups_in_order() {
        // run_all's desktop checks read HOME / XDG_* via the environment; hold
        // the env mutex so this can't race the tests that set HOME (concurrent
        // set_var/getenv is undefined behaviour).
        let _env = crate::ENV_MUTEX.lock().unwrap();
        let mut results = Vec::new();
        run_all(&mut results, &Config::default());

        let groups: Vec<&str> = results.iter().map(|r| r.group).collect();
        let mut order = Vec::new();
        for g in &groups {
            if order.last() != Some(g) {
                order.push(*g);
            }
        }
        assert_eq!(order, ["System", "Services", "Packages", "Desktop"]);

        let count = |name: &str| groups.iter().filter(|&&g| g == name).count();
        assert_eq!(count("System"), 4);
        assert_eq!(count("Services"), 2);
        assert_eq!(count("Packages"), 3);
        assert!(
            count("Desktop") >= 4,
            "Desktop group had {} rows",
            count("Desktop")
        );
    }

    #[test]
    fn every_emitted_id_is_known() {
        let _env = crate::ENV_MUTEX.lock().unwrap();
        let mut results = Vec::new();
        run_all(&mut results, &Config::default());
        assert!(!results.is_empty());
        for r in &results {
            assert!(CHECK_IDS.contains(&r.id), "unknown check id {}", r.id);
        }
    }

    fn cfg_off(except: &[&str], only_off: &[&str]) -> Config {
        let mut cfg = Config::default();
        for id in CHECK_IDS {
            let off = if only_off.is_empty() {
                !except.contains(&id)
            } else {
                only_off.contains(&id)
            };
            if off {
                cfg.checks.insert(id.to_string(), Mode::Off);
            }
        }
        cfg
    }

    #[test]
    fn off_checks_do_not_run_or_report() {
        let _env = crate::ENV_MUTEX.lock().unwrap();
        let cfg = cfg_off(&["tmp-writable"], &[]);
        let mut results = Vec::new();
        run_all(&mut results, &cfg);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "tmp-writable");
    }

    #[test]
    fn all_checks_off_in_group_removes_group() {
        let _env = crate::ENV_MUTEX.lock().unwrap();
        let cfg = cfg_off(
            &[],
            &[
                "kernel-version",
                "root-disk-space",
                "tmp-writable",
                "memory-info",
            ],
        );
        let mut results = Vec::new();
        run_all(&mut results, &cfg);
        assert!(results.iter().all(|r| r.group != "System"));
        let count = |name: &str| results.iter().filter(|r| r.group == name).count();
        assert_eq!(count("Services"), 2);
        assert_eq!(count("Packages"), 3);
    }
}
