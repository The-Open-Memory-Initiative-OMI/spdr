//! Base JEDEC timing block.
//!
//! Decodes the base configuration AC timing parameters into typed values. This
//! is the JEDEC base the module guarantees; it reads slower than the DDR5-6000
//! box rating because the advertised speed lives in the XMP and EXPO profiles
//! (Phase 9), not in the base block. A base speed below 6000 is correct here.
//!
//! DDR5 does not use DDR4's medium/fine time-base scheme. Each absolute-time
//! parameter is stored as a little-endian 16-bit integer that is already in its
//! unit with 1-unit granularity: picoseconds for most parameters, nanoseconds
//! for the tRFC family (refresh times are too large for 16-bit picoseconds). The
//! canonical fine unit here is the picosecond, named in [`Picoseconds`], so no
//! caller has to guess; the tRFC values are normalised from their stored
//! nanoseconds to picoseconds on decode. The bank-group-class parameters are
//! stored as both a picosecond value and an adjacent clock-count (nCK) lower
//! limit; the nCK count is kept distinct in [`ClockCycles`].
//!
//! Every offset, unit, and byte order is pinned against open references; see
//! `docs/implementations/2026-06-04-phase-3-timing.md` for per-parameter
//! provenance.

use crate::error::DecodeError;
use crate::reader::SpdImage;
use core::fmt;

// Byte offsets (JESD400-5 base configuration timing block).
const OFF_TCKAVG_MIN: usize = 20;
const OFF_TCKAVG_MAX: usize = 22;
const OFF_CAS_LATENCIES: usize = 24; // bytes 24..=28
const CAS_LATENCY_BYTES: usize = 5;
const OFF_TAA: usize = 30;
const OFF_TRCD: usize = 32;
const OFF_TRP: usize = 34;
const OFF_TRAS: usize = 36;
const OFF_TRC: usize = 38;
const OFF_TWR: usize = 40;
const OFF_TRFC1: usize = 42;
const OFF_TRFC2: usize = 44;
const OFF_TRFCSB: usize = 46;
const OFF_TRRD_L: usize = 70;
const OFF_TCCD_L: usize = 73;
const OFF_TCCD_L_WR: usize = 76;
const OFF_TCCD_L_WR2: usize = 79;
const OFF_TFAW: usize = 82;
const OFF_TWTR_L: usize = 85;
const OFF_TWTR_S: usize = 88;
const OFF_TRTP: usize = 91;

/// A duration in picoseconds, the canonical fine unit for DDR5 absolute-time
/// timings. The unit is named in the type so no caller has to guess: every
/// absolute-time field is normalised to picoseconds on decode (the tRFC family,
/// stored in nanoseconds, is scaled up by 1000).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Picoseconds(pub u32);

impl Picoseconds {
    /// The value in picoseconds.
    #[must_use]
    pub const fn picoseconds(self) -> u32 {
        self.0
    }
}

/// A timing expressed as a count of clock cycles (nCK). Kept distinct from the
/// absolute-time [`Picoseconds`] rather than folded into a single unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ClockCycles(pub u8);

impl ClockCycles {
    /// The number of clock cycles.
    #[must_use]
    pub const fn cycles(self) -> u8 {
        self.0
    }
}

/// A bank-group-class timing that DDR5 stores as both an absolute-time floor (in
/// picoseconds) and a clock-count floor (in nCK). The effective constraint a
/// controller applies is the larger of the two, so both are preserved here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct TimingPair {
    /// The absolute-time lower limit.
    pub time: Picoseconds,
    /// The clock-count lower limit.
    pub clocks: ClockCycles,
}

/// The set of supported CAS latencies, as a 40-bit mask over the five-byte
/// "CAS Latencies Supported" field. Bit `i` set means CL `20 + 2*i` is
/// supported (DDR5 CAS latencies are even, starting at CL20). The set is held
/// without allocation; iterate it with [`CasLatencies::iter`].
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct CasLatencies(u64);

impl CasLatencies {
    const BASE_CL: u16 = 20;
    const BIT_COUNT: u32 = (CAS_LATENCY_BYTES * 8) as u32;

    /// Whether `cl` is in the supported set.
    #[must_use]
    pub fn contains(self, cl: u16) -> bool {
        if cl < Self::BASE_CL || (cl - Self::BASE_CL) % 2 != 0 {
            return false;
        }
        let bit = u32::from((cl - Self::BASE_CL) / 2);
        bit < Self::BIT_COUNT && (self.0 >> bit) & 1 != 0
    }

    /// Iterate the supported CAS latency values in ascending order.
    pub fn iter(self) -> impl Iterator<Item = u16> {
        (0..Self::BIT_COUNT).filter_map(move |bit| {
            if (self.0 >> bit) & 1 != 0 {
                Some(Self::BASE_CL + 2 * bit as u16)
            } else {
                None
            }
        })
    }
}

