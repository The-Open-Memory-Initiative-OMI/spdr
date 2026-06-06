# spdr-cli

The command-line front-end for [`spdr`](https://crates.io/crates/spdr), a
read-only DDR5 SPD content decoder plus a semantic linter that validates beyond
the CRC. It reads a saved SPD dump or a Linux sysfs `eeprom` path, prints a
readable decode, and emits the lint report. It never writes SPD and never
touches hardware.

## Install

```
cargo install spdr-cli
```

The crate is `spdr-cli`, but the installed binary is named `spdr`. Note that
`cargo install spdr` does **not** work: `spdr` is the library crate and ships no
binary. Install `spdr-cli` for the tool, and `cargo add spdr` for the library.

## Usage

```
spdr decode <file>          # human-readable decode (default)
spdr decode <file> --json   # JSON, one object keyed by section

spdr lint <file>            # human-readable lint findings (default)
spdr lint <file> --json     # JSON array of findings (empty array when clean)
```

`decode` exits 0 when the image fully decoded, 1 when at least one section failed
(for example a truncated image; the sections that decoded are still printed), and
2 when the file was unreadable or the arguments were invalid. A base CRC mismatch
is reported, not an error, and does not change the decode exit code.

`lint` exits 0 when there are no `warning` or `error` findings (clean, or only
`info` advisories), 1 when at least one `warning` or `error` is present, and 2 on
the same operational failures as `decode`. When the base configuration does not
decode, the checks that depend on it are skipped while the reserved-bit check
still runs, and the output says so, so a clean result on an unparseable file is
not mistaken for a full bill of health.

## Scope

Unbuffered (UDIMM) is complete; SODIMM, RDIMM, and LRDIMM module-specific
decoding is deferred. See the [`spdr`](https://crates.io/crates/spdr) crate and
the repository README for the full scope and the validated-against ledger.

## License

Apache-2.0.
