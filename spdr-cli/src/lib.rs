//! `spdr-cli` library · the decode-and-render core behind the `spdr` binary.
//!
//! Split out from `main.rs` so [`render_human`] and [`render_json`] are pure,
//! snapshot-testable functions and the decode pipeline can be property-tested
//! without spawning a subprocess. The binary is a thin wrapper over [`run`].
//!
//! The CLI presents exactly what the library decodes, with no inflated labels:
//! the JEDEC base timings are the guaranteed fallback, and the rated DDR5 speed
//! is shown separately in the vendor-profiles section (XMP 3.0 / EXPO), each
//! anchored by its section CRC. The base CRC is presented as a reported status,
//! not a verdict. A section that fails to decode is shown with its error rather
//! than fabricated output.
//!
//! Alongside `decode`, the `lint` subcommand runs the semantic linter (the
//! validation-beyond-CRC pillar) and renders its findings, with its own
//! exit-code contract: `0` when clean or only `Info` advisories, `1` when a
//! `Warning` or `Error` finding is present, `2` when the file could not be read.
//! Its renderers ([`render_lint_human`], [`render_lint_json`]) are pure like the
//! decode renderers, so they are golden-tested without spawning the process.

use std::fmt::Write as _;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use serde_json::Value;
use spdr::{
    CasLatencies, CrcStatus, DecodeError, Expo, ExpoProfile, Finding, IdentityAndBase,
    Manufacturing, ModuleSpecific, Picoseconds, RatedTimings, Severity, Timings, VendorProfiles,
    Xmp, XmpProfile, decode_identity_and_base, decode_manufacturing, decode_module_specific,
    decode_timings, decode_vendor_profiles, lint, verify_base_crc,
};

/// The `spdr` command-line interface.
#[derive(Parser)]
#[command(name = "spdr", version, about = "Read-only DDR5 SPD content decoder")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

/// The `spdr` subcommands. `decode` prints the typed decode; `lint` runs the
/// semantic linter and reports its findings. Each subcommand defines its own
/// exit-code contract, so the two do not collide.
#[derive(Subcommand)]
pub enum Commands {
    /// Decode an SPD image and print it as human-readable text or JSON.
    Decode(DecodeArgs),
    /// Lint an SPD image for internal inconsistencies and print the findings.
    Lint(LintArgs),
}

/// Arguments to `spdr decode`.
#[derive(Args)]
pub struct DecodeArgs {
    /// Path to a raw 1024-byte SPD image (a dump file or a Linux sysfs `eeprom`).
    pub file: PathBuf,
    /// Emit JSON instead of human-readable text.
    #[arg(long)]
    pub json: bool,
}

/// Arguments to `spdr lint`. Mirrors [`DecodeArgs`]: the same file input and the
/// same `--json` switch.
#[derive(Args)]
pub struct LintArgs {
    /// Path to a raw 1024-byte SPD image (a dump file or a Linux sysfs `eeprom`).
    pub file: PathBuf,
    /// Emit JSON instead of human-readable text.
    #[arg(long)]
    pub json: bool,
}

/// The per-section decode results, each section independently `Ok` or a typed
/// [`DecodeError`]. Borrows the input image for the manufacturing part number.
pub struct DecodeResults<'a> {
    /// Identity and base SDRAM configuration.
    pub identity: Result<IdentityAndBase, DecodeError>,
    /// Base configuration CRC status (reported, never a verdict).
    pub crc: Result<CrcStatus, DecodeError>,
    /// Base JEDEC timing block.
    pub timings: Result<Timings, DecodeError>,
    /// Module-specific block.
    pub module: Result<ModuleSpecific, DecodeError>,
    /// Manufacturing information block.
    pub manufacturing: Result<Manufacturing<'a>, DecodeError>,
    /// Vendor overclocking profiles (XMP 3.0 and EXPO). An absent region is a
    /// successful decode, not an error.
    pub vendor: Result<VendorProfiles<'a>, DecodeError>,
}

