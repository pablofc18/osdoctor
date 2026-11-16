//! Human-readable grouped report. `render_text` is
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

/// Greedy word wrap on spaces. A word longer than `width` stays whole on its
/// own line.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split(' ').filter(|w| !w.is_empty()) {
        match lines.last_mut() {
            Some(l) if l.chars().count() + 1 + word.chars().count() <= width => {
                l.push(' ');
                l.push_str(word);
            }
            _ => lines.push(word.to_string()),
        }
    }
    lines
}

/// Width available to an explanation: 80 columns minus the 8-character
/// indentation.
const EXPLAIN_WIDTH: usize = 72;

pub fn render_text(results: &[CheckResult], color: bool, explain: bool) -> String {
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
        out.push_str(&format!(
            "  {}{}{} {}\n",
            col,
            r.status.symbol(),
            reset,
            r.message
        ));
        if !r.detail.is_empty() {
            out.push_str(&format!("      {}{}{}\n", dim, r.detail, reset));
        }
        if explain && !r.explain.is_empty() {
            for (n, line) in wrap(r.explain, EXPLAIN_WIDTH).iter().enumerate() {
                let (indent, marker) = if n == 0 {
                    ("      ", "? ")
                } else {
                    ("        ", "")
                };
                out.push_str(&format!("{indent}{dim}{marker}{line}{reset}\n"));
            }
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

pub fn print_text(results: &[CheckResult], explain: bool) -> std::io::Result<()> {
    super::write_stdout(&render_text(results, use_color(), explain))
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
            CheckResult::problem("w", "Desktop", CheckStatus::Warn, "missing thing", "", "E."),
        ];
        let expected = "System\n  ✓ Kernel ok\n      Linux x\n\nDesktop\n  ! missing thing\n\nSummary\n  Status: WARN\n  Checks: 1 ok, 1 warning, 0 failures\n";
        assert_eq!(render_text(&results, false, false), expected);
    }

    #[test]
    fn skipped_count_appended() {
        let results = vec![CheckResult::new(
            "s",
            "System",
            CheckStatus::Skip,
            "skipped",
            "",
        )];
        let out = render_text(&results, false, false);
        assert!(out.contains("  Checks: 0 ok, 0 warnings, 0 failures, 1 skipped\n"));
    }

    const EXPLANATION: &str = "This usually means the package database was left in an unclean state after an interrupted update. Check whether another package manager is running before doing anything, and review the pacman log for the last operation that ran.";

    fn problem_rows() -> Vec<CheckResult> {
        vec![CheckResult::problem(
            "p",
            "Packages",
            CheckStatus::Fail,
            "broken thing",
            "d",
            EXPLANATION,
        )]
    }

    #[test]
    fn explain_renders_after_detail_wrapped_at_80() {
        let rows = problem_rows();
        let out = render_text(&rows, false, true);
        let lines: Vec<&str> = out.lines().collect();
        // group header, row, detail, then the explanation
        assert_eq!(lines[2], "      d");
        assert!(lines[3].starts_with("      ? "));
        let expl: Vec<&str> = lines[3..]
            .iter()
            .take_while(|l| l.starts_with("      ? ") || l.starts_with("        "))
            .copied()
            .collect();
        assert!(expl.len() >= 3);
        assert!(expl.iter().all(|l| l.len() <= 80));
        assert!(expl[1..]
            .iter()
            .all(|l| l.starts_with("        ") && !l.starts_with("         ")));
        let words: Vec<&str> = expl
            .iter()
            .flat_map(|l| l.trim_start().trim_start_matches("? ").split(' '))
            .collect();
        assert_eq!(words.join(" "), EXPLANATION);
    }

    #[test]
    fn explain_hidden_without_flag() {
        let rows = problem_rows();
        assert_eq!(render_text(&rows, false, false).matches("? ").count(), 0);
    }

    #[test]
    fn explain_lines_reset_color() {
        let rows = problem_rows();
        let out = render_text(&rows, true, true);
        let mut n = 0;
        for l in out
            .lines()
            .filter(|l| l.contains("? ") || l.starts_with("        "))
        {
            n += 1;
            assert!(l.ends_with("\x1b[0m"), "{l:?}");
        }
        assert!(n >= 3);
    }

    #[test]
    fn wrap_keeps_long_words_whole() {
        assert_eq!(wrap("a bbbbbbbbbb c", 5), vec!["a", "bbbbbbbbbb", "c"]);
    }
}
