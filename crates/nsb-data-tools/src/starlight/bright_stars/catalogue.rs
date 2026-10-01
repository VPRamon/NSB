//! Deterministic ingestion of checksum-pinned upstream catalogues.

use super::artifact::{BrightStarInputProvenance, BrightStarInputRole};
use super::crossmatch::GaiaMatchRow;
use super::crossmatch::HipparcosAstrometry;
use crate::platform::checksum_io;
use anyhow::{bail, Context, Result};
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinnedCatalogueInput {
    pub path: PathBuf,
    pub provenance: BrightStarInputProvenance,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hipparcos2Record {
    pub astrometry: HipparcosAstrometry,
    pub hp_mag: f64,
    pub hp_mag_uncertainty: f64,
    pub solution_type: u16,
    pub components: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tycho2Photometry {
    pub tycho_id: String,
    pub hip: u32,
    pub bt_mag: Option<f64>,
    pub bt_mag_uncertainty: Option<f64>,
    pub vt_mag: Option<f64>,
    pub vt_mag_uncertainty: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HipGaiaIdentityMatch {
    pub hip: u32,
    pub gaia_source_id: u64,
    pub angular_distance_arcsec: f64,
    pub number_of_neighbours: u32,
}

pub fn ingest_hipparcos2(input: &PinnedCatalogueInput) -> Result<Vec<Hipparcos2Record>> {
    verify_input(input, BrightStarInputRole::Hipparcos2)?;
    let reader = open_text(&input.path)?;
    let mut records = Vec::new();
    for (index, line) in reader.lines().enumerate() {
        let line = line.with_context(|| format!("read Hipparcos-2 line {}", index + 1))?;
        if line.len() < 164 {
            bail!(
                "Hipparcos-2 line {} is shorter than the I/311 contract",
                index + 1
            );
        }
        let hip = field(&line, 0, 6, "HIP")?.parse::<u32>()?;
        let solution_type = field(&line, 7, 10, "Sn")?.parse::<u16>()?;
        let components = field(&line, 13, 14, "Nc")?.parse::<u8>()?;
        let ra_rad = field(&line, 15, 28, "RArad")?.parse::<f64>()?;
        let dec_rad = field(&line, 29, 42, "DErad")?.parse::<f64>()?;
        let pm_ra = field(&line, 51, 59, "pmRA")?.parse::<f64>()?;
        let pm_dec = field(&line, 60, 68, "pmDE")?.parse::<f64>()?;
        let e_ra = field(&line, 69, 75, "e_RArad")?.parse::<f64>()?;
        let e_dec = field(&line, 76, 82, "e_DErad")?.parse::<f64>()?;
        let e_pm_ra = field(&line, 90, 96, "e_pmRA")?.parse::<f64>()?;
        let e_pm_dec = field(&line, 97, 103, "e_pmDE")?.parse::<f64>()?;
        let hp_mag = field(&line, 129, 136, "Hpmag")?.parse::<f64>()?;
        let hp_error = field(&line, 137, 143, "e_Hpmag")?.parse::<f64>()?;
        let astrometry = HipparcosAstrometry {
            hip,
            ra_deg_j1991_25: ra_rad.to_degrees(),
            dec_deg_j1991_25: dec_rad.to_degrees(),
            pm_ra_cosdec_mas_per_year: pm_ra,
            pm_dec_mas_per_year: pm_dec,
            position_uncertainty_mas: e_ra.hypot(e_dec),
            proper_motion_uncertainty_mas_per_year: e_pm_ra.hypot(e_pm_dec),
        };
        super::crossmatch::propagate_hipparcos_to_j2016(&astrometry)?;
        if !hp_mag.is_finite() || !hp_error.is_finite() || hp_error < 0.0 {
            bail!("Hipparcos-2 HIP {hip} has invalid Hp photometry");
        }
        records.push(Hipparcos2Record {
            astrometry,
            hp_mag,
            hp_mag_uncertainty: hp_error,
            solution_type,
            components,
        });
    }
    records.sort_by_key(|record| record.astrometry.hip);
    if records
        .windows(2)
        .any(|pair| pair[0].astrometry.hip == pair[1].astrometry.hip)
    {
        bail!("Hipparcos-2 contains duplicate HIP identifiers");
    }
    Ok(records)
}

pub fn ingest_tycho2(input: &PinnedCatalogueInput) -> Result<BTreeMap<u32, Tycho2Photometry>> {
    verify_input(input, BrightStarInputRole::Tycho2)?;
    let reader = open_text(&input.path)?;
    let mut by_hip = BTreeMap::new();
    for (index, line) in reader.lines().enumerate() {
        let line = line.with_context(|| format!("read Tycho-2 line {}", index + 1))?;
        if line.len() < 151 {
            bail!(
                "Tycho-2 line {} is shorter than the I/259 contract",
                index + 1
            );
        }
        let hip = optional_field(&line, 142, 148)?.and_then(|value| value.parse::<u32>().ok());
        let Some(hip) = hip else {
            continue;
        };
        let value = Tycho2Photometry {
            tycho_id: format!(
                "{}-{}-{}",
                field(&line, 0, 4, "TYC1")?,
                field(&line, 5, 10, "TYC2")?,
                field(&line, 11, 12, "TYC3")?
            ),
            hip,
            bt_mag: optional_f64(&line, 110, 116)?,
            bt_mag_uncertainty: optional_f64(&line, 117, 122)?,
            vt_mag: optional_f64(&line, 123, 129)?,
            vt_mag_uncertainty: optional_f64(&line, 130, 135)?,
        };
        if by_hip.insert(hip, value).is_some() {
            bail!("Tycho-2 has multiple entries for HIP {hip}; component policy is required");
        }
    }
    Ok(by_hip)
}

pub fn ingest_hip_gaia_crossmatch(
    input: &PinnedCatalogueInput,
) -> Result<BTreeMap<u32, Vec<HipGaiaIdentityMatch>>> {
    verify_input(input, BrightStarInputRole::HipGaiaCrossmatch)?;
    let file = File::open(&input.path)?;
    let reader: Box<dyn Read> = if input.path.extension().is_some_and(|ext| ext == "gz") {
        Box::new(GzDecoder::new(file))
    } else {
        Box::new(file)
    };
    let mut csv = csv::ReaderBuilder::new()
        .comment(Some(b'#'))
        .from_reader(reader);
    let headers = csv.headers()?.clone();
    let column = |name: &str| {
        headers
            .iter()
            .position(|value| value == name)
            .with_context(|| format!("Hipparcos-Gaia crossmatch lacks {name}"))
    };
    let source_id = column("source_id")?;
    let original = column("original_ext_source_id")?;
    let distance = column("angular_distance")?;
    let neighbours = column("number_of_neighbours")?;
    let mut out: BTreeMap<u32, Vec<HipGaiaIdentityMatch>> = BTreeMap::new();
    for row in csv.records() {
        let row = row?;
        let item = HipGaiaIdentityMatch {
            hip: row[original].trim().parse()?,
            gaia_source_id: row[source_id].trim().parse()?,
            angular_distance_arcsec: row[distance].trim().parse()?,
            number_of_neighbours: row[neighbours].trim().parse()?,
        };
        if !item.angular_distance_arcsec.is_finite() || item.angular_distance_arcsec < 0.0 {
            bail!("invalid angular distance for HIP {}", item.hip);
        }
        out.entry(item.hip).or_default().push(item);
    }
    for rows in out.values_mut() {
        rows.sort_by(|a, b| {
            a.angular_distance_arcsec
                .total_cmp(&b.angular_distance_arcsec)
                .then(a.gaia_source_id.cmp(&b.gaia_source_id))
        });
    }
    Ok(out)
}

/// Ingest the checksum-pinned result of the documented Gaia DR3 quality ADQL
/// query used by the build. The CSV must contain one row per Gaia source.
pub fn ingest_gaia_quality_extract(
    input: &PinnedCatalogueInput,
) -> Result<BTreeMap<u64, GaiaMatchRow>> {
    verify_input(input, BrightStarInputRole::GaiaDr3QualityExtract)?;
    let file = File::open(&input.path)?;
    let reader: Box<dyn Read> = if input.path.extension().is_some_and(|ext| ext == "gz") {
        Box::new(GzDecoder::new(file))
    } else {
        Box::new(file)
    };
    let mut csv = csv::ReaderBuilder::new()
        .comment(Some(b'#'))
        .from_reader(reader);
    let headers = csv.headers()?.clone();
    let column = |name: &str| {
        headers
            .iter()
            .position(|value| value == name)
            .with_context(|| format!("Gaia quality extract lacks {name}"))
    };
    let source_id = column("source_id")?;
    let ra = column("ra")?;
    let dec = column("dec")?;
    let g = column("phot_g_mean_mag")?;
    let xp = column("has_xp_continuous")?;
    let phot = column("photometric_usable")?;
    let mut out = BTreeMap::new();
    for row in csv.records() {
        let row = row?;
        let id = row[source_id].trim().parse::<u64>()?;
        let parse_bool = |value: &str| -> Result<bool> {
            match value.trim().to_ascii_lowercase().as_str() {
                "true" | "1" => Ok(true),
                "false" | "0" => Ok(false),
                other => bail!("invalid boolean {other}"),
            }
        };
        let item = GaiaMatchRow {
            gaia_source_id: id,
            ra_deg_j2016: row[ra].trim().parse()?,
            dec_deg_j2016: row[dec].trim().parse()?,
            gaia_g_mag: (!row[g].trim().is_empty())
                .then(|| row[g].trim().parse())
                .transpose()?,
            gaia_xp_usable: parse_bool(&row[xp])?,
            gaia_photometric_usable: parse_bool(&row[phot])?,
        };
        if out.insert(id, item).is_some() {
            bail!("Gaia quality extract contains duplicate source_id {id}");
        }
    }
    Ok(out)
}

fn verify_input(input: &PinnedCatalogueInput, role: BrightStarInputRole) -> Result<()> {
    if input.provenance.role != role {
        bail!("catalogue input role does not match parser");
    }
    let actual = checksum_io::sha256_file(&input.path)?;
    if actual != input.provenance.sha256 {
        bail!("catalogue checksum mismatch for {}", input.path.display());
    }
    Ok(())
}

fn open_text(path: &PathBuf) -> Result<Box<dyn BufRead>> {
    let file = File::open(path)?;
    if path.extension().is_some_and(|extension| extension == "gz") {
        Ok(Box::new(BufReader::new(GzDecoder::new(file))))
    } else {
        Ok(Box::new(BufReader::new(file)))
    }
}

fn field<'a>(line: &'a str, start: usize, end: usize, label: &str) -> Result<&'a str> {
    let value = line
        .get(start..end)
        .context("catalogue record is not ASCII/fixed-width")?
        .trim();
    if value.is_empty() {
        bail!("required catalogue field {label} is empty");
    }
    Ok(value)
}

fn optional_field(line: &str, start: usize, end: usize) -> Result<Option<&str>> {
    let value = line
        .get(start..end)
        .context("catalogue record is not ASCII/fixed-width")?
        .trim();
    Ok((!value.is_empty()).then_some(value))
}

fn optional_f64(line: &str, start: usize, end: usize) -> Result<Option<f64>> {
    optional_field(line, start, end)?
        .map(str::parse::<f64>)
        .transpose()
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn checksum_mismatch_fails_before_parsing() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "not a catalogue").unwrap();
        let input = PinnedCatalogueInput {
            path: file.path().to_path_buf(),
            provenance: BrightStarInputProvenance {
                role: BrightStarInputRole::Hipparcos2,
                source_id: "I/311".into(),
                release: "2008-09-16".into(),
                sha256: "0".repeat(64),
                retrieval_url: "https://cdsarc.cds.unistra.fr/ftp/I/311/hip2.dat".into(),
                license_or_terms_url: "https://www.cosmos.esa.int/web/esdc/terms-and-conditions"
                    .into(),
            },
        };
        assert!(ingest_hipparcos2(&input)
            .unwrap_err()
            .to_string()
            .contains("checksum mismatch"));
    }
}
