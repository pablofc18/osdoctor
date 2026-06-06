//! Subprocess capture (replaces util_exec.c). Uses Command directly instead of
//! popen+a shell; stderr is discarded like the C's `2>/dev/null`.

use std::process::{Command, Stdio};

/// Run `program` with `args`, capturing stdout. Returns (stdout, exit_code),
/// where exit_code is None if the process could not be spawned or did not exit
/// normally.
pub fn run_capture(program: &str, args: &[&str]) -> (String, Option<i32>) {
    match Command::new(program)
        .args(args)
        .stderr(Stdio::null())
        .output()
    {
        Ok(o) => (
            String::from_utf8_lossy(&o.stdout).into_owned(),
            o.status.code(),
        ),
        Err(_) => (String::new(), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_failure_yields_none() {
        let (out, code) = run_capture("osdoctor_not_a_real_program_xyz", &[]);
        assert_eq!(out, "");
        assert_eq!(code, None);
    }

    #[test]
    fn captures_stdout() {
        // `true` exits 0 with no output; reliably present on Linux.
        let (out, code) = run_capture("true", &[]);
        assert_eq!(out, "");
        assert_eq!(code, Some(0));
    }
}
