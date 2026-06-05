//! Manufacturing information block.
//!
//! Decodes the per-module manufacturing identity that DDR5 SPD stores in its
//! second half: who made the module, where and when, its serial and part number,
//! its revision, and which DRAM (with stepping) it carries. The block lives at
//! bytes 512..=554.
//!
//! One structural difference from the earlier blocks: this region sits past byte
//! 509, so the base configuration CRC (Phase 2) does not cover it. There is no
//! integrity floor here. The verification is instead the published reference for
//! the fixture (serial 0104eef6): the module manufacturer ID, manufacturing date,
//! serial number, and part number are all published, so the decode must reproduce
//! them exactly, the way the CRC had to reproduce `0x8021`.
//!
//! Manufacturer IDs are JEP-106: a continuation/bank byte and a code byte, each
//! carrying odd parity in bit 7. The raw `(bank, code)` is always available; a
//! name is resolved from a small cited table and is `None` when the code is not
//! in it, never a guess. Every offset and encoding is pinned against open
//! references; see `docs/implementations/2026-06-05-phase-5-manufacturing.md`.

use crate::error::DecodeError;
use crate::reader::SpdImage;
use core::fmt;

// Byte offsets (JESD400-5 manufacturing information block).
const OFF_MODULE_MFR_ID: usize = 512; // 512 continuation/bank, 513 code
const OFF_MFR_LOCATION: usize = 514;
const OFF_MFR_DATE: usize = 515; // 515 year (BCD), 516 week (BCD)
const OFF_SERIAL: usize = 517; // 517..=520
const OFF_PART_NUMBER: usize = 521; // 521..=550
const PART_NUMBER_LEN: usize = 30;
const OFF_REVISION: usize = 551;
const OFF_DRAM_MFR_ID: usize = 552; // 552 continuation/bank, 553 code
const OFF_DRAM_STEPPING: usize = 554;

/// A JEP-106 manufacturer identifier: the raw `(bank, code)` plus a resolved name.
///
/// JEP-106 IDs are two bytes: a continuation byte counting `0x7F` continuation
/// codes (so `bank` = that count + 1) and a manufacturer code, each with odd
/// parity in bit 7. The parity bit is stripped to recover the 7-bit values. The
/// `(bank, code)` pair is always decoded; `name` is resolved from a cited table
/// and is `None` when the pair is absent, so an unknown manufacturer is reported
/// as its raw code rather than a guessed name.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ManufacturerId {
    /// JEP-106 bank, 1-based (bank 1 = no continuation bytes).
    pub bank: u8,
    /// JEP-106 manufacturer code, the 7-bit value (odd-parity bit stripped).
    pub code: u8,
    /// Resolved JEP-106 name, or `None` when `(bank, code)` is not in the table.
    pub name: Option<&'static str>,
}

impl fmt::Debug for ManufacturerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Hex code keeps the snapshot auditable against the JEP-106 list.
        write!(
            f,
            "ManufacturerId {{ bank: {}, code: {:#04x}, name: {:?} }}",
            self.bank, self.code, self.name
        )
    }
}

impl fmt::Display for ManufacturerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name {
            Some(name) => f.write_str(name),
            None => write!(f, "JEP-106 bank {} code {:#04x}", self.bank, self.code),
        }
    }
}

/// A module manufacturing date: a calendar year and an ISO-style week number,
/// each decoded from one BCD byte (the year byte is an offset from 2000).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ManufacturingDate {
    /// Full calendar year (2000 + the BCD year byte).
    pub year: u16,
    /// Week of the year (1..=53), from the BCD week byte.
    pub week: u8,
}

impl fmt::Display for ManufacturingDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "week {} of {}", self.week, self.year)
    }
}

/// A module serial number: four raw bytes assembled most-significant-first, the
/// order they are printed in. Rendered as eight uppercase hex digits.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SerialNumber(pub u32);

impl fmt::Debug for SerialNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SerialNumber({:08X})", self.0)
    }
}

impl fmt::Display for SerialNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:08X}", self.0)
    }
}

// Serialize as the eight-hex-digit string `Display` shows ("0104EEF6"), not the
// raw decimal `u32`. A serial number is an identifier presented in hex
// everywhere else (`Display`, `Debug`, the published reference); a decimal here
// would be clearly misleading, so this is a hand-written impl rather than the
// default derive. `no_std` and `alloc`-free: the digits go into a fixed buffer.
#[cfg(feature = "serde")]
impl serde::Serialize for SerialNumber {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut buf = [0u8; 8];
        for (i, b) in buf.iter_mut().enumerate() {
            let nibble = ((self.0 >> (28 - i * 4)) & 0xF) as u8;
            *b = if nibble < 10 {
                b'0' + nibble
            } else {
                b'A' + nibble - 10
            };
        }
        // `buf` is always eight ASCII hex digits, so this is valid UTF-8; the
        // fallback keeps the path panic-free without `unsafe`.
        serializer.serialize_str(core::str::from_utf8(&buf).unwrap_or(""))
    }
}

