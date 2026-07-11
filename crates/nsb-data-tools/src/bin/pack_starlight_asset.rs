use anyhow::{bail, Context, Result};
use clap::{ArgGroup, Parser};
use nsb::ValidatedStarlightMap;
use nsb_data_tools::starlight_approval::{
    load_and_validate_approval, normalize_sha256, ApprovalArtifactType, ApprovalRequirements,
    StarlightApproval, VerifiedApproval, STARLIGHT_PRODUCTION_BAND_NM,
};
use serde::{Deserialize, Serialize};
use siderust::checksum::{sha256, to_hex};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const NSIDE_SWEEP_SCHEMA_VERSION: u32 = 2;

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
    #[arg(
        long,
        requires_all = [
            "approval_root",
            "release_id",
            "nside_sweep_report",
            "missing_flux_approval",
            "independent_validation_approval",
            "redistribution_approval",
            "nside_review"
        ]
    )]
    production: bool,
    /// Root containing approval JSON and every checksummed file it references.
    #[arg(long, visible_alias = "artifact-root", requires = "production")]
    approval_root: Option<PathBuf>,
    /// Stable identifier shared by the output release and every approval.
    #[arg(long, requires = "production")]
    release_id: Option<String>,
    /// Checksummed nside sweep summary. Required for production.
    #[arg(long, requires = "production")]
    nside_sweep_report: Option<PathBuf>,
    /// Maintainer review attestation bound to the nside sweep checksum. Required for production.
    #[arg(long, visible_alias = "nside-review-approval", requires = "production")]
    nside_review: Option<PathBuf>,
    /// Human missing-flux production approval, relative to --approval-root.
    #[arg(long, requires = "production")]
    missing_flux_approval: Option<PathBuf>,
    /// Human independent-validation production approval, relative to --approval-root.
    #[arg(long, requires = "production")]
    independent_validation_approval: Option<PathBuf>,
    /// Human redistribution production approval, relative to --approval-root.
    #[arg(long, requires = "production")]
    redistribution_approval: Option<PathBuf>,
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
    recommended_candidate_nside: Option<u32>,
    review_required: bool,
    summaries: Vec<NsideSweepCandidate>,
}

#[derive(Debug, Deserialize)]
struct NsideSweepCandidate {
    nside: u32,
    spectral_contract_pass: bool,
    eligible_for_candidate_recommendation: bool,
}

#[derive(Debug)]
struct ReviewedNsideEvidence {
    report_sha256: String,
    selected_nside: u32,
    reviewer: String,
    reviewed_at_utc: String,
    release_id: String,
}

#[derive(Debug)]
struct ProductionApprovalDag {
    missing_flux: VerifiedApproval,
    independent_validation: VerifiedApproval,
    redistribution: VerifiedApproval,
    nside_review: VerifiedApproval,
    nside_evidence: ReviewedNsideEvidence,
}

#[derive(Debug, Deserialize, Serialize)]
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
    let approval_dag = if args.production {
        enforce_production_gates(
            &validation_summary,
            &diagnostics_summary,
            &header,
            &map_input_sha256,
        )?;
        Some(validate_production_approval_dag_prepack(
            &args,
            &header,
            &map_input_sha256,
            &validation,
        )?)
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
        approval_dag.as_ref().map(|dag| &dag.nside_evidence),
    )?;
    let release_csv = rewrite_csv_with_header(raw_map, &header)?;

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
    verify_manifest_output_binding(&manifest, release_csv.as_bytes())?;
    let manifest_raw = toml::to_string_pretty(&manifest)?;
    if let Some(dag) = approval_dag.as_ref() {
        let manifest_sha256 = format!("sha256:{}", to_hex(&sha256(manifest_raw.as_bytes())));
        validate_final_approval_bindings(
            &args,
            &map_input_sha256,
            &manifest_sha256,
            &validation_sha256,
            dag,
        )?;
    }

    std::fs::write(&args.output, release_csv.as_bytes())
        .with_context(|| format!("failed to write release CSV {}", args.output.display()))?;
    std::fs::write(&args.manifest, manifest_raw.as_bytes())
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
        bail!("--production requires finite/nonnegative, 300-650 spectral-contract, plane/pole, and longitude-wrap validation passes");
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
    if is_proxy_or_experimental(photometry_model) || is_placeholder(photometry_model) {
        bail!("--production requires a non-proxy, non-placeholder photometry_model");
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
    if !band_definition_is_exact_production_band(&normalized_band) {
        bail!("--production requires the explicitly validated 300-650 nm band");
    }
    Ok(())
}

