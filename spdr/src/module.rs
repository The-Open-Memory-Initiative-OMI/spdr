//! Module-specific block and module-type dispatch.
//!
//! DDR5 SPD splits its module description into a common module-parameters region
//! (bytes 192..=239, JESD400-5 Annex A.0) and per-module-type "standard module
//! parameters" that begin at byte 240. The physical-form fields decoded here
//! (module nominal height, maximum thickness, reference raw card, and the
//! edge-connector-to-DRAM address mapping) live at bytes 230..=233, inside the
//! common region and inside the byte range the base configuration CRC covers
//! (bytes 0..=509), so they are already integrity-checked. CRC survival is the
//! floor, not content correctness; this decode is the content.
//!
//! [`decode_module_specific`] reads the module-type byte (byte 3, the same field
//! Phase 1 decodes) through [`SpdImage`] and routes on it. The unbuffered (UDIMM)
//! case decodes the block above into [`UnbufferedModule`]. Every other registered
//! module type resolves to [`ModuleSpecific::NotYetDecoded`], which names the type
//! and parses no fields: SODIMM, RDIMM, and LRDIMM carry per-type register and
//! data-buffer parameters that this crate has no real fixture for, and the
//! standing rule is to never claim a decode that has not been checked against a
//! real module. Those types are deferred to later phases, each gated on a fixture.
//!
//! Every offset and encoding is pinned against open references, not memory; the
//! per-field provenance is recorded in
//! `docs/implementations/2026-06-05-phase-4-module-specific.md`.

use crate::error::DecodeError;
use crate::identity::{ModuleType, decode_module_type};
use crate::reader::SpdImage;
use core::fmt;

// Byte offsets within the SPD image.
const OFF_MODULE_TYPE: usize = 3; // key byte 3, decoded in Phase 1
const OFF_MODULE_HEIGHT: usize = 230;
const OFF_MODULE_MAX_THICKNESS: usize = 231;
const OFF_REFERENCE_RAW_CARD: usize = 232;
const OFF_ADDRESS_MAPPING: usize = 233;

/// A length in whole millimetres, the unit DDR5 SPD uses for the module's
/// physical form fields. The unit is named in the type so no caller has to guess,
/// as [`crate::Picoseconds`] does for timings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Millimeters(pub u8);

impl Millimeters {
    /// The length in millimetres.
    #[must_use]
    pub const fn millimeters(self) -> u8 {
        self.0
    }
}

impl fmt::Display for Millimeters {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} mm", self.0)
    }
}

/// The reference raw card a module is built from (byte 232).
///
/// JEDEC labels reference raw cards with an alphabet that skips the visually
/// ambiguous letters I, O, Q, S, X, and Z, so the lettering runs A, B, ... H, J,
/// K, ... Y, then two-letter cards (AA-style) past index 19. A card code of
/// `0x1f` means no reference raw card (rendered "ZZ"); bit 7 extends the code by
/// 31 to reach the two-letter range; bits [6:5] carry the card revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceRawCard {
    /// No reference raw card used (card code `0x1f`, "ZZ").
    NotUsed,
    /// A lettered reference raw card.
    Card {
        /// Zero-based card index (0 = card "A") with the bit-7 extension applied.
        index: u8,
        /// Card revision, from bits [6:5].
        revision: u8,
    },
}

/// The JEDEC reference-raw-card alphabet, skipping I, O, Q, S, X, Z (20 letters).
const RRC_ALPHABET: &[u8; 20] = b"ABCDEFGHJKLMNPRTUVWY";

impl fmt::Display for ReferenceRawCard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            ReferenceRawCard::NotUsed => f.write_str("ZZ (no reference raw card)"),
            ReferenceRawCard::Card { index, revision } => {
                let n = RRC_ALPHABET.len() as u8; // 20
                if index < n {
                    write!(
                        f,
                        "{} revision {revision}",
                        RRC_ALPHABET[index as usize] as char
                    )
                } else {
                    let hi = index / n;
                    let lo = index % n;
                    write!(
                        f,
                        "{}{} revision {revision}",
                        RRC_ALPHABET[hi as usize] as char, RRC_ALPHABET[lo as usize] as char
                    )
                }
            }
        }
    }
}

