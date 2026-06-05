//! Linter framework and rules · validation beyond the CRC.
//!
//! The CRC proves the bytes survived transit; it says nothing about whether the
//! decoded values cohere. The linter is the project's second pillar: it decodes
//! the sections its rules need and reports structured [`Finding`]s for values
//! that are internally inconsistent even in a CRC-valid SPD.
//!
//! It holds the decode path's contract: `no_std`, no `alloc`, never panics.
//! Findings carry the structured values that produced them (raw numbers, not
//! preformatted strings) and are reported through a caller-supplied sink, so the
//! core never allocates a collection; the caller decides whether to count, print,
//! or collect them. A finding's human message is produced only at the edge, by
//! its [`fmt::Display`], which writes into a caller-backed formatter with `write!`
//! and so stays alloc-free.
//!
//! Phase 8 ships the framework and the first rule, capacity consistency
//! ([`Finding::NonIntegerDeviceCount`]). Phase 9b adds the timing-relationship,
//! clock-consistency, and speed-bin rules: now that the base JEDEC timings
//! (Phase 3) and the XMP/EXPO rated profiles (Phase 9a) are decoded, [`lint`]
//! also decodes them and checks that the timings are internally self-consistent
//! and standard where they should be. Each rule runs only where its inputs were
//! decoded, so a rule needing a field a vendor profile does not carry simply does
//! not run on that profile; no rule treats an overclock profile as a defect for
//! being tighter or faster than a JEDEC bin. Every later rule plugs into the same
//! [`lint`] dispatch and the same [`Finding`] enum.

use crate::identity::{IdentityAndBase, decode_identity_and_base};
use crate::timing::{CasLatencies, Picoseconds, Timings, decode_timings};
use crate::vendor::{Expo, RatedTimings, VendorProfiles, Xmp, decode_vendor_profiles};
use core::fmt;

/// The severity of a lint [`Finding`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum Severity {
    /// The SPD is internally inconsistent: a derived quantity is undefined or wrong.
    Error,
    /// The SPD decodes, but a value is suspect or outside the expected range.
    Warning,
    /// An informational observation; no problem is implied.
    Info,
}

/// A JEDEC timing ordering that must hold between two absolute-time parameters.
/// Carried by [`Finding::TimingOrderingViolation`] so the finding names which
/// relation was violated without a preformatted message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum TimingOrder {
    /// tRAS must be at least tRCD (active-to-precharge spans the RAS-to-CAS delay).
    RasGeRcd,
    /// tRC must be at least tRAS (the row cycle contains the active window).
    RcGeRas,
}

impl TimingOrder {
    /// The full ordering as text, e.g. `"tRAS >= tRCD"`.
    const fn as_str(self) -> &'static str {
        match self {
            TimingOrder::RasGeRcd => "tRAS >= tRCD",
            TimingOrder::RcGeRas => "tRC >= tRAS",
        }
    }

    /// The left-hand parameter name (the one that must be the larger).
    const fn larger(self) -> &'static str {
        match self {
            TimingOrder::RasGeRcd => "tRAS",
            TimingOrder::RcGeRas => "tRC",
        }
    }

    /// The right-hand parameter name (the floor).
    const fn smaller(self) -> &'static str {
        match self {
            TimingOrder::RasGeRcd => "tRCD",
            TimingOrder::RcGeRas => "tRAS",
        }
    }
}

/// A clock-quantised timing parameter checked for being a whole multiple of the
/// cycle time. Carried by [`Finding::NonIntegerClockTiming`]. tAA is not here: its
/// clock integrality is the operating-CAS-latency check
/// ([`Finding::NonIntegerCasLatency`]), so it is not double-counted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum ClockTimingParam {
    /// RAS-to-CAS delay (tRCD).
    Trcd,
    /// Row precharge time (tRP).
    Trp,
}

impl ClockTimingParam {
    /// The parameter name as text.
    const fn as_str(self) -> &'static str {
        match self {
            ClockTimingParam::Trcd => "tRCD",
            ClockTimingParam::Trp => "tRP",
        }
    }
}

