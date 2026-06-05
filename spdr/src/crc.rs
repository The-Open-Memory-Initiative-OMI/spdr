//! CRC-16 primitive and base configuration CRC verification.
//!
//! The CRC is the floor of SPD validation: it proves only that the bytes
//! survived transit, nothing about whether their content is sane. It is a
//! queryable check and never blocks decoding; [`crate::decode_identity_and_base`]
//! does not consult it. The "beyond CRC" semantic linter is later work.
//!
//! Algorithm and layout are pinned against open references, not memory; see
//! `docs/implementations/2026-06-04-phase-2-crc.md` for provenance.

use crate::error::DecodeError;
use crate::reader::SpdImage;

/// Last byte (inclusive) covered by the base configuration CRC.
const CRC_COVERED_END: usize = 509;
/// Low byte of the stored base configuration CRC.
const OFF_STORED_CRC_LSB: usize = 510;
/// High byte of the stored base configuration CRC.
const OFF_STORED_CRC_MSB: usize = 511;

/// The outcome of a CRC verification: a small `Copy` status, never a hard error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct CrcStatus {
    /// CRC computed over the covered byte range.
    pub computed: u16,
    /// CRC read from the stored CRC bytes of the image.
    pub stored: u16,
    /// Whether the computed and stored values are equal.
    pub matches: bool,
}

/// Compute the JEDEC SPD CRC-16 over `bytes`.
///
/// This is CRC-16/XMODEM: polynomial `0x1021`, initial value `0x0000`, no bit
/// reflection (each byte enters the high half of the register and the register
/// shifts left), and no final XOR. It is the checksum JEDEC uses across
/// DDR3/DDR4/DDR5 SPD. Iterating the slice cannot panic.
#[must_use]
pub fn crc16(bytes: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &byte in bytes {
        crc ^= u16::from(byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// Verify the base configuration CRC of an SPD image.
///
/// Computes the CRC over bytes `0..=509`, reads the stored CRC from byte 510
/// (low) and byte 511 (high), and reports both values with whether they match.
/// This is a queryable check only: a mismatch is reported, never raised, so it
/// cannot block decoding. Returns [`DecodeError::Truncated`] only if the image
/// is too short to contain the covered range and the stored CRC (512 bytes).
pub fn verify_base_crc(bytes: &[u8]) -> Result<CrcStatus, DecodeError> {
    let covered = bytes
        .get(..=CRC_COVERED_END)
        .ok_or(DecodeError::Truncated {
            offset: CRC_COVERED_END,
            len: bytes.len(),
        })?;
    let computed = crc16(covered);

    let spd = SpdImage::new(bytes);
    let stored = u16::from_le_bytes([spd.byte(OFF_STORED_CRC_LSB)?, spd.byte(OFF_STORED_CRC_MSB)?]);

    Ok(CrcStatus {
        computed,
        stored,
        matches: computed == stored,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc16_xmodem_check_vector() {
        // The published CRC-16/XMODEM check value for the ASCII string
        // "123456789" is 0x31C3. This guards the pinned parameters independently
        // of any SPD image.
        assert_eq!(crc16(b"123456789"), 0x31C3);
    }
}