impl DecodeResults<'_> {
    /// Whether every section decoded. Drives exit code 0 versus 1. A CRC mismatch
    /// is itself a successful decode (`Ok`) and does not make this `false`;
    /// integrity and consistency are the linter's job (the `lint` subcommand).
    #[must_use]
    pub fn all_decoded(&self) -> bool {
        self.identity.is_ok()
            && self.crc.is_ok()
            && self.timings.is_ok()
            && self.module.is_ok()
            && self.manufacturing.is_ok()
            && self.vendor.is_ok()
    }
}

/// Run every library decoder over `bytes`, holding the per-section results. No
/// decoder can panic on malformed input (Phase 6), so this never panics.
#[must_use]
pub fn decode(bytes: &[u8]) -> DecodeResults<'_> {
    DecodeResults {
        identity: decode_identity_and_base(bytes),
        crc: verify_base_crc(bytes),
        timings: decode_timings(bytes),
        module: decode_module_specific(bytes),
        manufacturing: decode_manufacturing(bytes),
        vendor: decode_vendor_profiles(bytes),
    }
}

/// Parse arguments and run the chosen subcommand, returning the process exit
/// code. clap handles `--help`/`--version` (exit 0) and usage errors (exit 2)
/// itself before this returns.
#[must_use]
pub fn run() -> i32 {
    let cli = Cli::parse();
    match cli.command {
        Commands::Decode(args) => run_decode(&args),
        Commands::Lint(args) => run_lint(&args),
    }
}

/// The `decode` flow and its exit-code contract:
/// - `0`: every section decoded (a CRC mismatch is reported, not an error).
/// - `1`: ran, but at least one section returned a decode error; the report
///   (decoded sections plus the per-section errors) is printed to stdout first.
/// - `2`: could not run; the file was unreadable. clap already maps invalid
///   arguments to the same code.
fn run_decode(args: &DecodeArgs) -> i32 {
    let bytes = match std::fs::read(&args.file) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("spdr: cannot read {}: {error}", args.file.display());
            return 2;
        }
    };

    let results = decode(&bytes);

    let rendered = if args.json {
        match render_json(&results) {
            Ok(json) => json,
            Err(error) => {
                eprintln!("spdr: failed to render JSON: {error}");
                return 2;
            }
        }
    } else {
        render_human(&results)
    };
    println!("{rendered}");

    if results.all_decoded() { 0 } else { 1 }
}

// --- Lint surface ----------------------------------------------------------

/// The collected lint findings for an image, plus whether the base configuration
/// decoded. Mirrors [`DecodeResults`] for the lint path: produced once by
/// [`lint_report`], rendered by the pure render functions, and used to compute the
/// exit code.
pub struct LintReport {
    /// The findings, ordered deterministically (errors first, then by code).
    pub findings: Vec<Finding>,
    /// Whether the base configuration block decoded. When false, the checks that
    /// depend on it (capacity and cross-field consistency) were skipped, while the
    /// reserved-bit check still ran; a no-findings result is then not a full clean
    /// bill, and the human output says so.
    pub base_decode_ok: bool,
}

/// Run the core linter over `bytes`, collecting and ordering the findings. Pure
/// and panic-free (the core lint never panics). Findings are ordered by severity
/// (errors first) then by code, so the renders are deterministic.
#[must_use]
pub fn lint_report(bytes: &[u8]) -> LintReport {
    let mut findings = Vec::new();
    lint(bytes, &mut |finding| findings.push(finding));
    findings.sort_by(|a, b| {
        severity_rank(a.severity())
            .cmp(&severity_rank(b.severity()))
            .then_with(|| a.code().cmp(b.code()))
    });
    LintReport {
        base_decode_ok: decode_identity_and_base(bytes).is_ok(),
        findings,
    }
}

/// The lint exit code computed from the findings: `1` if any `Error` or `Warning`
/// is present, `0` if there are none or only `Info` advisories. The operational
/// `2` (unreadable file or bad arguments) is handled in [`run_lint`] and by clap,
/// never here.
#[must_use]
pub fn lint_exit_code(findings: &[Finding]) -> i32 {
    let actionable = findings
        .iter()
        .any(|finding| matches!(finding.severity(), Severity::Error | Severity::Warning));
    i32::from(actionable)
}

