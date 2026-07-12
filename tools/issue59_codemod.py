#!/usr/bin/env python3
"""One-shot source transformation for issue #59.

This file is removed by the workflow after the generated patch passes formatting
and compilation checks. It intentionally uses only the Python standard library.
"""

from __future__ import annotations

import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CRATE = ROOT / "crates" / "nsb-data-tools"
SRC = CRATE / "src"


def write(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content.rstrip() + "\n", encoding="utf-8")


def patch_lib() -> None:
    path = SRC / "lib.rs"
    text = path.read_text(encoding="utf-8")
    for module in ("artifact_io", "provenance", "scientific_contract"):
        line = f"pub mod {module};\n"
        if line not in text:
            text = text.replace("pub mod checksum_io;\n", f"pub mod {module};\n" + "pub mod checksum_io;\n", 1)
    path.write_text(text, encoding="utf-8")


def remove_function(text: str, name: str) -> tuple[str, bool]:
    pattern = re.compile(
        rf"(?m)^(?:\s*#\[[^\n]+\]\n)*(?:\s*///[^\n]*\n)*\s*(?:pub(?:\([^)]*\))?\s+)?fn\s+{re.escape(name)}\s*\("
    )
    match = pattern.search(text)
    if not match:
        return text, False
    brace = text.find("{", match.end())
    if brace < 0:
        raise RuntimeError(f"could not find body for {name}")
    depth = 0
    in_string = False
    escaped = False
    in_char = False
    line_comment = False
    block_comment = 0
    index = brace
    while index < len(text):
        char = text[index]
        nxt = text[index + 1] if index + 1 < len(text) else ""
        if line_comment:
            if char == "\n":
                line_comment = False
            index += 1
            continue
        if block_comment:
            if char == "/" and nxt == "*":
                block_comment += 1
                index += 2
                continue
            if char == "*" and nxt == "/":
                block_comment -= 1
                index += 2
                continue
            index += 1
            continue
        if in_string:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                in_string = False
            index += 1
            continue
        if in_char:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == "'":
                in_char = False
            index += 1
            continue
        if char == "/" and nxt == "/":
            line_comment = True
            index += 2
            continue
        if char == "/" and nxt == "*":
            block_comment = 1
            index += 2
            continue
        if char == '"':
            in_string = True
            index += 1
            continue
        if char == "'":
            # Lifetimes occur outside function bodies only rarely here; inside a
            # body an apostrophe followed by an identifier is a lifetime.
            if nxt.isalpha() or nxt == "_":
                index += 1
                continue
            in_char = True
            index += 1
            continue
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                end = index + 1
                while end < len(text) and text[end] in " \t":
                    end += 1
                if end < len(text) and text[end] == "\n":
                    end += 1
                return text[: match.start()] + text[end:], True
        index += 1
    raise RuntimeError(f"unterminated body for {name}")


def insert_import(text: str, import_line: str) -> str:
    if import_line.strip() in text:
        return text
    lines = text.splitlines(keepends=True)
    insert_at = 0
    while insert_at < len(lines):
        stripped = lines[insert_at].lstrip()
        if stripped.startswith("//!") or stripped.startswith("#!") or not stripped.strip():
            insert_at += 1
            continue
        break
    lines.insert(insert_at, import_line)
    return "".join(lines)


def consolidate_sha256_helpers() -> list[str]:
    changed: list[str] = []
    for path in sorted(SRC.rglob("*.rs")):
        if path.name == "checksum_io.rs":
            continue
        text = path.read_text(encoding="utf-8")
        updated, removed = remove_function(text, "sha256_file")
        if not removed:
            continue
        if path.parent.name == "bin":
            updated = insert_import(
                updated, "use nsb_data_tools::checksum_io::sha256_file;\n"
            )
        else:
            updated = insert_import(updated, "use crate::checksum_io::sha256_file;\n")
        path.write_text(updated, encoding="utf-8")
        changed.append(str(path.relative_to(ROOT)))
    return changed


def main() -> None:
    contract = {
        "schema_version": 1,
        "contract_id": "gaia_dr3_xp_photon_integration_v1",
        "band": {
            "min_nm": 336.0,
            "max_nm": 650.0,
            "boundary_policy": "inclusive_exact_samples",
        },
        "sampled_grid": {
            "start_nm": 336.0,
            "end_nm": 1020.0,
            "step_nm": 2.0,
            "length": 343,
            "band_start_index": 0,
            "band_end_index": 157,
        },
        "integration": {
            "owner": "nsb-data-tools::gaia_xp::integrate_photon_flux",
            "rule": "trapezoidal_signed",
            "photon_energy_model": "planck_times_c_over_wavelength",
            "negative_finite_samples": "retain",
            "non_finite_samples": "reject",
            "uncertainty": "independent_sample_errors_weighted_by_trapezoid_coefficients",
        },
        "identifiers": {
            "sampled_photometry_model": "gaia_dr3_xp_photon_radiance_336_650nm_v1",
            "continuous_photometry_model": "gaia_dr3_xp_continuous_reconstructed_336_650nm_v1",
            "photon_flux_column": "photon_flux_336_650_ph_m2_s",
            "wavelength_column": "xp_wavelength_nm",
            "flux_column": "xp_flux_w_m2_nm",
            "flux_error_column": "xp_flux_error_w_m2_nm",
        },
        "parity_tolerances": {
            "spectral_flux_relative": 1.0e-8,
            "integrated_flux_relative": 1.0e-8,
            "integrated_uncertainty_relative": 1.0e-6,
            "absolute_floor": 1.0e-30,
        },
    }
    write(
        CRATE / "contracts" / "gaia_xp_photon_integration_v1.json",
        json.dumps(contract, indent=2),
    )

    write(SRC / "scientific_contract.rs", SCIENTIFIC_CONTRACT_RS)
    write(SRC / "artifact_io.rs", ARTIFACT_IO_RS)
    write(SRC / "provenance.rs", PROVENANCE_RS)
    write(SRC / "checksum_io.rs", CHECKSUM_IO_RS)
    write(SRC / "gaia_xp_continuous.rs", GAIA_XP_CONTINUOUS_RS)
    write(
        ROOT / "tools" / "starlight-xp-continuous" / "reconstruct_and_integrate.py",
        RECONSTRUCT_PY,
    )
    write(
        ROOT / "tools" / "starlight-xp-continuous" / "phase5b_gaiaxpy_flux_validate.py",
        VALIDATE_PY,
    )
    write(CRATE / "tests" / "deduplication_contract.rs", DEDUP_TEST_RS)
    write(ROOT / "docs" / "DUPLICATION_REGISTER.md", DUPLICATION_REGISTER_MD)
    patch_lib()
    changed = consolidate_sha256_helpers()
    print("consolidated local sha256 helpers:")
    for path in changed:
        print(f"  {path}")


