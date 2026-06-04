# Phase 1 · Decode foundation and identity block

Date: 2026-06-04

## Problem / Motivation

`spdr` is a from-scratch DDR5 SPD decoder. Because JESD400-5 is paywalled, the
decoder doubles as an open reference, so every offset and encoding must be pinned
against an open source and reimplemented cleanly, never copied and never guessed.
Phase 0 left an empty `no_std` library. Phase 1 establishes the parts every later
field decoder depends on:

- a zero-copy reader that never panics on malformed input,
- a `no_std` decode error type,
- the typed-representation pattern (each field is a `Copy` scalar or an
  exhaustive enum), and
- the first real decode surface: the identity and base SDRAM configuration block.

Without this foundation there is nothing for the linter or the CLI to build on,
and no test corpus to anchor correctness claims.

## What Changed

| File | Change |
| --- | --- |
| `spdr/src/error.rs` | New `DecodeError` enum (`Truncated`, `UnknownEnum`); `Display` + `core::error::Error`, `no_std`. |
| `spdr/src/reader.rs` | New `SpdImage<'a>` zero-copy reader; `.get`-based `byte()` accessor returns `Result`, never indexes. |
| `spdr/src/identity.rs` | Identity-and-base decoder: typed representations, per-field private decoders, the `decode_identity_and_base` entry point, and per-field unit tests. |
| `spdr/src/lib.rs` | Wires the three modules and re-exports the public surface. |
| `spdr/Cargo.toml` | Adds `insta` as a dev-dependency for the snapshot test. |
| `spdr/tests/fixture.rs` | Golden-fixture tests: 1024-byte size guard, `insta` snapshot of the decode, and a truncation negative test. |
| `spdr/tests/snapshots/fixture__decodes_identity_and_base_snapshot.snap` | The accepted snapshot of the fixture decode. |
| `docs/validated-against.md` | Records the fixture (manufacturer, part, source, provenance). |
| `docs/numerical-claims.md` | Logs the numbers introduced by this phase with their source. |

The fixture `spdr/tests/fixtures/teamgroup-ud5-6000_0104eef6.spd` was supplied
before this work and is treated as opaque input; it was not created here.

## Implementation Approach

### The reader (`SpdImage`)

`SpdImage<'a>` holds only a borrowed `&'a [u8]`. The single primitive accessor is
`byte(offset) -> Result<u8, DecodeError>`, implemented as
`self.bytes.get(offset).copied().ok_or(Truncated { offset, len })`. Using
`slice::get` rather than indexing is the invariant that makes the whole crate
panic-free on short or malformed input: an out-of-range read becomes a typed
error, not an abort. Every field decoder reads exclusively through this method,
so the panic-free property holds by construction.

### The error type (`DecodeError`)

Two failure modes are modelled, matching the only two ways a content decode can
fail: the input ending early (`Truncated { offset, len }`) and a spec-defined
enumeration field holding a reserved or undefined value (`UnknownEnum { field,
value }`). The enum is `#[non_exhaustive]` because later phases add failure modes
(for example CRC mismatch); downstream code must not assume the set is closed.
`Display` is implemented for human-readable messages and `core::error::Error`
(stable in core since Rust 1.81, well within the 1.85 MSRV) so the std CLI can
treat it as a normal error.

### Typed-representation pattern

Each decoded field is one of:

- a `Copy` scalar (`u8`/`u16`/`bool`) where the value is a count or a flag
  (`row_address_bits`, `die_count`, `channels_per_dimm`, `primary_bus_width_bits`,
  `package_ranks_per_channel`, `hybrid`, `rank_mix_asymmetric`,
  `spd_bytes_total`), or
- an exhaustive enum where the field is categorical (`DeviceType`, `ModuleType`,
  `DensityPerDie`, `IoWidth`, `BankGroups`, `BanksPerBankGroup`, `PackageType`),
  plus the small `SpdRevision { major, minor }` struct.

Nothing allocates and nothing borrows from the input, so `IdentityAndBase` is
itself `Copy`. Enums carry small accessors (`gigabits()`, `bits()`, `count()`,
`as_str()`) and `Display` where a string form is non-obvious, giving the CLI
enough surface without doing any CLI work in this phase.

### Per-field decoders

