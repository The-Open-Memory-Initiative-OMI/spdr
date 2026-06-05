# spdr

A read-only, complete JESD400-5 SPD content decoder plus a semantic linter that validates beyond CRC.

Status: decodes the JESD400-5 base content of a DDR5 SPD · identity and base configuration, base configuration CRC, base JEDEC timings, the unbuffered (UDIMM) module-specific block, and the manufacturing block · as a library and a `spdr` CLI. The rated XMP/EXPO profiles and the semantic linter are later phases.

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
  SPD JEDEC base timings. The rated DDR5 profile lives in XMP/EXPO and is decoded in a later version.
  Base data rate:                DDR5-4800 (4800 MT/s, JEDEC base)
  ...
[Manufacturing]
  Module manufacturer:           Team Group Inc.
  Serial number:                 0104EEF6
  Part number:                   UD5-6000
  ...
```

The timings shown are the SPD JEDEC base, not the rated DDR5 profile (which lives in XMP/EXPO, decoded in a later version). The CRC line is a reported status (computed, stored, match), not a pass/fail verdict; the semantic linter is Phase 11. `--json` emits the same sections as a single JSON object, with any failed section carrying an `error` indicator so the document stays valid.

## Robustness

On any input, malformed or not, every decoder returns `Ok` or a typed `DecodeError` and never panics. The core crate is `#![forbid(unsafe_code)]`, so a panic is its only crash mode, which makes "never panics" the whole contract.

That contract is property-tested with proptest, in the gate and in CI: arbitrary byte images, single-byte mutations of a real fixture, and every truncation length are each run through the full public decode surface, and any panic fails the test. A cargo-fuzz harness (`spdr/fuzz/`) is also included for deeper fuzzing on Linux. The harness is committed but has not yet been run to depth, so this is property-tested, not "fuzzed" · that claim is earned only after a recorded deep run. See `docs/implementations/2026-06-05-phase-6-robustness.md` for the fuzz invocation and the deep-run ledger.
