//! Independent, minimal reader for the published Starlight candidate map.
//!
//! This intentionally does not call into `crate::starlight::map::product`:
//! independent validation tooling must not share a parsing bug with the
//! production writer it is meant to check. It still fails closed on any
//! structural anomaly.

use crate::platform::checksum_io;
use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub const EXPECTED_MAP_SCHEMA: &str = "nsb-healpix-starlight-candidate-v5";
pub const EXPECTED_FLUX_UNIT: &str = "ph_m-2_s-1";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CandidatePixel {
    pub flux_ph_m2_s: f64,
    pub statistical_uncertainty_ph_m2_s: f64,
    pub systematic_uncertainty_ph_m2_s: f64,
    pub total_uncertainty_ph_m2_s: f64,
    pub admitted_sources: u64,
    pub excluded_sources: u64,
}

#[derive(Debug, Clone)]
pub struct CandidateMap {
    pub nside: u32,
    pub schema: String,
    pub flux_unit: String,
    pub sha256: String,
    pub pixels: BTreeMap<u32, CandidatePixel>,
}

impl CandidateMap {
    pub fn admitted_sources(&self) -> BTreeMap<u32, u64> {
        self.pixels
            .iter()
            .map(|(pixel, value)| (*pixel, value.admitted_sources))
            .collect()
    }
}

/// Load and independently sanity-check the candidate map at `path`. Fails
/// closed on unknown schema, non-finite or negative values, out-of-order or
/// duplicate pixels, and (if `expected_sha256` is supplied) a checksum
/// mismatch.
pub fn load(
    path: &Path,
    expected_nside: u32,
    expected_sha256: Option<&str>,
) -> Result<CandidateMap> {
    let bytes = fs::read(path)
        .with_context(|| format!("read Starlight candidate map {}", path.display()))?;
    let sha256 = checksum_io::sha256_bytes(&bytes);
    if let Some(expected) = expected_sha256 {
        if expected != sha256 {
            bail!(
                "candidate map checksum mismatch for {}: expected {expected}, actual {sha256}",
                path.display()
            );
        }
    }
    let text = String::from_utf8(bytes)
        .with_context(|| format!("candidate map {} is not valid UTF-8", path.display()))?;

    let mut headers = BTreeMap::new();
    for line in text
        .lines()
        .take_while(|line| line.trim_start().starts_with('#'))
    {
        let (key, value) = line
            .trim_start()
            .trim_start_matches('#')
            .trim()
            .split_once('=')
            .with_context(|| format!("{} has a malformed header line", path.display()))?;
        headers.insert(key.to_string(), value.to_string());
    }
    let schema = headers
        .get("schema")
        .with_context(|| format!("{} has no schema header", path.display()))?
        .clone();
    if schema != EXPECTED_MAP_SCHEMA {
        bail!(
            "{} declares unsupported schema {schema}, expected {EXPECTED_MAP_SCHEMA}",
            path.display()
        );
    }
    let flux_unit = headers
        .get("flux_unit")
        .with_context(|| format!("{} has no flux_unit header", path.display()))?
        .clone();
    if flux_unit != EXPECTED_FLUX_UNIT {
        bail!(
            "{} declares unsupported flux_unit {flux_unit}, expected {EXPECTED_FLUX_UNIT}",
            path.display()
        );
    }
    let nside = headers
        .get("nside")
        .with_context(|| format!("{} has no nside header", path.display()))?
        .parse::<u32>()
        .with_context(|| format!("{} has a malformed nside header", path.display()))?;
    if nside != expected_nside {
        bail!(
            "{} declares nside={nside}, expected nside={expected_nside}",
            path.display()
        );
    }
    if headers.get("ordering").map(String::as_str) != Some("nested") {
        bail!("{} must declare nested ordering", path.display());
    }
    if headers.get("representation").map(String::as_str) != Some("sparse") {
        bail!("{} must declare sparse representation", path.display());
    }

    let mut data_lines = text
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'));
    let header_row = data_lines
        .next()
        .with_context(|| format!("{} has no data header row", path.display()))?;
    if header_row != "pixel,flux_ph_m2_s,statistical_uncertainty_ph_m2_s,systematic_uncertainty_ph_m2_s,total_uncertainty_ph_m2_s,admitted_sources,excluded_sources" {
        bail!("{} has an unrecognized column schema", path.display());
    }

    let domain = 12_u64 * u64::from(nside) * u64::from(nside);
    let mut pixels = BTreeMap::new();
    let mut previous = None;
    for (row_index, line) in data_lines.enumerate() {
        let fields = line.split(',').collect::<Vec<_>>();
        if fields.len() != 7 {
            bail!(
                "{} row {} is malformed: expected 7 fields, found {}",
                path.display(),
                row_index + 1,
                fields.len()
            );
        }
        let pixel = fields[0].parse::<u32>().with_context(|| {
            format!(
                "{} row {} has an invalid pixel id",
                path.display(),
                row_index + 1
            )
        })?;
        if u64::from(pixel) >= domain {
            bail!(
                "{} row {} pixel {pixel} is outside the nside={nside} domain",
                path.display(),
                row_index + 1
            );
        }
        if let Some(previous_pixel) = previous {
            if pixel <= previous_pixel {
                bail!(
                    "{} row {} breaks strictly increasing pixel order ({pixel} after {previous_pixel})",
                    path.display(),
                    row_index + 1
                );
            }
        }
        previous = Some(pixel);
        let flux = parse_nonnegative_finite(fields[1], "flux", path, row_index)?;
        let statistical =
            parse_nonnegative_finite(fields[2], "statistical uncertainty", path, row_index)?;
        let systematic =
            parse_nonnegative_finite(fields[3], "systematic uncertainty", path, row_index)?;
        let total = parse_nonnegative_finite(fields[4], "total uncertainty", path, row_index)?;
        let admitted = fields[5].parse::<u64>().with_context(|| {
            format!(
                "{} row {} has an invalid admitted_sources value",
                path.display(),
                row_index + 1
            )
        })?;
        let excluded = fields[6].parse::<u64>().with_context(|| {
            format!(
                "{} row {} has an invalid excluded_sources value",
                path.display(),
                row_index + 1
            )
        })?;
        pixels.insert(
            pixel,
            CandidatePixel {
                flux_ph_m2_s: flux,
                statistical_uncertainty_ph_m2_s: statistical,
                systematic_uncertainty_ph_m2_s: systematic,
                total_uncertainty_ph_m2_s: total,
                admitted_sources: admitted,
                excluded_sources: excluded,
            },
        );
    }
    if pixels.is_empty() {
        bail!("{} contains no occupied pixels", path.display());
    }
    Ok(CandidateMap {
        nside,
        schema,
        flux_unit,
        sha256,
        pixels,
    })
}

