//! Identity and base SDRAM configuration block.
//!
//! Decodes SPD bytes 0..=7 (base configuration and first-SDRAM parameters) plus
//! the module-organization bytes 234 and 235 into typed values. Every offset and
//! encoding is pinned against open references, not from memory; the per-field
//! provenance is recorded in
//! `docs/implementations/2026-06-04-phase-1-foundation.md`.
//!
//! Each field is decoded by a small private function so that every field decoder
//! has a focused unit test built straight from the encoding rule. The public
//! entry point [`decode_identity_and_base`] composes them over the zero-copy
//! [`SpdImage`].

use crate::error::DecodeError;
use crate::reader::SpdImage;
use core::fmt;

// Byte offsets within the SPD image (JESD400-5 base configuration block).
const OFF_SPD_SIZE: usize = 0;
const OFF_SPD_REVISION: usize = 1;
const OFF_DEVICE_TYPE: usize = 2;
const OFF_MODULE_TYPE: usize = 3;
const OFF_FIRST_DENSITY_PACKAGE: usize = 4;
const OFF_FIRST_ADDRESSING: usize = 5;
const OFF_FIRST_IO_WIDTH: usize = 6;
const OFF_FIRST_BANK_GROUPS: usize = 7;
const OFF_MODULE_ORGANIZATION: usize = 234;
const OFF_MEMORY_CHANNEL_BUS_WIDTH: usize = 235;

/// SPD revision, decoded from byte 1 as two plain (non-BCD) hex nibbles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpdRevision {
    /// High nibble: major revision.
    pub major: u8,
    /// Low nibble: minor revision.
    pub minor: u8,
}

impl fmt::Display for SpdRevision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// DRAM device type, from byte 2 (the whole-byte "key byte 1").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceType {
    Ddr3,
    Ddr4,
    Ddr5,
    Lpddr5,
}

impl DeviceType {
    /// The JEDEC name for this device type.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            DeviceType::Ddr3 => "DDR3 SDRAM",
            DeviceType::Ddr4 => "DDR4 SDRAM",
            DeviceType::Ddr5 => "DDR5 SDRAM",
            DeviceType::Lpddr5 => "LPDDR5 SDRAM",
        }
    }
}

impl fmt::Display for DeviceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Base module type, from the low nibble of byte 3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleType {
    /// Registered DIMM.
    Rdimm,
    /// Unbuffered DIMM.
    Udimm,
    /// Small-outline DIMM.
    Sodimm,
    /// Load-reduced DIMM.
    Lrdimm,
    /// Clocked unbuffered DIMM.
    Cudimm,
    /// Clocked small-outline unbuffered DIMM.
    Csoudimm,
    /// Multiplexed-rank DIMM.
    Mrdimm,
    /// Compression-attached memory module, gen 2.
    Camm2,
    /// Differential DIMM.
    Ddimm,
    /// Soldered-down memory (no module).
    SolderDown,
}

impl ModuleType {
    /// The JEDEC name for this module type.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            ModuleType::Rdimm => "RDIMM",
            ModuleType::Udimm => "UDIMM",
            ModuleType::Sodimm => "SODIMM",
            ModuleType::Lrdimm => "LRDIMM",
            ModuleType::Cudimm => "CUDIMM",
            ModuleType::Csoudimm => "CSOUDIMM",
            ModuleType::Mrdimm => "MRDIMM",
            ModuleType::Camm2 => "CAMM2",
            ModuleType::Ddimm => "DDIMM",
            ModuleType::SolderDown => "solder down",
        }
    }
}

impl fmt::Display for ModuleType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// SDRAM density per die, from the low five bits of byte 4 (the JEDEC table).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DensityPerDie {
    Gb4,
    Gb8,
    Gb12,
    Gb16,
    Gb24,
    Gb32,
    Gb48,
    Gb64,
}

