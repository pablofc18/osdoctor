//! System checks: kernel, root disk space, /tmp writability, memory.
//! Uses libc uname/statvfs and /proc/meminfo; never shells out.

use crate::config::{Config, Thresholds};
use crate::model::{CheckResult, CheckStatus};
use crate::util::fs::path_writable;
use std::ffi::CStr;

const GROUP: &str = "System";
const GIB: u64 = 1024 * 1024 * 1024;

fn uname() -> Option<(String, String, String)> {
    unsafe {
        let mut uts: libc::utsname = std::mem::zeroed();
        if libc::uname(&mut uts) != 0 {
            return None;
        }
        let cstr = |p: *const libc::c_char| CStr::from_ptr(p).to_string_lossy().into_owned();
        Some((
            cstr(uts.sysname.as_ptr()),
            cstr(uts.release.as_ptr()),
            cstr(uts.version.as_ptr()),
        ))
    }
}

fn statvfs_free_bytes(path: &str) -> Option<u64> {
    let c = std::ffi::CString::new(path).ok()?;
    unsafe {
        let mut vfs: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c.as_ptr(), &mut vfs) != 0 {
            return None;
        }
        Some(vfs.f_bavail as u64 * vfs.f_frsize as u64)
    }
}

fn parse_meminfo_line(line: &str, key: &str) -> Option<u64> {
    let rest = line.strip_prefix(key)?.trim_start();
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse::<u64>().ok()
    }
}

/// Disk status for `free_bytes` free on /. A threshold of 0 never triggers.
fn disk_status(free_bytes: u64, t: &Thresholds) -> CheckStatus {
    if free_bytes < t.disk_fail_gb.saturating_mul(GIB) {
        CheckStatus::Fail
    } else if free_bytes < t.disk_warn_gb.saturating_mul(GIB) {
        CheckStatus::Warn
    } else {
        CheckStatus::Ok
    }
}

/// True when available memory is below the configured percentage of total.
fn memory_low(avail_kb: u64, total_kb: u64, t: &Thresholds) -> bool {
    avail_kb.saturating_mul(100) < total_kb.saturating_mul(t.memory_warn_percent)
}

const EXPLAIN_MEM_UNREADABLE: &str =
    "/proc/meminfo could not be read, so available memory is unknown. \
It is always present on a normal Linux system; its absence suggests /proc is not mounted \
or access to it is restricted, for example in a sandbox.";
const EXPLAIN_MEM_UNPARSEABLE: &str = "/proc/meminfo was read but has no MemTotal line in the \
expected format, so available memory is unknown. This can happen with an unusual kernel or \
a sandbox that replaces the file.";
const EXPLAIN_MEM_LOW: &str = "Available memory is below the configured percentage of total \
memory, so the system may slow down, swap heavily, or have programs killed when it runs out. \
Look for programs using a lot of memory in a process monitor such as top or htop.";

/// The memory-info row for the contents of /proc/meminfo (None = unreadable).
fn memory_result(content: Option<&str>, t: &Thresholds) -> CheckResult {
    let Some(content) = content else {
        return CheckResult::problem(
            "memory-info",
            GROUP,
            CheckStatus::Warn,
            "Memory info unavailable (/proc/meminfo)",
            "",
            EXPLAIN_MEM_UNREADABLE,
        );
    };
    let mut total_kb: Option<u64> = None;
    let mut avail_kb: Option<u64> = None;
    for line in content.lines() {
        if let Some(v) = parse_meminfo_line(line, "MemTotal:") {
            total_kb = Some(v);
        } else if let Some(v) = parse_meminfo_line(line, "MemAvailable:") {
            avail_kb = Some(v);
        }
        if total_kb.is_some() && avail_kb.is_some() {
            break;
        }
    }
    let Some(total_kb) = total_kb else {
        return CheckResult::problem(
            "memory-info",
            GROUP,
            CheckStatus::Warn,
            "Could not parse /proc/meminfo",
            "",
            EXPLAIN_MEM_UNPARSEABLE,
        );
    };
    let total_mb = total_kb / 1024;
    let avail = avail_kb.unwrap_or(0);
    let avail_mb = avail / 1024;
    if avail_kb.is_some() && total_kb > 0 && memory_low(avail, total_kb, t) {
        CheckResult::problem(
            "memory-info",
            GROUP,
            CheckStatus::Warn,
            format!("Low available memory: {avail_mb} MB of {total_mb} MB"),
            "",
            EXPLAIN_MEM_LOW,
        )
    } else {
        CheckResult::new(
            "memory-info",
            GROUP,
            CheckStatus::Ok,
            format!("Memory: {avail_mb} MB available of {total_mb} MB"),
            "",
        )
    }
}

