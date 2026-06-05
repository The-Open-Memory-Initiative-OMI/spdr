# Phase 9a · XMP 3.0 and EXPO profile decode

Date: 2026-06-05

## Problem / Motivation

The number on the box (DDR5-6000 38-38-38-78 at 1.25 V) is not in the JEDEC base
block. The base timings the module guarantees decode to DDR5-4800 (Phase 3); the
advertised rated speed lives in the vendor overclocking profiles in the upper SPD
region: Intel XMP 3.0 and AMD EXPO. This phase decodes those two formats.

These are vendor extensions, not JESD400-5, and are less openly documented than
the base. So the discipline of the project (pin every offset against open
references, never from memory; fabricate nothing) is the spine here, and the
decode is anchored by two independent oracles rather than trusted on its own:

- **The section CRC is the region anchor.** Each profile section stores a CRC-16
  over a fixed byte range. Recomputing it over the pinned range and finding it
  equal to the stored value confirms the region, the range, and the algorithm at
  once, exactly as `0x8021` confirmed the base block in Phase 2. A mismatch would
  mean the region is wrong; the rule was to iterate the range until the match,
  and never present a decode of an unconfirmed region as authoritative.
- **The rated timing is the value oracle.** The fixture is rated DDR5-6000
  38-38-38-78 at 1.25 V. Decoding both XMP and EXPO reproduces that rating two
  independent ways.

This closes the last open reference markers carried for the fixture: the rated
profile (open since Phase 1) and the XMP and EXPO section CRCs (open since
Phase 2).

## What Changed

| File | Change |
| --- | --- |
| `spdr/src/vendor.rs` | New: `Millivolts`, `RatedTimings`, `XmpProfile`, `ExpoProfile`, the `Xmp`/`Expo` containers and `VendorProfiles`, `decode_xmp`/`decode_expo`/`decode_vendor_profiles`, the section-CRC helper reusing Phase 2's `crc16`, and per-field unit tests with crafted bytes. |
| `spdr/src/timing.rs` | `data_rate_mt_s` is now `pub(crate)` so the profile decode derives its rated data rate through the same rounding instead of duplicating it. |
| `spdr/src/lib.rs` | Wires `mod vendor`; re-exports the new types and decode entry points; adds a Phase 9a sentence to the crate doc. |
| `spdr/tests/vendor.rs` | New: the section-CRC assertions (computed equals stored, exact hex), the rated-timing oracle for both XMP and EXPO profile 1, the second-profile cross-check, the XMP/EXPO field-by-field agreement, and the absence test. |
| `spdr/tests/robustness.rs` | The arbitrary-bytes, single-byte-mutation, and every-truncation properties also run `decode_xmp`, `decode_expo`, and `decode_vendor_profiles` and assert no panic / `Ok`-or-`Truncated`. |
| `spdr-cli/src/lib.rs` | `DecodeResults` gains a `vendor` section; `decode`, `all_decoded`, `render_human`, and `render_json` render it; the Phase 7 base-timings note is updated to point at the new section. |
| `spdr-cli/tests/snapshots/cli__render_human_snapshot.snap` | Re-accepted: the updated base-timings note plus the new vendor-profiles section. |
| `spdr-cli/tests/snapshots/cli__render_json_snapshot.snap` | Re-accepted: the new `vendor_profiles` key. |
| `README.md` | Status and example updated: XMP/EXPO decoded and CRC-anchored; the rated DDR5-6000 shown; the linter is the remaining later phase. |
| `docs/numerical-claims.md` | Phase 1 rated-speed line marked confirmed; new Phase 9a section with the section CRCs, the rated values, the voltage encoding, and the decoded/preserved/deferred boundary. |
| `docs/validated-against.md` | New Phase 9a section; the last deferred markers (rated speed, XMP/EXPO CRCs) closed. |

The Phase 1 through 5 decoders and their snapshots are unchanged; only the Phase 7
CLI render snapshots update, for the new section. No existing decoder changed
behaviour.

## Implementation Approach

### The two formats and where they live

The end-user region runs from byte 640 to 1023. XMP 3.0 occupies 640-831 (a
64-byte header followed by two 64-byte profile blocks); EXPO occupies 832-959 (a
10-byte header, two 40-byte profile blocks, then reserved padding and the block
CRC). The two profile bodies of XMP fit before EXPO begins, which matches the
two-profile enumeration in the memtest86plus reference.

