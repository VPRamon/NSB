//! Reader for one reference's transformed comparison grid.
//!
//! Turning a heterogeneous published dataset into a HEALPix-nested grid of
//! Galactic 300-650 nm photon radiance is reference-specific scientific work
//! that is explicitly out of scope for this scaffolding PR (see the
//! `transformation_to_target` field on each reference entry). This module
//! only defines and validates the stable interchange format that a future
//! transformation step must produce, at
//! `<references-workspace>/<reference-id>/transformed-grid-v1.csv`.

use crate::platform::checksum_io;
use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridPixel {
    pub value_ph_m2_s: f64,
    pub statistical_uncertainty_ph_m2_s: f64,
}

#[derive(Debug, Clone)]
pub struct TransformedReferenceGrid {
    pub nside: u32,
    pub sha256: String,
    pub pixels: BTreeMap<u32, GridPixel>,
}

/// Return `None` (not an error) if no transformed grid exists yet for this
/// reference; that is the expected state until a transformation lands.
pub fn load_if_present(
    path: &Path,
    expected_nside: u32,
) -> Result<Option<TransformedReferenceGrid>> {
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(path)
        .with_context(|| format!("read transformed reference grid {}", path.display()))?;
    let sha256 = checksum_io::sha256_bytes(&bytes);
    let text = String::from_utf8(bytes).with_context(|| {
        format!(
            "transformed reference grid {} is not valid UTF-8",
            path.display()
        )
    })?;
    let mut lines = text
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'));
    let header = lines
        .next()
        .with_context(|| format!("{} has no header row", path.display()))?;
    if header != "pixel,value_ph_m2_s,statistical_uncertainty_ph_m2_s" {
        bail!("{} has an unrecognized column schema", path.display());
    }
    let domain = 12_u64 * u64::from(expected_nside) * u64::from(expected_nside);
    let mut pixels = BTreeMap::new();
    for (row_index, line) in lines.enumerate() {
        let fields = line.split(',').collect::<Vec<_>>();
        if fields.len() != 3 {
            bail!("{} row {} is malformed", path.display(), row_index + 1);
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
                "{} row {} pixel {pixel} is outside the nside={expected_nside} domain",
                path.display(),
                row_index + 1
            );
        }
        let value = fields[1].parse::<f64>().with_context(|| {
            format!(
                "{} row {} has an invalid value",
                path.display(),
                row_index + 1
            )
        })?;
        let sigma = fields[2].parse::<f64>().with_context(|| {
            format!(
                "{} row {} has an invalid uncertainty",
                path.display(),
                row_index + 1
            )
        })?;
        if !value.is_finite() || value <= 0.0 || !sigma.is_finite() || sigma < 0.0 {
            bail!(
                "{} row {} has a non-finite, non-positive value, or negative uncertainty",
                path.display(),
                row_index + 1
            );
        }
        if pixels
            .insert(
                pixel,
                GridPixel {
                    value_ph_m2_s: value,
                    statistical_uncertainty_ph_m2_s: sigma,
                },
            )
            .is_some()
        {
            bail!("{} contains duplicate pixel {pixel}", path.display());
        }
    }
    if pixels.is_empty() {
        bail!("{} contains no pixels", path.display());
    }
    Ok(Some(TransformedReferenceGrid {
        nside: expected_nside,
        sha256,
        pixels,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn missing_grid_is_not_an_error() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("missing.csv");
        assert!(load_if_present(&path, 1).unwrap().is_none());
    }

    #[test]
    fn loads_a_well_formed_grid() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("grid.csv");
        fs::write(
            &path,
            "pixel,value_ph_m2_s,statistical_uncertainty_ph_m2_s\n0,1.0,0.1\n1,2.0,0.2\n",
        )
        .unwrap();
        let grid = load_if_present(&path, 1).unwrap().unwrap();
        assert_eq!(grid.pixels.len(), 2);
    }

    #[test]
    fn rejects_non_positive_values_and_duplicates() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("grid.csv");
        fs::write(
            &path,
            "pixel,value_ph_m2_s,statistical_uncertainty_ph_m2_s\n0,0.0,0.1\n",
        )
        .unwrap();
        assert!(load_if_present(&path, 1)
            .unwrap_err()
            .to_string()
            .contains("non-finite"));

        fs::write(
            &path,
            "pixel,value_ph_m2_s,statistical_uncertainty_ph_m2_s\n0,1.0,0.1\n0,1.0,0.1\n",
        )
        .unwrap();
        assert!(load_if_present(&path, 1)
            .unwrap_err()
            .to_string()
            .contains("duplicate"));
    }
}