impl DensityPerDie {
    /// Density of a single die in gigabits.
    #[must_use]
    pub const fn gigabits(self) -> u16 {
        match self {
            DensityPerDie::Gb4 => 4,
            DensityPerDie::Gb8 => 8,
            DensityPerDie::Gb12 => 12,
            DensityPerDie::Gb16 => 16,
            DensityPerDie::Gb24 => 24,
            DensityPerDie::Gb32 => 32,
            DensityPerDie::Gb48 => 48,
            DensityPerDie::Gb64 => 64,
        }
    }
}

/// SDRAM I/O device width (DQ pins per device), from bits [7:5] of byte 6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoWidth {
    X4,
    X8,
    X16,
    X32,
}

impl IoWidth {
    /// Number of data (DQ) pins per device.
    #[must_use]
    pub const fn bits(self) -> u16 {
        match self {
            IoWidth::X4 => 4,
            IoWidth::X8 => 8,
            IoWidth::X16 => 16,
            IoWidth::X32 => 32,
        }
    }
}

/// Number of bank groups per die, from bits [7:5] of byte 7.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BankGroups {
    One,
    Two,
    Four,
    Eight,
}

impl BankGroups {
    /// Number of bank groups.
    #[must_use]
    pub const fn count(self) -> u8 {
        match self {
            BankGroups::One => 1,
            BankGroups::Two => 2,
            BankGroups::Four => 4,
            BankGroups::Eight => 8,
        }
    }
}

/// Number of banks within each bank group, from bits [2:0] of byte 7.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BanksPerBankGroup {
    One,
    Two,
    Four,
}

impl BanksPerBankGroup {
    /// Number of banks per bank group.
    #[must_use]
    pub const fn count(self) -> u8 {
        match self {
            BanksPerBankGroup::One => 1,
            BanksPerBankGroup::Two => 2,
            BanksPerBankGroup::Four => 4,
        }
    }
}

/// SDRAM package construction, from bits [7:5] of byte 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageType {
    /// A single die in the package.
    Monolithic,
    /// Two dies in one package, not vertically stacked (DDP).
    DualDie,
    /// A vertically stacked 3DS package (die count carried separately).
    ThreeDs,
}

impl PackageType {
    /// A short human-readable name for the package construction.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            PackageType::Monolithic => "monolithic",
            PackageType::DualDie => "dual-die (DDP)",
            PackageType::ThreeDs => "3DS",
        }
    }
}

impl fmt::Display for PackageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The decoded identity and base SDRAM configuration of an SPD image.
///
/// Every field is a `Copy` scalar or an exhaustive enum borrowed from nothing,
/// so the whole struct is `Copy`. Construct it with [`decode_identity_and_base`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdentityAndBase {
    /// Total addressable size of the SPD device in bytes (byte 0, bits [6:4]).
    pub spd_bytes_total: u16,
    /// SPD revision (byte 1).
    pub spd_revision: SpdRevision,
    /// DRAM device type (byte 2); for a DDR5 module this is [`DeviceType::Ddr5`].
    pub device_type: DeviceType,
    /// Base module type (byte 3, low nibble).
    pub module_type: ModuleType,
    /// Hybrid-module flag (byte 3, bit 7).
    pub hybrid: bool,
    /// SDRAM density per die (byte 4, bits [4:0]).
    pub density_per_die: DensityPerDie,
    /// SDRAM package construction (byte 4, bits [7:5]).
    pub package_type: PackageType,
    /// Number of dies in the SDRAM package (byte 4, bits [7:5]).
    pub die_count: u8,
    /// Number of row address bits (byte 5, bits [4:0], base 16).
    pub row_address_bits: u8,
    /// Number of column address bits (byte 5, bits [7:5], base 10).
    pub column_address_bits: u8,
    /// SDRAM I/O device width (byte 6, bits [7:5]).
    pub io_width: IoWidth,
    /// Bank groups per die (byte 7, bits [7:5]).
    pub bank_groups: BankGroups,
    /// Banks per bank group (byte 7, bits [2:0]).
    pub banks_per_bank_group: BanksPerBankGroup,
    /// Package ranks per channel (byte 234, bits [5:3], base 1).
    pub package_ranks_per_channel: u8,
    /// Whether the rank organization is asymmetric (byte 234, bit 6).
    pub rank_mix_asymmetric: bool,
    /// Number of channels (sub-channels) per DIMM (byte 235, bits [7:5]).
    pub channels_per_dimm: u8,
    /// Primary bus width per channel in bits (byte 235, bits [2:0]).
    pub primary_bus_width_bits: u16,
}

