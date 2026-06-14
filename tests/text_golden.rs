mod common;

use common::sample_results;
use osdoctor::output::text::render_text;

#[test]
fn render_matches_sample_file() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/sample-output.txt");
    let expected = std::fs::read_to_string(path).expect("read sample-output.txt");
    // The committed sample is the plain (piped / NO_COLOR) rendering.
    assert_eq!(render_text(&sample_results(), false), expected);
}