impl fmt::Debug for CasLatencies {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

// Serialize as the ascending list of supported CL values, matching `Debug`.
// The default derive on the newtype would emit the raw 40-bit mask integer,
// which is clearly misleading (it reads as a meaningless number), so this is one
// of the few places the brief's "attributes only where a default misleads"
// becomes a hand-written impl. `collect_seq` is `no_std`- and `alloc`-free.
#[cfg(feature = "serde")]
impl serde::Serialize for CasLatencies {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_seq(self.iter())
    }
}

/// The decoded base JEDEC timing parameters of an SPD image.
///
/// Absolute-time parameters are [`Picoseconds`]; the bank-group-class parameters
/// are [`TimingPair`] (a picosecond floor plus an nCK floor). Construct it with
/// [`decode_timings`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Timings {
    /// Minimum average clock cycle time; sets the base data rate.
    pub tckavg_min: Picoseconds,
    /// Maximum average clock cycle time.
    pub tckavg_max: Picoseconds,
    /// Supported CAS latencies.
    pub supported_cas_latencies: CasLatencies,
    /// CAS latency time (tAA).
    pub taa: Picoseconds,
    /// RAS-to-CAS delay (tRCD).
    pub trcd: Picoseconds,
    /// Row precharge time (tRP).
    pub trp: Picoseconds,
    /// Active-to-precharge time (tRAS).
    pub tras: Picoseconds,
    /// Active-to-active / refresh cycle time (tRC).
    pub trc: Picoseconds,
    /// Write recovery time (tWR).
    pub twr: Picoseconds,
    /// Normal refresh recovery time (tRFC1).
    pub trfc1: Picoseconds,
    /// Fine-granularity refresh recovery time (tRFC2).
    pub trfc2: Picoseconds,
    /// Same-bank refresh recovery time (tRFCsb).
    pub trfcsb: Picoseconds,
    /// Activate-to-activate, same bank group (tRRD_L).
    pub trrd_l: TimingPair,
    /// CAS-to-CAS, same bank group (tCCD_L).
    pub tccd_l: TimingPair,
    /// Write CAS-to-CAS, same bank group (tCCD_L_WR).
    pub tccd_l_wr: TimingPair,
    /// Second-write CAS-to-CAS, same bank group (tCCD_L_WR2).
    pub tccd_l_wr2: TimingPair,
    /// Four-activate window (tFAW).
    pub tfaw: TimingPair,
    /// Write-to-read, same bank group (tWTR_L).
    pub twtr_l: TimingPair,
    /// Write-to-read, different bank group (tWTR_S).
    pub twtr_s: TimingPair,
    /// Read-to-precharge (tRTP).
    pub trtp: TimingPair,
}

impl Timings {
    /// The base JEDEC data rate in MT/s implied by `tckavg_min`, rounded to the
    /// nearest 100. For this fixture this is the base fallback (DDR5-4800), not
    /// the advertised XMP/EXPO profile rate.
    #[must_use]
    pub fn base_data_rate_mt_s(&self) -> u32 {
        data_rate_mt_s(self.tckavg_min)
    }
}

/// Decode the base JEDEC timing block from a raw SPD image.
///
/// Reads every byte through [`SpdImage`], so a short image yields
/// [`DecodeError::Truncated`] rather than a panic. The base timing parameters
/// are raw integer encodings with no reserved values, so truncation is the only
/// error this can return.
pub fn decode_timings(bytes: &[u8]) -> Result<Timings, DecodeError> {
    let spd = SpdImage::new(bytes);

    Ok(Timings {
        tckavg_min: ps_units(read_le_u16(&spd, OFF_TCKAVG_MIN)?),
        tckavg_max: ps_units(read_le_u16(&spd, OFF_TCKAVG_MAX)?),
        supported_cas_latencies: decode_cas_latencies(&spd)?,
        taa: ps_units(read_le_u16(&spd, OFF_TAA)?),
        trcd: ps_units(read_le_u16(&spd, OFF_TRCD)?),
        trp: ps_units(read_le_u16(&spd, OFF_TRP)?),
        tras: ps_units(read_le_u16(&spd, OFF_TRAS)?),
        trc: ps_units(read_le_u16(&spd, OFF_TRC)?),
        twr: ps_units(read_le_u16(&spd, OFF_TWR)?),
        trfc1: ns_units(read_le_u16(&spd, OFF_TRFC1)?),
        trfc2: ns_units(read_le_u16(&spd, OFF_TRFC2)?),
        trfcsb: ns_units(read_le_u16(&spd, OFF_TRFCSB)?),
        trrd_l: read_pair(&spd, OFF_TRRD_L)?,
        tccd_l: read_pair(&spd, OFF_TCCD_L)?,
        tccd_l_wr: read_pair(&spd, OFF_TCCD_L_WR)?,
        tccd_l_wr2: read_pair(&spd, OFF_TCCD_L_WR2)?,
        tfaw: read_pair(&spd, OFF_TFAW)?,
        twtr_l: read_pair(&spd, OFF_TWTR_L)?,
        twtr_s: read_pair(&spd, OFF_TWTR_S)?,
        trtp: read_pair(&spd, OFF_TRTP)?,
    })
}

