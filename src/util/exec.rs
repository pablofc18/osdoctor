//! Subprocess capture. Uses Command directly instead of
//! popen+a shell, so no argument is ever shell-interpreted.

use std::process::Command;

/// What a finished subprocess printed and how it exited.
pub struct Captured {
    pub stdout: String,
    pub stderr: String,
    /// None if the process could not be spawned or was killed by a signal.
    pub code: Option<i32>,
}

/// Run `program` with `args`, capturing stdout and stderr (stdin is
/// inherited, as with popen).
pub fn run_capture(program: &str, args: &[&str]) -> Captured {
    match Command::new(program).args(args).output() {
        Ok(o) => Captured {
            stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
            code: o.status.code(),
        },
        Err(_) => Captured {
            stdout: String::new(),
            stderr: String::new(),
            code: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_failure_yields_none() {
        let c = run_capture("osdoctor_not_a_real_program_xyz", &[]);
        assert_eq!(c.stdout, "");
        assert_eq!(c.code, None);
    }

    #[test]
    fn captures_stdout_stderr_and_code() {
        let c = run_capture("sh", &["-c", "echo out; echo err >&2; exit 3"]);
        assert_eq!(c.stdout, "out\n");
        assert_eq!(c.stderr, "err\n");
        assert_eq!(c.code, Some(3));
    }
}
