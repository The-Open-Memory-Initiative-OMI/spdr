# Phase 4 · Module-specific block and module-type dispatch

Date: 2026-06-05

## Problem / Motivation

Phases 1 through 3 decoded the parts of an SPD image that are identical for every
module type: identity, base SDRAM geometry, the base CRC, and the base JEDEC
timings. The next layer is module-shaped: an SPD carries a module-specific block
whose meaning depends on the module type byte. For an unbuffered module (UDIMM)
that block describes the physical card · how tall it is, how thick front and back,
which JEDEC reference raw card it is built from, and how the edge connector maps
to the DRAM. For registered and load-reduced modules the same region instead
carries register (RCD) and data-buffer parameters.

This phase decodes the unbuffered block against the real fixture and builds the
dispatch around it. The standing rule is that we never claim a decode we have not
checked against a real module, and we have no SODIMM, RDIMM, or LRDIMM fixture, so
those types resolve to an explicit not-yet-decoded result that names the type and
parses no fields. Decoding them is a later phase, each gated on a real fixture.

## What Changed

| File | Change |
| --- | --- |
| `spdr/src/module.rs` | New: `Millimeters`, `ReferenceRawCard`, `UnbufferedModule`, `ModuleSpecific`; `decode_module_specific`; the per-field decoders; per-field unit tests. |
| `spdr/src/identity.rs` | `decode_module_type` is now `pub(crate)`, so the dispatch routes on the same single decode of byte 3 rather than duplicating it. |
| `spdr/src/lib.rs` | Wires the `module` module and re-exports the new types and `decode_module_specific`. |
| `spdr/tests/module.rs` | New: the module-specific snapshot, the dispatch stub test (SODIMM/RDIMM/LRDIMM defer), a reserved-module-type error test, and a truncation negative test. |
| `spdr/tests/snapshots/module__decodes_module_specific_snapshot.snap` | The accepted unbuffered snapshot. |
| `docs/validated-against.md` | Adds a "Confirmed by Phase 4" section and the explicit deferral of SODIMM/RDIMM/LRDIMM. |
| `docs/numerical-claims.md` | Logs the decoded UDIMM values; removes the stale hard-coded test-count footer line. |

The Phase 1, 2, and 3 snapshots are untouched.

## Implementation Approach

### Module-type dispatch

`decode_module_specific(&[u8]) -> Result<ModuleSpecific, DecodeError>` reads the
module-type byte (byte 3) through `SpdImage` and reuses Phase 1's
`decode_module_type`, so there is exactly one decode of byte 3 in the crate. It
routes on the result:

- `ModuleType::Udimm` decodes the unbuffered block (bytes 230..=233) into
  `UnbufferedModule` and returns `ModuleSpecific::Unbuffered`.
- Every other registered type returns `ModuleSpecific::NotYetDecoded(type)`, which
  names the type and reads no further bytes. No register, data-buffer, or physical
  value is fabricated for it.

A reserved module-type byte surfaces `DecodeError::UnknownEnum` from the shared
`decode_module_type`; a short image surfaces `DecodeError::Truncated`. Neither
panics.

### Unbuffered block

`decode_unbuffered` reads bytes 230, 231, 232, and 233 through `SpdImage` (so a
short image is a typed `Truncated` error, never a panic) and applies one pinned
encoding rule per field. Each rule lives in its own small function with a focused
unit test, matching the Phase 1 and Phase 3 style.

The physical dimensions are carried in a named `Millimeters` scalar so the unit is
explicit in the type, the way `Picoseconds` is for timings. The reference raw card
is an exhaustive `ReferenceRawCard` enum: a `NotUsed` variant for the defined
"no card" code and a `Card { index, revision }` variant otherwise. The address
mapping is the one functional bit (rank 1 mirrored) plus the raw attributes byte
preserved whole.

### CRC coverage note

Bytes 230..=233 sit inside the range the base configuration CRC covers (bytes
0..=509, Phase 2). The CRC therefore already proves these bytes survived transit.
That is the floor, not content correctness: the CRC says nothing about whether a
height code is plausible or a reserved bit is set. This phase decodes the content;
checking it is the linter's job, later.

## Mathematical / Statistical Details

Per-field map (offset is the byte index; bit ranges are inclusive):

