//! String helpers std does not provide: JSON escaping, a bounded
//! comma-separated accumulator, a capped de-duplicating set, and environment
//! variable expansion.

/// Escape `input` for embedding in a JSON double-quoted string.
pub fn json_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Bounded, comma-separated accumulator for the detail line of a check (for
/// example the names of failed units). The text never exceeds `cap - 1`
/// bytes: an item that does not fit is cut at a char boundary, and nothing
/// more is appended once the buffer is full.
pub struct Detail {
    buf: Vec<u8>,
    cap: usize,
    used: usize,
}

impl Detail {
    pub fn new(cap: usize) -> Self {
        Detail {
            buf: Vec::new(),
            cap,
            used: 0,
        }
    }

    pub fn used(&self) -> usize {
        self.used
    }

    pub fn append(&mut self, name: &str) {
        // No-op once the buffer is effectively full.
        if self.cap == 0 || self.used + 1 >= self.cap {
            return;
        }
        let sep = if self.used > 0 { ", " } else { "" };
        let piece = format!("{sep}{name}");
        let want = piece.len(); // intended length, like snprintf's return
        let space = self.cap - 1 - self.used; // bytes available for content
        let take = floor_char_boundary(&piece, want.min(space));
        self.buf.extend_from_slice(&piece.as_bytes()[..take]);
        self.used += want;
        if self.used >= self.cap {
            self.used = self.cap - 1;
        }
    }

    pub fn into_string(self) -> String {
        String::from_utf8(self.buf).unwrap_or_default()
    }
}

/// Push `value` into `set` unless it is already present or `set` has reached
/// `cap`. This is the small, capped, de-duplicating "string set" the desktop
/// config scanners use to collect missing commands / scripts.
pub fn push_unique_capped(set: &mut Vec<String>, value: &str, cap: usize) {
    if set.iter().any(|v| v == value) || set.len() >= cap {
        return;
    }
    set.push(value.to_string());
}

/// Expand $VAR / ${VAR} from the environment; undefined -> empty, lone '$' kept.
pub fn expand_env_vars(input: &str) -> String {
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

/// Largest byte index <= `index` that is a char boundary of `s`.
fn floor_char_boundary(s: &str, index: usize) -> usize {
    if index >= s.len() {
        return s.len();
    }
    let mut i = index;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_plain_and_specials() {
        assert_eq!(json_escape("plain"), "plain");
        assert_eq!(json_escape("a\"b\\c"), "a\\\"b\\\\c");
        assert_eq!(json_escape("tab\tnl\n"), "tab\\tnl\\n");
        assert_eq!(json_escape("\u{01}"), "\\u0001");
    }

    #[test]
    fn detail_basic() {
        let mut d = Detail::new(64);
        d.append("alpha");
        assert_eq!(d.used(), 5);
        d.append("beta");
        d.append("gamma");
        assert_eq!(d.used(), 18);
        assert_eq!(d.into_string(), "alpha, beta, gamma");
    }

    #[test]
    fn push_unique_capped_dedups_and_caps() {
        let mut s = Vec::new();
        push_unique_capped(&mut s, "a", 2);
        push_unique_capped(&mut s, "a", 2); // duplicate: ignored
        push_unique_capped(&mut s, "b", 2);
        push_unique_capped(&mut s, "c", 2); // at cap: ignored
        assert_eq!(s, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn env_vars_expand() {
        let _g = crate::ENV_MUTEX.lock().unwrap();
        std::env::set_var("OSD_TEST_EXPAND", "/v");
        std::env::remove_var("OSD_TEST_UNSET");
        assert_eq!(expand_env_vars("$OSD_TEST_EXPAND/a"), "/v/a");
        assert_eq!(expand_env_vars("${OSD_TEST_EXPAND}/a"), "/v/a");
        assert_eq!(expand_env_vars("x$OSD_TEST_UNSET/a"), "x/a"); // undefined -> empty
        assert_eq!(expand_env_vars("$(cmd) $ 5"), "$(cmd) $ 5"); // not variables
    }

    #[test]
    fn detail_zero_cap_is_noop() {
        let mut d = Detail::new(0);
        d.append("x");
        assert_eq!(d.used(), 0);
        assert_eq!(d.into_string(), "");
    }

    #[test]
    fn detail_truncation() {
        let mut d = Detail::new(8);
        d.append("aaa");
        d.append("bbbbbb"); // would overflow: written partial, used clamps
        assert!(d.used() <= 7);
        let before = d.used();
        d.append("ccc"); // no room left -> no-op
        assert_eq!(d.used(), before);
        let s = d.into_string();
        assert!(s.len() < 8);
        assert_eq!(s, "aaa, bb");
    }
}
