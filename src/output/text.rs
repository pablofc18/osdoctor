//! Human-readable grouped report (replaces output_text.c). `render_text` is
//! pure (color is a parameter) so it can be tested; `print_text` decides color
//! from the TTY + NO_COLOR and writes to stdout.

use crate::model::{overall_status, summarize, CheckResult, CheckStatus};

fn color_for(status: CheckStatus) -> &'static str {
    match status {
        CheckStatus::Ok => "\x1b[32m",
        CheckStatus::Warn => "\x1b[33m",
        CheckStatus::Fail => "\x1b[31m",
        CheckStatus::Skip => "\x1b[90m",
    }
}

pub fn render_text(results: &[CheckResult], color: bool) -> String {
    let reset = if color { "\x1b[0m" } else { "" };
    let dim = if color { "\x1b[90m" } else { "" };

    let mut out = String::new();
    let mut prev_group: Option<&str> = None;
    for (i, r) in results.iter().enumerate() {
        if prev_group != Some(r.group) {
            out.push_str(if i == 0 { "" } else { "\n" });
            out.push_str(r.group);
            out.push('\n');
            prev_group = Some(r.group);
        }
        let col = if color { color_for(r.status) } else { "" };
        out.push_str(&format!("  {}{}{} {}\n", col, r.status.symbol(), reset, r.message));
        if !r.detail.is_empty() {
            out.push_str(&format!("      {}{}{}\n", dim, r.detail, reset));
        }
    }

    let s = summarize(results);
    let overall = overall_status(s);
    let ocol = if color { color_for(overall) } else { "" };

    let mut counts = format!(
        "{} ok, {} warning{}, {} failure{}",
        s.ok,
        s.warn,
        if s.warn == 1 { "" } else { "s" },
        s.fail,
        if s.fail == 1 { "" } else { "s" }
    );
    if s.skip > 0 {
        counts.push_str(&format!(", {} skipped", s.skip));
    }

    out.push_str("\nSummary\n");
    out.push_str(&format!("  Status: {}{}{}\n", ocol, overall.label(), reset));
    out.push_str(&format!("  Checks: {}\n", counts));
    out
}

pub fn print_text(results: &[CheckResult]) {
    print!("{}", render_text(results, use_color()));
}

fn use_color() -> bool {
    use std::io::IsTerminal;
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    std::io::stdout().is_terminal()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CheckResult, CheckStatus};

    #[test]
    fn plain_render_groups_and_summary() {
        let results = vec![
            CheckResult::new("k", "System", CheckStatus::Ok, "Kernel ok", "Linux x"),
            CheckResult::new("w", "Desktop", CheckStatus::Warn, "missing thing", ""),
        ];
        let expected = "System\n  ✓ Kernel ok\n      Linux x\n\nDesktop\n  ! missing thing\n\nSummary\n  Status: WARN\n  Checks: 1 ok, 1 warning, 0 failures\n";
        assert_eq!(render_text(&results, false), expected);
    }

    #[test]
    fn skipped_count_appended() {
        let results = vec![CheckResult::new("s", "System", CheckStatus::Skip, "skipped", "")];
        let out = render_text(&results, false);
        assert!(out.contains("  Checks: 0 ok, 0 warnings, 0 failures, 1 skipped\n"));
    }
}
