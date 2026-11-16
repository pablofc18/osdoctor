//! Core types: a check result, its status, the summary of a run, and the
//! mapping from a summary to the overall status and process exit code.

/// Program version, sourced from Cargo so it cannot drift from the manifest.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CheckStatus {
    Ok,
    Warn,
    Fail,
    Skip,
}

impl CheckStatus {
    pub fn label(self) -> &'static str {
        match self {
            CheckStatus::Ok => "OK",
            CheckStatus::Warn => "WARN",
            CheckStatus::Fail => "FAIL",
            CheckStatus::Skip => "SKIP",
        }
    }

    pub fn json(self) -> &'static str {
        match self {
            CheckStatus::Ok => "ok",
            CheckStatus::Warn => "warn",
            CheckStatus::Fail => "fail",
            CheckStatus::Skip => "skip",
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            CheckStatus::Ok => "✓",
            CheckStatus::Warn => "!",
            CheckStatus::Fail => "✗",
            CheckStatus::Skip => "-",
        }
    }
}

#[derive(Clone, Debug)]
pub struct CheckResult {
    pub id: &'static str,
    pub group: &'static str,
    pub status: CheckStatus,
    pub message: String,
    pub detail: String,
    /// What a WARN/FAIL means, its likely cause and what to look at. Empty for
    /// OK and SKIP rows. `'static` so it can never carry runtime data such as
    /// unit names or paths read from the user's files.
    pub explain: &'static str,
}

impl CheckResult {
    /// An OK or SKIP row (no explanation). WARN/FAIL rows use [`Self::problem`].
    pub fn new(
        id: &'static str,
        group: &'static str,
        status: CheckStatus,
        message: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        debug_assert!(
            matches!(status, CheckStatus::Ok | CheckStatus::Skip),
            "{id}: WARN/FAIL rows must use CheckResult::problem"
        );
        CheckResult {
            id,
            group,
            status,
            message: message.into(),
            detail: detail.into(),
            explain: "",
        }
    }

    /// A WARN or FAIL row, which must carry a non-empty ASCII explanation.
    pub fn problem(
        id: &'static str,
        group: &'static str,
        status: CheckStatus,
        message: impl Into<String>,
        detail: impl Into<String>,
        explain: &'static str,
    ) -> Self {
        debug_assert!(
            matches!(status, CheckStatus::Warn | CheckStatus::Fail),
            "{id}: OK/SKIP rows must use CheckResult::new"
        );
        debug_assert!(!explain.is_empty(), "{id}: empty explanation");
        debug_assert!(explain.is_ascii(), "{id}: non-ASCII explanation");
        CheckResult {
            id,
            group,
            status,
            message: message.into(),
            detail: detail.into(),
            explain,
        }
    }
}

#[derive(Clone, Copy, Default, Debug)]
pub struct Summary {
    pub ok: i32,
    pub warn: i32,
    pub fail: i32,
    pub skip: i32,
}

pub fn summarize(results: &[CheckResult]) -> Summary {
    let mut s = Summary::default();
    for r in results {
        match r.status {
            CheckStatus::Ok => s.ok += 1,
            CheckStatus::Warn => s.warn += 1,
            CheckStatus::Fail => s.fail += 1,
            CheckStatus::Skip => s.skip += 1,
        }
    }
    s
}

/// any fail -> 2; any warn (no fail) -> 1, or 2 when strict; otherwise 0.
pub fn exit_code(summary: Summary, strict: bool) -> i32 {
    if summary.fail > 0 {
        2
    } else if summary.warn > 0 {
        if strict {
            2
        } else {
            1
        }
    } else {
        0
    }
}

/// Worst status present (fail > warn > ok); skips do not count.
pub fn overall_status(summary: Summary) -> CheckStatus {
    if summary.fail > 0 {
        CheckStatus::Fail
    } else if summary.warn > 0 {
        CheckStatus::Warn
    } else {
        CheckStatus::Ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(status: CheckStatus) -> CheckResult {
        match status {
            CheckStatus::Ok | CheckStatus::Skip => CheckResult::new("id", "G", status, "m", ""),
            _ => CheckResult::problem("id", "G", status, "m", "", "Because."),
        }
    }

    #[test]
    fn problem_carries_explanation() {
        let r = CheckResult::problem("x", "G", CheckStatus::Warn, "m", "", "Because.");
        assert_eq!(r.explain, "Because.");
        assert_eq!(
            CheckResult::new("x", "G", CheckStatus::Ok, "m", "").explain,
            ""
        );
    }

    #[test]
    #[should_panic]
    #[cfg(debug_assertions)]
    fn new_rejects_warn() {
        CheckResult::new("x", "G", CheckStatus::Warn, "m", "");
    }

    #[test]
    #[should_panic]
    #[cfg(debug_assertions)]
    fn problem_rejects_ok() {
        CheckResult::problem("x", "G", CheckStatus::Ok, "m", "", "Because.");
    }

    #[test]
    #[should_panic]
    #[cfg(debug_assertions)]
    fn problem_rejects_empty_explain() {
        CheckResult::problem("x", "G", CheckStatus::Fail, "m", "", "");
    }

    #[test]
    #[should_panic]
    #[cfg(debug_assertions)]
    fn problem_rejects_non_ascii() {
        CheckResult::problem("x", "G", CheckStatus::Fail, "m", "", "Caf\u{e9}.");
    }

    #[test]
    fn status_strings() {
        assert_eq!(CheckStatus::Ok.label(), "OK");
        assert_eq!(CheckStatus::Warn.json(), "warn");
        assert_eq!(CheckStatus::Fail.symbol(), "✗");
        assert_eq!(CheckStatus::Skip.label(), "SKIP");
    }

    #[test]
    fn summarize_counts() {
        let list = vec![
            r(CheckStatus::Ok),
            r(CheckStatus::Ok),
            r(CheckStatus::Warn),
            r(CheckStatus::Fail),
            r(CheckStatus::Skip),
        ];
        let s = summarize(&list);
        assert_eq!((s.ok, s.warn, s.fail, s.skip), (2, 1, 1, 1));
    }

    #[test]
    fn exit_codes() {
        let fail = Summary {
            ok: 0,
            warn: 0,
            fail: 1,
            skip: 0,
        };
        let warn = Summary {
            ok: 0,
            warn: 1,
            fail: 0,
            skip: 0,
        };
        let clean = Summary {
            ok: 3,
            warn: 0,
            fail: 0,
            skip: 0,
        };
        assert_eq!(exit_code(fail, false), 2);
        assert_eq!(exit_code(fail, true), 2);
        assert_eq!(exit_code(warn, false), 1);
        assert_eq!(exit_code(warn, true), 2);
        assert_eq!(exit_code(clean, false), 0);
    }

    #[test]
    fn overall() {
        assert_eq!(
            overall_status(Summary {
                fail: 1,
                ..Default::default()
            }),
            CheckStatus::Fail
        );
        assert_eq!(
            overall_status(Summary {
                warn: 1,
                ..Default::default()
            }),
            CheckStatus::Warn
        );
        assert_eq!(overall_status(Summary::default()), CheckStatus::Ok);
    }
}