/// Decode the identity and base SDRAM configuration from a raw SPD image.
///
/// Returns [`DecodeError::Truncated`] if the image is shorter than the highest
/// byte read (byte 235), or [`DecodeError::UnknownEnum`] if a spec-defined
/// enumeration field holds a reserved or undefined value.
pub fn decode_identity_and_base(bytes: &[u8]) -> Result<IdentityAndBase, DecodeError> {
    let spd = SpdImage::new(bytes);

    let spd_bytes_total = decode_spd_bytes_total(spd.byte(OFF_SPD_SIZE)?)?;
    let spd_revision = decode_spd_revision(spd.byte(OFF_SPD_REVISION)?);
    let device_type = decode_device_type(spd.byte(OFF_DEVICE_TYPE)?)?;
    let (module_type, hybrid) = decode_module_type(spd.byte(OFF_MODULE_TYPE)?)?;

    let first_density_package = spd.byte(OFF_FIRST_DENSITY_PACKAGE)?;
    let density_per_die = decode_density_per_die(first_density_package)?;
    let (package_type, die_count) = decode_package(first_density_package)?;

    let first_addressing = spd.byte(OFF_FIRST_ADDRESSING)?;
    let row_address_bits = decode_row_address_bits(first_addressing);
    let column_address_bits = decode_column_address_bits(first_addressing);

    let io_width = decode_io_width(spd.byte(OFF_FIRST_IO_WIDTH)?)?;

    let first_bank_groups = spd.byte(OFF_FIRST_BANK_GROUPS)?;
    let bank_groups = decode_bank_groups(first_bank_groups)?;
    let banks_per_bank_group = decode_banks_per_bank_group(first_bank_groups)?;

    let (package_ranks_per_channel, rank_mix_asymmetric) =
        decode_package_ranks(spd.byte(OFF_MODULE_ORGANIZATION)?);

    let bus_width = spd.byte(OFF_MEMORY_CHANNEL_BUS_WIDTH)?;
    let primary_bus_width_bits = decode_primary_bus_width(bus_width)?;
    let channels_per_dimm = decode_channels_per_dimm(bus_width)?;

    Ok(IdentityAndBase {
        spd_bytes_total,
        spd_revision,
        device_type,
        module_type,
        hybrid,
        density_per_die,
        package_type,
        die_count,
        row_address_bits,
        column_address_bits,
        io_width,
        bank_groups,
        banks_per_bank_group,
        package_ranks_per_channel,
        rank_mix_asymmetric,
        channels_per_dimm,
        primary_bus_width_bits,
    })
}

// --- Per-field decoders ----------------------------------------------------
//
// Each takes the single raw byte (or the relevant shared byte) and applies one
// pinned encoding rule, so each is unit-tested in isolation.

/// Byte 0, bits [6:4]: total addressable size of the SPD device in bytes.
fn decode_spd_bytes_total(byte0: u8) -> Result<u16, DecodeError> {
    match (byte0 >> 4) & 0x07 {
        1 => Ok(256),
        2 => Ok(512),
        3 => Ok(1024),
        4 => Ok(2048),
        value => Err(DecodeError::UnknownEnum {
            field: "SPD device size",
            value,
        }),
    }
}

/// Byte 1: SPD revision as two plain nibbles, major in [7:4], minor in [3:0].
fn decode_spd_revision(byte1: u8) -> SpdRevision {
    SpdRevision {
        major: byte1 >> 4,
        minor: byte1 & 0x0F,
    }
}

