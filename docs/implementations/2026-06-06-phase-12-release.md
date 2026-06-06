# Phase 12 · v0.1.0 release (crates.io)

Date: 2026-06-06

## Problem / Motivation

Phases 1 through 11 built the decoder, the four-family linter, and the CLI. The
crate was not publishable: the version was set but the per-crate crates.io
metadata (description, keywords, categories, readme) was absent, the
`spdr-cli` dependency on `spdr` was path-only (which the registry rejects), the
fuzz sub-workspace would have been packaged, and neither published crate shipped
a README that `cargo package` would include. There was also no CHANGELOG, and a
few docs lines had gone stale or self-contradictory as later phases settled
questions earlier phases had left open.

This phase makes both crates publish-ready and reconciles the docs. It is
metadata, packaging, a CHANGELOG, and a docs pass only: no new decode, no new
lint rule, no new fixture, no behavior change, and no new correctness claim. The
real `cargo publish` and the `v0.1.0` tag are Mert's, performed after review; a
publish is irreversible per version, so it stays gated behind a green dry-run.

## What Changed

| File | Change |
| --- | --- |
| `spdr/Cargo.toml` | Added `description`, `keywords`, `categories`, `readme = "README.md"`, and `exclude = ["fuzz"]` to `[package]`. |
| `spdr-cli/Cargo.toml` | Added `description`, `keywords`, `categories`, `readme = "README.md"`; gave the `spdr` dependency both `version = "0.1.0"` and `path` (was path-only). |
| `spdr/README.md` | New crate-level README for the library (install via `cargo add spdr`, no_std / serde, scope, license). |
| `spdr-cli/README.md` | New crate-level README for the CLI (install via `cargo install spdr-cli`, binary `spdr`, the `cargo install spdr` caveat, usage, exit codes). |
| `spdr/LICENSE`, `spdr-cli/LICENSE` | Copied the root Apache-2.0 text into each crate so the published tarball carries its license. |
| `spdr-cli/src/lib.rs` | Corrected the limited-coverage note (the `render_lint_human` string and the `LintReport::base_decode_ok` doc comment) to the accurate statement. |
| `spdr-cli/tests/cli.rs` | Updated the coverage-note `.contains()` assertion to the corrected wording. |
| `README.md` (root) | Corrected the echoed coverage line; added an Install section; updated the status line to the v0.1.0 scope (UDIMM complete, the rest deferred, property-tested not fuzzed, one validated module). |
| `CHANGELOG.md` | New, Keep a Changelog style, with an honest `0.1.0` section (doubles as the GitHub release notes). |
| `docs/validated-against.md` | Dropped the stale "Phase 9b linter pass remaining" clause; reconciled the byte-233 framing to the Phase 10 view; added a short v0.1.0 release note. |
| `docs/numerical-claims.md` | Reconciled the byte-233 line to the Phase 10 view (defined `dimmAttributes` field, deliberately not flagged). |

No source decode path, no lint rule, and no snapshot changed. The corrected
coverage note is not snapshotted (it is asserted with `.contains()`), so no lint
golden was regenerated; `cargo test --workspace` stays green with every snapshot
byte-for-byte unchanged.

## Implementation Approach

### Two crates, not one

The split is deliberate and preserved: `spdr` stays `#![no_std]`, allocation-free,
and `#![forbid(unsafe_code)]` so it remains embeddable; `spdr-cli` is the `std`
wrapper. They publish as two crates. The library installs with `cargo add spdr`;
the CLI installs with `cargo install spdr-cli` and its binary is named `spdr`.
`cargo install spdr` does not work because the library ships no binary; the
READMEs say so plainly rather than implying otherwise.

The `spdr-cli` dependency on `spdr` now carries both a `version` and a `path`:
`path` resolves the local crate during development, and `version` is what the
published `spdr-cli` depends on from the registry. A path-only dependency cannot
be published.

### Metadata

Per-crate `description` lines carry the scope, because the crates.io page is
skimmed: each says what the crate is and that it is unbuffered (UDIMM) complete
with SODIMM / RDIMM / LRDIMM deferred, so neither reads as a finished, general
DDR5 SPD library. `license` (`Apache-2.0`, the SPDX expression), `repository`,
`rust-version` (1.85), and `edition` (2024) are inherited from
`[workspace.package]` and were already correct. Categories use verified crates.io
slugs: `parser-implementations`, `hardware-support`, `no-std` for the library;
`command-line-utilities`, `hardware-support` for the CLI.

### Packaging