SCIENTIFIC_CONTRACT_RS = r'''
//! Versioned machine-readable scientific contracts shared with migration tools.
//!
//! Rust remains the sole production implementation of Gaia XP photon-flux
//! integration. The committed JSON is generated from the Rust constants and is
//! consumed by temporary GaiaXPy reference scripts so they cannot independently
//! redefine band edges, grids, column names, model identifiers or tolerances.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

use crate::gaia_xp::{
    BAND_MAX_NM, BAND_MIN_NM, NORMALIZED_FLUX_COLUMN, NORMALIZED_FLUX_ERROR_COLUMN,
    NORMALIZED_WAVELENGTH_COLUMN, PHOTOMETRY_MODEL as SAMPLED_PHOTOMETRY_MODEL,
    PHOTON_FLUX_COLUMN, XP_SAMPLED_BAND_END_INDEX, XP_SAMPLED_BAND_START_INDEX,
    XP_SAMPLED_GRID_END_NM, XP_SAMPLED_GRID_LEN, XP_SAMPLED_GRID_START_NM,
    XP_SAMPLED_GRID_STEP_NM,
};
use crate::gaia_xp_continuous::PHOTOMETRY_MODEL as CONTINUOUS_PHOTOMETRY_MODEL;

/// Supported schema version for the Gaia XP photon-integration contract.
pub const GAIA_XP_PHOTON_CONTRACT_SCHEMA_VERSION: u32 = 1;
/// Stable identifier for the Gaia XP photon-integration contract.
pub const GAIA_XP_PHOTON_CONTRACT_ID: &str = "gaia_dr3_xp_photon_integration_v1";
/// Embedded generated contract consumed by Rust tests and migration scripts.
pub const GAIA_XP_PHOTON_CONTRACT_JSON: &str =
    include_str!("../contracts/gaia_xp_photon_integration_v1.json");

/// Inclusive wavelength band used by Gaia XP sampled and continuous products.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BandContract {
    /// Inclusive lower wavelength in nanometres.
    pub min_nm: f64,
    /// Inclusive upper wavelength in nanometres.
    pub max_nm: f64,
    /// Required boundary selection policy.
    pub boundary_policy: String,
}

/// Official implicit Gaia XP sampled grid and in-band indices.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SampledGridContract {
    /// First wavelength in nanometres.
    pub start_nm: f64,
    /// Last wavelength in nanometres.
    pub end_nm: f64,
    /// Uniform wavelength step in nanometres.
    pub step_nm: f64,
    /// Number of samples on the complete grid.
    pub length: usize,
    /// Inclusive first in-band index.
    pub band_start_index: usize,
    /// Inclusive last in-band index.
    pub band_end_index: usize,
}

/// Integration ownership and numerical-policy identifiers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationContract {
    /// Authoritative production implementation.
    pub owner: String,
    /// Signed numerical integration rule.
    pub rule: String,
    /// Photon-energy conversion model identifier.
    pub photon_energy_model: String,
    /// Policy for finite negative samples.
    pub negative_finite_samples: String,
    /// Policy for NaN and infinite samples.
    pub non_finite_samples: String,
    /// Statistical uncertainty propagation model.
    pub uncertainty: String,
}

/// Stable model and column identifiers exchanged between stages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractIdentifiers {
    /// Gaia XP sampled model identifier.
    pub sampled_photometry_model: String,
    /// Gaia XP continuous reconstructed model identifier.
    pub continuous_photometry_model: String,
    /// Integrated photon-flux column.
    pub photon_flux_column: String,
    /// Normalized wavelength column.
    pub wavelength_column: String,
    /// Normalized flux column.
    pub flux_column: String,
    /// Normalized flux-error column.
    pub flux_error_column: String,
}

/// Frozen comparison tolerances for migration-oracle evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParityTolerances {
    /// Relative tolerance for per-sample flux parity.
    pub spectral_flux_relative: f64,
    /// Relative tolerance for integrated flux parity.
    pub integrated_flux_relative: f64,
    /// Relative tolerance for integrated uncertainty parity.
    pub integrated_uncertainty_relative: f64,
    /// Absolute denominator floor for relative comparisons.
    pub absolute_floor: f64,
}

/// Complete versioned Gaia XP photon-integration contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GaiaXpPhotonIntegrationContract {
    /// Contract schema version.
    pub schema_version: u32,
    /// Stable contract identifier.
    pub contract_id: String,
    /// Inclusive integration band.
    pub band: BandContract,
    /// Official sampled grid.
    pub sampled_grid: SampledGridContract,
    /// Numerical policy and production owner.
    pub integration: IntegrationContract,
    /// Stable identifiers.
    pub identifiers: ContractIdentifiers,
    /// Frozen migration parity tolerances.
    pub parity_tolerances: ParityTolerances,
}

impl GaiaXpPhotonIntegrationContract {
    /// Validate schema, numerical consistency and fail-closed policy identifiers.
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != GAIA_XP_PHOTON_CONTRACT_SCHEMA_VERSION {
            bail!(
                "unsupported Gaia XP scientific contract schema {}; expected {}",
                self.schema_version,
                GAIA_XP_PHOTON_CONTRACT_SCHEMA_VERSION
            );
        }
        if self.contract_id != GAIA_XP_PHOTON_CONTRACT_ID {
            bail!("unsupported Gaia XP scientific contract id {:?}", self.contract_id);
        }
        if !self.band.min_nm.is_finite()
            || !self.band.max_nm.is_finite()
            || self.band.min_nm >= self.band.max_nm
            || self.band.boundary_policy != "inclusive_exact_samples"
        {
            bail!("invalid Gaia XP integration-band contract");
        }
        let grid = &self.sampled_grid;
        if !grid.start_nm.is_finite()
            || !grid.end_nm.is_finite()
            || !grid.step_nm.is_finite()
            || grid.step_nm <= 0.0
            || grid.length < 2
            || grid.band_start_index >= grid.band_end_index
            || grid.band_end_index >= grid.length
        {
            bail!("invalid Gaia XP sampled-grid contract");
        }
        let derived_end = grid.start_nm + grid.step_nm * (grid.length - 1) as f64;
        if (derived_end - grid.end_nm).abs() > 1.0e-12 {
            bail!("Gaia XP sampled-grid end is inconsistent with start/step/length");
        }
        let band_start = grid.start_nm + grid.step_nm * grid.band_start_index as f64;
        let band_end = grid.start_nm + grid.step_nm * grid.band_end_index as f64;
        if (band_start - self.band.min_nm).abs() > 1.0e-12
            || (band_end - self.band.max_nm).abs() > 1.0e-12
        {
            bail!("Gaia XP band indices are inconsistent with the wavelength band");
        }
        if self.integration.owner != "nsb-data-tools::gaia_xp::integrate_photon_flux"
            || self.integration.rule != "trapezoidal_signed"
            || self.integration.photon_energy_model != "planck_times_c_over_wavelength"
            || self.integration.negative_finite_samples != "retain"
            || self.integration.non_finite_samples != "reject"
            || self.integration.uncertainty
                != "independent_sample_errors_weighted_by_trapezoid_coefficients"
        {
            bail!("unsupported Gaia XP integration policy");
        }
        for (name, tolerance) in [
            ("spectral_flux_relative", self.parity_tolerances.spectral_flux_relative),
            ("integrated_flux_relative", self.parity_tolerances.integrated_flux_relative),
            (
                "integrated_uncertainty_relative",
                self.parity_tolerances.integrated_uncertainty_relative,
            ),
            ("absolute_floor", self.parity_tolerances.absolute_floor),
        ] {
            if !tolerance.is_finite() || tolerance <= 0.0 {
                bail!("invalid Gaia XP parity tolerance {name}");
            }
        }
        Ok(())
    }
}

/// Parse and strictly validate a versioned Gaia XP scientific contract.
pub fn parse_gaia_xp_photon_contract(raw: &str) -> Result<GaiaXpPhotonIntegrationContract> {
    let contract: GaiaXpPhotonIntegrationContract =
        serde_json::from_str(raw).context("invalid Gaia XP scientific contract JSON")?;
    contract.validate()?;
    Ok(contract)
}

/// Return the embedded generated contract after strict validation.
pub fn gaia_xp_photon_contract() -> &'static GaiaXpPhotonIntegrationContract {
    static CONTRACT: OnceLock<GaiaXpPhotonIntegrationContract> = OnceLock::new();
    CONTRACT.get_or_init(|| {
        parse_gaia_xp_photon_contract(GAIA_XP_PHOTON_CONTRACT_JSON)
            .expect("embedded Gaia XP scientific contract must be valid")
    })
}

/// Build the authoritative contract from production Rust constants.
pub fn authoritative_gaia_xp_photon_contract() -> GaiaXpPhotonIntegrationContract {
    GaiaXpPhotonIntegrationContract {
        schema_version: GAIA_XP_PHOTON_CONTRACT_SCHEMA_VERSION,
        contract_id: GAIA_XP_PHOTON_CONTRACT_ID.to_string(),
        band: BandContract {
            min_nm: BAND_MIN_NM,
            max_nm: BAND_MAX_NM,
            boundary_policy: "inclusive_exact_samples".to_string(),
        },
        sampled_grid: SampledGridContract {
            start_nm: XP_SAMPLED_GRID_START_NM,
            end_nm: XP_SAMPLED_GRID_END_NM,
            step_nm: XP_SAMPLED_GRID_STEP_NM,
            length: XP_SAMPLED_GRID_LEN,
            band_start_index: XP_SAMPLED_BAND_START_INDEX,
            band_end_index: XP_SAMPLED_BAND_END_INDEX,
        },
        integration: IntegrationContract {
            owner: "nsb-data-tools::gaia_xp::integrate_photon_flux".to_string(),
            rule: "trapezoidal_signed".to_string(),
            photon_energy_model: "planck_times_c_over_wavelength".to_string(),
            negative_finite_samples: "retain".to_string(),
            non_finite_samples: "reject".to_string(),
            uncertainty:
                "independent_sample_errors_weighted_by_trapezoid_coefficients".to_string(),
        },
        identifiers: ContractIdentifiers {
            sampled_photometry_model: SAMPLED_PHOTOMETRY_MODEL.to_string(),
            continuous_photometry_model: CONTINUOUS_PHOTOMETRY_MODEL.to_string(),
            photon_flux_column: PHOTON_FLUX_COLUMN.to_string(),
            wavelength_column: NORMALIZED_WAVELENGTH_COLUMN.to_string(),
            flux_column: NORMALIZED_FLUX_COLUMN.to_string(),
            flux_error_column: NORMALIZED_FLUX_ERROR_COLUMN.to_string(),
        },
        parity_tolerances: ParityTolerances {
            spectral_flux_relative: 1.0e-8,
            integrated_flux_relative: 1.0e-8,
            integrated_uncertainty_relative: 1.0e-6,
            absolute_floor: 1.0e-30,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_contract_matches_production_rust_authority() {
        assert_eq!(
            gaia_xp_photon_contract(),
            &authoritative_gaia_xp_photon_contract()
        );
    }

    #[test]
    fn corrupted_contract_version_fails_closed() {
        let mut value: serde_json::Value =
            serde_json::from_str(GAIA_XP_PHOTON_CONTRACT_JSON).unwrap();
        value["schema_version"] = serde_json::json!(999);
        let error = parse_gaia_xp_photon_contract(&value.to_string()).expect_err("version");
        assert!(error.to_string().contains("unsupported"));
    }

    #[test]
    fn corrupted_grid_fails_closed() {
        let mut value: serde_json::Value =
            serde_json::from_str(GAIA_XP_PHOTON_CONTRACT_JSON).unwrap();
        value["sampled_grid"]["band_end_index"] = serde_json::json!(156);
        let error = parse_gaia_xp_photon_contract(&value.to_string()).expect_err("grid drift");
        assert!(error.to_string().contains("inconsistent"));
    }
}
'''

