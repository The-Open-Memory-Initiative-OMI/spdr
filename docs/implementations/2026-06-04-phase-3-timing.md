# Phase 3 · Timing block

Date: 2026-06-04

## Problem / Motivation

The base JEDEC timing block is the largest decode so far and the one most prone
to silent error: the offsets are not contiguous, two different time units are in
play, and some parameters are stored as both an absolute time and a clock count.
The DDR5-6000 on the box is an XMP/EXPO profile (Phase 9); the base block encodes
the slower JEDEC fallback the module guarantees. This phase pins that encoding
against open references and decodes the base timings into typed values, so the
later linter has real numbers to check (tRC against tRAS+tRP, timings against the
speed bin, and so on).

## What Changed

| File | Change |
| --- | --- |
| `spdr/src/timing.rs` | New: `Picoseconds`, `ClockCycles`, `TimingPair`, `CasLatencies`, `Timings`; the encoding helpers; `decode_timings`; helper unit tests. |
| `spdr/src/lib.rs` | Wires the `timing` module and re-exports the timing types and `decode_timings`. |
| `spdr/tests/timing.rs` | New: the timing snapshot, the implied-base-speed assertion, and a truncation negative test. |
| `spdr/tests/snapshots/timing__decodes_base_timings_snapshot.snap` | The accepted timing snapshot. |
| `docs/validated-against.md` | Adds a "Confirmed by Phase 3" section. |
| `docs/numerical-claims.md` | Logs the decoded base timings and the implied base speed. |

The Phase 1 identity snapshot is untouched.

## Implementation Approach

### Timing encoding helper

DDR5 abandons DDR4's medium/fine time-base scheme. Each absolute-time parameter
is a little-endian 16-bit integer already expressed in its unit with 1-unit
granularity: the stored number *is* the time. Two units appear: picoseconds for
almost everything, nanoseconds for the tRFC family (refresh times exceed the
16-bit picosecond range). The helpers are therefore thin and exact: `ps_units`
widens a raw picosecond value into `Picoseconds`; `ns_units` scales a raw
nanosecond value up by 1000 into the same canonical `Picoseconds`. `read_le_u16`
reads two bytes low-first through `SpdImage`. These three are the crux and are
unit-tested directly.

The canonical fine unit is named in the type (`Picoseconds`), so no caller has to
remember which field was stored in which unit; the tRFC values are normalised to
picoseconds on decode. The clock-count parameters stay in a distinct
`ClockCycles` rather than being forced into a time.

### Base timing block

`decode_timings(&[u8]) -> Result<Timings, DecodeError>` reads every byte through
`SpdImage`, so a short image is a typed `Truncated` error, not a panic. The base
timing parameters are raw integer encodings with no reserved values, so
truncation is the only error this can return (the Phase 1 `UnknownEnum` path has
no field to fire on here).

The supported CAS latencies are a five-byte (40-bit) mask in `CasLatencies`,
iterated without allocation; bit `i` set means CL `20 + 2*i` is supported. The
bank-group-class parameters (tRRD_L, tCCD_L and its write variants, tFAW, tWTR_L,
tWTR_S, tRTP) are each a `[ps u16][nCK u8]` triple decoded into a `TimingPair`
that keeps both the time floor and the clock-count floor.

## Mathematical / Statistical Details

Per-parameter map (offset is the low byte; LE = little-endian):

| Parameter | Offset | Width | Unit (stored) | Stored -> value |
| --- | --- | --- | --- | --- |
| tCKAVGmin | 20 | 2 LE | ps | raw |
| tCKAVGmax | 22 | 2 LE | ps | raw |
| CAS latencies | 24 | 5 | bitmask | bit i -> CL 20+2i |
| tAA | 30 | 2 LE | ps | raw |
| tRCD | 32 | 2 LE | ps | raw |
| tRP | 34 | 2 LE | ps | raw |
| tRAS | 36 | 2 LE | ps | raw |
| tRC | 38 | 2 LE | ps | raw |
| tWR | 40 | 2 LE | ps | raw |
| tRFC1 | 42 | 2 LE | ns | raw * 1000 -> ps |
| tRFC2 | 44 | 2 LE | ns | raw * 1000 -> ps |
| tRFCsb | 46 | 2 LE | ns | raw * 1000 -> ps |
| tRRD_L | 70 | 2 LE + 1 | ps + nCK | raw ps, raw nCK |
| tCCD_L | 73 | 2 LE + 1 | ps + nCK | raw ps, raw nCK |
| tCCD_L_WR | 76 | 2 LE + 1 | ps + nCK | raw ps, raw nCK |
| tCCD_L_WR2 | 79 | 2 LE + 1 | ps + nCK | raw ps, raw nCK |
| tFAW | 82 | 2 LE + 1 | ps + nCK | raw ps, raw nCK |
| tWTR_L | 85 | 2 LE + 1 | ps + nCK | raw ps, raw nCK |
| tWTR_S | 88 | 2 LE + 1 | ps + nCK | raw ps, raw nCK |
| tRTP | 91 | 2 LE + 1 | ps + nCK | raw ps, raw nCK |

Absolute-time vs clock count: tCKAVGmin/max, tAA, tRCD, tRP, tRAS, tRC, tWR, and
tRFC1/2/sb are absolute time only (the first eight stored in ps, the tRFC trio in
ns). The 70..93 parameters carry both a ps time floor and an nCK clock-count
floor; the effective constraint a controller applies is the larger of the two.