/// A single lint finding: one detected inconsistency, carrying the structured
/// values that produced it rather than a preformatted message.
///
/// `#[non_exhaustive]`: later phases add one variant per rule, and downstream
/// code must not assume the set is closed. Each variant has a stable kebab-case
/// [`Finding::code`] and an inherent [`Finding::severity`]; the human message is
/// its [`fmt::Display`], written alloc-free into a caller-backed formatter.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub enum Finding {
    /// The primary bus width per channel is not a positive integer multiple of
    /// the SDRAM I/O device width, so the per-rank data device count
    /// (bus width / I/O width) is fractional and the module capacity is
    /// undefined. A CRC-valid SPD can still hold this inconsistency, which is why
    /// the linter checks it.
    NonIntegerDeviceCount {
        /// Primary bus width per channel, in bits (the dividend).
        bus_width_bits: u16,
        /// SDRAM I/O device width, in bits (the divisor).
        io_width_bits: u16,
    },

    /// The row-cycle identity tRC = tRAS + tRP does not hold. tRC is, by
    /// definition, the active-to-precharge time plus the precharge time, so any
    /// other value is internally inconsistent. Checked on the base block only
    /// (the vendor profiles do not carry tRC; Phase 9a deferred it).
    TrcIdentityMismatch {
        /// The stored tRC, in picoseconds.
        trc_ps: u32,
        /// tRAS, in picoseconds.
        tras_ps: u32,
        /// tRP, in picoseconds.
        trp_ps: u32,
    },

    /// A required JEDEC timing ordering is violated: the parameter that must be
    /// the larger is smaller than its floor. These are definitional orderings, so
    /// a violation is an internal inconsistency rather than an out-of-spec value.
    TimingOrderingViolation {
        /// Which ordering was violated.
        relation: TimingOrder,
        /// The value of the parameter that must be the larger, in picoseconds.
        larger_ps: u32,
        /// The value of the floor it fell below, in picoseconds.
        smaller_ps: u32,
    },

    /// The operating CAS latency (tAA / tCK) is not a whole number of clocks:
    /// tAA is not an integer multiple of the cycle time, so the CL is not
    /// realizable as an integer. A warning, since the SPD still decodes.
    NonIntegerCasLatency {
        /// tAA, in picoseconds.
        taa_ps: u32,
        /// The cycle time tCK, in picoseconds.
        tck_ps: u32,
    },

    /// The operating CAS latency CL (tAA / tCK, a whole number) is not a member
    /// of the SPD's own decoded supported-CAS-latencies set. Checked on the base
    /// block only, which carries that set. An internal inconsistency: the SPD
    /// runs at a CL it does not advertise as supported.
    CasLatencyNotSupported {
        /// The operating CAS latency, in clocks.
        cas_latency: u16,
    },

    /// A clock-quantised timing (tRCD or tRP) is not a whole multiple of the
    /// cycle time, so it is not realizable in whole clocks. A warning.
    NonIntegerClockTiming {
        /// Which parameter is not a whole multiple of tCK.
        param: ClockTimingParam,
        /// The parameter value, in picoseconds.
        value_ps: u32,
        /// The cycle time tCK, in picoseconds.
        tck_ps: u32,
    },

    /// The data rate is not a recognized JEDEC-standard DDR5 rate. Informational
    /// only: a vendor overclock profile legitimately ships a custom rate, so this
    /// is never an error.
    NonStandardDataRate {
        /// The data rate, in MT/s.
        data_rate_mt_s: u32,
    },
}

impl Finding {
    /// The inherent severity of this finding.
    #[must_use]
    pub fn severity(&self) -> Severity {
        match self {
            Finding::NonIntegerDeviceCount { .. }
            | Finding::TrcIdentityMismatch { .. }
            | Finding::TimingOrderingViolation { .. }
            | Finding::CasLatencyNotSupported { .. } => Severity::Error,
            Finding::NonIntegerCasLatency { .. } | Finding::NonIntegerClockTiming { .. } => {
                Severity::Warning
            }
            Finding::NonStandardDataRate { .. } => Severity::Info,
        }
    }

