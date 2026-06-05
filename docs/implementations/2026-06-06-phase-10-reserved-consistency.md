# Phase 10 · Reserved-bit and consistency lint rules

Date: 2026-06-06

## Problem / Motivation

This phase adds the last two lint rule families to the framework: reserved-bit
checks and cross-field consistency checks. Both are where the paywalled spec bites
hardest and where an over-eager rule would flag a valid module, so the governing
principle is honest conservatism.

The Phase 8 fixture-lints-clean invariant is held at zero here, not evolved. The
fixture is a real, valid module, so anything it exhibits is by definition not a
defect. Two bits make this concrete and are deliberately not flagged:

- **Byte 233 bit 7** (set on the fixture, value 0x81). We cannot confirm from the
  paywalled spec that it is reserved; edlf `ddr5spd_structs.h` labels byte 233 a
  defined `dimmAttributes` field, not a reserved region; and a valid module setting
  it is evidence it is defined-but-undocumented, not a reserved-must-be-zero
  violation. The reserved-bit rule keys only off reference-declared-reserved
  regions, and byte 233 is not one, so the bit never enters the rule.
- **The rank-1 address-mirror bit on a single-rank module** (byte 233 bit 0, set
  on the fixture). A single-rank part has no second rank to mirror, so this is a
  benign don't-care, not an inconsistency. No rule treats it as a defect.

These uncertain bits stay decode-level preserved-raw facts (already visible in the
Phase 4 decode output as `module_attributes_raw = 0x81`), not lint findings.

## What Changed

| File | Change |
| --- | --- |
| `spdr/src/lint.rs` | New `Finding` variants (`ReservedBytesNonZero`, `PackageDieCountMismatch`), the `RESERVED_REGIONS` map, the `check_reserved_regions` and `check_package_coherence` rule functions, the dispatch wiring in `lint`, and per-rule unit tests. Imports `SpdImage` (for the raw-byte reads) and `PackageType`. |
| `docs/numerical-claims.md` | New Phase 10 section: the reserved regions checked and their source, and the coherence relationship verified on the fixture. |
| `docs/validated-against.md` | New Phase 10 note: the fixture lints clean under the full rule set, the reserved regions it has zero, the coherence it satisfies, and the deliberate non-flagging of byte 233 bit 7 and the mirror bit. |

No decoder, no renderer, and no snapshot changed. The Phase 1 through 9b suites and
all snapshots, including the two CLI render snapshots, are untouched: this phase
adds lint rules only. (Lint findings are not part of the CLI decode output, so the
new `Finding` variants do not affect any render snapshot.)

## Implementation Approach

### Dispatch

`lint` now runs the reserved-bit rule first, unconditionally (it reads raw bytes,
independent of any decode), then the decode-driven rules where their inputs exist:

```
check_reserved_regions(bytes, sink);                      // raw bytes, always
if let Ok(identity) = decode_identity_and_base(bytes) {
    check_capacity(&identity, sink);                      // Phase 8
    check_package_coherence(&identity, sink);             // Phase 10
}
if let Ok(timings)  = decode_timings(bytes)         { check_base_timings(...) }   // Phase 9b
if let Ok(profiles) = decode_vendor_profiles(bytes) { check_vendor_profiles(...) } // Phase 9b
```

### Reserved-bit rule (`Warning`, code `reserved-bytes-nonzero`)

The rule checks that every byte in a reference-declared-reserved-and-zero region is
zero, reading the bytes through `SpdImage` so a region the image is too short to
contain is skipped (the slice returns an error), never read out of bounds. It emits
one `Warning` per non-zero region, locating the first offending byte. Severity is
`Warning`, not `Error`: a set reserved bit is suspect but a later spec revision may
define it.

The map (`RESERVED_REGIONS`) is pinned to edlf `ddr5spd_structs.h`'s own named
`reserved_*` members, intersected with the regions the valid fixture confirms are
all-zero:

| Region | Bytes | edlf member | Fixture |
| --- | --- | --- | --- |
| isolated reserved byte | 15 | `reserved_15` | 0x00 |
| isolated reserved byte | 29 | `reserved_29` | 0x00 |
| base-timing tail | 103-127 | `reserved_103_127` | all 0 |
| reserved block 2 | 128-191 | `reserved_128_191` | all 0 |
| common-region reserved span | 214-229 | `reserved_214_229` | all 0 |
| common-region reserved span | 236-239 | `reserved_236_239` | all 0 |
| reserved block before the CRC | 448-509 | `reserved_448_509` | all 0 |

Two reference-declared-reserved regions are deliberately **excluded**, which is the
honest-conservative core of the rule:

- **`reserved_240_447`** (bytes 240-447): the module-type-specific parameter
  region. It is zero in this UDIMM fixture, but a valid RDIMM or LRDIMM populates it
  with register and data-buffer parameters, so it is not unconditionally reserved.
  Checking it would flag a valid module of another type, so it is excluded. The rule
  checks only regions reserved for every module type.
- **`reserved_555_639`** (bytes 555-639): non-zero in the fixture itself (bytes
  576-581 hold vendor data). A region the valid module populates is evidently
  vendor-usable, not must-be-zero, so it is excluded, exactly the brief's "exclude
  what a valid module uses" rule applied to the fixture's own bytes.

The result: the fixture has zero in every checked region and produces no reserved
finding; a crafted image with garbage in a genuinely-reserved-and-normally-zero
region does.

### Consistency rule (`Error`, code `package-die-count-mismatch`)

The SDRAM package type and the die count must cohere: a monolithic package carries
exactly one die; a dual-die (DDP) or 3DS package carries more than one. The rule
reads the `package_type` and `die_count` Phase 1 decodes and flags an incoherent
pair as an `Error` (a definitional incoherence in the geometry). The fixture
(monolithic, one die) satisfies it.

