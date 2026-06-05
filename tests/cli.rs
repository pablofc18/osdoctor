use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_osdoctor"))
}

#[test]
fn version_prints_exact_line() {
    let out = bin().arg("version").output().unwrap();
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), "osdoctor 0.1.0\n");
}

#[test]
fn help_mentions_usage() {
    let out = bin().arg("help").output().unwrap();
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("Usage:"));
    assert!(s.contains("osdoctor [command] [options]"));
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
