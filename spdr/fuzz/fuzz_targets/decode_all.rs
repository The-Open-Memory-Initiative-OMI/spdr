#![no_main]

//! Fuzz target · run every public `spdr` decoder on the fuzzer-provided bytes
//! and discard each result. The core crate forbids unsafe, so a panic is the
//! only crash mode; this target asserts nothing and leaves libFuzzer to catch a
//! panic, an overflow, or a hang. It mirrors the `arbitrary_bytes_panics_no_decoder`
//! proptest property exactly, so the always-on gate and the deeper fuzzer drive
//! the identical decode surface.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = spdr::decode_identity_and_base(data);
    let _ = spdr::verify_base_crc(data);
    let _ = spdr::crc16(data);
    let _ = spdr::decode_timings(data);
    let _ = spdr::decode_module_specific(data);
    let _ = spdr::decode_manufacturing(data);
});
