# Phase 11 · Lint CLI surface and exit codes

Date: 2026-06-06

## Problem / Motivation

Phases 8 through 10 built the semantic linter in the core crate: the framework, the
capacity rule, the timing and speed-bin rules, and the reserved-bit and consistency
rules. It had no user-facing surface. This phase adds one: the `spdr lint`
subcommand, with human and JSON rendering and an exit-code contract.

It is surface work only. No new decode, no new lint rule, no new correctness claim;
the core `lint` API and every rule stay exactly as they are. This phase renders
their output and defines the lint command's exit codes. The decode path is
untouched: its goldens, its exit codes, and the decoder and lint-core test suites
are unchanged.

## What Changed

| File | Change |
| --- | --- |
| `spdr-cli/src/lib.rs` | New `Commands::Lint(LintArgs)`, the `LintReport` type, `lint_report`, `lint_exit_code`, `run_lint`, and the pure renderers `render_lint_human` / `render_lint_json` (plus small helpers). Imports `Finding`, `Severity`, `lint`. The `run` dispatch gains a `Lint` arm. |
| `spdr-cli/tests/cli.rs` | Lint goldens (clean and with-findings, human and JSON), the per-severity exit-code tests, crafted-image tests for each severity, the findings-order test, a JSON-shape test, the lint render-robustness proptest, and the subprocess exit-code tests. |
| `spdr-cli/tests/snapshots/cli__lint_*.snap` | Four new goldens (clean/with-findings x human/JSON). |
| `README.md` | Documents `spdr lint` and its exit codes; retires the stale "linter is a later phase" status line; keeps scope honest (UDIMM-complete, SODIMM/RDIMM/LRDIMM deferred). |
| `docs/validated-against.md` | A Phase 11 note: the fixture lints clean through the CLI (exit 0). |

No core crate file changed; the decode renderers, the decode goldens, and the
decode exit-code tests are untouched. `docs/numerical-claims.md` is unchanged: no
new decoded number.

## Implementation Approach

### The exit-code contract

`spdr lint` uses its own exit codes, parallel in spirit to `spdr decode` (0 good,
2 operational), with code 1 redefined for the linter:

- **0**: the lint ran and produced no `Warning` or `Error` finding (the SPD is
  clean, or has only `Info`-level advisory observations).