`exclude = ["fuzz"]` keeps the cargo-fuzz sub-workspace out of the library
package. `cargo package --list -p spdr` confirms the result: a README and a
LICENSE are present, the 1 KB test fixture is present (so an auditor can run the
crate's tests), `fuzz/` is absent, and there are no stray or generated files
beyond cargo's own `Cargo.toml.orig`, `Cargo.lock`, and `.cargo_vcs_info.json`.

### The coverage-note correction

The pre-Phase-12 note read "only structure-independent checks ran," which is
inaccurate. Reading `lint::lint`, when the base/identity block fails to decode the
linter skips exactly the checks that depend on it (capacity and package/die-count
cross-field consistency) but still runs the reserved-bit rule (raw bytes,
decode-independent), and still runs the timing and vendor rules when those blocks
decode on their own. The corrected note states the accurate thing: the base
configuration did not decode, so the checks that depend on it (capacity and
cross-field consistency) were skipped, while the reserved-bit check still ran, and
a clean result is not a full bill of health. The same correction is applied to the
`LintReport::base_decode_ok` doc comment, the README line that echoes the note,
and the test assertion that pins it.

## Mathematical / Statistical Details

None. This phase changes metadata, packaging, prose, and one diagnostic string;
it introduces no formula, statistic, or decoded value. Every numeric claim in the
docs is unchanged and already logged in `docs/numerical-claims.md`.

## Design Decisions

- **Keep the crates separate.** Merging would force the CLI's `std`, `clap`, and
  `serde_json` onto every embedded consumer and break the `no_std` guarantee the
  library exists to provide. The two-crate split is the whole point, so the
  release preserves it and documents the install path for each.
- **Version-and-path dependency, not path-only.** Path-only is unpublishable; a
  bare version would not resolve locally before publish. Carrying both is the
  standard workspace-to-registry pattern and resolves in both worlds.
- **Crate-level READMEs, not a parent-path reference.** `cargo package` does not
  include files outside the package directory, so a `readme = "../README.md"`
  would not ship. Each crate gets its own README (verified present in
  `cargo package --list`); the root README stays for GitHub.
- **Ship the LICENSE in each package.** The SPDX `license` field is the binding
  declaration, but copying the Apache-2.0 text into each crate is cheap and means
  an offline auditor of the `.crate` tarball has the license in hand. The text is
  static, so the three copies never drift.
- **Correct the note in place; do not snapshot it.** The note is human guidance,
  not structured output, and it was already asserted with `.contains()` rather
  than a golden. Correcting the string, the doc comment, the README echo, and the
  assertion keeps the one source of truth honest without introducing a snapshot.
- **No new claims in a release phase.** The CHANGELOG and the docs describe only
  what the test suite and the validated-against ledger already show: UDIMM
  complete, the rest deferred, property-tested (not fuzzed), one validated module.

## Verification

From the workspace root, all green with zero warnings, on Windows:

```
cargo build --workspace
cargo build -p spdr                 # default features: core still no_std, serde-free
cargo build -p spdr --features serde
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace              # 23 CLI tests pass; no snapshot changed
```

Publish-readiness (non-mutating, no token; `--allow-dirty` only because the
working tree is uncommitted, which is Mert's step):

```
cargo package --list -p spdr --allow-dirty       # README + LICENSE present, fixture present, fuzz/ absent
cargo publish --dry-run -p spdr --allow-dirty    # packaged 29 files, compiled and verified, then aborted on dry run
```

The `spdr-cli` crate builds (in `cargo build --workspace`) and its metadata is
complete and its package contents verified (`cargo package --list -p spdr-cli`
shows README + LICENSE present). A full `cargo publish --dry-run -p spdr-cli`
cannot resolve until `spdr` is on the registry: it stops at
`no matching package named 'spdr' found`. This is expected, not a failure, and it
fixes the publish order: `spdr` is published first, then `spdr-cli`.

## Release runbook (Mert, after review)

1. Review this branch and merge to `main`.
2. From a clean `main`: `cargo publish -p spdr`.
3. After `spdr 0.1.0` is live on the index: `cargo publish -p spdr-cli`.
4. Tag the release `v0.1.0` and publish the GitHub release using the `0.1.0`
   section of `CHANGELOG.md` as the notes.

## Related Docs

- `.claude/briefs/phase-12-release.md` · the brief this phase implements.
- `CHANGELOG.md` · the `0.1.0` release summary and the deferrals.
- `docs/implementations/2026-06-06-phase-11-lint-cli.md` · the lint CLI surface
  whose coverage note this phase corrects.
- `docs/validated-against.md` · the validated-against ledger (one TEAMGROUP
  fixture) and the v0.1.0 release note.
- `docs/numerical-claims.md` · every pinned number, including the reconciled
  byte-233 line.