/// The decoded unbuffered (UDIMM) module-specific block.
///
/// Every field is a `Copy` scalar or exhaustive enum, so the whole struct is
/// `Copy`. Construct it via [`decode_module_specific`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnbufferedModule {
    /// Module nominal height (byte 230, bits [4:0] + 15 mm). JEDEC encodes this
    /// as the upper bound of a 1 mm range, so a value of 0 means "<= 15 mm".
    pub nominal_height: Millimeters,
    /// Maximum module thickness on the front (byte 231, bits [3:0] + 1 mm), the
    /// upper bound of a 1 mm range.
    pub max_thickness_front: Millimeters,
    /// Maximum module thickness on the back (byte 231, bits [7:4] + 1 mm), the
    /// upper bound of a 1 mm range.
    pub max_thickness_back: Millimeters,
    /// Reference raw card the module is built from (byte 232).
    pub reference_raw_card: ReferenceRawCard,
    /// Rank 1 (odd rank) edge-connector-to-DRAM address mapping is mirrored
    /// (byte 233, bit 0). The only functional bit pinned across the DDR3/DDR4
    /// reference decoders for this field; the rest of the byte is preserved in
    /// [`UnbufferedModule::module_attributes_raw`] rather than guessed.
    pub rank1_address_mirrored: bool,
    /// The raw module-attributes / address-mapping byte (byte 233), preserved
    /// whole so no vendor-set or reserved bit is lost. Only bit 0 is interpreted
    /// (above); the remaining bits are spec-reserved and intentionally not
    /// decoded. A later linter phase can flag a set reserved bit from this value.
    pub module_attributes_raw: u8,
}

/// The module-specific decode, dispatched on the module type (byte 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleSpecific {
    /// Unbuffered (UDIMM): the decoded physical-form block.
    Unbuffered(UnbufferedModule),
    /// A registered module type whose module-specific block is not yet decoded.
    /// Names the type and parses no fields; no register, data-buffer, or physical
    /// values are fabricated. Decoding it is gated on a real fixture in a later
    /// phase. SODIMM, RDIMM, and LRDIMM resolve here today.
    NotYetDecoded(ModuleType),
}

/// Decode the module-specific block from a raw SPD image.
///
/// Reads the module-type byte (byte 3) through [`SpdImage`] and routes on it:
/// the unbuffered case decodes bytes 230..=233 into [`UnbufferedModule`]; any
/// other registered type returns [`ModuleSpecific::NotYetDecoded`]. Returns
/// [`DecodeError::Truncated`] if the image is too short for a byte the chosen
/// path reads, or [`DecodeError::UnknownEnum`] if byte 3 holds a reserved module
/// type. Never panics on malformed input.
pub fn decode_module_specific(bytes: &[u8]) -> Result<ModuleSpecific, DecodeError> {
    let spd = SpdImage::new(bytes);
    let (module_type, _hybrid) = decode_module_type(spd.byte(OFF_MODULE_TYPE)?)?;

    match module_type {
        ModuleType::Udimm => Ok(ModuleSpecific::Unbuffered(decode_unbuffered(&spd)?)),
        other => Ok(ModuleSpecific::NotYetDecoded(other)),
    }
}

/// Decode the unbuffered (UDIMM) block at bytes 230..=233 through the reader.
fn decode_unbuffered(spd: &SpdImage) -> Result<UnbufferedModule, DecodeError> {
    let thickness = spd.byte(OFF_MODULE_MAX_THICKNESS)?;
    let mapping = spd.byte(OFF_ADDRESS_MAPPING)?;

    Ok(UnbufferedModule {
        nominal_height: decode_nominal_height(spd.byte(OFF_MODULE_HEIGHT)?),
        max_thickness_front: decode_thickness_front(thickness),
        max_thickness_back: decode_thickness_back(thickness),
        reference_raw_card: decode_reference_raw_card(spd.byte(OFF_REFERENCE_RAW_CARD)?),
        rank1_address_mirrored: mapping & 0x01 != 0,
        module_attributes_raw: mapping,
    })
}

// --- Per-field decoders ----------------------------------------------------
//
// Each applies one pinned encoding rule to its byte, so each has a focused unit
// test built straight from the rule.

/// Byte 230, bits [4:0]: module nominal height in mm with a 15 mm base.
fn decode_nominal_height(byte230: u8) -> Millimeters {
    Millimeters((byte230 & 0x1F) + 15)
}

/// Byte 231, bits [3:0]: front maximum thickness in mm with a 1 mm base.
fn decode_thickness_front(byte231: u8) -> Millimeters {
    Millimeters((byte231 & 0x0F) + 1)
}

/// Byte 231, bits [7:4]: back maximum thickness in mm with a 1 mm base.
fn decode_thickness_back(byte231: u8) -> Millimeters {
    Millimeters(((byte231 >> 4) & 0x0F) + 1)
}