ARTIFACT_IO_RS = r'''
//! Transactional persistence helpers for manifests and generated artefacts.

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};

fn part_path(path: &Path) -> Result<PathBuf> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .context("atomic output path must have a UTF-8 file name")?;
    Ok(path.with_file_name(format!(".{name}.part")))
}

/// Write bytes through a flushed sibling temporary file and atomically rename it.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let part = part_path(path)?;
    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&part)
            .with_context(|| format!("failed to create {}", part.display()))?;
        file.write_all(bytes)
            .with_context(|| format!("failed to write {}", part.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to flush {}", part.display()))?;
        fs::rename(&part, path).with_context(|| {
            format!(
                "failed to atomically promote {} to {}",
                part.display(),
                path.display()
            )
        })?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&part);
    }
    result
}

/// Serialize a value as pretty JSON with a trailing newline and persist atomically.
pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value).context("failed to serialize JSON artefact")?;
    bytes.push(b'\n');
    write_atomic(path, &bytes)
}

/// Strictly deserialize a typed JSON artefact from disk.
pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    serde_json::from_reader(BufReader::new(file))
        .with_context(|| format!("failed to parse typed JSON artefact {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fixture {
        schema_version: u32,
        value: String,
    }

    #[test]
    fn atomic_json_roundtrip() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("nested/report.json");
        let expected = Fixture {
            schema_version: 1,
            value: "ok".to_string(),
        };
        write_json_atomic(&path, &expected)?;
        assert_eq!(read_json::<Fixture>(&path)?, expected);
        assert!(!dir.path().join("nested/.report.json.part").exists());
        Ok(())
    }

    #[test]
    fn strict_json_rejects_unknown_fields() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("report.json");
        write_atomic(
            &path,
            br#"{"schema_version":1,"value":"ok","unexpected":true}"#,
        )?;
        assert!(read_json::<Fixture>(&path).is_err());
        Ok(())
    }
}
'''

PROVENANCE_RS = r'''
//! Canonical software-version and timestamp resolution for generated artefacts.

use anyhow::{bail, Result};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::process::Command;

/// Versioned execution provenance shared by generated reports and manifests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionProvenance {
    /// Provenance schema version.
    pub schema_version: u32,
    /// Git commit, or `unknown` only for non-production developer runs.
    pub software_commit: String,
    /// UTC generation timestamp in RFC3339 seconds form.
    pub generated_at_utc: String,
}

impl ExecutionProvenance {
    /// Capture canonical provenance using the shared resolution order.
    pub fn capture() -> Self {
        Self {
            schema_version: 1,
            software_commit: resolve_software_commit(),
            generated_at_utc: utc_now_rfc3339_seconds(),
        }
    }

    /// Reject incomplete provenance before production admission.
    pub fn validate_for_production(&self) -> Result<()> {
        if self.schema_version != 1 {
            bail!("unsupported execution provenance schema {}", self.schema_version);
        }
        if self.software_commit == "unknown" || self.software_commit.trim().is_empty() {
            bail!("production execution provenance requires a known software commit");
        }
        if chrono::DateTime::parse_from_rfc3339(&self.generated_at_utc).is_err() {
            bail!("execution provenance timestamp must be RFC3339");
        }
        Ok(())
    }
}

/// Return the current UTC timestamp with deterministic second precision.
pub fn utc_now_rfc3339_seconds() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// Resolve the software commit consistently across local and CI execution.
///
/// Resolution order is `NSB_SOFTWARE_COMMIT`, `GITHUB_SHA`, local
/// `git rev-parse HEAD`, then `unknown` for candidate/developer operation.
pub fn resolve_software_commit() -> String {
    resolve_software_commit_from(
        std::env::var("NSB_SOFTWARE_COMMIT").ok().as_deref(),
        std::env::var("GITHUB_SHA").ok().as_deref(),
        git_head().as_deref(),
    )
}

fn git_head() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

fn resolve_software_commit_from(
    explicit: Option<&str>,
    github: Option<&str>,
    git: Option<&str>,
) -> String {
    [explicit, github, git]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|value| !value.is_empty())
        .unwrap_or("unknown")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolution_order_is_stable() {
        assert_eq!(
            resolve_software_commit_from(Some(" explicit "), Some("github"), Some("git")),
            "explicit"
        );
        assert_eq!(
            resolve_software_commit_from(None, Some(" github "), Some("git")),
            "github"
        );
        assert_eq!(resolve_software_commit_from(None, None, None), "unknown");
    }

    #[test]
    fn production_rejects_unknown_commit() {
        let provenance = ExecutionProvenance {
            schema_version: 1,
            software_commit: "unknown".to_string(),
            generated_at_utc: "2026-07-12T16:00:00Z".to_string(),
        };
        assert!(provenance.validate_for_production().is_err());
    }
}
'''

