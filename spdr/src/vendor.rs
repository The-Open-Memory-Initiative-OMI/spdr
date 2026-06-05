//! Vendor overclocking profiles · Intel XMP 3.0 and AMD EXPO.
//!
//! These are the rated-speed profiles a module advertises on the box. They live
//! in the upper SPD region (bytes 640..=1023), above the JEDEC base block, and
//! are vendor extensions rather than JEDEC-defined content, so they are less
//! openly documented than the base. This decode is therefore anchored by two
//! oracles instead of being trusted on its own:
//!
//! - **The section CRC is the region anchor.** Each profile section stores a
//!   CRC-16 over a fixed byte range. We recompute it with the Phase 2 primitive
//!   ([`crate::crc16`], CRC-16/XMODEM) over the pinned range and compare to the
//!   stored value, returning a [`CrcStatus`] per section. A match confirms the
//!   region, the range, and the algorithm all at once, exactly as `0x8021`
//!   confirmed the base block. The match was the gate for pinning every offset
//!   here: the ranges below are not guessed, they are the ranges whose computed
//!   CRC equals the stored CRC.
//! - **The rated timing is the value oracle.** The reference fixture is rated
//!   DDR5-6000 38-38-38-78 at 1.25 V; the decoded XMP profile and the decoded
//!   EXPO profile each reproduce that, cross-checking the same rated numbers two
//!   independent ways.
//!
//! Presence is detected by the magic identifier: XMP 3.0 by the two bytes
//! `0x0C 0x4A` at offset 640, EXPO by ASCII `"EXPO"` at offset 832. Absent magic
//! yields [`Xmp::Absent`] / [`Expo::Absent`], a no-profile result that parses
//! nothing; arbitrary bytes therefore never produce a fabricated decode. Every
//! read goes through [`SpdImage`], so a short image is a typed
//! [`DecodeError::Truncated`], never a panic.
//!
//! ## Provenance
//!
//! Offsets are pinned against open references, not memory, and cross-checked:
//!
//! - XMP magic and per-profile timing offsets (tCK at profile+5, tAA at +13,
//!   tRCD at +15, tRP at +17, tRAS at +19, tRC at +21, all little-endian u16)
//!   are taken from memtest86plus `system/spd.c`.
//! - The XMP header and profile field order (vpp, vdd, vddq, then minCycleTime
//!   at +5; the three 16-byte profile names in the header at +14/+30/+46; the
//!   per-block checksum in the last two bytes), and the entire EXPO layout
//!   (10-byte header, then 40-byte profiles ordered vdd, vddq, vpp, then
//!   minCycleTime at +4) are taken from edlf/DDR5SPDEditor `ddr5spd_structs.h`.
//! - The voltage encoding (upper 3 bits = whole volts, lower 5 bits in 50 mV
//!   steps) and the CRC-16 parameters are taken from edlf/DDR5SPDEditor
//!   `utilities.cpp` (`ConvertByteToVoltageDDR5`, `Crc16`).
//!
//! The CRC ranges were then each confirmed by computed-equals-stored on the real
//! fixture. See `docs/implementations/2026-06-05-phase-9a-xmp-expo.md` for the
//! full per-field provenance and the decoded-versus-preserved-versus-deferred
//! boundary.

use crate::crc::{CrcStatus, crc16};
use crate::error::DecodeError;
use crate::reader::SpdImage;
use crate::timing::{Picoseconds, data_rate_mt_s};
use core::fmt;

// --- XMP 3.0 layout (bytes 640..=831) --------------------------------------

/// Offset of the XMP 3.0 header, which is also the magic identifier.
const OFF_XMP_MAGIC: usize = 640;
/// XMP 3.0 magic bytes (`0x0C 0x4A`) at [`OFF_XMP_MAGIC`].
const XMP_MAGIC: [u8; 2] = [0x0C, 0x4A];
/// Byte holding the per-profile enable bits (bit `i` enables profile `i + 1`).
const OFF_XMP_ENABLE: usize = 643;
/// Header offset of profile-name 1 (16 ASCII bytes); names 2 and 3 follow at
/// +16 and +32.
const OFF_XMP_NAME1: usize = 654;
/// Length of one XMP block (header or profile), in bytes; the CRC sits in the
/// last two bytes and covers the [`BLOCK_LEN`] - 2 bytes before it.
const BLOCK_LEN: usize = 64;
/// Offset of XMP profile 1; profile 2 follows one [`BLOCK_LEN`] later.
const OFF_XMP_PROFILE1: usize = 704;