fn validate_production_approval_dag_prepack(
    args: &Args,
    header: &BTreeMap<String, String>,
    map_sha256: &str,
    validation_raw: &[u8],
) -> Result<ProductionApprovalDag> {
    let missing_flux = load_required_approval(
        args,
        args.missing_flux_approval.as_deref(),
        ApprovalArtifactType::MissingFlux,
        None,
        Some(map_sha256),
        None,
    )?;
    let independent_validation = load_required_approval(
        args,
        args.independent_validation_approval.as_deref(),
        ApprovalArtifactType::IndependentValidation,
        None,
        Some(map_sha256),
        None,
    )?;
    let validation_sha256 = format!("sha256:{}", to_hex(&sha256(validation_raw)));
    require_approval_file_binding(
        &independent_validation.approval,
        &validation_sha256,
        "independent-validation approval to --validation",
    )?;
    let redistribution = load_required_approval(
        args,
        args.redistribution_approval.as_deref(),
        ApprovalArtifactType::Redistribution,
        None,
        None,
        None,
    )?;
    let (nside_review, nside_evidence) = validate_nside_review(args, header, map_sha256)?;
    Ok(ProductionApprovalDag {
        missing_flux,
        independent_validation,
        redistribution,
        nside_review,
        nside_evidence,
    })
}

fn validate_nside_review(
    args: &Args,
    header: &BTreeMap<String, String>,
    map_sha256: &str,
) -> Result<(VerifiedApproval, ReviewedNsideEvidence)> {
    let report_path = args
        .nside_sweep_report
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("--production requires --nside-sweep-report"))?;
    let report_raw = std::fs::read(report_path).with_context(|| {
        format!(
            "failed to read nside sweep report {}",
            report_path.display()
        )
    })?;
    let report: NsideSweepReport =
        serde_json::from_slice(&report_raw).context("failed to parse nside sweep report")?;

    if report.schema_version != NSIDE_SWEEP_SCHEMA_VERSION {
        bail!("--production requires nside sweep schema_version={NSIDE_SWEEP_SCHEMA_VERSION}");
    }
    if report.photometry_model != *required_header(header, "photometry_model")?
        || report.band_nm[0].to_bits() != f64::from(STARLIGHT_PRODUCTION_BAND_NM[0]).to_bits()
        || report.band_nm[1].to_bits() != f64::from(STARLIGHT_PRODUCTION_BAND_NM[1]).to_bits()
    {
        bail!("--production requires a map-compatible 300-650 nm nside sweep report");
    }
    if !report.review_required {
        bail!("--production requires a sweep report that explicitly requires maintainer review");
    }
    let report_sha256 = format!("sha256:{}", to_hex(&sha256(&report_raw)));
    let recommended_nside = report.recommended_candidate_nside.ok_or_else(|| {
        anyhow::anyhow!("--production requires an automated nside recommendation")
    })?;
    let map_nside = required_header(header, "nside")?
        .parse::<u32>()
        .context("map header nside must be an integer")?;
    if recommended_nside != map_nside {
        bail!(
            "--production nside sweep selected {recommended_nside}, but map header declares {map_nside}"
        );
    }
    let selected = report
        .summaries
        .iter()
        .find(|summary| summary.nside == recommended_nside)
        .ok_or_else(|| anyhow::anyhow!("selected nside is absent from the sweep summaries"))?;
    if !selected.spectral_contract_pass || !selected.eligible_for_candidate_recommendation {
        bail!("--production requires the reviewed nside to pass every sweep recommendation gate");
    }
    let review = load_required_approval(
        args,
        args.nside_review.as_deref(),
        ApprovalArtifactType::NsideReview,
        Some(recommended_nside),
        Some(map_sha256),
        None,
    )?;
    require_approval_file_binding(
        &review.approval,
        &report_sha256,
        "nside-review approval to --nside-sweep-report",
    )?;
    let evidence = ReviewedNsideEvidence {
        report_sha256,
        selected_nside: recommended_nside,
        reviewer: review.approval.reviewer_name.clone(),
        reviewed_at_utc: review.approval.date.clone(),
        release_id: review.approval.release_id.clone(),
    };
    Ok((review, evidence))
}

