# Numerical claims

Every number that appears in the docs, the README, or a commit message, paired with its source, so each claim can be audited.

## Fixture · TEAMGROUP UD5-6000 (Phase 1)

| Claim | Value | Source |
| --- | --- | --- |
| Fixture size | 1024 bytes | Measured: `wc -c` of `spdr/tests/fixtures/teamgroup-ud5-6000_0104eef6.spd`; asserted by the `fixture_is_1024_bytes` test. |
| Fixture sha256 | `cecfa75eb704272ad5b135e77a534cc416aec55a8daea54823b5dbf6d7761c98` | Measured: `sha256sum` of the fixture file. |
| SPD device size | 1024 bytes | Decoded from byte 0 bits [6:4]; see the accepted snapshot. |
| SPD revision | 1.0 | Decoded from byte 1. |
| Density per die | 16 Gb | Decoded from byte 4 bits [4:0]. |
| Row / column address bits | 16 / 10 | Decoded from byte 5. |
| I/O width | x8 | Decoded from byte 6 bits [7:5]. |
| Bank groups x banks per group | 8 x 4 (32 banks) | Decoded from byte 7. |
| Package ranks per channel | 1 | Decoded from byte 234 bits [5:3]. |
| Channels per DIMM | 2 | Decoded from byte 235 bits [7:5]. |
| Primary bus width per channel | 32 bits | Decoded from byte 235 bits [2:0]. |
| Module width / ECC | 64-bit, non-ECC | Derived: 2 channels x 32-bit; ECC extension bits [4:3] = 0. |
| Module capacity | 16 GB | Derived from the decoded geometry (8 x8 devices x 16 Gb x 1 rank); matches the part rating. |
| Rated speed / timings / voltage | DDR5-6000, 38-38-38-78, 1.25 V | Part rating (TEAMGROUP T-Create Expert 6000, part code CTCED532G6000HC38ADC01); to be confirmed against the SPD timing bytes in phase 2. |
| Reference markers (not yet decoded) | mfr `0x04ef`, week 37 / 2023, main CRC `0x8021` | Provenance note from the dump source; to be confirmed in phases 2 and 5. |

The unit-test suite count (16 tests) and the toolchain version (Rust 1.96.0 stable) are operational facts recorded in `docs/implementations/2026-06-04-phase-1-foundation.md`, not public correctness claims.