// Field offsets within a 64-byte XMP profile block (little-endian u16 timings).
const XMP_VPP: usize = 0;
const XMP_VDD: usize = 1;
const XMP_VDDQ: usize = 2;
const XMP_TCK: usize = 5;
const XMP_TAA: usize = 13;
const XMP_TRCD: usize = 15;
const XMP_TRP: usize = 17;
const XMP_TRAS: usize = 19;

// --- EXPO layout (bytes 832..=959) -----------------------------------------

/// Offset of the EXPO block, which begins with the magic identifier.
const OFF_EXPO_MAGIC: usize = 832;
/// EXPO magic bytes (ASCII `"EXPO"`) at [`OFF_EXPO_MAGIC`].
const EXPO_MAGIC: [u8; 4] = *b"EXPO";
/// Length of the whole EXPO block; its single CRC covers the first
/// [`EXPO_BLOCK_LEN`] - 2 bytes and is stored in the last two.
const EXPO_BLOCK_LEN: usize = 128;
/// Offset of EXPO profile 1 (header is 10 bytes); profile 2 follows one
/// [`EXPO_PROFILE_LEN`] later.
const OFF_EXPO_PROFILE1: usize = 842;
/// Length of one EXPO profile block, in bytes.
const EXPO_PROFILE_LEN: usize = 40;

// Field offsets within a 40-byte EXPO profile block (little-endian u16 timings).
const EXPO_VDD: usize = 0;
const EXPO_VDDQ: usize = 1;
const EXPO_VPP: usize = 2;
const EXPO_TCK: usize = 4;
const EXPO_TAA: usize = 6;
const EXPO_TRCD: usize = 8;
const EXPO_TRP: usize = 10;
const EXPO_TRAS: usize = 12;

/// A voltage in millivolts, the canonical unit for the profile rails. The unit
/// is named in the type so no caller has to guess, as [`Picoseconds`] does for
/// timings. The stored encoding (upper 3 bits = whole volts, lower 5 bits in
/// 50 mV steps) is normalised to millivolts on decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Millivolts(pub u16);

impl Millivolts {
    /// The voltage in millivolts.
    #[must_use]
    pub const fn millivolts(self) -> u16 {
        self.0
    }
}

impl fmt::Display for Millivolts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{:03} V", self.0 / 1000, self.0 % 1000)
    }
}

/// The rated values a vendor profile advertises, shared by XMP and EXPO.
///
/// Every field is a `Copy` scalar borrowed from nothing, so the struct is
/// `Copy`. The cycle time gives the data rate; the four core timings are the
/// ones the rated-timing oracle covers (CAS, tRCD, tRP, tRAS). Other profile
/// timings (tRC, tWR, the tRFC family, and the bank-group-class parameters)
/// are present in the block but not surfaced here: they are preserved in the
/// image and deferred rather than claimed, since the oracle does not cover them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct RatedTimings {
    /// Minimum cycle time (tCK); sets the data rate.
    pub cycle_time: Picoseconds,
    /// Data rate in MT/s implied by the cycle time, rounded to the nearest 100.
    pub data_rate_mt_s: u32,
    /// Rated CAS latency (CL), derived as tAA rounded to whole cycles of tCK.
    pub cas_latency: u16,
    /// CAS latency time (tAA).
    pub taa: Picoseconds,
    /// RAS-to-CAS delay (tRCD).
    pub trcd: Picoseconds,
    /// Row precharge time (tRP).
    pub trp: Picoseconds,
    /// Active-to-precharge time (tRAS).
    pub tras: Picoseconds,
    /// Supply voltage (VDD).
    pub vdd: Millivolts,
    /// I/O supply voltage (VDDQ).
    pub vddq: Millivolts,
    /// Wordline boost voltage (VPP).
    pub vpp: Millivolts,
}