/// The decoded manufacturing information block.
///
/// `part_number` borrows from the input image (zero-copy, no `alloc`), so the
/// whole struct carries the image lifetime. Every other field is a `Copy` scalar
/// or small type, so the struct is `Copy`. Construct it with
/// [`decode_manufacturing`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Manufacturing<'a> {
    /// Module manufacturer (bytes 512..=513, JEP-106).
    pub module_manufacturer: ManufacturerId,
    /// Module manufacturing location (byte 514, a manufacturer-specific code).
    pub manufacturing_location: u8,
    /// Module manufacturing date (bytes 515..=516, BCD year and week).
    pub manufacturing_date: ManufacturingDate,
    /// Module serial number (bytes 517..=520).
    pub serial_number: SerialNumber,
    /// Module part number (bytes 521..=550, ASCII, trailing padding trimmed),
    /// borrowed from the input image.
    pub part_number: &'a str,
    /// Module revision code (byte 551).
    pub revision_code: u8,
    /// DRAM manufacturer (bytes 552..=553, JEP-106, same decode as the module).
    pub dram_manufacturer: ManufacturerId,
    /// DRAM stepping (byte 554); `0xff` is the conventional "not specified".
    pub dram_stepping: u8,
}

/// Decode the manufacturing information block from a raw SPD image.
///
/// Reads every byte through [`SpdImage`], so a short image is a typed
/// [`DecodeError::Truncated`], never a panic. A non-ASCII part number is a typed
/// [`DecodeError::NonAscii`]. The `part_number` field borrows from `bytes`.
pub fn decode_manufacturing(bytes: &[u8]) -> Result<Manufacturing<'_>, DecodeError> {
    let spd = SpdImage::new(bytes);

    let module_manufacturer = decode_manufacturer_id(
        spd.byte(OFF_MODULE_MFR_ID)?,
        spd.byte(OFF_MODULE_MFR_ID + 1)?,
    );
    let manufacturing_location = spd.byte(OFF_MFR_LOCATION)?;
    let manufacturing_date = decode_date(spd.byte(OFF_MFR_DATE)?, spd.byte(OFF_MFR_DATE + 1)?);
    let serial_number = SerialNumber(u32::from_be_bytes([
        spd.byte(OFF_SERIAL)?,
        spd.byte(OFF_SERIAL + 1)?,
        spd.byte(OFF_SERIAL + 2)?,
        spd.byte(OFF_SERIAL + 3)?,
    ]));
    let part_number = decode_part_number(
        spd.slice(OFF_PART_NUMBER, PART_NUMBER_LEN)?,
        OFF_PART_NUMBER,
    )?;
    let revision_code = spd.byte(OFF_REVISION)?;
    let dram_manufacturer =
        decode_manufacturer_id(spd.byte(OFF_DRAM_MFR_ID)?, spd.byte(OFF_DRAM_MFR_ID + 1)?);
    let dram_stepping = spd.byte(OFF_DRAM_STEPPING)?;

    Ok(Manufacturing {
        module_manufacturer,
        manufacturing_location,
        manufacturing_date,
        serial_number,
        part_number,
        revision_code,
        dram_manufacturer,
        dram_stepping,
    })
}

// --- Per-field decoders ----------------------------------------------------

/// Decode a JEP-106 manufacturer ID from its continuation/bank byte and code
/// byte. Bit 7 of each is odd parity and is stripped: `bank` is the 7-bit
/// continuation count plus 1, `code` is the 7-bit manufacturer code. The name is
/// resolved from the cited table, or `None` when the pair is absent.
fn decode_manufacturer_id(lsb: u8, msb: u8) -> ManufacturerId {
    let bank = (lsb & 0x7F) + 1;
    let code = msb & 0x7F;
    ManufacturerId {
        bank,
        code,
        name: resolve_jep106(bank, code),
    }
}

/// Decode a BCD byte (two decimal digits packed one per nibble) to its value.
fn bcd(byte: u8) -> u8 {
    (byte >> 4) * 10 + (byte & 0x0F)
}

/// Decode the manufacturing date from the BCD year byte (offset from 2000) and
/// the BCD week byte.
fn decode_date(year_byte: u8, week_byte: u8) -> ManufacturingDate {
    ManufacturingDate {
        year: 2000 + u16::from(bcd(year_byte)),
        week: bcd(week_byte),
    }
}

/// Decode the ASCII part-number field into a borrowed `&str`, trimming trailing
/// space and null padding. `base` is the field's absolute offset, used to report
/// the position of a non-ASCII byte. Returns [`DecodeError::NonAscii`] if any
/// byte is outside the ASCII range.
fn decode_part_number(field: &[u8], base: usize) -> Result<&str, DecodeError> {
    if let Some(pos) = field.iter().position(|b| !b.is_ascii()) {
        return Err(DecodeError::NonAscii {
            field: "module part number",
            offset: base + pos,
        });
    }
    // Validated ASCII, so this is valid UTF-8; the map_err is unreachable but
    // keeps the path panic-free.
    let text = core::str::from_utf8(field).map_err(|_| DecodeError::NonAscii {
        field: "module part number",
        offset: base,
    })?;
    Ok(text.trim_end_matches([' ', '\0']))
}

