//! Decode errors for the SPD content decoder.

use core::fmt;

/// An error produced while decoding an SPD image.
///
/// The decoder never panics on malformed input. Two failure modes are modelled:
/// the input ending before a required byte, and a spec-defined enumeration field
/// holding a value with no defined meaning. Both are `Copy` and `no_std` clean.
///
/// The enum is `#[non_exhaustive]`: later decode phases add their own failure
/// modes, and downstream code must not assume the set is closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecodeError {
    /// The input ended before a required byte. `offset` is the byte index that
    /// was requested; `len` is the actual length of the input image.
    Truncated { offset: usize, len: usize },

    /// A field that the spec defines as an enumeration held a value with no
    /// defined meaning. `field` names the field; `value` is the raw encoding.
    UnknownEnum { field: &'static str, value: u8 },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            DecodeError::Truncated { offset, len } => write!(
                f,
                "input truncated: byte {offset} requested but image is only {len} bytes"
            ),
            DecodeError::UnknownEnum { field, value } => {
                write!(f, "unknown value {value:#04x} for field `{field}`")
            }
        }
    }
}

impl core::error::Error for DecodeError {}