/// A decoded Intel XMP 3.0 profile.
///
/// Borrows the profile name from the input image (zero-copy). The per-profile
/// section CRC is carried alongside the values, so a consumer can see whether
/// the region the values came from is confirmed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct XmpProfile<'a> {
    /// 1-based profile number (1 or 2).
    pub profile_number: u8,
    /// The custom profile name from the header, trimmed of trailing padding.
    /// `None` if the name slot is blank or not clean printable ASCII; a bad name
    /// is never fabricated and never fails the rest of the decode.
    pub name: Option<&'a str>,
    /// The rated values.
    pub rated: RatedTimings,
    /// This profile's section CRC status (the region anchor).
    pub crc: CrcStatus,
}

/// The Intel XMP 3.0 region: either absent (no magic) or present with its
/// header CRC and the enabled profiles.
///
/// Up to two profiles fit in the XMP region before the EXPO block at 832, which
/// matches the two-profile enumeration in the memtest86plus reference. A profile
/// is included only when its enable bit is set; a disabled slot is `None` rather
/// than a decode of bytes the vendor did not mark active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum Xmp<'a> {
    /// No XMP 3.0 magic at offset 640: the region is absent.
    Absent,
    /// XMP 3.0 magic present: the header CRC and the enabled profiles.
    Present {
        /// The XMP header section CRC (the region anchor for the header block).
        header_crc: CrcStatus,
        /// Profile 1, if its enable bit is set.
        profile1: Option<XmpProfile<'a>>,
        /// Profile 2, if its enable bit is set.
        profile2: Option<XmpProfile<'a>>,
    },
}

/// A decoded AMD EXPO profile.
///
/// EXPO profiles carry no name and no per-profile CRC; the whole EXPO block is
/// covered by one CRC, held on the [`Expo::Present`] container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ExpoProfile {
    /// 1-based profile number (1 or 2).
    pub profile_number: u8,
    /// The rated values.
    pub rated: RatedTimings,
}

/// The AMD EXPO region: either absent (no magic) or present with its block CRC
/// and the populated profiles.
///
/// EXPO has a single CRC over the whole block rather than per-profile CRCs, so
/// the block CRC is the region anchor for both profiles. EXPO does not expose a
/// pinned per-profile enable encoding the way XMP does, so a profile is included
/// only when its cycle time is non-zero (a zeroed slot is unpopulated); the
/// block CRC confirms the region the values are read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum Expo {
    /// No EXPO magic at offset 832: the region is absent.
    Absent,
    /// EXPO magic present: the block CRC and the populated profiles.
    Present {
        /// The EXPO block section CRC (the region anchor for the whole block).
        block_crc: CrcStatus,
        /// Profile 1, if populated (non-zero cycle time).
        profile1: Option<ExpoProfile>,
        /// Profile 2, if populated (non-zero cycle time).
        profile2: Option<ExpoProfile>,
    },
}

/// Both vendor-profile regions of an SPD image, decoded together.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct VendorProfiles<'a> {
    /// The Intel XMP 3.0 region.
    pub xmp: Xmp<'a>,
    /// The AMD EXPO region.
    pub expo: Expo,
}

/// Decode both vendor-profile regions (XMP 3.0 and EXPO) from a raw SPD image.
///
/// A convenience over [`decode_xmp`] and [`decode_expo`]; see those for the
/// per-region contract. Returns [`DecodeError::Truncated`] if the image is too
/// short for a byte a present region reads; an absent region is not an error.
pub fn decode_vendor_profiles(bytes: &[u8]) -> Result<VendorProfiles<'_>, DecodeError> {
    Ok(VendorProfiles {
        xmp: decode_xmp(bytes)?,
        expo: decode_expo(bytes)?,
    })
}

/// Decode the Intel XMP 3.0 region from a raw SPD image.
///
/// Detects presence by the magic at offset 640. If absent, returns
/// [`Xmp::Absent`] and reads nothing further. If present, decodes the header
/// CRC and each enabled profile through [`SpdImage`]. Returns
/// [`DecodeError::Truncated`] only if the image is long enough to hold the magic
/// but too short for a byte a present profile reads. Never panics.
pub fn decode_xmp(bytes: &[u8]) -> Result<Xmp<'_>, DecodeError> {
    let spd = SpdImage::new(bytes);

    let magic = [spd.byte(OFF_XMP_MAGIC)?, spd.byte(OFF_XMP_MAGIC + 1)?];
    if magic != XMP_MAGIC {
        return Ok(Xmp::Absent);
    }

    let enable = spd.byte(OFF_XMP_ENABLE)?;
    let header_crc = section_crc(&spd, OFF_XMP_MAGIC, BLOCK_LEN)?;

    let profile1 = if enable & 0x01 != 0 {
        Some(decode_xmp_profile(
            &spd,
            1,
            OFF_XMP_PROFILE1,
            OFF_XMP_NAME1,
        )?)
    } else {
        None
    };
    let profile2 = if enable & 0x02 != 0 {
        Some(decode_xmp_profile(
            &spd,
            2,
            OFF_XMP_PROFILE1 + BLOCK_LEN,
            OFF_XMP_NAME1 + 16,
        )?)
    } else {
        None
    };

    Ok(Xmp::Present {
        header_crc,
        profile1,
        profile2,
    })
}

