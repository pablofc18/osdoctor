//! Report renderers and the process's only stdout/stderr writers.

pub mod json;
pub mod text;

use std::io::{self, Write};

/// Write `s` to stdout and flush. A closed pipe (the reader went away, as
/// with `osdoctor scan | head -1`) is not an error: the output is simply
/// unwanted. Unlike `print!`, this never panics.
pub fn write_stdout(s: &str) -> io::Result<()> {
    let mut out = io::stdout().lock();
    match out.write_all(s.as_bytes()).and_then(|()| out.flush()) {
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        r => r,
    }
}

/// Write `s` to stderr, ignoring errors: there is nowhere left to report them.
pub fn write_stderr(s: &str) {
    let _ = io::stderr().lock().write_all(s.as_bytes());
}
