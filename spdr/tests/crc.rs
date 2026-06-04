//! Base configuration CRC tests over the real DDR5 SPD fixture.
//!
//! These use direct assertions (not a snapshot): a CRC is a single 16-bit value,
//! and the Phase 1 snapshot is deliberately left untouched. The positive test
//! asserts the published reference value for this exact module; the negative test
//! is a test transform (a one-byte mutation) of an in-memory copy of the real
//! fixture, not a fabricated dump.

use spdr::verify_base_crc;

const FIXTURE: &[u8] = include_bytes!("fixtures/teamgroup-ud5-6000_0104eef6.spd");

#[test]
fn fixture_main_crc_is_0x8021_and_matches() {
    let status = verify_base_crc(FIXTURE).expect("the fixture contains a full base block");
    assert_eq!(status.computed, 0x8021, "computed main CRC");
    assert_eq!(status.stored, 0x8021, "stored main CRC");
    assert!(status.matches, "computed must equal stored");
}

#[test]
fn single_byte_mutation_breaks_the_crc() {
    let mut mutated = FIXTURE.to_vec();
    // Flip one bit of byte 2 (the DRAM device-type key), which lies inside the
    // CRC-covered range 0..=509. The stored CRC is unchanged, so they must differ.
    mutated[2] ^= 0x01;

    let status = verify_base_crc(&mutated).expect("still a full-length image");
    assert_ne!(
        status.computed, status.stored,
        "a covered-byte mutation must change the computed CRC"
    );
    assert!(!status.matches, "verification must now report a mismatch");
}
