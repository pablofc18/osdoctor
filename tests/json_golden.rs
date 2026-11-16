mod common;

use common::sample_results;
use osdoctor::output::json::render_json;

#[test]
fn render_matches_sample_file() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/sample-output.json");
    let expected = std::fs::read_to_string(path).expect("read sample-output.json");
    assert_eq!(render_json(&sample_results()), expected);
}
