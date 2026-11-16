//! Machine-readable JSON report. `render_json` builds
//! the exact byte sequence; `print_json` writes it to stdout.

use crate::model::{overall_status, summarize, CheckResult};
use crate::util::strings::json_escape;

pub fn render_json(results: &[CheckResult], explain: bool) -> String {
    let s = summarize(results);
    let mut out = String::new();
    out.push_str("{\n");
    out.push_str(&format!(
        "  \"status\": \"{}\",\n",
        overall_status(s).json()
    ));
    out.push_str("  \"summary\": {\n");
    out.push_str(&format!("    \"ok\": {},\n", s.ok));
    out.push_str(&format!("    \"warn\": {},\n", s.warn));
    out.push_str(&format!("    \"fail\": {},\n", s.fail));
    out.push_str(&format!("    \"skip\": {}\n", s.skip));
    out.push_str("  },\n");
    out.push_str("  \"groups\": [");

    let mut cur_group: Option<&str> = None;
    for r in results {
        if cur_group != Some(r.group) {
            if cur_group.is_some() {
                out.push_str("\n      ]\n    },");
            }
            out.push_str("\n    {\n      \"name\": \"");
            out.push_str(&json_escape(r.group));
            out.push_str("\",\n      \"checks\": [");
            cur_group = Some(r.group);
        } else {
            out.push(',');
        }

        out.push_str("\n        {\n");
        out.push_str("          \"id\": \"");
        out.push_str(&json_escape(r.id));
        out.push_str("\",\n");
        out.push_str(&format!("          \"status\": \"{}\",\n", r.status.json()));
        out.push_str("          \"message\": \"");
        out.push_str(&json_escape(&r.message));
        out.push('"');
        if !r.detail.is_empty() {
            out.push_str(",\n          \"detail\": \"");
            out.push_str(&json_escape(&r.detail));
            out.push('"');
        }
        if explain && !r.explain.is_empty() {
            out.push_str(",\n          \"explain\": \"");
            out.push_str(&json_escape(r.explain));
            out.push('"');
        }
        out.push_str("\n        }");
    }

    if cur_group.is_some() {
        out.push_str("\n      ]\n    }");
    }
    out.push_str("\n  ]\n");
    out.push_str("}\n");
    out
}

pub fn print_json(results: &[CheckResult], explain: bool) -> std::io::Result<()> {
    super::write_stdout(&render_json(results, explain))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CheckResult, CheckStatus};

    #[test]
    fn empty_results() {
        let out = render_json(&[], false);
        assert_eq!(
            out,
            "{\n  \"status\": \"ok\",\n  \"summary\": {\n    \"ok\": 0,\n    \"warn\": 0,\n    \"fail\": 0,\n    \"skip\": 0\n  },\n  \"groups\": [\n  ]\n}\n"
        );
    }

    #[test]
    fn one_group_two_checks_with_detail() {
        let results = vec![
            CheckResult::new("a-id", "System", CheckStatus::Ok, "msg a", "det a"),
            CheckResult::problem("b-id", "System", CheckStatus::Warn, "msg b", "", "E."),
        ];
        let expected = "{\n  \"status\": \"warn\",\n  \"summary\": {\n    \"ok\": 1,\n    \"warn\": 1,\n    \"fail\": 0,\n    \"skip\": 0\n  },\n  \"groups\": [\n    {\n      \"name\": \"System\",\n      \"checks\": [\n        {\n          \"id\": \"a-id\",\n          \"status\": \"ok\",\n          \"message\": \"msg a\",\n          \"detail\": \"det a\"\n        },\n        {\n          \"id\": \"b-id\",\n          \"status\": \"warn\",\n          \"message\": \"msg b\"\n        }\n      ]\n    }\n  ]\n}\n";
        assert_eq!(render_json(&results, false), expected);
    }

    fn warn_row(explain: &'static str) -> Vec<CheckResult> {
        vec![CheckResult::problem(
            "w",
            "System",
            CheckStatus::Warn,
            "m",
            "d",
            explain,
        )]
    }

    #[test]
    fn json_explain_only_with_flag() {
        let rows = warn_row("Because.");
        assert!(!render_json(&rows, false).contains("explain"));
        assert!(render_json(&rows, true).contains(",\n          \"explain\": \"Because.\""));
    }

    #[test]
    fn json_explain_is_escaped() {
        let rows = warn_row(r#"Say "hi" \ bye."#);
        assert!(render_json(&rows, true).contains(r#""explain": "Say \"hi\" \\ bye.""#));
    }

    #[test]
    fn explain_without_problems_is_identical() {
        let ok_rows = vec![
            CheckResult::new("a", "System", CheckStatus::Ok, "m", "d"),
            CheckResult::new("b", "System", CheckStatus::Skip, "m", ""),
        ];
        assert_eq!(render_json(&ok_rows, true), render_json(&ok_rows, false));
        assert_eq!(
            crate::output::text::render_text(&ok_rows, false, true),
            crate::output::text::render_text(&ok_rows, false, false)
        );
    }
}
