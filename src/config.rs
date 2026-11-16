//! Optional TOML config file: check thresholds and per-check modes.
//! Parsed with `toml` + serde into a raw form, then validated into `Config`.
//! A missing default file means defaults; every other problem is an error.

use crate::checks::CHECK_IDS;
use crate::model::{CheckResult, CheckStatus};
use crate::util::fs::{config_home, display_path, join_path};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fmt;
use toml::Spanned;

/// What to do with a check: skip it, or report its WARN/FAIL rows as WARN or FAIL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Off,
    Warn,
    Fail,
}

/// Limits used by the system checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Thresholds {
    /// WARN when / has less free space than this many GB.
    pub disk_warn_gb: u64,
    /// FAIL when / has less free space than this many GB.
    pub disk_fail_gb: u64,
    /// WARN when less than this percentage of memory is available.
    pub memory_warn_percent: u64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Thresholds {
            disk_warn_gb: 10,
            disk_fail_gb: 2,
            memory_warn_percent: 10,
        }
    }
}

/// A validated config. `Config::default()` is the behavior with no file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Config {
    pub thresholds: Thresholds,
    /// Check ID to mode; IDs not listed run normally.
    pub checks: BTreeMap<String, Mode>,
}

/// A config problem, already prefixed with the file path (the CLI adds `osdoctor: `).
#[derive(Debug, PartialEq)]
pub struct ConfigError(pub String);

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ConfigError {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    thresholds: Option<RawThresholds>,
    #[serde(default)]
    checks: BTreeMap<Spanned<String>, Spanned<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawThresholds {
    disk_warn_gb: Option<Spanned<u64>>,
    disk_fail_gb: Option<Spanned<u64>>,
    memory_warn_percent: Option<Spanned<u64>>,
}

/// 1-based line number of byte offset `pos` in `text`.
fn line_of(text: &str, pos: usize) -> usize {
    text.as_bytes()[..pos.min(text.len())]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
        + 1
}

impl Config {
    /// Parse and validate config `text`; `path` is only used in error messages.
    pub fn parse(text: &str, path: &str) -> Result<Config, ConfigError> {
        let shown = display_path(path);
        let err = |msg: String| ConfigError(format!("{shown}: {msg}"));
        let raw: RawConfig =
            toml::from_str(text).map_err(|e| err(e.to_string().trim_end().to_string()))?;

        let mut config = Config::default();
        let line = |s: &std::ops::Range<usize>| line_of(text, s.start);

        if let Some(t) = raw.thresholds {
            let th = &mut config.thresholds;
            if let Some(v) = &t.disk_warn_gb {
                th.disk_warn_gb = *v.get_ref();
            }
            if let Some(v) = &t.disk_fail_gb {
                th.disk_fail_gb = *v.get_ref();
            }
            if th.disk_fail_gb > th.disk_warn_gb {
                // Point at disk_fail_gb if set, else at disk_warn_gb (one is set,
                // since the defaults are consistent).
                let at = t.disk_fail_gb.as_ref().or(t.disk_warn_gb.as_ref());
                let n = at.map_or(1, |v| line(&v.span()));
                return Err(err(format!(
                    "disk_fail_gb ({}) must not exceed disk_warn_gb ({}) at line {n}",
                    th.disk_fail_gb, th.disk_warn_gb
                )));
            }
            if let Some(v) = &t.memory_warn_percent {
                let pct = *v.get_ref();
                if pct > 100 {
                    return Err(err(format!(
                        "memory_warn_percent must be between 0 and 100, got {pct} at line {}",
                        line(&v.span())
                    )));
                }
                th.memory_warn_percent = pct;
            }
        }

        for (key, value) in &raw.checks {
            let id = key.get_ref();
            if !CHECK_IDS.contains(&id.as_str()) {
                return Err(err(format!(
                    "unknown check \"{id}\" at line {}\n  known checks: {}",
                    line(&key.span()),
                    CHECK_IDS.join(", ")
                )));
            }
            let mode = match value.get_ref().as_str() {
                "off" => Mode::Off,
                "warn" => Mode::Warn,
                "fail" => Mode::Fail,
                other => {
                    return Err(err(format!(
                        "invalid mode \"{other}\" for {id} at line {}: expected \"off\", \"warn\" or \"fail\"",
                        line(&value.span())
                    )))
                }
            };
            config.checks.insert(id.clone(), mode);
        }

        Ok(config)
    }

    /// Load the config from `explicit` (must exist), or from
    /// `$XDG_CONFIG_HOME/osdoctor/config.toml` (defaults if it does not exist).
    pub fn load(explicit: Option<&str>) -> Result<Config, ConfigError> {
        match explicit {
            Some(p) => Self::read(p, false),
            None => Self::read(&join_path(&config_home(), "osdoctor/config.toml"), true),
        }
    }

    /// Read and parse `path`. With `missing_ok`, a file that does not exist
    /// means defaults; any other failure (permissions, a directory, bad
    /// UTF-8) is always an error, so a config is never silently ignored.
    fn read(path: &str, missing_ok: bool) -> Result<Config, ConfigError> {
        let fail = |e: &dyn fmt::Display| ConfigError(format!("{}: {e}", display_path(path)));
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) if missing_ok && e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Config::default())
            }
            Err(e) => return Err(fail(&e)),
        };
        let text = String::from_utf8(bytes).map_err(|e| fail(&e))?;
        Self::parse(&text, path)
    }

    /// Whether the check `id` should run (false only when its mode is `off`).
    pub fn enabled(&self, id: &str) -> bool {
        self.checks.get(id) != Some(&Mode::Off)
    }

    /// Rewrite WARN/FAIL results whose check has a `warn` or `fail` mode.
    /// OK and SKIP results are never changed.
    pub fn apply_overrides(&self, results: &mut [CheckResult]) {
        for r in results {
            if !matches!(r.status, CheckStatus::Warn | CheckStatus::Fail) {
                continue;
            }
            match self.checks.get(r.id) {
                Some(Mode::Warn) => r.status = CheckStatus::Warn,
                Some(Mode::Fail) => r.status = CheckStatus::Fail,
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: &str = "/cfg/config.toml";

    #[test]
    fn empty_file_is_defaults() {
        assert_eq!(Config::parse("", P).unwrap(), Config::default());
        assert_eq!(
            Config::parse("# only a comment\n", P).unwrap(),
            Config::default()
        );
        assert_eq!(
            Config::default().thresholds,
            Thresholds {
                disk_warn_gb: 10,
                disk_fail_gb: 2,
                memory_warn_percent: 10
            }
        );
    }

    #[test]
    fn thresholds_parse() {
        let c = Config::parse(
            "[thresholds]\ndisk_warn_gb = 50\ndisk_fail_gb = 5\nmemory_warn_percent = 20\n",
            P,
        )
        .unwrap();
        assert_eq!(
            c.thresholds,
            Thresholds {
                disk_warn_gb: 50,
                disk_fail_gb: 5,
                memory_warn_percent: 20
            }
        );
        // Partial tables keep the other defaults.
        assert_eq!(
            Config::parse("[thresholds]\ndisk_fail_gb = 1\n", P)
                .unwrap()
                .thresholds
                .disk_warn_gb,
            10
        );
    }

    #[test]
    fn modes_parse() {
        let c = Config::parse(
            "[checks]\nwaybar-scripts = \"warn\"\npacman-log = \"off\"\nhyprland-binds = \"fail\"\n",
            P,
        )
        .unwrap();
        assert_eq!(c.checks.get("waybar-scripts"), Some(&Mode::Warn));
        assert_eq!(c.checks.get("pacman-log"), Some(&Mode::Off));
        assert_eq!(c.checks.get("hyprland-binds"), Some(&Mode::Fail));
    }

    #[test]
    fn syntax_error_has_path_and_line() {
        let e = Config::parse("[thresholds\n", P).unwrap_err().0;
        assert!(e.starts_with("/cfg/config.toml: "), "{e}");
        assert!(e.contains("line 1"), "{e}");
        assert!(!e.ends_with('\n'), "{e:?}");
    }

    #[test]
    fn unknown_table_or_key_is_error() {
        assert!(Config::parse("[treshold]\n", P)
            .unwrap_err()
            .0
            .contains("treshold"));
        assert!(Config::parse("[thresholds]\ndisk_warn = 3\n", P)
            .unwrap_err()
            .0
            .contains("disk_warn"));
    }

    #[test]
    fn wrong_type_or_negative_is_error() {
        assert!(Config::parse("[thresholds]\ndisk_warn_gb = \"ten\"\n", P).is_err());
        assert!(Config::parse("[thresholds]\ndisk_warn_gb = -1\n", P).is_err());
        assert!(Config::parse("[thresholds]\ndisk_warn_gb = 1.5\n", P).is_err());
    }

    #[test]
    fn unknown_check_id_names_line_and_known_ids() {
        let e = Config::parse("[checks]\n\nwaybar-script = \"off\"\n", P)
            .unwrap_err()
            .0;
        assert!(
            e.contains("unknown check \"waybar-script\" at line 3"),
            "{e}"
        );
        assert!(
            e.contains("known checks: kernel-version, root-disk-space"),
            "{e}"
        );
    }

    #[test]
    fn bad_mode_names_line_and_choices() {
        let e = Config::parse("[checks]\npacman-log = \"disabled\"\n", P)
            .unwrap_err()
            .0;
        assert!(e.contains("line 2"), "{e}");
        assert!(e.contains("\"off\", \"warn\" or \"fail\""), "{e}");
    }

    #[test]
    fn threshold_ranges_validated() {
        let e = Config::parse("[thresholds]\ndisk_warn_gb = 5\ndisk_fail_gb = 8\n", P)
            .unwrap_err()
            .0;
        assert!(
            e.contains("disk_fail_gb (8) must not exceed disk_warn_gb (5)"),
            "{e}"
        );
        let e = Config::parse("[thresholds]\nmemory_warn_percent = 101\n", P)
            .unwrap_err()
            .0;
        assert!(
            e.contains("memory_warn_percent") && e.contains("line 2"),
            "{e}"
        );
    }

    #[test]
    fn enabled_and_overrides() {
        let c = Config::parse(
            "[checks]\npacman-log = \"off\"\nwaybar-scripts = \"warn\"\nhyprland-binds = \"fail\"\n",
            P,
        )
        .unwrap();
        assert!(!c.enabled("pacman-log"));
        assert!(c.enabled("waybar-scripts") && c.enabled("kernel-version"));
        let mut r = vec![
            CheckResult::new("waybar-scripts", "Desktop", CheckStatus::Fail, "m", ""),
            CheckResult::new("hyprland-binds", "Desktop", CheckStatus::Warn, "m", ""),
            CheckResult::new("hyprland-binds", "Desktop", CheckStatus::Ok, "m", ""),
            CheckResult::new("waybar-scripts", "Desktop", CheckStatus::Skip, "m", ""),
            CheckResult::new("tmp-writable", "System", CheckStatus::Fail, "m", ""),
        ];
        c.apply_overrides(&mut r);
        let s: Vec<_> = r.iter().map(|x| x.status).collect();
        assert_eq!(
            s,
            [
                CheckStatus::Warn,
                CheckStatus::Fail,
                CheckStatus::Ok,
                CheckStatus::Skip,
                CheckStatus::Fail
            ]
        );
    }

    #[test]
    fn example_config_is_valid() {
        let c = Config::parse(
            include_str!("../examples/config.toml"),
            "examples/config.toml",
        )
        .unwrap();
        assert_eq!(c.checks.get("waybar-scripts"), Some(&Mode::Warn));
    }

    #[test]
    fn load_explicit_missing_is_error() {
        let e = Config::load(Some("/nonexistent/osdoctor.toml"))
            .unwrap_err()
            .0;
        assert!(e.starts_with("/nonexistent/osdoctor.toml: "), "{e}");
    }

    #[test]
    fn load_unreadable_path_is_error() {
        // A directory is unreadable as a file, even as root.
        let dir = crate::unique_dir("cfg_dir");
        assert!(Config::load(Some(dir.to_str().unwrap())).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_non_utf8_is_error() {
        let dir = crate::unique_dir("cfg_utf8");
        let f = dir.join("config.toml");
        std::fs::write(&f, b"[checks]\n\xff\xfe = \"off\"\n").unwrap();
        assert!(Config::load(Some(f.to_str().unwrap())).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_default_path() {
        let _env = crate::ENV_MUTEX.lock().unwrap();
        let saved = std::env::var_os("XDG_CONFIG_HOME");
        let dir = crate::unique_dir("cfg_xdg");
        std::env::set_var("XDG_CONFIG_HOME", &dir);
        let file = dir.join("osdoctor/config.toml");

        let missing = Config::load(None);

        std::fs::create_dir_all(dir.join("osdoctor")).unwrap();
        std::fs::write(&file, "[checks]\npacman-log = \"off\"\n").unwrap();
        let present = Config::load(None);

        // A directory that cannot be searched hides whether the file exists;
        // that is an error, not "no config" (root bypasses permissions).
        let perms = |mode| {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir.join("osdoctor"), std::fs::Permissions::from_mode(mode))
                .unwrap()
        };
        perms(0o000);
        // SAFETY: geteuid has no preconditions.
        let is_root = unsafe { libc::geteuid() } == 0;
        let hidden = Config::load(None);
        perms(0o755);

        std::fs::remove_file(&file).unwrap();
        std::fs::create_dir(&file).unwrap();
        let unreadable = Config::load(None);

        match saved {
            Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
        std::fs::remove_dir_all(&dir).ok();

        assert_eq!(missing, Ok(Config::default()));
        assert_eq!(present.unwrap().checks.get("pacman-log"), Some(&Mode::Off));
        assert!(is_root || hidden.is_err(), "{hidden:?}");
        assert!(unreadable.is_err());
    }
}
