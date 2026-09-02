//! GaiaSource partition ingest shared by production and diagnostic workers.

use crate::starlight::healpix::IcrsSkyPosition;
use anyhow::{bail, Context, Result};
use csv::ReaderBuilder;
use flate2::read::GzDecoder;
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

const CSV_BUFFER_CAPACITY: usize = 1024 * 1024;

#[derive(Debug, Clone)]
pub(crate) struct GaiaSourceEntry {
    #[allow(dead_code)]
    pub source_id: u64,
    pub icrs: IcrsSkyPosition,
    pub phot_g_mean_mag: Option<f64>,
    pub phot_bp_mean_mag: Option<f64>,
    pub phot_rp_mean_mag: Option<f64>,
    pub bp_rp: Option<f64>,
    pub duplicated_source: bool,
    pub in_qso_candidates: bool,
    pub in_galaxy_candidates: bool,
    pub predictors: Option<BTreeMap<String, f64>>,
}

pub(crate) fn load_gaia_sources(
    path: &Path,
    predictor_names: &[String],
) -> Result<HashMap<u64, GaiaSourceEntry>> {
    let decoder = GzDecoder::new(
        File::open(path).with_context(|| format!("open GaiaSource object {}", path.display()))?,
    );
    let mut reader = ReaderBuilder::new()
        .comment(Some(b'#'))
        .buffer_capacity(CSV_BUFFER_CAPACITY)
        .from_reader(BufReader::new(decoder));
    let headers = reader.headers()?.clone();
    let source_index = headers
        .iter()
        .position(|header| header.trim() == "source_id")
        .context("GaiaSource partition has no source_id column")?;
    let ra_index = headers
        .iter()
        .position(|header| header.trim() == "ra")
        .context("GaiaSource partition has no ra column")?;
    let dec_index = headers
        .iter()
        .position(|header| header.trim() == "dec")
        .context("GaiaSource partition has no dec column")?;
    let phot_g_index = optional_column(&headers, "phot_g_mean_mag");
    let phot_bp_index = optional_column(&headers, "phot_bp_mean_mag");
    let phot_rp_index = optional_column(&headers, "phot_rp_mean_mag");
    let bp_rp_index = optional_column(&headers, "bp_rp");
    let duplicated_index = optional_column(&headers, "duplicated_source");
    let qso_index = optional_column(&headers, "in_qso_candidates");
    let galaxy_index = optional_column(&headers, "in_galaxy_candidates");
    let predictor_indexes = predictor_names
        .iter()
        .map(|name| {
            headers
                .iter()
                .position(|header| header.trim() == name)
                .with_context(|| format!("GaiaSource partition has no UV predictor column {name}"))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut source_ids = HashMap::new();
    for (row_index, row) in reader.records().enumerate() {
        let row = row.with_context(|| {
            format!(
                "read GaiaSource row {} from {}",
                row_index + 2,
                path.display()
            )
        })?;
        let source_id = row
            .get(source_index)
            .context("GaiaSource row has no source_id field")?
            .trim()
            .parse::<u64>()
            .context("GaiaSource source_id is not u64")?;
        let ra_deg = row
            .get(ra_index)
            .context("GaiaSource row has no ra field")?
            .trim()
            .parse::<f64>()
            .context("GaiaSource ra is not numeric")?;
        let dec_deg = row
            .get(dec_index)
            .context("GaiaSource row has no dec field")?
            .trim()
            .parse::<f64>()
            .context("GaiaSource dec is not numeric")?;
        let icrs = match IcrsSkyPosition::new(ra_deg, dec_deg) {
            Ok(position) => position,
            Err(_) => continue,
        };
        let predictors = predictor_names
            .iter()
            .zip(&predictor_indexes)
            .map(|(name, index)| {
                let value = row
                    .get(*index)
                    .context("GaiaSource row has no UV predictor field")?
                    .trim()
                    .parse::<f64>()
                    .context("GaiaSource UV predictor is not numeric")?;
                if !value.is_finite() {
                    bail!("GaiaSource UV predictor is not finite");
                }
                Ok((name.clone(), value))
            })
            .collect::<Result<BTreeMap<_, _>>>()
            .ok();
        let entry = GaiaSourceEntry {
            source_id,
            icrs,
            phot_g_mean_mag: optional_f64(&row, phot_g_index)?,
            phot_bp_mean_mag: optional_f64(&row, phot_bp_index)?,
            phot_rp_mean_mag: optional_f64(&row, phot_rp_index)?,
            bp_rp: optional_f64(&row, bp_rp_index)?,
            duplicated_source: optional_bool(&row, duplicated_index)?,
            in_qso_candidates: optional_bool(&row, qso_index)?,
            in_galaxy_candidates: optional_bool(&row, galaxy_index)?,
            predictors,
        };
        if source_ids.insert(source_id, entry).is_some() {
            bail!("GaiaSource partition contains duplicate source_id {source_id}");
        }
    }
    Ok(source_ids)
}

fn optional_column(headers: &csv::StringRecord, name: &str) -> Option<usize> {
    headers.iter().position(|header| header.trim() == name)
}

fn optional_f64(row: &csv::StringRecord, index: Option<usize>) -> Result<Option<f64>> {
    let Some(index) = index else {
        return Ok(None);
    };
    let raw = row
        .get(index)
        .context("GaiaSource row is missing an optional numeric field")?
        .trim();
    if raw.is_empty() || raw.eq_ignore_ascii_case("null") || raw == "nan" {
        return Ok(None);
    }
    let value = raw
        .parse::<f64>()
        .with_context(|| format!("GaiaSource numeric field is invalid: {raw}"))?;
    if !value.is_finite() {
        return Ok(None);
    }
    Ok(Some(value))
}

fn optional_bool(row: &csv::StringRecord, index: Option<usize>) -> Result<bool> {
    let Some(index) = index else {
        return Ok(false);
    };
    let raw = row
        .get(index)
        .context("GaiaSource row is missing an optional boolean field")?
        .trim();
    if raw.is_empty() {
        return Ok(false);
    }
    match raw.to_ascii_lowercase().as_str() {
        "1" | "true" | "t" | "yes" => Ok(true),
        "0" | "false" | "f" | "no" => Ok(false),
        _ => bail!("GaiaSource boolean field is invalid: {raw}"),
    }
}
