# Phase 9b · Timing-relationship and speed-bin lint rules

Date: 2026-06-05

## Problem / Motivation

The CRC proves the bytes survived transit; Phase 8 opened the linter to validate
beyond it, with the capacity-consistency rule. This phase adds the second family
of rules, now that both the base JEDEC timings (Phase 3) and the XMP/EXPO rated
profiles (Phase 9a) are decoded: do the timings cohere? A CRC-valid SPD can still
state a tRC that is not tRAS + tRP, a tRAS shorter than tRCD, or a CAS latency it
does not list as supported. These are internal inconsistencies a CRC cannot catch.

The governing constraint is the Phase 8 fixture-lints-clean invariant: the fixture
is a real, valid module, so every rule must emit only on a genuine violation, and
the fixture, valid base block plus two legitimate overclock profiles, must still
produce zero findings. This phase is designed to hold the invariant at zero, not
to evolve it.

The one thing not to get wrong: an XMP/EXPO overclock profile legitimately runs
tighter and faster than any JEDEC bin, by design. No rule here may treat a vendor
profile exceeding JEDEC limits as a defect. Every rule in this phase checks
internal self-consistency (relationships that hold for any DDR5 timing set) or
flags non-standard rates only as `Info`. The JEDEC-limit conformance that would
distinguish base from vendor (the per-bin timing tables) is explicitly deferred.

## What Changed

| File | Change |
| --- | --- |
| `spdr/src/lint.rs` | New `Finding` variants (tRC identity, ordering, non-integer CL, CL-not-supported, non-integer clock timing, non-standard rate), the `TimingOrder` and `ClockTimingParam` enums, the rule functions, the base-and-vendor dispatch in `lint`, and per-rule unit tests. |
| `spdr/src/lib.rs` | Re-exports `TimingOrder` and `ClockTimingParam` (referenced by the new public `Finding` fields); adds a Phase 9b sentence to the crate doc. |
| `docs/numerical-claims.md` | New Phase 9b section: the timing relationships verified on the fixture and the JEDEC standard-rate list with its source. |
| `docs/validated-against.md` | New Phase 9b note: the fixture lints clean under the expanded rule set, with the relationships it satisfies. |

No decoder, no renderer, and no snapshot changed. The Phase 1 through 9a suites and
all snapshots (including the two CLI render snapshots) are untouched: this phase
adds lint rules only.

## Implementation Approach

### Dispatch · run each rule only where its inputs exist

`lint` now decodes three sections independently and runs the rules that have
inputs:

```
if let Ok(identity) = decode_identity_and_base(bytes) { check_capacity(...) }   // Phase 8
if let Ok(timings)  = decode_timings(bytes)           { check_base_timings(...) }
if let Ok(profiles) = decode_vendor_profiles(bytes)   { check_vendor_profiles(...) }
```

The base block carries every timing, so all rules run on it. A vendor profile
carries only the Phase 9a subset (tCK, CL, tAA, tRCD, tRP, tRAS), so
`check_rated_profile` runs only the rules whose inputs are in that subset: the
tRAS >= tRCD ordering, the operating-CAS integrality, the tRCD/tRP clock-multiple
checks, and the recognized-rate check. The base-only rules (tRC identity, tRC >=
tRAS, and CAS-latency-supported-set, which needs the supported-CAS bitmask a
profile does not carry) do not run on profiles. Every division guards a zero cycle
time and skips rather than dividing, so a degenerate or partial decode cannot
panic.

The rule functions take `&mut dyn FnMut(Finding)` rather than a generic sink, so
they can call one another and be driven directly from unit tests; `lint`'s generic
`&mut F` coerces to the trait-object reference at the call site, keeping the public
signature and the alloc-free callback contract from Phase 8 unchanged.

### Timing-relationship rules

- **tRC = tRAS + tRP** (`Finding::TrcIdentityMismatch`, `Error`, base block only).
  tRC is, by definition, the active-to-precharge time plus the precharge time. The
  fixture satisfies it exactly: 48640 = 32000 + 16640. Vendor profiles do not
  carry tRC (Phase 9a deferred it), so this runs on the base block only.