    /// A stable, kebab-case lint code identifying the rule that produced this
    /// finding. Stable across versions, so it can be referenced or filtered on.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Finding::NonIntegerDeviceCount { .. } => "non-integer-device-count",
            Finding::TrcIdentityMismatch { .. } => "trc-identity-mismatch",
            Finding::TimingOrderingViolation { .. } => "timing-ordering-violation",
            Finding::NonIntegerCasLatency { .. } => "non-integer-cas-latency",
            Finding::CasLatencyNotSupported { .. } => "cas-latency-not-supported",
            Finding::NonIntegerClockTiming { .. } => "non-integer-clock-timing",
            Finding::NonStandardDataRate { .. } => "non-standard-data-rate",
        }
    }
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Finding::NonIntegerDeviceCount {
                bus_width_bits,
                io_width_bits,
            } => write!(
                f,
                "primary bus width {bus_width_bits} bits is not a positive integer multiple of the {io_width_bits}-bit SDRAM I/O width, so the per-rank device count is fractional and the module capacity is undefined"
            ),
            Finding::TrcIdentityMismatch {
                trc_ps,
                tras_ps,
                trp_ps,
            } => write!(
                f,
                "tRC {trc_ps} ps does not equal tRAS + tRP ({tras_ps} + {trp_ps} = {} ps); the row-cycle identity tRC = tRAS + tRP is violated",
                tras_ps + trp_ps
            ),
            Finding::TimingOrderingViolation {
                relation,
                larger_ps,
                smaller_ps,
            } => write!(
                f,
                "{} {larger_ps} ps is less than {} {smaller_ps} ps; the ordering {} must hold",
                relation.larger(),
                relation.smaller(),
                relation.as_str()
            ),
            Finding::NonIntegerCasLatency { taa_ps, tck_ps } => write!(
                f,
                "tAA {taa_ps} ps is not a whole multiple of tCK {tck_ps} ps, so the operating CAS latency is not an integer number of clocks"
            ),
            Finding::CasLatencyNotSupported { cas_latency } => write!(
                f,
                "the operating CAS latency CL{cas_latency} (tAA / tCK) is not in the SPD's decoded supported-CAS-latency set"
            ),
            Finding::NonIntegerClockTiming {
                param,
                value_ps,
                tck_ps,
            } => write!(
                f,
                "{} {value_ps} ps is not a whole multiple of tCK {tck_ps} ps, so it is not realizable in whole clocks",
                param.as_str()
            ),
            Finding::NonStandardDataRate { data_rate_mt_s } => write!(
                f,
                "data rate {data_rate_mt_s} MT/s is not a recognized JEDEC-standard DDR5 rate"
            ),
        }
    }
}

/// Lint an SPD image, reporting each [`Finding`] to `sink`.
///
/// Decodes, once, the sections the current rules need; runs each rule; and skips
/// any rule whose section did not decode (a decode failure is the caller's decode
/// error to surface, not a lint finding). The sink callback keeps the core
/// alloc-free: the caller counts, prints, or collects. Never panics on any input
/// (the decode path is bounds-checked and the only rule does a guarded modulo).
///
/// Phase 8 decodes identity and base and runs the capacity-consistency rule.
/// Phase 9b additionally decodes the base timings and the vendor profiles and
/// runs the timing-relationship, clock-consistency, and speed-bin rules. Each
/// section is decoded independently and a rule runs only where its inputs are
/// present, so a short or partial image still lints what it can without panicking.
pub fn lint<F: FnMut(Finding)>(bytes: &[u8], sink: &mut F) {
    if let Ok(identity) = decode_identity_and_base(bytes) {
        check_capacity(&identity, sink);
    }
    if let Ok(timings) = decode_timings(bytes) {
        check_base_timings(&timings, sink);
    }
    if let Ok(profiles) = decode_vendor_profiles(bytes) {
        check_vendor_profiles(&profiles, sink);
    }
}

// --- Rules -----------------------------------------------------------------

/// Capacity consistency · the precondition of the JEDEC module-capacity formula.
///
/// The data device count per rank is the primary bus width per channel divided by
/// the SDRAM I/O device width; the full capacity is that count times the per-die
/// density, dies per package, package ranks per channel, and channels (pinned
/// against memtest86plus `parse_spd_ddr5`, which multiplies by the bus width and
/// divides by the I/O width). The one precondition a CRC-valid SPD can violate is
/// that this division be exact and positive: if the bus width is not a positive
/// integer multiple of the I/O width, the device count is fractional and the
/// capacity is undefined.
///
/// This checks only the divisibility precondition, which cannot overflow; it does
/// not compute the capacity product (a separate derived quantity and a needless
/// overflow surface).
fn check_capacity<F: FnMut(Finding)>(identity: &IdentityAndBase, sink: &mut F) {
    let bus_width_bits = identity.primary_bus_width_bits;
    let io_width_bits = identity.io_width.bits();

    // Guard `io_width_bits == 0` first so the modulo never divides by zero. A
    // zero bus width is a multiple of any width but gives a zero device count,
    // which is not positive, so it is flagged too.
    if io_width_bits == 0 || bus_width_bits == 0 || bus_width_bits % io_width_bits != 0 {
        sink(Finding::NonIntegerDeviceCount {
            bus_width_bits,
            io_width_bits,
        });
    }
}