// --- Encoding helpers ------------------------------------------------------

/// A raw picosecond-encoded timing: the stored 16-bit value is already in
/// picoseconds with 1 ps granularity (no DDR4 medium/fine time base).
fn ps_units(raw: u16) -> Picoseconds {
    Picoseconds(u32::from(raw))
}

/// A raw nanosecond-encoded timing (the tRFC family), normalised to the
/// canonical picosecond unit.
fn ns_units(raw: u16) -> Picoseconds {
    Picoseconds(u32::from(raw) * 1000)
}

/// Read a little-endian 16-bit value at `offset` through the reader.
fn read_le_u16(spd: &SpdImage, offset: usize) -> Result<u16, DecodeError> {
    Ok(u16::from_le_bytes([
        spd.byte(offset)?,
        spd.byte(offset + 1)?,
    ]))
}

/// Read a `[ps u16][nCK u8]` triple at `offset` into a [`TimingPair`].
fn read_pair(spd: &SpdImage, offset: usize) -> Result<TimingPair, DecodeError> {
    Ok(TimingPair {
        time: ps_units(read_le_u16(spd, offset)?),
        clocks: ClockCycles(spd.byte(offset + 2)?),
    })
}

/// Assemble the five-byte CAS-latency field into a 40-bit little-endian mask.
fn decode_cas_latencies(spd: &SpdImage) -> Result<CasLatencies, DecodeError> {
    let mut mask: u64 = 0;
    for i in 0..CAS_LATENCY_BYTES {
        mask |= u64::from(spd.byte(OFF_CAS_LATENCIES + i)?) << (8 * i);
    }
    Ok(CasLatencies(mask))
}

/// Data rate in MT/s for a clock period, rounded to the nearest 100; 0 if the
/// period is zero.
fn data_rate_mt_s(tckavg_min: Picoseconds) -> u32 {
    if tckavg_min.0 == 0 {
        return 0;
    }
    let raw = 2_000_000 / tckavg_min.0;
    ((raw + 50) / 100) * 100
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ps_units_is_identity_in_picoseconds() {
        // DDR5 stores the value already in ps, 1 ps granularity (no MTB/FTB).
        assert_eq!(ps_units(0x01A0), Picoseconds(416));
        assert_eq!(ps_units(30000), Picoseconds(30000));
    }

    #[test]
    fn ns_units_scales_nanoseconds_to_picoseconds() {
        // The tRFC family is stored in ns; normalise to the canonical ps unit.
        assert_eq!(ns_units(295), Picoseconds(295_000));
        assert_eq!(ns_units(130), Picoseconds(130_000));
    }

    #[test]
    fn le_u16_reads_low_byte_first() {
        let spd = SpdImage::new(&[0xA0, 0x01]);
        assert_eq!(read_le_u16(&spd, 0).unwrap(), 0x01A0);
    }

    #[test]
    fn read_pair_reads_ps_then_nck() {
        // 0x1388 = 5000 ps, then 0x08 = 8 nCK.
        let spd = SpdImage::new(&[0x88, 0x13, 0x08]);
        let pair = read_pair(&spd, 0).unwrap();
        assert_eq!(pair.time, Picoseconds(5000));
        assert_eq!(pair.clocks, ClockCycles(8));
    }

    #[test]
    fn cas_latency_bitmask_decodes_to_even_cl_set() {
        // Fixture bytes 24-28 = FE 07 00 00 00 at their absolute offsets.
        let mut img = [0u8; OFF_CAS_LATENCIES + CAS_LATENCY_BYTES];
        img[OFF_CAS_LATENCIES] = 0xFE;
        img[OFF_CAS_LATENCIES + 1] = 0x07;
        let spd = SpdImage::new(&img);
        let cl = decode_cas_latencies(&spd).unwrap();

        assert!(cl.contains(22));
        assert!(cl.contains(40));
        assert!(!cl.contains(20)); // bit 0 clear
        assert!(!cl.contains(42)); // bit 11 clear
        assert!(!cl.contains(23)); // odd CLs are never in the set
        assert_eq!(cl.iter().count(), 10);
        assert_eq!(cl.iter().next(), Some(22));
        assert_eq!(cl.iter().last(), Some(40));
    }

    #[test]
    fn data_rate_rounds_to_nearest_hundred() {
        // tCKAVGmin 416 ps => 4807 MT/s => DDR5-4800 base.
        assert_eq!(data_rate_mt_s(Picoseconds(416)), 4800);
        assert_eq!(data_rate_mt_s(Picoseconds(0)), 0);
    }
}