/// Byte 2: whole-byte DRAM device type key.
fn decode_device_type(byte2: u8) -> Result<DeviceType, DecodeError> {
    match byte2 {
        0x0B => Ok(DeviceType::Ddr3),
        0x0C => Ok(DeviceType::Ddr4),
        0x12 => Ok(DeviceType::Ddr5),
        0x13 => Ok(DeviceType::Lpddr5),
        value => Err(DecodeError::UnknownEnum {
            field: "DRAM device type",
            value,
        }),
    }
}

/// Byte 3: base module type in bits [3:0]; hybrid flag in bit 7.
///
/// Crate-visible so the module-specific dispatch ([`crate::decode_module_specific`])
/// routes on the same single decode of byte 3, rather than duplicating it.
pub(crate) fn decode_module_type(byte3: u8) -> Result<(ModuleType, bool), DecodeError> {
    let hybrid = byte3 & 0x80 != 0;
    let module_type = match byte3 & 0x0F {
        0x01 => ModuleType::Rdimm,
        0x02 => ModuleType::Udimm,
        0x03 => ModuleType::Sodimm,
        0x04 => ModuleType::Lrdimm,
        0x05 => ModuleType::Cudimm,
        0x06 => ModuleType::Csoudimm,
        0x07 => ModuleType::Mrdimm,
        0x08 => ModuleType::Camm2,
        0x0A => ModuleType::Ddimm,
        0x0B => ModuleType::SolderDown,
        value => {
            return Err(DecodeError::UnknownEnum {
                field: "module type",
                value,
            });
        }
    };
    Ok((module_type, hybrid))
}

/// Byte 4, bits [4:0]: SDRAM density per die.
fn decode_density_per_die(byte4: u8) -> Result<DensityPerDie, DecodeError> {
    match byte4 & 0x1F {
        0x01 => Ok(DensityPerDie::Gb4),
        0x02 => Ok(DensityPerDie::Gb8),
        0x03 => Ok(DensityPerDie::Gb12),
        0x04 => Ok(DensityPerDie::Gb16),
        0x05 => Ok(DensityPerDie::Gb24),
        0x06 => Ok(DensityPerDie::Gb32),
        0x07 => Ok(DensityPerDie::Gb48),
        0x08 => Ok(DensityPerDie::Gb64),
        value => Err(DecodeError::UnknownEnum {
            field: "SDRAM density per die",
            value,
        }),
    }
}

/// Byte 4, bits [7:5]: package construction and die count.
fn decode_package(byte4: u8) -> Result<(PackageType, u8), DecodeError> {
    match (byte4 >> 5) & 0x07 {
        0 => Ok((PackageType::Monolithic, 1)),
        1 => Ok((PackageType::DualDie, 2)),
        2 => Ok((PackageType::ThreeDs, 2)),
        3 => Ok((PackageType::ThreeDs, 4)),
        4 => Ok((PackageType::ThreeDs, 8)),
        5 => Ok((PackageType::ThreeDs, 16)),
        value => Err(DecodeError::UnknownEnum {
            field: "SDRAM package type",
            value,
        }),
    }
}

/// Byte 5, bits [4:0]: number of row address bits, base 16.
fn decode_row_address_bits(byte5: u8) -> u8 {
    16 + (byte5 & 0x1F)
}

/// Byte 5, bits [7:5]: number of column address bits, base 10.
fn decode_column_address_bits(byte5: u8) -> u8 {
    10 + ((byte5 >> 5) & 0x07)
}

/// Byte 6, bits [7:5]: SDRAM I/O device width.
fn decode_io_width(byte6: u8) -> Result<IoWidth, DecodeError> {
    match (byte6 >> 5) & 0x07 {
        0 => Ok(IoWidth::X4),
        1 => Ok(IoWidth::X8),
        2 => Ok(IoWidth::X16),
        3 => Ok(IoWidth::X32),
        value => Err(DecodeError::UnknownEnum {
            field: "SDRAM I/O width",
            value,
        }),
    }
}

