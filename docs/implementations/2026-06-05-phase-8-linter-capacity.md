# Phase 8 · Linter framework and capacity rule

Date: 2026-06-05

## Problem / Motivation

The CRC (Phase 2) is the floor of SPD validation: it proves the bytes survived
transit, nothing about whether the decoded values cohere. A CRC-valid SPD can
still describe an impossible module. This phase opens the project's second pillar,
the linter, which validates beyond the CRC.

The deliverable is the framework every later rule (timing relationships, speed-bin
conformance, reserved bits, cross-field consistency) plugs into; the single
capacity-consistency rule is the proof it works. The linter holds the decode
path's contract: `no_std`, no `alloc`, never panics. Findings are structured data
reported through a callback, so the core never allocates a collection and the
caller decides whether to count, print, or collect.

## What Changed

| File | Change |
| --- | --- |
| `spdr/src/lint.rs` | New: `Severity`, the `#[non_exhaustive]` `Finding` enum with `severity`/`code`/`Display`, the `lint` callback dispatch, the `check_capacity` rule, and per-rule unit tests. |
| `spdr/src/lib.rs` | Wires `mod lint`; re-exports `Finding`, `Severity`, `lint`; adds a Phase 8 sentence to the crate doc. |
| `spdr/tests/lint.rs` | New: the `fixture_lints_clean` integration test (the permanent clean-lint baseline). |
| `spdr/tests/robustness.rs` | The arbitrary-bytes and single-byte-mutation properties also call `lint(input, &mut |_| {})` and assert no panic. |
| `docs/numerical-claims.md` | Records the capacity formula, its pinned source, and the fixture's 16 GB derivation. |
| `docs/validated-against.md` | Adds a Linter note: the fixture produces zero findings under the Phase 8 rule set. |

The Phase 1 through 7 source, suites, and snapshots are unchanged. The serde-gated
derives on `Finding`/`Severity` do not affect `Debug`, and no decoder changed.

## Implementation Approach

### Framework

- **`Severity`**: an enum `Error`, `Warning`, `Info`. Derives `Debug`, `Clone`,
  `Copy`, `PartialEq`, `Eq`, and a gated `#[cfg_attr(feature = "serde", derive(serde::Serialize))]`.
- **`Finding`**: a `#[non_exhaustive]` enum, one variant per check, each carrying
  the structured values the check produced, never a preformatted message. Derives
  `Debug`, `Clone`, `PartialEq`, `Eq`, and the gated serde `Serialize` (no `Copy`:
  later variants may carry non-`Copy` data). Inherent `severity(&self) -> Severity`
  and `code(&self) -> &'static str` (a stable kebab-case lint code per variant).
  `core::fmt::Display` writes the human message into the formatter with `write!`,
  so it is alloc-free; the caller backs the formatter. One variant exists now,
  `NonIntegerDeviceCount { bus_width_bits, io_width_bits }`, code
  `non-integer-device-count`, severity `Error`.
- **`lint`**: `pub fn lint<F: FnMut(Finding)>(bytes: &[u8], sink: &mut F)`. It
  decodes, once, the sections the current rules need; runs each rule; and skips
  any rule whose section did not decode (a decode failure is the caller's decode
  error, not a lint finding). The callback sink keeps the core alloc-free. For
  this phase it decodes identity and base and runs the capacity rule.

The structured-finding-with-edge-formatting design is the same one the decode path
uses: the core produces typed data with no `alloc`, and a human string is built
only at the boundary, by `Display`, into a caller-provided buffer.

### First rule · capacity consistency

The JEDEC module-capacity formula derives capacity from geometry. The data device
count per rank is the primary bus width per channel divided by the SDRAM I/O
device width, and the full capacity is that count times the per-die density, dies
per package, package ranks per channel, and channels:

```
capacity = (bus_width_per_channel / io_width)
         x density_per_die x die_per_package x ranks_per_channel x channels
```

`check_capacity` takes the decoded `IdentityAndBase` (not raw bytes) and reads the
two values it already exposes: `primary_bus_width_bits` and `io_width.bits()`. It
enforces only the formula's precondition, the one thing a CRC-valid SPD can
violate: the device count per rank must be a whole, positive number, i.e. the bus
width must be a positive integer multiple of the I/O width. If the I/O width is
zero, or the bus width is zero, or the bus width is not a multiple of the I/O
width, it emits `Finding::NonIntegerDeviceCount { bus_width_bits, io_width_bits }`
at `Error`; otherwise it emits nothing.

It deliberately does not compute the full capacity product: that is a separate
derived quantity not needed for the check, and a needless overflow surface. The
divisibility check (a guarded modulo) cannot overflow and cannot divide by zero
(the `io_width == 0` guard short-circuits before the modulo). On the fixture (bus
32 bits, I/O x8) the device count is 4, the precondition holds, and the rule emits
nothing.

### The fixture-lints-clean invariant

The fixture is a real, decode-verified module, so `lint` over it must yield zero
findings. `spdr/tests/lint.rs` asserts exactly that. It is a permanent regression
guard: as rules are added, a rule that flags the valid fixture is a bug in the
rule, and this baseline catches it.