- **Orderings** (`Finding::TimingOrderingViolation`, `Error`). tRAS >= tRCD (the
  active window must span the RAS-to-CAS delay) and tRC >= tRAS (the row cycle
  contains the active window). tRAS >= tRCD needs only tRAS and tRCD, both of which
  a vendor profile carries, so it runs on profiles too; tRC >= tRAS is base-only.
  The brief's tFAW >= 4 x tRRD_S is **not** run: tRRD_S is not in the Phase 3
  decoded set (only tRRD_L is), so the rule has no input. It is recorded as
  deferred below, per the "where those fields are decoded" gate.

### Clock-consistency rules

- **Operating CAS latency** (`Finding::NonIntegerCasLatency`, `Warning`;
  `Finding::CasLatencyNotSupported`, `Error`). The operating CL is tAA / tCK. If
  tAA is not a whole multiple of tCK the CL is not an integer number of clocks, a
  `Warning`, and the supported-set check is skipped (the CL is ill-defined). If it
  is a whole multiple and a supported-CAS set is available (the base block), the
  resulting CL must be a member of it, else an `Error`. The fixture's base CL is
  16640 / 416 = 40, which is in its supported set {22, 24, ..., 40}.
- **Whole-clock timings** (`Finding::NonIntegerClockTiming`, `Warning`). tRCD and
  tRP must be whole multiples of tCK to be realizable in whole clocks. tAA is not
  re-checked here: its integrality is the operating-CAS-latency check, so it is not
  double-counted.

### Speed-bin rule

- **Recognized rate** (`Finding::NonStandardDataRate`, `Info`). The data rate is
  checked against a pinned list of JEDEC-standard DDR5 rates. A rate not on the
  list is `Info`, never `Error`, because a vendor overclock profile may legitimately
  ship a custom rate. A zero rate (the degenerate case of a zero cycle time) is
  skipped, matching the zero-tCK guard on the clock-based rules.

### Deferred: full JEDEC sub-grade-table conformance

Matching a bin's specific tAA/tRCD/tRP limits (for example, that a DDR5-4800 base
profile meets the JEDEC 4800AC/4800B limits) is deferred. It needs the per-bin
JEDEC timing tables pinned, and it carries the base-versus-vendor nuance this phase
deliberately avoids: an overclock profile is *expected* to beat the JEDEC limits,
so a naive "tighter than JEDEC" check would flag every valid XMP/EXPO profile. The
recognized-rate and clock-consistency checks here are verifiable without the
tables; the tabular conformance is a tracked follow-on. The tFAW >= 4 x tRRD_S
ordering is deferred for the same input-availability reason (tRRD_S undecoded).

## Mathematical / Statistical Details

### The relationships, verified on the fixture

| Relationship | Rule | Fixture (base) | Holds |
| --- | --- | --- | --- |
| tRC = tRAS + tRP | exact identity | 48640 = 32000 + 16640 | yes |
| tRAS >= tRCD | ordering | 32000 >= 16640 | yes |
| tRC >= tRAS | ordering | 48640 >= 32000 | yes |
| CL = tAA / tCK integer | clock consistency | 16640 / 416 = 40 | yes |
| CL in supported set | clock consistency | 40 in {22..40} | yes |
| tRCD, tRP whole tCK multiples | clock consistency | 16640 / 416 = 40 each | yes |
| data rate is JEDEC-standard | speed bin | 4800 in the bin list | yes |

The two vendor profiles satisfy the applicable subset as well (DDR5-6000 profile:
tRAS 25974 >= tRCD 12654; CL = 12654 / 333 = 38 exactly; tRCD/tRP 12654 are 38 x
333; 6000 standard. DDR5-5600 profile: tRAS 29988 >= tRCD 14280; CL = 14280 / 357
= 40 exactly; 5600 standard).

### Pinned sources

- **tRC = tRAS + tRP and the orderings** are the JEDEC definitions of the row-cycle
  and active-window timings (tRC is defined as the active-to-precharge time tRAS
  plus the precharge time tRP; tRAS spans the RAS-to-CAS delay tRCD). They are
  universal DRAM timing identities, the same ones decode-dimms and memtest86plus
  rely on, and the fixture's own decoded base timings verify them
  (48640 = 32000 + 16640, 32000 >= 16640). Facts are not copyrightable; the rules
  were written from the definitions, no licensed source copied.
