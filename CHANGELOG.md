# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

[0.1.0]: https://github.com/The-Open-Memory-Initiative-OMI/spdr/releases/tag/v0.1.0