/// The JEDEC-standard DDR5 data rates (MT/s), the speed-bin ladder in 400 MT/s
/// steps from 3200 to 8800. Pinned from the JEDEC DDR5 standard (JESD79-5 and its
/// addenda: the original defined bins up to 6400, JESD79-5A added the 5600/6400
/// timing definitions, and the April 2024 update added 8800). The fixture's base
/// 4800 and its vendor 5600 and 6000 are all on this list. A rate not on it is
/// informational only, because a vendor overclock profile may ship a custom rate.
const STANDARD_DDR5_RATES: [u32; 15] = [
    3200, 3600, 4000, 4400, 4800, 5200, 5600, 6000, 6400, 6800, 7200, 7600, 8000, 8400, 8800,
];

/// Run every base-block timing rule. The base block carries every timing, so all
/// of the timing-relationship, clock-consistency, and speed-bin rules apply.
fn check_base_timings(t: &Timings, sink: &mut dyn FnMut(Finding)) {
    check_trc_identity(t.trc, t.tras, t.trp, sink);
    check_ordering(TimingOrder::RasGeRcd, t.tras, t.trcd, sink);
    check_ordering(TimingOrder::RcGeRas, t.trc, t.tras, sink);
    check_cas_latency(t.taa, t.tckavg_min, Some(t.supported_cas_latencies), sink);
    check_clock_multiple(ClockTimingParam::Trcd, t.trcd, t.tckavg_min, sink);
    check_clock_multiple(ClockTimingParam::Trp, t.trp, t.tckavg_min, sink);
    check_standard_rate(t.base_data_rate_mt_s(), sink);
}

/// Run the timing rules on each populated vendor profile (XMP and EXPO). A vendor
/// profile carries only the Phase 9a subset (tCK, CL, tAA, tRCD, tRP, tRAS), so
/// only the rules whose inputs are in that subset run: the tRAS >= tRCD ordering,
/// the operating-CAS-latency integrality (without the supported-set membership,
/// which a profile does not carry), the tRCD/tRP clock-multiple checks, and the
/// recognized-rate check. The tRC identity, the tRC >= tRAS ordering, and the
/// CAS-latency-supported-set check are base-only. No rule here treats a profile
/// as a defect for being faster or tighter than a JEDEC bin.
fn check_vendor_profiles(profiles: &VendorProfiles, sink: &mut dyn FnMut(Finding)) {
    if let Xmp::Present {
        profile1, profile2, ..
    } = &profiles.xmp
    {
        for profile in [profile1, profile2].into_iter().flatten() {
            check_rated_profile(&profile.rated, sink);
        }
    }
    if let Expo::Present {
        profile1, profile2, ..
    } = &profiles.expo
    {
        for profile in [profile1, profile2].into_iter().flatten() {
            check_rated_profile(&profile.rated, sink);
        }
    }
}

/// Run the applicable rules on one vendor profile's rated timings.
fn check_rated_profile(r: &RatedTimings, sink: &mut dyn FnMut(Finding)) {
    check_ordering(TimingOrder::RasGeRcd, r.tras, r.trcd, sink);
    check_cas_latency(r.taa, r.cycle_time, None, sink);
    check_clock_multiple(ClockTimingParam::Trcd, r.trcd, r.cycle_time, sink);
    check_clock_multiple(ClockTimingParam::Trp, r.trp, r.cycle_time, sink);
    check_standard_rate(r.data_rate_mt_s, sink);
}

