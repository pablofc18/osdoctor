//! Hyprland exec-bind validation (part of check_desktop.c). Walks
//! ~/.config/hypr/hyprland.conf and every file it transitively `source =`s,
//! resolving ~, $VAR, relative and glob includes, with cycle and depth guards.

use crate::model::{CheckResult, CheckStatus};
use crate::util::fs::{
    expand_home_path, file_exists, glob_paths, is_executable_in_path, join_path, read_file,
};

const GROUP: &str = "Desktop";
pub const MAX_INCLUDE_DEPTH: i32 = 32;
const VISITED_CAP: usize = 256;
const SET_CAP: usize = 64;
const CONFIG_MAX_BYTES: usize = 1 << 20;

#[derive(Default)]
pub struct BindScan {
    pub missing: Vec<String>,
    pub checked: i32,
}

impl BindScan {
    fn add_missing(&mut self, value: &str) {
        if self.missing.iter().any(|m| m == value) || self.missing.len() >= SET_CAP {
            return;
        }
        self.missing.push(value.to_string());
    }
}

fn first_token(s: &str) -> String {
    s.trim_start_matches([' ', '\t'])
        .split([' ', '\t'])
        .next()
        .unwrap_or("")
        .to_string()
}

fn strip_quotes(s: &str) -> String {
    let b = s.as_bytes();
    let n = b.len();
    if n >= 2 && (b[0] == b'"' || b[0] == b'\'') && b[n - 1] == b[0] {
        s[1..n - 1].to_string()
    } else {
        s.to_string()
    }
}

fn token_is_checkable(tok: &str) -> bool {
    !tok.is_empty() && !tok.starts_with('$') && !tok.contains('$') && !tok.contains('`')
}

/// If `kw` (trimmed text before '=') is a bind keyword, return Some(has_desc),
/// where has_desc is true when the flags after "bind" include 'd'.
fn bind_has_desc(kw: &str) -> Option<bool> {
    if !kw.starts_with("bind") || !kw.bytes().all(|c| c.is_ascii_lowercase()) {
        return None;
    }
    Some(kw[4..].contains('d'))
}

/// Start of comma-field `n` (0-based) within `s`, or None if too few fields.
fn nth_field(s: &str, n: usize) -> Option<&str> {
    let mut p = s;
    for _ in 0..n {
        let idx = p.find(',')?;
        p = &p[idx + 1..];
    }
    Some(p)
}

/// If `line` is `source = <path>`, return the trimmed, unquoted value.
fn parse_source_value(line: &str) -> Option<String> {
    if !line.starts_with("source") {
        return None;
    }
    let eq = line.find('=')?;
    if line[..eq].trim() != "source" {
        return None;
    }
    Some(strip_quotes(line[eq + 1..].trim()))
}

/// Expand $VAR / ${VAR} from the environment; undefined -> empty, lone '$' kept.
fn expand_env_vars(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '$' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        i += 1;
        let braced = i < chars.len() && chars[i] == '{';
        if braced {
            i += 1;
        }
        let mut name = String::new();
        while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
            name.push(chars[i]);
            i += 1;
        }
        if braced && i < chars.len() && chars[i] == '}' {
            i += 1;
        }
        if name.is_empty() {
            out.push('$');
            continue;
        }
        if let Ok(val) = std::env::var(&name) {
            out.push_str(&val);
        }
    }
    out
}

fn process_bind_line(line: &str, scan: &mut BindScan) {
    if !line.starts_with("bind") {
        return;
    }
    let eq = match line.find('=') {
        Some(i) => i,
        None => return,
    };
    let has_desc = match bind_has_desc(line[..eq].trim()) {
        Some(d) => d,
        None => return,
    };
    let rhs = &line[eq + 1..];
    let disp = match nth_field(rhs, if has_desc { 3 } else { 2 }) {
        Some(d) => d,
        None => return,
    };
    let disp_end = match disp.find(',') {
        Some(i) => i,
        None => return,
    };
    let dispatcher = disp[..disp_end].trim();
    if dispatcher != "exec" && dispatcher != "execr" {
        return;
    }
    let tok = strip_quotes(&first_token(&disp[disp_end + 1..]));
    if !token_is_checkable(&tok) {
        return;
    }
    scan.checked += 1;
    if !is_executable_in_path(&expand_home_path(&tok)) {
        scan.add_missing(&tok);
    }
}

