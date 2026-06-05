# Phase 5 · Manufacturing information block

Date: 2026-06-05

## Problem / Motivation

The manufacturing block names the module: who made it, where and when, its serial
and part number, its revision, and which DRAM (with stepping) it carries. It is the
block with the strongest oracle in the whole project. The fixture's module
manufacturer ID, manufacturing date, serial number, and part number are all
published for serial 0104eef6, so the decode is not just locked by a snapshot, it
is checked against known-correct values, the way the CRC had to reproduce
`0x8021`.

One structural difference from every earlier block: the manufacturing region sits
at bytes 512..=554, past the byte-509 end of the base configuration CRC. The Phase
2 integrity floor does not reach here. So there is no "the bytes at least survived
transit" guarantee for this block; the published reference values are the
verification instead.

This phase also closes the two reference markers carried since Phase 1: the module
manufacturer ID `0x04ef` and the manufacturing date week 37 of 2023, both now
decoded and matching the published reference.

## What Changed

| File | Change |
| --- | --- |
| `spdr/src/manufacturing.rs` | New: `ManufacturerId`, `ManufacturingDate`, `SerialNumber`, `Manufacturing`; `decode_manufacturing`; the per-field decoders; the cited JEP-106 subset table; per-field unit tests. |
| `spdr/src/reader.rs` | Adds `SpdImage::slice`, a zero-copy bounds-checked sub-slice accessor, so the part number can be a borrowed `&str`. |
| `spdr/src/error.rs` | Adds `DecodeError::NonAscii { field, offset }` for a non-ASCII text field. |
| `spdr/src/lib.rs` | Wires the `manufacturing` module and re-exports its types and `decode_manufacturing`. |
| `spdr/tests/manufacturing.rs` | New: the manufacturing snapshot, the published-reference assertions, a truncation test, and a non-ASCII part-number test. |
| `spdr/tests/snapshots/manufacturing__decodes_manufacturing_snapshot.snap` | The accepted manufacturing snapshot. |
| `docs/validated-against.md` | Adds a "Confirmed by Phase 5" section, closes the manufacturer-ID and date markers, and adds the base-decode-complete milestone note. |
| `docs/numerical-claims.md` | Logs the manufacturing values; moves `0x04ef` and week 37 / 2023 from "to be confirmed" to confirmed. |

The Phase 1 through 4 snapshots are untouched.

## Implementation Approach

### Block layout and the reader

`decode_manufacturing(&[u8]) -> Result<Manufacturing, DecodeError>` reads every
byte through `SpdImage`, so a short image is a typed `Truncated` error, never a
panic. Each field is decoded by a small function with a focused unit test, matching
the earlier phases.

The part number is returned as a `&str` borrowed from the input, with no `alloc`.
That needs a borrowed sub-slice, so this phase adds `SpdImage::slice(offset, len)`:
a bounds-checked accessor that returns `&'a [u8]` (a `Truncated` error if short)
using `slice::get`, never indexing. `Manufacturing<'a>` therefore carries the image
lifetime; every other field is a `Copy` scalar or small type.

### JEP-106 manufacturer decode

A JEP-106 manufacturer ID is two bytes: a continuation/bank byte and a code byte,
each carrying odd parity in bit 7. `decode_manufacturer_id` strips the parity bit
(`& 0x7f`) from both: the bank is the 7-bit continuation count plus one (bank 1 =
no continuations), and the code is the 7-bit manufacturer value. The raw `(bank,
code)` is always produced. A name is then resolved from a small cited table keyed
on `(bank, code)`; when the pair is absent, the name is `None` and the raw pair
stands, never a guessed name.

### BCD date, serial, part number

The manufacturing date is two BCD bytes: `bcd(b) = (b >> 4) * 10 + (b & 0x0f)`. The
year byte is an offset from 2000 (`2000 + bcd(year)`), the week byte is the plain
BCD week. The serial number is four bytes assembled most-significant-first into a
`u32` and rendered as eight uppercase hex digits, which is how serials are printed.