/// The `lint` flow and its exit-code contract:
/// - `0`: lint ran, no `Warning` or `Error` finding (clean, or only `Info`).
/// - `1`: lint ran, at least one `Warning` or `Error` finding.
/// - `2`: could not run; the file was unreadable. clap maps invalid arguments to
///   the same code.
fn run_lint(args: &LintArgs) -> i32 {
    let bytes = match std::fs::read(&args.file) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("spdr: cannot read {}: {error}", args.file.display());
            return 2;
        }
    };

    let report = lint_report(&bytes);

    let rendered = if args.json {
        match render_lint_json(&report) {
            Ok(json) => json,
            Err(error) => {
                eprintln!("spdr: failed to render JSON: {error}");
                return 2;
            }
        }
    } else {
        render_lint_human(&report)
    };
    println!("{rendered}");

    lint_exit_code(&report.findings)
}

/// The severity sort rank: errors first, then warnings, then info.
fn severity_rank(severity: Severity) -> u8 {
    match severity {
        Severity::Error => 0,
        Severity::Warning => 1,
        Severity::Info => 2,
    }
}

/// The lowercase severity label used in both lint renders.
fn severity_label(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
    }
}

/// Render the lint report as readable text: an optional limited-coverage note, a
/// summary line, then one block per finding (severity and code, then the
/// message). Pure and allocation-bounded; never panics.
#[must_use]
pub fn render_lint_human(report: &LintReport) -> String {
    let mut out = String::new();
    out.push_str("[Lint]\n");

    if !report.base_decode_ok {
        out.push_str(
            "  Note: the base configuration did not decode, so the checks that depend on it (capacity and cross-field consistency) were skipped, while the reserved-bit check still ran; a clean result here is not a full bill of health.\n",
        );
    }

    if report.findings.is_empty() {
        out.push_str(
            "  No findings. The SPD is internally consistent under the current rule set.\n",
        );
        return out;
    }

    let _ = writeln!(out, "  {}", lint_summary(&report.findings));
    for finding in &report.findings {
        let _ = writeln!(
            out,
            "  {} · {}",
            severity_label(finding.severity()),
            finding.code()
        );
        let _ = writeln!(out, "    {finding}");
    }
    out
}

/// Build the summary line, for example `"2 findings: 1 error, 1 warning."`.
fn lint_summary(findings: &[Finding]) -> String {
    let (errors, warnings, infos) = severity_counts(findings);
    let total = findings.len();
    let noun = if total == 1 { "finding" } else { "findings" };

    let mut parts = Vec::new();
    if errors > 0 {
        parts.push(count_phrase(errors, "error"));
    }
    if warnings > 0 {
        parts.push(count_phrase(warnings, "warning"));
    }
    if infos > 0 {
        parts.push(count_phrase(infos, "info"));
    }
    format!("{total} {noun}: {}.", parts.join(", "))
}

/// `(errors, warnings, infos)` counts over the findings.
fn severity_counts(findings: &[Finding]) -> (usize, usize, usize) {
    let mut errors = 0;
    let mut warnings = 0;
    let mut infos = 0;
    for finding in findings {
        match finding.severity() {
            Severity::Error => errors += 1,
            Severity::Warning => warnings += 1,
            Severity::Info => infos += 1,
        }
    }
    (errors, warnings, infos)
}

/// `"1 error"` / `"2 errors"`; "info" is left invariant (it reads as a mass noun).
fn count_phrase(n: usize, label: &str) -> String {
    if n == 1 || label == "info" {
        format!("{n} {label}")
    } else {
        format!("{n} {label}s")
    }
}

