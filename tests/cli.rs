use std::os::fd::FromRawFd;
use std::process::{Command, Stdio};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_osdoctor"))
}

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
fn unknown_command_exits_three() {
    let out = bin().arg("bogus").output().unwrap();
    assert_eq!(out.status.code(), Some(3));
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
