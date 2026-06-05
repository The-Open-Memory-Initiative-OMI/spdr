# spdr

A read-only, complete JESD400-5 SPD content decoder plus a semantic linter that validates beyond CRC.

Status: scaffolding stage · no decoding logic yet.

## Robustness

On any input, malformed or not, every decoder returns `Ok` or a typed `DecodeError` and never panics. The core crate is `#![forbid(unsafe_code)]`, so a panic is its only crash mode, which makes "never panics" the whole contract.

That contract is property-tested with proptest, in the gate and in CI: arbitrary byte images, single-byte mutations of a real fixture, and every truncation length are each run through the full public decode surface, and any panic fails the test. A cargo-fuzz harness (`spdr/fuzz/`) is also included for deeper fuzzing on Linux. The harness is committed but has not yet been run to depth, so this is property-tested, not "fuzzed" · that claim is earned only after a recorded deep run. See `docs/implementations/2026-06-05-phase-6-robustness.md` for the fuzz invocation and the deep-run ledger.
