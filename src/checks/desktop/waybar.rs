//! Waybar local-script validation (part of check_desktop.c). Walks the
//! double-quoted JSON strings in the Waybar config, finds tokens that look like
//! local script paths, and FAILs on any that do not resolve.

use crate::model::{CheckResult, CheckStatus};
use crate::util::fs::{expand_home_path, file_exists, read_file};

const GROUP: &str = "Desktop";
const CONFIG_MAX_BYTES: usize = 1 << 20;
const SET_CAP: usize = 64;

fn looks_like_local_script(tok: &str) -> bool {
    !tok.is_empty()
        && (tok.starts_with("~/")
            || tok.starts_with("./")
            || tok.starts_with("$HOME/")
            || tok.starts_with("/home/")
            || tok.starts_with("scripts/")
            || tok.contains("/scripts/")
            || tok.contains("/.config/"))
}

fn strip_script_edges(s: &str) -> String {
    let b = s.as_bytes();
    let mut start = 0;
    while start < b.len() && matches!(b[start], b'"' | b'\'' | b'`') {
        start += 1;
    }
    let mut end = b.len();
    while end > start && matches!(b[end - 1], b'"' | b'\'' | b'`' | b';' | b',' | b'&' | b'|') {
        end -= 1;
    }
    s[start..end].to_string()
}

fn resolve_script(tok: &str, config_dir: &str) -> String {
    if tok.starts_with("$HOME/") {
        let home = std::env::var("HOME").unwrap_or_default();
        let rest = &tok[5..]; // keeps the leading '/'
        return format!("{home}{rest}");
    }
    if tok.starts_with('~') {
        return expand_home_path(tok);
    }
    if tok.starts_with('/') {
        return tok.to_string();
    }
    let rel = tok.strip_prefix("./").unwrap_or(tok);
    format!("{config_dir}/{rel}")
}

fn add_dedup(set: &mut Vec<String>, value: &str) {
    if set.iter().any(|m| m == value) || set.len() >= SET_CAP {
        return;
    }
    set.push(value.to_string());
}

fn scan_strings(content: &str, config_dir: &str, missing: &mut Vec<String>, checked: &mut i32) {
    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '"' {
            i += 1;
            continue;
        }
        let mut buf = String::new();
        i += 1; // opening quote
        while i < chars.len() && chars[i] != '"' {
            if chars[i] == '\\' && i + 1 < chars.len() {
                buf.push(chars[i + 1]);
                i += 2;
                continue;
            }
            buf.push(chars[i]);
            i += 1;
        }
        if i < chars.len() && chars[i] == '"' {
            i += 1; // closing quote
        }
        for t in buf.split([' ', '\t']) {
            if t.is_empty() {
                continue;
            }
            let tok = strip_script_edges(t);
            if !looks_like_local_script(&tok) {
                continue;
            }
            *checked += 1;
            if !file_exists(&resolve_script(&tok, config_dir)) {
                add_dedup(missing, &tok);
            }
        }
    }
}

pub fn check_scripts(results: &mut Vec<CheckResult>) {
    let jsonc = expand_home_path("~/.config/waybar/config.jsonc");
    let plain = expand_home_path("~/.config/waybar/config");
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
    let config_dir = std::path::Path::new(&cfg)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| ".".to_string());

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
    scan_strings(&content, &config_dir, &mut missing, &mut checked);

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
        results.push(CheckResult::new(
            "waybar-scripts",
            GROUP,
            CheckStatus::Fail,
            format!("Waybar config references missing script: {m}"),
            "",
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn edges_and_heuristic() {
        assert_eq!(strip_script_edges("\"~/x\";"), "~/x");
        assert_eq!(strip_script_edges("`scripts/a`"), "scripts/a");
        assert!(looks_like_local_script("scripts/a"));
        assert!(looks_like_local_script("~/.config/waybar/scripts/b"));
        assert!(!looks_like_local_script("waybar"));
    }

    #[test]
    fn scan_relative_scripts() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut dir = std::env::temp_dir();
        dir.push(format!("osdoctor_waybar_{}_{}", std::process::id(), nanos));
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        let mut f = std::fs::File::create(dir.join("scripts/present")).unwrap();
        f.write_all(b"#!/bin/sh\n").unwrap();

        // Two referenced scripts: one present, one missing.
        let content = r#"{ "custom/a": { "exec": "scripts/present" }, "custom/b": { "exec": "scripts/missing" } }"#;
        let mut missing = Vec::new();
        let mut checked = 0;
        scan_strings(content, dir.to_str().unwrap(), &mut missing, &mut checked);
        assert_eq!(checked, 2);
        assert_eq!(missing, vec!["scripts/missing".to_string()]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