Each field is decoded by a small private function that takes the relevant raw
byte and applies exactly one pinned encoding rule. Bytes that pack two fields
(3, 4, 5, 7, 234, 235) are read once and fed to two decoders. This split exists
so every field decoder has a focused unit test built straight from the encoding
rule. The public `decode_identity_and_base(&[u8])` reads each byte through
`SpdImage` and composes the decoders, short-circuiting on the first error via
`?`.

## Mathematical / Statistical Details

There is no statistics here, only bit-field extraction. The encoding rules, in
plain notation (`b` is the raw byte, `field` a masked-and-shifted sub-value):

- SPD device size (byte 0, bits [6:4]): table {1->256, 2->512, 3->1024,
  4->2048} bytes; other codes are undefined and error.
- SPD revision (byte 1): `major = b >> 4`, `minor = b & 0x0F`, plain hex nibbles
  (not BCD).
- Device type (byte 2): whole-byte key; 0x12 = DDR5.
- Module type (byte 3): `b & 0x0F` indexes the JEDEC module-type table; hybrid
  flag is bit 7 (`b & 0x80`).
- Density per die (byte 4, bits [4:0]): JEDEC table {1->4, 2->8, 3->12, 4->16,
  5->24, 6->32, 7->48, 8->64} Gb. This is a table, not a linear `field * 4`; the
  two agree only up to code 4 and diverge above it (see Design Decisions).
- Package / die count (byte 4, bits [7:5]): {0->(monolithic, 1), 1->(DDP, 2),
  2->(3DS, 2), 3->(3DS, 4), 4->(3DS, 8), 5->(3DS, 16)}; codes 6-7 error.
- Row address bits (byte 5, bits [4:0]): `16 + field`.
- Column address bits (byte 5, bits [7:5]): `10 + field`.
- I/O width (byte 6, bits [7:5]): `4 << field` -> {x4, x8, x16, x32}.
- Bank groups (byte 7, bits [7:5]): `1 << field` -> {1, 2, 4, 8}.
- Banks per bank group (byte 7, bits [2:0]): `1 << field` -> {1, 2, 4}.
- Package ranks per channel (byte 234, bits [5:3]): `field + 1`; asymmetry is
  bit 6.
- Primary bus width per channel (byte 235, bits [2:0]): `8 << field` -> {8, 16,
  32, 64} bits.
- Channels per DIMM (byte 235, bits [7:5]): `1 << field` -> {1, 2, 4, 8}.

Reserved sub-field values that the spec leaves undefined are decoded to
`UnknownEnum` rather than computed, so the decoder cannot silently invent a value
the standard does not define.

## Reference provenance (which open source backed each block)

Facts and byte offsets are not copyrightable; these were reimplemented in Rust,
not copied. Each field was pinned against at least one open source and, where the
sources disagreed, the disagreement was resolved against a second and third.

| Field(s) | Primary reference(s) |
| --- | --- |
| Byte 0 size, byte 1 revision, byte 2 device type | Bus Pirate DDR5 SPD docs (cites JESD400-5C); decode-dimms DDR5 patch; pyhwinfo `spd_eeprom.py`. |
| Byte 3 module type (full 16-entry table) | decode-dimms DDR5 patch `@module_types` array (verbatim index->name); edlf `DDR5SPDEditor` confirms 0x02-0x05. |
| Byte 4 density table | pyhwinfo density list and edlf `Density` enum (JEDEC table); memtest86plus corroborates. |
| Byte 4 package / die count | pyhwinfo `MONO/DDP/3DS` branch; memtest86plus 3DS die multiplier `1 << (field-1)`. |
| Byte 5 row/column bits | decode-dimms (`(b&0x1f)+16`, `((b>>5)..)+10`), pyhwinfo, and edlf getters `firstAddressing & 0x1F` / `>> 5` all agree on rows=[4:0], cols=[7:5]. |
| Byte 6 I/O width; byte 7 bank groups/banks | edlf `deviceWidthMap`, `bankGroupsBitsMap`, `banksPerBankGroupBitsMap` with masks/shifts; decode-dimms agrees. |
| Byte 234 ranks / rank mix | decode-dimms `($b>>3)&0x07)+1` and `$b & 0x40`; memtest86plus corroborates. |
| Byte 235 bus width / ECC / channels | memtest86plus and decode-dimms (`8 << (b&7)`, ECC bits [4:3], channels bits [7:5]); JESD400-5D.01 sec 11.11 as reproduced in arXiv 2605.08725. |

