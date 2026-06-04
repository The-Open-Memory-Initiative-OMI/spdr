#![no_std]
#![forbid(unsafe_code)]

//! `spdr` · the JESD400-5 DDR5 SPD content decoder.
//!
//! The decoder is zero-copy over a byte slice: decoded values borrow directly
//! from the input SPD image and the crate needs no heap, so it stays embeddable
//! in firmware and UEFI contexts.
//!
//! Phase 1 covers the decode foundation ([`SpdImage`], [`DecodeError`]) and the
//! identity-and-base configuration block ([`IdentityAndBase`]).

mod error;
mod identity;
mod reader;

pub use error::DecodeError;
pub use identity::{
    BankGroups, BanksPerBankGroup, DensityPerDie, DeviceType, IdentityAndBase, IoWidth, ModuleType,
    PackageType, SpdRevision, decode_identity_and_base,
};
pub use reader::SpdImage;
