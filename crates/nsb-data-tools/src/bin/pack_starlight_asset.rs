use anyhow::{bail, Context, Result};
use clap::{ArgGroup, Parser};
use nsb::ValidatedStarlightMap;
use serde::{Deserialize, Serialize};
use siderust::checksum::{sha256, to_hex};
use std::collections::BTreeMap;
use std::path::PathBuf;

const GAIA_XP_MODEL: &str = "gaia_dr3_xp_photon_radiance_336_650nm_v1";
const GAIA_XP_BAND_MIN_NM: f64 = 336.0;
const GAIA_XP_BAND_MAX_NM: f64 = 650.0;
const GAIA_XP_BAND_DEFINITION: &str = "Gaia DR3 XP passband-integrated 336-650 nm photon radiance";

/// Pack a validated starlight map into a checksummed release asset.
#[derive(Debug, Parser)]
#[command(group(
    ArgGroup::new("mode")
        .required(true)
        .args(["candidate", "production"])
))]
struct Args {
    /// Generated starlight map CSV.
    #[arg(long)]
    input: PathBuf,
    /// Build diagnostics JSON.
    #[arg(long)]
    diagnostics: PathBuf,
    /// Validation report JSON.
    #[arg(long)]
    validation: PathBuf,
    /// Release CSV path. The output is a raw UTF-8 HEALPix CSV map.
    #[arg(long)]
    output: PathBuf,
    /// Output runtime TOML manifest path.
    #[arg(long)]
    manifest: PathBuf,
    /// Pack a non-production release candidate artifact.
    #[arg(long)]
    candidate: bool,
    /// Pack a production artifact. Requires complete production validation evidence.
    #[arg(long, requires_all = ["nside_sweep_report", "nside_review"])]
    production: bool,
    /// Checksummed nside sweep summary. Required for production.
    #[arg(long, requires = "production")]
    nside_sweep_report: Option<PathBuf>,
    /// Maintainer review attestation bound to the nside sweep checksum. Required for production.
    #[arg(long, requires = "production")]
    nside_review: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct ValidationSummary {
    production_ready: bool,
    #[serde(default)]
    independent_comparison_pass: bool,
    #[serde(default)]
    flux_conservation_pass: Option<bool>,
    #[serde(default)]
    radiance_field: Option<String>,
    #[serde(default)]
    finite_nonnegative_pass: bool,
    #[serde(default)]
    spectral_contract_pass: bool,
    #[serde(default)]
    plane_pole_pass: bool,
    #[serde(default)]
    longitude_wrap_pass: bool,
}

#[derive(Debug, Deserialize)]
struct DiagnosticsSummary {
    #[serde(default)]
    output_sha256: String,
    #[serde(default)]
    photometry_model: String,
    input_integrated_flux_sum_ph_cm2_ns: Option<f64>,
    #[serde(default)]
    integrated_flux_conservation_pass: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct NsideSweepReport {
    schema_version: u32,
    photometry_model: String,
    band_nm: [f64; 2],
    recommended_nside: Option<u32>,
    review_required: bool,
    summaries: Vec<NsideSweepCandidate>,
}

#[derive(Debug, Deserialize)]
struct NsideSweepCandidate {
    nside: u32,
    production_ready: bool,
    spectral_contract_pass: bool,
    eligible_for_recommendation: bool,
}

#[derive(Debug, Deserialize)]
struct NsideReview {
    schema_version: u32,
    sweep_report_sha256: String,
    reviewed: bool,
    selected_nside: Option<u32>,
    reviewer: Option<String>,
    reviewed_at_utc: Option<String>,
    rationale: Option<String>,
}

#[derive(Debug)]
struct ReviewedNsideEvidence {
    report_sha256: String,
    review_sha256: String,
    selected_nside: u32,
    reviewer: String,
    reviewed_at_utc: String,
}

#[derive(Debug, Serialize)]
struct RuntimeManifest {
    schema_version: u32,
    calibration_status: String,
    dataset_name: String,
    version: String,
    generation_date: String,
    source_catalogue: String,
    source_catalogue_release: String,
    source_catalogue_license: String,
    source_catalogue_checksum: String,
    source_selection: String,
    magnitude_limit: String,
    map_resolution: String,
    photometry_model: String,
    band_definition: String,
    smoothing: String,
    generated_by: String,
    generation_command: String,
    map_sha256: String,
    validation_report: String,
    independent_comparison: String,
    flux_conservation_validated: bool,
    input_integrated_flux_sum: Option<f64>,
    integrated_flux_conservation_tolerance: Option<f64>,
    header: BTreeMap<String, String>,
}

fn main() -> Result<()> {
    run(Args::parse())
}

fn run(args: Args) -> Result<()> {
    let map = std::fs::read(&args.input)
        .with_context(|| format!("failed to read map {}", args.input.display()))?;
    let diagnostics = std::fs::read(&args.diagnostics)
        .with_context(|| format!("failed to read diagnostics {}", args.diagnostics.display()))?;
    let validation = std::fs::read(&args.validation)
        .with_context(|| format!("failed to read validation {}", args.validation.display()))?;
    let validation_summary: ValidationSummary =
        serde_json::from_slice(&validation).context("failed to parse validation report")?;
    let diagnostics_summary: DiagnosticsSummary =
        serde_json::from_slice(&diagnostics).context("failed to parse diagnostics report")?;
    if args.candidate && !validation_summary.production_ready {
        eprintln!("warning: packing a non-production-ready starlight asset candidate");
    }

    let raw_map = std::str::from_utf8(&map).context("map must be UTF-8 CSV")?;
    let mut header = parse_header_metadata(raw_map);
    let map_input_sha256 = format!("sha256:{}", to_hex(&sha256(&map)));
    let nside_evidence = if args.production {
        enforce_production_gates(
            &validation_summary,
            &diagnostics_summary,
            &header,
            &map_input_sha256,
        )?;
        Some(validate_nside_review(&args, &header)?)
    } else if header
        .get("photometry_model")
        .is_some_and(|model| is_proxy_or_experimental(model))
    {
        eprintln!("warning: packing candidate with proxy or experimental photometry model");
        None
    } else {
        None
    };

    let mode = if args.production {
        "production"
    } else {
        "candidate"
    };
    let validation_sha256 = format!("sha256:{}", to_hex(&sha256(&validation)));
    complete_runtime_header(
        &mut header,
        mode,
        &args,
        &validation_sha256,
        &validation_summary,
        nside_evidence.as_ref(),
    )?;
    let release_csv = rewrite_csv_with_header(raw_map, &header)?;
    std::fs::write(&args.output, release_csv.as_bytes())
        .with_context(|| format!("failed to write release CSV {}", args.output.display()))?;

    let map_sha256 = format!("sha256:{}", to_hex(&sha256(release_csv.as_bytes())));
    let manifest = RuntimeManifest {
        schema_version: 1,
        calibration_status: required_header(&header, "calibration_status")?.to_string(),
        dataset_name: required_header(&header, "dataset_name")?.to_string(),
        version: required_header(&header, "version")?.to_string(),
        generation_date: required_header(&header, "generation_date_utc")?.to_string(),
        source_catalogue: required_header(&header, "source_catalogue")?.to_string(),
        source_catalogue_release: required_header(&header, "source_catalogue_release")?.to_string(),
        source_catalogue_license: required_header(&header, "source_catalogue_license")?.to_string(),
        source_catalogue_checksum: required_header(&header, "source_catalogue_checksum")?
            .to_string(),
        source_selection: required_header(&header, "source_selection")?.to_string(),
        magnitude_limit: required_header(&header, "magnitude_limit")?.to_string(),
        map_resolution: required_header(&header, "map_resolution")?.to_string(),
        photometry_model: required_header(&header, "photometry_model")?.to_string(),
        band_definition: required_header(&header, "band_definition")?.to_string(),
        smoothing: required_header(&header, "smoothing")?.to_string(),
        generated_by: required_header(&header, "generated_by")?.to_string(),
        generation_command: required_header(&header, "generation_command")?.to_string(),
        map_sha256,
        validation_report: required_header(&header, "validation_report")?.to_string(),
        independent_comparison: required_header(&header, "independent_comparison")?.to_string(),
        flux_conservation_validated: validation_summary.flux_conservation_pass.unwrap_or(false)
            && diagnostics_summary
                .integrated_flux_conservation_pass
                .unwrap_or(true),
        input_integrated_flux_sum: diagnostics_summary.input_integrated_flux_sum_ph_cm2_ns,
        integrated_flux_conservation_tolerance: diagnostics_summary
            .input_integrated_flux_sum_ph_cm2_ns
            .map(|_| 1.0e-9),
        header: header.clone(),
    };
    let raw = toml::to_string_pretty(&manifest)?;
    std::fs::write(&args.manifest, raw)
        .with_context(|| format!("failed to write manifest {}", args.manifest.display()))?;
    if args.production {
        ValidatedStarlightMap::from_files(&args.output, &args.manifest)
            .context("packer output failed runtime validated-starlight self-load")?;
    }
    Ok(())
}

fn enforce_production_gates(
    validation: &ValidationSummary,
    diagnostics: &DiagnosticsSummary,
    header: &BTreeMap<String, String>,
    map_input_sha256: &str,
) -> Result<()> {
    if !validation.production_ready {
        bail!("--production requires validation production_ready=true");
    }
    if !validation.independent_comparison_pass {
        bail!("--production requires independent_comparison_pass=true");
    }
    if validation.radiance_field.as_deref() != Some("integrated_ph_cm2_ns_sr") {
        bail!("--production requires integrated radiance validation");
    }
    if validation.flux_conservation_pass != Some(true) {
        bail!("--production requires integrated flux_conservation_pass=true");
    }
    if diagnostics.integrated_flux_conservation_pass == Some(false)
        || diagnostics.input_integrated_flux_sum_ph_cm2_ns.is_none()
    {
        bail!("--production requires diagnostics integrated flux-conservation evidence");
    }
    if !validation.finite_nonnegative_pass
        || !validation.spectral_contract_pass
        || !validation.plane_pole_pass
        || !validation.longitude_wrap_pass
    {
        bail!("--production requires finite/nonnegative, 336-650 spectral-contract, plane/pole, and longitude-wrap validation passes");
    }
    if diagnostics.output_sha256.trim().is_empty() {
        bail!("--production requires diagnostics output_sha256");
    }
    if !normalize_checksum(&diagnostics.output_sha256)
        .eq_ignore_ascii_case(normalize_checksum(map_input_sha256))
    {
        bail!("--production requires diagnostics output_sha256 to match the input map");
    }
    let photometry_model = required_header(header, "photometry_model")?;
    if photometry_model != GAIA_XP_MODEL {
        bail!("--production requires photometry_model={GAIA_XP_MODEL}");
    }
    if diagnostics.photometry_model != *photometry_model {
        bail!("--production requires diagnostics photometry_model to match map header");
    }
    let calibration_status = required_header(header, "calibration_status")?;
    if calibration_status.eq_ignore_ascii_case("experimental") {
        bail!("--production rejects experimental calibration metadata");
    }
    for key in [
        "source_catalogue",
        "source_catalogue_release",
        "source_catalogue_license",
        "source_catalogue_checksum",
        "band_definition",
        "validation_report",
    ] {
        let value = required_header(header, key)?;
        if is_placeholder(value) {
            bail!("--production requires non-placeholder header {key}");
        }
    }
    let band = required_header(header, "band_definition")?.to_ascii_lowercase();
    let normalized_band = band.replace(['–', '—'], "-");
    if !normalized_band.eq_ignore_ascii_case(GAIA_XP_BAND_DEFINITION) {
        bail!("--production requires the explicitly validated 336-650 nm band");
    }
    Ok(())
}

fn validate_nside_review(
    args: &Args,
    header: &BTreeMap<String, String>,
) -> Result<ReviewedNsideEvidence> {
    let report_path = args
        .nside_sweep_report
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("--production requires --nside-sweep-report"))?;
    let review_path = args
        .nside_review
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("--production requires --nside-review"))?;
    let report_raw = std::fs::read(report_path).with_context(|| {
        format!(
            "failed to read nside sweep report {}",
            report_path.display()
        )
    })?;
    let review_raw = std::fs::read(review_path)
        .with_context(|| format!("failed to read nside review {}", review_path.display()))?;
    let report: NsideSweepReport =
        serde_json::from_slice(&report_raw).context("failed to parse nside sweep report")?;
    let review: NsideReview =
        serde_json::from_slice(&review_raw).context("failed to parse nside review")?;