/// Byte 232: reference raw card code (bits [4:0]), with the bit-7 extension and
/// the bits [6:5] revision. Code `0x1f` is the defined "no card" value.
fn decode_reference_raw_card(byte232: u8) -> ReferenceRawCard {
    let code = byte232 & 0x1F;
    if code == 0x1F {
        return ReferenceRawCard::NotUsed;
    }
    let index = if byte232 & 0x80 != 0 {
        code + 0x1F
    } else {
        code
    };
    let revision = (byte232 >> 5) & 0x03;
    ReferenceRawCard::Card { index, revision }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nominal_height_base_15mm() {
        // Fixture byte 230 = 0x11: bits [4:0] = 17, +15 => 32 mm (a 31.25 mm
        // UDIMM lands in the 31 < h <= 32 range).
        assert_eq!(decode_nominal_height(0x11), Millimeters(32));
        assert_eq!(decode_nominal_height(0x00), Millimeters(15));
        // Upper bits are ignored; only bits [4:0] count.
        assert_eq!(decode_nominal_height(0xF1), Millimeters(32));
        assert_eq!(decode_nominal_height(0x1F), Millimeters(46));
    }

    #[test]
    fn max_thickness_front_and_back_base_1mm() {
        // Fixture byte 231 = 0x01: front nibble 1 => 2 mm, back nibble 0 => 1 mm.
        assert_eq!(decode_thickness_front(0x01), Millimeters(2));
        assert_eq!(decode_thickness_back(0x01), Millimeters(1));
        // Distinct nibbles: 0x21 => front 2 mm, back 3 mm.
        assert_eq!(decode_thickness_front(0x21), Millimeters(2));
        assert_eq!(decode_thickness_back(0x21), Millimeters(3));
        assert_eq!(decode_thickness_front(0x0F), Millimeters(16));
    }

    #[test]
    fn reference_raw_card_code_revision_extension() {
        // Fixture byte 232 = 0x00: card index 0 (A), revision 0, no extension.
        assert_eq!(
            decode_reference_raw_card(0x00),
            ReferenceRawCard::Card {
                index: 0,
                revision: 0
            }
        );
        // bits [6:5] = 01 => revision 1.
        assert_eq!(
            decode_reference_raw_card(0x20),
            ReferenceRawCard::Card {
                index: 0,
                revision: 1
            }
        );
        // bit 7 set => extension: index = code + 31. Here code 1 => index 32.
        assert_eq!(
            decode_reference_raw_card(0x81),
            ReferenceRawCard::Card {
                index: 32,
                revision: 0
            }
        );
        // code 0x1f is the defined "no reference raw card" value (ZZ).
        assert_eq!(decode_reference_raw_card(0x1F), ReferenceRawCard::NotUsed);
    }

    #[test]
    fn reference_raw_card_letters_skip_ambiguous() {
        let a = render(ReferenceRawCard::Card {
            index: 0,
            revision: 0,
        });
        assert_eq!(a.as_bytes(), b"A revision 0");
        // Index 9 is the 10th card; the alphabet skips I, so it renders as K.
        let k = render(ReferenceRawCard::Card {
            index: 9,
            revision: 0,
        });
        assert_eq!(k.as_bytes(), b"K revision 0");
        // Index 20 wraps into the two-letter range: BA.
        let ba = render(ReferenceRawCard::Card {
            index: 20,
            revision: 2,
        });
        assert_eq!(ba.as_bytes(), b"BA revision 2");
        assert_eq!(
            render(ReferenceRawCard::NotUsed).as_bytes(),
            b"ZZ (no reference raw card)"
        );
    }

    #[test]
    fn address_mapping_bit0_is_rank1_mirror() {
        // Fixture byte 233 = 0x81: bit 0 set => rank 1 mapping mirrored; the
        // whole byte (including the set reserved bit 7) is preserved.
        let mirrored = unbuffered_image(0x11, 0x01, 0x00, 0x81);
        let m = decode_unbuffered(&SpdImage::new(&mirrored)).unwrap();
        assert!(m.rank1_address_mirrored);
        assert_eq!(m.module_attributes_raw, 0x81);

        // bit 0 clear => standard mapping, even with other bits set.
        let standard = unbuffered_image(0x11, 0x01, 0x00, 0x80);
        let m = decode_unbuffered(&SpdImage::new(&standard)).unwrap();
        assert!(!m.rank1_address_mirrored);
        assert_eq!(m.module_attributes_raw, 0x80);
    }

    #[test]
    fn millimeters_display() {
        assert_eq!(render(Millimeters(32)).as_bytes(), b"32 mm");
    }

    // --- test helpers ------------------------------------------------------

    /// Build a minimal image carrying just the unbuffered block bytes 230..=233.
    fn unbuffered_image(height: u8, thickness: u8, raw_card: u8, mapping: u8) -> [u8; 234] {
        let mut img = [0u8; 234];
        img[OFF_MODULE_HEIGHT] = height;
        img[OFF_MODULE_MAX_THICKNESS] = thickness;
        img[OFF_REFERENCE_RAW_CARD] = raw_card;
        img[OFF_ADDRESS_MAPPING] = mapping;
        img
    }

    /// A small no_std-friendly sink so [`fmt::Display`] output can be asserted
    /// without `alloc`.
    struct Sink {
        data: [u8; 32],
        len: usize,
    }
    impl Sink {
        fn as_bytes(&self) -> &[u8] {
            &self.data[..self.len]
        }
    }
    impl fmt::Write for Sink {
        fn write_str(&mut self, s: &str) -> fmt::Result {
            for &b in s.as_bytes() {
                self.data[self.len] = b;
                self.len += 1;
            }
            Ok(())
        }
    }
    fn render(value: impl fmt::Display) -> Sink {
        use core::fmt::Write;
        let mut sink = Sink {
            data: [0; 32],
            len: 0,
        };
        write!(sink, "{value}").unwrap();
        sink
    }
}