CHECKSUM_IO_RS = r'''
//! Typed, streaming checksums for maintainer artefacts and official inventories.

use anyhow::{bail, Context, Result};
use md5::Md5;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use siderust::checksum::to_hex;
use std::fmt;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::str::FromStr;

const BUFFER_LEN: usize = 1024 * 1024;

/// Supported checksum algorithms with explicit provenance semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChecksumAlgorithm {
    /// MD5, retained only for verification of official Gaia inventories.
    Md5,
    /// SHA-256 for NSB-generated provenance and artefacts.
    Sha256,
}

impl ChecksumAlgorithm {
    /// Canonical lowercase algorithm identifier.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Md5 => "md5",
            Self::Sha256 => "sha256",
        }
    }

    const fn hex_len(self) -> usize {
        match self {
            Self::Md5 => 32,
            Self::Sha256 => 64,
        }
    }
}

/// Algorithm-qualified, validated lowercase hexadecimal checksum.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Checksum {
    algorithm: ChecksumAlgorithm,
    hex: String,
}

impl Checksum {
    /// Construct and validate a checksum value.
    pub fn new(algorithm: ChecksumAlgorithm, hex: impl Into<String>) -> Result<Self> {
        let hex = hex.into();
        if hex.len() != algorithm.hex_len()
            || !hex.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            bail!(
                "invalid {} checksum: expected {} lowercase hexadecimal characters",
                algorithm.as_str(),
                algorithm.hex_len()
            );
        }
        Ok(Self { algorithm, hex })
    }

    /// Checksum algorithm.
    pub const fn algorithm(&self) -> ChecksumAlgorithm {
        self.algorithm
    }

    /// Unqualified lowercase hexadecimal digest.
    pub fn hex(&self) -> &str {
        &self.hex
    }
}

impl fmt::Display for Checksum {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.algorithm.as_str(), self.hex)
    }
}

impl FromStr for Checksum {
    type Err = anyhow::Error;

    fn from_str(raw: &str) -> Result<Self> {
        let raw = raw.trim();
        if let Some((algorithm, hex)) = raw.split_once(':') {
            let algorithm = match algorithm {
                "md5" => ChecksumAlgorithm::Md5,
                "sha256" => ChecksumAlgorithm::Sha256,
                other => bail!("unsupported checksum algorithm {other:?}"),
            };
            return Self::new(algorithm, hex.to_string());
        }
        match raw.len() {
            32 => Self::new(ChecksumAlgorithm::Md5, raw.to_string()),
            64 => Self::new(ChecksumAlgorithm::Sha256, raw.to_string()),
            _ => bail!("checksum must be algorithm-qualified or have a recognized digest length"),
        }
    }
}

impl Serialize for Checksum {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Checksum {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

fn digest_file<D: Digest + Default>(path: &Path, label: &str) -> Result<Vec<u8>> {
    let file = File::open(path)
        .with_context(|| format!("failed to open {} for {label}", path.display()))?;
    let mut reader = BufReader::with_capacity(BUFFER_LEN, file);
    let mut hasher = D::default();
    let mut buffer = vec![0_u8; BUFFER_LEN];
    loop {
        let read = reader
            .read(&mut buffer)
            .with_context(|| format!("failed to read {} for {label}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().to_vec())
}

/// Compute an algorithm-qualified streaming checksum.
pub fn checksum_file(path: &Path, algorithm: ChecksumAlgorithm) -> Result<Checksum> {
    let bytes = match algorithm {
        ChecksumAlgorithm::Md5 => digest_file::<Md5>(path, "MD5")?,
        ChecksumAlgorithm::Sha256 => digest_file::<Sha256>(path, "SHA-256")?,
    };
    Checksum::new(algorithm, to_hex(&bytes))
}

/// Verify a file against a typed checksum without algorithm ambiguity.
pub fn verify_file(path: &Path, expected: &Checksum, label: &str) -> Result<()> {
    let actual = checksum_file(path, expected.algorithm())?;
    if &actual != expected {
        bail!(
            "{label} checksum mismatch for {}: expected {expected}, actual {actual}",
            path.display()
        );
    }
    Ok(())
}

/// Compute SHA-256 as unqualified hex for compatibility with existing reports.
pub fn sha256_file(path: &Path) -> Result<String> {
    Ok(checksum_file(path, ChecksumAlgorithm::Sha256)?.hex)
}

/// Verify SHA-256 while accepting legacy unqualified values.
pub fn verify_sha256_file(path: &Path, expected: &str, label: &str) -> Result<()> {
    let checksum: Checksum = if expected.contains(':') {
        expected.parse()?
    } else {
        format!("sha256:{expected}").parse()?
    };
    if checksum.algorithm() != ChecksumAlgorithm::Sha256 {
        bail!("{label} requires a SHA-256 checksum, found {checksum}");
    }
    verify_file(path, &checksum, label)
}

/// Compute MD5 as unqualified hex for official Gaia inventory compatibility.
pub fn md5_file(path: &Path) -> Result<String> {
    Ok(checksum_file(path, ChecksumAlgorithm::Md5)?.hex)
}

/// Verify an official MD5 inventory value.
pub fn verify_md5_file(path: &Path, expected: &str, label: &str) -> Result<()> {
    let checksum: Checksum = if expected.contains(':') {
        expected.parse()?
    } else {
        format!("md5:{expected}").parse()?
    };
    if checksum.algorithm() != ChecksumAlgorithm::Md5 {
        bail!("{label} requires an MD5 checksum, found {checksum}");
    }
    verify_file(path, &checksum, label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_sha256_matches_known_digest() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("fixture.bin");
        std::fs::write(&path, b"abc")?;
        assert_eq!(
            checksum_file(&path, ChecksumAlgorithm::Sha256)?.to_string(),
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            checksum_file(&path, ChecksumAlgorithm::Md5)?.to_string(),
            "md5:900150983cd24fb0d6963f7d28e17f72"
        );
        Ok(())
    }

    #[test]
    fn serde_is_canonical_and_legacy_hex_is_accepted() -> Result<()> {
        let checksum: Checksum =
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".parse()?;
        assert_eq!(checksum.algorithm(), ChecksumAlgorithm::Sha256);
        assert_eq!(
            serde_json::to_string(&checksum)?,
            "\"sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\""
        );
        Ok(())
    }

    #[test]
    fn rejects_algorithm_mismatch() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("fixture.bin");
        std::fs::write(&path, b"abc")?;
        let error = verify_sha256_file(
            &path,
            "md5:900150983cd24fb0d6963f7d28e17f72",
            "fixture",
        )
        .expect_err("algorithm mismatch");
        assert!(error.to_string().contains("SHA-256"));
        Ok(())
    }

    #[test]
    fn large_file_uses_bounded_buffer() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("large.bin");
        let mut file = std::fs::File::create(&path)?;
        let chunk = vec![7_u8; BUFFER_LEN];
        for _ in 0..8 {
            use std::io::Write;
            file.write_all(&chunk)?;
        }
        drop(file);
        assert_eq!(sha256_file(&path)?.len(), 64);
        Ok(())
    }
}
'''