fn load_required_approval(
    args: &Args,
    path: Option<&Path>,
    artifact_type: ApprovalArtifactType,
    nside: Option<u32>,
    map_sha256: Option<&str>,
    manifest_sha256: Option<&str>,
) -> Result<VerifiedApproval> {
    let root = args
        .approval_root
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("--production requires --approval-root"))?;
    let release_id = args
        .release_id
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("--production requires --release-id"))?;
    let path = path.ok_or_else(|| {
        anyhow::anyhow!(
            "--production requires the {} approval path",
            artifact_type.as_str()
        )
    })?;
    load_and_validate_approval(
        root,
        path,
        ApprovalRequirements {
            artifact_type,
            release_id,
            nside,
            map_sha256,
            manifest_sha256,
            require_positive: true,
        },
    )
    .with_context(|| format!("--production rejected {} approval", artifact_type.as_str()))
}

fn validate_final_approval_bindings(
    args: &Args,
    map_sha256: &str,
    manifest_sha256: &str,
    validation_sha256: &str,
    initial: &ProductionApprovalDag,
) -> Result<()> {
    let missing_flux = load_required_approval(
        args,
        args.missing_flux_approval.as_deref(),
        ApprovalArtifactType::MissingFlux,
        None,
        Some(map_sha256),
        None,
    )?;
    let independent_validation = load_required_approval(
        args,
        args.independent_validation_approval.as_deref(),
        ApprovalArtifactType::IndependentValidation,
        None,
        Some(map_sha256),
        None,
    )?;
    require_approval_file_binding(
        &independent_validation.approval,
        validation_sha256,
        "independent-validation approval to --validation",
    )?;
    let redistribution = load_required_approval(
        args,
        args.redistribution_approval.as_deref(),
        ApprovalArtifactType::Redistribution,
        None,
        initial
            .redistribution
            .approval
            .map_sha256
            .as_ref()
            .map(|_| map_sha256),
        initial
            .redistribution
            .approval
            .manifest_sha256
            .as_ref()
            .map(|_| manifest_sha256),
    )?;
    let map_nside = initial.nside_evidence.selected_nside;
    let nside_review = load_required_approval(
        args,
        args.nside_review.as_deref(),
        ApprovalArtifactType::NsideReview,
        Some(map_nside),
        Some(map_sha256),
        None,
    )?;
    require_approval_file_binding(
        &nside_review.approval,
        &initial.nside_evidence.report_sha256,
        "nside-review approval to --nside-sweep-report",
    )?;

    for (name, before, after) in [
        ("missing_flux", &initial.missing_flux, &missing_flux),
        (
            "independent_validation",
            &initial.independent_validation,
            &independent_validation,
        ),
        ("redistribution", &initial.redistribution, &redistribution),
        ("nside_review", &initial.nside_review, &nside_review),
    ] {
        if before.sha256 != after.sha256 {
            bail!("--production {name} approval changed while the release was being packed");
        }
    }
    Ok(())
}

fn require_approval_file_binding(
    approval: &StarlightApproval,
    expected_sha256: &str,
    label: &str,
) -> Result<()> {
    let expected = normalize_sha256(expected_sha256)?;
    let matches = approval
        .input_files
        .iter()
        .chain(&approval.output_files)
        .filter_map(|file| normalize_sha256(&file.sha256).ok())
        .any(|digest| digest == expected);
    if !matches {
        bail!("--production requires {label} checksum binding");
    }
    Ok(())
}

