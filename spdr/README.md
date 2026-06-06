# spdr

A read-only, zero-copy DDR5 SPD content decoder plus a semantic linter that
validates beyond the CRC. Input is a raw 1024-byte SPD image; output is a fully
typed decode and a structured lint report. The crate never writes SPD, never
touches hardware, and performs no network I/O. Because JESD400-5 is paywalled,
the decoder doubles as an open reference: every field decoder is explicit and
documented rather than table-driven.

This is the library crate. The `spdr` command-line tool lives in the companion
[`spdr-cli`](https://crates.io/crates/spdr-cli) crate.

## Install

```
cargo add spdr
```

`spdr` is `#![no_std]` and allocation-free by default, so it stays embeddable in
firmware and UEFI contexts. It is `#![forbid(unsafe_code)]`, and every slice
access is bounds-checked, so malformed input returns an `Err` rather than
panicking.

An optional `serde` feature derives `Serialize` (only) on the public decoded
types for JSON output; it is off by default, so the default build stays `no_std`
and serde-free:

```
cargo add spdr --features serde
```

## What it decodes

- The JESD400-5 base content: identity and base configuration, the base
  configuration CRC (reported, never a gate on decoding), the base JEDEC
  timings, the unbuffered (UDIMM) module-specific block, and the manufacturing
  block.
- The vendor overclocking profiles, Intel XMP 3.0 and AMD EXPO, each anchored by
  its own section CRC so an unconfirmed region is never presented as
  authoritative.

The linter (the `lint` function and the `Finding` / `Severity` types) goes past
the CRC: the CRC only proves the bytes survived transit, while the linter reports
values that are internally inconsistent even in a CRC-valid SPD. It covers four
rule families: capacity math, JEDEC timing relationships and speed-bin
recognition, reference-declared reserved bits, and cross-field consistency.

## Scope

Unbuffered (UDIMM) is complete. SODIMM, RDIMM, and LRDIMM module-specific
decoding is deferred and gated on real fixtures of each type; those types resolve
to an explicit not-yet-decoded result rather than a guess. Full JEDEC
sub-grade-table conformance and the tFAW >= 4 x tRRD_S ordering are likewise
deferred. The no-panic contract is property-tested (a cargo-fuzz harness is
committed but not yet deep-run, so this is "property-tested," not "fuzzed"). The
decoder is confirmed correct against one real module so far, a TEAMGROUP
T-Create Expert 6000 (UD5-6000); see `docs/validated-against.md` in the
repository.

## License

Apache-2.0.