GAIA_XP_CONTINUOUS_RS = r'''
//! Gaia DR3 XP continuous coefficient products and reconstructed-spectrum metadata.
//!
//! Coefficient CSV files are retrieved via Gaia DataLink (`RETRIEVAL_TYPE=XP_CONTINUOUS`).
//! Spectrum calibration uses pinned GaiaXPy offline during the #61 migration; NSB
//! integrates normalized grids in Rust with the same 336–650 nm photon-flux
//! contract as sampled XP.

use anyhow::{Context, Result};
use csv::ReaderBuilder;
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::checksum_io::Checksum;
use crate::gaia_xp::{integrate_photon_flux, parse_normalized_record, PhotonFluxIntegral};
pub use crate::gaia_xp_continuous_canonical::{
    parse_bulk_ecsv_record, parse_datalink_gaiaxpy_csv, stream_bulk_ecsv_gz,
    write_gaiaxpy_datalink_csv, CanonicalXpContinuousRecord, FieldDiffSummary,
    XpContinuousSourceFormat, CANONICAL_XP_CONTINUOUS_SCHEMA, CORRELATION_PACKING,
};

/// Stable identifier for GaiaXPy-reconstructed continuous XP integrated in 336–650 nm.
pub const PHOTOMETRY_MODEL: &str = "gaia_dr3_xp_continuous_reconstructed_336_650nm_v1";

/// Pinned GaiaXPy version used only as a migration oracle (see issue #61).
pub const PINNED_GAIA_XPY_VERSION: &str = "2.1.4";

/// Integrated 336–650 nm reconstruction for one source.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReconstructedContribution {
    /// Gaia source identifier.
    pub source_id: String,
    /// Signed integrated photon flux.
    pub flux_336_650_ph_m2_s: f64,
    /// Propagated statistical uncertainty when available.
    pub statistical_uncertainty_336_650_ph_m2_s: Option<f64>,
    /// Additional systematic uncertainty assigned by the validated policy.
    pub systematic_uncertainty_336_650_ph_m2_s: f64,
    /// Positive signed-segment contribution.
    pub positive_integral_ph_m2_s: f64,
    /// Negative signed-segment contribution.
    pub negative_integral_ph_m2_s: f64,
    /// Number of negative samples.
    pub negative_sample_count: usize,
    /// Number of finite in-band samples.
    pub finite_sample_count: usize,
    /// Number of valid in-band wavelengths.
    pub valid_wavelength_count: usize,
    /// Pipe-separated quality flags.
    pub quality_flags: String,
    /// Whether reconstruction required extrapolation.
    pub extrapolated: bool,
    /// Typed workflow status identifier.
    pub reconstruction_status: String,
    /// Algorithm-qualified source checksum.
    pub input_checksum: Checksum,
    /// Algorithm-qualified calibration checksum.
    pub calibration_checksum: Checksum,
    /// Population contribution branch.
    pub branch: String,
}

/// Validate a raw Gaia DataLink `XP_CONTINUOUS` coefficient CSV for one source.
pub fn validate_continuous_coefficient_csv(bytes: &[u8], expected_source_id: &str) -> Result<()> {
    parse_continuous_coefficient_csv(bytes, expected_source_id).map(|_| ())
}

/// Parse directly into the one canonical XP continuous representation.
pub fn parse_continuous_coefficient_csv(
    bytes: &[u8],
    expected_source_id: &str,
) -> Result<CanonicalXpContinuousRecord> {
    parse_datalink_gaiaxpy_csv(bytes, expected_source_id)
}

/// Write one canonical coefficient record in GaiaXPy-compatible CSV form.
pub fn write_canonical_coefficient_csv(
    path: &Path,
    record: &CanonicalXpContinuousRecord,
) -> Result<()> {
    write_gaiaxpy_datalink_csv(path, record)
}

/// Read one canonical coefficient CSV without cloning into a legacy schema.
pub fn read_canonical_coefficient_csv(path: &Path) -> Result<CanonicalXpContinuousRecord> {
    let bytes = std::fs::read(path).with_context(|| path.display().to_string())?;
    let mut reader = ReaderBuilder::new()
        .comment(Some(b'#'))
        .trim(csv::Trim::All)
        .from_reader(bytes.as_slice());
    let headers = reader.headers()?.clone();
    let source_idx = headers
        .iter()
        .position(|header| header == "source_id")
        .context("source_id")?;
    let row = reader
        .records()
        .next()
        .transpose()
        .context("canonical coefficient row")?
        .ok_or_else(|| anyhow::anyhow!("empty canonical coefficient file"))?;
    let source_id = row.get(source_idx).context("source_id")?;
    parse_datalink_gaiaxpy_csv(&bytes, source_id)
}

/// Integrate a normalized reconstructed continuous spectrum CSV in Rust.
pub fn integrate_reconstructed_csv(path: &Path) -> Result<(String, PhotonFluxIntegral)> {
    let mut reader = ReaderBuilder::new()
        .comment(Some(b'#'))
        .trim(csv::Trim::All)
        .from_reader(std::fs::File::open(path).with_context(|| {
            format!("failed to open reconstructed spectrum {}", path.display())
        })?);
    let headers = reader.headers()?.clone();
    let record = reader
        .records()
        .next()
        .transpose()
        .context("failed to read reconstructed spectrum row")?
        .ok_or_else(|| anyhow::anyhow!("reconstructed spectrum CSV is empty"))?;
    let product = parse_normalized_record(&headers, &record)?;
    let integral = integrate_photon_flux(&product)?;
    Ok((product.source_id, integral))
}

/// Convert a Rust integral into the canonical contribution schema.
pub fn integral_to_contribution(
    source_id: &str,
    integral: &PhotonFluxIntegral,
    input_checksum: Checksum,
    calibration_checksum: Checksum,
) -> ReconstructedContribution {
    ReconstructedContribution {
        source_id: source_id.to_string(),
        flux_336_650_ph_m2_s: integral.total_ph_m2_s,
        statistical_uncertainty_336_650_ph_m2_s: integral.uncertainty_ph_m2_s,
        systematic_uncertainty_336_650_ph_m2_s: 0.0,
        positive_integral_ph_m2_s: integral.positive_ph_m2_s,
        negative_integral_ph_m2_s: integral.negative_ph_m2_s,
        negative_sample_count: integral.negative_samples,
        finite_sample_count: integral.band_samples,
        valid_wavelength_count: integral.band_samples,
        quality_flags: String::new(),
        extrapolated: false,
        reconstruction_status: "valid".to_string(),
        input_checksum,
        calibration_checksum,
        branch: "xp_continuous_reconstructed".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_html_coefficient_payload() {
        let error = validate_continuous_coefficient_csv(b"<html>error</html>", "1")
            .expect_err("html must fail");
        assert!(error.to_string().contains("HTML"));
    }

    fn minimal_datalink_csv(source_id: &str, bp_errors: &str, rp_errors: &str) -> String {
        format!(
            concat!(
                "source_id,bp_n_parameters,bp_standard_deviation,rp_n_parameters,rp_standard_deviation,",
                "bp_coefficients,bp_coefficient_errors,bp_coefficient_correlations,",
                "rp_coefficients,rp_coefficient_errors,rp_coefficient_correlations\n",
                "{source_id},2,1.00000000e0,2,1.00000000e0,",
                "\"(1.0,2.0)\",\"{bp_errors}\",\"(0.2)\",",
                "\"(3.0,4.0)\",\"{rp_errors}\",\"(0.1)\"\n",
            ),
            source_id = source_id,
            bp_errors = bp_errors,
            rp_errors = rp_errors
        )
    }

    #[test]
    fn rejects_mismatched_bp_error_lengths() {
        let raw = minimal_datalink_csv("1", "(0.1)", "(0.3,0.4)");
        assert!(parse_datalink_gaiaxpy_csv(raw.as_bytes(), "1").is_err());
    }

    #[test]
    fn rejects_duplicate_rows() {
        let row = minimal_datalink_csv("42", "(0.1,0.2)", "(0.3,0.4)");
        let raw = format!("{row}{row}");
        let error = parse_continuous_coefficient_csv(raw.as_bytes(), "42").expect_err("dup");
        assert!(error.to_string().contains("exactly one row"));
    }

    #[test]
    fn canonical_roundtrip_has_no_compatibility_clone() {
        let raw = minimal_datalink_csv("99", "(0.1,0.2)", "(0.3,0.4)");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("99.csv");
        let record = parse_datalink_gaiaxpy_csv(raw.as_bytes(), "99").unwrap();
        write_canonical_coefficient_csv(&path, &record).unwrap();
        let read = read_canonical_coefficient_csv(&path).unwrap();
        assert_eq!(read, record);
        assert_eq!(read.schema_version, CANONICAL_XP_CONTINUOUS_SCHEMA);
    }
}
'''

