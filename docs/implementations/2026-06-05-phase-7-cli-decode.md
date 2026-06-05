# Phase 7 · CLI decode output

Date: 2026-06-05

## Problem / Motivation

Phases 1 through 6 built and hardened the library; nothing yet let a person point
the tool at a dump and read the result. This phase adds that first user-facing
surface: `spdr decode <file>` reads an SPD image, runs it through every library
decoder, and prints the result as human-readable text or JSON, exiting on a clean
contract.

The constraint is honesty, the same one the library holds. The CLI presents
exactly what the library decodes, with no inflated labels. The timings are shown
as the SPD JEDEC base, with a one-line note that the rated DDR5 profile lives in
XMP/EXPO and is decoded later; they are not dressed up as the box speed. The base
CRC is presented as a reported status (computed, stored, match), not a verdict
that implies more than the base CRC checks. A section that fails to decode is
shown with its error, never with fabricated output.

`lint` is reserved for Phase 11. The subcommand structure is built so adding it is
a one-variant change, but it is not added or stubbed here.

## What Changed

| File | Change |
| --- | --- |
| `spdr/Cargo.toml` | Adds an optional `serde` feature (`serde = ["dep:serde"]`) and `serde = { version = "1", optional = true, default-features = false, features = ["derive"] }`. Off by default. |
| `spdr/src/identity.rs` | Gated `#[cfg_attr(feature = "serde", derive(serde::Serialize))]` on the nine public identity types. |
| `spdr/src/timing.rs` | Gated derive on `Picoseconds`, `ClockCycles`, `TimingPair`, `Timings`; a hand-written `Serialize` for `CasLatencies` (the ascending CL list, not the raw mask). |
| `spdr/src/module.rs` | Gated derive on `Millimeters`, `ReferenceRawCard`, `UnbufferedModule`, `ModuleSpecific`. |
| `spdr/src/manufacturing.rs` | Gated derive on `ManufacturerId`, `ManufacturingDate`, `Manufacturing`; a hand-written `Serialize` for `SerialNumber` (the eight-hex-digit string). |
| `spdr/src/crc.rs` | Gated derive on `CrcStatus`. |
| `spdr-cli/Cargo.toml` | Binary renamed to `spdr`; depends on `spdr` (with `serde`), `clap` (derive), `serde_json`; dev-deps `insta`, `assert_cmd`, `tempfile`, `proptest`. |
| `spdr-cli/src/lib.rs` | New: the clap `Cli`/`Commands`/`DecodeArgs`, the `decode` pipeline and `DecodeResults`, the pure `render_human`/`render_json`, and `run`. |
| `spdr-cli/src/main.rs` | Thin wrapper: `std::process::exit(spdr_cli::run())`. |
| `spdr-cli/tests/cli.rs` | New: the two render snapshots, the JSON-parses-back test, the exit-code e2e tests, and the render-robustness proptest. |
| `spdr-cli/tests/snapshots/cli__render_human_snapshot.snap` | Accepted human-format golden. |
| `spdr-cli/tests/snapshots/cli__render_json_snapshot.snap` | Accepted JSON-format golden. |
| `.github/workflows/ci.yml` | Adds a `cargo build -p spdr` step guarding the serde-free no_std build. |
| `README.md` | Adds a Usage section. |

The Phase 1 through 6 source, suites, and snapshots are unchanged. Adding a gated
`Serialize` derive does not affect `Debug`, so every earlier snapshot stays
byte-for-byte green.

## Implementation Approach

### Core crate · optional `serde` feature, Serialize only

The tool is read-only, so the core gains the ability to serialize decoded data
out, never to parse it in. A `serde` feature, off by default, adds
`#[derive(serde::Serialize)]` to every public decoded type through
`#[cfg_attr(feature = "serde", derive(serde::Serialize))]`. No `Deserialize` is
derived anywhere. `serde` is pulled with `default-features = false`, which keeps
the Serialize path `no_std`-compatible; the crate keeps `#![no_std]` and
`#![forbid(unsafe_code)]`, and the default build pulls in no serde at all. CI
guards this with a dedicated `cargo build -p spdr` step, because the workspace
build always enables the feature through the CLI.

Two types get a hand-written impl instead of the default derive, because the
default would be clearly misleading (the only exception the brief allows):

- `CasLatencies` wraps a 40-bit mask; the default derive would emit that mask as
  one meaningless integer. The hand-written impl serializes the ascending list of
  supported CL values (`[22, 24, ... , 40]`), matching its `Debug`. It uses
  `Serializer::collect_seq`, which is `alloc`-free.
- `SerialNumber` wraps a `u32`; the default would emit a decimal (`17034998`)
  where every other surface (its `Display`, `Debug`, the published reference, the
  human report) shows the eight-hex-digit form `0104EEF6`. The hand-written impl
  writes the eight ASCII hex digits into a fixed stack buffer and serializes the
  string, `no_std` and `alloc`-free and panic-free (no `unsafe`).

Everything else uses the default derive. Enums serialize as their variant names
(`"Udimm"`, `"Gb16"`), which is faithful; small raw codes (a manufacturer
`code`, the CRC `computed`/`stored`) serialize as integers, which is faithful and
not misleading, so they are left as the default to keep the serde surface minimal.

### CLI crate · `spdr decode`

