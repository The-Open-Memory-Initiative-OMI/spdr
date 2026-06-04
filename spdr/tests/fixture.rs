//! Golden-fixture tests over a real DDR5 SPD image.
//!
//! The fixture is a real, published 1024-byte DDR5 SPD dump (see
//! `docs/validated-against.md`); it is treated as opaque input. The snapshot
//! locks the decode against regressions. It does not by itself prove the decode
//! is correct: correctness is verified at review against an independent decoder
//! and the part datasheet.

use spdr::{DecodeError, decode_identity_and_base};

const FIXTURE: &[u8] = include_bytes!("fixtures/teamgroup-ud5-6000_0104eef6.spd");

#[test]
fn fixture_is_1024_bytes() {
    assert_eq!(FIXTURE.len(), 1024, "DDR5 SPD images are 1024 bytes");
}

#[test]
fn decodes_identity_and_base_snapshot() {
    let decoded = decode_identity_and_base(FIXTURE).expect("the fixture decodes cleanly");
    insta::assert_debug_snapshot!(decoded);
}

#[test]
fn truncated_input_errors_without_panic() {
    let truncated = &FIXTURE[..8];
    let err = decode_identity_and_base(truncated).expect_err("truncated input must error");
    assert!(
        matches!(err, DecodeError::Truncated { .. }),
        "expected a truncation error, got {err:?}"
    );
}
