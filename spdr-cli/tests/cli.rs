//! CLI integration tests · the format goldens, the JSON-valid check, the
//! exit-code contract, and the render-robustness proptest (the Phase 6 payoff on
//! the render side, which the decode-only properties did not reach).

use std::io::Write as _;

use assert_cmd::Command;
use proptest::prelude::*;

const FIXTURE: &[u8] = include_bytes!("../../spdr/tests/fixtures/teamgroup-ud5-6000_0104eef6.spd");

/// Absolute path to the fixture, for the subprocess e2e tests (independent of
/// the test's working directory).
const FIXTURE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../spdr/tests/fixtures/teamgroup-ud5-6000_0104eef6.spd"
);

#[test]
fn render_human_snapshot() {
    let results = spdr_cli::decode(FIXTURE);
    insta::assert_snapshot!(spdr_cli::render_human(&results));
}

#[test]
fn render_json_snapshot() {
    let results = spdr_cli::decode(FIXTURE);
    insta::assert_snapshot!(spdr_cli::render_json(&results).expect("the fixture renders as JSON"));
}

#[test]
fn render_json_parses_back_as_valid_json() {
    let results = spdr_cli::decode(FIXTURE);
    let json = spdr_cli::render_json(&results).expect("the fixture renders as JSON");
    serde_json::from_str::<serde_json::Value>(&json)
        .expect("render_json output must be valid JSON");
}

proptest! {
    /// Render robustness · for an arbitrary image of length 0..=2048, the full
    /// pipeline (decode, then both renderers, including every error path) never
    /// panics. This is the render side that Phase 6's decode-only properties did
    /// not reach.
    #[test]
    fn render_pipeline_never_panics(data in proptest::collection::vec(any::<u8>(), 0..=2048)) {
        let results = spdr_cli::decode(&data);
        let _ = spdr_cli::render_human(&results);
        let _ = spdr_cli::render_json(&results);
    }
}

#[test]
fn decode_fixture_exits_zero() {
    Command::cargo_bin("spdr")
        .unwrap()
        .arg("decode")
        .arg(FIXTURE_PATH)
        .assert()
        .success();
}

#[test]
fn decode_nonexistent_path_exits_two_with_stderr() {
    let assert = Command::cargo_bin("spdr")
        .unwrap()
        .arg("decode")
        .arg("this-path-does-not-exist.spd")
        .assert()
        .code(2);
    assert!(
        !assert.get_output().stderr.is_empty(),
        "a fatal file error must be reported on stderr"
    );
}

#[test]
fn decode_truncated_file_exits_one_with_partial_output() {
    // 250 bytes decodes identity, timings, and the module block, but not the
    // base CRC (needs 512) or manufacturing (needs 555), so the run is genuinely
    // partial: some sections decode, some report a truncation error.
    let mut tmp = tempfile::NamedTempFile::new().expect("create temp file");
    tmp.write_all(&FIXTURE[..250])
        .expect("write truncated fixture");
    tmp.flush().expect("flush temp file");

    let assert = Command::cargo_bin("spdr")
        .unwrap()
        .arg("decode")
        .arg(tmp.path())
        .assert()
        .code(1);

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(
        stdout.contains("[Identity and base]"),
        "partial output should still include the sections that decoded"
    );
    assert!(
        stdout.contains("error:"),
        "partial output should include the per-section errors"
    );
}