RECONSTRUCT_PY = r'''#!/usr/bin/env python3
"""Migration-only GaiaXPy reconstruction of normalized XP continuous spectra.

Rust is the sole production owner of 336–650 nm photon-flux integration. This
script consumes the generated Rust scientific contract, calibrates spectra with
the frozen GaiaXPy oracle, and writes normalized samples for Rust validation.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from datetime import datetime, timezone
from pathlib import Path

import gaiaxpy
import numpy as np

CONTRACT_PATH = (
    Path(__file__).resolve().parents[2]
    / "crates/nsb-data-tools/contracts/gaia_xp_photon_integration_v1.json"
)


def load_contract() -> dict:
    contract = json.loads(CONTRACT_PATH.read_text(encoding="utf-8"))
    if contract.get("schema_version") != 1:
        raise RuntimeError("unsupported Gaia XP scientific contract schema")
    if contract.get("contract_id") != "gaia_dr3_xp_photon_integration_v1":
        raise RuntimeError("unexpected Gaia XP scientific contract id")
    return contract


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return f"sha256:{digest.hexdigest()}"


def sampling_grid(contract: dict) -> np.ndarray:
    band = contract["band"]
    grid = contract["sampled_grid"]
    count = grid["band_end_index"] - grid["band_start_index"] + 1
    sampling = band["min_nm"] + np.arange(count, dtype=float) * grid["step_nm"]
    if not np.isclose(sampling[-1], band["max_nm"]):
        raise RuntimeError("generated contract has inconsistent Gaia XP band grid")
    return sampling


def format_series(values: np.ndarray, scientific: bool) -> str:
    parts = []
    for value in values:
        if not np.isfinite(value):
            raise ValueError("non-finite calibrated flux sample")
        parts.append(f"{float(value):.8e}" if scientific else f"{float(value):.8f}")
    return ";".join(parts)


def write_normalized_csv(
    output_path: Path,
    source_id: str,
    wavelengths_nm: np.ndarray,
    flux_w_m2_nm: np.ndarray,
    flux_error_w_m2_nm: np.ndarray,
    contract: dict,
) -> None:
    columns = contract["identifiers"]
    part = output_path.with_suffix(output_path.suffix + ".part")
    header = (
        "source_id,"
        f"{columns['wavelength_column']},"
        f"{columns['flux_column']},"
        f"{columns['flux_error_column']}\n"
    )
    row = (
        f"{source_id},"
        f"{format_series(wavelengths_nm, False)},"
        f"{format_series(flux_w_m2_nm, True)},"
        f"{format_series(flux_error_w_m2_nm, True)}\n"
    )
    part.write_text(header + row, encoding="utf-8")
    part.replace(output_path)


def source_id_from_stem(stem: str) -> str:
    return stem.removeprefix("xp_source_")


def reconstruct_file(coefficient_path: Path, output_dir: Path, contract: dict) -> list[dict]:
    sampling = sampling_grid(contract)
    calibrated, _correlation = gaiaxpy.calibrate(
        str(coefficient_path), sampling=sampling, save_file=False, truncation=False
    )
    entries = []
    for _, row in calibrated.iterrows():
        source_id = str(int(row["source_id"]))
        output_path = output_dir / f"{source_id}.csv"
        if output_path.exists():
            entries.append(
                {
                    "source_id": source_id,
                    "status": "skipped_existing",
                    "output_checksum": sha256_file(output_path),
                }
            )
            continue
        flux = np.asarray(row["flux"], dtype=float)
        flux_error = np.asarray(row["flux_error"], dtype=float)
        if flux.shape != sampling.shape or flux_error.shape != sampling.shape:
            raise RuntimeError(f"calibrated grid mismatch for {source_id}")
        write_normalized_csv(output_path, source_id, sampling, flux, flux_error, contract)
        entries.append(
            {
                "source_id": source_id,
                "status": "reconstructed",
                "output_path": str(output_path),
                "coefficient_checksum": sha256_file(coefficient_path),
                "output_checksum": sha256_file(output_path),
                "samples": int(len(sampling)),
                "integration_status": "deferred_to_rust",
            }
        )
    return entries


def reconstruct_one(
    coefficient_path: Path, output_path: Path, contract: dict
) -> dict:
    if output_path.exists():
        return {
            "source_id": source_id_from_stem(coefficient_path.stem),
            "status": "skipped_existing",
            "output_checksum": sha256_file(output_path),
        }
    entries = reconstruct_file(coefficient_path, output_path.parent, contract)
    for entry in entries:
        if entry["source_id"] == source_id_from_stem(coefficient_path.stem):
            return entry
    raise RuntimeError(f"no calibrated row for {coefficient_path}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--coefficients-dir", type=Path, default=None)
    parser.add_argument("--coefficient-file", type=Path, default=None)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--limit", type=int, default=None)
    args = parser.parse_args()

    contract = load_contract()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    entries = []
    if args.coefficient_file is not None:
        entries.extend(reconstruct_file(args.coefficient_file, args.output_dir, contract))
    else:
        if args.coefficients_dir is None:
            raise SystemExit("either --coefficients-dir or --coefficient-file is required")
        coefficient_paths = sorted(args.coefficients_dir.glob("*.csv"))
        if args.limit is not None:
            coefficient_paths = coefficient_paths[: args.limit]
        for coefficient_path in coefficient_paths:
            source_id = source_id_from_stem(coefficient_path.stem)
            output_path = args.output_dir / f"{source_id}.csv"
            entries.append(reconstruct_one(coefficient_path, output_path, contract))

    manifest = {
        "schema_version": 2,
        "scientific_contract_id": contract["contract_id"],
        "scientific_contract_schema_version": contract["schema_version"],
        "scientific_contract_checksum": sha256_file(CONTRACT_PATH),
        "photometry_model": contract["identifiers"]["continuous_photometry_model"],
        "gaiaxpy_version": gaiaxpy.__version__,
        "generation_timestamp_utc": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "integration_owner": contract["integration"]["owner"],
        "entries": entries,
    }
    args.manifest.parent.mkdir(parents=True, exist_ok=True)
    part = args.manifest.with_suffix(args.manifest.suffix + ".part")
    part.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    part.replace(args.manifest)
    print(f"reconstructed {len(entries)} spectra for Rust integration -> {args.output_dir}")


if __name__ == "__main__":
    main()
'''

