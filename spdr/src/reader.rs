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
    pub const fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Read the single byte at `offset`. Uses `slice::get`, never indexing.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError::Truncated`] if the image is too short to contain
    /// the byte at `offset`.
    pub fn byte(&self, offset: usize) -> Result<u8, DecodeError> {
        self.bytes
            .get(offset)
            .copied()
            .ok_or(DecodeError::Truncated {
                offset,
                len: self.bytes.len(),
            })
    }

    /// Borrow `len` bytes starting at `offset`. Zero-copy: the returned slice
    /// borrows the input image for the image's lifetime, so a decoded value can
    /// hold it without a copy (the part-number `&str` does). Uses `slice::get`,
    /// never indexing. `len` must be at least 1.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError::Truncated`] if the image is too short to contain
    /// `len` bytes at `offset`.
    pub fn slice(&self, offset: usize, len: usize) -> Result<&'a [u8], DecodeError> {
        self.bytes
            .get(offset..offset + len)
            .ok_or(DecodeError::Truncated {
                offset: offset + len - 1,
                len: self.bytes.len(),
            })
    }
}
