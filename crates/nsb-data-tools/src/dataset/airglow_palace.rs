//! Deterministic PALACE v1.0 continuum projection.
//!
//! Only the CC-BY-4.0 model-data tables are read. The GPL PALACE program is
//! deliberately not copied or executed: the transformation below preserves
//! the three published continuum templates and their published climatology.

use super::{Artifact, RunConfig, ValidationGate};
use crate::dataset::engine::atomic_write;
use crate::platform::checksum_io;
use anyhow::{bail, Context, Result};
use std::fs;
use std::io::Read;
use std::path::Path;

pub(crate) const OUTPUT_NAME: &str = "airglow_palace_v1.dat";
const SOURCE_NAME: &str = "PALACE-v1.0.zip";
const CONT_PATH: &str = "PALACE/src/palace/data/palace_cont.fits";
const VAR_PATH: &str = "PALACE/src/palace/data/palace_var.fits";
const COMPONENTS: [(&str, &str, f32, usize); 3] = [
    ("HO2", "HO2", 81.0, 47),
    ("FeO", "FeO", 88.0, 50),
    ("O2", "O2Ac", 94.0, 44),
];

pub(crate) fn validate_config(config: &RunConfig) -> Result<()> {
    if config.sources.len() != 1 || config.sources[0].name != SOURCE_NAME {
        bail!("airglow-continuum requires one source named {SOURCE_NAME}");
    }
    let source = &config.sources[0];
    for (label, value) in [
        ("release", source.release.as_deref()),
        ("metadata_url", source.metadata_url.as_deref()),
        ("license", source.license.as_deref()),
        ("units", source.units.as_deref()),
    ] {
        if value.is_none_or(str::is_empty) {
            bail!("PALACE source requires {label}");
        }
    }
    Ok(())
}

pub(crate) fn build(config: &RunConfig) -> Result<Vec<Artifact>> {
    let input = config.workspace.root.join("sources").join(SOURCE_NAME);
    if !input.is_file() {
        bail!("updated PALACE source is missing; run update first");
    }
    let mut archive = zip::ZipArchive::new(fs::File::open(&input)?)?;
    let continuum = read_zip_member(&mut archive, CONT_PATH)?;
    let variability = read_zip_member(&mut archive, VAR_PATH)?;
    let bytes = generate(&continuum, &variability)?;
    let output_root = config.workspace.root.join("outputs");
    let output = output_root.join(OUTPUT_NAME);
    atomic_write(&output, &bytes)?;
    let artifact = artifact(&output)?;
    atomic_write(
        &output_root.join("artifacts.json"),
        &serde_json::to_vec_pretty(&vec![&artifact])?,
    )?;
    Ok(vec![artifact])
}

fn read_zip_member<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
) -> Result<Vec<u8>> {
    let mut member = archive
        .by_name(name)
        .with_context(|| format!("PALACE archive is missing {name}"))?;
    let mut bytes = Vec::with_capacity(member.size() as usize);
    member.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn artifact(path: &Path) -> Result<Artifact> {
    Ok(Artifact {
        name: OUTPUT_NAME.to_string(),
        path: path.to_path_buf(),
        sha256: checksum_io::sha256_file(path)?,
        bytes: fs::metadata(path)?.len(),
    })
}

#[derive(Debug)]
struct FitsTable<'a> {
    bytes: &'a [u8],
    data_offset: usize,
    row_len: usize,
    rows: usize,
}

impl<'a> FitsTable<'a> {
    fn parse(bytes: &'a [u8]) -> Result<Self> {
        let (_, primary_end) = read_header(bytes, 0)?;
        let (header, data_offset) = read_header(bytes, primary_end)?;
        if header_value(&header, "XTENSION")?
            .trim_matches(' ')
            .trim_matches('\'')
            != "BINTABLE"
        {
            bail!("PALACE FITS extension is not BINTABLE");
        }
        let row_len = parse_header_usize(&header, "NAXIS1")?;
        let rows = parse_header_usize(&header, "NAXIS2")?;
        let data_len = row_len
            .checked_mul(rows)
            .context("FITS table size overflow")?;
        if data_offset + data_len > bytes.len() {
            bail!("truncated PALACE FITS table");
        }
        Ok(Self {
            bytes,
            data_offset,
            row_len,
            rows,
        })
    }

    fn row(&self, index: usize) -> Result<&'a [u8]> {
        if index >= self.rows {
            bail!("FITS row {index} is out of bounds");
        }
        let start = self.data_offset + index * self.row_len;
        Ok(&self.bytes[start..start + self.row_len])
    }
}