/// Byte 7, bits [7:5]: number of bank groups.
fn decode_bank_groups(byte7: u8) -> Result<BankGroups, DecodeError> {
    match (byte7 >> 5) & 0x07 {
        0 => Ok(BankGroups::One),
        1 => Ok(BankGroups::Two),
        2 => Ok(BankGroups::Four),
        3 => Ok(BankGroups::Eight),
        value => Err(DecodeError::UnknownEnum {
            field: "bank groups",
            value,
        }),
    }
}

/// Byte 7, bits [2:0]: number of banks per bank group.
fn decode_banks_per_bank_group(byte7: u8) -> Result<BanksPerBankGroup, DecodeError> {
    match byte7 & 0x07 {
        0 => Ok(BanksPerBankGroup::One),
        1 => Ok(BanksPerBankGroup::Two),
        2 => Ok(BanksPerBankGroup::Four),
        value => Err(DecodeError::UnknownEnum {
            field: "banks per bank group",
            value,
        }),
    }
}

/// Byte 234: package ranks per channel (bits [5:3], base 1) and the rank-mix
/// asymmetry flag (bit 6).
fn decode_package_ranks(byte234: u8) -> (u8, bool) {
    let ranks = ((byte234 >> 3) & 0x07) + 1;
    let asymmetric = byte234 & 0x40 != 0;
    (ranks, asymmetric)
}

/// Byte 235, bits [2:0]: primary bus width per channel in bits.
fn decode_primary_bus_width(byte235: u8) -> Result<u16, DecodeError> {
    match byte235 & 0x07 {
        0 => Ok(8),
        1 => Ok(16),
        2 => Ok(32),
        3 => Ok(64),
        value => Err(DecodeError::UnknownEnum {
            field: "primary bus width",
            value,
        }),
    }
}

