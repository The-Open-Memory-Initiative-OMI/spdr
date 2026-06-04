//! Zero-copy reader over a raw SPD image.

use crate::error::DecodeError;

/// A zero-copy view over a raw SPD byte image.
///
/// Holds only a borrowed slice; no copy or allocation happens. Every accessor is
/// bounds-checked through [`slice::get`] and returns [`DecodeError::Truncated`]
/// rather than panicking, so malformed or short input can never cause an
/// out-of-range panic. Every field decoder in this crate reads through here.
#[derive(Debug, Clone, Copy)]
pub struct SpdImage<'a> {
    bytes: &'a [u8],
}

impl<'a> SpdImage<'a> {
    /// Wrap a raw SPD image. No copying or validation happens here; the bytes
    /// are treated as opaque input until a field decoder reads them.
    #[must_use]
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    /// The length of the underlying image in bytes.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Whether the underlying image is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Read the single byte at `offset`, or [`DecodeError::Truncated`] if the
    /// image is too short to contain it. Uses `slice::get`, never indexing.
    pub fn byte(&self, offset: usize) -> Result<u8, DecodeError> {
        self.bytes
            .get(offset)
            .copied()
            .ok_or(DecodeError::Truncated {
                offset,
                len: self.bytes.len(),
            })
    }
}