    if report.schema_version != 1 || review.schema_version != 1 {
        bail!("--production requires nside sweep and review schema_version=1");
    }
    if report.photometry_model != GAIA_XP_MODEL
        || report.band_nm[0].to_bits() != GAIA_XP_BAND_MIN_NM.to_bits()
        || report.band_nm[1].to_bits() != GAIA_XP_BAND_MAX_NM.to_bits()
    {
        bail!("--production requires a 336-650 Gaia XP nside sweep report");
    }
    if !report.review_required {
        bail!("--production requires a sweep report that explicitly requires maintainer review");
    }
    let report_sha256 = format!("sha256:{}", to_hex(&sha256(&report_raw)));
    if !normalize_checksum(&review.sweep_report_sha256)
        .eq_ignore_ascii_case(normalize_checksum(&report_sha256))
    {
        bail!("--production requires the nside review to match the sweep report checksum");
    }
    if !review.reviewed {
        bail!("--production requires reviewed=true in the nside review attestation");
    }
    let recommended_nside = report.recommended_nside.ok_or_else(|| {
        anyhow::anyhow!("--production requires an automated nside recommendation")
    })?;
    let selected_nside = review
        .selected_nside
        .ok_or_else(|| anyhow::anyhow!("--production requires a reviewed selected_nside"))?;
    if selected_nside != recommended_nside {
        bail!(
            "--production requires reviewed selected_nside={selected_nside} to match automated recommendation {recommended_nside}"
        );
    }
    let map_nside = required_header(header, "nside")?
        .parse::<u32>()
        .context("map header nside must be an integer")?;
    if selected_nside != map_nside {
        bail!(
            "--production nside review selected {selected_nside}, but map header declares {map_nside}"
        );
    }
    let selected = report
        .summaries
        .iter()
        .find(|summary| summary.nside == selected_nside)
        .ok_or_else(|| anyhow::anyhow!("selected nside is absent from the sweep summaries"))?;
    if !selected.production_ready
        || !selected.spectral_contract_pass
        || !selected.eligible_for_recommendation
    {
        bail!("--production requires the reviewed nside to pass every sweep recommendation gate");
    }

