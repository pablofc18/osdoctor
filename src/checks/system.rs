//! System checks (replaces check_system.c): kernel, root disk, /tmp, memory.
//! Uses libc uname/statvfs and /proc/meminfo; never shells out.

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

fn run_memory(results: &mut Vec<CheckResult>) {
    let content = match std::fs::read_to_string("/proc/meminfo") {
        Ok(c) => c,
        Err(_) => {
            results.push(CheckResult::new(
                "memory-info",
                GROUP,
                CheckStatus::Warn,
                "Memory info unavailable (/proc/meminfo)",
                "",
            ));
            return;
        }
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
    let total_kb = match total_kb {
        Some(t) => t,
        None => {
            results.push(CheckResult::new(
                "memory-info",
                GROUP,
                CheckStatus::Warn,
                "Could not parse /proc/meminfo",
                "",
            ));
            return;
        }
    };
    let total_mb = total_kb / 1024;
    let avail = avail_kb.unwrap_or(0);
    let avail_mb = avail / 1024;
    if avail_kb.is_some() && total_kb > 0 && avail * 10 < total_kb {
        results.push(CheckResult::new(
            "memory-info",
            GROUP,
            CheckStatus::Warn,
            format!("Low available memory: {avail_mb} MB of {total_mb} MB"),
            "",
        ));
    } else {
        results.push(CheckResult::new(
            "memory-info",
            GROUP,
            CheckStatus::Ok,
            format!("Memory: {avail_mb} MB available of {total_mb} MB"),
            "",
        ));
    }
}

pub fn run(results: &mut Vec<CheckResult>) {
    match uname() {
        Some((sysname, release, version)) => results.push(CheckResult::new(
            "kernel-version",
            GROUP,
            CheckStatus::Ok,
            format!("Kernel detected: {release}"),
            format!("{sysname} {release} {version}"),
        )),
        None => results.push(CheckResult::new(
            "kernel-version",
            GROUP,
            CheckStatus::Warn,
            "Could not read kernel version",
            "",
        )),
    }

    match statvfs_free_bytes("/") {
        Some(free_bytes) => {
            let status = if free_bytes < 2 * GIB {
                CheckStatus::Fail
            } else if free_bytes < 10 * GIB {
                CheckStatus::Warn
            } else {
                CheckStatus::Ok
            };
            results.push(CheckResult::new(
                "root-disk-space",
                GROUP,
                status,
                format!("Root filesystem has {} GB free", free_bytes / GIB),
                "",
            ));
        }
        None => results.push(CheckResult::new(
            "root-disk-space",
            GROUP,
            CheckStatus::Warn,
            "Could not stat root filesystem",
            "",
        )),
    }

    if path_writable("/tmp") {
        results.push(CheckResult::new(
            "tmp-writable",
            GROUP,
            CheckStatus::Ok,
            "/tmp is writable",
            "",
        ));
    } else {
        results.push(CheckResult::new(
            "tmp-writable",
            GROUP,
            CheckStatus::Fail,
            "/tmp is not writable",
            "",
        ));
    }

    run_memory(results);
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
    fn run_emits_four_system_checks() {
        let mut results = Vec::new();
        run(&mut results);
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
