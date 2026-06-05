//! Vendor-profile integration tests over the real fixture · the two oracles.
//!
//! The XMP 3.0 and EXPO regions are the thinnest-referenced decode in the
//! project, so they are not trusted on their own; they are pinned by two
//! independent oracles asserted here:
//!
//! - **Section CRCs** (the region anchor): each section's CRC, recomputed over
//!   the pinned range with the Phase 2 primitive, equals the value stored in the
//!   image. The match confirms the region, the range, and the algorithm. The
//!   stored values for this dump (serial 0104EEF6) are asserted by their exact
//!   hex, the way Phase 2 asserted the base block's `0x8021`.
//! - **Rated timings** (the value oracle): the module is rated DDR5-6000
//!   38-38-38-78 at 1.25 V. Both the decoded XMP profile 1 and the decoded EXPO
//!   profile 1 reproduce that, cross-checking the same numbers two ways. The
//!   second profile of each (DDR5-5600 40-40-84) is checked too.
//!
//! The fixture is loaded with `include_bytes!`, as the other integration tests
//! do.

use spdr::{
    Expo, ExpoProfile, Millivolts, Picoseconds, RatedTimings, Xmp, XmpProfile, decode_expo,
    decode_xmp,
};

const FIXTURE: &[u8] = include_bytes!("fixtures/teamgroup-ud5-6000_0104eef6.spd");

// The published section CRCs for this dump (serial 0104EEF6), little-endian as
// stored. Each is the region anchor for its block.
const XMP_HEADER_CRC: u16 = 0x252C;
const XMP_PROFILE1_CRC: u16 = 0x0A5F;
const XMP_PROFILE2_CRC: u16 = 0x0AC4;
const EXPO_BLOCK_CRC: u16 = 0x9FE2;

/// Pull the present XMP region, failing loudly if the magic is somehow absent.
fn xmp() -> Xmp<'static> {
    decode_xmp(FIXTURE).expect("fixture is full length")
}

/// Pull the present EXPO region.
fn expo() -> Expo {
    decode_expo(FIXTURE).expect("fixture is full length")
}

#[test]
fn xmp_section_crcs_match_stored() {
    let Xmp::Present {
        header_crc,
        profile1,
        profile2,
    } = xmp()
    else {
        panic!("the fixture carries the XMP 3.0 magic");
    };

    // The region anchor: computed equals stored for every section.
    assert!(header_crc.matches, "XMP header CRC: {header_crc:?}");
    assert_eq!(header_crc.computed, XMP_HEADER_CRC);
    assert_eq!(header_crc.stored, XMP_HEADER_CRC);

    let p1 = profile1.expect("profile 1 enabled");
    assert!(p1.crc.matches, "XMP profile 1 CRC: {:?}", p1.crc);
    assert_eq!(p1.crc.computed, XMP_PROFILE1_CRC);
    assert_eq!(p1.crc.stored, XMP_PROFILE1_CRC);

    let p2 = profile2.expect("profile 2 enabled");
    assert!(p2.crc.matches, "XMP profile 2 CRC: {:?}", p2.crc);
    assert_eq!(p2.crc.computed, XMP_PROFILE2_CRC);
    assert_eq!(p2.crc.stored, XMP_PROFILE2_CRC);
}

#[test]
fn expo_block_crc_matches_stored() {
    let Expo::Present { block_crc, .. } = expo() else {
        panic!("the fixture carries the EXPO magic");
    };
    assert!(block_crc.matches, "EXPO block CRC: {block_crc:?}");
    assert_eq!(block_crc.computed, EXPO_BLOCK_CRC);
    assert_eq!(block_crc.stored, EXPO_BLOCK_CRC);
}

#[test]
fn xmp_profile1_is_rated_ddr5_6000_38_38_38_78_1v25() {
    let p = present_xmp_profile1();
    assert_eq!(p.name, Some("TG-6000-38-38-78"));
    assert_rated_6000_38_38_38_78_1v25(&p.rated);
}