/// Decode the AMD EXPO region from a raw SPD image.
///
/// Detects presence by the `"EXPO"` magic at offset 832. If absent, returns
/// [`Expo::Absent`]. If present, decodes the single block CRC and each populated
/// profile through [`SpdImage`]. Returns [`DecodeError::Truncated`] only if the
/// image is long enough to hold the magic but too short for a byte the present
/// block reads. Never panics.
pub fn decode_expo(bytes: &[u8]) -> Result<Expo, DecodeError> {
    let spd = SpdImage::new(bytes);

    if spd.slice(OFF_EXPO_MAGIC, EXPO_MAGIC.len())? != EXPO_MAGIC {
        return Ok(Expo::Absent);
    }

    let block_crc = section_crc(&spd, OFF_EXPO_MAGIC, EXPO_BLOCK_LEN)?;
    let profile1 = decode_expo_profile(&spd, 1, OFF_EXPO_PROFILE1)?;
    let profile2 = decode_expo_profile(&spd, 2, OFF_EXPO_PROFILE1 + EXPO_PROFILE_LEN)?;

    Ok(Expo::Present {
        block_crc,
        profile1,
        profile2,
    })
}

// --- Profile decoders ------------------------------------------------------

/// Decode one XMP profile at `base`, with its name at `name_off`.
fn decode_xmp_profile<'a>(
    spd: &SpdImage<'a>,
    number: u8,
    base: usize,
    name_off: usize,
) -> Result<XmpProfile<'a>, DecodeError> {
    let rated = rated_timings(
        spd,
        Voltages {
            vdd: spd.byte(base + XMP_VDD)?,
            vddq: spd.byte(base + XMP_VDDQ)?,
            vpp: spd.byte(base + XMP_VPP)?,
        },
        Offsets {
            tck: base + XMP_TCK,
            taa: base + XMP_TAA,
            trcd: base + XMP_TRCD,
            trp: base + XMP_TRP,
            tras: base + XMP_TRAS,
        },
    )?;
    let crc = section_crc(spd, base, BLOCK_LEN)?;
    let name = read_name(spd, name_off)?;

    Ok(XmpProfile {
        profile_number: number,
        name,
        rated,
        crc,
    })
}

/// Decode one EXPO profile at `base`, or `None` if the slot is unpopulated
/// (zero cycle time).
fn decode_expo_profile(
    spd: &SpdImage,
    number: u8,
    base: usize,
) -> Result<Option<ExpoProfile>, DecodeError> {
    if read_le_u16(spd, base + EXPO_TCK)? == 0 {
        return Ok(None);
    }

    let rated = rated_timings(
        spd,
        Voltages {
            vdd: spd.byte(base + EXPO_VDD)?,
            vddq: spd.byte(base + EXPO_VDDQ)?,
            vpp: spd.byte(base + EXPO_VPP)?,
        },
        Offsets {
            tck: base + EXPO_TCK,
            taa: base + EXPO_TAA,
            trcd: base + EXPO_TRCD,
            trp: base + EXPO_TRP,
            tras: base + EXPO_TRAS,
        },
    )?;

    Ok(Some(ExpoProfile {
        profile_number: number,
        rated,
    }))
}

/// The three voltage bytes of a profile, in their raw encoding.
struct Voltages {
    vdd: u8,
    vddq: u8,
    vpp: u8,
}

/// The five timing-field offsets of a profile. XMP and EXPO place these
/// differently, so the caller supplies the absolute offsets and this assembles
/// the shared [`RatedTimings`].
struct Offsets {
    tck: usize,
    taa: usize,
    trcd: usize,
    trp: usize,
    tras: usize,
}