fn read_header(bytes: &[u8], start: usize) -> Result<(Vec<String>, usize)> {
    let mut cards = Vec::new();
    let mut offset = start;
    loop {
        if offset + 80 > bytes.len() {
            bail!("truncated FITS header");
        }
        let card = std::str::from_utf8(&bytes[offset..offset + 80])?.to_string();
        offset += 80;
        let end = card.starts_with("END     ");
        cards.push(card);
        if end {
            let padded = offset.div_ceil(2880) * 2880;
            return Ok((cards, padded));
        }
    }
}

fn header_value<'a>(cards: &'a [String], key: &str) -> Result<&'a str> {
    let card = cards
        .iter()
        .find(|card| card.get(..8).is_some_and(|field| field.trim() == key))
        .with_context(|| format!("FITS header is missing {key}"))?;
    let value = card
        .get(10..)
        .context("malformed FITS card")?
        .split('/')
        .next()
        .unwrap_or_default()
        .trim();
    Ok(value)
}

fn parse_header_usize(cards: &[String], key: &str) -> Result<usize> {
    Ok(header_value(cards, key)?.parse()?)
}

fn be_i32(row: &[u8], offset: usize) -> Result<i32> {
    Ok(i32::from_be_bytes(
        row.get(offset..offset + 4)
            .context("truncated FITS i32 field")?
            .try_into()?,
    ))
}

fn be_f32(row: &[u8], offset: usize) -> Result<f32> {
    Ok(f32::from_bits(u32::from_be_bytes(
        row.get(offset..offset + 4)
            .context("truncated FITS f32 field")?
            .try_into()?,
    )))
}

fn be_f64(row: &[u8], offset: usize) -> Result<f64> {
    Ok(f64::from_bits(u64::from_be_bytes(
        row.get(offset..offset + 8)
            .context("truncated FITS f64 field")?
            .try_into()?,
    )))
}

fn generate(continuum: &[u8], variability: &[u8]) -> Result<Vec<u8>> {
    let cont = FitsTable::parse(continuum)?;
    let var = FitsTable::parse(variability)?;
    if cont.row_len != 32 || cont.rows != 110_001 || var.row_len != 296 || var.rows != 144 {
        bail!("unexpected PALACE v1.0 FITS table shape");
    }

    let mut out = String::new();
    out.push_str("# NSB PALACE v1.0 continuum runtime product\n");
    out.push_str("# Derived only from palace_cont.fits and palace_var.fits; lines are excluded.\n");
    out.push_str("# source_release = PALACE v1.0 (2024-10)\n");
    out.push_str("# source_record = https://zenodo.org/records/14064023\n");
    out.push_str("# source_archive_sha256 = a42577ffee8f76d9ac3765a9a2811d766c30158cf74c0a364ebf7a726dd9a0dc\n");
    out.push_str("# source_inputs = PALACE/src/palace/data/palace_cont.fits; PALACE/src/palace/data/palace_var.fits\n");
    out.push_str("# scientific_role = unresolved continuum templates plus month/local-time/F10.7 climatology and residual variability; PALACE line list excluded\n");
    out.push_str("# units = wavelength nm; source continuum radiance R nm^-1; dimensionless climatology factors\n");
    out.push_str("# runtime_band_nm = 300-650\n");
    out.push_str("schema nsb-airglow-palace-continuum-v1\n");
    out.push_str("wavelength_unit nm\n");
    out.push_str("radiance_unit R_per_nm\n");
    out.push_str("reference_solar_flux_sfu 100\n");
    out.push_str("solar_flux_evidence_range_sfu 67 166\n");
    out.push_str("wavelength_range_nm 300 650\n");
    out.push_str("wavelength_step_nm 1\n");
    out.push_str("component_count 3\n");
    out.push_str("climatology_rows 144\n");
    for (name, variability_class, height, _) in COMPONENTS {
        out.push_str(&format!(
            "component {name} {variability_class} {height:.1}\n"
        ));
    }
    out.push_str("spectra_begin\n");
    for wavelength_nm in 300..=650 {
        let index = ((wavelength_nm - 300) * 50) as usize;
        let row = cont.row(index)?;
        let wavelength_um = be_f64(row, 0)?;
        if ((wavelength_um * 1000.0) - f64::from(wavelength_nm)).abs() > 1.0e-8 {
            bail!("PALACE continuum wavelength grid mismatch at {wavelength_nm} nm");
        }
        let values = [be_f32(row, 16)?, be_f32(row, 20)?, be_f32(row, 24)?];
        if values
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
        {
            bail!("invalid PALACE continuum value at {wavelength_nm} nm");
        }
        out.push_str(&format!(
            "{wavelength_nm} {:.7e} {:.7e} {:.7e}\n",
            values[0], values[1], values[2]
        ));
    }
    out.push_str("spectra_end\nclimatology_begin\n");
    for index in 0..var.rows {
        let row = var.row(index)?;
        let month = be_i32(row, 0)?;
        let time_bin = be_i32(row, 8)?;
        let weight = be_f32(row, 16)?;
        if month != (index / 12 + 1) as i32 || time_bin != (index % 12 + 1) as i32 {
            bail!("PALACE climatology rows are not canonical month/time order");
        }
        out.push_str(&format!("{month} {time_bin} {:.7e}", weight));
        for (_, _, _, field_index) in COMPONENTS {
            let offset = 20 + (field_index - 5) * 4;
            let mean = be_f32(row, offset)?;
            let solar_slope = be_f32(row, offset + 4)?;
            let sigma = be_f32(row, offset + 8)?;
            if !mean.is_finite()
                || !solar_slope.is_finite()
                || !sigma.is_finite()
                || mean < 0.0
                || sigma < 0.0
            {
                bail!("invalid PALACE climatology value at month {month}, bin {time_bin}");
            }
            out.push_str(&format!(" {:.7e} {:.7e} {:.7e}", mean, solar_slope, sigma));
        }
        out.push('\n');
    }
    out.push_str("climatology_end\n");
    Ok(out.into_bytes())
}

