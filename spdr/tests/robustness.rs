//! Robustness harness · on any input, every decoder returns `Ok` or a typed
//! `DecodeError` and never panics.
//!
//! This is robustness, not correctness. The golden snapshots prove the right
//! values come out of the real fixture; this file proves nothing blows up on the
//! wrong bytes. Because the core crate forbids unsafe, a panic is the only crash
//! mode, so "no panic" is the whole contract.
//!
//! Three checks cover the whole public decode surface (identity and base, base
//! CRC, timings, module-specific, manufacturing). Two are proptest properties
//! whose body just calls every decoder and discards the result: a panic fails the
//! test and proptest shrinks the input to a minimal reproducer. The third is a
//! plain exhaustive test over every truncation length, which additionally asserts
//! each decoder returns `Ok` or `Truncated` (the space is small and finite, so no
//! proptest is needed). The fixture is loaded with `include_bytes!`, as the other
//! integration tests do; the generated and mutated bytes are not asserted to be
//! real module data, they are deliberately wrong input.

use proptest::prelude::*;
use spdr::DecodeError;

const FIXTURE: &[u8] = include_bytes!("fixtures/teamgroup-ud5-6000_0104eef6.spd");

/// Run every public `spdr` decoder on `data`, discarding each result. A panic in
/// any decoder fails the calling test; a typed `DecodeError` is fine. This is the
/// whole decode surface and mirrors exactly what the cargo-fuzz `decode_all`
/// target runs.
fn run_all_decoders(data: &[u8]) {
    let _ = spdr::decode_identity_and_base(data);
    let _ = spdr::verify_base_crc(data);
    let _ = spdr::crc16(data);
    let _ = spdr::decode_timings(data);
    let _ = spdr::decode_module_specific(data);
    let _ = spdr::decode_manufacturing(data);
}

proptest! {
    /// Arbitrary bytes · for a generated image of length `0..=2048`, no decoder
    /// panics. The length spans below, at, and above the 1024-byte SPD size, so
    /// short, full, and over-long images are all exercised.
    #[test]
    fn arbitrary_bytes_panics_no_decoder(
        data in proptest::collection::vec(any::<u8>(), 0..=2048),
    ) {
        run_all_decoders(&data);
    }

    /// Single-byte mutation · starting from the real fixture, set one byte at a
    /// proptest-chosen index to a proptest-chosen value; no decoder panics.
    /// Shrinking yields the minimal offending `(index, value)` if one exists.
    #[test]
    fn single_byte_mutation_panics_no_decoder(
        index in 0usize..FIXTURE.len(),
        value in any::<u8>(),
    ) {
        let mut img = FIXTURE.to_vec();
        img[index] = value;
        run_all_decoders(&img);
    }
}

/// Every truncation length · a plain exhaustive test (the space is `0..=1024`,
/// small and finite, so no proptest is needed). For every prefix length of the
/// fixture, running every decoder panics none of them, and each decoder returns
/// `Ok` or `Truncated`. It can never be `UnknownEnum` or `NonAscii`, because the
/// bytes that remain are the real fixture's own valid bytes; a decoder either
/// reads every byte it needs (`Ok`) or runs off the end (`Truncated`).
#[test]
fn every_truncation_returns_ok_or_truncated() {
    for len in 0..=FIXTURE.len() {
        let truncated = &FIXTURE[..len];

        assert_ok_or_truncated(
            spdr::decode_identity_and_base(truncated),
            len,
            "identity and base",
        );
        assert_ok_or_truncated(spdr::verify_base_crc(truncated), len, "base CRC");
        // `crc16` returns a plain `u16` and cannot error; exercise it for the
        // panic check only.
        let _ = spdr::crc16(truncated);
        assert_ok_or_truncated(spdr::decode_timings(truncated), len, "timings");
        assert_ok_or_truncated(
            spdr::decode_module_specific(truncated),
            len,
            "module-specific",
        );
        assert_ok_or_truncated(spdr::decode_manufacturing(truncated), len, "manufacturing");
    }
}

/// Assert a decode result over a truncation of the valid fixture is either `Ok`
/// or `DecodeError::Truncated`. Any other error variant would be a logic error
/// (the remaining bytes are all valid), and a panic would already have aborted
/// the test before this returned.
fn assert_ok_or_truncated<T: std::fmt::Debug>(
    result: Result<T, DecodeError>,
    len: usize,
    decoder: &str,
) {
    assert!(
        matches!(result, Ok(_) | Err(DecodeError::Truncated { .. })),
        "{decoder} at truncation length {len} returned {result:?}, expected Ok or Truncated",
    );
}
