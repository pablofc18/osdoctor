//! Hyprland exec-bind validation. Walks `$XDG_CONFIG_HOME/hypr/hyprland.conf`
//! (default `~/.config/hypr/hyprland.conf`) and every file it transitively
//! `source =`s, resolving Hyprland `$variables`, environment variables, `~`,
//! relative and glob includes, with cycle and depth guards. Each exec bind's
//! program (as `sh -c` would run it) must resolve to an executable.

use crate::model::{CheckResult, CheckStatus};
use crate::util::fs::{
    config_home, display_path, expand_home_path, file_exists, glob_paths, is_executable_from,
    join_path, read_file,
};
use crate::util::shell::command_word;
use crate::util::strings::{expand_env_vars, push_unique_capped};
use std::collections::HashMap;
use std::path::Path;

const GROUP: &str = "Desktop";
pub const MAX_INCLUDE_DEPTH: i32 = 32;
const VISITED_CAP: usize = 256;
const SET_CAP: usize = 64;
const CONFIG_MAX_BYTES: usize = 1 << 20;

#[derive(Default)]
pub struct BindScan {
    pub missing: Vec<String>,
    pub checked: i32,
    /// Whether the root config itself could be read. False means the scan saw
    /// nothing, which must not be reported as "all binds resolved".
    pub root_read: bool,
    /// Memoized PATH-resolution results, so a command bound many times only
    /// walks PATH once.
    resolved: HashMap<String, bool>,
    /// Hyprland `$name = value` variables defined so far, in config order.
    vars: HashMap<String, String>,
    /// Directory relative commands resolve against: Hyprland runs exec binds
    /// from its own working directory, which is the user's home.
    home: String,
}

impl BindScan {
    fn add_missing(&mut self, value: &str) {
        push_unique_capped(&mut self.missing, value, SET_CAP);
    }

    /// Whether `tok` resolves to an executable, caching the answer per token.
    fn resolves(&mut self, tok: &str) -> bool {
        if let Some(&known) = self.resolved.get(tok) {
            return known;
        }
        let ok = is_executable_from(&expand_home_path(tok), &self.home);
        self.resolved.insert(tok.to_string(), ok);
        ok
    }

    /// Replace each `$name` that is a defined Hyprland variable with its value
    /// (the longest identifier after '$' is the name). Anything else, such as
    /// environment variables, is left for the shell-style expansion.
    fn expand_vars(&self, input: &str) -> String {
        let mut out = String::with_capacity(input.len());
        let mut rest = input;
        while let Some(i) = rest.find('$') {
            out.push_str(&rest[..i]);
            let after = &rest[i + 1..];
            let len = after
                .find(|c: char| !(c == '_' || c.is_ascii_alphanumeric()))
                .unwrap_or(after.len());
            match self.vars.get(&after[..len]) {
                Some(value) if len > 0 => out.push_str(value),
                _ => out.push_str(&rest[i..i + 1 + len]),
            }
            rest = &after[len..];
        }
        out.push_str(rest);
        out
    }

    /// Record a `$name = value` definition. Returns false if `line` is not one.
    fn define_var(&mut self, line: &str) -> bool {
        let Some(def) = line.strip_prefix('$') else {
            return false;
        };
        let Some((name, value)) = def.split_once('=') else {
            return false;
        };
        let name = name.trim();
        if name.is_empty() || !name.chars().all(|c| c == '_' || c.is_ascii_alphanumeric()) {
            return false;
        }
        let value = self.expand_vars(value.trim());
        self.vars.insert(name.to_string(), value);
        true
    }
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
    // `exec, [rules] cmd` launches cmd with window rules: the command starts
    // after the closing bracket. An unterminated rule block has no command.
    let mut args = disp[disp_end + 1..].trim_start_matches([' ', '\t']);
    if let Some(rest) = args.strip_prefix('[') {
        match rest.find(']') {
            Some(i) => args = &rest[i + 1..],
            None => return,
        }
    }
    // Hyprland substitutes its $variables, then runs the result via sh -c,
    // which expands environment variables and finds the program word.
    let cmd = expand_env_vars(&scan.expand_vars(args));
    let tok = match command_word(&cmd) {
        Some(t) => t.to_string(),
        None => return,
    };
    if !token_is_checkable(&tok) {
        return;
    }
    scan.checked += 1;
    if !scan.resolves(&tok) {
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
    let expanded = expand_env_vars(&scan.expand_vars(value));
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
    if depth == 0 {
        scan.root_read = true;
    }
    // Relative includes resolve against the directory of the path as opened,
    // like Hyprland does, not of its symlink target: a hyprland.conf symlinked
    // into a dotfiles repo still sources siblings from ~/.config/hypr.
    let base_dir = Path::new(path)
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
        if scan.define_var(line) {
            continue;
        }
        process_bind_line(line, scan);
    }
}

pub fn scan_binds(root_path: &str) -> BindScan {
    let mut scan = BindScan {
        home: std::env::var("HOME").unwrap_or_else(|_| "/".to_string()),
        ..BindScan::default()
    };
    let mut visited: Vec<String> = Vec::new();
    scan_config_file(root_path, &mut scan, &mut visited, 0);
    scan
}

