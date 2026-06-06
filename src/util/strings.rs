//! String helpers that have no faithful std equivalent (replaces the tested
//! parts of util_string.c). Trivial helpers (trim/starts_with/contains/line
//! splitting) are dropped in favour of std (`str::trim`, `str::starts_with`,
//! `str::contains`, `str::lines`), which match the C behaviour exactly.

/// Escape `input` for embedding in a JSON double-quoted string. Mirrors
/// json_escape() in util_string.c. Returns an owned String (Rust strings grow,
/// so the C truncation path is not needed).
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

/// Bounded, comma-separated accumulator. Reproduces the snprintf-truncation and
/// `used`-clamp semantics of svc_detail_append/pkg_detail_append so the comma
/// sample truncates and stops appending exactly as in C. One type replaces the
/// two identical C helpers.
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
        // C guard: no-op once the buffer is effectively full.
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