pub(crate) fn validate_artifact(path: &Path) -> Result<()> {
    let text = fs::read_to_string(path)?;
    let mut lines = text.lines();
    if !text.contains("schema nsb-airglow-palace-continuum-v1")
        || !text.contains("spectra_begin")
        || !text.contains("spectra_end")
        || !text.contains("climatology_begin")
        || !text.ends_with("climatology_end\n")
    {
        bail!("{OUTPUT_NAME} does not satisfy the v1 framing contract");
    }
    let spectral_rows = lines
        .by_ref()
        .skip_while(|line| *line != "spectra_begin")
        .skip(1)
        .take_while(|line| *line != "spectra_end")
        .count();
    let climatology_rows = text
        .lines()
        .skip_while(|line| *line != "climatology_begin")
        .skip(1)
        .take_while(|line| *line != "climatology_end")
        .count();
    if spectral_rows != 351 || climatology_rows != 144 {
        bail!("unexpected PALACE runtime row counts: {spectral_rows} spectra, {climatology_rows} climatology");
    }
    Ok(())
}

pub(crate) fn validation_gates(artifacts: &[Artifact]) -> Result<Vec<ValidationGate>> {
    let artifact = artifacts
        .iter()
        .find(|artifact| artifact.name == OUTPUT_NAME)
        .context("PALACE runtime artifact is missing")?;
    validate_artifact(&artifact.path)?;
    Ok(vec![
        ValidationGate {
            name: "palace-continuum-only".into(),
            passed: true,
            detail: "three PALACE continuum components; palace_lines.fits is not an input".into(),
        },
        ValidationGate {
            name: "palace-domain".into(),
            passed: true,
            detail: "351 samples cover 300..650 nm; 12 months x 12 local-time bins; bin-specific F10.7 slopes and residual variability".into(),
        },
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn committed_product_satisfies_schema() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../nsb/data")
            .join(OUTPUT_NAME);
        if path.is_file() {
            validate_artifact(&path).unwrap();
        }
    }

    #[test]
    fn generator_is_byte_deterministic_for_pinned_archive_when_available() {
        let archive_path = std::env::var_os("NSB_PALACE_V1_ARCHIVE").map(PathBuf::from);
        let Some(archive_path) = archive_path else {
            return;
        };
        let mut archive = zip::ZipArchive::new(fs::File::open(archive_path).unwrap()).unwrap();
        let continuum = read_zip_member(&mut archive, CONT_PATH).unwrap();
        let variability = read_zip_member(&mut archive, VAR_PATH).unwrap();
        let first = generate(&continuum, &variability).unwrap();
        let second = generate(&continuum, &variability).unwrap();
        assert_eq!(first, second);
        let committed = fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../nsb/data")
                .join(OUTPUT_NAME),
        )
        .unwrap();
        assert_eq!(first, committed);
    }
}