### Robustness (extending Phase 6)

`lint` is a new entry point over arbitrary bytes and the capacity rule does a
modulo, so it must never divide by zero or panic. The arbitrary-bytes and
single-byte-mutation properties in `spdr/tests/robustness.rs` now also call
`lint(input, &mut |_| {})` (discarding findings) and assert no panic. Over
arbitrary bytes `lint` either decodes and checks or fails to decode and skips,
never panics; the properties confirm it.

## Mathematical / Statistical Details

The capacity formula and the precondition the rule enforces:

| Quantity | Expression | Fixture |
| --- | --- | --- |
| Device count per rank (per channel) | `primary bus width per channel / SDRAM I/O width` | `32 / 8 = 4` |
| Module capacity (bits) | `device count x density per die x dies per package x ranks per channel x channels` | `4 x 16 Gb x 1 x 1 x 2 = 128 Gb` |
| Module capacity (bytes) | capacity bits / 8 | `128 Gb / 8 = 16 GB` |

The rule checks only the precondition `bus_width_bits mod io_width_bits == 0` with
`bus_width_bits > 0` and `io_width_bits > 0`. If it fails, the device count
`bus_width / io_width` is not a whole positive number, so the capacity product is
undefined; the integer division in the reference computation would silently
truncate. For the fixture, `32 mod 8 == 0` and `32 / 8 = 4 > 0`, so the
precondition holds and no finding is emitted.

### Pinned source

The capacity computation is pinned against memtest86plus `parse_spd_ddr5`
(`system/spd.c`), which accumulates per-rank size as (paraphrased):

```
cur_rank  = <per-die density>
cur_rank *= 1 << (die_per_package - 1)
cur_rank *= 2                       // channels per DIMM
cur_rank *= 1 << (bus_width_code + 3)   // primary bus width per channel, bits
cur_rank /= 1 << (io_width_code + 2)    // divide by SDRAM I/O width, bits
cur_rank *= 1 << ranks_code         // package ranks per channel
```

The `*= bus_width; /= io_width` pair is the device-count term
`bus_width / io_width`, and the integer division is exactly why the bus width must
be a positive multiple of the I/O width for the capacity to be well-defined. The
geometry fields this consumes (die density, SDRAM width, ranks, channels, bus
width) are the same ones decoded by pyhwinfo `spd_eeprom.py` (`die_size`, `width`,
`ranks`) and by this crate's Phase 1 identity decode. decode-dimms computes the
analogous capacity product for earlier DDR generations from the same geometry. The
fixture's known 16 GB is the verification, the value Phase 1 already logged, now
with the formula pinned and the precondition checked.

Facts and formulas are not copyrightable; the rule was reimplemented in Rust from
the pinned references, no externally licensed source copied.

## Design Decisions

- **Callback sink, not a returned collection.** `lint` reports each finding to a
  `&mut F: FnMut(Finding)` so the core stays `no_std` and alloc-free; the caller
  owns the collection strategy. This matches the decode path's zero-allocation
  contract.
- **Structured findings, formatted at the edge.** `Finding` carries raw values,
  not strings; the human message is its `Display`, written with `write!` into a
  caller-backed formatter. The same design as the decoded types: typed data in the
  core, presentation at the boundary.
- **Stable kebab-case codes, inherent severity.** `code()` is a stable identifier
  a consumer can filter or reference across versions; `severity()` is inherent to
  the variant. Both are simple matches, exhaustive within the crate even though
  the enum is `#[non_exhaustive]` for downstream code.
- **Check the precondition, not the product.** The rule verifies divisibility
  only. Computing the full capacity is unnecessary for this check and adds an
  overflow surface; the modulo cannot overflow and is guarded against
  divide-by-zero.
- **The rule takes the decoded struct, not bytes.** It reads `IdentityAndBase`,
  reusing the Phase 1 decode and its bounds checking, rather than re-reading raw
  offsets. `lint` owns the single decode and the skip-on-decode-failure policy.

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

Phase 8 tests (the Phase 1 through 7 suites still pass): two per-rule unit tests in
`lint.rs` (an inconsistent crafted geometry, bus 8 bits with I/O x16, emits exactly
one `NonIntegerDeviceCount` with the right severity, code, and field values; a
consistent crafted geometry, bus 64 bits with I/O x8, emits nothing); the
`fixture_lints_clean` integration test; and the two robustness properties extended
to exercise `lint` over arbitrary and single-byte-mutated input with no panic.

## Related Docs

- `.claude/briefs/phase-8-linter-capacity.md` · the brief this phase implements.
- `docs/numerical-claims.md` · the capacity formula, its pinned source, and the
  fixture's 16 GB derivation.
- `docs/validated-against.md` · the Linter note: the fixture lints clean under the
  Phase 8 rule set.
- `docs/implementations/2026-06-05-phase-6-robustness.md` · the no-panic contract
  the extended robustness properties uphold for the linter.
- `docs/implementations/2026-06-04-phase-1-foundation.md` · the identity decode the
  capacity rule consumes (`primary_bus_width_bits`, `io_width`).
```