The part number is the 30-byte ASCII field at 521..=550. It is validated to be
ASCII (a non-ASCII byte is a typed `NonAscii` error naming the offset, not a panic
or a lossy reinterpretation), then `trim_end_matches([' ', '\0'])` removes the
trailing space and null padding, yielding a borrowed `&str`.

### No integrity floor here

Bytes 512..=554 are past the byte-509 end of the base CRC, so Phase 2 does not
cover them. The doc and the validated-against ledger say so plainly: the published
reference values, not a checksum, are the verification for this block.

## Mathematical / Statistical Details

Per-field map:

| Field | Offset | Width | Stored -> value |
| --- | --- | --- | --- |
| Module manufacturer ID | 512 | 2 | JEP-106 `(bank, code)`, parity bit 7 stripped |
| Manufacturing location | 514 | 1 | raw manufacturer-specific code |
| Manufacturing date | 515 | 2 | `year = 2000 + bcd(515)`, `week = bcd(516)` |
| Serial number | 517 | 4 | big-endian `u32` |
| Part number | 521 | 30 | ASCII, trailing space/null trimmed, borrowed `&str` |
| Revision code | 551 | 1 | raw |
| DRAM manufacturer ID | 552 | 2 | JEP-106, same decode as the module |
| DRAM stepping | 554 | 1 | raw (`0xff` = not specified) |

JEP-106 parity and bank: each ID byte uses bit 7 as odd parity over the byte. The
first byte's low 7 bits count `0x7f` continuation codes; the JEP-106 bank shown is
that count plus 1. So `0x04` -> 4 continuations -> bank 5, and `0x80` -> 0
continuations (the `0x80` is the parity bit on a zero count) -> bank 1. The code
byte's low 7 bits are the manufacturer code: `0xef` -> `0x6f`, `0xad` -> `0x2d`.

BCD: each nibble is one decimal digit, so `0x23` is decimal 23 and `0x37` is 37.

### Decoded manufacturing values (fixture)

| Field | Raw | Decoded |
| --- | --- | --- |
| Module manufacturer | bytes 512..=513 = `0x04 0xef` | JEP-106 bank 5, code `0x6f` -> "Team Group Inc." (TEAMGROUP) |
| Manufacturing location | byte 514 = `0x00` | 0 |
| Manufacturing date | bytes 515..=516 = `0x23 0x37` | week 37 of 2023 |
| Serial number | bytes 517..=520 = `01 04 ee f6` | `0104EEF6` |
| Part number | bytes 521..=550 = "UD5-6000" + padding | "UD5-6000" |
| Revision code | byte 551 = `0x00` | 0 |
| DRAM manufacturer | bytes 552..=553 = `0x80 0xad` | JEP-106 bank 1, code `0x2d` -> "SK Hynix" |
| DRAM stepping | byte 554 = `0xff` | 255 (not specified) |

The four bold oracle fields (module manufacturer ID `0x04ef` -> TEAMGROUP, date
week 37 of 2023, serial `0104EEF6`, part number "UD5-6000") match the published
reference for serial 0104eef6 exactly.

## Reference provenance

| Aspect | Reference |
| --- | --- |
| Offsets 512..=554 and the 30-byte part number | edlf `DDR5SPDEditor` `ddr5spd_structs.h`: `moduleManufacturer` (512), `manufactureLocation` (514), `manufactureDate[2]` (515), `serialNumber[4]` (517), `modulePartnumber[partNumberSize]` (521) with `constexpr size_t partNumberSize = 30`, `moduleRevision` (551), `dramManufacturer` (552), `dramStepping` (554). |
| Decode logic (manufacturer lookup, BCD date, serial, part-number trim) | pyhwinfo `spd_eeprom.py`: `jep106decode(get_bits(data, 512, 0, 15))`, `manuf_year = 2000 + bcd_to_ui8(...515)`, `manuf_week = bcd_to_ui8(...516)`, serial from bytes 517..520, `part_number = data[521:551]...strip()`, DRAM ID at 552, stepping at 554. |
| JEP-106 parity and continuation convention | decode-dimms (i2c-tools) `manufacturer`: counts `0x7f` continuation codes for the bank, checks odd parity on bit 7 (`parity($first) != 1` is "Invalid"). |
| JEP-106 names | freeipmi `libfreeipmi/spec/ipmi-jedec-manufacturer-identification-code-spec.c`, which reproduces the public JEP-106 assignments: bank 5 `0xef` = "Team Group Inc."; bank 1 `0xad` = "SK Hynix"; bank 1 `0x2c` = "Micron Technology"; bank 1 `0xce` = "Samsung"; bank 1 `0xda` = "Winbond Electronic". |
| Published reference values (the oracle) | `ubihazard/ddr5-spd-recovery` dump metadata for serial 0104eef6: TEAMGROUP, part UD5-6000, week 37 / 2023, serial 0104eef6. |

