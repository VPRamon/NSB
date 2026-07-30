//! Result, report, and artifact-manifest schemas emitted by `validation run`.

use super::metrics::MetricsSummary;
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

pub const VALIDATION_RESULTS_SCHEMA_VERSION: u32 = 1;
pub const VALIDATION_MANIFEST_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationResults {
    pub schema_version: u32,
    pub generated_at_unix_seconds: u64,
    pub issue: u32,
    pub preregistration_sha256: String,
    pub references_sha256: String,
    pub regions_sha256: String,
    pub candidate_map_path: String,
    pub candidate_map_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_map_pinned_sha256: Option<String>,
    pub band_nm: [u16; 2],
    pub flux_unit: String,
    pub region_ids: Vec<String>,
    pub reference_statuses: Vec<ReferenceRunStatus>,
    pub reference_results: Vec<ReferenceValidationResult>,
    pub technical_gates_passed: bool,
    pub technical_gate_failures: Vec<String>,
    /// Always `"pending"`. Set only by the human review recorded against #47.
    pub scientific_review_status: String,
    /// Always `false`. This pipeline never asserts scientific validation.
    pub scientifically_validated: bool,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceRunStatus {
    pub reference_id: String,
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceValidationResult {
    pub reference_id: String,
    pub region_metrics: Vec<RegionMetricsEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionMetricsEntry {
    pub region_id: String,
    pub metrics: MetricsSummary,
    pub tolerance_failures: Vec<String>,
}

impl ValidationResults {
    /// Never call this with `true`: the invariant is enforced structurally by
    /// always constructing results with this exact pair, but is asserted
    /// here too so a future refactor cannot silently change it.
    pub fn assert_never_scientifically_validated(&self) {
        assert!(!self.scientifically_validated);
        assert_eq!(self.scientific_review_status, "pending");
    }
}

pub fn render_markdown(results: &ValidationResults) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Starlight independent validation report");
    let _ = writeln!(out);
    let _ = writeln!(out, "- Issue: #{}", results.issue);
    let _ = writeln!(
        out,
        "- Generated (unix seconds): {}",
        results.generated_at_unix_seconds
    );
    let _ = writeln!(
        out,
        "- Band: {}-{} nm ({})",
        results.band_nm[0], results.band_nm[1], results.flux_unit
    );
    let _ = writeln!(out, "- Candidate map: `{}`", results.candidate_map_path);
    let _ = writeln!(
        out,
        "- Candidate map SHA-256: `{}`",
        results.candidate_map_sha256
    );
    match &results.candidate_map_pinned_sha256 {
        Some(pinned) => {
            let _ = writeln!(out, "- Pinned checksum verified against: `{pinned}`");
        }
        None => {
            let _ = writeln!(
                out,
                "- No checksum was pinned for this run; the candidate map identity above is reported but not independently cross-checked."
            );
        }
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "## Scientific review status");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "`scientific_review_status = \"{}\"`, `scientifically_validated = {}`. This pipeline never marks a candidate as scientifically validated on its own; that decision is recorded only by a qualified human scientist in issue #47.",
        results.scientific_review_status, results.scientifically_validated
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "## Technical gates");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "`technical_gates_passed = {}`",
        results.technical_gates_passed
    );
    if results.technical_gate_failures.is_empty() {
        let _ = writeln!(out, "\nNo gate failures recorded.");
    } else {
        let _ = writeln!(out);
        for failure in &results.technical_gate_failures {
            let _ = writeln!(out, "- {failure}");
        }
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "## Reference status");
    let _ = writeln!(out);
    let _ = writeln!(out, "| Reference | Status | Detail |");
    let _ = writeln!(out, "|---|---|---|");
    for status in &results.reference_statuses {
        let _ = writeln!(
            out,
            "| {} | {} | {} |",
            status.reference_id, status.status, status.detail
        );
    }
    if results.reference_results.is_empty() {
        let _ = writeln!(
            out,
            "\nNo reference produced computed metrics in this run: all references are pending acquisition, or acquired but not yet transformed onto the candidate grid. No metrics were invented to fill this gap."
        );
    } else {
        for reference in &results.reference_results {
            let _ = writeln!(out);
            let _ = writeln!(out, "## Metrics for reference `{}`", reference.reference_id);
            for region in &reference.region_metrics {
                let _ = writeln!(out);
                let _ = writeln!(out, "### Region `{}`", region.region_id);
                let _ = writeln!(out);
                let metrics = &region.metrics;
                let _ = writeln!(out, "| Metric | Value |");
                let _ = writeln!(out, "|---|---|");
                let _ = writeln!(out, "| sample_count | {} |", metrics.sample_count);
                let _ = writeln!(out, "| signed_bias | {:.6e} |", metrics.signed_bias);
                let _ = writeln!(out, "| absolute_bias | {:.6e} |", metrics.absolute_bias);
                let _ = writeln!(out, "| relative_bias | {:.6} |", metrics.relative_bias);
                let _ = writeln!(out, "| mae | {:.6e} |", metrics.mae);
                let _ = writeln!(
                    out,
                    "| median_absolute_error | {:.6e} |",
                    metrics.median_absolute_error
                );
                let _ = writeln!(out, "| rmse | {:.6e} |", metrics.rmse);
                let _ = writeln!(
                    out,
                    "| relative_error_p50 | {:.6} |",
                    metrics.relative_error_p50
                );
                let _ = writeln!(
                    out,
                    "| relative_error_p68 | {:.6} |",
                    metrics.relative_error_p68
                );
                let _ = writeln!(
                    out,
                    "| relative_error_p95 | {:.6} |",
                    metrics.relative_error_p95
                );
                let _ = writeln!(out, "| coverage_68 | {:.6} |", metrics.coverage_68);
                let _ = writeln!(out, "| coverage_95 | {:.6} |", metrics.coverage_95);
                let _ = writeln!(
                    out,
                    "| outlier_fraction | {:.6} |",
                    metrics.outlier_fraction
                );
                if !region.tolerance_failures.is_empty() {
                    let _ = writeln!(out);
                    let _ = writeln!(out, "Tolerance failures:");
                    for failure in &region.tolerance_failures {
                        let _ = writeln!(out, "- {failure}");
                    }
                }
            }
        }
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "## Notes");
    let _ = writeln!(out);
    let _ = writeln!(out, "{}", results.notes);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_report_never_claims_scientific_validation() {
        let results = ValidationResults {
            schema_version: VALIDATION_RESULTS_SCHEMA_VERSION,
            generated_at_unix_seconds: 0,
            issue: 87,
            preregistration_sha256: "a".repeat(64),
            references_sha256: "a".repeat(64),
            regions_sha256: "a".repeat(64),
            candidate_map_path: "crates/nsb/data/starlight_nside128.csv".to_string(),
            candidate_map_sha256: "a".repeat(64),
            candidate_map_pinned_sha256: None,
            band_nm: [300, 650],
            flux_unit: "ph_m-2_s-1".to_string(),
            region_ids: vec!["all-sky".to_string()],
            reference_statuses: vec![ReferenceRunStatus {
                reference_id: "example".to_string(),
                status: "pending-acquisition".to_string(),
                detail: "not yet acquired".to_string(),
            }],
            reference_results: vec![],
            technical_gates_passed: false,
            technical_gate_failures: vec!["no acquired reference data available".to_string()],
            scientific_review_status: "pending".to_string(),
            scientifically_validated: false,
            notes: "technical scaffolding".to_string(),
        };
        results.assert_never_scientifically_validated();
        let markdown = render_markdown(&results);
        assert!(markdown.contains("technical_gates_passed = false"));
        assert!(markdown.contains("scientifically_validated = false"));
        assert!(!markdown.contains("scientifically_validated = true"));
    }
}