fn run_memory(results: &mut Vec<CheckResult>, t: &Thresholds) {
    let content = std::fs::read_to_string("/proc/meminfo").ok();
    results.push(memory_result(content.as_deref(), t));
}

pub fn run(results: &mut Vec<CheckResult>, cfg: &Config) {
    if cfg.enabled("kernel-version") {
        run_kernel(results);
    }
    if cfg.enabled("root-disk-space") {
        run_disk(results, &cfg.thresholds);
    }
    if cfg.enabled("tmp-writable") {
        run_tmp(results);
    }
    if cfg.enabled("memory-info") {
        run_memory(results, &cfg.thresholds);
    }
}

fn run_kernel(results: &mut Vec<CheckResult>) {
    match uname() {
        Some((sysname, release, version)) => results.push(CheckResult::new(
            "kernel-version",
            GROUP,
            CheckStatus::Ok,
            format!("Kernel detected: {release}"),
            format!("{sysname} {release} {version}"),
        )),
        None => results.push(CheckResult::problem(
            "kernel-version",
            GROUP,
            CheckStatus::Warn,
            "Could not read kernel version",
            "",
            "osdoctor could not read the running kernel version from the system. This is \
very unusual on Linux and suggests osdoctor is running inside a restricted sandbox.",
        )),
    }
}

const EXPLAIN_DISK_LOW: &str = "The root filesystem has less free space than the configured \
threshold. When it fills up, updates, logs and applications start failing. Look for large \
files in your home directory, the pacman package cache in /var/cache/pacman/pkg and the \
systemd journal.";

/// The root-disk-space row for `free_bytes` free on / (None = statvfs failed).
fn disk_result(free_bytes: Option<u64>, t: &Thresholds) -> CheckResult {
    let Some(free_bytes) = free_bytes else {
        return CheckResult::problem(
            "root-disk-space",
            GROUP,
            CheckStatus::Warn,
            "Could not stat root filesystem",
            "",
            "osdoctor could not read the free space of the root filesystem. This is unusual \
and suggests / is not mounted normally, for example inside a restricted container or sandbox.",
        );
    };
    let message = format!("Root filesystem has {} GB free", free_bytes / GIB);
    match disk_status(free_bytes, t) {
        CheckStatus::Ok => CheckResult::new("root-disk-space", GROUP, CheckStatus::Ok, message, ""),
        status => CheckResult::problem(
            "root-disk-space",
            GROUP,
            status,
            message,
            "",
            EXPLAIN_DISK_LOW,
        ),
    }
}

fn run_disk(results: &mut Vec<CheckResult>, t: &Thresholds) {
    results.push(disk_result(statvfs_free_bytes("/"), t));
}