### Magic detection and presence

Presence is the magic identifier, and the magic is also the presence test:

- XMP 3.0: the two bytes `0x0C 0x4A` at offset 640.
- EXPO: the four ASCII bytes `"EXPO"` at offset 832.

`decode_xmp` / `decode_expo` read the magic through `SpdImage`. If it is absent
they return `Xmp::Absent` / `Expo::Absent`, a no-profile result that parses
nothing further, so arbitrary bytes never produce a fabricated decode. If the
magic is present they decode the header CRC and the profiles. Because the magic
is read through the bounds-checked reader, an image too short to even hold the
magic yields a typed `DecodeError::Truncated`, never a panic.

### Pinned offsets and their sources

Every offset is pinned against open references and cross-checked, never taken
from memory:

- **memtest86plus `system/spd.c`** pins the XMP magic (`0x0C 0x4A` at 640) and the
  per-profile timing offsets, which it reads as little-endian u16: tCK at
  profile+5, tAA at +13, tRCD at +15, tRP at +17, tRAS at +19, tRC at +21. It
  enumerates exactly two XMP profiles.
- **edlf `DDR5SPDEditor` `ddr5spd_structs.h`** pins the field order and the block
  layout: the XMP header (version at +2, profile-enable bits at +3, three 16-byte
  profile names at +14 / +30 / +46, the checksum in the last two bytes), the XMP
  profile order (vpp, vdd, vddq, then minCycleTime at +5), and the entire EXPO
  layout (10-byte header with the `"EXPO"` magic, then 40-byte profiles ordered
  vdd, vddq, vpp, then minCycleTime at +4).
- **edlf `DDR5SPDEditor` `utilities.cpp`** pins the voltage encoding
  (`ConvertByteToVoltageDDR5`) and the CRC-16 parameters (`Crc16`: polynomial
  `0x1021`, initial value 0, no reflection, no final XOR, which is the
  CRC-16/XMODEM this crate already implements as `crc16`).

The two references agree on every overlapping offset, and each CRC range was then
confirmed by computed-equals-stored on the fixture (below), so the offsets are
triangulated, not asserted from one source.

### The section CRC as the region anchor

