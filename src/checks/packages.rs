//! Packages checks: pacman db lock, log, and orphan
//! packages.
//!
//! The orphan-package lookup sits behind a small backend seam. `query_pacman`
//! runs `pacman -Qdtq`; when built with the `libalpm` feature,
//! `packages_alpm::query` reads the local pacman database directly and is
//! tried first, falling back to pacman only on a hard error. `emit` turns
//! either backend's result into the check output, so both share wording.

use crate::config::Config;
use crate::model::{CheckResult, CheckStatus};
use crate::util::exec::run_capture;
use crate::util::fs::{file_exists, file_readable, is_executable_in_path};
use crate::util::strings::Detail;

const GROUP: &str = "Packages";
pub(crate) const DETAIL_CAP: usize = 1024;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum QueryStatus {
    /// Authoritative answer: `count` and `detail` are filled.
    Ok,
    /// The backend cannot run (pacman not installed, libalpm init failed).
    Unavailable,
    /// pacman ran but failed; `detail` holds its first error line.
    Failed,
}

pub struct Query {
    pub status: QueryStatus,
    pub count: i32,
    pub detail: String,
}

pub fn emit(results: &mut Vec<CheckResult>, q: &Query) {
    if q.status == QueryStatus::Failed {
        results.push(CheckResult::new(
            "orphan-packages",
            GROUP,
            CheckStatus::Skip,
            "Could not query orphan packages",
            q.detail.clone(),
        ));
        return;
    }
    if q.status == QueryStatus::Unavailable {
        results.push(CheckResult::new(
            "orphan-packages",
            GROUP,
            CheckStatus::Skip,
            "pacman not available",
            "",
        ));
        return;
    }
    if q.count <= 0 {
        results.push(CheckResult::new(
            "orphan-packages",
            GROUP,
            CheckStatus::Ok,
            "No orphan packages",
            "",
        ));
        return;
    }
    let plural = if q.count == 1 { "" } else { "s" };
    results.push(CheckResult::problem(
        "orphan-packages",
        GROUP,
        CheckStatus::Warn,
        format!("{} orphan package{plural} detected", q.count),
        q.detail.clone(),
        "These packages were installed as dependencies and no installed package needs them \
anymore. Review the list before removing any: some may be tools you use directly.",
    ));
}

/// Turn a `pacman -Qdtq` result into a Query. pacman exits 0 and lists the
/// orphans, or exits 1 silently when there are none. Any other outcome (a
/// non-zero exit with an error on stderr, another exit code, or no exit code)
/// is a failure, not "no orphans".
fn classify(code: Option<i32>, stdout: &str, stderr: &str) -> Query {
    let clean_none = code == Some(1) && stdout.trim().is_empty() && stderr.trim().is_empty();
    if code != Some(0) && !clean_none {
        // Prefer pacman's "error: ..." line over any preceding warnings.
        let mut lines = stderr.lines().map(str::trim).filter(|l| !l.is_empty());
        let reason = lines
            .clone()
            .find(|l| l.starts_with("error:"))
            .or_else(|| lines.next())
            .map(str::to_string)
            .unwrap_or_else(|| match code {
                Some(c) => format!("pacman -Qdtq exited with status {c}"),
                None => "pacman -Qdtq was terminated".to_string(),
            });
        return Query {
            status: QueryStatus::Failed,
            count: 0,
            detail: reason,
        };
    }
    let mut detail = Detail::new(DETAIL_CAP);
    let mut n = 0;
    for line in stdout.lines().map(str::trim).filter(|l| !l.is_empty()) {
        n += 1;
        detail.append(line);
    }
    Query {
        status: QueryStatus::Ok,
        count: n,
        detail: detail.into_string(),
    }
}

fn query_pacman() -> Query {
    if !is_executable_in_path("pacman") {
        return Query {
            status: QueryStatus::Unavailable,
            count: 0,
            detail: String::new(),
        };
    }
    let c = run_capture("pacman", &["-Qdtq"]);
    classify(c.code, &c.stdout, &c.stderr)
}

/// Enumerate orphans, preferring libalpm when compiled in. An Ok result is
/// authoritative; only a hard error (Unavailable) falls back to pacman.
fn query_orphans() -> Query {
    #[cfg(feature = "libalpm")]
    {
        let q = super::packages_alpm::query();
        if q.status != QueryStatus::Unavailable {
            return q;
        }
    }
    query_pacman()
}

pub fn run(results: &mut Vec<CheckResult>, cfg: &Config) {
    if cfg.enabled("pacman-lock") {
        check_lock(results);
    }
    if cfg.enabled("pacman-log") {
        check_log(results);
    }
    if cfg.enabled("orphan-packages") {
        emit(results, &query_orphans());
    }
}

fn check_lock(results: &mut Vec<CheckResult>) {
    results.push(lock_result(file_exists("/var/lib/pacman/db.lck")));
}

