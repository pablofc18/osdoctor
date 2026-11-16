//! Argument parsing and command dispatch. Returns the process
//! exit code (0/1/2 from results, 3 for usage errors).

use crate::checks;
use crate::config::Config;
use crate::model::{exit_code, summarize, CheckResult, VERSION};
use crate::output::{self, write_stderr, write_stdout};

enum Command {
    Scan,
    System,
    Services,
    Packages,
    Desktop,
    Version,
    Help,
}

fn parse_command(arg: &str) -> Option<Command> {
    match arg {
        "scan" => Some(Command::Scan),
        "system" => Some(Command::System),
        "services" => Some(Command::Services),
        "packages" => Some(Command::Packages),
        "desktop" => Some(Command::Desktop),
        "version" => Some(Command::Version),
        "help" => Some(Command::Help),
        _ => None,
    }
}

fn usage() -> String {
    format!(
        r#"osdoctor {VERSION} - Linux desktop health checks for Arch/Hyprland systems

Usage:
  osdoctor <command> [options]

Commands:
  scan        Run all health checks
  system      Run system checks only
  services    Run service checks only
  packages    Run package checks only
  desktop     Run desktop/session checks only
  version     Print version and exit
  help        Show this help and exit

Options:
  --json          Output results as JSON
  --strict        Treat warnings as failures for the exit code
  --config PATH   Read configuration from PATH
  -h, --help      Show this help
  -V, --version   Print version

Configuration:
  $XDG_CONFIG_HOME/osdoctor/config.toml (default ~/.config/osdoctor/config.toml)

Exit codes:
  0  all checks passed
  1  warnings found, no failures
  2  one or more failures found
  3  invalid usage or internal error
"#
    )
}

/// Exit code for invalid usage or an internal error.
const EXIT_USAGE: i32 = 3;

/// Report a usage error, followed by the usage text when `with_usage`.
fn usage_error(msg: &str, with_usage: bool) -> i32 {
    let usage = if with_usage {
        format!("\n{}", usage())
    } else {
        String::new()
    };
    write_stderr(&format!("osdoctor: {msg}\n{usage}"));
    EXIT_USAGE
}

/// Map a failed stdout write to the internal-error exit code. A closed pipe
/// is already treated as success by write_stdout.
fn output_failed(e: std::io::Error) -> i32 {
    write_stderr(&format!("osdoctor: cannot write output: {e}\n"));
    EXIT_USAGE
}

pub fn run(args: &[String]) -> i32 {
    let mut json = false;
    let mut strict = false;
    let mut want_help = false;
    let mut want_version = false;
    let mut config_path: Option<String> = None;
    let mut cmd: Option<Command> = None;

    let mut it = args.iter().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--config" => {
                if config_path.is_some() {
                    return usage_error("--config given more than once", false);
                }
                match it.next() {
                    Some(p) => config_path = Some(p.clone()),
                    None => return usage_error("--config requires a path", true),
                }
            }
            "--json" => json = true,
            "--strict" => strict = true,
            "-h" | "--help" => want_help = true,
            "-V" | "--version" => want_version = true,
            a if a.starts_with('-') => {
                return usage_error(&format!("unknown option '{a}'"), true);
            }
            a => {
                if cmd.is_some() {
                    return usage_error(&format!("unexpected argument '{a}'"), false);
                }
                match parse_command(a) {
                    Some(c) => cmd = Some(c),
                    None => return usage_error(&format!("unknown command '{a}'"), true),
                }
            }
        }
    }

    if want_help || matches!(cmd, Some(Command::Help)) {
        return write_stdout(&usage()).map_or_else(output_failed, |()| 0);
    }
    if want_version || matches!(cmd, Some(Command::Version)) {
        let line = format!("osdoctor {VERSION}\n");
        return write_stdout(&line).map_or_else(output_failed, |()| 0);
    }
    let Some(cmd) = cmd else {
        return usage_error("no command given", true);
    };

    let config = match Config::load(config_path.as_deref()) {
        Ok(c) => c,
        Err(e) => {
            write_stderr(&format!("osdoctor: {e}\n"));
            return EXIT_USAGE;
        }
    };
    let mut results: Vec<CheckResult> = Vec::new();
    match cmd {
        Command::System => checks::run_system(&mut results, &config),
        Command::Services => checks::run_services(&mut results, &config),
        Command::Packages => checks::run_packages(&mut results, &config),
        Command::Desktop => checks::run_desktop(&mut results, &config),
        Command::Scan => checks::run_all(&mut results, &config),
        Command::Version | Command::Help => unreachable!(),
    }

    config.apply_overrides(&mut results);

    let printed = if json {
        output::json::print_json(&results)
    } else {
        output::text::print_text(&results)
    };
    match printed {
        Ok(()) => exit_code(summarize(&results), strict),
        Err(e) => output_failed(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn version_and_help_exit_zero() {
        assert_eq!(run(&argv(&["osdoctor", "version"])), 0);
        assert_eq!(run(&argv(&["osdoctor", "help"])), 0);
        assert_eq!(run(&argv(&["osdoctor", "-V"])), 0);
        assert_eq!(run(&argv(&["osdoctor", "--help"])), 0);
    }

    #[test]
    fn usage_errors_exit_three() {
        assert_eq!(run(&argv(&["osdoctor"])), 3); // no command
        assert_eq!(run(&argv(&["osdoctor", "bogus"])), 3); // unknown command
        assert_eq!(run(&argv(&["osdoctor", "--nope"])), 3); // unknown option
        assert_eq!(run(&argv(&["osdoctor", "scan", "system"])), 3); // extra arg
    }

    #[test]
    fn usage_text_has_version_and_sections() {
        let u = usage();
        assert!(u.starts_with(&format!("osdoctor {VERSION} - Linux desktop health checks")));
        assert!(u.contains("\n  osdoctor <command> [options]\n"));
        assert!(u.contains("\nExit codes:\n"));
        assert!(u.contains("  --config PATH   Read configuration from PATH\n"));
        assert!(u.contains("\nConfiguration:\n  $XDG_CONFIG_HOME/osdoctor/config.toml"));
    }
}
