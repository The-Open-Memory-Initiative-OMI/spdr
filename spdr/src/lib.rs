#![no_std]
#![forbid(unsafe_code)]

//! `spdr` · the JESD400-5 DDR5 SPD content decoder.
//!
//! The decoder is zero-copy over a byte slice: decoded values borrow directly
//! from the input SPD image and the crate needs no heap, so it stays embeddable
//! in firmware and UEFI contexts.
//!
//! Phase 1 covers the decode foundation ([`SpdImage`], [`DecodeError`]) and the
//! identity-and-base configuration block ([`IdentityAndBase`]). Phase 2 adds the
//! CRC-16 primitive ([`crc16`]) and the base configuration CRC check
//! ([`verify_base_crc`]), a queryable check that never blocks decoding. Phase 3
//! adds the base JEDEC timing block ([`Timings`], [`decode_timings`]). Phase 4
//! adds the module-specific block and the module-type dispatch
//! ([`ModuleSpecific`], [`decode_module_specific`]): the unbuffered (UDIMM) case
//! is decoded; SODIMM, RDIMM, and LRDIMM resolve to an explicit not-yet-decoded
//! result, deferred to later phases gated on real fixtures. Phase 5 adds the
//! manufacturing information block ([`Manufacturing`], [`decode_manufacturing`]),
//! including JEP-106 manufacturer resolution.

mod crc;
mod error;
mod identity;
mod manufacturing;
mod module;
mod reader;
mod timing;

pub use crc::{CrcStatus, crc16, verify_base_crc};
pub use error::DecodeError;
pub use identity::{
    BankGroups, BanksPerBankGroup, DensityPerDie, DeviceType, IdentityAndBase, IoWidth, ModuleType,
    PackageType, SpdRevision, decode_identity_and_base,
};
pub use manufacturing::{
    ManufacturerId, Manufacturing, ManufacturingDate, SerialNumber, decode_manufacturing,
};
pub use module::{
    Millimeters, ModuleSpecific, ReferenceRawCard, UnbufferedModule, decode_module_specific,
};
pub use reader::SpdImage;
pub use timing::{CasLatencies, ClockCycles, Picoseconds, TimingPair, Timings, decode_timings};