    let reviewer = review
        .reviewer
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("--production requires nside review reviewer"))?;
    validate_review_text("reviewer", reviewer, 3)?;
    let reviewed_at_utc = review
        .reviewed_at_utc
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("--production requires nside review reviewed_at_utc"))?;
    if !looks_like_utc_timestamp(reviewed_at_utc) {
        bail!("--production requires reviewed_at_utc in RFC3339 UTC form");
    }
    let rationale = review
        .rationale
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("--production requires nside review rationale"))?;
    validate_review_text("rationale", rationale, 20)?;

    Ok(ReviewedNsideEvidence {
        report_sha256,
        review_sha256: format!("sha256:{}", to_hex(&sha256(&review_raw))),
        selected_nside,
        reviewer: reviewer.to_string(),
        reviewed_at_utc: reviewed_at_utc.to_string(),
    })
}

fn validate_review_text(name: &str, value: &str, minimum_len: usize) -> Result<()> {
    let trimmed = value.trim();
    if trimmed.len() < minimum_len || trimmed.chars().any(char::is_control) {
        bail!("--production requires a substantive nside review {name}");
    }
    let lower = trimmed.to_ascii_lowercase();
    for blocked in ["todo", "placeholder", "pending", "unreviewed"] {
        if lower.contains(blocked) {
            bail!("--production nside review {name} contains blocked marker {blocked:?}");
        }
    }
    if name == "reviewer"
        && ["automatic", "automated", "ci bot", "pipeline bot"]
            .iter()
            .any(|blocked| lower.contains(blocked))
    {
        bail!("--production requires a human nside reviewer identity");
    }
    Ok(())
}