Base data rate: tCKAVGmin is the clock period, so data rate in MT/s is
`2_000_000 / tCKAVGmin_ps`, rounded to the nearest 100. For 416 ps this is 4807,
rounding to 4800, i.e. DDR5-4800. The implied base CL is
`tAA / tCKAVGmin = 16640 / 416 = 40`, i.e. CL40.

Consistency check that holds on the fixture: tRC (48640 ps) equals tRAS (32000) +
tRP (16640). This is a linter rule in a later phase; it already holds here.

## Reference provenance

Three independent references were extracted and reconciled, the way the Phase 1
density table was. They agreed on the units, byte order, and time base, and on
the offsets through tFAW; the only disagreement was the labels of the last three
`[ps][nCK]` triples.

| Aspect | Reference |
| --- | --- |
| Time base (raw value already in ps/ns, 1-unit granularity, no MTB/FTB) | decode-dimms DDR5 patch (`ddr5_ns` helper, `$ctime = ddr5_ns(bytes, 20)`); memtest86plus `parse_spd_ddr5` (reads tCK/tAA/... as LE u16 in the same ps unit); pyhwinfo `spd_eeprom.py`. |
| Offsets (struct member order) | edlf `DDR5SPDEditor` `ddr5spd_structs.h`, a `#pragma pack(1)` struct whose member order maps 1:1 to byte offsets from byte 20. |
| tRFC stored in ns, not ps | edlf `utilities.cpp`: `TimeToTicksDDR5_RFC` carries an extra `*1000` versus the ps-based `TimeToTicksDDR5`; the raw values (295/160/130) equal the exact JEDEC 16Gb refresh times. |
| CAS bitmask bit->CL mapping (`bit = (cl-20)/2`) | edlf `utilities.cpp` `IsCLSupportedDDR5`; decode-dimms CAS block. |
| Labels of the last three triples (tWTR_L@85, tWTR_S@88, tRTP@91) | Resolved by edlf struct order, confirmed because it makes every value match the JEDEC minimums exactly: tWTR_L 10 ns / 16 nCK, tWTR_S 2.5 ns / 4 nCK, tRTP 7.5 ns / 12 nCK. The alternative labeling (one lane) made tWTR_L 7.5 ns, which contradicts the JEDEC tWTR_L floor of 10 ns. |

Facts and offsets are not copyrightable; the decoders were reimplemented in Rust.

## Design Decisions

- **One canonical fine unit (`Picoseconds`), tRFC normalised on decode.** The
  brief asks for a single named unit so callers never guess; the tRFC family,
  though stored in ns, is scaled to ps so every absolute-time field is uniform.
  The ns origin is recorded here and in the helper.
- **`TimingPair` keeps ps and nCK distinct.** The 70..93 parameters genuinely
  carry two floors; collapsing them to one would discard real data and the
  effective-constraint semantics. They are kept as a pair, not forced into a
  single representation.
- **`CasLatencies` as a 40-bit mask newtype with an iterator and a custom
  `Debug`.** No allocation; the custom `Debug` renders the CL set as a list in
  the snapshot rather than an opaque integer, which keeps the snapshot readable
  and the value auditable.
- **Truncation is the only error.** Base timings have no reserved encodings, so
  there is no `UnknownEnum` path to add here; a short image is `Truncated`.
- **Scope held to the base block.** The 3DS dual-load-region tRFC variants, the
  RFM/ARFM block, and the tCCD_M triples exist in the layout but are zero on this
  monolithic non-3DS part and were not solidly pinned across sources, so they are
  left for a later phase rather than guessed. XMP/EXPO profile timings are Phase 9
  and untouched.

## Verification

From the workspace root, all green with zero warnings:

```
cargo build --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Timing-specific tests (Phase 1 and 2 suites still pass): six helper/per-decoder
unit tests (`ps_units`, `ns_units`, `read_le_u16`, `read_pair`, the CAS bitmask,
the data-rate rounding), the timing snapshot over the real fixture, the
implied-base-speed assertion (DDR5-4800), and a truncation negative test.

### Decoded base timings (fixture)

tCKAVGmin 416 ps (DDR5-4800 base, CL40 implied), tCKAVGmax 1000 ps; supported CL
{22, 24, 26, 28, 30, 32, 34, 36, 38, 40}; tAA/tRCD/tRP 16.640 ns each, tRAS
32.000 ns, tRC 48.640 ns (= tRAS + tRP), tWR 30.000 ns; tRFC1 295 ns, tRFC2
160 ns, tRFCsb 130 ns; tRRD_L 5 ns / 8 nCK, tCCD_L 5 ns / 8 nCK, tCCD_L_WR
20 ns / 32 nCK, tCCD_L_WR2 10 ns / 16 nCK, tFAW 13.333 ns / 32 nCK, tWTR_L
10 ns / 16 nCK, tWTR_S 2.5 ns / 4 nCK, tRTP 7.5 ns / 12 nCK. This is the base
JEDEC fallback; the 6000 38-38-38-78 profiles are confirmed in Phase 9.

## Related Docs

- `docs/validated-against.md` · the Phase 3 base-timing confirmation.
- `docs/numerical-claims.md` · the decoded timing numbers and their source.
- `docs/implementations/2026-06-04-phase-1-foundation.md` · density 16 Gb, which
  the tRFC values (295/160/130 ns) corroborate.
- `.claude/briefs/phase-3-timing.md` · the brief this phase implements.
