//! Module-specific block tests over the real DDR5 SPD fixture, plus the
//! module-type dispatch.
//!
//! This adds a new module-specific snapshot and leaves the Phase 1, 2, and 3
//! snapshots untouched. The snapshot locks the unbuffered decode against
//! regression; it does not prove correctness, which is verified at review against
//! an independent decoder's physical-attribute readout (DDR5SPDEditor) and the
//! part's mechanical datasheet.

use spdr::{DecodeError, ModuleSpecific, ModuleType, decode_module_specific};

const FIXTURE: &[u8] = include_bytes!("fixtures/teamgroup-ud5-6000_0104eef6.spd");

#[test]
fn decodes_module_specific_snapshot() {
    let decoded = decode_module_specific(FIXTURE).expect("the unbuffered fixture decodes cleanly");
    insta::assert_debug_snapshot!(decoded);
}

#[test]
fn registered_types_resolve_to_not_yet_decoded() {
    // The dispatch must name a registered module type and parse no fields, never
    // erroring and never fabricating register or data-buffer values. Exercise the
    // router with a crafted module-type byte (byte 3) for each deferred type; no
    // fixture of these types exists, so only byte 3 is read.
    for (key_byte, expected) in [
        (0x01u8, ModuleType::Rdimm),
        (0x03, ModuleType::Sodimm),
        (0x04, ModuleType::Lrdimm),
    ] {
        let mut img = [0u8; 4];
        img[3] = key_byte;
        let decoded = decode_module_specific(&img).expect("dispatch must not error");
        assert_eq!(
            decoded,
            ModuleSpecific::NotYetDecoded(expected),
            "module-type byte {key_byte:#04x} should defer to NotYetDecoded({expected:?})"
        );
    }
}

#[test]
fn reserved_module_type_errors() {
    // Byte 3 low nibble 0x00 is a reserved module type; the dispatch surfaces a
    // typed UnknownEnum, not a panic and not a fabricated decode.
    let mut img = [0u8; 4];
    img[3] = 0x00;
    let err = decode_module_specific(&img).expect_err("a reserved module type must error");
    assert!(
        matches!(err, DecodeError::UnknownEnum { .. }),
        "expected an UnknownEnum error, got {err:?}"
    );
}

#[test]
fn truncated_unbuffered_errors_without_panic() {
    // Byte 3 of the fixture is UDIMM, so the unbuffered path runs and reads
    // through byte 233; 40 bytes is too short for it.
    let truncated = &FIXTURE[..40];
    let err = decode_module_specific(truncated).expect_err("truncated input must error");
    assert!(
        matches!(err, DecodeError::Truncated { .. }),
        "expected a truncation error, got {err:?}"
    );
}