/// Byte 235, bits [7:5]: number of channels (sub-channels) per DIMM.
fn decode_channels_per_dimm(byte235: u8) -> Result<u8, DecodeError> {
    match (byte235 >> 5) & 0x07 {
        0 => Ok(1),
        1 => Ok(2),
        2 => Ok(4),
        3 => Ok(8),
        value => Err(DecodeError::UnknownEnum {
            field: "channels per DIMM",
            value,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spd_bytes_total_defined_and_reserved() {
        assert_eq!(decode_spd_bytes_total(0x30).unwrap(), 1024);
        assert_eq!(decode_spd_bytes_total(0x20).unwrap(), 512);
        assert_eq!(decode_spd_bytes_total(0x10).unwrap(), 256);
        assert!(matches!(
            decode_spd_bytes_total(0x00),
            Err(DecodeError::UnknownEnum { .. })
        ));
    }

    #[test]
    fn spd_revision_nibbles() {
        assert_eq!(
            decode_spd_revision(0x10),
            SpdRevision { major: 1, minor: 0 }
        );
        assert_eq!(
            decode_spd_revision(0x21),
            SpdRevision { major: 2, minor: 1 }
        );
    }

    #[test]
    fn device_type_known_and_unknown() {
        assert_eq!(decode_device_type(0x12).unwrap(), DeviceType::Ddr5);
        assert_eq!(decode_device_type(0x0C).unwrap(), DeviceType::Ddr4);
        assert!(matches!(
            decode_device_type(0xFF),
            Err(DecodeError::UnknownEnum { .. })
        ));
    }

    #[test]
    fn module_type_and_hybrid_flag() {
        assert_eq!(
            decode_module_type(0x02).unwrap(),
            (ModuleType::Udimm, false)
        );
        // Bit 7 set => hybrid; low nibble 0x03 => SODIMM.
        assert_eq!(
            decode_module_type(0x83).unwrap(),
            (ModuleType::Sodimm, true)
        );
        // Low nibble 0x00 is reserved.
        assert!(matches!(
            decode_module_type(0x00),
            Err(DecodeError::UnknownEnum { .. })
        ));
    }

    #[test]
    fn density_per_die_table() {
        assert_eq!(decode_density_per_die(0x04).unwrap(), DensityPerDie::Gb16);
        assert_eq!(decode_density_per_die(0x08).unwrap(), DensityPerDie::Gb64);
        assert_eq!(decode_density_per_die(0x04).unwrap().gigabits(), 16);
        assert!(matches!(
            decode_density_per_die(0x00),
            Err(DecodeError::UnknownEnum { .. })
        ));
    }

    #[test]
    fn package_and_die_count() {
        assert_eq!(decode_package(0x00).unwrap(), (PackageType::Monolithic, 1));
        // bits [7:5] = 0b011 => 4-high 3DS.
        assert_eq!(decode_package(0x60).unwrap(), (PackageType::ThreeDs, 4));
        // bits [7:5] = 0b110 is reserved.
        assert!(matches!(
            decode_package(0xC0),
            Err(DecodeError::UnknownEnum { .. })
        ));
    }

    #[test]
    fn row_and_column_address_bits() {
        assert_eq!(decode_row_address_bits(0x00), 16);
        assert_eq!(decode_row_address_bits(0x02), 18);
        assert_eq!(decode_column_address_bits(0x00), 10);
        // bits [7:5] = 0b001 => 11 columns.
        assert_eq!(decode_column_address_bits(0x20), 11);
    }

    #[test]
    fn io_width_table() {
        assert_eq!(decode_io_width(0x00).unwrap(), IoWidth::X4);
        assert_eq!(decode_io_width(0x20).unwrap(), IoWidth::X8);
        assert_eq!(decode_io_width(0x20).unwrap().bits(), 8);
        // bits [7:5] = 0b111 is reserved.
        assert!(matches!(
            decode_io_width(0xE0),
            Err(DecodeError::UnknownEnum { .. })
        ));
    }

    #[test]
    fn bank_groups_count() {
        assert_eq!(decode_bank_groups(0x00).unwrap(), BankGroups::One);
        assert_eq!(decode_bank_groups(0x60).unwrap(), BankGroups::Eight);
        assert_eq!(decode_bank_groups(0x60).unwrap().count(), 8);
        assert!(matches!(
            decode_bank_groups(0xE0),
            Err(DecodeError::UnknownEnum { .. })
        ));
    }

    #[test]
    fn banks_per_bank_group_count() {
        assert_eq!(
            decode_banks_per_bank_group(0x00).unwrap(),
            BanksPerBankGroup::One
        );
        assert_eq!(
            decode_banks_per_bank_group(0x02).unwrap(),
            BanksPerBankGroup::Four
        );
        assert_eq!(decode_banks_per_bank_group(0x02).unwrap().count(), 4);
        // bits [2:0] = 0b011 is reserved.
        assert!(matches!(
            decode_banks_per_bank_group(0x03),
            Err(DecodeError::UnknownEnum { .. })
        ));
    }

    #[test]
    fn package_ranks_and_mix() {
        assert_eq!(decode_package_ranks(0x00), (1, false));
        // bit 6 set => asymmetric; bits [5:3] = 0b001 => 2 ranks.
        assert_eq!(decode_package_ranks(0x48), (2, true));
    }

    #[test]
    fn primary_bus_width_table() {
        assert_eq!(decode_primary_bus_width(0x00).unwrap(), 8);
        assert_eq!(decode_primary_bus_width(0x02).unwrap(), 32);
        assert_eq!(decode_primary_bus_width(0x03).unwrap(), 64);
        // bits [2:0] = 0b101 is reserved.
        assert!(matches!(
            decode_primary_bus_width(0x05),
            Err(DecodeError::UnknownEnum { .. })
        ));
    }

    #[test]
    fn channels_per_dimm_table() {
        assert_eq!(decode_channels_per_dimm(0x00).unwrap(), 1);
        // bits [7:5] = 0b001 => 2 sub-channels.
        assert_eq!(decode_channels_per_dimm(0x20).unwrap(), 2);
        // bits [7:5] = 0b111 is reserved.
        assert!(matches!(
            decode_channels_per_dimm(0xE0),
            Err(DecodeError::UnknownEnum { .. })
        ));
    }
}
