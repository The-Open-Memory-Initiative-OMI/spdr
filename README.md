<p align="center">
  <img src="assets/banner.svg" width="820"
       alt="spdr · a read-only DDR5 SPD decoder and semantic linter; the wordmark beside a spider drawn from copper PCB traces with a memory chip for its body, eight legs routed as traces with via pads at the joints and feet, over a faint web on a dark circuit board">
</p>

# spdr

A read-only, complete JESD400-5 SPD content decoder plus a semantic linter that validates beyond CRC.

Status: decodes the JESD400-5 base content of a DDR5 SPD · identity and base configuration, base configuration CRC, base JEDEC timings, the unbuffered (UDIMM) module-specific block, and the manufacturing block · plus the vendor overclocking profiles (Intel XMP 3.0 and AMD EXPO), each anchored by its own section CRC · and a semantic linter that validates beyond the CRC (capacity, timing relationships, speed bins, reserved bits, and cross-field consistency). Both the decoder and the linter are exposed as a library and as the `spdr` CLI (`spdr decode`, `spdr lint`). Scope is unbuffered-complete (UDIMM); SODIMM, RDIMM, and LRDIMM module-specific decoding is deferred and gated on real fixtures, as are full JEDEC bin-table conformance and the tFAW >= 4 x tRRD_S ordering. The no-panic contract is property-tested (the cargo-fuzz harness is committed but not yet deep-run, so this is "property-tested," not "fuzzed"), and the decoder is confirmed correct against one real module so far, a TEAMGROUP T-Create Expert 6000 (UD5-6000); see `docs/validated-against.md`. This is the v0.1.0 release.

## Install

The library and the CLI are two crates:

```
cargo add spdr            # the library (no_std, serde-free by default)
cargo install spdr-cli    # the CLI; the installed binary is named `spdr`
```

`cargo install spdr` does **not** work: `spdr` is the library crate and ships no binary. Install `spdr-cli` for the tool. The library stays `#![no_std]`, allocation-free, and `#![forbid(unsafe_code)]`; an optional `serde` feature (`cargo add spdr --features serde`) derives `Serialize` only, for JSON output, and is off by default.

## Usage

Decode a raw SPD dump (a saved image, or a Linux sysfs `eeprom`):

```
spdr decode <file>          # human-readable text (default)
spdr decode <file> --json   # JSON, one object keyed by section
```

Exit codes:

| Code | Meaning |
| --- | --- |
| 0 | Fully decoded. A base CRC mismatch is reported, not an error, and does not change the code. |
| 1 | Ran, but at least one section failed to decode (for example a truncated image). The sections that decoded plus the per-section errors are printed, then exit 1. |
| 2 | Could not run: the file was unreadable, or the arguments were invalid. |

Example human output (abridged):

```
[Identity and base]
  SPD device size:               1024 bytes
  DRAM device type:              DDR5 SDRAM
  Module type:                   UDIMM
  Density per die:               16 Gb
  ...
[Base configuration CRC]
  Reported status of the base CRC (bytes 0-509). Not a verdict; the vendor section CRCs are separate.
  Computed:                      0x8021
  Stored:                        0x8021
  Match:                         yes
[JEDEC base timings]
  SPD JEDEC base timings (the guaranteed fallback). The rated DDR5 speed is shown below in the vendor-profiles section.
  Base data rate:                DDR5-4800 (4800 MT/s, JEDEC base)
  ...
[Manufacturing]
  Module manufacturer:           Team Group Inc.
  Serial number:                 0104EEF6
  Part number:                   UD5-6000
  ...
[Vendor profiles (XMP 3.0 / EXPO)]
  Rated overclock profiles. Each section is CRC-checked; the match is the region anchor.
  Intel XMP 3.0:                 present
    Header section CRC:          computed 0x252C, stored 0x252C (match)
    Profile 1: TG-6000-38-38-78
      Data rate:                 DDR5-6000 (6000 MT/s)
      CAS latency:               CL38
      tRCD:                      12654 ps (38 clocks)
      VDD / VDDQ / VPP:          1.250 V / 1.250 V / 1.800 V
      Section CRC:               computed 0x0A5F, stored 0x0A5F (match)
  ...
```

The base timings are the SPD JEDEC fallback the module guarantees; the rated DDR5 speed (DDR5-6000 here) lives in the vendor profiles, decoded in the XMP 3.0 / EXPO section. Each profile section carries its own CRC, recomputed over a pinned range and compared to the stored value: the match is what anchors the region, so an unconfirmed region is never presented as authoritative. For this fixture both XMP and EXPO decode the same rated DDR5-6000 38-38-38-78 at 1.25 V, cross-checking it two independent ways. The base CRC line is a reported status (computed, stored, match), not a pass/fail verdict; the semantic checks beyond the CRC are the job of `spdr lint` (below). `--json` emits the same sections as a single JSON object, with any failed section carrying an `error` indicator so the document stays valid.

## Linting

The linter validates beyond the CRC: the CRC only proves the bytes survived transit, while the linter reports values that are internally inconsistent even in a CRC-valid SPD. It checks capacity math, JEDEC timing relationships, speed-bin recognition, reference-declared reserved bits, and cross-field consistency.

```
spdr lint <file>          # human-readable findings (default)
spdr lint <file> --json   # JSON array of findings (empty array when clean)
```

Each finding has a severity (`error`, `warning`, or `info`), a stable kebab-case code, and a message. Exit codes:

| Code | Meaning |
| --- | --- |
| 0 | Lint ran with no `warning` or `error` findings (clean, or only `info` advisories). |
| 1 | Lint ran and found at least one `warning` or `error`. |
| 2 | Could not run: the file was unreadable, or the arguments were invalid. |

`info` is advisory (a non-standard but legitimate data rate, for example) and does not fail the exit code; it is still printed. When the base configuration does not decode, the checks that depend on it (capacity and cross-field consistency) are skipped while the reserved-bit check still runs, and the human output notes this so a clean result on an unparseable file is not mistaken for a full bill of health. A severity-threshold flag is deferred past v0.1.0.

A clean module prints:

```
[Lint]
  No findings. The SPD is internally consistent under the current rule set.
```

and a module with findings lists them, worst severity first:

```
[Lint]
  2 findings: 1 error, 1 warning.
  error · trc-identity-mismatch
    tRC 48641 ps does not equal tRAS + tRP (32000 + 16640 = 48640 ps); the row-cycle identity tRC = tRAS + tRP is violated
  warning · reserved-bytes-nonzero
    reserved byte at offset 128 is 0xff, but a reference-declared-reserved region must be zero
```

The reference fixture lints clean (exit 0). The `--json` form emits the same findings as an array, each with `severity`, `code`, `message`, and structured `fields`.

## Robustness

On any input, malformed or not, every decoder returns `Ok` or a typed `DecodeError` and never panics. The core crate is `#![forbid(unsafe_code)]`, so a panic is its only crash mode, which makes "never panics" the whole contract.

That contract is property-tested with proptest, in the gate and in CI: arbitrary byte images, single-byte mutations of a real fixture, and every truncation length are each run through the full public decode surface, and any panic fails the test. A cargo-fuzz harness (`spdr/fuzz/`) is also included for deeper fuzzing on Linux. The harness is committed but has not yet been run to depth, so this is property-tested, not "fuzzed" · that claim is earned only after a recorded deep run. See `docs/implementations/2026-06-05-phase-6-robustness.md` for the fuzz invocation and the deep-run ledger.