fn parse_nonnegative_finite(raw: &str, label: &str, path: &Path, row_index: usize) -> Result<f64> {
    let value = raw.parse::<f64>().with_context(|| {
        format!(
            "{} row {} has an invalid {label}",
            path.display(),
            row_index + 1
        )
    })?;
    if !value.is_finite() || value < 0.0 {
        bail!(
            "{} row {} has a non-finite or negative {label}",
            path.display(),
            row_index + 1
        );
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_fixture(directory: &TempDir, body: &str) -> std::path::PathBuf {
        let path = directory.path().join("starlight_nside1.csv");
        fs::write(&path, body).unwrap();
        path
    }

    const HEADER: &str = concat!(
        "# schema=nsb-healpix-starlight-candidate-v5\n",
        "# ordering=nested\n",
        "# representation=sparse\n",
        "# nside=1\n",
        "# flux_unit=ph_m-2_s-1\n",
        "pixel,flux_ph_m2_s,statistical_uncertainty_ph_m2_s,systematic_uncertainty_ph_m2_s,total_uncertainty_ph_m2_s,admitted_sources,excluded_sources\n",
    );

    #[test]
    fn loads_a_well_formed_sparse_map() {
        let directory = TempDir::new().unwrap();
        let path = write_fixture(
            &directory,
            &format!("{HEADER}0,1.0,0.1,0.2,0.223606797749979,5,1\n"),
        );
        let map = load(&path, 1, None).unwrap();
        assert_eq!(map.pixels.len(), 1);
        assert_eq!(map.pixels[&0].admitted_sources, 5);
    }

    #[test]
    fn rejects_checksum_mismatch() {
        let directory = TempDir::new().unwrap();
        let path = write_fixture(
            &directory,
            &format!("{HEADER}0,1.0,0.1,0.2,0.223606797749979,5,1\n"),
        );
        assert!(load(&path, 1, Some(&"0".repeat(64))).is_err());
    }

    #[test]
    fn rejects_wrong_nside_and_unknown_schema() {
        let directory = TempDir::new().unwrap();
        let path = write_fixture(
            &directory,
            &format!("{HEADER}0,1.0,0.1,0.2,0.223606797749979,5,1\n"),
        );
        assert!(load(&path, 2, None).is_err());

        let body = format!("{HEADER}0,1.0,0.1,0.2,0.223606797749979,5,1\n").replace(
            "nsb-healpix-starlight-candidate-v5",
            "nsb-healpix-starlight-candidate-v3",
        );
        let path = write_fixture(&directory, &body);
        assert!(load(&path, 1, None).is_err());
    }

    #[test]
    fn rejects_out_of_order_and_duplicate_pixels() {
        let directory = TempDir::new().unwrap();
        let body = format!(
            "{HEADER}1,1.0,0.1,0.2,0.223606797749979,5,1\n0,1.0,0.1,0.2,0.223606797749979,5,1\n"
        );
        let path = write_fixture(&directory, &body);
        assert!(load(&path, 1, None).is_err());
    }

    #[test]
    fn rejects_negative_or_non_finite_values() {
        let directory = TempDir::new().unwrap();
        let body = format!("{HEADER}0,-1.0,0.1,0.2,0.223606797749979,5,1\n");
        let path = write_fixture(&directory, &body);
        assert!(load(&path, 1, None).is_err());
    }
}