fn scan_source(
    value: &str,
    base_dir: &str,
    scan: &mut BindScan,
    visited: &mut Vec<String>,
    depth: i32,
) {
    let expanded = expand_env_vars(value);
    let pattern = if expanded.starts_with('~') {
        expand_home_path(&expanded)
    } else if expanded.starts_with('/') {
        expanded
    } else {
        join_path(base_dir, &expanded)
    };
    for path in glob_paths(&pattern) {
        scan_config_file(&path, scan, visited, depth + 1);
    }
}

fn scan_config_file(path: &str, scan: &mut BindScan, visited: &mut Vec<String>, depth: i32) {
    if depth > MAX_INCLUDE_DEPTH {
        return;
    }
    let canon = match std::fs::canonicalize(path) {
        Ok(c) => c,
        Err(_) => return, // missing / unreadable include - skip silently
    };
    let canon_str = canon.to_string_lossy().into_owned();
    if visited.iter().any(|v| v == &canon_str) || visited.len() >= VISITED_CAP {
        return;
    }
    visited.push(canon_str.clone());

    let content = match read_file(&canon_str, CONFIG_MAX_BYTES) {
        Some(c) => c,
        None => return,
    };
    let base_dir = canon
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| ".".to_string());

    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(value) = parse_source_value(line) {
            scan_source(&value, &base_dir, scan, visited, depth);
            continue;
        }
        process_bind_line(line, scan);
    }
}

pub fn scan_binds(root_path: &str) -> BindScan {
    let mut scan = BindScan::default();
    let mut visited: Vec<String> = Vec::new();
    scan_config_file(root_path, &mut scan, &mut visited, 0);
    scan
}

