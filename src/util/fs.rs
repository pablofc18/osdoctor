//! Filesystem helpers: existence/permission predicates, executable lookup,
//! home and XDG path handling, bounded file reads, and glob expansion.
//! `access`/`glob` go through libc, which std does not expose.

use std::ffi::{CStr, CString};

fn access(path: &str, mode: i32) -> bool {
    match CString::new(path) {
        Ok(c) => unsafe { libc::access(c.as_ptr(), mode) == 0 },
        Err(_) => false,
    }
}

pub fn file_exists(path: &str) -> bool {
    access(path, libc::F_OK)
}

pub fn file_readable(path: &str) -> bool {
    access(path, libc::R_OK)
}

pub fn path_writable(path: &str) -> bool {
    access(path, libc::W_OK)
}

/// Join `a` and `b` with exactly one separator, dropping leading slashes on `b`.
pub fn join_path(a: &str, b: &str) -> String {
    let b = b.trim_start_matches('/');
    if a.ends_with('/') {
        format!("{a}{b}")
    } else {
        format!("{a}/{b}")
    }
}

/// True if `path` is a regular file (following symlinks) we may execute.
/// Directories also pass access(X_OK), so they are excluded explicitly.
fn is_executable_file(path: &str) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file()) && access(path, libc::X_OK)
}

/// True if `cmd` resolves to an executable regular file the way a shell
/// running in `cwd` would find it: a name containing '/' is a path (relative
/// ones resolve against `cwd`); a bare name is searched along PATH, where an
/// empty entry also means `cwd`.
pub fn is_executable_from(cmd: &str, cwd: &str) -> bool {
    if cmd.is_empty() {
        return false;
    }
    if cmd.contains('/') {
        if cmd.starts_with('/') {
            return is_executable_file(cmd);
        }
        return is_executable_file(&join_path(cwd, cmd));
    }
    let path = match std::env::var("PATH") {
        Ok(p) if !p.is_empty() => p,
        _ => "/usr/local/bin:/usr/bin:/bin".to_string(),
    };
    path.split(':').any(|dir| {
        let d = if dir.is_empty() { cwd } else { dir };
        is_executable_file(&join_path(d, cmd))
    })
}

/// `is_executable_from` relative to our own working directory: whether a
/// program we spawn ourselves (systemctl, pacman) can be found.
pub fn is_executable_in_path(cmd: &str) -> bool {
    is_executable_from(cmd, ".")
}

/// The user's XDG config directory: `$XDG_CONFIG_HOME` when it is set to an
/// absolute path (the XDG spec ignores relative values), else `~/.config`.
/// Hyprland and Waybar both read their configs from here.
pub fn config_home() -> String {
    match std::env::var("XDG_CONFIG_HOME") {
        Ok(dir) if dir.starts_with('/') => dir,
        _ => expand_home_path("~/.config"),
    }
}

/// `path` for display, with a leading `$HOME/` shortened to `~/`.
pub fn display_path(path: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let home = home.trim_end_matches('/');
    if !home.is_empty() {
        if let Some(rest) = path.strip_prefix(home) {
            if rest.starts_with('/') {
                return format!("~{rest}");
            }
        }
    }
    path.to_string()
}

/// Expand a leading bare `~` or `~/` using `$HOME`. Other inputs (including
/// `~user`) are returned unchanged.
pub fn expand_home_path(path: &str) -> String {
    if path == "~" || path.starts_with("~/") {
        let home = std::env::var("HOME").unwrap_or_default();
        let rest = &path[1..]; // keeps the leading '/' or is empty
        format!("{home}{rest}")
    } else {
        path.to_string()
    }
}

/// Read up to `max_bytes` of `path` as a UTF-8 (lossy) string, or None on error.
pub fn read_file(path: &str, max_bytes: usize) -> Option<String> {
    use std::io::Read;
    let f = std::fs::File::open(path).ok()?;
    // Pre-size from the file length (clamped to the cap) to avoid the growth
    // reallocs of an empty Vec.
    let cap = f
        .metadata()
        .map(|m| (m.len() as usize).min(max_bytes))
        .unwrap_or(0);
    let mut buf = Vec::with_capacity(cap);
    f.take(max_bytes as u64).read_to_end(&mut buf).ok()?;
    // Move the bytes into the String when they are valid UTF-8 (the common
    // case, zero-copy); fall back to the lossy conversion only when they aren't.
    Some(
        String::from_utf8(buf)
            .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned()),
    )
}