/// Assemble the shared rated values from a profile's voltages and timing
/// offsets. The cycle time gives the data rate; tAA divided by the cycle time
/// gives the rated CAS latency.
fn rated_timings(spd: &SpdImage, v: Voltages, o: Offsets) -> Result<RatedTimings, DecodeError> {
    let cycle_time = ps(read_le_u16(spd, o.tck)?);
    let taa = ps(read_le_u16(spd, o.taa)?);

    Ok(RatedTimings {
        cycle_time,
        data_rate_mt_s: data_rate_mt_s(cycle_time),
        cas_latency: rated_cas_latency(taa, cycle_time),
        taa,
        trcd: ps(read_le_u16(spd, o.trcd)?),
        trp: ps(read_le_u16(spd, o.trp)?),
        tras: ps(read_le_u16(spd, o.tras)?),
        vdd: voltage(v.vdd),
        vddq: voltage(v.vddq),
        vpp: voltage(v.vpp),
    })
}

// --- Encoding helpers ------------------------------------------------------

/// Compute a block's section CRC: CRC-16/XMODEM over the first `block_len` - 2
/// bytes at `start`, compared to the two-byte little-endian CRC stored in the
/// block's last two bytes. Reuses the Phase 2 primitive; the match is what
/// confirms the range and the algorithm.
fn section_crc(spd: &SpdImage, start: usize, block_len: usize) -> Result<CrcStatus, DecodeError> {
    let covered = spd.slice(start, block_len - 2)?;
    let computed = crc16(covered);
    let stored = u16::from_le_bytes([
        spd.byte(start + block_len - 2)?,
        spd.byte(start + block_len - 1)?,
    ]);
    Ok(CrcStatus {
        computed,
        stored,
        matches: computed == stored,
    })
}

/// A raw picosecond-encoded timing: the stored u16 is already in picoseconds
/// (DDR5's 1 ps granularity), matching the base timing block.
fn ps(raw: u16) -> Picoseconds {
    Picoseconds(u32::from(raw))
}

/// Read a little-endian u16 at `offset` through the reader.
fn read_le_u16(spd: &SpdImage, offset: usize) -> Result<u16, DecodeError> {
    Ok(u16::from_le_bytes([
        spd.byte(offset)?,
        spd.byte(offset + 1)?,
    ]))
}

/// Decode a profile voltage byte to millivolts: the upper 3 bits are whole
/// volts and the lower 5 bits count 50 mV steps (the `ConvertByteToVoltageDDR5`
/// encoding). For `0x25` this is 1 V + 5 * 50 mV = 1250 mV; for `0x30`, 1 V +
/// 16 * 50 mV = 1800 mV.
fn voltage(byte: u8) -> Millivolts {
    Millivolts(u16::from(byte >> 5) * 1000 + u16::from(byte & 0x1F) * 50)
}

/// Derive the rated CAS latency: tAA in whole cycles of the cycle time, rounded
/// to nearest. Guards a zero cycle time (returns 0) so malformed input cannot
/// divide by zero.
fn rated_cas_latency(taa: Picoseconds, cycle_time: Picoseconds) -> u16 {
    let tck = cycle_time.0;
    if tck == 0 {
        return 0;
    }
    ((taa.0 + tck / 2) / tck) as u16
}