VALIDATE_PY = r'''#!/usr/bin/env python3
"""Migration-only GaiaXPy spectral parity for bulk/DataLink canonical pairs.

This oracle compares calibrated samples only. Integrated photon flux and
uncertainty are validated by the authoritative Rust implementation.
"""

from __future__ import annotations

import argparse
import csv
import json
from pathlib import Path

import gaiaxpy
import numpy as np

CONTRACT_PATH = (
    Path(__file__).resolve().parents[2]
    / "crates/nsb-data-tools/contracts/gaia_xp_photon_integration_v1.json"
)


def load_contract() -> dict:
    contract = json.loads(CONTRACT_PATH.read_text(encoding="utf-8"))
    if contract.get("schema_version") != 1:
        raise RuntimeError("unsupported Gaia XP scientific contract schema")
    return contract


def sampling_grid(contract: dict) -> np.ndarray:
    band = contract["band"]
    grid = contract["sampled_grid"]
    count = grid["band_end_index"] - grid["band_start_index"] + 1
    sampling = band["min_nm"] + np.arange(count, dtype=float) * grid["step_nm"]
    if not np.isclose(sampling[-1], band["max_nm"]):
        raise RuntimeError("generated contract has inconsistent Gaia XP band grid")
    return sampling


def inspect_table(path: Path) -> list[dict]:
    import pandas as pd

    table = pd.read_csv(path, comment="#")
    rows = []
    for column in table.columns:
        value = table[column].iloc[0]
        array_length = None
        shape = None
        if isinstance(value, str):
            stripped = value.strip()
            if stripped.startswith("(") and stripped.endswith(")"):
                array_length = len(stripped[1:-1].split(","))
                shape = (array_length,)
        rows.append(
            {
                "column": column,
                "dtype": str(table[column].dtype),
                "shape": shape,
                "array_length": array_length,
                "first_row_type": type(value).__name__,
            }
        )
    return rows


def reconstruct(path: Path, sampling: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    calibrated, _ = gaiaxpy.calibrate(
        str(path), sampling=sampling, save_file=False, truncation=False
    )
    if len(calibrated) != 1:
        raise RuntimeError(f"expected one calibrated row for {path}, found {len(calibrated)}")
    row = calibrated.iloc[0]
    flux = np.asarray(row["flux"], dtype=float)
    flux_error = np.asarray(row["flux_error"], dtype=float)
    if flux.shape != sampling.shape or flux_error.shape != sampling.shape:
        raise RuntimeError(f"calibrated grid mismatch for {path}")
    return flux, flux_error


def relative_max(left: np.ndarray, right: np.ndarray, floor: float) -> float:
    denominator = np.maximum(np.maximum(np.abs(left), np.abs(right)), floor)
    return float(np.max(np.abs(left - right) / denominator))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--gaiaxpy-csv-dir", type=Path, required=True)
    parser.add_argument("--comparison-json", type=Path, required=True)
    parser.add_argument("--output-json", type=Path, required=True)
    parser.add_argument("--output-csv", type=Path, required=True)
    parser.add_argument("--inspect-json", type=Path, required=True)
    args = parser.parse_args()

    contract = load_contract()
    tolerance = contract["parity_tolerances"]["spectral_flux_relative"]
    floor = contract["parity_tolerances"]["absolute_floor"]
    comparison = json.loads(args.comparison_json.read_text(encoding="utf-8"))
    sampling = sampling_grid(contract)
    inspect_rows: list[dict] = []
    enriched: list[dict] = []

    for row in comparison:
        source_id = row["source_id"]
        bulk_path = args.gaiaxpy_csv_dir / f"{source_id}_bulk.csv"
        datalink_path = args.gaiaxpy_csv_dir / f"{source_id}_datalink.csv"
        entry = dict(row)
        if not bulk_path.is_file() or not datalink_path.is_file():
            entry["status"] = "missing_gaiaxpy_csv"
            entry["gaiaxpy_equivalent"] = False
            enriched.append(entry)
            continue
        if not inspect_rows:
            inspect_rows.extend(inspect_table(bulk_path))

        bulk_flux, bulk_unc = reconstruct(bulk_path, sampling)
        datalink_flux, datalink_unc = reconstruct(datalink_path, sampling)
        flux_relative_max = relative_max(bulk_flux, datalink_flux, floor)
        uncertainty_relative_max = relative_max(bulk_unc, datalink_unc, floor)
        equivalent = (
            flux_relative_max <= tolerance and uncertainty_relative_max <= tolerance
        )
        entry.update(
            {
                "spectral_flux_relative_max": flux_relative_max,
                "spectral_uncertainty_relative_max": uncertainty_relative_max,
                "gaiaxpy_equivalent": equivalent,
                "integration_owner": contract["integration"]["owner"],
                "status": (
                    "equivalent"
                    if equivalent and entry.get("canonical_equivalent")
                    else "spectral_mismatch"
                ),
            }
        )
        enriched.append(entry)

    args.output_json.parent.mkdir(parents=True, exist_ok=True)
    args.output_json.write_text(json.dumps(enriched, indent=2) + "\n", encoding="utf-8")
    args.inspect_json.write_text(json.dumps(inspect_rows, indent=2) + "\n", encoding="utf-8")
    fieldnames = list(enriched[0].keys()) if enriched else []
    with args.output_csv.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(enriched)

    passed = sum(1 for row in enriched if row.get("gaiaxpy_equivalent"))
    print(f"GaiaXPy spectral parity: {passed}/{len(enriched)} equivalent -> {args.output_json}")


if __name__ == "__main__":
    main()
'''

DEDUP_TEST_RS = r'''
use nsb_data_tools::scientific_contract::{
    authoritative_gaia_xp_photon_contract, gaia_xp_photon_contract,
};
use std::fs;
use std::path::{Path, PathBuf};

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn rust_files(root: &Path) -> Vec<PathBuf> {
    fn visit(path: &Path, files: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(path).expect("read source directory") {
            let path = entry.expect("directory entry").path();
            if path.is_dir() {
                visit(&path, files);
            } else if path.extension().and_then(|value| value.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }
    let mut files = Vec::new();
    visit(root, &mut files);
    files
}

#[test]
fn generated_scientific_contract_matches_rust_authority() {
    assert_eq!(
        gaia_xp_photon_contract(),
        &authoritative_gaia_xp_photon_contract()
    );
}

#[test]
fn migration_python_does_not_reimplement_photon_integration() {
    let root = repository_root();
    for relative in [
        "tools/starlight-xp-continuous/reconstruct_and_integrate.py",
        "tools/starlight-xp-continuous/phase5b_gaiaxpy_flux_validate.py",
    ] {
        let text = fs::read_to_string(root.join(relative)).expect("migration script");
        assert!(text.contains("gaia_xp_photon_integration_v1.json"));
        for forbidden in [
            "6.62607015e-34",
            "299792458.0",
            "BAND_MIN_NM =",
            "BAND_MAX_NM =",
            "GRID_STEP_NM =",
            "np.trapezoid",
            "np.trapz",
            "photon_energy_j",
        ] {
            assert!(
                !text.contains(forbidden),
                "{relative} independently defines forbidden scientific logic {forbidden:?}"
            );
        }
    }
}

#[test]
fn canonical_xp_continuous_schema_has_no_legacy_parallel_model() {
    let root = repository_root();
    let text = fs::read_to_string(
        root.join("crates/nsb-data-tools/src/gaia_xp_continuous.rs"),
    )
    .expect("XP continuous module");
    assert!(!text.contains("ContinuousCoefficients"));
    assert!(!text.contains("canonical_to_legacy"));
}

#[test]
fn production_rust_has_one_sha256_file_implementation() {
    let root = repository_root().join("crates/nsb-data-tools/src");
    let definitions = rust_files(&root)
        .into_iter()
        .filter_map(|path| {
            let text = fs::read_to_string(&path).ok()?;
            let count = text.matches("fn sha256_file(").count();
            (count > 0).then_some((path, count))
        })
        .collect::<Vec<_>>();
    assert_eq!(definitions.len(), 1, "SHA-256 helpers: {definitions:?}");
    assert!(definitions[0].0.ends_with("checksum_io.rs"));
    assert_eq!(definitions[0].1, 1);
}

#[test]
fn retained_data_tools_do_not_spawn_sibling_cargo_binaries() {
    let root = repository_root().join("crates/nsb-data-tools/src");
    for path in rust_files(&root) {
        let text = fs::read_to_string(&path).expect("Rust source");
        assert!(
            !text.contains("Command::new(\"cargo\")"),
            "{} launches a sibling workspace binary through cargo",
            path.display()
        );
    }
}
'''