fn verify_manifest_output_binding(manifest: &RuntimeManifest, release_csv: &[u8]) -> Result<()> {
    let actual = format!("sha256:{}", to_hex(&sha256(release_csv)));
    let expected = normalize_sha256(&manifest.map_sha256)?;
    if normalize_sha256(&actual)? != expected {
        bail!("runtime manifest map_sha256 does not bind the packed release output");
    }
    Ok(())
}

fn band_definition_is_exact_production_band(value: &str) -> bool {
    let compact = value
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    let marker = "300-650nm";
    compact.match_indices(marker).any(|(index, _)| {
        index == 0
            || !compact[..index]
                .chars()
                .next_back()
                .is_some_and(|character| character.is_ascii_digit())
    })
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
            format!("approval schema v1 release {}", evidence.release_id),
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
    use nsb_data_tools::starlight_approval::{
        ApprovalDecision, ApprovalFileDigest, ReviewerKind, APPROVAL_SCHEMA_VERSION,
    };
    use siderust::coordinates::cartesian::Direction;
    use siderust::coordinates::frames::Galactic;
    use siderust::healpix::{HealpixGrid, HealpixIndex, HealpixOrdering, Nside};

    const TEST_MODEL: &str = "synthetic_calibrated_photon_radiance_300_650nm_v1";

    #[test]
    fn packs_fixture_map_and_manifest() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("map.csv");
        let diagnostics = dir.path().join("diagnostics.json");
        let validation = dir.path().join("validation.json");
        let output = dir.path().join("map.release.csv");
        let manifest = dir.path().join("map.manifest.toml");
        std::fs::write(&input, production_map(TEST_MODEL, "candidate"))?;
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
            approval_root: None,
            release_id: None,
            nside_sweep_report: None,
            nside_review: None,
            missing_flux_approval: None,
            independent_validation_approval: None,
            redistribution_approval: None,
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
        let review = args
            .approval_root
            .as_ref()
            .expect("approval root")
            .join(args.nside_review.as_ref().expect("production review path"));
        let raw = std::fs::read_to_string(&review)?
            .replace("\"decision\": \"approved\"", "\"decision\": \"pending\"")
            .replace("\"production_use\": true", "\"production_use\": false");
        std::fs::write(&review, raw)?;
        let err = run(args).expect_err("unreviewed nside recommendation must fail closed");
        assert!(format!("{err:#}").contains("decision=approved and production_use=true"));
        Ok(())
    }

    #[test]
    fn production_rejects_legacy_nside_attestation() -> Result<()> {
        let (args, _dir) = fixture_args(production_validation(), true)?;
        let review = args
            .approval_root
            .as_ref()
            .expect("approval root")
            .join(args.nside_review.as_ref().expect("production review path"));
        std::fs::write(
            review,
            r#"{
  "schema_version": 2,
  "sweep_report_sha256": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "reviewed": true,
  "selected_nside": 8,
  "reviewer": "Legacy reviewer",
  "reviewed_at_utc": "2026-07-11T12:00:00Z",
  "rationale": "Legacy review deliberately lacks release and map bindings."
}
"#,
        )?;
        let error = run(args).expect_err("legacy nside review must fail closed");
        assert!(format!("{error:#}").contains("failed to parse approval artefact"));
        Ok(())
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
        assert!(rendered.contains("--approval-root"));
        assert!(rendered.contains("--release-id"));
        assert!(rendered.contains("--nside-sweep-report"));
        assert!(rendered.contains("--nside-review"));
        assert!(rendered.contains("--missing-flux-approval"));
        assert!(rendered.contains("--independent-validation-approval"));
        assert!(rendered.contains("--redistribution-approval"));
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
        assert!(err
            .to_string()
            .contains("non-proxy, non-placeholder photometry_model"));
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

    #[test]
    fn production_requires_independent_approval_to_bind_validation_file() -> Result<()> {
        let (args, _dir) = fixture_args(production_validation(), true)?;
        let root = args.approval_root.as_deref().expect("approval root");
        let path = root.join(
            args.independent_validation_approval
                .as_ref()
                .expect("independent approval"),
        );
        let mut approval: StarlightApproval = serde_json::from_slice(&std::fs::read(&path)?)?;
        approval.input_files[0] = file_entry(root, "license-inventory.json")?;
        std::fs::write(&path, serde_json::to_vec_pretty(&approval)?)?;
        let error = run(args).expect_err("unbound validation approval must fail closed");
        assert!(format!("{error:#}").contains("approval to --validation checksum binding"));
        Ok(())
    }

    #[test]
    fn production_rejects_approval_bound_to_another_map() -> Result<()> {
        let (args, _dir) = fixture_args(production_validation(), true)?;
        let root = args.approval_root.as_deref().expect("approval root");
        let path = root.join(
            args.missing_flux_approval
                .as_ref()
                .expect("missing-flux approval"),
        );
        let mut approval: StarlightApproval = serde_json::from_slice(&std::fs::read(&path)?)?;
        approval.map_sha256 = Some(format!("sha256:{}", "b".repeat(64)));
        std::fs::write(&path, serde_json::to_vec_pretty(&approval)?)?;
        let error = run(args).expect_err("approval for another map must fail closed");
        assert!(format!("{error:#}").contains("map_sha256 does not match"));
        Ok(())
    }

    #[test]
    fn supplied_manifest_approval_binding_must_match_output_manifest() -> Result<()> {
        let (args, _dir) = fixture_args(production_validation(), true)?;
        let root = args.approval_root.as_deref().expect("approval root");
        let path = root.join(
            args.redistribution_approval
                .as_ref()
                .expect("redistribution approval"),
        );
        let mut approval: StarlightApproval = serde_json::from_slice(&std::fs::read(&path)?)?;
        approval.manifest_sha256 = Some(format!("sha256:{}", "0".repeat(64)));
        std::fs::write(&path, serde_json::to_vec_pretty(&approval)?)?;
        let output = args.output.clone();
        let manifest = args.manifest.clone();
        let error = run(args).expect_err("wrong manifest binding must fail closed");
        assert!(format!("{error:#}").contains("manifest_sha256 does not match"));
        assert!(!output.exists());
        assert!(!manifest.exists());
        Ok(())
    }

    #[test]
    fn runtime_manifest_is_bound_to_exact_release_output() -> Result<()> {
        let (args, _dir) = fixture_args(production_validation(), true)?;
        let output = args.output.clone();
        let manifest_path = args.manifest.clone();
        run(args)?;
        let output_raw = std::fs::read(output)?;
        let mut manifest: RuntimeManifest =
            toml::from_str(&std::fs::read_to_string(manifest_path)?)?;
        verify_manifest_output_binding(&manifest, &output_raw)?;
        manifest.map_sha256 = format!("sha256:{}", "f".repeat(64));
        assert!(verify_manifest_output_binding(&manifest, &output_raw).is_err());
        Ok(())
    }

    #[test]
    fn production_rejects_old_336_650_band_even_with_passing_flags() -> Result<()> {
        let (args, _dir) = fixture_args(production_validation(), true)?;
        let raw = std::fs::read_to_string(&args.input)?.replace("300-650 nm", "336-650 nm");
        std::fs::write(&args.input, raw)?;
        write_production_diagnostics(&args.input, &args.diagnostics, TEST_MODEL)?;
        let error = run(args).expect_err("old partial band must fail closed");
        assert!(error.to_string().contains("validated 300-650 nm band"));
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
        let missing_flux = dir.path().join("missing-flux-approval.json");
        let independent = dir.path().join("independent-validation-approval.json");
        let redistribution = dir.path().join("redistribution-approval.json");
        std::fs::write(&input, production_map(TEST_MODEL, "production"))?;
        write_production_diagnostics(&input, &diagnostics, TEST_MODEL)?;
        std::fs::write(&validation, validation_raw)?;
        let sweep_raw = format!(
            r#"{{
  "schema_version": 2,
  "photometry_model": "{TEST_MODEL}",
  "band_nm": [300.0, 650.0],
  "recommended_candidate_nside": 8,
  "review_required": true,
  "summaries": [{{
    "nside": 8,
    "spectral_contract_pass": true,
    "eligible_for_candidate_recommendation": true
  }}]
}}
"#
        );
        std::fs::write(&sweep, &sweep_raw)?;
        for (name, raw) in [
            ("missing-model.json", b"missing model evidence\n".as_slice()),
            (
                "missing-report.json",
                b"missing report evidence\n".as_slice(),
            ),
            (
                "external-validation.json",
                b"external validation evidence\n".as_slice(),
            ),
            (
                "license-inventory.json",
                b"license inventory evidence\n".as_slice(),
            ),
            (
                "redistribution-report.json",
                b"redistribution evidence\n".as_slice(),
            ),
            ("nside-metrics.json", b"nside metrics evidence\n".as_slice()),
        ] {
            std::fs::write(dir.path().join(name), raw)?;
        }
        let map_sha = file_sha256(&input)?;
        write_approval(
            dir.path(),
            &missing_flux,
            ApprovalArtifactType::MissingFlux,
            None,
            Some(map_sha.clone()),
            &["missing-model.json"],
            &["missing-report.json"],
        )?;
        write_approval(
            dir.path(),
            &independent,
            ApprovalArtifactType::IndependentValidation,
            None,
            Some(map_sha.clone()),
            &["validation.json"],
            &["external-validation.json"],
        )?;
        write_approval(
            dir.path(),
            &redistribution,
            ApprovalArtifactType::Redistribution,
            None,
            None,
            &["license-inventory.json"],
            &["redistribution-report.json"],
        )?;
        write_approval(
            dir.path(),
            &review,
            ApprovalArtifactType::NsideReview,
            Some(8),
            Some(map_sha),
            &["nside-sweep.json"],
            &["nside-metrics.json"],
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
                approval_root: production.then(|| dir.path().to_path_buf()),
                release_id: production.then(|| "synthetic-release-v1".to_string()),
                nside_sweep_report: production.then_some(sweep),
                nside_review: production.then_some(PathBuf::from("nside-review.json")),
                missing_flux_approval: production
                    .then_some(PathBuf::from("missing-flux-approval.json")),
                independent_validation_approval: production
                    .then_some(PathBuf::from("independent-validation-approval.json")),
                redistribution_approval: production
                    .then_some(PathBuf::from("redistribution-approval.json")),
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
                "# band_definition=Synthetic calibrated passband-integrated 300-650 nm photon radiance\n",
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

    fn write_approval(
        root: &Path,
        path: &Path,
        artifact_type: ApprovalArtifactType,
        nside: Option<u32>,
        map_sha256: Option<String>,
        inputs: &[&str],
        outputs: &[&str],
    ) -> Result<()> {
        let approval = StarlightApproval {
            schema_version: APPROVAL_SCHEMA_VERSION,
            artifact_type,
            decision: ApprovalDecision::Approved,
            production_use: true,
            reviewer_kind: ReviewerKind::Human,
            reviewer_name: "Synthetic fixture maintainer".to_string(),
            date: "2026-07-11T12:00:00Z".to_string(),
            release_id: "synthetic-release-v1".to_string(),
            band_nm: STARLIGHT_PRODUCTION_BAND_NM,
            nside,
            map_sha256,
            manifest_sha256: None,
            input_files: inputs
                .iter()
                .map(|path| file_entry(root, path))
                .collect::<Result<_>>()?,
            output_files: outputs
                .iter()
                .map(|path| file_entry(root, path))
                .collect::<Result<_>>()?,
            rationale: format!(
                "Synthetic human fixture approval exercises the {} production gate.",
                artifact_type.as_str()
            ),
            references: vec!["synthetic-fixture-reference-v1".to_string()],
        };
        std::fs::write(
            path,
            format!("{}\n", serde_json::to_string_pretty(&approval)?),
        )?;
        Ok(())
    }

    fn file_entry(root: &Path, path: &str) -> Result<ApprovalFileDigest> {
        Ok(ApprovalFileDigest {
            path: path.to_string(),
            sha256: file_sha256(&root.join(path))?,
        })
    }

    fn file_sha256(path: &Path) -> Result<String> {
        Ok(format!(
            "sha256:{}",
            nsb_data_tools::checksum_io::sha256_file(path)?
        ))
    }
}
