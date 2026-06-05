//! Argument parsing and command dispatch (replaces cli.c). Returns the process
//! exit code (0/1/2 from results, 3 for usage errors).

use crate::checks;
use crate::model::{exit_code, summarize, CheckResult, VERSION};
use crate::output;

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
        "osdoctor {VERSION} - Linux desktop health checks for Arch/Hyprland systems\n\
\n\
Usage:\n\
\u{20}\u{20}osdoctor [command] [options]\n\
\n\
Commands:\n\
\u{20}\u{20}scan        Run all health checks (default when no command is given)\n\
\u{20}\u{20}system      Run system checks only\n\
\u{20}\u{20}services    Run service checks only\n\
\u{20}\u{20}packages    Run package checks only\n\
\u{20}\u{20}desktop     Run desktop/session checks only\n\
\u{20}\u{20}version     Print version and exit\n\
\u{20}\u{20}help        Show this help and exit\n\
\n\
Options:\n\
\u{20}\u{20}--json          Output results as JSON\n\
\u{20}\u{20}--strict        Treat warnings as failures for the exit code\n\
\u{20}\u{20}-h, --help      Show this help\n\
\u{20}\u{20}-V, --version   Print version\n\
\n\
Exit codes:\n\
\u{20}\u{20}0  all checks passed\n\
\u{20}\u{20}1  warnings found, no failures\n\
\u{20}\u{20}2  one or more failures found\n\
\u{20}\u{20}3  invalid usage or internal error\n"
    )
}

pub fn run(args: &[String]) -> i32 {
    let mut json = false;
    let mut strict = false;
    let mut want_help = false;
    let mut want_version = false;
    let mut cmd: Option<Command> = None;

    for arg in args.iter().skip(1) {
        match arg.as_str() {
            "--json" => json = true,
            "--strict" => strict = true,
            "-h" | "--help" => want_help = true,
            "-V" | "--version" => want_version = true,
            a if a.starts_with('-') => {
                eprintln!("osdoctor: unknown option '{a}'\n");
                eprint!("{}", usage());
                return 3;
            }
            a => {
                if cmd.is_some() {
                    eprintln!("osdoctor: unexpected argument '{a}'");
                    return 3;
                }
                match parse_command(a) {
                    Some(c) => cmd = Some(c),
                    None => {
                        eprintln!("osdoctor: unknown command '{a}'\n");
                        eprint!("{}", usage());
                        return 3;
                    }
                }
            }
        }
    }

    if want_help || matches!(cmd, Some(Command::Help)) {
        print!("{}", usage());
        return 0;
    }
    if want_version || matches!(cmd, Some(Command::Version)) {
        println!("osdoctor {VERSION}");
        return 0;
    }
    let cmd = match cmd {
        Some(c) => c,
        None => {
            eprintln!("osdoctor: no command given\n");
            eprint!("{}", usage());
            return 3;
        }
    };

    let mut results: Vec<CheckResult> = Vec::new();
    match cmd {
        Command::System => checks::run_system(&mut results),
        Command::Services => checks::run_services(&mut results),
        Command::Packages => checks::run_packages(&mut results),
        Command::Desktop => checks::run_desktop(&mut results),
        Command::Scan => checks::run_all(&mut results),
        Command::Version | Command::Help => unreachable!(),
    }

    if json {
        output::json::print_json(&results);
    } else {
        output::text::print_text(&results);
    }

    exit_code(summarize(&results), strict)
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
        assert!(u.contains("\n  osdoctor [command] [options]\n"));
        assert!(u.contains("\nExit codes:\n"));
    }
}