`section_crc(start, block_len)` computes `crc16` (the Phase 2 primitive) over the
first `block_len - 2` bytes at `start` and compares it to the little-endian u16
stored in the block's last two bytes, returning the Phase 2 `CrcStatus { computed,
stored, matches }`. Reusing `crc16` was the first thing tried, per the brief; the
computed values equalled the stored values on the first attempt, so the algorithm
is confirmed reused unchanged. The confirmed ranges:

| Section | CRC over | Stored at | Value |
| --- | --- | --- | --- |
| XMP header | bytes 640-701 | 702-703 | `0x252C` |
| XMP profile 1 | bytes 704-765 | 766-767 | `0x0A5F` |
| XMP profile 2 | bytes 768-829 | 830-831 | `0x0AC4` |
| EXPO block | bytes 832-957 | 958-959 | `0x9FE2` |

XMP carries a CRC per block (header and each profile); EXPO carries one CRC over
the whole block. The match over each range is what licenses presenting the decode
of that region as authoritative.

### Decoding each profile

Both formats share the rated values, so a private `rated_timings` assembles a
`RatedTimings` from a profile's three voltage bytes and its five timing offsets
(the two formats place these differently, so the caller supplies the absolute
offsets). For each profile it decodes the cycle time (giving the data rate), the
rated CAS latency (tAA in whole cycles of tCK), tAA, tRCD, tRP, tRAS, and the
three voltages. XMP additionally decodes the per-profile name (borrowed zero-copy
from the header) and carries the per-profile CRC.

Which profiles are populated is reported, not assumed:

- XMP: a profile is included only when its enable bit (byte 643, bit `i` for
  profile `i + 1`) is set. The fixture's `0x03` enables profiles 1 and 2, and the
  third name slot is blank, consistent with two profiles.
- EXPO: its per-profile enable-bit layout was not confidently pinned, so a profile
  is included when its cycle time is non-zero (a zeroed slot is unpopulated). The
  block CRC confirms the region the values are read from.

### Decoded vs preserved-raw vs deferred

The boundary is explicit and auditable:

- **Decoded and verified:** the four section CRCs; per profile, the cycle time /
  data rate, rated CAS latency, tAA, tRCD, tRP, tRAS, and VDD / VDDQ / VPP; and
  the XMP profile names.
- **Deferred (inside the CRC-confirmed region, not surfaced):** the remaining
  profile timings (tRC, tWR, the tRFC family, and the bank-group-class
  parameters), the XMP `vMemCtrl` rail and the command-rate / DIMMs-per-channel
  metadata, and the EXPO per-profile enable-bit semantics. These bytes are covered
  by the section CRC, but the rated-timing oracle does not reach them, so they are
  left in the image rather than claimed. Nothing here is fabricated; the line is
  drawn at what the oracle independently supports.

### Absence and robustness

`Xmp::Absent` / `Expo::Absent` name the absence and parse nothing. The
arbitrary-bytes property in `spdr/tests/robustness.rs` exercises the magic search
and the CRC-over-a-fixed-range on random images of length 0-2048: a present-magic
fluke either reads fully (`Ok`) or runs off the end (`Truncated`), never panics or
reads out of bounds. The every-truncation test confirms each vendor decoder
returns `Ok` or `Truncated` for every prefix of the fixture. `fixture_lints_clean`
is unaffected: Phase 9a adds no lint rule.

## Mathematical / Statistical Details

### Section CRC

CRC-16/XMODEM: polynomial `0x1021`, initial value `0x0000`, no bit reflection, no
final XOR, computed MSB-first over the covered bytes (the same `crc16` verified in
Phase 2 against the catalogue check value `0x31C3` for `"123456789"`). For each
section, `computed = crc16(bytes[start .. start + block_len - 2])` and `stored` is
the little-endian u16 in the block's last two bytes. The four sections all satisfy
`computed == stored` (`0x252C`, `0x0A5F`, `0x0AC4`, `0x9FE2`); see the table above
for the ranges.

### Voltage encoding

The profile voltage byte packs the voltage as upper-3-bits whole volts and
lower-5-bits in 50 mV steps (the `ConvertByteToVoltageDDR5` encoding). In
millivolts:

```
mV = (byte >> 5) * 1000 + (byte & 0x1F) * 50
```

| Byte | Whole volts (`>>5`) | 50 mV steps (`&0x1F`) | Voltage |
| --- | --- | --- | --- |
| `0x25` | 1 | 5 | 1 V + 250 mV = 1.250 V |
| `0x24` | 1 | 4 | 1 V + 200 mV = 1.200 V |
| `0x30` | 1 | 16 | 1 V + 800 mV = 1.800 V |

The fixture's profile 1 has VDD = VDDQ = `0x25` = 1.250 V and VPP = `0x30` =
1.800 V, reproducing the rated 1.25 V.

### Data rate and rated CAS latency

The cycle time is a little-endian u16 already in picoseconds (DDR5's 1 ps
granularity, no DDR4 time base). The data rate reuses the base block's rounding:

```
data_rate_mt_s = round_to_nearest_100(2_000_000 / tck_ps)
```

For tck = 333 ps: `2_000_000 / 333 = 6006`, rounded to 6000 (DDR5-6000). The rated
CAS latency is tAA in whole cycles of tCK, rounded to nearest, guarded against a
zero cycle time:

```
CL = (taa_ps + tck_ps / 2) / tck_ps          (tck != 0)
```

For tAA = 12654 ps, tck = 333 ps: `(12654 + 166) / 333 = 38` (CL38). The same
division gives the clock counts shown in the CLI: tRCD / tRP 12654 ps = 38
clocks, tRAS 25974 ps = 78 clocks. Profile 2 (tck 357 ps) gives DDR5-5600, CL40,
and 40 / 40 / 84 clocks, an independent corroboration of the offsets.

## Design Decisions

- **The CRC match is the gate, not a guess.** The offsets were not committed until
  the computed CRC equalled the stored CRC over the proposed range. The ranges in
  the table are the ranges that matched; a region that would not match over any
  justifiable range would have been reported unconfirmed and decoded only as far
  as the rated-timing oracle independently supported. All four matched on the
  first attempt with the reused `crc16`.
- **Reuse `crc16` and `data_rate_mt_s`, do not duplicate.** The CRC is the Phase 2
  primitive and the rounding is the Phase 3 helper (made `pub(crate)`), so the
  rated data rate is computed identically to the base data rate and the section
  CRC identically to the base CRC. One implementation, one place to audit.
- **Shared `RatedTimings`, format-specific offsets.** XMP and EXPO encode the same
  rated values at different offsets. A shared struct with a private assembler keeps
  the value model identical across formats (so the field-by-field cross-check is
  meaningful) while the offset tables stay distinct and pinned per format.
- **Report which profiles are populated, do not invent.** XMP presence is gated on
  the pinned enable bit; EXPO presence on a non-zero cycle time, because its
  enable-bit layout was not confidently pinned. The honest signal is used in each
  case rather than guessing a semantic.
- **Names are best-effort, never fatal.** A profile name is `Option<&str>`: a blank
  or non-printable name slot yields `None` rather than failing the decode or
  fabricating text. The rated values, the verifiable payload, are independent of
  the name.
- **CRC status travels with the values, non-blocking.** Each section's `CrcStatus`
  is carried on the decoded region (the Phase 2 pattern): queryable, never raised,
  so a consumer can see whether the region a value came from is confirmed without
  the decode itself blocking on it.
- **Voltages in millivolts, named in the type.** `Millivolts` parallels
  `Picoseconds` and `Millimeters`: the unit is in the type so no caller guesses,
  and the raw bit-field encoding is normalised on decode.

## Verification

From the workspace root, all green with zero warnings, on Windows:

```
cargo build --workspace
cargo build -p spdr                 # default features: core still no_std, serde-free
cargo build -p spdr --features serde
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Phase 9a tests (the Phase 1 through 8 suites still pass; only the Phase 7 CLI
render snapshots update):

