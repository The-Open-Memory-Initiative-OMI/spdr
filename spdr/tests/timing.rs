//! Base JEDEC timing tests over the real DDR5 SPD fixture.
//!
//! This adds a new timing snapshot and leaves the Phase 1 identity snapshot
//! untouched. The snapshot locks the decode against regression; it does not
//! prove correctness, which is verified at review against an independent
//! decoder's base-timing readout (base versus base, not base versus the 6000
//! XMP/EXPO profile).

use spdr::{DecodeError, decode_timings};

const FIXTURE: &[u8] = include_bytes!("fixtures/teamgroup-ud5-6000_0104eef6.spd");

#[test]
fn decodes_base_timings_snapshot() {
    let timings = decode_timings(FIXTURE).expect("the fixture has a full timing block");
    insta::assert_debug_snapshot!(timings);
}

#[test]
fn implied_base_speed_is_ddr5_4800() {
    // The base JEDEC fallback, not the advertised 6000 profile (Phase 9).
    let timings = decode_timings(FIXTURE).expect("the fixture has a full timing block");
    assert_eq!(timings.base_data_rate_mt_s(), 4800);
}

#[test]
fn truncated_input_errors_without_panic() {
    // 40 bytes is too short for the timing block (it extends to byte 93).
    let truncated = &FIXTURE[..40];
    let err = decode_timings(truncated).expect_err("truncated input must error");
    assert!(
        matches!(err, DecodeError::Truncated { .. }),
        "expected a truncation error, got {err:?}"
    );
}