Sources: i2c-tools `decode-dimms` DDR5 patches (patchwork.ozlabs.org patches
2161722 / 2161728); `remittor/pyhwinfo` `spd_eeprom.py`;
`memtest86plus/system/spd.c` `parse_spd_ddr5`; `edlf/DDR5SPDEditor`
(`ddr5spd_structs.h`, `ddr5spd.cpp`); Bus Pirate DDR5 SPD documentation; a public
reproduction of JESD400-5D.01 sec 11.11.

## Design Decisions

- **Per-field private decoders over one monolithic function.** Splitting lets
  each field have a minimal-input unit test, which is where the crafted-bytes
  test budget is allowed to be spent. The alternative (one big decode with the
  snapshot as the only check) would make a wrong bit-field invisible until review.
- **Exhaustive enums with `UnknownEnum` on reserved values, not an `Other(u8)`
  catch-all.** This matches the spec semantics ("undefined value in a defined
  field is an error") and gives callers compile-time exhaustiveness. The field
  enums are deliberately not `#[non_exhaustive]`; `DecodeError` is, because the
  decoder grows new error modes but the JEDEC value tables for these specific
  fields are fixed for this revision.
- **JEDEC density table, not decode-dimms' linear `field * 4`.** The decode-dimms
  DDR5 patch computes density as `(b & 0x1f) * 4`, which is correct only for
  codes 1-4 and wrong for 5+ (it yields 20/28/32 Gb where JEDEC defines
  24/48/64 Gb). The fixture is code 4 (16 Gb) so both agree here, but the table
  is the correct general rule and is what pyhwinfo and edlf use.
- **Byte 5 bit positions cross-checked three ways.** One reconciliation pass
  initially reported edlf as using a different layout ([5:3]/[1:0]); reading the
  edlf getter source directly showed it uses `firstAddressing & 0x1F` (rows) and
  `>> 5` (columns), i.e. the same [4:0]/[7:5] layout as decode-dimms and
  pyhwinfo. The apparent conflict was a misread, now resolved: the layout is
  unanimous.
- **Byte 0 decoded from bits [6:4] (device size), not the legacy "bytes used"
  nibble.** In DDR5 the low nibble is the beta level, not a byte count; the
  meaningful size selector is bits [6:4]. The struct field is named
  `spd_bytes_total` to reflect this.

## Verification

From the workspace root, all green with zero warnings:

```
cargo build --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The suite has 16 tests: 13 per-field unit tests (one per field decoder, crafted
bytes to known values, each also exercising a reserved-value error path), the
1024-byte fixture size guard, the truncation negative test (first 8 bytes -> a
`Truncated` error, no panic), and the accepted `insta` snapshot of the full
identity-and-base decode of the real fixture. The snapshot was generated and
accepted (`INSTA_UPDATE=always`) and locks the decode against regression; it does
not by itself prove correctness, which is verified at review against an
independent decoder and the part datasheet.

Decoded identity-and-base values for the fixture (TEAMGROUP UD5-6000):

| Field | Value |
| --- | --- |
| SPD device size | 1024 bytes |
| SPD revision | 1.0 |
| DRAM device type | DDR5 SDRAM |
| Module type | UDIMM (not hybrid) |
| Density per die | 16 Gb |
| Package | monolithic, 1 die |
| Row / column address bits | 16 / 10 |
| I/O width | x8 |
| Bank groups x banks per group | 8 x 4 (32 banks) |
| Package ranks per channel | 1 (symmetric) |
| Channels per DIMM | 2 |
| Primary bus width per channel | 32 bits |

These are internally coherent: two 32-bit sub-channels give a 64-bit non-ECC
module; eight x8 devices of 16 Gb each at one rank give a 16 GB module, matching
the part's rating.

## Related Docs

- `docs/validated-against.md` · the fixture this decode is confirmed against.
- `docs/numerical-claims.md` · the numbers in these docs and their sources.
- `.claude/briefs/phase-1-foundation.md` · the brief this phase implements.
