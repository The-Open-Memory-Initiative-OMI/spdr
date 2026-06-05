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
| Rated speed / timings / voltage | DDR5-6000, 38-38-38-78, 1.25 V | Part rating (TEAMGROUP T-Create Expert 6000, part code CTCED532G6000HC38ADC01); to be confirmed against the SPD timing bytes in a later phase. |
| Reference markers (not yet decoded) | mfr `0x04ef`, week 37 / 2023 | Provenance note from the dump source; to be confirmed in a later phase. |

## Main configuration CRC (Phase 2)

| Claim | Value | Source |
| --- | --- | --- |
| Main CRC, computed | `0x8021` | Computed by `crc16` (CRC-16/XMODEM) over fixture bytes 0-509; asserted by `fixture_main_crc_is_0x8021_and_matches`. |
| Main CRC, stored | `0x8021` | Fixture bytes 510-511 (`0x21 0x80`, little-endian); equals the computed value. |
| Published reference | `0x8021` | The published main CRC for serial 0104eef6; computed equals stored equals this value. |
| CRC-16/XMODEM check vector | `0x31C3` | Standard catalogue check value for input `123456789`; asserted by `crc16_xmodem_check_vector`. |

## Base JEDEC timings (Phase 3)

All decoded from the fixture and cross-checked against independent decoders (decode-dimms, memtest86plus, pyhwinfo, edlf `DDR5SPDEditor`). Absolute-time values are normalised to picoseconds in the type; nanosecond figures below are for human reading.

| Parameter | Value | Source |
| --- | --- | --- |
| Base JEDEC speed | DDR5-4800 (base fallback, not the 6000 profile) | Derived from tCKAVGmin 416 ps; rounded to nearest 100; the 6000 profiles are Phase 9. |
| Implied base CL | CL40 | tAA 16.640 ns / tCKAVGmin 416 ps = 40. |
| tCKAVGmin / tCKAVGmax | 416 ps / 1000 ps | Bytes 20-21 / 22-23, LE ps. |
| Supported CAS latencies | {22, 24, 26, 28, 30, 32, 34, 36, 38, 40} | Bytes 24-28 bitmask, bit i -> CL 20+2i. |
| tAA / tRCD / tRP | 16.640 ns each | Bytes 30-31 / 32-33 / 34-35, LE ps (16640). |
| tRAS / tRC | 32.000 ns / 48.640 ns | Bytes 36-37 / 38-39; tRC = tRAS + tRP. |
| tWR | 30.000 ns | Bytes 40-41, LE ps (30000). |
| tRFC1 / tRFC2 / tRFCsb | 295 ns / 160 ns / 130 ns | Bytes 42-43 / 44-45 / 46-47, LE ns; exact JEDEC 16 Gb figures. |
| tRRD_L / tCCD_L | 5 ns / 8 nCK each | Bytes 70-72 / 73-75 ([ps][nCK]). |
| tCCD_L_WR / tCCD_L_WR2 | 20 ns / 32 nCK · 10 ns / 16 nCK | Bytes 76-78 / 79-81. |
| tFAW | 13.333 ns / 32 nCK | Bytes 82-84. |
| tWTR_L / tWTR_S | 10 ns / 16 nCK · 2.5 ns / 4 nCK | Bytes 85-87 / 88-90. |
| tRTP | 7.5 ns / 12 nCK | Bytes 91-93. |

## Unbuffered module-specific block (Phase 4)

Decoded from the fixture (a UDIMM). Offsets are pinned against edlf `DDR5SPDEditor` (`ddr5spd_structs.h`) and the UniC `SCA08GU04M1F1C-48B` datasheet block map; encodings against decode-dimms and JEDEC Standard 21-C Annex K. Bytes 230-233 fall inside the main-CRC-covered range (0-509), so they are already integrity-checked (the floor, not content correctness).

| Claim | Value | Source |
| --- | --- | --- |
| Module type dispatch | UDIMM decoded; SODIMM / RDIMM / LRDIMM deferred | Byte 3 low nibble (0x02 = UDIMM); other registered types resolve to `NotYetDecoded`, no fixture yet. |
| Module nominal height | 32 mm | Byte 230 = 0x11; `(byte & 0x1f) + 15`; top of the 31 < h <= 32 mm range (a 31.25 mm UDIMM). |
| Module max thickness, front | 2 mm | Byte 231 bits [3:0] = 0x1; `(nibble) + 1`. |
| Module max thickness, back | 1 mm | Byte 231 bits [7:4] = 0x0; `(nibble) + 1`. |
| Reference raw card | card A, revision 0 | Byte 232 = 0x00; code 0 -> index 0 (alphabet `ABCDEFGHJKLMNPRTUVWY`), revision bits [6:5] = 0, no extension. |
| Rank 1 address mapping | mirrored | Byte 233 bit 0 = 1; `byte & 0x01`. |
| Module attributes raw | 0x81 | Byte 233 preserved whole; bit 0 interpreted above, bit 7 is a reserved-set bit left for the linter. |

Test counts and the toolchain version are operational facts recorded in the per-phase implementation docs, not public correctness claims; they are deliberately not pinned in this ledger so it does not go stale each phase.