- `spdr/src/vendor.rs` unit tests: the voltage encoding (`0x25` -> 1250 mV,
  `0x30` -> 1800 mV), the `Millivolts` display, the rated-CL rounding and its
  zero-tck guard, the little-endian u16 read, the name trim / non-printable
  rejection / truncation handling, the absence result on a no-magic image, and a
  crafted XMP and a crafted EXPO profile each decoding DDR5-6000 38-38-38-78 at
  1.25 V from bytes built straight from the pinned offsets.
- `spdr/tests/vendor.rs` integration tests over the real fixture: the four section
  CRCs computed-equals-stored at the exact published hex; XMP profile 1 and EXPO
  profile 1 each DDR5-6000 / CL38 / tRCD-tRP-tRAS 38/38/78 (in time and in clock
  counts) / 1.25 V; profile 2 of each DDR5-5600 40-40-84; XMP and EXPO profile 1
  identical field by field; and the absence test.
- `spdr/tests/robustness.rs`: the arbitrary-bytes and single-byte-mutation
  properties and the every-truncation test extended over the vendor decoders, no
  panic, `Ok`-or-`Truncated`.
- `spdr/tests/lint.rs`: `fixture_lints_clean` still green (no rule added).
- `spdr-cli/tests/cli.rs`: the two updated render snapshots, the JSON-valid check,
  the exit-code contract, and the render-robustness proptest now exercising the
  new section.

## Related Docs

- `.claude/briefs/phase-9a-xmp-expo.md` · the brief this phase implements.
- `docs/numerical-claims.md` · the section CRCs, the rated values, the voltage
  encoding, and the decoded/preserved/deferred boundary; the Phase 1 rated-speed
  line now confirmed.
- `docs/validated-against.md` · the Phase 9a section; the last deferred markers
  closed.
- `docs/implementations/2026-06-04-phase-2-crc.md` · the `crc16` primitive and the
  `CrcStatus` pattern this phase reuses as the region anchor.
- `docs/implementations/2026-06-04-phase-3-timing.md` · the picosecond timing model
  and the `data_rate_mt_s` rounding the rated values reuse.
- `docs/implementations/2026-06-05-phase-6-robustness.md` · the no-panic contract
  the extended robustness properties uphold for the vendor decoders.
- `docs/implementations/2026-06-05-phase-7-cli-decode.md` · the render surface the
  vendor section extends, and the snapshot it updates.
