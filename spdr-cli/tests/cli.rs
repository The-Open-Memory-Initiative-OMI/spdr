//! CLI integration tests · the format goldens, the JSON-valid check, the
//! exit-code contract, and the render-robustness proptest (the Phase 6 payoff on
//! the render side, which the decode-only properties did not reach).

use std::io::Write as _;

use assert_cmd::Command;
use proptest::prelude::*;
use spdr::Finding;
use spdr_cli::LintReport;

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

// --- Lint surface (Phase 11) -----------------------------------------------

/// One finding of each severity, in the deterministic order `lint_report`
/// produces (errors first, then warnings, then info), for the render goldens.
fn one_of_each_severity() -> Vec<Finding> {
    vec![
        Finding::TrcIdentityMismatch {
            trc_ps: 48641,
            tras_ps: 32000,
            trp_ps: 16640,
        },
        Finding::ReservedBytesNonZero {
            offset: 128,
            value: 0xFF,
        },
        Finding::NonStandardDataRate {
            data_rate_mt_s: 5000,
        },
    ]
}

/// A 94-byte timing-only image: a valid, self-consistent base timing block whose
/// only flaw is a non-standard 5000 MT/s rate (tCK 400 ps), so the linter emits
/// exactly one Info finding. The identity block does not decode (byte 0 is zero),
/// so it also exercises the limited-coverage note.
fn info_only_timing_image() -> [u8; 94] {
    let mut img = [0u8; 94];
    img[20..22].copy_from_slice(&400u16.to_le_bytes()); // tCKAVGmin -> 5000 MT/s
    img[24] = 0x20; // supported CAS: CL30 (bit 5)
    img[30..32].copy_from_slice(&12000u16.to_le_bytes()); // tAA = 30 * 400 (CL30)
    img[32..34].copy_from_slice(&12000u16.to_le_bytes()); // tRCD
    img[34..36].copy_from_slice(&12000u16.to_le_bytes()); // tRP
    img[36..38].copy_from_slice(&24000u16.to_le_bytes()); // tRAS = 60 * 400
    img[38..40].copy_from_slice(&36000u16.to_le_bytes()); // tRC = tRAS + tRP
    img
}

#[test]
fn lint_human_clean_snapshot() {
    let report = spdr_cli::lint_report(FIXTURE);
    insta::assert_snapshot!(spdr_cli::render_lint_human(&report));
}

#[test]
fn lint_json_clean_snapshot() {
    let report = spdr_cli::lint_report(FIXTURE);
    insta::assert_snapshot!(
        spdr_cli::render_lint_json(&report).expect("the clean fixture renders as JSON")
    );
}

#[test]
fn lint_human_with_findings_snapshot() {
    let report = LintReport {
        findings: one_of_each_severity(),
        base_decode_ok: true,
    };
    insta::assert_snapshot!(spdr_cli::render_lint_human(&report));
}

#[test]
fn lint_json_with_findings_snapshot() {
    let report = LintReport {
        findings: one_of_each_severity(),
        base_decode_ok: true,
    };
    insta::assert_snapshot!(spdr_cli::render_lint_json(&report).expect("findings render as JSON"));
}

#[test]
fn lint_fixture_is_clean_exit_zero() {
    let report = spdr_cli::lint_report(FIXTURE);
    assert!(report.findings.is_empty(), "the real fixture lints clean");
    assert_eq!(spdr_cli::lint_exit_code(&report.findings), 0);
    // The clean JSON is the empty array.
    let json = spdr_cli::render_lint_json(&report).expect("renders");
    assert_eq!(json.trim(), "[]");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert_eq!(parsed.as_array().map(Vec::len), Some(0));
}

#[test]
fn lint_exit_code_per_severity() {
    // clean -> 0
    assert_eq!(spdr_cli::lint_exit_code(&[]), 0);
    // Info only -> 0
    assert_eq!(
        spdr_cli::lint_exit_code(&[Finding::NonStandardDataRate {
            data_rate_mt_s: 5000
        }]),
        0
    );
    // a Warning -> 1
    assert_eq!(
        spdr_cli::lint_exit_code(&[Finding::ReservedBytesNonZero {
            offset: 128,
            value: 0xFF
        }]),
        1
    );
    // an Error -> 1
    assert_eq!(
        spdr_cli::lint_exit_code(&[Finding::TrcIdentityMismatch {
            trc_ps: 48641,
            tras_ps: 32000,
            trp_ps: 16640
        }]),
        1
    );
}

