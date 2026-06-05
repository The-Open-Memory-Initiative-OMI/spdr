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
//! ([`Finding::NonIntegerDeviceCount`]). Every later rule plugs into the same
//! [`lint`] dispatch and the same [`Finding`] enum.

use crate::identity::{IdentityAndBase, decode_identity_and_base};
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
}

impl Finding {
    /// The inherent severity of this finding.
    #[must_use]
    pub fn severity(&self) -> Severity {
        match self {
            Finding::NonIntegerDeviceCount { .. } => Severity::Error,
        }
    }

    /// A stable, kebab-case lint code identifying the rule that produced this
    /// finding. Stable across versions, so it can be referenced or filtered on.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Finding::NonIntegerDeviceCount { .. } => "non-integer-device-count",
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
pub fn lint<F: FnMut(Finding)>(bytes: &[u8], sink: &mut F) {
    if let Ok(identity) = decode_identity_and_base(bytes) {
        check_capacity(&identity, sink);
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
}