Facts and offsets are not copyrightable; the decoders and the JEP-106 subset were
reimplemented in Rust, no externally licensed source copied.

## Design Decisions

- **Borrowed `&str` part number, no `alloc`.** The core crate is `no_std` with no
  `alloc`, so the part number borrows from the input via the new
  `SpdImage::slice`. `trim_end_matches([' ', '\0'])` trims the padding in place and
  returns a sub-`&str`; nothing is copied.
- **Non-ASCII is a typed error, not a guess.** A part-number byte outside ASCII
  yields `DecodeError::NonAscii { field, offset }` naming the first offending byte,
  rather than panicking or reinterpreting the bytes as Latin-1 (which is what a
  lossy reader would do). This keeps the "malformed input never panics, never
  guesses" contract.
- **JEP-106 table is a cited subset with a raw fallback.** Only a small
  memory-industry subset is embedded, sourced from freeipmi's JEDEC table. Any
  code absent from it resolves to `None`, so the raw `(bank, code)` is reported,
  never a guessed name. Of the embedded entries, only the fixture's own (Team
  Group Inc., SK Hynix) are a verified correctness claim; the rest are cited
  reference data. Embedding the full multi-bank JEP-106 list was out of scope and
  would risk wholesale copying.
- **JEP-106 official name, not the brand.** The resolved name is "Team Group Inc."
  exactly as the cited reference lists it; the report and ledger note that this is
  the TEAMGROUP brand. Using the cited source's own string keeps the table
  honest.
- **Parity bit stripped, not enforced as an error.** Bit 7 odd parity is handled by
  stripping it to recover the 7-bit code and count. A parity mismatch is not raised
  as a decode error (it is a cross-field consistency check for the future linter,
  not one of this phase's typed errors, which are truncation and non-ASCII).
- **Serial as a hex `u32`, custom `Debug`.** The serial is rendered as eight hex
  digits (`0104EEF6`) in both `Debug` and `Display`, so the snapshot is auditable
  against the published serial rather than showing an opaque decimal.

## Verification

From the workspace root, all green with zero warnings:

```
cargo build --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Phase 5 tests (Phase 1 through 4 suites still pass): seven per-field unit tests in
`manufacturing.rs` (JEP-106 parity/bank/code extraction and resolution, the
absent-code raw fallback, the BCD unpack, the date decode, the serial assembly, the
part-number trim, and the non-ASCII offset error); and four integration tests in
`tests/manufacturing.rs` (the manufacturing snapshot over the real fixture, the
four published-reference assertions, a truncation test, and a non-ASCII
part-number test that mutates a real dump).

The four published-reference fields are self-verifying. Correctness of the rest
(location, revision, DRAM manufacturer and stepping) is confirmed at review against
DDR5SPDEditor's readout.

## Related Docs

- `docs/validated-against.md` · the Phase 5 confirmation, the closed markers, and
  the base-decode-complete milestone note.
- `docs/numerical-claims.md` · the decoded manufacturing values and their source.
- `docs/implementations/2026-06-04-phase-1-foundation.md` · where the `0x04ef` and
  week 37 / 2023 markers were first recorded as to-be-confirmed.
- `.claude/briefs/phase-5-manufacturing.md` · the brief this phase implements.