| Field | Offset | Bits | Stored -> value |
| --- | --- | --- | --- |
| Module nominal height | 230 | [4:0] | `(byte & 0x1f) + 15` mm |
| Max thickness, front | 231 | [3:0] | `(byte & 0x0f) + 1` mm |
| Max thickness, back | 231 | [7:4] | `((byte >> 4) & 0x0f) + 1` mm |
| Reference raw card code | 232 | [4:0] | `0x1f` -> no card; else card index |
| Reference raw card extension | 232 | 7 | if set, index += 31 |
| Reference raw card revision | 232 | [6:5] | revision 0..3 |
| Rank 1 address mapping | 233 | 0 | 1 -> mirrored, 0 -> standard |

Height and thickness are range encodings: the stored value is the upper bound of a
1 mm-wide range with a base offset. Height has a 15 mm base (a value of 0 means
"height <= 15 mm"); each thickness nibble has a 1 mm base. A value of `n` means
"n+base-1 mm < dimension <= n+base mm". For the fixture, height code 17 gives
32 mm, the top of the 31 mm < h <= 32 mm range that a 31.25 mm UDIMM falls in.

Reference raw card letters use the JEDEC alphabet `ABCDEFGHJKLMNPRTUVWY`, which
skips the visually ambiguous letters I, O, Q, S, X, and Z. Indices 0..19 are a
single letter; index 20 and up are two letters (`A[index/20]` then
`A[index%20]`). Code `0x1f` is the defined "no reference raw card" value, rendered
"ZZ". Bit 7 adds 31 to the index to reach the two-letter range. The revision is
bits [6:5].

The rank 1 (odd-rank) address mapping is a single bit: set means the second rank's
edge-connector-to-DRAM wiring is mirrored relative to the first. The remaining bits
of byte 233 are spec-reserved; they are preserved in `module_attributes_raw` rather
than interpreted (see Design Decisions).

### Decoded UDIMM values (fixture)

| Field | Raw byte | Decoded |
| --- | --- | --- |
| Module type (byte 3) | `0x02` | UDIMM -> unbuffered decode |
| Nominal height (byte 230) | `0x11` | 32 mm (top of the 31 < h <= 32 mm range) |
| Max thickness front (byte 231 [3:0]) | `0x1` | 2 mm |
| Max thickness back (byte 231 [7:4]) | `0x0` | 1 mm |
| Reference raw card (byte 232) | `0x00` | card A, revision 0 |
| Rank 1 address mapping (byte 233 bit 0) | `1` | mirrored |
| Module attributes raw (byte 233) | `0x81` | 0x81 preserved (bit 7 reserved-set) |

## Reference provenance

Offsets and encodings were pinned across several open references and reconciled,
the way earlier phases were. None of these are the paywalled JESD400-5 text;
facts and offsets are not copyrightable and the decoders were reimplemented in
Rust.

| Aspect | Reference |
| --- | --- |
| DDR5 offsets 230..=235 | edlf `DDR5SPDEditor` `ddr5spd_structs.h`: a packed struct whose members `moduleHeight`, `moduleMaxThickness`, `refRawCard`, `dimmAttributes`, `moduleOrganization`, `memoryChannelBusWidth` land at 230, 231, 232, 233, 234, 235 (preceded by `reserved_214_229[16]`). The last two cross-validate this crate's Phase 1 constants (234, 235), which anchors the whole offset chain. |
| DDR5 block structure | UniC `SCA08GU04M1F1C-48B` 288-pin DDR5 UDIMM datasheet (Rev C, 2022-09), Table 7: bytes 192..=239 are "Common Module Parameters · Annex A.0", bytes 240..=255 onward are per-type "Standard module parameters · Annex A.x", and the CRC covers bytes 0..=509. This places 230..=233 in the common region and inside CRC coverage. |
| Height / thickness / address-mapping encodings | decode-dimms (i2c-tools, Sensirion mirror): height `(byte & 31) + 15`, thickness front `(byte & 15) + 1` and back `((byte >> 4) & 15) + 1`, rank-1 mapping `byte & 0x01 ? "Mirrored" : "Standard"`. These physical-form encodings are JEDEC-stable across DDR3/DDR4/DDR5. |
| Reference raw card encoding | decode-dimms `ddr3_reference_card`: alphabet `ABCDEFGHJKLMNPRTUVWY`, code `byte & 0x1f`, `return "ZZ" if code == 0x1f`, `index += 31 if byte & 0x80`, revision `(byte >> 5) & 3`, single- vs two-letter split at the alphabet length. |
| SPD structure / per-family module-specific section | JEDEC Standard 21-C Annex K (DDR3 SPD, page 4.1.2.11, Release 24): the SPD address map and the "Module Type Specific Section, indexed by Key Byte 3" structure, confirming the dispatch shape carried forward to DDR5. |
| Physical cross-check | The same UniC datasheet's package drawing (Figure 2): front-view height 31.25 mm and side-view max thickness 2.67 mm. A 31.25 mm card lands in the height-code-17 -> 32 mm range, corroborating the +15 mm base. (This is a different module than the fixture, so it checks the encoding, not the fixture's byte values.) |

