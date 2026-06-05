//! Manufacturing block tests over the real DDR5 SPD fixture.
//!
//! This block sits past byte 509, so the base CRC does not cover it; the
//! published reference for serial 0104eef6 is the verification instead. The four
//! published fields (module manufacturer ID, manufacturing date, serial number,
//! part number) are asserted directly, the way the CRC asserted `0x8021`. This
//! adds a new manufacturing snapshot and leaves the Phase 1 through 4 snapshots
//! untouched.

use spdr::{DecodeError, SerialNumber, decode_manufacturing};

const FIXTURE: &[u8] = include_bytes!("fixtures/teamgroup-ud5-6000_0104eef6.spd");

#[test]
fn decodes_manufacturing_snapshot() {
    let decoded = decode_manufacturing(FIXTURE).expect("the fixture has a manufacturing block");
    insta::assert_debug_snapshot!(decoded);
}

#[test]
fn published_reference_fields_match() {
    let m = decode_manufacturing(FIXTURE).expect("the fixture has a manufacturing block");

    // Module manufacturer ID 0x04ef -> JEP-106 bank 5, code 0x6f -> Team Group
    // Inc. (the TEAMGROUP brand). The raw bytes are the oracle.
    assert_eq!([FIXTURE[512], FIXTURE[513]], [0x04, 0xEF]);
    assert_eq!(m.module_manufacturer.bank, 5);
    assert_eq!(m.module_manufacturer.code, 0x6F);
    assert_eq!(m.module_manufacturer.name, Some("Team Group Inc."));

    // Manufacturing date: week 37 of 2023.
    assert_eq!(m.manufacturing_date.year, 2023);
    assert_eq!(m.manufacturing_date.week, 37);

    // Serial number: 0104EEF6.
    assert_eq!(m.serial_number, SerialNumber(0x0104_EEF6));

    // Part number: "UD5-6000".
    assert_eq!(m.part_number, "UD5-6000");
}

#[test]
fn truncated_input_errors_without_panic() {
    // 520 bytes stops inside the manufacturing block (the serial ends at 520),
    // so a byte the decode needs is missing.
    let truncated = &FIXTURE[..520];
    let err = decode_manufacturing(truncated).expect_err("truncated input must error");
    assert!(
        matches!(err, DecodeError::Truncated { .. }),
        "expected a truncation error, got {err:?}"
    );
}

#[test]
fn non_ascii_part_number_errors_without_panic() {
    // Mutate a real dump (as the CRC test does): make the first part-number byte
    // non-ASCII. The decode must surface a typed NonAscii error, not a panic and
    // not a lossy guess.
    let mut img = [0u8; 1024];
    img.copy_from_slice(FIXTURE);
    img[521] = 0xFF;
    let err = decode_manufacturing(&img).expect_err("non-ASCII part number must error");
    assert!(
        matches!(err, DecodeError::NonAscii { .. }),
        "expected a non-ASCII error, got {err:?}"
    );
}