/// tRC = tRAS + tRP, the exact row-cycle identity (base block only). tRC is the
/// active-to-precharge time plus the precharge time by definition; any other
/// value is internally inconsistent. The fixture satisfies it (48640 = 32000 +
/// 16640). The sum cannot overflow: both operands are u16-derived picoseconds.
fn check_trc_identity(
    trc: Picoseconds,
    tras: Picoseconds,
    trp: Picoseconds,
    sink: &mut dyn FnMut(Finding),
) {
    if trc.0 != tras.0 + trp.0 {
        sink(Finding::TrcIdentityMismatch {
            trc_ps: trc.0,
            tras_ps: tras.0,
            trp_ps: trp.0,
        });
    }
}

/// A required ordering `larger >= smaller`; emits when it is violated.
fn check_ordering(
    relation: TimingOrder,
    larger: Picoseconds,
    smaller: Picoseconds,
    sink: &mut dyn FnMut(Finding),
) {
    if larger.0 < smaller.0 {
        sink(Finding::TimingOrderingViolation {
            relation,
            larger_ps: larger.0,
            smaller_ps: smaller.0,
        });
    }
}

/// The operating CAS latency: tAA must be a whole multiple of tCK (so CL is an
/// integer number of clocks), and, where a supported-CAS set is available (the
/// base block), the resulting CL must be in it. A non-integer CL is a warning and
/// short-circuits the set check, since the CL is then ill-defined; a CL outside
/// the set is an error. Guards a zero cycle time so it never divides by zero.
fn check_cas_latency(
    taa: Picoseconds,
    tck: Picoseconds,
    supported: Option<CasLatencies>,
    sink: &mut dyn FnMut(Finding),
) {
    let tck_ps = tck.0;
    if tck_ps == 0 {
        return;
    }
    if taa.0 % tck_ps != 0 {
        sink(Finding::NonIntegerCasLatency {
            taa_ps: taa.0,
            tck_ps,
        });
        return;
    }
    if let Some(set) = supported {
        let cas_latency = (taa.0 / tck_ps) as u16;
        if !set.contains(cas_latency) {
            sink(Finding::CasLatencyNotSupported { cas_latency });
        }
    }
}

/// A clock-quantised timing (tRCD or tRP) must be a whole multiple of tCK to be
/// realizable in whole clocks. Guards a zero cycle time.
fn check_clock_multiple(
    param: ClockTimingParam,
    value: Picoseconds,
    tck: Picoseconds,
    sink: &mut dyn FnMut(Finding),
) {
    let tck_ps = tck.0;
    if tck_ps != 0 && value.0 % tck_ps != 0 {
        sink(Finding::NonIntegerClockTiming {
            param,
            value_ps: value.0,
            tck_ps,
        });
    }
}

