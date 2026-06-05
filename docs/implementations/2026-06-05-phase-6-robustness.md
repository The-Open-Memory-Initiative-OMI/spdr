# Phase 6 · Robustness harness

Date: 2026-06-05

## Problem / Motivation

Every earlier phase relied on one contract but only spot-checked it: on any input,
malformed or not, every decoder returns `Ok` or a typed `DecodeError` and never
panics. The golden snapshots prove the right values come out of the real fixture;
they say nothing about what happens on the wrong bytes. This phase makes that
no-panic contract an always-on, generated check rather than a handful of
hand-written truncation cases.

This is robustness, not correctness. Because the core crate is
`#![forbid(unsafe_code)]`, a panic is the only crash mode it has, so "never
panics" is the whole contract. The probable source of a panic in a
bounds-checked parser is not indexing (every read already goes through
`SpdImage`'s `slice::get`) but arithmetic: a shift by a fuzzed exponent, or a
multiply or add in a unit normalization, overflowing in a debug build. The
properties below drive exactly that surface.

Two tools, split by where they can run:

- **proptest** lives in the gate. It runs on Windows and in CI and is the
  always-on check. No CI workflow change is needed: the new properties are
  ordinary `#[test]`s and ride into the existing `cargo test` job.
- **cargo-fuzz** is committed as a harness but is **not built or run here**.
  libFuzzer needs a clang/LLVM toolchain (Linux or WSL) that Windows MSVC does
  not provide, so the fuzz crate cannot link on this machine. It is scaffolded,
  rustfmt-clean, and ready for Mert to run on Linux. No fuzz result is claimed.

## What Changed

| File | Change |
| --- | --- |
| `spdr/Cargo.toml` | Adds `proptest = "1"` as a dev-dependency (gate-only; the core crate gains no runtime dependency). |
| `spdr/tests/robustness.rs` | New: the three robustness checks (arbitrary bytes, single-byte mutation, every truncation length) over the whole public decode surface. |
| `spdr/fuzz/Cargo.toml` | New: the `spdr-fuzz` package · `publish = false`, edition 2024, `[package.metadata] cargo-fuzz = true`, `libfuzzer-sys = "0.4.13"` + `spdr = { path = ".." }`, one `decode_all` bin, and an empty `[workspace]` so the root gate ignores it. |
| `spdr/fuzz/fuzz_targets/decode_all.rs` | New: a single `fuzz_target!` running every decoder on the bytes, mirroring the arbitrary-bytes property. |
| `spdr/fuzz/.gitignore` | New: ignores `target/`, `artifacts/`, `coverage/`; keeps `corpus/`. |
| `spdr/fuzz/corpus/decode_all/teamgroup-ud5-6000_0104eef6.spd` | New: the fixture, seeded as the corpus so a fresh clone fuzzes from a valid SPD and mutates outward. |
| `README.md` | Adds an honest robustness note (property-tested for the no-panic contract; fuzz harness included for Linux). |

No decoder source under `spdr/src/` changed: the properties found no panic, so no
checked-arithmetic fix was required. Every Phase 1 through 5 snapshot is
byte-for-byte unchanged and green.

## Implementation Approach

`spdr/tests/robustness.rs` defines one helper, `run_all_decoders(&[u8])`, that
calls every public decoder and discards each result:

- `decode_identity_and_base` (identity and base block)
- `verify_base_crc` and `crc16` (base CRC)
- `decode_timings` (base JEDEC timings)
- `decode_module_specific` (module-specific block and dispatch)
- `decode_manufacturing` (manufacturing block)

A panic in any of them fails the calling test; a typed `DecodeError` is fine. The
cargo-fuzz target runs the identical list, so the always-on gate and the deeper
fuzzer drive the same decode surface.

### The three checks

- **Arbitrary bytes** (`arbitrary_bytes_panics_no_decoder`, proptest). For a
  generated `Vec<u8>` of length `0..=2048`, `run_all_decoders` panics nothing.
  The length range spans below, at, and above the 1024-byte SPD size, so short,
  full, and over-long images are all exercised. The body only calls and discards;
  a panic fails the test and proptest shrinks the input to a minimal reproducer.
- **Single-byte mutation** (`single_byte_mutation_panics_no_decoder`, proptest).
  Starting from the real fixture, one byte at a proptest-chosen index is set to a
  proptest-chosen value, then `run_all_decoders` panics nothing. This walks
  outward one byte at a time from a known-valid image, the most likely shape to
  trip a single mis-bounded field; shrinking would report the minimal offending
  `(index, value)` if one existed.
- **Every truncation length** (`every_truncation_returns_ok_or_truncated`, a plain
  exhaustive `#[test]`). The truncation space is `0..=1024`, small and finite, so
  proptest is unnecessary. For every prefix length of the fixture, every decoder
  panics none of them and additionally returns `Ok` or `Truncated`. It can never
  be `UnknownEnum` or `NonAscii`, because the bytes that remain are the real
  fixture's own valid bytes: a decoder either reads every byte it needs (`Ok`) or
  runs off the end (`Truncated`). This is a strictly stronger assertion than
  no-panic, and it is cheap because the space is enumerable.

### The cargo-fuzz harness

`spdr/fuzz/` is the canonical cargo-fuzz layout (the `fuzz/` directory beside the
member crate's manifest), so `spdr = { path = ".." }` resolves to the `spdr`
package. The empty `[workspace]` table makes `fuzz/` its own workspace root, so
`cargo build/clippy/test --workspace` from the repository root never sees it. The
fuzz crate is allowed to use unsafe through the `fuzz_target!` macro; that is a
separate crate and does not relax the `spdr` crate's `forbid(unsafe_code)`.

## Mathematical / Statistical Details

There is no statistical method here; the auditable content is the argument for
why every decoder's arithmetic is panic-free in a debug build, which is what the
properties confirm empirically. Every byte read already goes through
`SpdImage`'s `slice::get`, so out-of-range reads are `Truncated`, not panics. The
remaining panic risk is integer overflow on a debug build. Each arithmetic
operation in the decode paths is bounded as follows:

| Decoder | Operation | Bound |
| --- | --- | --- |
| identity | `16 + (b & 0x1f)` (rows), `10 + ((b >> 5) & 0x07)` (cols), `((b >> 3) & 0x07) + 1` (ranks) | `<= 47`, `<= 17`, `<= 8`; all fit `u8` |
| timing | `ns_units(raw) = u32::from(raw) * 1000` | `raw <= 65535`, so `<= 65_535_000 < u32::MAX` |
| timing | CAS mask shift `<< (8 * i)`, `i in 0..5` | shift amount `<= 32` on a `u64` (`< 64`) |
| timing | `read_le_u16` / `read_pair` offset `+ 1`, `+ 2` | constant offsets `<= 93` |
| module | `(b & 0x1f) + 15`, `(b & 0x0f) + 1`, `((b >> 4) & 0x0f) + 1` | `<= 46`, `<= 16`, `<= 16`; fit `u8` |
| module | reference-raw-card `code + 0x1f` (bit-7 extension) | `code` is `b & 0x1f` with `0x1f` taken early, so `code <= 0x1e`; `<= 61` |
| manufacturing | `(b & 0x7f) + 1` (bank), `(b >> 4) * 10 + (b & 0x0f)` (BCD), `2000 + bcd(b)` (year) | `<= 128`, `<= 165`, `<= 2165`; fit `u8`/`u16` |
| manufacturing | part-number slice `OFF_PART_NUMBER + PART_NUMBER_LEN`, `base + pos` | constant `521 + 30`; `pos < 30`, so `<= 550` |
| crc | `u16::from(byte) << 8`, `crc << 1` | shift amounts `8`, `1` (`< 16`); `<<` discards high bits, never overflow-panics |

Every operation stays within its integer type, so there is no debug overflow on
any input, and the properties pass with no panic found. No `checked_*` or
`saturating_*` change was needed; had a property failed, the fix would have been a
checked or saturating operation (or a validated range) plus a regression test
capturing the shrunk input, never a weakening of the property.

## Design Decisions

- **proptest in the gate, cargo-fuzz committed-but-not-run.** The split follows
  what each tool needs. proptest is pure Rust and runs anywhere `cargo test`
  does, so it is the always-on regression guard. libFuzzer needs clang/LLVM and
  does not link under Windows MSVC, so the fuzz harness is scaffolded and
  rustfmt-clean but deliberately not built here. The report states the fuzz crate
  was not run rather than implying a fuzz result.
- **A stronger assertion only where the space is finite.** The two proptest
  properties assert no-panic only, because arbitrary and mutated bytes legitimately
  produce `UnknownEnum` and `NonAscii`, which are correct outcomes, not failures.
  The truncation test asserts the stronger `Ok` or `Truncated`, because truncating
  the valid fixture can only ever shorten it, and that stronger check is cheap over
  an enumerable `0..=1024`.
- **One shared `run_all_decoders` across proptest and the fuzz target.** Both call
  the identical decoder list, so the gate and the fuzzer cannot drift apart in
  coverage. Adding a decoder in a later phase means updating one helper and one
  fuzz target.
- **Seed the corpus with the real fixture.** A fresh clone fuzzes from a valid SPD
  and mutates outward, which reaches the deep decode paths far faster than starting
  from empty input. The fixture is committed (corpus is not gitignored); it is the
  same bytes already used by the snapshot tests, not new fabricated data.
- **`fuzz/` inside the member crate, its own workspace.** Placing `fuzz/` at
  `spdr/fuzz/` is the layout `cargo fuzz` expects and makes `path = ".."` point at
  the `spdr` package. The empty `[workspace]` table keeps the root `--workspace`
  gate from ever trying to build a crate that cannot link on Windows.
- **`proptest = "1"`, `libfuzzer-sys = "0.4.13"`.** proptest follows the repo's
  existing `insta = "1"` pinning style (a dev-only gate dependency). libfuzzer-sys
  is pinned to the current crates.io release, per the brief.

## Verification

From the workspace root, all green with zero warnings, on Windows:

```
cargo build --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The new robustness tests (`tests/robustness.rs`):
`arbitrary_bytes_panics_no_decoder`, `single_byte_mutation_panics_no_decoder`,
and `every_truncation_returns_ok_or_truncated` all pass · no panic was found. The
Phase 1 through 5 unit, integration, and snapshot suites are untouched and green.

### Fuzzing (Linux / WSL, run manually by Mert)

The fuzz crate is **not built or run on Windows**: libFuzzer cannot link under
MSVC. On a Linux or WSL host with a nightly toolchain and `cargo-fuzz` installed
(`cargo install cargo-fuzz`), from the `spdr/` crate directory:

```
# Unbounded run (Ctrl-C to stop):
cargo +nightly fuzz run decode_all

# Bounded example (60 seconds):
cargo +nightly fuzz run decode_all -- -max_total_time=60

# Reproduce a crash, if libFuzzer writes one to fuzz/artifacts/decode_all/:
cargo +nightly fuzz run decode_all fuzz/artifacts/decode_all/<crash-input>
```

A crash would be a panic in a decoder; the fix is the same checked-arithmetic
discipline as above, with the artifact added as a regression test, and the valid
fixture's decode left unchanged.

### Deep-run ledger (to be filled in once a deep run is done)

| Date | Host | Command | Executions | Duration | Result |
| --- | --- | --- | --- | --- | --- |
| _pending_ | _pending_ | `cargo +nightly fuzz run decode_all -- -max_total_time=<s>` | _pending_ | _pending_ | _pending_ |

Only after a recorded deep run with no crash does the README earn the word
"fuzzed" as a completed claim.

## Related Docs

- `.claude/briefs/phase-6-robustness.md` · the brief this phase implements.
- `docs/implementations/2026-06-04-phase-1-foundation.md` · the `SpdImage`
  bounds-checked reader and the `DecodeError` contract these properties exercise.
- `docs/numerical-claims.md` and `docs/validated-against.md` · deliberately
  unchanged: robustness is a property of the code, not a per-module correctness
  claim, so no entry is forced into either ledger.
