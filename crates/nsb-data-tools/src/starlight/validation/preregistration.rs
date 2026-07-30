//! Frozen preregistration for the independent Starlight validation pipeline
//! (GitHub issue #87). Tolerances here are fixed before any candidate map is
//! compared against real reference data, so they cannot be adjusted after
//! seeing results.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

pub const PREREGISTRATION_SCHEMA_VERSION: u32 = 1;
pub const TARGET_BAND_NM: [u16; 2] = [300, 650];
pub const TARGET_FLUX_UNIT: &str = "ph_m-2_s-1";
pub const EXPECTED_CANDIDATE_MAP_PATH: &str = "crates/nsb/data/starlight_nside128.csv";

pub const REQUIRED_METRICS: [&str; 12] = [
    "signed_bias",
    "absolute_bias",
    "relative_bias",
    "mae",
    "median_absolute_error",
    "rmse",
    "relative_error_p50",
    "relative_error_p68",
    "relative_error_p95",
    "coverage_68",
    "coverage_95",
    "outlier_fraction",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preregistration {
    pub schema_version: u32,
    pub issue: u32,
    pub title: String,
    pub candidate: CandidateIdentity,
    pub band_nm: [u16; 2],
    pub flux_unit: String,
    pub metrics: Vec<String>,
    pub tolerances: Tolerances,
    pub exclusion_rules: Vec<ExclusionRule>,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateIdentity {
    pub map_path: String,
    pub map_schema: String,
    pub checksum_pinning_status: String,
    pub checksum_note: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tolerances {
    pub absolute_all_sky_bias_max: f64,
    pub median_absolute_regional_relative_error_max: f64,
    pub regional_relative_error_p95_max: f64,
    pub coverage_68_min: f64,
    pub coverage_68_max: f64,
    pub coverage_95_min: f64,
    pub coverage_95_max: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExclusionRule {
    pub id: String,
    pub description: String,
    pub status: ExclusionRuleStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExclusionRuleStatus {
    Placeholder,
    Active,
}

impl Preregistration {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != PREREGISTRATION_SCHEMA_VERSION {
            bail!(
                "unsupported Starlight validation preregistration schema_version {}",
                self.schema_version
            );
        }
        if self.issue != 87 {
            bail!(
                "preregistration must be pinned to issue 87, found {}",
                self.issue
            );
        }
        require_text("title", &self.title)?;
        if self.band_nm != TARGET_BAND_NM {
            bail!(
                "preregistration band must be exactly {:?} nm",
                TARGET_BAND_NM
            );
        }
        if self.flux_unit != TARGET_FLUX_UNIT {
            bail!("preregistration flux_unit must be {TARGET_FLUX_UNIT}");
        }
        self.candidate.validate()?;
        let declared_metrics = self.metrics.iter().map(String::as_str).collect::<Vec<_>>();
        for required in REQUIRED_METRICS {
            if !declared_metrics.contains(&required) {
                bail!("preregistration is missing required metric {required}");
            }
        }
        self.tolerances.validate()?;
        if self.exclusion_rules.is_empty() {
            bail!(
                "preregistration must declare at least one exclusion rule, even if a placeholder"
            );
        }
        for rule in &self.exclusion_rules {
            require_text("exclusion rule id", &rule.id)?;
            require_text("exclusion rule description", &rule.description)?;
        }
        require_text("notes", &self.notes)?;
        Ok(())
    }
}

impl CandidateIdentity {
    fn validate(&self) -> Result<()> {
        if self.map_path != EXPECTED_CANDIDATE_MAP_PATH {
            bail!(
                "preregistration candidate map_path must be pinned to {EXPECTED_CANDIDATE_MAP_PATH}, found {}",
                self.map_path
            );
        }
        require_text("candidate map_schema", &self.map_schema)?;
        require_text(
            "candidate checksum_pinning_status",
            &self.checksum_pinning_status,
        )?;
        require_text("candidate checksum_note", &self.checksum_note)?;
        Ok(())
    }
}

impl Tolerances {
    fn validate(&self) -> Result<()> {
        let fractions = [
            self.absolute_all_sky_bias_max,
            self.median_absolute_regional_relative_error_max,
            self.regional_relative_error_p95_max,
            self.coverage_68_min,
            self.coverage_68_max,
            self.coverage_95_min,
            self.coverage_95_max,
        ];
        if fractions
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
        {
            bail!("preregistration tolerances must be finite and non-negative");
        }
        if self.coverage_68_min >= self.coverage_68_max || self.coverage_68_max > 1.0 {
            bail!("preregistration coverage_68 bounds are invalid");
        }
        if self.coverage_95_min >= self.coverage_95_max || self.coverage_95_max > 1.0 {
            bail!("preregistration coverage_95 bounds are invalid");
        }
        Ok(())
    }
}

fn require_text(label: &str, value: &str) -> Result<()> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty()
        || ["placeholder", "todo", "tbd", "unknown", "unspecified"]
            .iter()
            .any(|marker| normalized == *marker)
    {
        bail!("{label} is missing or contains a placeholder");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> Preregistration {
        Preregistration {
            schema_version: PREREGISTRATION_SCHEMA_VERSION,
            issue: 87,
            title: "Starlight independent validation preregistration".to_string(),
            candidate: CandidateIdentity {
                map_path: EXPECTED_CANDIDATE_MAP_PATH.to_string(),
                map_schema: "nsb-healpix-starlight-candidate-v5".to_string(),
                checksum_pinning_status: "pending-regeneration-after-94".to_string(),
                checksum_note:
                    "checksum may change after the #94 uncertainty audit regenerates the map"
                        .to_string(),
            },
            band_nm: TARGET_BAND_NM,
            flux_unit: TARGET_FLUX_UNIT.to_string(),
            metrics: REQUIRED_METRICS
                .iter()
                .map(|value| value.to_string())
                .collect(),
            tolerances: Tolerances {
                absolute_all_sky_bias_max: 0.03,
                median_absolute_regional_relative_error_max: 0.05,
                regional_relative_error_p95_max: 0.10,
                coverage_68_min: 0.63,
                coverage_68_max: 0.73,
                coverage_95_min: 0.90,
                coverage_95_max: 0.98,
            },
            exclusion_rules: vec![ExclusionRule {
                id: "placeholder-outlier-rule".to_string(),
                description: "reserved for a future catastrophic-outlier exclusion rule"
                    .to_string(),
                status: ExclusionRuleStatus::Placeholder,
            }],
            notes: "technical scaffolding only; scientific review deferred to #47".to_string(),
        }
    }

    #[test]
    fn valid_document_passes() {
        valid().validate().unwrap();
    }

    #[test]
    fn wrong_issue_or_band_is_rejected() {
        let mut document = valid();
        document.issue = 94;
        assert!(document.validate().is_err());
        let mut document = valid();
        document.band_nm = [300, 336];
        assert!(document.validate().is_err());
    }

    #[test]
    fn wrong_candidate_map_path_is_rejected() {
        let mut document = valid();
        document.candidate.map_path = "somewhere/else.csv".to_string();
        assert!(document.validate().is_err());
    }

    #[test]
    fn missing_required_metric_is_rejected() {
        let mut document = valid();
        document.metrics.retain(|metric| metric != "coverage_95");
        assert!(document.validate().is_err());
    }

    #[test]
    fn invalid_coverage_bounds_are_rejected() {
        let mut document = valid();
        document.tolerances.coverage_68_min = 0.8;
        document.tolerances.coverage_68_max = 0.7;
        assert!(document.validate().is_err());
    }

    #[test]
    fn empty_exclusion_rules_are_rejected() {
        let mut document = valid();
        document.exclusion_rules.clear();
        assert!(document.validate().is_err());
    }
}