/// The data rate must be a recognized JEDEC-standard DDR5 rate. Informational
/// only: a vendor profile may legitimately ship a custom rate. A zero rate is the
/// degenerate case of a zero cycle time (no rate decoded), so it is skipped rather
/// than flagged, matching the zero-tCK guard on the clock-based rules.
fn check_standard_rate(data_rate_mt_s: u32, sink: &mut dyn FnMut(Finding)) {
    if data_rate_mt_s != 0 && !STANDARD_DDR5_RATES.contains(&data_rate_mt_s) {
        sink(Finding::NonStandardDataRate { data_rate_mt_s });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal identity-and-base image that decodes cleanly, with the
    /// I/O-width byte (6) and the bus-width byte (235) set by the caller. Every
    /// other byte the identity decoder reads is a valid placeholder. These are
    /// crafted geometry bytes to drive a rule the valid fixture does not trigger.
    fn identity_image(io_width_byte: u8, bus_width_byte: u8) -> [u8; 236] {
        let mut img = [0u8; 236];
        img[0] = 0x30; // SPD device size: 1024 bytes
        img[2] = 0x12; // DRAM device type: DDR5
        img[3] = 0x02; // module type: UDIMM
        img[4] = 0x04; // density 16 Gb, monolithic
        img[6] = io_width_byte; // I/O width (bits [7:5])
        img[7] = 0x00; // bank groups / banks: valid
        img[235] = bus_width_byte; // primary bus width (bits [2:0]) and channels
        img
    }

    #[test]
    fn inconsistent_geometry_emits_one_finding() {
        // Bus 8 bits with I/O x16: device count 8 / 16 = 0.5, fractional.
        let img = identity_image(0x40, 0x00); // I/O x16 (16 bits), bus 8 bits
        let mut count = 0u32;
        let mut last = None;
        lint(&img, &mut |finding| {
            count += 1;
            last = Some(finding);
        });

        assert_eq!(count, 1);
        let finding = last.expect("exactly one finding was emitted");
        assert_eq!(
            finding,
            Finding::NonIntegerDeviceCount {
                bus_width_bits: 8,
                io_width_bits: 16,
            }
        );
        assert_eq!(finding.severity(), Severity::Error);
        assert_eq!(finding.code(), "non-integer-device-count");
    }

    #[test]
    fn consistent_geometry_emits_nothing() {
        // Bus 64 bits with I/O x8: device count 64 / 8 = 8, a whole number.
        let img = identity_image(0x20, 0x03); // I/O x8 (8 bits), bus 64 bits
        let mut count = 0u32;
        lint(&img, &mut |_| {
            count += 1;
        });
        assert_eq!(count, 0);
    }

    // --- Phase 9b timing rules · crafted timings drive each violation -------

    /// Run a single rule, capturing the count and the last finding.
    fn run<R: FnOnce(&mut dyn FnMut(Finding))>(rule: R) -> (u32, Option<Finding>) {
        let mut count = 0u32;
        let mut last = None;
        let mut sink = |finding: Finding| {
            count += 1;
            last = Some(finding);
        };
        rule(&mut sink);
        (count, last)
    }

    /// A minimal image that `decode_timings` accepts, with tCK, tAA, and the
    /// five-byte CAS-latency bitmask set by the caller. Lets a test obtain a real
    /// decoded `CasLatencies` set (whose inner field is private) for the
    /// supported-set rule.
    fn timing_image(tck: u16, taa: u16, cas: [u8; 5]) -> [u8; 94] {
        let mut img = [0u8; 94];
        img[20..22].copy_from_slice(&tck.to_le_bytes()); // tCKAVGmin
        img[24..29].copy_from_slice(&cas); // supported CAS latencies bitmask
        img[30..32].copy_from_slice(&taa.to_le_bytes()); // tAA
        img
    }

    #[test]
    fn broken_trc_identity_is_error() {
        let (count, last) = run(|sink| {
            check_trc_identity(
                Picoseconds(48641),
                Picoseconds(32000),
                Picoseconds(16640),
                sink,
            );
        });
        assert_eq!(count, 1);
        let finding = last.unwrap();
        assert_eq!(
            finding,
            Finding::TrcIdentityMismatch {
                trc_ps: 48641,
                tras_ps: 32000,
                trp_ps: 16640,
            }
        );
        assert_eq!(finding.severity(), Severity::Error);
        assert_eq!(finding.code(), "trc-identity-mismatch");
    }

    #[test]
    fn exact_trc_identity_emits_nothing() {
        // The fixture's own values: 48640 = 32000 + 16640.
        let (count, _) = run(|sink| {
            check_trc_identity(
                Picoseconds(48640),
                Picoseconds(32000),
                Picoseconds(16640),
                sink,
            );
        });
        assert_eq!(count, 0);
    }

    #[test]
    fn ras_below_rcd_is_ordering_error() {
        let (count, last) = run(|sink| {
            check_ordering(
                TimingOrder::RasGeRcd,
                Picoseconds(10000),
                Picoseconds(12000),
                sink,
            );
        });
        assert_eq!(count, 1);
        let finding = last.unwrap();
        assert_eq!(
            finding,
            Finding::TimingOrderingViolation {
                relation: TimingOrder::RasGeRcd,
                larger_ps: 10000,
                smaller_ps: 12000,
            }
        );
        assert_eq!(finding.severity(), Severity::Error);
        assert_eq!(finding.code(), "timing-ordering-violation");

        // The fixture's tRAS 32000 >= tRCD 16640 emits nothing.
        let (clean, _) = run(|sink| {
            check_ordering(
                TimingOrder::RasGeRcd,
                Picoseconds(32000),
                Picoseconds(16640),
                sink,
            );
        });
        assert_eq!(clean, 0);
    }

    #[test]
    fn non_integer_cas_latency_is_warning() {
        // tAA 12655 ps is not a multiple of tCK 333 ps (38 * 333 = 12654).
        let (count, last) = run(|sink| {
            check_cas_latency(Picoseconds(12655), Picoseconds(333), None, sink);
        });
        assert_eq!(count, 1);
        let finding = last.unwrap();
        assert_eq!(
            finding,
            Finding::NonIntegerCasLatency {
                taa_ps: 12655,
                tck_ps: 333,
            }
        );
        assert_eq!(finding.severity(), Severity::Warning);
        assert_eq!(finding.code(), "non-integer-cas-latency");

        // 12654 / 333 = 38 exactly, and no supported set to check: nothing.
        let (clean, _) =
            run(|sink| check_cas_latency(Picoseconds(12654), Picoseconds(333), None, sink));
        assert_eq!(clean, 0);
    }

    #[test]
    fn cas_latency_outside_supported_set_is_error() {
        // tCK 416, tAA 16640 -> CL40, but the supported set holds only CL22.
        let only_cl22 = decode_timings(&timing_image(416, 16640, [0x02, 0, 0, 0, 0]))
            .unwrap()
            .supported_cas_latencies;
        let (count, last) = run(|sink| {
            check_cas_latency(Picoseconds(16640), Picoseconds(416), Some(only_cl22), sink);
        });
        assert_eq!(count, 1);
        let finding = last.unwrap();
        assert_eq!(finding, Finding::CasLatencyNotSupported { cas_latency: 40 });
        assert_eq!(finding.severity(), Severity::Error);
        assert_eq!(finding.code(), "cas-latency-not-supported");

        // With CL40 in the set (bit 10 -> byte 25 bit 2), nothing is emitted.
        let with_cl40 = decode_timings(&timing_image(416, 16640, [0, 0x04, 0, 0, 0]))
            .unwrap()
            .supported_cas_latencies;
        let (clean, _) = run(|sink| {
            check_cas_latency(Picoseconds(16640), Picoseconds(416), Some(with_cl40), sink);
        });
        assert_eq!(clean, 0);
    }

    #[test]
    fn non_integer_clock_timing_is_warning() {
        // tRCD 12655 ps is not a whole multiple of tCK 333 ps.
        let (count, last) = run(|sink| {
            check_clock_multiple(
                ClockTimingParam::Trcd,
                Picoseconds(12655),
                Picoseconds(333),
                sink,
            );
        });
        assert_eq!(count, 1);
        let finding = last.unwrap();
        assert_eq!(
            finding,
            Finding::NonIntegerClockTiming {
                param: ClockTimingParam::Trcd,
                value_ps: 12655,
                tck_ps: 333,
            }
        );
        assert_eq!(finding.severity(), Severity::Warning);
        assert_eq!(finding.code(), "non-integer-clock-timing");

        // 12654 / 333 = 38 exactly: nothing.
        let (clean, _) = run(|sink| {
            check_clock_multiple(
                ClockTimingParam::Trp,
                Picoseconds(12654),
                Picoseconds(333),
                sink,
            );
        });
        assert_eq!(clean, 0);
    }

    #[test]
    fn non_standard_rate_is_info() {
        // 5000 MT/s is not a JEDEC bin (the ladder steps 4800, 5200, ...).
        let (count, last) = run(|sink| check_standard_rate(5000, sink));
        assert_eq!(count, 1);
        let finding = last.unwrap();
        assert_eq!(
            finding,
            Finding::NonStandardDataRate {
                data_rate_mt_s: 5000,
            }
        );
        assert_eq!(finding.severity(), Severity::Info);
        assert_eq!(finding.code(), "non-standard-data-rate");

        // The fixture's three rates are all standard, and a zero rate (no cycle
        // time decoded) is skipped rather than flagged.
        for rate in [4800u32, 5600, 6000, 0] {
            let (clean, _) = run(|sink| check_standard_rate(rate, sink));
            assert_eq!(clean, 0, "{rate} MT/s should emit nothing");
        }
    }

    #[test]
    fn zero_cycle_time_skips_clock_checks_without_dividing() {
        // A zero tCK must not divide by zero; the clock-based rules just skip.
        let (cas, _) =
            run(|sink| check_cas_latency(Picoseconds(12654), Picoseconds(0), None, sink));
        assert_eq!(cas, 0);
        let (mult, _) = run(|sink| {
            check_clock_multiple(
                ClockTimingParam::Trcd,
                Picoseconds(12654),
                Picoseconds(0),
                sink,
            );
        });
        assert_eq!(mult, 0);
    }
}
