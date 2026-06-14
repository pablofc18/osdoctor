//! Check group runners + orchestration (replaces checks.h + run_all_checks).

pub mod desktop;
pub mod packages;
pub mod services;
pub mod system;

use crate::model::CheckResult;

pub fn run_system(results: &mut Vec<CheckResult>) {
    system::run(results);
}

pub fn run_services(results: &mut Vec<CheckResult>) {
    services::run(results);
}

pub fn run_packages(results: &mut Vec<CheckResult>) {
    packages::run(results);
}

pub fn run_desktop(results: &mut Vec<CheckResult>) {
    desktop::run(results);
}

pub fn run_all(results: &mut Vec<CheckResult>) {
    system::run(results);
    services::run(results);
    packages::run(results);
    desktop::run(results);
}

#[cfg(test)]
mod tests {
    use super::*;

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
        run_all(&mut results);

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
}