#[test]
fn expo_profile1_is_rated_ddr5_6000_38_38_38_78_1v25() {
    let p = present_expo_profile1();
    assert_rated_6000_38_38_38_78_1v25(&p.rated);
}

#[test]
fn xmp_profile2_is_rated_ddr5_5600_40_40_84() {
    let Xmp::Present { profile2, .. } = xmp() else {
        panic!("XMP present");
    };
    let p = profile2.expect("profile 2 enabled");
    assert_eq!(p.name, Some("TG-5600-40-40-84"));
    assert_eq!(p.rated.data_rate_mt_s, 5600);
    assert_eq!(p.rated.cas_latency, 40);
    // 40-40-84 in clock counts at tCK 357 ps.
    assert_eq!(clock_counts(&p.rated), (40, 40, 84));
}

#[test]
fn expo_profile2_is_rated_ddr5_5600_40_40_84() {
    let Expo::Present { profile2, .. } = expo() else {
        panic!("EXPO present");
    };
    let p = profile2.expect("profile 2 populated");
    assert_eq!(p.rated.data_rate_mt_s, 5600);
    assert_eq!(p.rated.cas_latency, 40);
    assert_eq!(clock_counts(&p.rated), (40, 40, 84));
}

#[test]
fn xmp_and_expo_profile1_agree_field_by_field() {
    // The two formats encode the same rated profile differently; decoding both
    // and finding them identical is the cross-check the brief calls for.
    let a = present_xmp_profile1().rated;
    let b = present_expo_profile1().rated;
    assert_eq!(a, b);
}

#[test]
fn absent_profiles_on_input_without_magic() {
    // A synthetic image with no magic yields the no-profile result, parses
    // nothing, and does not panic.
    let blank = [0u8; 1024];
    assert_eq!(decode_xmp(&blank).unwrap(), Xmp::Absent);
    assert_eq!(decode_expo(&blank).unwrap(), Expo::Absent);
}

// --- helpers ---------------------------------------------------------------

fn present_xmp_profile1() -> XmpProfile<'static> {
    let Xmp::Present { profile1, .. } = xmp() else {
        panic!("XMP present");
    };
    profile1.expect("profile 1 enabled")
}

fn present_expo_profile1() -> ExpoProfile {
    let Expo::Present { profile1, .. } = expo() else {
        panic!("EXPO present");
    };
    profile1.expect("profile 1 populated")
}

/// Whole-cycle counts of (tRCD, tRP, tRAS), each timing divided by the cycle
/// time. This is the "in clock counts" reading the brief asks the assertion to
/// reproduce.
fn clock_counts(r: &RatedTimings) -> (u32, u32, u32) {
    let tck = r.cycle_time.picoseconds();
    let cycles = |t: Picoseconds| (t.picoseconds() + tck / 2) / tck;
    (cycles(r.trcd), cycles(r.trp), cycles(r.tras))
}

/// The full rated-timing oracle: DDR5-6000, CL38, tRCD/tRP/tRAS 38/38/78 both in
/// time and in clock counts, and 1.25 V on VDD and VDDQ (with VPP at 1.8 V).
fn assert_rated_6000_38_38_38_78_1v25(r: &RatedTimings) {
    assert_eq!(r.cycle_time, Picoseconds(333));
    assert_eq!(r.data_rate_mt_s, 6000);
    assert_eq!(r.cas_latency, 38);

    // In time: 38/38/78 cycles at 333 ps.
    assert_eq!(r.trcd, Picoseconds(12654));
    assert_eq!(r.trp, Picoseconds(12654));
    assert_eq!(r.tras, Picoseconds(25974));
    // In clock counts: dividing by the cycle time gives 38/38/78.
    assert_eq!(clock_counts(r), (38, 38, 78));

    assert_eq!(r.vdd, Millivolts(1250));
    assert_eq!(r.vddq, Millivolts(1250));
    assert_eq!(r.vpp, Millivolts(1800));
}