An honest note on this rule: the Phase 1 decode derives both `package_type` and
`die_count` from the same byte-4 bits [7:5] (the JEDEC package-type table), so a
real image is always coherent and the rule never fires on decoded data. It is
therefore a defense-in-depth invariant guard, valuable as documentation of the
invariant and as a regression catch if the decode ever sourced the die count
separately. It is exercised by constructing an incoherent geometry directly (the
unit test decodes a valid monolithic geometry, then sets the die count to four).

### Guards and robustness

The reserved-bit rule's only "arithmetic" is `offset + i` over small in-range
indices, and every region read is bounds-checked by `SpdImage::slice`. The
consistency rule does no arithmetic. The Phase 8 lint-in-robustness properties,
which run `lint` over arbitrary and single-byte-mutated bytes, confirm the new
rules never panic and never read out of bounds.

## Mathematical / Statistical Details

This phase is structural rather than numeric. The two relationships:

- **Reserved region zero-ness.** For each `(offset, length)` in `RESERVED_REGIONS`,
  every byte in `bytes[offset .. offset + length]` must equal zero. The map is the
  intersection of (reference-declared-reserved) and (fixture-confirmed-zero), minus
  the type-dependent and fixture-non-zero exclusions above.
- **Package/die coherence.** `package_type == Monolithic` requires `die_count == 1`;
  `package_type in {DualDie, ThreeDs}` requires `die_count > 1`. Pinned to the JEDEC
  byte-4 package-type encoding Phase 1 decodes and the edlf `sdram1Density` package
  bits.

Both are facts (which bytes a reference declares reserved; the JEDEC package/die
table), not copyrightable expression; the rules were written from the references'
declarations, no licensed source copied.

## Design Decisions

- **Pin the reserved map to the references' own declarations, then intersect with
  the fixture.** The map is not a guess about which bits "look" reserved; it is
  edlf's named `reserved_*` members, kept only where the valid fixture confirms
  zero. This is the methodology the brief mandates, and it is what makes the rule
  safe to ship without the paywalled spec.
- **Exclude type-dependent and fixture-used regions.** `reserved_240_447` is
  excluded because it is per-module-type (a valid RDIMM uses it), and
  `reserved_555_639` because the fixture itself uses it. Both exclusions follow the
  one principle: never flag a region a valid module legitimately populates.
- **Reserved bits are `Warning`, incoherence is `Error`.** A set reserved bit is
  suspicious but may be a future definition, so it is flagged for attention. A
  monolithic-multi-die geometry is a definitional contradiction, so it is an error.
- **Do not flag uncertain bits.** Byte 233 bit 7 and the single-rank mirror bit are
  set on the valid fixture, which is positive evidence they are not defects. They
  are left as decode-level preserved-raw facts, and a unit test locks in that byte
  233 bit 7 produces no reserved finding.
- **Keep the package-coherence rule honest about its reach.** Rather than dress it
  up as catching real-world incoherence (which the current decode makes impossible),
  the doc and the test state plainly that it is an invariant guard exercised by a
  constructed struct.

## Deliberately not flagged (on the record)

| Bit | Fixture | Why not flagged |
| --- | --- | --- |
| Byte 233 bit 7 | set (0x81) | Byte 233 is edlf's defined `dimmAttributes` field, not a reference-declared reserved region; a valid module setting it is evidence it is defined-but-undocumented. Not in the reserved map; locked in by `byte_233_bit_7_is_not_a_reserved_finding`. |
| Byte 233 bit 0 (rank-1 mirror) | set | A single-rank module has no second rank to mirror; the bit is a benign don't-care, not an inconsistency. No rule inspects it. |

Both remain visible in the decode output (`module_attributes_raw = 0x81`), so the
information is preserved; it is simply not elevated to a lint finding.

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

Phase 10 tests (the Phase 1 through 9b suites and all snapshots, including the CLI
render snapshots, are untouched):

- Per-rule unit tests in `lint.rs` with crafted bytes: garbage in `reserved_128_191`
  emits `ReservedBytesNonZero { offset: 128, value: 0xFF }` (Warning, the right code
  and location); an all-zero image and a short image emit nothing; an image with
  only byte 233 bit 7 set emits no reserved finding; and a monolithic-but-four-die
  geometry emits `PackageDieCountMismatch { package_type: Monolithic, die_count: 4 }`
  (Error), with the consistent monolithic-one-die case emitting nothing.
- `fixture_lints_clean` stays green under the full rule set (capacity, timing,
  speed-bin, reserved-bit, consistency): the real module produces zero findings,
  with every applicable rule running.
- The Phase 8 lint-in-robustness properties still pass: `lint` runs the new rules,
  including the raw-region reads, over arbitrary and mutated bytes with no panic and
  no out-of-bounds read.

## Related Docs

- `.claude/briefs/phase-10-reserved-consistency.md` · the brief this phase implements.
- `docs/numerical-claims.md` · the reserved regions and their source, and the
  coherence relationship verified on the fixture.
- `docs/validated-against.md` · the Phase 10 note: the fixture lints clean under the
  full rule set, and the deliberate non-flagging of byte 233 bit 7 and the mirror bit.
- `docs/implementations/2026-06-05-phase-8-linter-capacity.md` · the framework and
  the fixture-lints-clean invariant this phase holds at zero.
- `docs/implementations/2026-06-05-phase-4-module-specific.md` · the Phase 4 decode
  of byte 233 (`module_attributes_raw`), which preserves bit 7 and the mirror bit
  raw rather than guessing them, the decode-level counterpart to this phase's
  decision not to flag them.