fn run_tmp(results: &mut Vec<CheckResult>) {
    if path_writable("/tmp") {
        results.push(CheckResult::new(
            "tmp-writable",
            GROUP,
            CheckStatus::Ok,
            "/tmp is writable",
            "",
        ));
    } else {
        results.push(CheckResult::problem(
            "tmp-writable",
            GROUP,
            CheckStatus::Fail,
            "/tmp is not writable",
            "",
            "Many programs create temporary files in /tmp and fail when they cannot. Check \
that /tmp exists, is not mounted read-only, and keeps its usual permissions: writable by \
everyone, with the sticky bit set.",
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meminfo_parse() {
        assert_eq!(
            parse_meminfo_line("MemTotal:       32684 kB", "MemTotal:"),
            Some(32684)
        );
        assert_eq!(
            parse_meminfo_line("MemAvailable:   100 kB", "MemAvailable:"),
            Some(100)
        );
        assert_eq!(
            parse_meminfo_line("MemFree:        5 kB", "MemTotal:"),
            None
        );
    }

    #[test]
    fn disk_status_uses_thresholds() {
        let t = Thresholds::default();
        assert_eq!(disk_status(GIB, &t), CheckStatus::Fail);
        assert_eq!(disk_status(2 * GIB, &t), CheckStatus::Warn);
        assert_eq!(disk_status(10 * GIB, &t), CheckStatus::Ok);
        let t = Thresholds {
            disk_warn_gb: 0,
            disk_fail_gb: 0,
            ..t
        };
        assert_eq!(disk_status(0, &t), CheckStatus::Ok); // 0 disables both
    }

    #[test]
    fn huge_disk_threshold_does_not_overflow() {
        let t = Thresholds {
            disk_warn_gb: u64::MAX,
            disk_fail_gb: u64::MAX,
            ..Thresholds::default()
        };
        assert_eq!(disk_status(500 * GIB, &t), CheckStatus::Fail);
    }

    #[test]
    fn memory_low_uses_percent() {
        let t = Thresholds::default(); // 10%
        assert!(memory_low(9, 100, &t));
        assert!(!memory_low(10, 100, &t));
        let t = Thresholds {
            memory_warn_percent: 0,
            ..t
        };
        assert!(!memory_low(0, 100, &t));
    }

    #[test]
    fn disk_results_explain_problems() {
        let t = Thresholds::default();
        let ok = disk_result(Some(100 * GIB), &t);
        assert_eq!((ok.status, ok.explain), (CheckStatus::Ok, ""));
        let warn = disk_result(Some(2 * GIB), &t);
        let fail = disk_result(Some(0), &t);
        assert_eq!(warn.status, CheckStatus::Warn);
        assert_eq!(fail.status, CheckStatus::Fail);
        // One text per situation, whatever the severity.
        assert_eq!(warn.explain, fail.explain);
        assert!(warn.explain.contains("threshold"), "{}", warn.explain);
        let unknown = disk_result(None, &t);
        assert_eq!(unknown.status, CheckStatus::Warn);
        assert_ne!(unknown.explain, warn.explain);
    }

    #[test]
    fn memory_results_explain_each_situation() {
        let t = Thresholds::default();
        let ok = memory_result(Some("MemTotal: 1000 kB\nMemAvailable: 900 kB\n"), &t);
        assert_eq!((ok.status, ok.explain), (CheckStatus::Ok, ""));
        let low = memory_result(Some("MemTotal: 1000 kB\nMemAvailable: 10 kB\n"), &t);
        let unreadable = memory_result(None, &t);
        let garbled = memory_result(Some("nonsense\n"), &t);
        for r in [&low, &unreadable, &garbled] {
            assert_eq!(r.status, CheckStatus::Warn);
        }
        assert_eq!(low.message, "Low available memory: 0 MB of 0 MB");
        assert_eq!(
            unreadable.message,
            "Memory info unavailable (/proc/meminfo)"
        );
        assert_eq!(garbled.message, "Could not parse /proc/meminfo");
        assert_ne!(low.explain, unreadable.explain);
        assert_ne!(unreadable.explain, garbled.explain);
        assert_ne!(low.explain, garbled.explain);
    }

    #[test]
    fn run_emits_four_system_checks() {
        let mut results = Vec::new();
        run(&mut results, &Config::default());
        let ids: Vec<&str> = results.iter().map(|r| r.id).collect();
        assert_eq!(
            ids,
            [
                "kernel-version",
                "root-disk-space",
                "tmp-writable",
                "memory-info"
            ]
        );
        assert!(results.iter().all(|r| r.group == "System"));
    }
}