The crate is `spdr-cli`; the binary is `spdr` (`[[bin]] name = "spdr"`). Logic
lives in `src/lib.rs` so the renderers are pure and snapshot-testable and the
pipeline can be property-tested without a subprocess; `src/main.rs` is just
`std::process::exit(spdr_cli::run())`.

clap structure: a `Cli` with `#[command(name = "spdr", version)]` and a
`Commands` subcommand enum holding `Decode(DecodeArgs { file: PathBuf, --json })`.
The single-variant enum is the seam for `Lint` in Phase 11.

`decode(bytes) -> DecodeResults` runs every library decoder (identity and base,
base CRC, timings, module-specific, manufacturing) and holds the five per-section
`Result`s. `DecodeResults::all_decoded()` is true only when every section is `Ok`;
a CRC mismatch is itself an `Ok` (a reported status) and does not make it false.

Rendering is two pure functions:

- `render_human(&DecodeResults) -> String`: sectioned, aligned `key: value` plain
  text. Each section is a `[Header]` followed by aligned lines, or, on failure, an
  `error:` line. The timings section carries the JEDEC-base note; the CRC section
  carries the "reported status, not a verdict" note. It never panics: no indexing,
  no `unwrap` on decoded data.
- `render_json(&DecodeResults) -> Result<String, serde_json::Error>`: a
  CLI-assembled JSON object keyed by section. Each value is the serde-serialized
  library type; a failed section carries `{ "error": <message> }`, so the object
  always has all five keys and stays complete and valid JSON. The per-section
  value is built by a small macro (not a generic fn) so the section types stay
  concrete and the CLI depends only on `serde_json`, not `serde` directly.

### Exit-code contract

- `0`: fully decoded, every section returned a value. A CRC mismatch is reported,
  not an error, and does not change the code (integrity is the linter's job in
  Phase 11).
- `1`: ran, but at least one section returned a decode error (for example a
  truncated image). The report, the sections that decoded plus the per-section
  errors, is printed to stdout first, then the process exits 1.
- `2`: could not run; the file was unreadable. clap already maps invalid arguments
  to the same code, so usage errors and file errors share exit 2.

`lint` will define its own exit codes in Phase 11 and, as a separate subcommand,
will not collide with these.

## Representation notes

There is no new numeric algorithm here; the CLI presents values the library
already decoded and validated, so there is nothing new to audit mathematically.
The only representation decisions are the two serde impls above (CL list, hex
serial) and the human-format unit choice: absolute-time timings are shown in their
canonical stored unit, picoseconds (for example `tAA: 16640 ps`), rather than a
derived nanosecond figure, so the output matches the library's decoded value
exactly rather than introducing a conversion the library does not perform.

## Design Decisions

- **Serialize only, no Deserialize.** The tool never parses structured input back
  in; it only reads raw SPD bytes and writes decoded data out. Deriving
  `Deserialize` would add surface with no use and invite treating decoded output
  as a round-trippable format, which it is not.
- **Feature off by default, guarded in CI.** The core must stay embeddable in
  firmware/UEFI, so serde is opt-in and the default build is serde-free and
  no_std. Because the workspace build enables the feature through the CLI, a
  dedicated `cargo build -p spdr` CI step keeps the serde-free build honest.
- **Logic in a lib, binary is a wrapper.** Putting `render_human`/`render_json`
  and `decode` in `spdr-cli`'s library makes the format snapshot-testable and the
  pipeline property-testable directly, with the subprocess reserved for the
  exit-code contract only.
- **JSON assembled by the CLI, not the library.** The library serializes each
  decoded type; the CLI owns the section keys and the failed-section error
  indicator. This keeps the core's serde surface minimal and the "object always
  valid, all keys present" guarantee in the layer that renders.
- **Hand-written serde only where the default misleads.** `CasLatencies` (raw
  mask) and `SerialNumber` (decimal vs hex) are the two cases; everything else
  uses the default derive, keeping the core's serde surface small.
- **Honest labeling.** The timings heading is "JEDEC base timings" with the
  XMP/EXPO note, and the CRC is a reported status, not a pass/fail verdict, so the
  output never claims more than the base content the library actually decodes.

## Verification

From the workspace root, all green with zero warnings, on Windows:

```
cargo build --workspace
cargo build -p spdr                 # default features: core still no_std, serde-free
cargo build -p spdr --features serde  # the feature builds and keeps no_std
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Phase 7 tests (the Phase 1 through 6 suites still pass): two render snapshots over
the real fixture (`render_human`, `render_json`), a test that `render_json` parses
back as valid JSON, three assert_cmd e2e tests for the exit contract (fixture
exits 0; a nonexistent path exits 2 with a stderr message; a 250-byte truncated
temp file exits 1 with partial output), and the render-robustness proptest
(decode then `render_human` then `render_json` over arbitrary `Vec<u8>` of length
0..=2048, asserting no panic).

The render-robustness proptest passed with no panic found. It is the render-side
counterpart to Phase 6's decode-only properties, covering the error-rendering
paths the decode-only properties did not reach.

## Related Docs

- `.claude/briefs/phase-7-cli-decode.md` · the brief this phase implements.
- `docs/implementations/2026-06-05-phase-6-robustness.md` · the no-panic decode
  contract this phase's render-robustness proptest extends to the render side.
- `docs/numerical-claims.md` and `docs/validated-against.md` · deliberately
  unchanged: the CLI presents the already-validated decode, it adds no new
  correctness claim about the fixture.
- `README.md` · the Usage section added in this phase.
```