fn looks_like_utc_timestamp(value: &str) -> bool {
    let Some(body) = value.strip_suffix('Z') else {
        return false;
    };
    let Some((date, time)) = body.split_once('T') else {
        return false;
    };
    if !date.is_ascii() || !time.is_ascii() {
        return false;
    }
    if date.len() != 10 || date.as_bytes()[4] != b'-' || date.as_bytes()[7] != b'-' {
        return false;
    }
    let (clock, fractional) = time
        .split_once('.')
        .map_or((time, None), |(clock, fraction)| (clock, Some(fraction)));
    if clock.len() != 8 || clock.as_bytes()[2] != b':' || clock.as_bytes()[5] != b':' {
        return false;
    }
    if fractional.is_some_and(|fraction| {
        fraction.is_empty() || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    }) {
        return false;
    }
    matches!(parse_ascii_u32(&date[0..4]), Some(1..=9999))
        && matches!(parse_ascii_u32(&date[5..7]), Some(1..=12))
        && matches!(parse_ascii_u32(&date[8..10]), Some(1..=31))
        && matches!(parse_ascii_u32(&clock[0..2]), Some(0..=23))
        && matches!(parse_ascii_u32(&clock[3..5]), Some(0..=59))
        && matches!(parse_ascii_u32(&clock[6..8]), Some(0..=59))
}