## Design Decisions

- **UDIMM decoded; SODIMM, RDIMM, LRDIMM deferred, never guessed.** We have one
  real fixture and it is a UDIMM. The substantive module-specific content for
  registered and load-reduced modules is the per-type register and data-buffer
  block at bytes 240+, which we cannot pin without a fixture of that type. Rather
  than emit a misleading partial decode (height and thickness but invented
  register values), those types resolve to `NotYetDecoded(type)`: the type is
  named, no field is parsed, nothing is fabricated. The dispatch test exercises
  this for all three.
- **Byte 233: decode only the pinned bit, preserve the rest raw.** The rank 1
  address-mapping bit (bit 0) is the one functional bit pinned across the DDR3 and
  DDR4 reference decoders, carried to DDR5 by field-name continuity. JESD400-5 is
  paywalled, so rather than guess the meaning of bits [7:1] we decode bit 0 and
  preserve the whole byte in `module_attributes_raw`. The fixture has bit 7 set
  (a reserved bit); preserving the raw byte keeps that fact for the future linter
  instead of silently dropping or inventing meaning for it. Reporting the
  rank-1-mirror bit even on a single-rank module is faithful to the byte; whether
  it is semantically meaningful is a linter question, not a decode question.
- **`ReferenceRawCard` as an enum with numeric index + revision, letters via
  `Display`.** The "no card" code is a real defined value, so it is its own
  variant rather than a magic index. The index is kept numeric in the decode
  (auditable, snapshot-stable) and rendered to the JEDEC skip-letter alphabet only
  in `Display`, which keeps the `no_std`/no-`alloc` core simple and pins the letter
  mapping to decode-dimms exactly.
- **`Millimeters` named scalar.** Physical dimensions are dimensions, so they get a
  named unit type, matching the `Picoseconds` precedent. No caller has to remember
  that the number is millimetres.
- **Reuse `decode_module_type` rather than re-decode byte 3.** Making it
  `pub(crate)` keeps a single source of truth for the module-type encoding and the
  reserved-value error path, instead of a second copy that could drift.
- **No reserved-value error path inside the UDIMM fields.** Height and thickness
  are pure arithmetic with no reserved encodings; the reference-raw-card `0x1f` is
  a defined "no card" value, not an error; the address mapping is a single bit. The
  only reserved-to-error path in this phase is the module-type byte itself, which
  is tested via `reserved_module_type_errors`.

## Verification

From the workspace root, all green with zero warnings:

```
cargo build --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Phase 4 tests (Phase 1 through 3 suites still pass): six per-field/decoder unit
tests in `module.rs` (`nominal_height`, `max_thickness_front_and_back`,
`reference_raw_card_code_revision_extension`,
`reference_raw_card_letters_skip_ambiguous`, `address_mapping_bit0_is_rank1_mirror`,
`millimeters_display`); and four integration tests in `tests/module.rs` (the
unbuffered snapshot over the real fixture, the dispatch stub test that
SODIMM/RDIMM/LRDIMM defer to `NotYetDecoded`, the reserved-module-type error test,
and a truncation negative test).

Correctness of the UDIMM fields is verified at review against an independent
decoder (DDR5SPDEditor reports the module's physical attributes) and the part's
mechanical detail; a mismatch is a decode bug to fix and re-snapshot.

## Related Docs

- `docs/validated-against.md` · the Phase 4 unbuffered confirmation and the
  deferral of the other module types.
- `docs/numerical-claims.md` · the decoded UDIMM values and their source.
- `docs/implementations/2026-06-04-phase-1-foundation.md` · the module-type byte
  (byte 3) decode this phase dispatches on, and the 234/235 constants that anchor
  the offset chain.
- `.claude/briefs/phase-4-module-specific.md` · the brief this phase implements.
