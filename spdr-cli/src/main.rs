//! `spdr` · the read-only DDR5 SPD content decoder CLI.
//!
//! A thin wrapper over [`spdr_cli::run`]; the decode-and-render logic lives in
//! the library so it stays unit- and snapshot-testable without a subprocess.

fn main() {
    std::process::exit(spdr_cli::run());
}