/// Render the lint report as a JSON array of findings (an empty array when there
/// are none). Each finding carries its lowercase `severity`, its stable `code`, a
/// human `message`, and its structured `fields` (the core `Finding`'s serde
/// representation, unwrapped to just the variant's fields, since `code` already
/// names the rule).
///
/// # Errors
/// Returns a [`serde_json::Error`] only if serializing a finding fails, which the
/// finding types do not do in practice.
pub fn render_lint_json(report: &LintReport) -> Result<String, serde_json::Error> {
    let findings = report
        .findings
        .iter()
        .map(finding_to_json)
        .collect::<Result<Vec<_>, _>>()?;
    serde_json::to_string_pretty(&Value::Array(findings))
}

/// Build one finding's JSON object.
fn finding_to_json(finding: &Finding) -> Result<Value, serde_json::Error> {
    // The core derives an externally-tagged enum, `{"Variant": {fields}}`; unwrap
    // it to just the fields, since the stable `code` already identifies the rule.
    let fields = match serde_json::to_value(finding)? {
        Value::Object(map) if map.len() == 1 => map
            .into_iter()
            .next()
            .map_or(Value::Null, |(_, value)| value),
        other => other,
    };
    Ok(serde_json::json!({
        "severity": severity_label(finding.severity()),
        "code": finding.code(),
        "message": finding.to_string(),
        "fields": fields,
    }))
}

// --- Human rendering -------------------------------------------------------

/// Indent and column width for the aligned `key: value` lines.
const LABEL_WIDTH: usize = 30;

/// Render the decode as sectioned, aligned `key: value` plain text. Pure and
/// allocation-bounded; a failed section shows its error instead of fabricated
/// fields. Never panics (no indexing, no `unwrap` on decoded data).
#[must_use]
pub fn render_human(results: &DecodeResults) -> String {
    let mut out = String::new();

    render_identity(&mut out, &results.identity);
    out.push('\n');
    render_crc(&mut out, &results.crc);
    out.push('\n');
    render_timings(&mut out, &results.timings);
    out.push('\n');
    render_module(&mut out, &results.module);
    out.push('\n');
    render_manufacturing(&mut out, &results.manufacturing);
    out.push('\n');
    render_vendor(&mut out, &results.vendor);

    out
}

/// Write one aligned `  label   value` line. `label` carries its own trailing
/// colon. Writing to a `String` never fails, so the result is discarded.
fn field(out: &mut String, label: &str, value: impl std::fmt::Display) {
    let _ = writeln!(out, "  {label:<LABEL_WIDTH$} {value}");
}

/// Write the error line for a section that failed to decode.
fn section_error(out: &mut String, error: &DecodeError) {
    let _ = writeln!(out, "  error: {error}");
}

fn render_identity(out: &mut String, result: &Result<IdentityAndBase, DecodeError>) {
    out.push_str("[Identity and base]\n");
    match result {
        Ok(id) => {
            field(
                out,
                "SPD device size:",
                format_args!("{} bytes", id.spd_bytes_total),
            );
            field(out, "SPD revision:", id.spd_revision);
            field(out, "DRAM device type:", id.device_type);
            field(out, "Module type:", id.module_type);
            field(out, "Hybrid module:", yes_no(id.hybrid));
            field(
                out,
                "Density per die:",
                format_args!("{} Gb", id.density_per_die.gigabits()),
            );
            field(out, "Package:", id.package_type);
            field(out, "Dies per package:", id.die_count);
            field(out, "Row address bits:", id.row_address_bits);
            field(out, "Column address bits:", id.column_address_bits);
            field(out, "I/O width:", format_args!("x{}", id.io_width.bits()));
            field(out, "Bank groups:", id.bank_groups.count());
            field(
                out,
                "Banks per bank group:",
                id.banks_per_bank_group.count(),
            );
            field(
                out,
                "Package ranks per channel:",
                id.package_ranks_per_channel,
            );
            field(
                out,
                "Rank mix:",
                if id.rank_mix_asymmetric {
                    "asymmetric"
                } else {
                    "symmetric"
                },
            );
            field(out, "Channels per DIMM:", id.channels_per_dimm);
            field(
                out,
                "Primary bus width per channel:",
                format_args!("{} bits", id.primary_bus_width_bits),
            );
        }
        Err(error) => section_error(out, error),
    }
}