- **The supported-CAS set** is the SPD's own field (JESD400-5 bytes 24-28),
  decoded in Phase 3; the CL-in-set check is self-referential to the image.
- **The JEDEC standard DDR5 data rates** are the speed-bin ladder in 400 MT/s steps
  from 3200 to 8800 MT/s, pinned from the JEDEC DDR5 standard JESD79-5 and its
  addenda: the original defined bins through 6400, JESD79-5A added the 5600 and
  6400 timing definitions, and the April 2024 update (JESD79-5C) added 8800. The
  fixture's 4800 (base), 5600, and 6000 (vendor) are all on the list. The list is
  used only for an `Info`-level observation, so an omission cannot produce a false
  error.

## Design Decisions

- **Hold the invariant at zero.** Every rule checks a relationship the valid
  fixture satisfies, so it lints clean. The rules were chosen for exactly this:
  internal-consistency identities and orderings, plus a recognized-rate observation,
  none of which a valid module violates.
- **Never flag a vendor profile for beating JEDEC.** No rule compares a profile's
  timings to JEDEC limits. The only base-versus-vendor distinction is which rules
  have inputs (a profile lacks tRC and the supported-CAS set), not a stricter
  standard. The non-standard-rate finding is `Info`, not `Error`, precisely because
  custom rates are legitimate.
- **Run only where inputs exist.** A rule needing a field a profile does not carry
  simply does not run on that profile. This keeps the decode-driven dispatch honest
  (no fabricated inputs) and means a partial or short image lints what it can.
- **Guard every division.** A zero cycle time skips the CAS, clock-multiple, and
  rate checks rather than dividing, so the new rules uphold the Phase 6 no-panic
  contract on arbitrary bytes, confirmed by the existing lint-in-robustness
  properties.
- **tAA integrality is the CAS-latency check, not double-counted.** tAA / tCK
  integrality is reported once, as the operating-CAS-latency warning; the
  whole-clock rule covers tRCD and tRP. One fact, one finding.
- **Defer the tabular conformance honestly.** The recognized-rate and
  clock-consistency checks are what is verifiable without the per-bin JEDEC tables.
  The tabular sub-grade conformance and the tFAW >= 4 x tRRD_S ordering are recorded
  as tracked follow-ons rather than approximated.

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

Phase 9b tests (the Phase 1 through 9a suites and all snapshots, including the CLI
render snapshots, are untouched):

- Per-rule unit tests in `lint.rs` with crafted timings: a broken tRC = tRAS + tRP
  (`TrcIdentityMismatch`, Error), a tRAS < tRCD (`TimingOrderingViolation`, Error),
  a non-integer CL (`NonIntegerCasLatency`, Warning), a CL outside the decoded
  supported set (`CasLatencyNotSupported`, Error), a non-integer tRCD
  (`NonIntegerClockTiming`, Warning), and a non-standard rate (`NonStandardDataRate`,
  Info), each emitting exactly the expected finding with the right severity and
  code; the matching consistent cases (the fixture's own values) emitting nothing;
  and a zero-cycle-time case confirming the clock-based rules skip without dividing.
- `fixture_lints_clean` stays green under the expanded rule set: the real module
  produces zero findings, with the base block and both vendor profiles passing
  every applicable rule.
- The Phase 8 lint-in-robustness properties (`arbitrary_bytes_panics_no_decoder`,
  `single_byte_mutation_panics_no_decoder`) still pass: `lint` runs the new rules
  over arbitrary and mutated bytes with no panic and no divide-by-zero.

## Related Docs

- `.claude/briefs/phase-9b-timing-rules.md` · the brief this phase implements.
- `docs/numerical-claims.md` · the timing relationships verified on the fixture and
  the JEDEC standard-rate list with its source.
- `docs/validated-against.md` · the Phase 9b note: the fixture lints clean under the
  expanded rule set.
- `docs/implementations/2026-06-05-phase-8-linter-capacity.md` · the framework, the
  callback-sink contract, and the fixture-lints-clean invariant this phase extends.
- `docs/implementations/2026-06-04-phase-3-timing.md` · the base timings the
  relationship rules consume.
- `docs/implementations/2026-06-05-phase-9a-xmp-expo.md` · the vendor profiles the
  rules also run on, and the deferred fields that bound which rules apply.
