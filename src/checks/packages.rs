//! Packages checks (replaces check_packages.c): pacman db lock, log, and orphan
//! packages.
//!
//! The orphan-package lookup sits behind a small backend seam. `query_pacman`
//! runs `pacman -Qdtq`; when built with the `libalpm` feature,
//! `packages_alpm::query` reads the local pacman database directly and is
//! tried first, falling back to pacman only on a hard error. `emit` turns
//! either backend's result into the check output, so both share wording.

use crate::model::{CheckResult, CheckStatus};
use crate::util::exec::run_capture;
use crate::util::fs::{file_exists, file_readable, is_executable_in_path};
use crate::util::strings::Detail;

const GROUP: &str = "Packages";
pub(crate) const DETAIL_CAP: usize = 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum QueryStatus {
    Ok,
    Unavailable,
}

pub struct Query {
    pub status: QueryStatus,
    pub count: i32,
    pub detail: String,
}

pub fn emit(results: &mut Vec<CheckResult>, q: &Query) {
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
    results.push(CheckResult::new(
        "orphan-packages",
        GROUP,
        CheckStatus::Warn,
        format!("{} orphan package{plural} detected", q.count),
        q.detail.clone(),
    ));
}

fn query_pacman() -> Query {
    if !is_executable_in_path("pacman") {
        return Query {
            status: QueryStatus::Unavailable,
            count: 0,
            detail: String::new(),
        };
    }
    let (out, _code) = run_capture("pacman", &["-Qdtq"]);
    let mut detail = Detail::new(DETAIL_CAP);
    let mut n = 0;
    for line in out.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        n += 1;
        detail.append(line);
    }
    Query {
        status: QueryStatus::Ok,
        count: n,
        detail: detail.into_string(),
    }
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

pub fn run(results: &mut Vec<CheckResult>) {
    if file_exists("/var/lib/pacman/db.lck") {
        results.push(CheckResult::new(
            "pacman-lock",
            GROUP,
            CheckStatus::Fail,
            "pacman database is locked (db.lck present)",
            "Remove /var/lib/pacman/db.lck only if no pacman process is running.",
        ));
    } else {
        results.push(CheckResult::new(
            "pacman-lock",
            GROUP,
            CheckStatus::Ok,
            "pacman database is unlocked",
            "",
        ));
    }

    if file_readable("/var/log/pacman.log") {
        results.push(CheckResult::new(
            "pacman-log",
            GROUP,
            CheckStatus::Ok,
            "pacman log exists",
            "",
        ));
    } else {
        results.push(CheckResult::new(
            "pacman-log",
            GROUP,
            CheckStatus::Warn,
            "pacman log missing or unreadable",
            "",
        ));
    }

    emit(results, &query_orphans());
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
}