fn render_crc(out: &mut String, result: &Result<CrcStatus, DecodeError>) {
    out.push_str("[Base configuration CRC]\n");
    out.push_str("  Reported status of the base CRC (bytes 0-509). Not a verdict; the vendor section CRCs are separate.\n");
    match result {
        Ok(crc) => {
            field(out, "Computed:", format_args!("{:#06X}", crc.computed));
            field(out, "Stored:", format_args!("{:#06X}", crc.stored));
            field(out, "Match:", yes_no(crc.matches));
        }
        Err(error) => section_error(out, error),
    }
}

fn render_timings(out: &mut String, result: &Result<Timings, DecodeError>) {
    out.push_str("[JEDEC base timings]\n");
    out.push_str(
        "  SPD JEDEC base timings (the guaranteed fallback). The rated DDR5 speed is shown below in the vendor-profiles section.\n",
    );
    match result {
        Ok(t) => {
            let rate = t.base_data_rate_mt_s();
            field(
                out,
                "Base data rate:",
                format_args!("DDR5-{rate} ({rate} MT/s, JEDEC base)"),
            );
            field(
                out,
                "tCKAVGmin:",
                format_args!("{} ps", t.tckavg_min.picoseconds()),
            );
            field(
                out,
                "tCKAVGmax:",
                format_args!("{} ps", t.tckavg_max.picoseconds()),
            );
            field(
                out,
                "Supported CAS latencies:",
                cas_list(t.supported_cas_latencies),
            );
            field(out, "tAA:", format_args!("{} ps", t.taa.picoseconds()));
            field(out, "tRCD:", format_args!("{} ps", t.trcd.picoseconds()));
            field(out, "tRP:", format_args!("{} ps", t.trp.picoseconds()));
            field(out, "tRAS:", format_args!("{} ps", t.tras.picoseconds()));
            field(out, "tRC:", format_args!("{} ps", t.trc.picoseconds()));
            field(out, "tWR:", format_args!("{} ps", t.twr.picoseconds()));
            field(out, "tRFC1:", format_args!("{} ps", t.trfc1.picoseconds()));
            field(out, "tRFC2:", format_args!("{} ps", t.trfc2.picoseconds()));
            field(
                out,
                "tRFCsb:",
                format_args!("{} ps", t.trfcsb.picoseconds()),
            );
            field(out, "tRRD_L:", pair(t.trrd_l));
            field(out, "tCCD_L:", pair(t.tccd_l));
            field(out, "tCCD_L_WR:", pair(t.tccd_l_wr));
            field(out, "tCCD_L_WR2:", pair(t.tccd_l_wr2));
            field(out, "tFAW:", pair(t.tfaw));
            field(out, "tWTR_L:", pair(t.twtr_l));
            field(out, "tWTR_S:", pair(t.twtr_s));
            field(out, "tRTP:", pair(t.trtp));
        }
        Err(error) => section_error(out, error),
    }
}

fn render_module(out: &mut String, result: &Result<ModuleSpecific, DecodeError>) {
    out.push_str("[Module-specific]\n");
    match result {
        Ok(ModuleSpecific::Unbuffered(m)) => {
            field(out, "Module type:", "UDIMM (unbuffered)");
            field(out, "Nominal height:", m.nominal_height);
            field(out, "Max thickness, front:", m.max_thickness_front);
            field(out, "Max thickness, back:", m.max_thickness_back);
            field(out, "Reference raw card:", m.reference_raw_card);
            field(
                out,
                "Rank 1 address mapping:",
                if m.rank1_address_mirrored {
                    "mirrored"
                } else {
                    "standard"
                },
            );
            field(
                out,
                "Module attributes (raw):",
                format_args!("{:#04X}", m.module_attributes_raw),
            );
        }
        Ok(ModuleSpecific::NotYetDecoded(module_type)) => {
            field(
                out,
                "Module type:",
                format_args!("{module_type} (module-specific block not yet decoded)"),
            );
        }
        Err(error) => section_error(out, error),
    }
}