fn parse_ascii_u32(value: &str) -> Option<u32> {
    value
        .bytes()
        .all(|byte| byte.is_ascii_digit())
        .then(|| value.parse().ok())
        .flatten()
}

fn normalize_checksum(value: &str) -> &str {
    value.trim().strip_prefix("sha256:").unwrap_or(value.trim())
}

fn complete_runtime_header(
    header: &mut BTreeMap<String, String>,
    mode: &str,
    args: &Args,
    validation_sha256: &str,
    validation: &ValidationSummary,
    nside_evidence: Option<&ReviewedNsideEvidence>,
) -> Result<()> {
    let nside = required_header(header, "nside")?.to_string();
    let ordering = required_header(header, "ordering")?.to_string();
    set_default(header, "map_type", "healpix");
    set_default(header, "coordinate_frame", "galactic");
    set_default(header, "dataset_name", "NSB Gaia DR3 starlight map");
    set_default(header, "version", "v1");
    set_default(
        header,
        "map_resolution",
        &format!("HEALPix nside={nside} ordering={ordering}"),
    );
    set_default(
        header,
        "source_selection",
        "Gaia DR3 XP-selected source population",
    );
    set_default(
        header,
        "magnitude_limit",
        "Gaia DR3 release input selection",
    );
    set_default(header, "smoothing", "none");
    set_default(header, "source_catalogue_release", "DR3");
    header.insert(
        "validation_report".to_string(),
        format!("{} sha256 {}", args.validation.display(), validation_sha256),
    );
    header.insert(
        "independent_comparison".to_string(),
        format!(
            "structured independent comparison passed: {}",
            validation.independent_comparison_pass
        ),
    );
    header.insert("calibration_status".to_string(), mode.to_string());
    if let Some(evidence) = nside_evidence {
        header.insert(
            "nside_sweep_report".to_string(),
            format!("reviewed report {}", evidence.report_sha256),
        );
        header.insert(
            "nside_sweep_review".to_string(),
            format!("attestation {}", evidence.review_sha256),
        );
        header.insert(
            "nside_sweep_selected_nside".to_string(),
            evidence.selected_nside.to_string(),
        );
        header.insert(
            "nside_sweep_reviewer".to_string(),
            evidence.reviewer.clone(),
        );
        header.insert(
            "nside_sweep_reviewed_at_utc".to_string(),
            evidence.reviewed_at_utc.clone(),
        );
    }
    for required in [
        "generation_date_utc",
        "source_catalogue",
        "source_catalogue_license",
        "source_catalogue_checksum",
        "photometry_model",
        "band_definition",
        "generated_by",
        "generation_command",
    ] {
        required_header(header, required)?;
    }
    Ok(())
}

fn set_default(header: &mut BTreeMap<String, String>, key: &str, value: &str) {
    header
        .entry(key.to_string())
        .or_insert_with(|| value.to_string());
}

