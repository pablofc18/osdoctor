//! Waybar local-script validation. Reads the Waybar config
//! (`$XDG_CONFIG_HOME/waybar/config.jsonc`, else `.../waybar/config`), and for
//! every command field (`exec`, `exec-*`, `on-*`) takes the program `sh -c`
//! would run. Programs that look like local scripts must exist; missing ones
//! FAIL, since the module is then broken.

use crate::model::{CheckResult, CheckStatus};
use crate::util::fs::{config_home, expand_home_path, file_exists, join_path, read_file};
use crate::util::shell::command_word;
use crate::util::strings::{expand_env_vars, push_unique_capped};

const GROUP: &str = "Desktop";
const CONFIG_MAX_BYTES: usize = 1 << 20;
const SET_CAP: usize = 64;

/// Whether a (variable-expanded) program path points at a user's own script
/// rather than an installed program found on PATH.
fn looks_like_local_script(prog: &str) -> bool {
    !prog.is_empty()
        && !prog.contains("://") // a URL (https://, file://), not a script on disk
        && (prog.starts_with("~/")
            || prog.starts_with("./")
            || prog.starts_with("/home/")
            || prog.starts_with("scripts/")
            || prog.contains("/scripts/")
            || prog.contains("/.config/"))
}

/// Waybar keys whose string value is a shell command: `exec`, `exec-if`,
/// `on-click*`, `on-scroll-*`, `on-update`, ...
fn is_command_key(key: &str) -> bool {
    key == "exec" || key.starts_with("exec-") || key.starts_with("on-")
}

/// Resolve a script path for an existence test; relative paths resolve
/// against the config directory.
fn resolve_script(prog: &str, config_dir: &str) -> String {
    if prog.starts_with('~') {
        return expand_home_path(prog);
    }
    if prog.starts_with('/') {
        return prog.to_string();
    }
    join_path(config_dir, prog.strip_prefix("./").unwrap_or(prog))
}

#[derive(Debug, PartialEq)]
enum Token {
    Str(String),
    Colon,
    /// Any other JSON punctuation or literal; only its position matters.
    Other,
}

/// Tokenize JSONC: strings (with escapes decoded), colons, and everything
/// else. `//` line and `/* */` block comments are dropped (only outside
/// strings; an unterminated block comment runs to the end of the input).
fn tokenize(content: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut it = content.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '/' if it.peek() == Some(&'/') => {
                for c2 in it.by_ref() {
                    if c2 == '\n' {
                        break;
                    }
                }
            }
            '/' if it.peek() == Some(&'*') => {
                it.next(); // the '*' of the opener
                let mut prev = '\0';
                for c2 in it.by_ref() {
                    if prev == '*' && c2 == '/' {
                        break;
                    }
                    prev = c2;
                }
            }
            '"' => {
                // A backslash escapes the next char; a trailing lone
                // backslash is kept.
                let mut buf = String::new();
                while let Some(c2) = it.next() {
                    match c2 {
                        '"' => break,
                        '\\' => buf.push(it.next().unwrap_or('\\')),
                        _ => buf.push(c2),
                    }
                }
                tokens.push(Token::Str(buf));
            }
            ':' => tokens.push(Token::Colon),
            c if c.is_whitespace() => {}
            _ => tokens.push(Token::Other),
        }
    }
    tokens
}

/// The command strings of `"<command key>": "<command>"` pairs in `content`.
fn command_values(content: &str) -> Vec<String> {
    let tokens = tokenize(content);
    let mut out = Vec::new();
    for w in tokens.windows(3) {
        if let [Token::Str(key), Token::Colon, Token::Str(value)] = w {
            if is_command_key(key) {
                out.push(value.clone());
            }
        }
    }
    out
}