fn render_manufacturing(out: &mut String, result: &Result<Manufacturing, DecodeError>) {
    out.push_str("[Manufacturing]\n");
    match result {
        Ok(m) => {
            field(out, "Module manufacturer:", m.module_manufacturer);
            field(out, "Manufacturing location:", m.manufacturing_location);
            field(out, "Manufacturing date:", m.manufacturing_date);
            field(out, "Serial number:", m.serial_number);
            field(out, "Part number:", m.part_number);
            field(out, "Revision code:", m.revision_code);
            field(out, "DRAM manufacturer:", m.dram_manufacturer);
            field(out, "DRAM stepping:", dram_stepping(m.dram_stepping));
        }
        Err(error) => section_error(out, error),
    }
}

// --- Vendor-profile rendering ----------------------------------------------

/// Label column width for the indented vendor-profile lines. Chosen so values
/// align at the same column as the flat sections (33) regardless of nesting
/// depth: at every indent, indent + (WIDTH - indent) + 1 is constant.
const VENDOR_LABEL_WIDTH: usize = 32;

fn render_vendor(out: &mut String, result: &Result<VendorProfiles, DecodeError>) {
    out.push_str("[Vendor profiles (XMP 3.0 / EXPO)]\n");
    out.push_str(
        "  Rated overclock profiles. Each section is CRC-checked; the match is the region anchor.\n",
    );
    match result {
        Ok(v) => {
            render_xmp(out, &v.xmp);
            render_expo(out, &v.expo);
        }
        Err(error) => section_error(out, error),
    }
}

fn render_xmp(out: &mut String, xmp: &Xmp) {
    match xmp {
        Xmp::Absent => vline(out, 2, "Intel XMP 3.0:", "absent"),
        Xmp::Present {
            header_crc,
            profile1,
            profile2,
        } => {
            vline(out, 2, "Intel XMP 3.0:", "present");
            vline(out, 4, "Header section CRC:", crc_summary(header_crc));
            render_xmp_slot(out, 1, profile1);
            render_xmp_slot(out, 2, profile2);
        }
    }
}

fn render_xmp_slot(out: &mut String, number: u8, slot: &Option<XmpProfile>) {
    match slot {
        Some(p) => {
            let name = p.name.unwrap_or("(unnamed)");
            vheading(out, 4, &format!("Profile {number}: {name}"));
            render_rated(out, &p.rated);
            vline(out, 6, "Section CRC:", crc_summary(&p.crc));
        }
        None => vheading(out, 4, &format!("Profile {number}: (not enabled)")),
    }
}

fn render_expo(out: &mut String, expo: &Expo) {
    match expo {
        Expo::Absent => vline(out, 2, "AMD EXPO:", "absent"),
        Expo::Present {
            block_crc,
            profile1,
            profile2,
        } => {
            vline(out, 2, "AMD EXPO:", "present");
            vline(out, 4, "Block section CRC:", crc_summary(block_crc));
            render_expo_slot(out, 1, profile1);
            render_expo_slot(out, 2, profile2);
        }
    }
}

fn render_expo_slot(out: &mut String, number: u8, slot: &Option<ExpoProfile>) {
    match slot {
        Some(p) => {
            vheading(out, 4, &format!("Profile {number}:"));
            render_rated(out, &p.rated);
        }
        None => vheading(out, 4, &format!("Profile {number}: (not populated)")),
    }
}

/// Render the shared rated values at the per-profile indent (6 spaces).
fn render_rated(out: &mut String, r: &RatedTimings) {
    vline(
        out,
        6,
        "Data rate:",
        format_args!("DDR5-{0} ({0} MT/s)", r.data_rate_mt_s),
    );
    vline(out, 6, "CAS latency:", format_args!("CL{}", r.cas_latency));
    vline(
        out,
        6,
        "tCKAVGmin:",
        format_args!("{} ps", r.cycle_time.picoseconds()),
    );
    vline(out, 6, "tRCD:", timing_clocks(r.trcd, r.cycle_time));
    vline(out, 6, "tRP:", timing_clocks(r.trp, r.cycle_time));
    vline(out, 6, "tRAS:", timing_clocks(r.tras, r.cycle_time));
    vline(
        out,
        6,
        "VDD / VDDQ / VPP:",
        format_args!("{} / {} / {}", r.vdd, r.vddq, r.vpp),
    );
}