/// Expand a POSIX glob pattern via libc::glob (the matcher Hyprland's own
/// `source =` globbing uses). Returns
/// matched paths (empty on GLOB_NOMATCH or error).
pub fn glob_paths(pattern: &str) -> Vec<String> {
    let mut out = Vec::new();
    let c = match CString::new(pattern) {
        Ok(c) => c,
        Err(_) => return out,
    };
    unsafe {
        let mut g: libc::glob_t = std::mem::zeroed();
        if libc::glob(c.as_ptr(), 0, None, &mut g) == 0 {
            for i in 0..g.gl_pathc {
                let p = *g.gl_pathv.add(i);
                if !p.is_null() {
                    if let Ok(s) = CStr::from_ptr(p).to_str() {
                        out.push(s.to_string());
                    }
                }
            }
        }
        libc::globfree(&mut g);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unique_dir;
    use std::io::Write;

    #[test]
    fn join_cases() {
        assert_eq!(join_path("a", "b"), "a/b");
        assert_eq!(join_path("a/", "b"), "a/b");
        assert_eq!(join_path("/usr", "/bin"), "/usr/bin");
    }

    #[test]
    fn expand_home() {
        let _g = crate::ENV_MUTEX.lock().unwrap();
        std::env::set_var("HOME", "/home/tester");
        assert_eq!(expand_home_path("~/x"), "/home/tester/x");
        assert_eq!(expand_home_path("~"), "/home/tester");
        assert_eq!(expand_home_path("/absolute/path"), "/absolute/path");
        assert_eq!(expand_home_path("~root/x"), "~root/x"); // ~user not expanded
    }

    #[test]
    fn executable_lookup() {
        assert!(is_executable_in_path("sh"));
        assert!(is_executable_in_path("/bin/sh"));
        assert!(!is_executable_in_path(
            "osdoctor_definitely_not_a_real_command_xyz"
        ));
        assert!(!is_executable_in_path("/no/such/path/binary"));
    }

    #[test]
    fn directories_are_not_executables() {
        let dir = unique_dir("exe_dir");
        std::fs::create_dir_all(dir.join("tool")).unwrap();
        let base = dir.to_str().unwrap();
        // A directory passes access(X_OK) but cannot be executed.
        assert!(!is_executable_from(&format!("{base}/tool"), "/"));
        assert!(!is_executable_from("tool/", base));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn relative_paths_resolve_against_cwd() {
        use std::os::unix::fs::PermissionsExt;
        let dir = unique_dir("exe_rel");
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        let script = dir.join("bin/launch.sh");
        std::fs::write(&script, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let base = dir.to_str().unwrap();
        assert!(is_executable_from("bin/launch.sh", base));
        assert!(is_executable_from("./bin/launch.sh", base));
        assert!(!is_executable_from("bin/launch.sh", "/"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn config_home_honours_absolute_xdg() {
        let _g = crate::ENV_MUTEX.lock().unwrap();
        std::env::set_var("HOME", "/home/tester");
        std::env::set_var("XDG_CONFIG_HOME", "/srv/cfg");
        assert_eq!(config_home(), "/srv/cfg");
        std::env::set_var("XDG_CONFIG_HOME", "relative/cfg"); // ignored per spec
        assert_eq!(config_home(), "/home/tester/.config");
        std::env::remove_var("XDG_CONFIG_HOME");
        assert_eq!(config_home(), "/home/tester/.config");
    }

    #[test]
    fn display_path_shortens_home() {
        let _g = crate::ENV_MUTEX.lock().unwrap();
        std::env::set_var("HOME", "/home/tester");
        assert_eq!(display_path("/home/tester/.config/x"), "~/.config/x");
        assert_eq!(display_path("/home/testerX/y"), "/home/testerX/y");
        assert_eq!(display_path("/etc/x"), "/etc/x");
    }

    #[test]
    fn file_predicates_and_read() {
        let dir = unique_dir("fs");
        let file = dir.join("data.txt");
        let fp = file.to_str().unwrap();
        assert!(!file_exists(fp));
        let mut f = std::fs::File::create(&file).unwrap();
        f.write_all(b"hello from osdoctor\n").unwrap();
        drop(f);
        assert!(file_exists(fp));
        assert!(file_readable(fp));
        assert!(path_writable(dir.to_str().unwrap()));
        let content = read_file(fp, 1024).unwrap();
        assert_eq!(content, "hello from osdoctor\n");
        std::fs::remove_dir_all(&dir).ok();
    }
}
