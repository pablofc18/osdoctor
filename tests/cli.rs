use std::os::fd::FromRawFd;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A fresh empty directory under the system temp dir, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> TempDir {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "osdoctor-test-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl std::ops::Deref for TempDir {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

/// The binary, isolated from the developer's real config: HOME and
/// XDG_CONFIG_HOME point at a directory that does not exist, so no config
/// file is found. Tests that need a config use `bin_in`.
fn bin() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_osdoctor"));
    cmd.env("HOME", "/nonexistent")
        .env("XDG_CONFIG_HOME", "/nonexistent");
    cmd
}

/// A fresh config home; returns it and the path of its default config file.
fn config_home_with(contents: &str) -> (TempDir, PathBuf) {
    let dir = TempDir::new();
    let cfg_dir = dir.join("osdoctor");
    std::fs::create_dir_all(&cfg_dir).unwrap();
    let file = cfg_dir.join("config.toml");
    std::fs::write(&file, contents).unwrap();
    (dir, file)
}

fn bin_in(home: &Path) -> Command {
    let mut cmd = bin();
    cmd.env("HOME", home).env("XDG_CONFIG_HOME", home);
    cmd
}

const SYSTEM_OFF_EXCEPT_TMP: &str =
    "[checks]\nkernel-version = \"off\"\nroot-disk-space = \"off\"\nmemory-info = \"off\"\n";

#[test]
fn version_prints_exact_line() {
    let out = bin().arg("version").output().unwrap();
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!("osdoctor {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn short_and_long_aliases_match_commands() {
    for (alias, cmd) in [
        ("-V", "version"),
        ("--version", "version"),
        ("-h", "help"),
        ("--help", "help"),
    ] {
        let a = bin().arg(alias).output().unwrap();
        let c = bin().arg(cmd).output().unwrap();
        assert_eq!(a.status.code(), Some(0), "{alias}");
        assert_eq!(a.stdout, c.stdout, "{alias}");
    }
}

#[test]
fn help_mentions_usage() {
    let out = bin().arg("help").output().unwrap();
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("Usage:"));
    assert!(s.contains("osdoctor <command> [options]"));
}

#[test]
fn scan_json_is_well_formed() {
    let out = bin().args(["scan", "--json"]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.starts_with("{\n"));
    assert!(s.contains("\"groups\""));
    assert!(s.trim_end().ends_with('}'));
    // exit code is 0/1/2 depending on host; just require a defined code.
    assert!(matches!(out.status.code(), Some(0) | Some(1) | Some(2)));
}

#[test]
fn usage_errors_exit_three() {
    let cases: [&[&str]; 4] = [
        &[],                 // no command
        &["bogus"],          // unknown command
        &["--nope"],         // unknown option
        &["scan", "system"], // extra argument
    ];
    for args in cases {
        let out = bin().args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(3), "{args:?}");
        assert!(out.stdout.is_empty(), "{args:?}");
        assert!(out.stderr.starts_with(b"osdoctor: "), "{args:?}");
    }
}

/// A Stdio whose read end is already closed, so every write fails with EPIPE
/// (Rust ignores SIGPIPE, so the write returns an error instead of killing us).
fn closed_pipe() -> Stdio {
    let mut fds = [0; 2];
    assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
    unsafe {
        libc::close(fds[0]);
        Stdio::from_raw_fd(fds[1])
    }
}

#[test]
fn closed_stdout_is_not_a_crash() {
    // `osdoctor scan | head -1` style: the reader goes away. The output is
    // simply unwanted; no panic, and the exit code still reflects the checks.
    for args in [
        &["desktop"][..],
        &["desktop", "--json"],
        &["help"],
        &["version"],
    ] {
        let out = bin()
            .args(args)
            .stdout(closed_pipe())
            .stderr(Stdio::piped())
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!stderr.contains("panicked"), "{args:?}: {stderr}");
        assert!(
            matches!(out.status.code(), Some(0) | Some(1) | Some(2)),
            "{args:?}: {:?}",
            out.status
        );
    }
}

#[test]
fn unwritable_stdout_is_an_internal_error() {
    // Any other write failure (here: a full device) means the report was
    // lost, which is exit code 3 with a message, not a panic.
    let full = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/full")
        .unwrap();
    let out = bin()
        .arg("version")
        .stdout(full)
        .stderr(Stdio::piped())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(3), "{stderr}");
    assert!(
        stderr.starts_with("osdoctor: cannot write output:"),
        "{stderr}"
    );
    assert!(!stderr.contains("panicked"), "{stderr}");
}