/// JEP-106 manufacturer names, a memory-industry subset sourced from the
/// freeipmi JEDEC manufacturer ID table
/// (`libfreeipmi/spec/ipmi-jedec-manufacturer-identification-code-spec.c`), which
/// reproduces the public JEP-106 assignments. Keyed by `(bank, 7-bit code)`. The
/// fixture's own entries (Team Group Inc., SK Hynix) are the verified correctness
/// claim; the others are cited reference data. Any code absent here resolves to
/// `None`, so the raw `(bank, code)` is reported rather than a guessed name.
const JEP106: &[(u8, u8, &str)] = &[
    (1, 0x2C, "Micron Technology"),
    (1, 0x2D, "SK Hynix"),
    (1, 0x4E, "Samsung"),
    (1, 0x5A, "Winbond Electronic"),
    (5, 0x6F, "Team Group Inc."),
];

/// Resolve a JEP-106 `(bank, code)` to a name from the cited table, or `None`.
fn resolve_jep106(bank: u8, code: u8) -> Option<&'static str> {
    let mut i = 0;
    while i < JEP106.len() {
        let (b, c, name) = JEP106[i];
        if b == bank && c == code {
            return Some(name);
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manufacturer_id_strips_parity_and_resolves() {
        // Fixture module ID bytes 0x04, 0xef: continuation 4 => bank 5; code byte
        // 0xef has parity bit 7 set, 7-bit code 0x6f => Team Group Inc.
        let m = decode_manufacturer_id(0x04, 0xEF);
        assert_eq!(m.bank, 5);
        assert_eq!(m.code, 0x6F);
        assert_eq!(m.name, Some("Team Group Inc."));

        // Fixture DRAM ID bytes 0x80, 0xad: continuation 0 (parity bit set) =>
        // bank 1; 7-bit code 0x2d => SK Hynix.
        let d = decode_manufacturer_id(0x80, 0xAD);
        assert_eq!(d.bank, 1);
        assert_eq!(d.code, 0x2D);
        assert_eq!(d.name, Some("SK Hynix"));
    }

    #[test]
    fn manufacturer_id_absent_code_returns_raw_no_name() {
        // Bank 1, code 0x01 (AMD in the full list) is not in our subset: the raw
        // pair is reported, the name is None, never a guess.
        let m = decode_manufacturer_id(0x00, 0x81);
        assert_eq!(m.bank, 1);
        assert_eq!(m.code, 0x01);
        assert_eq!(m.name, None);
    }

    #[test]
    fn bcd_unpacks_two_digits_per_nibble() {
        assert_eq!(bcd(0x23), 23);
        assert_eq!(bcd(0x37), 37);
        assert_eq!(bcd(0x00), 0);
        assert_eq!(bcd(0x99), 99);
    }

    #[test]
    fn date_year_offset_from_2000_and_week() {
        // Fixture date bytes 0x23, 0x37 => week 37 of 2023.
        let d = decode_date(0x23, 0x37);
        assert_eq!(d.year, 2023);
        assert_eq!(d.week, 37);
    }

    #[test]
    fn serial_assembles_big_endian() {
        // Fixture serial bytes 01 04 ee f6 => 0x0104EEF6.
        let s = SerialNumber(u32::from_be_bytes([0x01, 0x04, 0xEE, 0xF6]));
        assert_eq!(s, SerialNumber(0x0104_EEF6));
    }

    #[test]
    fn part_number_trims_trailing_padding() {
        // "UD5-6000" then spaces (the fixture form) trims to "UD5-6000".
        let mut field = [b' '; PART_NUMBER_LEN];
        field[..8].copy_from_slice(b"UD5-6000");
        assert_eq!(
            decode_part_number(&field, OFF_PART_NUMBER).unwrap(),
            "UD5-6000"
        );

        // Trailing nulls are trimmed too; an all-padding field is empty.
        let mut nulls = [0u8; PART_NUMBER_LEN];
        nulls[..3].copy_from_slice(b"ABC");
        assert_eq!(decode_part_number(&nulls, OFF_PART_NUMBER).unwrap(), "ABC");
        assert_eq!(
            decode_part_number(&[b' '; PART_NUMBER_LEN], OFF_PART_NUMBER).unwrap(),
            ""
        );
    }

    #[test]
    fn part_number_non_ascii_errors_with_offset() {
        let mut field = [b' '; PART_NUMBER_LEN];
        field[2] = 0xFF; // non-ASCII byte at field index 2
        let err = decode_part_number(&field, OFF_PART_NUMBER).unwrap_err();
        assert_eq!(
            err,
            DecodeError::NonAscii {
                field: "module part number",
                offset: OFF_PART_NUMBER + 2,
            }
        );
    }
}