/// The pacman-lock row for whether /var/lib/pacman/db.lck exists.
fn lock_result(locked: bool) -> CheckResult {
    if locked {
        CheckResult::problem(
            "pacman-lock",
            GROUP,
            CheckStatus::Fail,
            "pacman database is locked (db.lck present)",
            "",
            "A package manager is running right now, or pacman crashed or was interrupted and \
left its lock file /var/lib/pacman/db.lck behind. If no pacman, yay or other package manager \
is running, the lock is stale and can be removed as root. Never remove it while one is running.",
        )
    } else {
        CheckResult::new(
            "pacman-lock",
            GROUP,
            CheckStatus::Ok,
            "pacman database is unlocked",
            "",
        )
    }
}

fn check_log(results: &mut Vec<CheckResult>) {
    results.push(log_result(file_readable("/var/log/pacman.log")));
}

/// The pacman-log row for whether /var/log/pacman.log is readable.
fn log_result(readable: bool) -> CheckResult {
    if readable {
        CheckResult::new(
            "pacman-log",
            GROUP,
            CheckStatus::Ok,
            "pacman log exists",
            "",
        )
    } else {
        CheckResult::problem(
            "pacman-log",
            GROUP,
            CheckStatus::Warn,
            "pacman log missing or unreadable",
            "",
            "pacman records every install, upgrade and removal in /var/log/pacman.log, which \
is useful when tracking down what changed. Check whether the file was deleted, lost its read \
permission, or whether LogFile in /etc/pacman.conf points somewhere else.",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn last(r: &[CheckResult]) -> &CheckResult {
        r.last().unwrap()
    }

    #[test]
    fn emit_ok_zero() {
        let mut r = Vec::new();
        emit(
            &mut r,
            &Query {
                status: QueryStatus::Ok,
                count: 0,
                detail: String::new(),
            },
        );
        assert_eq!(last(&r).status, CheckStatus::Ok);
        assert_eq!(last(&r).id, "orphan-packages");
        assert_eq!(last(&r).group, "Packages");
        assert_eq!(last(&r).message, "No orphan packages");
    }

    #[test]
    fn emit_counts_and_plural() {
        let mut r = Vec::new();
        emit(
            &mut r,
            &Query {
                status: QueryStatus::Ok,
                count: 3,
                detail: "a, b, c".into(),
            },
        );
        assert_eq!(last(&r).status, CheckStatus::Warn);
        assert_eq!(last(&r).message, "3 orphan packages detected");
        assert_eq!(last(&r).detail, "a, b, c");

        emit(
            &mut r,
            &Query {
                status: QueryStatus::Ok,
                count: 1,
                detail: "only".into(),
            },
        );
        assert_eq!(last(&r).message, "1 orphan package detected");
    }

    #[test]
    fn emit_unavailable() {
        let mut r = Vec::new();
        emit(
            &mut r,
            &Query {
                status: QueryStatus::Unavailable,
                count: 0,
                detail: String::new(),
            },
        );
        assert_eq!(last(&r).status, CheckStatus::Skip);
        assert_eq!(last(&r).message, "pacman not available");
    }

    #[test]
    fn emit_failed_is_skip_with_reason() {
        let mut r = Vec::new();
        emit(
            &mut r,
            &Query {
                status: QueryStatus::Failed,
                count: 0,
                detail: "error: database broken".into(),
            },
        );
        assert_eq!(last(&r).status, CheckStatus::Skip);
        assert_eq!(last(&r).message, "Could not query orphan packages");
        assert_eq!(last(&r).detail, "error: database broken");
    }

    #[test]
    fn pacman_lock_advice_is_an_explanation() {
        let r = lock_result(true);
        assert_eq!(r.status, CheckStatus::Fail);
        assert_eq!(r.detail, "");
        assert!(r.explain.contains("Never remove it while"), "{}", r.explain);
        assert_eq!(lock_result(false).status, CheckStatus::Ok);
    }

    #[test]
    fn warnings_carry_explanations() {
        let mut r = Vec::new();
        emit(
            &mut r,
            &Query {
                status: QueryStatus::Ok,
                count: 2,
                detail: "a, b".into(),
            },
        );
        assert!(last(&r).explain.contains("Review the list"));
        let r = log_result(false);
        assert_eq!(r.status, CheckStatus::Warn);
        assert!(r.explain.contains("/var/log/pacman.log"), "{}", r.explain);
    }

    #[test]
    fn classify_pacman_outcomes() {
        // Orphans listed (exit 0).
        let q = classify(Some(0), "a\n\nb\n", "");
        assert_eq!(
            (q.status, q.count, q.detail.as_str()),
            (QueryStatus::Ok, 2, "a, b")
        );

        // No orphans: pacman exits 1 without output.
        let q = classify(Some(1), "", "");
        assert_eq!((q.status, q.count), (QueryStatus::Ok, 0));

        // Failures are not "no orphans".
        let q = classify(Some(1), "", "warning: x\nerror: database broken\n");
        assert_eq!(q.status, QueryStatus::Failed);
        assert_eq!(q.detail, "error: database broken");
        let q = classify(Some(2), "", "");
        assert_eq!(q.status, QueryStatus::Failed);
        assert_eq!(q.detail, "pacman -Qdtq exited with status 2");
        let q = classify(None, "", "");
        assert_eq!(q.status, QueryStatus::Failed);
    }
}
