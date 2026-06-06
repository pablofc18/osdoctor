use osdoctor::model::{CheckResult, CheckStatus};
use osdoctor::output::json::render_json;

fn sample_results() -> Vec<CheckResult> {
    use CheckStatus::*;
    vec![
        CheckResult::new(
            "kernel-version",
            "System",
            Ok,
            "Kernel detected: 7.0.9-arch2-1",
            "Linux 7.0.9-arch2-1 #1 SMP PREEMPT_DYNAMIC Fri, 22 May 2026 19:25:09 +0000",
        ),
        CheckResult::new(
            "root-disk-space",
            "System",
            Ok,
            "Root filesystem has 403 GB free",
            "",
        ),
        CheckResult::new("tmp-writable", "System", Ok, "/tmp is writable", ""),
        CheckResult::new(
            "memory-info",
            "System",
            Ok,
            "Memory: 26792 MB available of 31956 MB",
            "",
        ),
        CheckResult::new(
            "failed-system-services",
            "Services",
            Ok,
            "No failed system services",
            "",
        ),
        CheckResult::new(
            "failed-user-services",
            "Services",
            Ok,
            "No failed user services",
            "",
        ),
        CheckResult::new(
            "pacman-lock",
            "Packages",
            Ok,
            "pacman database is unlocked",
            "",
        ),
        CheckResult::new("pacman-log", "Packages", Ok, "pacman log exists", ""),
        CheckResult::new("orphan-packages", "Packages", Ok, "No orphan packages", ""),
        CheckResult::new(
            "wayland-session",
            "Desktop",
            Ok,
            "Wayland session detected",
            "",
        ),
        CheckResult::new(
            "hyprland-socket",
            "Desktop",
            Ok,
            "Hyprland socket detected",
            "",
        ),
        CheckResult::new(
            "hyprland-binds",
            "Desktop",
            Warn,
            "Hyprland bind references missing command: ghostty",
            "",
        ),
        CheckResult::new(
            "waybar-scripts",
            "Desktop",
            Fail,
            "Waybar config references missing script: ~/.config/waybar/scripts/battery",
            "",
        ),
    ]
}

#[test]
fn render_matches_sample_file() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/sample-output.json");
    let expected = std::fs::read_to_string(path).expect("read sample-output.json");
    assert_eq!(render_json(&sample_results()), expected);
}