/// Write one indented `label   value` line; values align at a fixed column.
fn vline(out: &mut String, indent: usize, label: &str, value: impl std::fmt::Display) {
    let pad = VENDOR_LABEL_WIDTH.saturating_sub(indent);
    let _ = writeln!(out, "{:i$}{label:<pad$} {value}", "", i = indent);
}

/// Write one indented heading line (no value column).
fn vheading(out: &mut String, indent: usize, text: &str) {
    let _ = writeln!(out, "{:i$}{text}", "", i = indent);
}

/// Format a timing in picoseconds with its whole-cycle count, guarding a zero
/// cycle time so a malformed-but-decoded profile cannot divide by zero.
fn timing_clocks(t: Picoseconds, cycle_time: Picoseconds) -> String {
    let ps = t.picoseconds();
    let tck = cycle_time.picoseconds();
    match (ps + tck / 2).checked_div(tck) {
        Some(clocks) => format!("{ps} ps ({clocks} clocks)"),
        None => format!("{ps} ps"),
    }
}

/// One-line CRC summary: the computed and stored values and whether they match.
fn crc_summary(crc: &CrcStatus) -> String {
    format!(
        "computed {:#06X}, stored {:#06X} ({})",
        crc.computed,
        crc.stored,
        if crc.matches { "match" } else { "MISMATCH" }
    )
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

/// Format a [`spdr::TimingPair`] as `"<ps> ps / <nCK> nCK"`.
fn pair(value: spdr::TimingPair) -> String {
    format!(
        "{} ps / {} nCK",
        value.time.picoseconds(),
        value.clocks.cycles()
    )
}

/// Format the supported CAS latency set as an ascending comma-separated list.
fn cas_list(value: CasLatencies) -> String {
    let mut out = String::new();
    for (i, cl) in value.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        let _ = write!(out, "{cl}");
    }
    if out.is_empty() {
        out.push_str("(none)");
    }
    out
}

/// Format DRAM stepping, naming the conventional `0xff` "not specified".
fn dram_stepping(value: u8) -> String {
    if value == 0xFF {
        "255 (not specified)".to_string()
    } else {
        format!("{value}")
    }
}

// --- JSON rendering --------------------------------------------------------

/// Build the per-section JSON value: the serialized library type, or, for a
/// failed section, an `{ "error": <message> }` object so the JSON stays complete
/// and valid. A macro, not a generic function, so the section types stay
/// concrete without the CLI depending on `serde` directly (only `serde_json`).
macro_rules! section_value {
    ($result:expr) => {
        match $result {
            Ok(value) => serde_json::to_value(value)?,
            Err(error) => serde_json::json!({ "error": error.to_string() }),
        }
    };
}

/// Render the decode as a JSON object keyed by section. Each value is the
/// serde-serialized library type; a failed section carries an error indicator,
/// so the object always has all five keys and stays valid JSON.
///
/// # Errors
/// Returns a [`serde_json::Error`] only if serializing a decoded value fails,
/// which the decoded types do not do in practice.
pub fn render_json(results: &DecodeResults) -> Result<String, serde_json::Error> {
    let mut object = serde_json::Map::new();
    object.insert(
        "identity_and_base".to_string(),
        section_value!(&results.identity),
    );
    object.insert("base_crc".to_string(), section_value!(&results.crc));
    object.insert(
        "jedec_base_timings".to_string(),
        section_value!(&results.timings),
    );
    object.insert(
        "module_specific".to_string(),
        section_value!(&results.module),
    );
    object.insert(
        "manufacturing".to_string(),
        section_value!(&results.manufacturing),
    );
    object.insert(
        "vendor_profiles".to_string(),
        section_value!(&results.vendor),
    );
    serde_json::to_string_pretty(&Value::Object(object))
}
