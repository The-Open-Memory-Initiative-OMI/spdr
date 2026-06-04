# Validated against

The enumerable set of real DDR5 modules this decoder is confirmed correct on; every public correctness claim is backed by an entry here, not by a vibe.

## TEAMGROUP T-Create Expert 6000 CL38 · UD5-6000

- **Fixture:** `spdr/tests/fixtures/teamgroup-ud5-6000_0104eef6.spd` (1024 bytes, sha256 `cecfa75eb704272ad5b135e77a534cc416aec55a8daea54823b5dbf6d7761c98`).
- **Module:** TEAMGROUP T-Create Expert 6000 CL38. Part number UD5-6000; full part code CTCED532G6000HC38ADC01. Rated DDR5-6000, 38-38-38-78, 1.25 V.
- **Capacity / organization:** 16 GB, single rank, x8 devices, 16 Gb monolithic dies (consistent with the Phase 1 decode below).
- **Source:** `ubihazard/ddr5-spd-recovery`, file `dumps/teamgroup/t-create-expert_6000_38-38-38-78_1.25_1x8_16x2_[ctced532g6000hc38adc01]/ud5-6000_0104eef6.spd`.
- **Provenance:** public community dump. The SPD content is factual module data, attributed to the source collection. Cross-checked field by field against independent open decoders (`decode-dimms`, pyhwinfo, memtest86plus, edlf `DDR5SPDEditor`) and the part rating; correctness is confirmed at review, not asserted by the snapshot alone.

### Confirmed by Phase 1 (identity and base block)

DDR5 SDRAM, UDIMM (non-hybrid), SPD revision 1.0, SPD device size 1024 bytes; 16 Gb density per die, monolithic (1 die), 16 row / 10 column address bits, x8 I/O width, 8 bank groups x 4 banks per group (32 banks); 1 package rank per channel (symmetric), 2 channels per DIMM, 32-bit primary bus width per channel (2 x 32 = 64-bit, non-ECC).

### Confirmed by Phase 2 (base configuration CRC)

Main configuration CRC `0x8021`: computed over bytes 0-509 with CRC-16/XMODEM and read from the stored bytes 510-511 (little-endian), computed equals stored equals `0x8021`, matching the published reference for serial 0104eef6.

### Reference markers to confirm in later phases (not asserted now)

Module manufacturer ID `0x04ef`, manufacturing date week 37 of 2023, plus the published XMP and EXPO section CRCs. The manufacturing fields are confirmed in a later phase; the XMP and EXPO section CRCs are vendor extensions confirmed in Phase 9. (The main configuration CRC marker is now confirmed above.)
