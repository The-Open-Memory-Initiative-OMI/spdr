# Quality pass · narrowing, `const`, and the doc gate

A behaviour-preserving pass over the shipped surface, dated 2026-10-25 and
reviewed upstream in PR #1. No decoded output changed and every golden snapshot
passes byte for byte. Three things are worth recording here because each is
meaningful on its own: a CI gate that did not exist, a workspace lint policy,
and a numeric-behaviour change.

Every number quoted here is traced in `docs/numerical-claims.md`.

## Problem / Motivation

The core crate is `#![forbid(unsafe_code)]` and never panics, so its
interesting failure mode is not a crash but a **wrong value**. Two failure
classes were open:

1. **Silent narrowing.** A wider integer was narrowed to a narrower one with an
   unchecked `as`. For decoded bytes every such quotient fits, so the casts
   were safe in practice and fragile in principle: the invariant lived in a
   comment instead of in the code, and a hand-built out-of-range value could
   wrap to a plausible-looking number.
2. **Invisible doc breakage.** `cargo doc` was not run anywhere, so rustdoc
   warnings could not fail a build. On the base `6b0d2d5` the doc build emits
   22 warnings: 21 unresolved bit-range links in `spdr` and one public doc link
   to a private item in `spdr-cli`.

## What Changed

### The CI gate

CI now builds the documentation with warnings as errors:

```yaml
- name: Docs
  run: RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --lib
```

`--lib` keeps the doc set to the two library APIs and dodges the known
output-filename collision between the `spdr` library and the `spdr` binary
(cargo#6313).

### The workspace lint policy

`[workspace.lints.clippy]` denies a curated set from `clippy::pedantic`:
`cast_possible_truncation`, `missing_errors_doc`, `doc_markdown`,
`ref_option`, `needless_pass_by_value` and `trivially_copy_pass_by_ref`.
Curated rather than blanket, so that a future upstream pedantic lint cannot
break CI on a toolchain bump. `bool_to_int_with_if` was dropped from the set
during this pass; see below.

Two facts about that set were measured rather than assumed, and both corrected
claims made earlier in the review:

- `ref_option` is **not** newer than the MSRV. Clippy 1.85.1 reports it as
  implied by `clippy::pedantic`, and an invented lint name on the same
  toolchain gives `error[E0602]: unknown lint`, so the name is resolved rather
  than ignored.
- `bool_to_int_with_if` fires on the *readable* branch form,
  `if ok { 0 } else { 1 }`, and asks for `i32::from(!ok)` instead. It is a
  style guard with no bearing on a wrong value, and its only effect here was
  to force a less readable exit-code expression, so it was dropped from the
  workspace deny list and the branch form kept.

### The numeric-behaviour change

Two CAS-latency derivations narrowed a `u32` quotient to `u16`. They now
narrow explicitly and saturate, so an out-of-range hand-built value cannot
wrap. The wrap case was a plausible latency: 65541 cycles came out as CL5,
which would have been reported as a real value.

The rated-CL derivation computes its round-to-nearest in `u64` rather than in
`u32`, so the rounding stays exact across the whole `u32` input range instead
of saturating the add and rounding down. Only the final narrowing to `u16`
saturates.

The supported-CAS mask arithmetic now computes its bit index in a single
integer domain, with a compile-time assertion tying the bit count to the byte
count it derives from. The reference-raw-card `Display` no longer casts the
alphabet length or the array indices.

### `const` and the public API

31 pure decoders and accessors are now `const fn` (27 in `spdr`, 4 in
`spdr-cli`). Every one of them is a crate-internal helper or an accessor: no
public `decode_*` entry point is `const`, so a downstream user still cannot
decode an SPD image at compile time.

Six public items did gain `const`, which is additive and therefore a semver
commitment: `Finding::severity`, `Finding::code`, `SpdImage::is_empty`,
`CasLatencies::contains` and `Timings::base_data_rate_mt_s` in `spdr`, plus
`DecodeResults::all_decoded` in `spdr-cli`. Removing `const` later would be
breaking.

Two crate-internal helpers had their visibility widened from `pub(crate)` to
`pub` during the pass. That was reverted: the compiler enforced the boundary,
a comment does not, and a future `pub use identity::*` would otherwise export
them silently. `const` was kept.

### Documentation completeness

Every public `Result`-returning function has an explicit `# Errors` section,
and identifiers in prose are backticked. Bit-range notation in doc comments
is written as a code span (`[7:5]`) rather than an escape, which rustdoc does
not turn into an intra-doc link and which reads cleanly in a crate whose source
is meant to be the open reference.

## Design Decisions

- **Curated lints, not blanket `pedantic`.** A blanket set means any new
  upstream lint breaks CI on a toolchain bump, which is a maintenance tax the
  project does not need. The curated set names the classes this pass actually
  found.
- **Saturation, not wrapping.** A narrow cast that saturates is wrong on
  out-of-range input; a narrow cast that wraps is wrong *and* plausible. When
  the value cannot fit, reporting the maximum is the honest answer.
- **`u64` for the intermediate.** Widening the add and the divide costs
  nothing on a `no_std` target and removes a rounding error that only shows up
  near the top of the `u32` range.
- **The bit-domain test is a characterization test.** The base arithmetic was
  already correct, so the test pins the behaviour rather than catching a
  regression. The two saturation tests are genuine regression tests: both were
  confirmed to fail against the previous `as` cast.
- **The README wording is unchanged.** It still says *property-tested, not
  fuzzed*. The deep run is still owed on a Linux or WSL host; on Windows the
  harness never reached a single execution. The on-box substitute is
  `PROPTEST_CASES=100000 cargo test --release --test robustness`, which is
  property-testing, not fuzzing.

## Verification

- `cargo fmt --check` · clean.
- `cargo clippy --workspace --all-targets -- -D warnings` · clean.
- `cargo clippy --workspace --all-targets -- -W clippy::pedantic` · one
  warning: `bool_to_int_with_if` on the restored `if ok { 0 } else { 1 }` exit
  code. The lint is not in the deny set and the CI gate does not enable blanket
  `pedantic`, so the `-D warnings` gate stays clean. Measured on Windows and
  Linux; traced in `docs/numerical-claims.md`. The zero-warning result belongs
  to the pre-review head, where the exit code was `i32::from(!ok)`.
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --lib` · clean.
- `cargo build -p spdr` · clean, the existing `no_std` guard.
- `cargo test --workspace` · the full suite passing.
- MSRV 1.85.1: `cargo build -p spdr`, the clippy gate, and the full test suite
  all clean.

The rendered documentation was correct before this pass and is correct after it.
What changed is that the warnings are gone and the build now fails on them.

## Related Docs

- `docs/numerical-claims.md` · every number quoted above, with its source.
- `docs/implementations/2026-06-05-phase-6-robustness.md` · the fuzz harness
  and its measured Windows failure mode, with the 2026-10-16 addendum.
- `CHANGELOG.md` · the user-facing summary of the same change.
