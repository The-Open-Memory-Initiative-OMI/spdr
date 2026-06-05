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

### Confirmed by Phase 3 (base JEDEC timing block)

Base JEDEC speed: DDR5-4800 (tCKAVGmin 416 ps), CL40 implied (tAA 16.640 ns / tCKAVGmin). This is the base fallback the module guarantees; the advertised DDR5-6000 38-38-38-78 lives in the XMP and EXPO profiles and is confirmed in Phase 9, not here.

Decoded base timings: tCKAVGmin 416 ps, tCKAVGmax 1000 ps; supported CAS latencies {22, 24, 26, 28, 30, 32, 34, 36, 38, 40}; tAA / tRCD / tRP 16.640 ns, tRAS 32.000 ns, tRC 48.640 ns (= tRAS + tRP), tWR 30.000 ns; tRFC1 295 ns, tRFC2 160 ns, tRFCsb 130 ns; tRRD_L 5 ns / 8 nCK, tCCD_L 5 ns / 8 nCK, tCCD_L_WR 20 ns / 32 nCK, tCCD_L_WR2 10 ns / 16 nCK, tFAW 13.333 ns / 32 nCK, tWTR_L 10 ns / 16 nCK, tWTR_S 2.5 ns / 4 nCK, tRTP 7.5 ns / 12 nCK. The tRFC values (295 / 160 / 130 ns) are the exact JEDEC 16 Gb figures, corroborating the Phase 1 density of 16 Gb per die.

### Confirmed by Phase 4 (unbuffered module-specific block)

The fixture is a UDIMM, so the module-type dispatch routes it to the unbuffered decode. Decoded unbuffered fields (bytes 230-233, inside the main-CRC-covered range 0-509, so already integrity-checked):

- Module nominal height: 32 mm (byte 230 = 0x11; `(byte & 0x1f) + 15`). This is the top of the 31 < h <= 32 mm range, consistent with a 31.25 mm UDIMM.
- Module maximum thickness: 2 mm front, 1 mm back (byte 231 = 0x01; each nibble + 1 mm).
- Reference raw card: card A, revision 0 (byte 232 = 0x00; code 0 on the JEDEC alphabet `ABCDEFGHJKLMNPRTUVWY`, revision bits [6:5] = 0).
- Rank 1 edge-connector-to-DRAM address mapping: mirrored (byte 233 bit 0 = 1). The full byte (0x81) is preserved raw; only bit 0 is interpreted, and the set reserved bit 7 is left for the linter, not guessed.

Offsets are pinned against edlf `DDR5SPDEditor` (`ddr5spd_structs.h`) and the UniC `SCA08GU04M1F1C-48B` datasheet block map (bytes 192-239 are common module parameters, Annex A.0); encodings against decode-dimms and JEDEC Standard 21-C Annex K. Correctness of these physical fields is confirmed at review against DDR5SPDEditor's physical-attribute readout and the part's mechanical detail, not by the snapshot alone.

SODIMM, RDIMM, and LRDIMM module-specific decoding is **not yet implemented**. Those types resolve to an explicit not-yet-decoded result that names the type and parses no fields: their substantive content is the per-type register (RCD) and data-buffer block at bytes 240+, which is not guessed without a real fixture. Decoding each is deferred to a later phase gated on a real module of that type.

### Confirmed by Phase 5 (manufacturing block)

The manufacturing block sits at bytes 512-554, past the byte-509 end of the main CRC, so the Phase 2 integrity floor does not reach it. The verification is the published reference for serial 0104eef6 instead. Four fields are self-verifying oracles; the rest are confirmed at review against DDR5SPDEditor's readout.

- Module manufacturer: `0x04ef` -> JEP-106 bank 5, code 0x6f -> "Team Group Inc." (the TEAMGROUP brand). **Oracle match.**
- Manufacturing date: week 37 of 2023 (bytes 515-516 BCD). **Oracle match.**
- Serial number: `0104EEF6` (bytes 517-520). **Oracle match.**
- Part number: "UD5-6000" (bytes 521-550 ASCII, trailing padding trimmed). **Oracle match.**
- Manufacturing location: 0 (byte 514, manufacturer-specific raw code).
- Module revision code: 0 (byte 551).
- DRAM manufacturer: `0x80ad` -> JEP-106 bank 1, code 0x2d -> "SK Hynix" (bytes 552-553).
- DRAM stepping: 255 (`0xff`, the conventional "not specified") (byte 554).

Offsets pinned against edlf `DDR5SPDEditor` (`ddr5spd_structs.h`) and pyhwinfo; the JEP-106 parity/bank convention against decode-dimms; manufacturer names against the freeipmi JEDEC manufacturer ID table. The JEP-106 names "Team Group Inc." and "SK Hynix" are the fixture's two verified entries; the rest of the embedded table is cited reference data.

This **closes the two reference markers carried since Phase 1**: the module manufacturer ID `0x04ef` and the manufacturing date week 37 of 2023 are now decoded and matching the published reference, no longer to-be-confirmed.

### Milestone: base JESD400-5 content decode complete for unbuffered DDR5 modules

With Phase 5, the base JESD400-5 SPD content is fully decoded for an unbuffered (UDIMM) DDR5 module: identity and base geometry, the base configuration CRC, the base JEDEC timings, the unbuffered module-specific block, and the manufacturing information block. The fixture decodes end to end and its published-reference fields (CRC `0x8021`, manufacturer 0x04ef -> TEAMGROUP, serial 0104eef6, part UD5-6000, week 37 / 2023) all reproduce exactly.

The limits of that claim, stated honestly:

- **XMP and EXPO are not JESD400-5.** The advertised DDR5-6000 38-38-38-78 profiles are vendor extensions in the end-user-programmable region, outside the JESD400-5 base content. They are decoded in Phase 9 and are not part of this milestone.
- **SODIMM, RDIMM, and LRDIMM remain deferred.** Their module-specific register and data-buffer blocks are not decoded; those types still resolve to an explicit not-yet-decoded result, each gated on a real fixture in a later phase.

### Linter baseline (Phase 8)

Under the Phase 8 rule set (the single capacity-consistency rule), the fixture produces **zero lint findings**. Its geometry is internally consistent: the primary bus width per channel (32 bits) is a positive integer multiple of the SDRAM I/O width (x8), so the per-rank device count is the whole number 4, and the capacity derives to 16 GB (matching the part rating and the Phase 1 decode). This is the clean-lint baseline the `fixture_lints_clean` test enforces permanently: as rules are added in later phases, a rule that flags this valid module is a bug in the rule, and that test catches it. The capacity formula and its pinned source are recorded in `docs/numerical-claims.md`.

### Reference markers to confirm in later phases (not asserted now)

The published XMP and EXPO section CRCs only. These are vendor extensions confirmed in Phase 9. (The main configuration CRC marker is confirmed in Phase 2 above; the module manufacturer ID and manufacturing date markers are confirmed in Phase 5 above.)