fn rewrite_csv_with_header(raw_map: &str, header: &BTreeMap<String, String>) -> Result<String> {
    let data_start = raw_map
        .lines()
        .position(|line| {
            let line = line.trim();
            !line.is_empty() && !line.starts_with('#')
        })
        .ok_or_else(|| anyhow::anyhow!("map CSV has no data header"))?;
    let lines = raw_map.lines().collect::<Vec<_>>();
    let mut out = String::new();
    for key in [
        "map_type",
        "coordinate_frame",
        "nside",
        "ordering",
        "map_resolution",
        "dataset_name",
        "version",
        "calibration_status",
        "generation_date_utc",
        "source_catalogue",
        "source_catalogue_release",
        "source_catalogue_license",
        "source_catalogue_checksum",
        "source_selection",
        "magnitude_limit",
        "band_definition",
        "photometry_model",
        "smoothing",
        "generated_by",
        "generation_command",
        "validation_report",
        "independent_comparison",
    ] {
        let value = required_header(header, key)?;
        out.push_str("# ");
        out.push_str(key);
        out.push('=');
        out.push_str(value);
        out.push('\n');
    }
    for key in [
        "nside_sweep_report",
        "nside_sweep_review",
        "nside_sweep_selected_nside",
        "nside_sweep_reviewer",
        "nside_sweep_reviewed_at_utc",
    ] {
        if let Some(value) = header.get(key) {
            out.push_str("# ");
            out.push_str(key);
            out.push('=');
            out.push_str(value);
            out.push('\n');
        }
    }
    for line in &lines[data_start..] {
        out.push_str(line);
        out.push('\n');
    }
    Ok(out)
}

fn required_header<'a>(header: &'a BTreeMap<String, String>, key: &str) -> Result<&'a String> {
    header
        .get(key)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("--production requires map header {key}"))
}

fn is_proxy_or_experimental(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("proxy") || lower.contains("experimental")
}

fn is_placeholder(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("placeholder")
        || lower.contains("not recorded")
        || lower.contains("review required")
        || lower.contains("review-required")
        || lower.contains("manual-seed")
}

