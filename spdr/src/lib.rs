#![no_std]
#![forbid(unsafe_code)]

//! `spdr` · the JESD400-5 DDR5 SPD content decoder.
//!
//! The decoder is zero-copy over a byte slice: decoded values borrow directly
//! from the input SPD image and the crate needs no heap, so it stays embeddable
//! in firmware and UEFI contexts. No decoding logic lives here yet; this is the
//! scaffolded core.