#[test]
fn lint_warning_image_exits_one() {
    // A reserved-region byte set to garbage is a Warning; the exit code is 1.
    let mut img = FIXTURE.to_vec();
    img[128] = 0xFF;
    let report = spdr_cli::lint_report(&img);
    assert_eq!(spdr_cli::lint_exit_code(&report.findings), 1);
    let human = spdr_cli::render_lint_human(&report);
    assert!(human.contains("warning · reserved-bytes-nonzero"));
}

#[test]
fn lint_error_image_exits_one() {
    // Break the tRC = tRAS + tRP identity (byte 39, tRC high byte). That is an
    // Error; the exit code is 1.
    let mut img = FIXTURE.to_vec();
    img[39] = 0xBF; // tRC 0xBF00 != tRAS + tRP
    let report = spdr_cli::lint_report(&img);
    assert_eq!(spdr_cli::lint_exit_code(&report.findings), 1);
    let human = spdr_cli::render_lint_human(&report);
    assert!(human.contains("error · trc-identity-mismatch"));
}

#[test]
fn lint_info_only_image_exits_zero_with_coverage_note() {
    let img = info_only_timing_image();
    let report = spdr_cli::lint_report(&img);
    assert!(!report.base_decode_ok, "the identity block does not decode");
    assert_eq!(report.findings.len(), 1);
    assert_eq!(report.findings[0].code(), "non-standard-data-rate");
    // Info does not fail the exit code.
    assert_eq!(spdr_cli::lint_exit_code(&report.findings), 0);
    let human = spdr_cli::render_lint_human(&report);
    assert!(human.contains("info · non-standard-data-rate"));
    assert!(
        human.contains("only structure-independent checks ran"),
        "limited-coverage note must appear when the base decode failed"
    );
}

#[test]
fn lint_findings_sorted_errors_first() {
    // One Warning (reserved byte) and one Error (broken tRC) in the same image;
    // the Error sorts before the Warning.
    let mut img = FIXTURE.to_vec();
    img[128] = 0xFF; // reserved -> Warning
    img[39] = 0xBF; // tRC -> Error
    let report = spdr_cli::lint_report(&img);
    assert_eq!(report.findings.len(), 2);
    assert_eq!(report.findings[0].code(), "trc-identity-mismatch");
    assert_eq!(report.findings[1].code(), "reserved-bytes-nonzero");
}

#[test]
fn lint_json_shapes_each_finding() {
    let report = LintReport {
        findings: one_of_each_severity(),
        base_decode_ok: true,
    };
    let json = spdr_cli::render_lint_json(&report).expect("renders");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    let array = parsed.as_array().expect("an array");
    assert_eq!(array.len(), 3);
    for finding in array {
        for key in ["severity", "code", "message", "fields"] {
            assert!(finding.get(key).is_some(), "each finding has `{key}`");
        }
    }
    assert_eq!(array[0]["severity"], "error");
    assert_eq!(array[0]["code"], "trc-identity-mismatch");
}

proptest! {
    /// Lint render robustness · for an arbitrary image, the full lint pipeline
    /// (collect findings, then both renderers) never panics, mirroring the decode
    /// render-robustness property.
    #[test]
    fn lint_render_pipeline_never_panics(data in proptest::collection::vec(any::<u8>(), 0..=2048)) {
        let report = spdr_cli::lint_report(&data);
        let _ = spdr_cli::render_lint_human(&report);
        let _ = spdr_cli::render_lint_json(&report);
        let _ = spdr_cli::lint_exit_code(&report.findings);
    }
}

#[test]
fn lint_fixture_exits_zero() {
    Command::cargo_bin("spdr")
        .unwrap()
        .arg("lint")
        .arg(FIXTURE_PATH)
        .assert()
        .success(); // exit 0
}

#[test]
fn lint_nonexistent_path_exits_two_with_stderr() {
    let assert = Command::cargo_bin("spdr")
        .unwrap()
        .arg("lint")
        .arg("this-path-does-not-exist.spd")
        .assert()
        .code(2);
    assert!(
        !assert.get_output().stderr.is_empty(),
        "a fatal file error must be reported on stderr"
    );
}

#[test]
fn lint_missing_file_arg_exits_two() {
    // clap maps a missing required argument to exit code 2.
    Command::cargo_bin("spdr")
        .unwrap()
        .arg("lint")
        .assert()
        .code(2);
}

#[test]
fn lint_warning_file_exits_one() {
    // A reserved-region byte set to garbage produces a Warning, so the process
    // exits 1.
    let mut img = FIXTURE.to_vec();
    img[128] = 0xFF;
    let mut tmp = tempfile::NamedTempFile::new().expect("create temp file");
    tmp.write_all(&img).expect("write mutated fixture");
    tmp.flush().expect("flush temp file");

    Command::cargo_bin("spdr")
        .unwrap()
        .arg("lint")
        .arg(tmp.path())
        .assert()
        .code(1);
}