fn parse_header_metadata(raw: &str) -> BTreeMap<String, String> {
    raw.lines()
        .filter_map(|line| line.strip_prefix('#'))
        .filter_map(|line| line.trim().split_once('='))
        .map(|(key, value)| (key.trim().to_string(), value.trim().to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use siderust::coordinates::cartesian::Direction;
    use siderust::coordinates::frames::Galactic;
    use siderust::healpix::{HealpixGrid, HealpixIndex, HealpixOrdering, Nside};

    #[test]
    fn packs_fixture_map_and_manifest() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("map.csv");
        let diagnostics = dir.path().join("diagnostics.json");
        let validation = dir.path().join("validation.json");
        let output = dir.path().join("map.release.csv");
        let manifest = dir.path().join("map.manifest.toml");
        std::fs::write(
            &input,
            production_map("gaia_dr3_xp_photon_radiance_336_650nm_v1", "candidate"),
        )?;
        std::fs::write(&diagnostics, "{}\n")?;
        std::fs::write(&validation, "{\"production_ready\":false}\n")?;

        run(Args {
            input,
            diagnostics,
            validation,
            output: output.clone(),
            manifest: manifest.clone(),
            candidate: true,
            production: false,
            nside_sweep_report: None,
            nside_review: None,
        })?;

        let csv = std::fs::read_to_string(output)?;
        assert!(csv.starts_with("# map_type=healpix"));
        assert!(csv.contains("# calibration_status=candidate"));
        assert!(csv.contains("healpix_index,integrated_ph_cm2_ns_sr,b_s10,v_s10"));
        let manifest_raw = std::fs::read_to_string(manifest)?;
        assert!(manifest_raw.contains("calibration_status = \"candidate\""));
        assert!(manifest_raw.contains("map_sha256 = \"sha256:"));
        Ok(())
    }

    #[test]
    fn production_output_is_runtime_loadable() -> Result<()> {
        let (args, _dir) = fixture_args(production_validation(), true)?;
        run(args)?;
        Ok(())
    }

    #[test]
    fn production_rejects_unreviewed_nside_recommendation() -> Result<()> {
        let (args, _dir) = fixture_args(production_validation(), true)?;
        let review = args.nside_review.as_ref().expect("production review path");
        let raw =
            std::fs::read_to_string(review)?.replace("\"reviewed\": true", "\"reviewed\": false");
        std::fs::write(review, raw)?;
        let err = run(args).expect_err("unreviewed nside recommendation must fail closed");
        assert!(err
            .to_string()
            .contains("--production requires reviewed=true"));
        Ok(())
    }

    #[test]
    fn nside_review_timestamp_must_be_valid_utc_shape() {
        assert!(looks_like_utc_timestamp("2026-07-11T12:34:56Z"));
        assert!(looks_like_utc_timestamp("2026-07-11T12:34:56.123Z"));
        assert!(!looks_like_utc_timestamp("2026-13-11T12:34:56Z"));
        assert!(!looks_like_utc_timestamp("2026-07-11T25:34:56Z"));
        assert!(!looks_like_utc_timestamp("2026-07-11T12:34:56+00:00"));
    }

    #[test]
    fn production_cli_requires_sweep_report_and_review_paths() {
        let error = Args::try_parse_from([
            "pack_starlight_asset",
            "--input",
            "map.csv",
            "--diagnostics",
            "diagnostics.json",
            "--validation",
            "validation.json",
            "--output",
            "release.csv",
            "--manifest",
            "release.toml",
            "--production",
        ])
        .expect_err("production CLI must require reviewed nside evidence");
        let rendered = error.to_string();
        assert!(rendered.contains("--nside-sweep-report"));
        assert!(rendered.contains("--nside-review"));
    }

    #[test]
    fn production_rejects_not_ready_validation() -> Result<()> {
        let (args, _dir) = fixture_args("{\"production_ready\":false}\n", true)?;
        let err = run(args).expect_err("not production ready");
        assert!(err
            .to_string()
            .contains("--production requires validation production_ready=true"));
        Ok(())
    }

    #[test]
    fn production_rejects_proxy_model() -> Result<()> {
        let (args, _dir) = fixture_args(production_validation(), true)?;
        std::fs::write(
            &args.input,
            production_map("v_s10_scaled_integrated_proxy_v1", "production"),
        )?;
        write_production_diagnostics(
            &args.input,
            &args.diagnostics,
            "v_s10_scaled_integrated_proxy_v1",
        )?;
        let err = run(args).expect_err("proxy photometry rejected");
        assert!(err.to_string().contains(
            "--production requires photometry_model=gaia_dr3_xp_photon_radiance_336_650nm_v1"
        ));
        Ok(())
    }

    #[test]
    fn production_rejects_missing_independent_comparison() -> Result<()> {
        let validation = r#"{
  "production_ready": true,
  "independent_comparison_pass": false,
  "finite_nonnegative_pass": true,
  "plane_pole_pass": true,
  "longitude_wrap_pass": true
}
"#;
        let (args, _dir) = fixture_args(validation, true)?;
        let err = run(args).expect_err("missing comparison rejected");
        assert!(err
            .to_string()
            .contains("--production requires independent_comparison_pass=true"));
        Ok(())
    }

    fn fixture_args(validation_raw: &str, production: bool) -> Result<(Args, tempfile::TempDir)> {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("map.csv");
        let diagnostics = dir.path().join("diagnostics.json");
        let validation = dir.path().join("validation.json");
        let output = dir.path().join("map.release.csv");
        let manifest = dir.path().join("map.manifest.toml");
        let sweep = dir.path().join("nside-sweep.json");
        let review = dir.path().join("nside-review.json");
        std::fs::write(
            &input,
            production_map("gaia_dr3_xp_photon_radiance_336_650nm_v1", "production"),
        )?;
        write_production_diagnostics(&input, &diagnostics, GAIA_XP_MODEL)?;
        std::fs::write(&validation, validation_raw)?;
        let sweep_raw = r#"{
  "schema_version": 1,
  "photometry_model": "gaia_dr3_xp_photon_radiance_336_650nm_v1",
  "band_nm": [336.0, 650.0],
  "recommended_nside": 8,
  "review_required": true,
  "summaries": [{
    "nside": 8,
    "production_ready": true,
    "spectral_contract_pass": true,
    "eligible_for_recommendation": true
  }]
}
"#;
        std::fs::write(&sweep, sweep_raw)?;
        let sweep_sha = format!("sha256:{}", to_hex(&sha256(sweep_raw.as_bytes())));
        std::fs::write(
            &review,
            format!(
                r#"{{
  "schema_version": 1,
  "sweep_report_sha256": "{sweep_sha}",
  "reviewed": true,
  "selected_nside": 8,
  "reviewer": "NSB test maintainer",
  "reviewed_at_utc": "2026-07-11T12:00:00Z",
  "rationale": "Reviewed all resolution, flux, seam, noise, size, and runtime evidence."
}}
"#,
            ),
        )?;
        Ok((
            Args {
                input,
                diagnostics,
                validation,
                output,
                manifest,
                candidate: !production,
                production,
                nside_sweep_report: production.then_some(sweep),
                nside_review: production.then_some(review),
            },
            dir,
        ))
    }

    fn production_validation() -> &'static str {
        r#"{
  "production_ready": true,
  "independent_comparison_pass": true,
  "radiance_field": "integrated_ph_cm2_ns_sr",
  "flux_conservation_pass": true,
  "finite_nonnegative_pass": true,
  "spectral_contract_pass": true,
  "plane_pole_pass": true,
  "longitude_wrap_pass": true
}
"#
    }

    fn production_map(photometry_model: &str, calibration_status: &str) -> String {
        let grid = HealpixGrid::new(Nside::new(8).unwrap(), HealpixOrdering::Ring).unwrap();
        let mut raw = format!(
            concat!(
                "# map_type=healpix\n",
                "# coordinate_frame=galactic\n",
                "# nside=8\n",
                "# ordering=ring\n",
                "# dataset_name=synthetic Gaia XP fixture\n",
                "# version=fixture-v1\n",
                "# generation_date_utc=2026-06-24T00:00:00Z\n",
                "# photometry_model={}\n",
                "# calibration_status={}\n",
                "# source_catalogue=Gaia\n",
                "# source_catalogue_release=DR3\n",
                "# source_catalogue_license=reviewed redistribution policy\n",
                "# source_catalogue_checksum=sha256:1111111111111111111111111111111111111111111111111111111111111111\n",
                "# source_selection=synthetic Gaia XP fixture source selection\n",
                "# magnitude_limit=G <= 20\n",
                "# map_resolution=HEALPix nside=8 ordering=ring\n",
                "# band_definition=Gaia DR3 XP passband-integrated 336-650 nm photon radiance\n",
                "# smoothing=none\n",
                "# generated_by=pack_starlight_asset unit test\n",
                "# generation_command=pack_starlight_asset fixture\n",
                "# validation_report=independent validation report\n",
                "# independent_comparison=structured fixture comparison\n",
                "healpix_index,integrated_ph_cm2_ns_sr,b_s10,v_s10\n",
            ),
            photometry_model, calibration_status
        );
        for index in 0..grid.npix() {
            let direction: Direction<Galactic> =
                grid.pixel_center(HealpixIndex::new(index)).unwrap();
            let latitude = direction.as_array()[2].asin().to_degrees().abs();
            let value = if latitude <= 10.0 { 2.0 } else { 1.0 };
            raw.push_str(&format!("{index},{value},0,0\n"));
        }
        raw
    }

    fn production_source_flux() -> f64 {
        let grid = HealpixGrid::new(Nside::new(8).unwrap(), HealpixOrdering::Ring).unwrap();
        let mut source_flux = 0.0;
        for index in 0..grid.npix() {
            let direction: Direction<Galactic> =
                grid.pixel_center(HealpixIndex::new(index)).unwrap();
            let latitude = direction.as_array()[2].asin().to_degrees().abs();
            let value = if latitude <= 10.0 { 2.0 } else { 1.0 };
            source_flux += value * grid.pixel_area_sr();
        }
        source_flux
    }

    fn write_production_diagnostics(input: &PathBuf, path: &PathBuf, model: &str) -> Result<()> {
        let input_raw = std::fs::read(input)?;
        let checksum = format!("sha256:{}", to_hex(&sha256(&input_raw)));
        std::fs::write(
            path,
            format!(
                r#"{{
  "output_sha256":"{checksum}",
  "photometry_model":"{model}",
  "input_integrated_flux_sum_ph_cm2_ns": {:.17},
  "integrated_flux_conservation_pass": true
}}"#,
                production_source_flux()
            ),
        )?;
        Ok(())
    }
}