#[test]
fn config_off_removes_checks_and_overrides_apply() {
    let (home, file) =
        config_home_with(&format!("{SYSTEM_OFF_EXCEPT_TMP}tmp-writable = \"fail\"\n"));
    let out = bin_in(&home)
        .args(["system", "--json", "--config"])
        .arg(&file)
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    assert_eq!(s.matches("\"id\"").count(), 1, "{s}");
    assert!(s.contains("\"tmp-writable\""));
    let code = out.status.code();
    if s.contains("\"status\": \"ok\"") {
        assert_eq!(code, Some(0));
    } else {
        assert_eq!(code, Some(2), "{s}");
    }
}

#[test]
fn invalid_config_exits_three_with_path() {
    let (home, file) = config_home_with("[checks]\nwaybar-script = \"off\"\n");
    let out = bin_in(&home)
        .args(["scan", "--config"])
        .arg(&file)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.starts_with("osdoctor: "), "{err}");
    assert!(err.contains("config.toml"), "{err}");
    assert!(
        err.contains("unknown check \"waybar-script\" at line 2"),
        "{err}"
    );
}

#[test]
fn missing_explicit_config_exits_three() {
    let home = TempDir::new();
    let out = bin_in(&home)
        .args(["scan", "--config"])
        .arg(home.join("nope.toml"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("nope.toml"));
}

#[test]
fn default_config_location_is_used() {
    let (home, _) = config_home_with("[checks]\nkernel-version = \"off\"\n");
    let out = bin_in(&home).args(["system", "--json"]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("\"tmp-writable\""), "{s}");
    assert!(!s.contains("kernel-version"), "{s}");
}

#[test]
fn broken_config_does_not_block_help_or_version() {
    let (home, _) = config_home_with("[[[");
    for arg in ["help", "version", "-h", "-V"] {
        let out = bin_in(&home).arg(arg).output().unwrap();
        assert_eq!(out.status.code(), Some(0), "{arg}");
    }
}

#[test]
fn config_flag_without_value_exits_three() {
    let out = bin().args(["scan", "--config"]).output().unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--config requires a path"));
}

#[test]
fn config_flag_twice_exits_three() {
    let out = bin()
        .args(["scan", "--config", "a", "--config", "b"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--config given more than once"));
}

#[test]
fn strict_applies_after_overrides() {
    let base = "[thresholds]\nmemory_warn_percent = 100\n[checks]\nkernel-version = \"off\"\nroot-disk-space = \"off\"\ntmp-writable = \"off\"\n";
    let run = |extra: &str, strict: bool| {
        let (home, file) = config_home_with(&format!("{base}{extra}"));
        let mut cmd = bin_in(&home);
        cmd.args(["system", "--config"]).arg(&file);
        if strict {
            cmd.arg("--strict");
        }
        cmd.output().unwrap().status.code()
    };
    assert_eq!(run("memory-info = \"fail\"\n", false), Some(2));
    assert_eq!(run("memory-info = \"warn\"\n", false), Some(1));
    assert_eq!(run("memory-info = \"warn\"\n", true), Some(2));
}

#[test]
fn dev_null_config_means_defaults() {
    let (home, _) = config_home_with("[checks]\nkernel-version = \"off\"\n");
    let out = bin_in(&home)
        .args(["system", "--json", "--config", "/dev/null"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("kernel-version"));
}

const MEMORY_WARN_CONFIG: &str = "[checks]\nkernel-version = \"off\"\nroot-disk-space = \"off\"\ntmp-writable = \"off\"\n\n[thresholds]\nmemory_warn_percent = 100\n";

#[test]
fn explain_flag_shows_explanations() {
    let (home, f) = config_home_with(MEMORY_WARN_CONFIG);
    let run = |extra: &[&str]| {
        let out = bin_in(&home)
            .args(["system", "--config"])
            .arg(&f)
            .args(extra)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    assert!(!run(&[]).lines().any(|l| l.starts_with("      ? ")));
    assert!(run(&["--explain"])
        .lines()
        .any(|l| l.starts_with("      ? ")));
    assert!(run(&["--explain", "--json"]).contains("\"explain\": \""));
}

#[test]
fn explain_does_not_change_exit_code() {
    let (home, f) = config_home_with(MEMORY_WARN_CONFIG);
    let code = |extra: &[&str]| {
        bin_in(&home)
            .args(["system", "--config"])
            .arg(&f)
            .args(extra)
            .output()
            .unwrap()
            .status
            .code()
    };
    assert_eq!(code(&["--strict"]), Some(2));
    assert_eq!(code(&["--strict", "--explain"]), Some(2));
    assert_eq!(code(&[]), Some(1));
    assert_eq!(code(&["--explain"]), Some(1));
}

#[test]
fn explain_is_ignored_by_help_and_version() {
    for cmd in ["help", "version"] {
        let out = bin().args([cmd, "--explain"]).output().unwrap();
        assert_eq!(out.status.code(), Some(0), "{cmd}");
    }
}