pub fn check_binds(results: &mut Vec<CheckResult>) {
    let cfg = expand_home_path("~/.config/hypr/hyprland.conf");
    if !file_exists(&cfg) {
        results.push(CheckResult::new(
            "hyprland-binds",
            GROUP,
            CheckStatus::Skip,
            "Hyprland config not found",
            "~/.config/hypr/hyprland.conf",
        ));
        return;
    }
    let scan = scan_binds(&cfg);
    if scan.missing.is_empty() {
        results.push(CheckResult::new(
            "hyprland-binds",
            GROUP,
            CheckStatus::Ok,
            format!("Hyprland bind commands resolved ({} checked)", scan.checked),
            "",
        ));
        return;
    }
    for m in &scan.missing {
        results.push(CheckResult::new(
            "hyprland-binds",
            GROUP,
            CheckStatus::Warn,
            format!("Hyprland bind references missing command: {m}"),
            "",
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn unique_dir(tag: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut p = std::env::temp_dir();
        p.push(format!(
            "osdoctor_hypr_{}_{}_{}",
            tag,
            std::process::id(),
            nanos
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn write(path: &std::path::Path, content: &str) {
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }

    fn has_missing(scan: &BindScan, cmd: &str) -> bool {
        scan.missing.iter().any(|m| m == cmd)
    }

    #[test]
    fn root_binds() {
        let dir = unique_dir("root");
        let root = dir.join("hyprland.conf");
        write(
            &root,
            "# comment\nbind = SUPER, Return, exec, sh\nbind = SUPER, X, exec, osd_bogus_root\n",
        );
        let scan = scan_binds(root.to_str().unwrap());
        assert_eq!(scan.checked, 2);
        assert!(has_missing(&scan, "osd_bogus_root"));
        assert!(!has_missing(&scan, "sh"));
        assert_eq!(scan.missing.len(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn relative_include() {
        let dir = unique_dir("rel");
        write(&dir.join("hyprland.conf"), "source = binds.conf\n");
        write(
            &dir.join("binds.conf"),
            "bind = SUPER, Return, exec, sh\nbind = SUPER, X, exec, osd_bogus_inc\n",
        );
        let scan = scan_binds(dir.join("hyprland.conf").to_str().unwrap());
        assert_eq!(scan.checked, 2);
        assert!(has_missing(&scan, "osd_bogus_inc"));
        assert_eq!(scan.missing.len(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn tilde_include() {
        let _g = crate::ENV_MUTEX.lock().unwrap();
        let dir = unique_dir("tilde");
        std::env::set_var("HOME", &dir);
        std::fs::create_dir_all(dir.join("inc")).unwrap();
        write(&dir.join("hyprland.conf"), "source = ~/inc/sub.conf\n");
        write(
            &dir.join("inc/sub.conf"),
            "bind = SUPER, A, exec, osd_bogus_tilde\n",
        );
        let scan = scan_binds(dir.join("hyprland.conf").to_str().unwrap());
        assert_eq!(scan.checked, 1);
        assert!(has_missing(&scan, "osd_bogus_tilde"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn envvar_include() {
        let _g = crate::ENV_MUTEX.lock().unwrap();
        let dir = unique_dir("env");
        std::env::set_var("OSD_TEST_ENVDIR", &dir);
        write(
            &dir.join("hyprland.conf"),
            "source = $OSD_TEST_ENVDIR/env.conf\nsource = ${OSD_TEST_ENVDIR}/env2.conf\n",
        );
        write(
            &dir.join("env.conf"),
            "bind = SUPER, B, exec, osd_bogus_env1\n",
        );
        write(
            &dir.join("env2.conf"),
            "bind = SUPER, C, exec, osd_bogus_env2\n",
        );
        let scan = scan_binds(dir.join("hyprland.conf").to_str().unwrap());
        assert_eq!(scan.checked, 2);
        assert!(has_missing(&scan, "osd_bogus_env1"));
        assert!(has_missing(&scan, "osd_bogus_env2"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn glob_include() {
        let dir = unique_dir("glob");
        std::fs::create_dir_all(dir.join("parts")).unwrap();
        write(&dir.join("hyprland.conf"), "source = parts/*.conf\n");
        write(
            &dir.join("parts/a.conf"),
            "bind = SUPER, A, exec, osd_bogus_glob_a\n",
        );
        write(
            &dir.join("parts/b.conf"),
            "bind = SUPER, B, exec, osd_bogus_glob_b\n",
        );
        let scan = scan_binds(dir.join("hyprland.conf").to_str().unwrap());
        assert_eq!(scan.checked, 2);
        assert!(has_missing(&scan, "osd_bogus_glob_a"));
        assert!(has_missing(&scan, "osd_bogus_glob_b"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cycle_terminates() {
        let dir = unique_dir("cycle");
        write(
            &dir.join("a.conf"),
            "source = b.conf\nbind = SUPER, A, exec, osd_bogus_cyc_a\n",
        );
        write(
            &dir.join("b.conf"),
            "source = a.conf\nbind = SUPER, B, exec, osd_bogus_cyc_b\n",
        );
        let scan = scan_binds(dir.join("a.conf").to_str().unwrap());
        assert_eq!(scan.checked, 2);
        assert!(has_missing(&scan, "osd_bogus_cyc_a"));
        assert!(has_missing(&scan, "osd_bogus_cyc_b"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn depth_guard() {
        let dir = unique_dir("depth");
        let chain = MAX_INCLUDE_DEPTH + 5;
        for i in 0..chain {
            let content = if i + 1 < chain {
                format!(
                    "source = f{}.conf\nbind = SUPER, A, exec, osd_bogus_d{}\n",
                    i + 1,
                    i
                )
            } else {
                format!("bind = SUPER, A, exec, osd_bogus_d{i}\n")
            };
            write(&dir.join(format!("f{i}.conf")), &content);
        }
        let scan = scan_binds(dir.join("f0.conf").to_str().unwrap());
        assert_eq!(scan.checked, MAX_INCLUDE_DEPTH + 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn bindd_description_field() {
        let dir = unique_dir("bindd");
        write(
            &dir.join("hyprland.conf"),
            "bindd = SUPER, Return, Terminal, exec, sh\nbindd = SUPER, X, Some App, exec, osd_bogus_bindd\nbindde = SUPER, Y, Repeating, exec, osd_bogus_bindde\n",
        );
        let scan = scan_binds(dir.join("hyprland.conf").to_str().unwrap());
        assert_eq!(scan.checked, 3);
        assert!(has_missing(&scan, "osd_bogus_bindd"));
        assert!(has_missing(&scan, "osd_bogus_bindde"));
        assert!(!has_missing(&scan, "sh"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_source_skipped() {
        let dir = unique_dir("missing");
        write(
            &dir.join("hyprland.conf"),
            "source = does_not_exist.conf\nbind = SUPER, Z, exec, osd_bogus_after\n",
        );
        let scan = scan_binds(dir.join("hyprland.conf").to_str().unwrap());
        assert_eq!(scan.checked, 1);
        assert!(has_missing(&scan, "osd_bogus_after"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