pub fn check_binds(results: &mut Vec<CheckResult>) {
    let cfg = join_path(&config_home(), "hypr/hyprland.conf");
    if !file_exists(&cfg) {
        results.push(CheckResult::new(
            "hyprland-binds",
            GROUP,
            CheckStatus::Skip,
            "Hyprland config not found",
            display_path(&cfg),
        ));
        return;
    }
    let scan = scan_binds(&cfg);
    if !scan.root_read {
        results.push(CheckResult::new(
            "hyprland-binds",
            GROUP,
            CheckStatus::Skip,
            "Could not read Hyprland config",
            display_path(&cfg),
        ));
        return;
    }
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
    use crate::unique_dir;
    use std::io::Write;

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
    fn exec_rules_prefix_skipped() {
        // `exec, [rules] cmd` launches cmd with window rules; the command is
        // the first token after the closing bracket, not "[workspace".
        let dir = unique_dir("rules");
        let root = dir.join("hyprland.conf");
        write(
            &root,
            "bind = SUPER, A, exec, [workspace 2 silent] osd_bogus_rules\n\
             bind = SUPER, B, exec, [float; size 50% 50%]sh -c true\n\
             bind = SUPER, C, exec, [workspace 3 silent\n",
        );
        let scan = scan_binds(root.to_str().unwrap());
        assert_eq!(scan.checked, 2);
        assert_eq!(scan.missing, vec!["osd_bogus_rules".to_string()]);
        assert!(scan.root_read);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn shell_syntax_in_exec_binds() {
        // exec binds run via sh -c: env assignments are skipped, and builtins
        // or operators are not programs on PATH.
        let dir = unique_dir("shell");
        let root = dir.join("hyprland.conf");
        write(
            &root,
            "bind = SUPER, A, exec, GDK_BACKEND=x11 osd_bogus_env_prefixed\n\
             bind = SUPER, B, exec, cd ~ && osd_bogus_after_cd\n\
             bind = SUPER, C, exec, [ -f x ] && osd_bogus_after_test\n\
             bind = SUPER, D, exec, sh -c 'true'; osd_bogus_second\n\
             bind = SUPER, E, exec, osd_bogus_semicolon; true\n",
        );
        let scan = scan_binds(root.to_str().unwrap());
        assert_eq!(scan.checked, 3);
        assert_eq!(
            scan.missing,
            vec![
                "osd_bogus_env_prefixed".to_string(),
                "osd_bogus_semicolon".to_string(),
            ]
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn hyprland_variables_expand() {
        // $name = value definitions apply to later exec binds and source paths;
        // the longest identifier is the name ($term vs $terminal).
        let dir = unique_dir("vars");
        std::fs::create_dir_all(dir.join("conf")).unwrap();
        write(
            &dir.join("hyprland.conf"),
            &format!(
                "$term = osd_bogus_term\n\
                 $terminal = sh\n\
                 $cfg = {}/conf\n\
                 source = $cfg/binds.conf\n\
                 bind = SUPER, A, exec, $terminal -c true\n\
                 bind = SUPER, B, exec, $term --hold\n\
                 bind = SUPER, C, exec, $undefined_var\n",
                dir.display()
            ),
        );
        write(
            &dir.join("conf/binds.conf"),
            "bind = SUPER, D, exec, osd_bogus_via_var_include\n",
        );
        let scan = scan_binds(dir.join("hyprland.conf").to_str().unwrap());
        assert_eq!(scan.checked, 3);
        assert_eq!(
            scan.missing,
            vec![
                "osd_bogus_via_var_include".to_string(),
                "osd_bogus_term".to_string(),
            ]
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn relative_include_from_symlinked_root() {
        // A hyprland.conf symlinked into a dotfiles repo sources siblings of
        // the symlink (as Hyprland does), not of its target.
        let dir = unique_dir("symlink");
        std::fs::create_dir_all(dir.join("dots")).unwrap();
        std::fs::create_dir_all(dir.join("hypr")).unwrap();
        write(&dir.join("dots/hyprland.conf"), "source = binds.conf\n");
        std::os::unix::fs::symlink(
            dir.join("dots/hyprland.conf"),
            dir.join("hypr/hyprland.conf"),
        )
        .unwrap();
        write(
            &dir.join("hypr/binds.conf"),
            "bind = SUPER, A, exec, osd_bogus_beside_symlink\n",
        );
        let scan = scan_binds(dir.join("hypr/hyprland.conf").to_str().unwrap());
        assert_eq!(scan.checked, 1);
        assert!(has_missing(&scan, "osd_bogus_beside_symlink"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn commands_resolve_from_home() {
        // Relative commands resolve against Hyprland's working directory (the
        // home directory), and directories are not executables.
        use std::os::unix::fs::PermissionsExt;
        let _g = crate::ENV_MUTEX.lock().unwrap();
        let dir = unique_dir("home_cmds");
        std::env::set_var("HOME", &dir);
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        let script = dir.join("bin/launch.sh");
        write(&script, "#!/bin/sh\n");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        write(
            &dir.join("hyprland.conf"),
            "bind = SUPER, A, exec, bin/launch.sh\n\
             bind = SUPER, B, exec, ~/bin/launch.sh\n\
             bind = SUPER, C, exec, ~/bin\n",
        );
        let scan = scan_binds(dir.join("hyprland.conf").to_str().unwrap());
        assert_eq!(scan.checked, 3);
        assert_eq!(scan.missing, vec!["~/bin".to_string()]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unreadable_root_is_flagged() {
        // A root config that exists but cannot be read (here: a directory,
        // which stays unreadable even as root) must not look like a clean
        // scan of zero binds.
        let dir = unique_dir("unreadable");
        let root = dir.join("hyprland.conf");
        std::fs::create_dir_all(&root).unwrap();
        let scan = scan_binds(root.to_str().unwrap());
        assert!(!scan.root_read);
        assert_eq!(scan.checked, 0);

        // An empty but readable root is a clean scan.
        let ok = dir.join("empty.conf");
        write(&ok, "");
        assert!(scan_binds(ok.to_str().unwrap()).root_read);
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
