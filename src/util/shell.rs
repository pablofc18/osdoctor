//! Static inspection of `sh -c` command strings. Hyprland `exec` binds and
//! Waybar `exec`/`on-*` fields are run through `/bin/sh -c`, so the program
//! they launch is the first word after any `NAME=value` assignments, unless
//! that word is shell syntax that cannot be resolved without running it.

/// Shell builtins and reserved words. A command starting with one of these
/// is not checked against PATH: it runs inside the shell (cd, test, export),
/// or the real program comes later (exec, command, if ... then ...).
const SHELL_WORDS: &[&str] = &[
    "!", ".", ":", "[", "[[", "alias", "bg", "break", "builtin", "case", "cd", "command",
    "continue", "declare", "do", "done", "echo", "elif", "else", "esac", "eval", "exec", "exit",
    "export", "false", "fg", "fi", "for", "function", "if", "jobs", "kill", "let", "local",
    "printf", "pwd", "read", "readonly", "return", "select", "set", "shift", "source", "test",
    "then", "time", "trap", "true", "type", "typeset", "ulimit", "umask", "unalias", "unset",
    "until", "wait", "while",
];

/// True for a `NAME=value` environment assignment prefix (`GDK_BACKEND=x11`).
fn is_assignment(word: &str) -> bool {
    let Some((name, _)) = word.split_once('=') else {
        return false;
    };
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c == '_' || c.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

/// Strip one pair of matching surrounding quotes.
fn unquote(word: &str) -> &str {
    for q in ['"', '\''] {
        if word.len() >= 2 && word.starts_with(q) && word.ends_with(q) {
            return &word[1..word.len() - 1];
        }
    }
    word
}

/// Characters that end a word in sh: operators, redirections, subshells.
const OPERATORS: &[char] = &['&', '|', ';', '(', ')', '<', '>'];

/// The program `sh -c cmd` would execute, or None when it cannot be resolved
/// statically: an empty command, a builtin or reserved word, or a word that
/// starts with shell syntax (operators, redirections, subshells, groups).
/// Words are split on blanks and end at an operator (`foo;` is `foo`);
/// quoting inside a word is not interpreted.
pub fn command_word(cmd: &str) -> Option<&str> {
    let word = cmd
        .split([' ', '\t'])
        .find(|w| !w.is_empty() && !is_assignment(w))?;
    if word.starts_with(OPERATORS) || word.starts_with(['{', '}']) {
        return None;
    }
    let word = word.find(OPERATORS).map_or(word, |i| &word[..i]);
    let word = unquote(word);
    if word.is_empty() || SHELL_WORDS.contains(&word) {
        return None;
    }
    Some(word)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_and_quoted_programs() {
        assert_eq!(command_word("firefox --new-window"), Some("firefox"));
        assert_eq!(command_word("  \t kitty"), Some("kitty"));
        assert_eq!(command_word("\"~/bin/run\" arg"), Some("~/bin/run"));
        assert_eq!(command_word("'app'"), Some("app"));
    }

    #[test]
    fn operators_end_the_word() {
        assert_eq!(command_word("foo; bar"), Some("foo"));
        assert_eq!(command_word("foo&&bar"), Some("foo"));
        assert_eq!(command_word("foo|grep x"), Some("foo"));
        assert_eq!(command_word("foo>/dev/null"), Some("foo"));
        assert_eq!(command_word("$(foo)"), Some("$"));
    }

    #[test]
    fn env_assignments_are_skipped() {
        assert_eq!(command_word("GDK_BACKEND=x11 app"), Some("app"));
        assert_eq!(command_word("A=1 _B=2 app -x"), Some("app"));
        assert_eq!(command_word("A=1"), None);
        // Not assignments: no valid name before '='.
        assert_eq!(command_word("=x app"), Some("=x"));
        assert_eq!(command_word("1A=x app"), Some("1A=x"));
    }

    #[test]
    fn shell_syntax_is_not_a_program() {
        assert_eq!(command_word("cd ~ && foo"), None);
        assert_eq!(command_word("[ -f x ] && foo"), None);
        assert_eq!(command_word("test -f x"), None);
        assert_eq!(command_word("&& foo"), None);
        assert_eq!(command_word("(cd /tmp; foo)"), None);
        assert_eq!(command_word("{ foo; }"), None);
        assert_eq!(command_word("exec foo"), None);
        assert_eq!(command_word(""), None);
        assert_eq!(command_word("\"\""), None);
    }
}
