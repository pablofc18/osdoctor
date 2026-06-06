//! Services checks (replaces check_services.c): failed system/user units via
//! `systemctl`. The query->emit seam is preserved (only the systemctl backend
//! is implemented; the sd-bus backend is out of scope for the port).

use crate::model::{CheckResult, CheckStatus};
use crate::util::exec::run_capture;
use crate::util::fs::is_executable_in_path;
use crate::util::strings::Detail;

const GROUP: &str = "Services";
const DETAIL_CAP: usize = 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    System,
    User,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum QueryStatus {
    Ok,
    NoSession,
    Unavailable,
}

pub struct Query {
    pub status: QueryStatus,
    pub count: i32,
    pub detail: String,
}

/// Translate a backend query result into a check result (wording matches the C).
pub fn emit(results: &mut Vec<CheckResult>, scope: Scope, q: &Query) {
    let id = match scope {
        Scope::System => "failed-system-services",
        Scope::User => "failed-user-services",
    };
    let noun = match scope {
        Scope::System => "system",
        Scope::User => "user",
    };
    match q.status {
        QueryStatus::Ok if q.count <= 0 => {
            results.push(CheckResult::new(
                id,
                GROUP,
                CheckStatus::Ok,
                format!("No failed {noun} services"),
                "",
            ));
        }
        QueryStatus::Ok => {
            let plural = if q.count == 1 { "" } else { "s" };
            results.push(CheckResult::new(
                id,
                GROUP,
                CheckStatus::Warn,
                format!("{} failed {noun} service{plural}", q.count),
                q.detail.clone(),
            ));
        }
        QueryStatus::NoSession => {
            let msg = match scope {
                Scope::User => "No systemd user session available",
                Scope::System => "No system bus available",
            };
            results.push(CheckResult::new(id, GROUP, CheckStatus::Skip, msg, ""));
        }
        QueryStatus::Unavailable => {
            results.push(CheckResult::new(
                id,
                GROUP,
                CheckStatus::Skip,
                "systemctl not available",
                "",
            ));
        }
    }
}

fn count_units(out: &str) -> (i32, String) {
    let mut detail = Detail::new(DETAIL_CAP);
    let mut count = 0;
    for line in out.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        count += 1;
        if let Some(unit) = line.split_whitespace().next() {
            detail.append(unit);
        }
    }
    (count, detail.into_string())
}

fn query_systemctl(scope: Scope) -> Query {
    if !is_executable_in_path("systemctl") {
        return Query {
            status: QueryStatus::Unavailable,
            count: 0,
            detail: String::new(),
        };
    }
    let args: &[&str] = match scope {
        Scope::System => &["--failed", "--no-legend", "--plain"],
        Scope::User => &["--user", "--failed", "--no-legend", "--plain"],
    };
    let (out, code) = run_capture("systemctl", args);
    if scope == Scope::User && code != Some(0) {
        return Query {
            status: QueryStatus::NoSession,
            count: 0,
            detail: String::new(),
        };
    }
    let (count, detail) = count_units(&out);
    Query {
        status: QueryStatus::Ok,
        count,
        detail,
    }
}

pub fn run(results: &mut Vec<CheckResult>) {
    let q = query_systemctl(Scope::System);
    emit(results, Scope::System, &q);
    let q = query_systemctl(Scope::User);
    emit(results, Scope::User, &q);
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
            Scope::System,
            &Query {
                status: QueryStatus::Ok,
                count: 0,
                detail: String::new(),
            },
        );
        assert_eq!(last(&r).status, CheckStatus::Ok);
        assert_eq!(last(&r).id, "failed-system-services");
        assert_eq!(last(&r).group, "Services");
        assert_eq!(last(&r).message, "No failed system services");

        emit(
            &mut r,
            Scope::User,
            &Query {
                status: QueryStatus::Ok,
                count: 0,
                detail: String::new(),
            },
        );
        assert_eq!(last(&r).message, "No failed user services");
    }

    #[test]
    fn emit_ok_counts_and_plural() {
        let mut r = Vec::new();
        emit(
            &mut r,
            Scope::System,
            &Query {
                status: QueryStatus::Ok,
                count: 3,
                detail: "a, b, c".into(),
            },
        );
        assert_eq!(last(&r).status, CheckStatus::Warn);
        assert_eq!(last(&r).message, "3 failed system services");
        assert_eq!(last(&r).detail, "a, b, c");

        emit(
            &mut r,
            Scope::System,
            &Query {
                status: QueryStatus::Ok,
                count: 1,
                detail: "only".into(),
            },
        );
        assert_eq!(last(&r).message, "1 failed system service");

        emit(
            &mut r,
            Scope::User,
            &Query {
                status: QueryStatus::Ok,
                count: 1,
                detail: "only".into(),
            },
        );
        assert_eq!(last(&r).message, "1 failed user service");
    }

    #[test]
    fn emit_no_session_user() {
        let mut r = Vec::new();
        emit(
            &mut r,
            Scope::User,
            &Query {
                status: QueryStatus::NoSession,
                count: 0,
                detail: String::new(),
            },
        );
        assert_eq!(last(&r).status, CheckStatus::Skip);
        assert_eq!(last(&r).message, "No systemd user session available");
    }

    #[test]
    fn emit_unavailable() {
        let mut r = Vec::new();
        emit(
            &mut r,
            Scope::System,
            &Query {
                status: QueryStatus::Unavailable,
                count: 0,
                detail: String::new(),
            },
        );
        emit(
            &mut r,
            Scope::User,
            &Query {
                status: QueryStatus::Unavailable,
                count: 0,
                detail: String::new(),
            },
        );
        assert!(r
            .iter()
            .all(|c| c.status == CheckStatus::Skip && c.message == "systemctl not available"));
    }

    #[test]
    fn count_units_collects_first_token() {
        let (n, detail) = count_units("a.service loaded failed\n\nb.service x y\n");
        assert_eq!(n, 2);
        assert_eq!(detail, "a.service, b.service");
    }
}
