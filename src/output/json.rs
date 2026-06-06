//! Machine-readable JSON report (replaces output_json.c). `render_json` builds
//! the exact byte sequence; `print_json` writes it to stdout.

use crate::model::{overall_status, summarize, CheckResult};
use crate::util::strings::json_escape;

pub fn render_json(results: &[CheckResult]) -> String {
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
        out.push_str("\n        }");
    }

    if cur_group.is_some() {
        out.push_str("\n      ]\n    }");
    }
    out.push_str("\n  ]\n");
    out.push_str("}\n");
    out
}

pub fn print_json(results: &[CheckResult]) {
    print!("{}", render_json(results));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CheckResult, CheckStatus};

    #[test]
    fn empty_results() {
        let out = render_json(&[]);
        assert_eq!(
            out,
            "{\n  \"status\": \"ok\",\n  \"summary\": {\n    \"ok\": 0,\n    \"warn\": 0,\n    \"fail\": 0,\n    \"skip\": 0\n  },\n  \"groups\": [\n  ]\n}\n"
        );
    }

    #[test]
    fn one_group_two_checks_with_detail() {
        let results = vec![
            CheckResult::new("a-id", "System", CheckStatus::Ok, "msg a", "det a"),
            CheckResult::new("b-id", "System", CheckStatus::Warn, "msg b", ""),
        ];
        let expected = "{\n  \"status\": \"warn\",\n  \"summary\": {\n    \"ok\": 1,\n    \"warn\": 1,\n    \"fail\": 0,\n    \"skip\": 0\n  },\n  \"groups\": [\n    {\n      \"name\": \"System\",\n      \"checks\": [\n        {\n          \"id\": \"a-id\",\n          \"status\": \"ok\",\n          \"message\": \"msg a\",\n          \"detail\": \"det a\"\n        },\n        {\n          \"id\": \"b-id\",\n          \"status\": \"warn\",\n          \"message\": \"msg b\"\n        }\n      ]\n    }\n  ]\n}\n";
        assert_eq!(render_json(&results), expected);
    }
}