- **1**: the lint ran and produced at least one `Warning` or `Error` finding.
- **2**: the lint could not run: the file is unreadable or the arguments are
  invalid (the same operational meaning as decode's 2).

`Info` is the advisory tier by design (a non-standard but legitimate data rate is
`Info`), so it does not fail the exit code; it is still printed. Severity detail
lives in the output, not the code. A severity-threshold flag (escalate warnings,
or suppress them) is deferred past v0.1.0.

The whole rule is one small, tested function:

```rust
pub fn lint_exit_code(findings: &[Finding]) -> i32 {
    let actionable = findings.iter()
        .any(|f| matches!(f.severity(), Severity::Error | Severity::Warning));
    i32::from(actionable)
}
```

The operational `2` is handled in `run_lint` (the file read) and by clap (bad
arguments), never in this function.

### The limited-coverage note

The reserved-bit rule reads raw bytes and runs regardless of whether anything
decoded; the other rules need a successful decode. So when the base configuration
does not decode, only the structure-independent checks ran, and a no-findings
result is not a full clean bill. `LintReport` records `base_decode_ok` (from
`decode_identity_and_base(bytes).is_ok()`), and the human renderer prints a note
when it is false:

```
[Lint]
  Note: the base configuration did not decode, so only structure-independent checks ran; a clean result here is not a full bill of health.
  ...
```

The exit code still follows the contract above; the note is human guidance, not a
code change.

### Rendering, pure and golden-tested

`lint_report(bytes) -> LintReport` runs the core `lint` once, collecting findings
into a `Vec` and ordering them deterministically (by severity, errors first, then
by code) so the goldens are stable. The renderers are pure (`&LintReport -> String`)
and golden-tested without spawning the process, exactly as the decode renderers are:

- **Human**: the `[Lint]` header, the optional limited-coverage note, a summary
  line (`"2 findings: 1 error, 1 warning."` or a clean message), then one block per
  finding (`severity · code`, then the message on the next line).
- **JSON**: a valid document, an empty array when there are no findings. Each
  finding is an object with its lowercase `severity`, its stable `code`, a human
  `message` (the framework's `Display`), and its structured `fields`. The fields
  reuse the core `Finding`'s gated serde derive, unwrapped from the externally
  tagged `{"Variant": {fields}}` to just the inner `{fields}` (the `code` already
  names the rule). The core is unchanged; the severity/code/message wrapper is
  added CLI-side.

`run_lint` reuses the Phase 7 file-read front-end (unreadable file is exit 2),
runs `lint_report`, renders, prints, and returns `lint_exit_code`.

## Mathematical / Statistical Details

None. This phase renders existing findings and maps them to an exit code; it
introduces no formula, statistic, or decoded value. The only computation is the
severity reduction above (any error-or-warning to 1, else 0) and a deterministic
sort key `(severity_rank, code)`.

## Design Decisions

- **Code 1 redefined, 0 and 2 kept decode-consistent.** A user already knows
  decode's 0 (good) and 2 (operational). Lint keeps those meanings and gives 1 the
  linter's natural sense (actionable findings present), so the two subcommands read
  consistently without colliding.
- **Info never fails the build.** The advisory tier exists precisely so a
  legitimate-but-non-standard observation (a custom vendor data rate) does not turn
  a CI gate red. Info is printed but does not reach code 1.
- **The limited-coverage note, not a code change.** An unparseable file that
  happens to have zero reserved-bit findings must not read as a clean bill. The
  honest fix is a human note plus the unchanged code, not a special exit code that
  would complicate the contract.
- **Pure renderers, like decode.** Keeping `render_lint_human` / `render_lint_json`
  pure (findings in, String out) lets the goldens test the exact output without a
  subprocess, and keeps the subprocess tests to what only they can check (the real
  exit code and the file front-end).
- **JSON fields reuse the core derive, unwrapped.** Rather than hand-write each
  finding's fields CLI-side (which would drift from the core) or duplicate the
  variant tag (which the `code` already encodes), the renderer serializes the
  `Finding` and lifts out the inner field object. The core stays serde-gated and
  unchanged.
- **Deterministic order.** Errors first, then warnings, then info, then by code,
  via a stable sort, so the goldens never flake and a human reads the worst first.

## Verification

From the workspace root, all green with zero warnings, on Windows:

```
cargo build --workspace
cargo build -p spdr                 # default features: core still no_std, serde-free
cargo build -p spdr --features serde
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The JSON path uses serde, so the CLI enables `spdr/serde` (as the decode JSON
does); the core stays serde-free by default, guarded by the `cargo build -p spdr`
step.

Phase 11 tests (the decode goldens and exit codes, and the decoder and lint-core
suites, are untouched):

- Goldens: `lint_human_clean_snapshot` and `lint_json_clean_snapshot` on the
  fixture (no findings, the JSON is `[]`), plus `lint_human_with_findings_snapshot`
  and `lint_json_with_findings_snapshot` (one finding of each severity) to lock the
  with-findings shape.
- Exit code per arm: `lint_exit_code_per_severity` pins clean to 0, Info-only to 0,
  Warning to 1, Error to 1. The subprocess tests pin the operational arms: the
  fixture exits 0, a missing-argument invocation and an unreadable path exit 2, and
  a reserved-byte-mutated file exits 1.
- Crafted-image tests through the real pipeline: a reserved-region mutation
  (`Warning` to exit 1), a broken-tRC mutation (`Error` to exit 1), and a
  non-standard-rate timing image (`Info`-only to exit 0, with the limited-coverage
  note). `lint_findings_sorted_errors_first` confirms the order.
- A JSON-validity / shape check, and the lint render-robustness proptest over
  arbitrary bytes (no panic), mirroring the decode render property.

## Related Docs

- `.claude/briefs/phase-11-lint-cli.md` · the brief this phase implements.
- `docs/implementations/2026-06-05-phase-7-cli-decode.md` · the decode CLI surface
  this phase mirrors (the subcommand shape, the `--json` switch, the file
  front-end, and the pure-renderer/golden pattern).
- `docs/implementations/2026-06-05-phase-8-linter-capacity.md` · the linter
  framework and the `Finding` / `Severity` types this surface renders.
- `docs/implementations/2026-06-06-phase-10-reserved-consistency.md` · the final
  rule families; the reserved-bit rule is the structure-independent check behind
  the limited-coverage note.
- `docs/validated-against.md` · the fixture lints clean through the CLI (exit 0).
