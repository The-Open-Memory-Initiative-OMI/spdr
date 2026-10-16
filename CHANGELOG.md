# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Quality and documentation pass over the shipped surface. No public API change
and no decoded-output change: every test and snapshot passes unchanged.

### Added

- Regression tests for the two saturation guards and the supported-CAS bit
  domain, so the silent-narrowing fix above cannot come back unnoticed: each was
  confirmed to fail against the previous `as` cast (it reported a wrapped `CL5`
  instead of a saturated one).
- 29 pure decoders and accessors marked `const fn`, so a constant SPD byte can be
  decoded at compile time — the point of an embeddable `no_std` crate.
- An explicit `# Errors` section on every public `Result`-returning function,
  and a curated set of `clippy::pedantic` lints (silent-cast, doc, and argument
  idioms) denied at the workspace level, so the classes of issue fixed here
  cannot silently regress. CI now also builds rustdoc with `-D warnings`.

### Fixed

- The documented reason the fuzz harness "cannot link on Windows" was wrong: it
  does compile and link under MSVC, then fails to load on the
  `clang_rt.asan_dynamic-x86_64.dll` import. Corrected in
  `spdr/fuzz/Cargo.toml` and the Phase 6 report, with the attempt recorded in
  the deep-run ledger instead of left silently blank.
- Rustdoc parsed bit-range notation in doc comments (`bits [7:5]`) as
  intra-doc links, so every rendered field doc was a broken link; the notation
  is now escaped and renders exactly as written. Caught by the new doc gate.
- The two internal CAS-latency derivations (the linter's supported-set check and
  the vendor-profile rated CL) narrowed `u32` quotients to `u16` with an
  unchecked `as`. The quotient always fit for decoded SPD bytes, but the
  narrowing is now explicit and saturating instead of wrapping for out-of-range
  hand-built values.
- The supported-CAS mask arithmetic and the reference-raw-card `Display` used
  unchecked narrowing casts on a constant bit count and on array-length
  indexing; both now compute in one integer domain, with a compile-time check
  tying the mask bit count to its byte count.
- A rustdoc warning: a public doc link to a private item in `spdr-cli`.

## [0.1.1] - 2026-09-16

Metadata only. No code, no API change, no behaviour change from 0.1.0.

### Changed

- Declare `homepage` on both crates, so crates.io links the project site from
  the crate pages. Added to `[workspace.package]` and opted into by each member
  with `homepage.workspace = true`, since workspace inheritance is per field.
- `spdr-cli` now requires `spdr` 0.1.1, keeping the two in lockstep as 0.1.0
  released them.

## [0.1.0] - 2026-06-06

First release: a read-only DDR5 SPD content decoder plus a semantic linter that
validates beyond the CRC, shipped as two crates · the `spdr` library
(`#![no_std]`, allocation-free, `#![forbid(unsafe_code)]`) and the `spdr-cli`
tool (binary `spdr`).

### Added

- **JESD400-5 base content decode.** Identity and base configuration, the base
  configuration CRC (reported as computed/stored/match, never a gate on
  decoding), the base JEDEC timings, the unbuffered (UDIMM) module-specific
  block, and the manufacturing information block (including JEP-106 manufacturer
  resolution). Zero-copy over a byte slice; bounds-checked, so malformed input
  returns an error rather than panicking.
- **Vendor overclocking profiles.** Intel XMP 3.0 and AMD EXPO, each anchored by
  its own section CRC (computed-equals-stored over a pinned range), so an
  unconfirmed region is never presented as authoritative.
- **Semantic linter, four rule families.** Capacity math (the integer-device-count
  precondition of the JEDEC capacity formula); timing relationships and speed-bin
  recognition (the tRC = tRAS + tRP identity, the tRAS >= tRCD and tRC >= tRAS
  orderings, integer CAS latency and supported-set membership, whole-clock tRCD /
  tRP, and JEDEC-standard data-rate recognition); reference-declared reserved bits;
  and cross-field consistency (package-type / die-count coherence). Findings are
  structured (severity, stable kebab-case code, message, byte offset / fields). No
  rule treats a vendor overclock profile as a defect for being tighter or faster
  than a JEDEC bin.
- **CLI: `spdr decode` and `spdr lint`,** each with a human-readable default and a
  `--json` form. Exit-code contracts:
  - `decode` · `0` fully decoded (a base CRC mismatch is reported, not an error),
    `1` at least one section failed to decode (partial output is still printed),
    `2` the file was unreadable or the arguments were invalid.
  - `lint` · `0` clean or `info`-only advisories, `1` at least one `warning` or
    `error` finding, `2` the same operational failures as `decode`. When the base
    configuration does not decode, the checks that depend on it (capacity and
    cross-field consistency) are skipped while the reserved-bit check still runs,
    and the output notes this so a clean result on an unparseable file is not
    mistaken for a full bill of health.
- **Optional `serde` feature** on the `spdr` library: `Serialize`-only derives on
  the public decoded types, for the CLI's `--json` output. Off by default, so the
  default library build stays `no_std` and serde-free.

### Robustness

- The no-panic contract is **property-tested** with proptest, in the gate and in
  CI: arbitrary byte images, single-byte mutations of a real fixture, and every
  truncation length run through the full public decode surface, plus a lint and a
  render render-robustness property; any panic fails the test. A cargo-fuzz
  harness (`spdr/fuzz/`) is committed for deeper fuzzing on Linux but has not yet
  been run to depth, so this release claims "property-tested," not "fuzzed."

### Validated against

- One real module so far: a TEAMGROUP T-Create Expert 6000 (UD5-6000), confirmed
  field by field against independent open decoders and its published reference.
  See `docs/validated-against.md`.

### Deferred (not in this release)

- SODIMM, RDIMM, and LRDIMM module-specific decoding (their register and
  data-buffer blocks), each gated on a real fixture of that type; those module
  types currently resolve to an explicit not-yet-decoded result.
- Full JEDEC sub-grade-table conformance (matching a bin's specific tAA / tRCD /
  tRP limits).
- The tFAW >= 4 x tRRD_S ordering rule (tRRD_S is not in the decoded timing set).
- A lint severity-threshold flag.

[0.1.1]: https://github.com/The-Open-Memory-Initiative-OMI/spdr/releases/tag/v0.1.1
[0.1.0]: https://github.com/The-Open-Memory-Initiative-OMI/spdr/releases/tag/v0.1.0