/// Read a 16-byte profile name at `off`, trimmed of trailing spaces and NULs.
/// Returns `None` if the trimmed name is empty or holds a non-printable byte,
/// so a blank or garbled name never fails the decode and is never fabricated.
/// Returns [`DecodeError::Truncated`] only if the image is too short for the
/// 16-byte field.
fn read_name<'a>(spd: &SpdImage<'a>, off: usize) -> Result<Option<&'a str>, DecodeError> {
    let raw = spd.slice(off, 16)?;
    let end = raw
        .iter()
        .rposition(|&b| b != b' ' && b != 0)
        .map_or(0, |i| i + 1);
    let trimmed = raw.get(..end).unwrap_or_default();
    if trimmed.is_empty() || !trimmed.iter().all(|&b| (0x20..=0x7E).contains(&b)) {
        return Ok(None);
    }
    Ok(core::str::from_utf8(trimmed).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voltage_byte_to_millivolts() {
        // ConvertByteToVoltageDDR5: upper 3 bits = whole volts, lower 5 bits in
        // 50 mV steps. 0x25 -> 1.250 V, 0x30 -> 1.800 V (the fixture's VDD/VPP).
        assert_eq!(voltage(0x25), Millivolts(1250));
        assert_eq!(voltage(0x30), Millivolts(1800));
        assert_eq!(voltage(0x00), Millivolts(0));
        // Pure whole-volt and pure-step extremes.
        assert_eq!(voltage(0x20), Millivolts(1000)); // 1 V, 0 steps
        assert_eq!(voltage(0x1F), Millivolts(1550)); // 0 V, 31 * 50 mV
    }

    #[test]
    fn millivolts_display_is_three_decimal_volts() {
        assert_eq!(render(Millivolts(1250)).as_bytes(), b"1.250 V");
        assert_eq!(render(Millivolts(1800)).as_bytes(), b"1.800 V");
    }

    #[test]
    fn rated_cas_latency_rounds_taa_in_cycles() {
        // DDR5-6000: tCK 333 ps, tAA 12654 ps -> exactly CL38.
        assert_eq!(rated_cas_latency(Picoseconds(12654), Picoseconds(333)), 38);
        // DDR5-5600: tCK 357 ps, tAA 14280 ps -> exactly CL40.
        assert_eq!(rated_cas_latency(Picoseconds(14280), Picoseconds(357)), 40);
        // A zero cycle time cannot divide by zero.
        assert_eq!(rated_cas_latency(Picoseconds(12654), Picoseconds(0)), 0);
    }

    #[test]
    fn read_le_u16_low_byte_first() {
        let spd = SpdImage::new(&[0x4D, 0x01]);
        assert_eq!(read_le_u16(&spd, 0).unwrap(), 0x014D); // 333
    }

    #[test]
    fn read_name_trims_padding_and_rejects_nonprintable() {
        // "TG" padded to 16 with spaces -> trimmed to "TG".
        let mut img = [b' '; 16];
        img[0] = b'T';
        img[1] = b'G';
        let spd = SpdImage::new(&img);
        assert_eq!(read_name(&spd, 0).unwrap(), Some("TG"));

        // All padding -> None, never an empty fabricated name.
        let blank = [b' '; 16];
        assert_eq!(read_name(&SpdImage::new(&blank), 0).unwrap(), None);

        // A non-printable byte -> None, and the rest of the decode is unaffected.
        let mut bad = [b'A'; 16];
        bad[3] = 0x80;
        assert_eq!(read_name(&SpdImage::new(&bad), 0).unwrap(), None);

        // Too short for the 16-byte field -> typed Truncated, never a panic.
        assert!(matches!(
            read_name(&SpdImage::new(&[b'A'; 8]), 0),
            Err(DecodeError::Truncated { .. })
        ));
    }

    #[test]
    fn absent_when_no_magic() {
        // A full-length zero image has neither magic: both regions are absent,
        // nothing is parsed, and nothing panics.
        let img = [0u8; 1024];
        assert_eq!(decode_xmp(&img).unwrap(), Xmp::Absent);
        assert_eq!(decode_expo(&img).unwrap(), Expo::Absent);
        let both = decode_vendor_profiles(&img).unwrap();
        assert_eq!(both.xmp, Xmp::Absent);
        assert_eq!(both.expo, Expo::Absent);
    }

    #[test]
    fn crafted_xmp_profile_decodes_rated_values() {
        // A minimal image carrying just the XMP magic, one enabled profile, and
        // that profile's tCK/tAA/tRCD/tRP/tRAS/voltages, built straight from the
        // pinned offsets. Values chosen to be DDR5-6000 38-38-38-78 at 1.25 V.
        let mut img = [0u8; 832];
        img[OFF_XMP_MAGIC] = XMP_MAGIC[0];
        img[OFF_XMP_MAGIC + 1] = XMP_MAGIC[1];
        img[OFF_XMP_ENABLE] = 0x01; // profile 1 only
        write_xmp_profile(&mut img, OFF_XMP_PROFILE1, 333, 12654, 12654, 12654, 25974);

        let Xmp::Present {
            profile1, profile2, ..
        } = decode_xmp(&img).unwrap()
        else {
            panic!("magic present, expected Xmp::Present");
        };
        assert!(profile2.is_none(), "profile 2 not enabled");
        let p = profile1.expect("profile 1 enabled");
        assert_eq!(p.profile_number, 1);
        assert_eq!(p.rated.data_rate_mt_s, 6000);
        assert_eq!(p.rated.cas_latency, 38);
        assert_eq!(p.rated.trcd, Picoseconds(12654));
        assert_eq!(p.rated.trp, Picoseconds(12654));
        assert_eq!(p.rated.tras, Picoseconds(25974));
        assert_eq!(p.rated.vdd, Millivolts(1250));
        assert_eq!(p.rated.vpp, Millivolts(1800));
    }

    #[test]
    fn crafted_expo_profile_decodes_rated_values() {
        let mut img = [0u8; 960];
        img[OFF_EXPO_MAGIC..OFF_EXPO_MAGIC + 4].copy_from_slice(&EXPO_MAGIC);
        write_expo_profile(&mut img, OFF_EXPO_PROFILE1, 333, 12654, 12654, 12654, 25974);

        let Expo::Present {
            profile1, profile2, ..
        } = decode_expo(&img).unwrap()
        else {
            panic!("magic present, expected Expo::Present");
        };
        assert!(
            profile2.is_none(),
            "profile 2 slot is zeroed -> unpopulated"
        );
        let p = profile1.expect("profile 1 populated");
        assert_eq!(p.profile_number, 1);
        assert_eq!(p.rated.data_rate_mt_s, 6000);
        assert_eq!(p.rated.cas_latency, 38);
        assert_eq!(p.rated.tras, Picoseconds(25974));
        assert_eq!(p.rated.vdd, Millivolts(1250));
        assert_eq!(p.rated.vpp, Millivolts(1800));
    }

    // --- test helpers ------------------------------------------------------

    fn write_le_u16(img: &mut [u8], off: usize, value: u16) {
        let [lo, hi] = value.to_le_bytes();
        img[off] = lo;
        img[off + 1] = hi;
    }

    fn write_xmp_profile(
        img: &mut [u8],
        base: usize,
        tck: u16,
        taa: u16,
        trcd: u16,
        trp: u16,
        tras: u16,
    ) {
        img[base + XMP_VPP] = 0x30; // 1.800 V
        img[base + XMP_VDD] = 0x25; // 1.250 V
        img[base + XMP_VDDQ] = 0x25; // 1.250 V
        write_le_u16(img, base + XMP_TCK, tck);
        write_le_u16(img, base + XMP_TAA, taa);
        write_le_u16(img, base + XMP_TRCD, trcd);
        write_le_u16(img, base + XMP_TRP, trp);
        write_le_u16(img, base + XMP_TRAS, tras);
    }

    fn write_expo_profile(
        img: &mut [u8],
        base: usize,
        tck: u16,
        taa: u16,
        trcd: u16,
        trp: u16,
        tras: u16,
    ) {
        img[base + EXPO_VDD] = 0x25; // 1.250 V
        img[base + EXPO_VDDQ] = 0x25; // 1.250 V
        img[base + EXPO_VPP] = 0x30; // 1.800 V
        write_le_u16(img, base + EXPO_TCK, tck);
        write_le_u16(img, base + EXPO_TAA, taa);
        write_le_u16(img, base + EXPO_TRCD, trcd);
        write_le_u16(img, base + EXPO_TRP, trp);
        write_le_u16(img, base + EXPO_TRAS, tras);
    }

    /// A small no_std sink so [`fmt::Display`] output can be asserted without
    /// `alloc`, mirroring the helper in the module-specific tests.
    struct Sink {
        data: [u8; 16],
        len: usize,
    }
    impl Sink {
        fn as_bytes(&self) -> &[u8] {
            &self.data[..self.len]
        }
    }
    impl fmt::Write for Sink {
        fn write_str(&mut self, s: &str) -> fmt::Result {
            for &b in s.as_bytes() {
                self.data[self.len] = b;
                self.len += 1;
            }
            Ok(())
        }
    }
    fn render(value: impl fmt::Display) -> Sink {
        use core::fmt::Write;
        let mut sink = Sink {
            data: [0; 16],
            len: 0,
        };
        write!(sink, "{value}").unwrap();
        sink
    }
}
