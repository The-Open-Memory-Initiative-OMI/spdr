//! Linter integration tests over the real fixture.
//!
//! The fixture is a real, decode-verified module, so it must produce zero lint
//! findings. This is the permanent clean-lint baseline: as rules are added in
//! later phases, a rule that flags the valid fixture is a bug in the rule, and
//! this test catches it.

use spdr::{Finding, lint};

const FIXTURE: &[u8] = include_bytes!("fixtures/teamgroup-ud5-6000_0104eef6.spd");

#[test]
fn fixture_lints_clean() {
    let mut findings: Vec<Finding> = Vec::new();
    lint(FIXTURE, &mut |finding| findings.push(finding));
    assert!(
        findings.is_empty(),
        "the real fixture must produce zero lint findings under the Phase 8 rule set, got {findings:?}"
    );
}