fn scan_commands(content: &str, config_dir: &str, missing: &mut Vec<String>, checked: &mut i32) {
    for cmd in command_values(content) {
        // sh -c expands environment variables before running the program.
        let Some(prog) = command_word(&cmd) else {
            continue;
        };
        let expanded = expand_env_vars(prog);
        if expanded.contains(['$', '`']) || !looks_like_local_script(&expanded) {
            continue; // command substitution, or not a local script
        }
        *checked += 1;
        if !file_exists(&resolve_script(&expanded, config_dir)) {
            push_unique_capped(missing, prog, SET_CAP);
        }
    }
}

pub fn check_scripts(results: &mut Vec<CheckResult>) {
    let dir = join_path(&config_home(), "waybar");
    let jsonc = join_path(&dir, "config.jsonc");
    let plain = join_path(&dir, "config");
    let cfg = if file_exists(&jsonc) {
        jsonc
    } else if file_exists(&plain) {
        plain
    } else {
        results.push(CheckResult::new(
            "waybar-scripts",
            GROUP,
            CheckStatus::Skip,
            "Waybar config not found",
            "",
        ));
        return;
    };

    let content = match read_file(&cfg, CONFIG_MAX_BYTES) {
        Some(c) => c,
        None => {
            results.push(CheckResult::new(
                "waybar-scripts",
                GROUP,
                CheckStatus::Skip,
                "Could not read Waybar config",
                "",
            ));
            return;
        }
    };

    let mut missing: Vec<String> = Vec::new();
    let mut checked = 0;
    scan_commands(&content, &dir, &mut missing, &mut checked);

    if missing.is_empty() {
        let msg = if checked == 0 {
            "Waybar config present (no local scripts referenced)".to_string()
        } else {
            format!("Waybar scripts resolved ({checked} checked)")
        };
        results.push(CheckResult::new(
            "waybar-scripts",
            GROUP,
            CheckStatus::Ok,
            msg,
            "",
        ));
        return;
    }
    for m in &missing {
        results.push(CheckResult::problem(
            "waybar-scripts",
            GROUP,
            CheckStatus::Fail,
            format!("Waybar config references missing script: {m}"),
            "",
            "Your Waybar config runs a local script that does not exist, so the module using \
it shows nothing or stops responding. Relative paths are resolved from the Waybar config \
directory; check whether the script was moved, renamed or never copied there.",
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unique_dir;

    fn scan(content: &str, config_dir: &str) -> (Vec<String>, i32) {
        let mut missing = Vec::new();
        let mut checked = 0;
        scan_commands(content, config_dir, &mut missing, &mut checked);
        (missing, checked)
    }

    #[test]
    fn local_script_heuristic() {
        assert!(looks_like_local_script("scripts/a"));
        assert!(looks_like_local_script("~/.config/waybar/scripts/b"));
        assert!(looks_like_local_script("/home/u/bin/c"));
        assert!(!looks_like_local_script("waybar"));
        assert!(!looks_like_local_script("/usr/bin/pavucontrol"));
        // URLs are opened, not executed from disk.
        assert!(!looks_like_local_script("https://example.com/scripts/x"));
        assert!(!looks_like_local_script(
            "file:///home/u/.config/waybar/scripts/x"
        ));
    }

    #[test]
    fn command_keys() {
        for k in [
            "exec",
            "exec-if",
            "on-click",
            "on-click-right",
            "on-scroll-up",
        ] {
            assert!(is_command_key(k), "{k}");
        }
        for k in [
            "format",
            "tooltip-format",
            "interval",
            "executable",
            "return-type",
        ] {
            assert!(!is_command_key(k), "{k}");
        }
    }

    #[test]
    fn scan_relative_scripts() {
        let dir = unique_dir("waybar");
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::write(dir.join("scripts/present"), "#!/bin/sh\n").unwrap();

        // Two referenced scripts: one present, one missing.
        let content = r#"{ "custom/a": { "exec": "scripts/present" }, "custom/b": { "exec": "./scripts/missing --flag" } }"#;
        let (missing, checked) = scan(content, dir.to_str().unwrap());
        assert_eq!(checked, 2);
        assert_eq!(missing, vec!["./scripts/missing".to_string()]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn only_the_program_of_command_fields_is_checked() {
        // Arguments, data files, and non-command fields are not scripts.
        let content = r#"{
          "exec": "cat /home/u/.cache/weather.txt",
          "exec-if": "test -f ~/.config/waybar/scripts/optional",
          "tooltip-format": "see ~/.config/waybar/scripts/readme",
          "format": "scripts/not-a-command",
          "on-click": "GDK_BACKEND=x11 ~/.config/waybar/scripts/missing-click; notify-send done",
          "on-scroll-up": "pamixer -i 5"
        }"#;
        let (missing, checked) = scan(content, "/nonexistent");
        assert_eq!(checked, 1);
        assert_eq!(
            missing,
            vec!["~/.config/waybar/scripts/missing-click".to_string()]
        );
    }

    #[test]
    fn env_vars_expand_like_sh() {
        let _g = crate::ENV_MUTEX.lock().unwrap();
        let dir = unique_dir("waybar_env");
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::write(dir.join("scripts/bat"), "#!/bin/sh\n").unwrap();
        std::env::set_var("OSD_TEST_WAYBAR", dir.to_str().unwrap());

        let content = r#"{
          "a": { "exec": "$OSD_TEST_WAYBAR/scripts/bat" },
          "b": { "exec": "${OSD_TEST_WAYBAR}/scripts/bat" },
          "c": { "exec": "${OSD_TEST_WAYBAR}/scripts/gone" },
          "d": { "exec": "$(~/.config/waybar/scripts/bat)" },
          "e": { "exec": "`~/.config/waybar/scripts/bat`" }
        }"#;
        let (missing, checked) = scan(content, "/nonexistent");
        // $(...) and backticks are command substitutions: not resolvable.
        assert_eq!(checked, 3);
        assert_eq!(missing, vec!["${OSD_TEST_WAYBAR}/scripts/gone".to_string()]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_script_row_is_explained() {
        let _g = crate::ENV_MUTEX.lock().unwrap();
        let dir = unique_dir("waybar_explain");
        std::fs::create_dir_all(dir.join("waybar")).unwrap();
        std::fs::write(
            dir.join("waybar/config.jsonc"),
            r#"{ "custom/x": { "exec": "scripts/osd_gone" } }"#,
        )
        .unwrap();
        let saved = std::env::var_os("XDG_CONFIG_HOME");
        std::env::set_var("XDG_CONFIG_HOME", &dir);
        let mut r = Vec::new();
        check_scripts(&mut r);
        match saved {
            Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].status, CheckStatus::Fail);
        assert!(r[0].explain.contains("Waybar config"), "{}", r[0].explain);
    }

    #[test]
    fn jsonc_comments_ignored() {
        // Waybar configs are JSONC: commands inside // and /* */ comments are
        // not referenced. Comment markers inside strings are kept.
        let content = r#"// "exec": "scripts/line-comment"
{
  /* "exec": "scripts/block-comment",
     "on-click": "scripts/block-comment-2" */
  "exec": "scripts/real", // "exec": "scripts/trailing"
  "on-click": "scripts/kept // not-a-comment", /**/ "exec-if": "scripts/after-empty-block"
}
/* unterminated "exec": "scripts/unterminated""#;
        let (missing, checked) = scan(content, "/nonexistent");
        assert_eq!(
            missing,
            vec![
                "scripts/real".to_string(),
                "scripts/kept".to_string(),
                "scripts/after-empty-block".to_string(),
            ]
        );
        assert_eq!(checked, 3);
    }

    #[test]
    fn escapes_decode() {
        let tokens = tokenize(r#"{"a\"b": "c\\d", "e": "trailing\"#);
        assert_eq!(
            tokens,
            vec![
                Token::Other,
                Token::Str("a\"b".into()),
                Token::Colon,
                Token::Str("c\\d".into()),
                Token::Other,
                Token::Str("e".into()),
                Token::Colon,
                Token::Str("trailing\\".into()), // lone trailing backslash kept
            ]
        );
    }
}