DUPLICATION_REGISTER_MD = r'''# NSB semantic duplication register

Status: normative after issue #59. This register is concept-oriented: two files
that look similar are not duplicates unless they independently own the same
scientific rule, schema, state transition or operational contract.

| Concept | Implementations audited | Type | Authority | Consumers / constraints | Risk | Resolution | Verification |
|---|---|---|---|---|---|---|---|
| Gaia XP 336–650 nm band, sampled grid and identifiers | `gaia_xp.rs`; `reconstruct_and_integrate.py`; `phase5b_gaiaxpy_flux_validate.py`; release/validation binaries | Cross-language semantic duplicate | `nsb-data-tools::gaia_xp`; generated `contracts/gaia_xp_photon_integration_v1.json` is the secondary contract | Rust catalogue/reconstruction tools; temporary GaiaXPy oracle | Scientific drift | Rust constants remain production authority; migration scripts load the generated versioned JSON and define no band/grid/model constants | `scientific_contract` equality and corruption tests; repository policy test scans Python literals |
| Photon energy and signed trapezoidal integration | Rust `integrate_photon_flux*`; two Python integrations | Cross-language algorithm duplicate | `gaia_xp::integrate_photon_flux` | All production sampled and continuous integrations | Scientific correctness | Python integration removed; GaiaXPy scripts emit/compare spectra only and explicitly defer integration to Rust | Rust integration tests plus Python-source policy test |
| Uncertainty propagation | Rust weighted sample-error variance; Python trapezoid of uncertainty | Scientific-policy duplicate | `gaia_xp` | Reconstructed contribution validation | Incorrect uncertainty coverage | Python implementation removed; frozen tolerances remain in generated contract | Contract equality/corruption tests and existing Phase 5 frozen evidence |
| Planck constant and speed of light | Local typed Planck constant plus `qtty::velocity::C`; Python literals | Cross-language physical-constant duplicate | qtty owns `C`; `gaia_xp` owns the single justified local SI Planck constant pending an upstream qtty constant | Photon conversion | Numerical drift | Python literals removed; no local speed-of-light constant | Source policy test rejects literals outside Rust authority |
| XP continuous coefficient schema | `CanonicalXpContinuousRecord`; `ContinuousCoefficients`; `canonical_to_legacy` | Compatibility-layer duplicate | `gaia_xp_continuous_canonical` | DataLink and official bulk normalizers | Field loss, clones, schema divergence | Legacy model and adapter removed; public parse/read APIs return the canonical record | Canonical roundtrip and repository policy test |
| Checksum algorithm/value representation | `checksum_io`; local `sha256_file` helpers; raw algorithm-qualified strings | Textual and semantic duplicate | `checksum_io::{Checksum, ChecksumAlgorithm}` | Gaia MD5 inventory, NSB SHA-256 provenance, existing legacy reports | Algorithm confusion and inconsistent formatting | One streaming typed implementation; legacy raw hex accepted only at parsing boundary; local Rust helpers consolidated | Known MD5/SHA-256 fixtures, algorithm-mismatch test, one-definition repository test |
| Atomic writes and JSON manifest persistence | ad hoc `.part` writes and direct `fs::write` | Semantic duplicate | `artifact_io` | New and migrated product/report writers | Partial files, crash inconsistency | Shared flushed atomic byte/JSON persistence and strict typed JSON reader established; migrations must use this API when touching retained writers | Atomic roundtrip, cleanup and unknown-field tests |
| Software commit resolution | environment overrides, `git rev-parse`, `unknown` fallbacks | Semantic duplicate | `provenance::resolve_software_commit` | Generated reports and candidate/production evidence | Reproducibility drift | One resolution order and production validation contract | Deterministic injected-resolution tests; unknown commit rejected for production |
| UTC timestamp formatting | multiple `chrono` formatting strings | Textual duplicate | `provenance::utc_now_rfc3339_seconds` | Generated reports/manifests | Non-canonical timestamps | Shared RFC3339-seconds UTC formatter | Provenance validation tests |
| HEALPix indexing and coordinate transforms | NSB data tools and Siderust | Potential generic duplicate | Siderust | Map builders and accumulators | Geometry inconsistency | NSB delegates generic transform/index primitives to Siderust; NSB owns product-specific accumulation/accounting only | Existing map fixtures and Siderust-backed tests |
| HEALPix contribution export/merge | historical Phase 5B validators and product builders | Workflow/validator duplicate | Retained library generation/validation services | Integrated Starlight candidate | Divergent source accounting | One-shot Phase 5B binaries removed by #58; assertions remain in tests/frozen evidence or retained product services | Tool-registry tests and integrated pipeline tests |
| Retry/backoff/download behavior | TAP, DataLink and bulk download paths | Similar but protocol-specific behavior | Protocol acquisition modules (`gaia_tap`, `gaia_datalink`, `gaia_bulk`) | Distinct HTTP/service contracts | Over-abstraction or inconsistent retry evidence | Shared concepts are typed within acquisition modules; protocol-specific status handling remains separate intentionally | Existing retry/resume tests; no generic utility extraction without common semantics |
| Pipeline ordering and sibling process orchestration | historical shell/Python wrappers; Phase 5B binaries; PR #57 orchestrator | Workflow duplicate | Retained Rust library services and durable commands | Release maintainers | Different defaults/resume/success criteria | Historical wrappers/binaries removed by #58; policy test forbids retained Rust from invoking sibling binaries through `cargo run` | `retained_data_tools_do_not_spawn_sibling_cargo_binaries` |
| CLI input/output/resume option groups | many durable binaries | Repeated syntax with domain-specific validation | Each thin command plus shared typed library configuration when semantics truly match | External/release-maintainer CLI compatibility | Default drift | #58 reduced the surface; options are consolidated only where validation and meaning are identical, avoiding a stringly generic argument bag | Tool registry, CLI help and command-specific tests |
| Internal report and policy schemas | typed structs plus legacy `serde_json::Value` external/archival readers | Hidden schema duplication | Versioned Rust structs adjacent to their owning domain module | Frozen historical evidence and external compatibility | Silent missing/defaulted fields | New contracts use `deny_unknown_fields` and strict `artifact_io::read_json`; remaining dynamic reads are compatibility boundaries and must become typed when modified under #60 | Strict JSON tests; #60 architecture gates |
| Validation predicates | historical `run_phase5b_*` and audit binaries versus tests | One-off executable duplicate | Reusable library validation plus tests | Maintainers and CI | Divergent pass/fail gates | Validation-only Phase 5/5B binaries removed by #58; retained validation commands call durable domain logic | Tool registry and integration tests |
| Phase 5/5B executable surface | prepare/freeze/finalize/pilot/merge/resume/cross-comparison programs | Historical workflow duplication | Frozen evidence, tests, and retained capability commands | Reproducibility without permanent phase executables | Maintenance burden | Classified and removed/consolidated by #58; no phase-numbered compiled command remains | `tool_registry` integration tests |
| Python schema emission | `emit_phase5b_schema_artifacts.py` versus canonical Rust schema | Generated-contract duplicate | Canonical Rust schema metadata | Temporary GaiaXPy migration only | Schema drift | Migration-only under #61; generated scientific contract now demonstrates the required Rust-to-secondary pattern; script has no production authority | Registry removal contract and #61 CI gate |
| Python package/environment hashing | `audit_gaiaxpy_environment.py` and Python tests | Migration-only checksum/provenance duplicate | Rust checksum/provenance for supported workflows | Frozen GaiaXPy oracle until #61 | Temporary ecosystem drift | Explicit temporary exception with removal issue #61; not used by supported product commands | Tool registry enforces migration-only status and removal issue |
| qtty/Siderust units and constants | local domain quantities versus upstream generic units | Potential ownership duplicate | qtty/Siderust for generic physical/astronomical concepts | Runtime and data tools | Unit mismatch | Existing qtty quantities and `qtty::velocity::C` are used; justified local Gaia product contracts remain in `nsb-data-tools` only | Compile-time unit checking and dependency-policy tests |

## Ownership rules

- `nsb` owns runtime composition, typed queries/results and admission of validated
  assets. It does not own Gaia acquisition or product-generation workflows.
- qtty/Siderust own generic units, physical/astronomical constants where exposed,
  coordinate transformations and HEALPix primitives.
- `nsb-data-tools` owns Gaia product schemas, NSB-specific photon integration,
  typed checksums/provenance, product persistence and validation services.
- Python is a migration oracle only while #61 remains open. It is not a second
  production authority.
- A generated secondary contract must be mechanically checked against Rust. A
  hand-maintained copy is forbidden.

## Change-control rule

Deduplication must preserve approved scientific outputs. Changes to band edges,
grid policy, integration, uncertainty propagation or model identifiers require a
separate evidence-backed scientific review. Structural consolidation is guarded
by frozen Phase 5 evidence, generated-contract equality and fail-closed schema
version tests.
'''


if __name__ == "__main__":
    main()
