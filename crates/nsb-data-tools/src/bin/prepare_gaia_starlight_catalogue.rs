use anyhow::{bail, Context, Result};
use clap::Parser;
use csv::{ReaderBuilder, StringRecord, WriterBuilder};
use serde::Serialize;
use siderust::catalogs::gaia::{
    integrate_photon_flux, GaiaDr3QualityFlags, GaiaDr3RawSourceRow, GaiaDr3Source,
    GaiaXpSampledSpectrum, PassbandIntegratedStellarSource, SpectralBand, SpectralFluxSample,
    StellarPhotometryModel,
};
use siderust::checksum::{sha256, to_hex};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;

const GAIA_XP_MODEL: &str = "gaia_dr3_xp_photon_radiance_330_650nm_v1";

/// Prepare canonical Gaia DR3 passband-integrated starlight source rows.
///
/// This release-time tool consumes a local maintainer Gaia DR3 extract. It does
/// not download Gaia data and it does not emit raw Gaia XP spectra into the NSB
/// runtime product.
#[derive(Debug, Parser)]
struct Args {
    /// Maintainer-generated Gaia DR3 extract CSV.
    #[arg(long)]
    input: PathBuf,
    /// Canonical derived source CSV. Use '-' for stdout.
    #[arg(long)]
    output: PathBuf,
    /// Optional JSON diagnostics report.
    #[arg(long)]
    diagnostics_output: Option<PathBuf>,
    /// Source catalogue name, normally "Gaia".
    #[arg(long)]
    catalog_name: String,
    /// Source catalogue release, normally "DR3".
    #[arg(long)]
    catalog_release: String,
    /// Reviewed catalogue license or derived-product redistribution policy.
    #[arg(long)]
    catalog_license: String,
    /// Expected SHA-256 of the raw Gaia extract.
    #[arg(long)]
    source_checksum: Option<String>,
    /// Photometry model identifier.
    #[arg(long, default_value = GAIA_XP_MODEL)]
    photometry_model: String,
    /// Lower wavelength bound, nm.
    #[arg(long, default_value_t = 330.0)]
    band_min_nm: f64,
    /// Upper wavelength bound, nm.
    #[arg(long, default_value_t = 650.0)]
    band_max_nm: f64,
    /// Require XP passband photometry for every accepted row.
    #[arg(long)]
    require_passband_photometry: bool,
}

#[derive(Debug, Clone, Copy)]
struct Columns {
    source_id: usize,
    ra: usize,
    dec: usize,
    ref_epoch: usize,
    pmra: Option<usize>,
    pmdec: Option<usize>,
    parallax: Option<usize>,
    radial_velocity: Option<usize>,
    phot_g_mean_mag: Option<usize>,
    phot_bp_mean_mag: Option<usize>,
    phot_rp_mean_mag: Option<usize>,
    xp_wavelength_nm: usize,
    xp_flux_w_m2_nm: usize,
    quality_ok: Option<usize>,
    duplicated_source: Option<usize>,
}

#[derive(Debug, Serialize)]
struct Diagnostics<'a> {
    schema_version: u32,
    catalogue_name: &'a str,
    catalogue_release: &'a str,
    catalogue_license: &'a str,
    input_checksum: String,
    photometry_model: &'a str,
    siderust_photometry_model: String,
    band_min_nm: f64,
    band_max_nm: f64,
    rows_read: usize,
    rows_used: usize,
    rows_rejected: usize,
    rejection_reasons: BTreeMap<String, usize>,
}

fn main() -> Result<()> {
    run(Args::parse())
}

fn run(args: Args) -> Result<()> {
    validate_args(&args)?;
    let input_checksum = checksum_input(&args)?;
    let band = SpectralBand::new(
        "Gaia XP stellar radiance release band",
        args.band_min_nm,
        args.band_max_nm,
    )
    .map_err(|err| anyhow::anyhow!(err.to_string()))?;

    let mut reader = ReaderBuilder::new()
        .comment(Some(b'#'))
        .trim(csv::Trim::All)
        .from_path(&args.input)
        .with_context(|| format!("failed to open Gaia extract {}", args.input.display()))?;
    let headers = reader
        .headers()
        .context("failed to read Gaia CSV header")?
        .clone();
    let columns = Columns::from_headers(&headers)?;

    let mut writer = WriterBuilder::new().from_writer(output_writer(&args.output)?);
    writer.write_record([
        "source_id",
        "ra_deg",
        "dec_deg",
        "epoch_jyr",
        "photon_flux_ph_m2_s",
        "photometry_model",
        "weight",
    ])?;

    let mut rows_read = 0usize;
    let mut rows_used = 0usize;
    let mut rejection_reasons = BTreeMap::<String, usize>::new();

    for row in reader.records() {
        let row = row.context("failed to read Gaia CSV record")?;
        rows_read += 1;
        match convert_row(&row, columns, band, &args) {
            Ok(Some(output)) => {
                rows_used += 1;
                writer.write_record(output)?;
            }
            Ok(None) => {
                *rejection_reasons
                    .entry("missing passband photometry".to_string())
                    .or_default() += 1;
            }
            Err(err) => {
                *rejection_reasons.entry(err.to_string()).or_default() += 1;
            }
        }
    }
    writer.flush()?;

    if rows_used == 0 {
        bail!("Gaia preparation produced no accepted starlight sources");
    }

    if let Some(path) = &args.diagnostics_output {
        let diagnostics = Diagnostics {
            schema_version: 1,
            catalogue_name: &args.catalog_name,
            catalogue_release: &args.catalog_release,
            catalogue_license: &args.catalog_license,
            input_checksum,
            photometry_model: &args.photometry_model,
            siderust_photometry_model: format!(
                "{:?}",
                StellarPhotometryModel::GaiaDr3XpPhotonRadiance330650NmV1
            ),
            band_min_nm: args.band_min_nm,
            band_max_nm: args.band_max_nm,
            rows_read,
            rows_used,
            rows_rejected: rows_read - rows_used,
            rejection_reasons,
        };
        let raw = serde_json::to_string_pretty(&diagnostics)?;
        std::fs::write(path, format!("{raw}\n"))
            .with_context(|| format!("failed to write diagnostics {}", path.display()))?;
    }

    Ok(())
}

fn validate_args(args: &Args) -> Result<()> {
    for (name, value) in [
        ("--catalog-name", &args.catalog_name),
        ("--catalog-release", &args.catalog_release),
        ("--catalog-license", &args.catalog_license),
        ("--photometry-model", &args.photometry_model),
    ] {
        if value.trim().is_empty() {
            bail!("{name} must not be empty");
        }
    }
    if args.photometry_model != GAIA_XP_MODEL {
        bail!(
            "unsupported Gaia production photometry model {}",
            args.photometry_model
        );
    }
    if !args.band_min_nm.is_finite()
        || !args.band_max_nm.is_finite()
        || args.band_min_nm >= args.band_max_nm
    {
        bail!("band bounds must be finite and satisfy min < max");
    }
    Ok(())
}

fn checksum_input(args: &Args) -> Result<String> {
    let bytes = std::fs::read(&args.input)
        .with_context(|| format!("failed to checksum {}", args.input.display()))?;
    let actual = format!("sha256:{}", to_hex(&sha256(&bytes)));
    if let Some(expected) = args.source_checksum.as_deref() {
        let expected = expected.strip_prefix("sha256:").unwrap_or(expected);
        let actual_digest = actual.trim_start_matches("sha256:");
        if expected != actual_digest {
            bail!(
                "source checksum mismatch for {}: expected sha256:{expected}, actual {actual}",
                args.input.display()
            );
        }
    }
    Ok(actual)
}

impl Columns {
    fn from_headers(headers: &StringRecord) -> Result<Self> {
        Ok(Self {
            source_id: required_header(headers, "source_id")?,
            ra: required_header(headers, "ra")?,
            dec: required_header(headers, "dec")?,
            ref_epoch: required_header(headers, "ref_epoch")?,
            pmra: optional_header(headers, "pmra"),
            pmdec: optional_header(headers, "pmdec"),
            parallax: optional_header(headers, "parallax"),
            radial_velocity: optional_header(headers, "radial_velocity"),
            phot_g_mean_mag: optional_header(headers, "phot_g_mean_mag"),
            phot_bp_mean_mag: optional_header(headers, "phot_bp_mean_mag"),
            phot_rp_mean_mag: optional_header(headers, "phot_rp_mean_mag"),
            xp_wavelength_nm: required_header(headers, "xp_wavelength_nm")?,
            xp_flux_w_m2_nm: required_header(headers, "xp_flux_w_m2_nm")?,
            quality_ok: optional_header(headers, "quality_ok"),
            duplicated_source: optional_header(headers, "duplicated_source"),
        })
    }
}

fn convert_row(
    row: &StringRecord,
    columns: Columns,
    band: SpectralBand,
    args: &Args,
) -> Result<Option<[String; 7]>> {
    let raw = GaiaDr3RawSourceRow {
        source_id: parse_u64(row, columns.source_id, "source_id")?,
        ra_deg: parse_required_f64(row, columns.ra, "ra")?,
        dec_deg: parse_required_f64(row, columns.dec, "dec")?,
        ref_epoch_jyr: parse_required_f64(row, columns.ref_epoch, "ref_epoch")?,
        pmra_mas_per_yr: parse_optional_f64(row, columns.pmra, "pmra")?,
        pmdec_mas_per_yr: parse_optional_f64(row, columns.pmdec, "pmdec")?,
        parallax_mas: parse_optional_f64(row, columns.parallax, "parallax")?,
        radial_velocity_km_s: parse_optional_f64(row, columns.radial_velocity, "radial_velocity")?,
        phot_g_mean_mag: parse_optional_f64(row, columns.phot_g_mean_mag, "phot_g_mean_mag")?,
        phot_bp_mean_mag: parse_optional_f64(row, columns.phot_bp_mean_mag, "phot_bp_mean_mag")?,
        phot_rp_mean_mag: parse_optional_f64(row, columns.phot_rp_mean_mag, "phot_rp_mean_mag")?,
        quality: GaiaDr3QualityFlags {
            quality_ok: parse_optional_bool(row, columns.quality_ok).unwrap_or(true),
            duplicated_source: parse_optional_bool(row, columns.duplicated_source),
        },
    };

    let source = GaiaDr3Source::try_from(raw).map_err(|err| anyhow::anyhow!(err.to_string()))?;
    let samples = parse_samples(row, columns)?;
    if samples.is_empty() {
        if args.require_passband_photometry {
            bail!("missing passband photometry");
        }
        return Ok(None);
    }
    let spectrum =
        GaiaXpSampledSpectrum::new(samples).map_err(|err| anyhow::anyhow!(err.to_string()))?;
    let photon_flux =
        integrate_photon_flux(&spectrum, band).map_err(|err| anyhow::anyhow!(err.to_string()))?;
    let stellar_source = PassbandIntegratedStellarSource::from_gaia_source(
        &source,
        photon_flux,
        StellarPhotometryModel::GaiaDr3XpPhotonRadiance330650NmV1,
    );

    Ok(Some([
        stellar_source.source_id.to_string(),
        format!("{:.10}", source.astrometry.direction.azimuth.value()),
        format!("{:.10}", source.astrometry.direction.polar.value()),
        format!("{:.6}", stellar_source.epoch.value()),
        format!("{:.16e}", stellar_source.photon_flux.photons_m2_s()),
        args.photometry_model.clone(),
        "1.0000000000".to_string(),
    ]))
}

fn parse_samples(row: &StringRecord, columns: Columns) -> Result<Vec<SpectralFluxSample>> {
    let wavelengths = split_series(row, columns.xp_wavelength_nm, "xp_wavelength_nm")?;
    let fluxes = split_series(row, columns.xp_flux_w_m2_nm, "xp_flux_w_m2_nm")?;
    if wavelengths.len() != fluxes.len() {
        bail!("XP wavelength and flux arrays must have equal length");
    }
    wavelengths
        .into_iter()
        .zip(fluxes)
        .map(|(wavelength, flux)| {
            SpectralFluxSample::new(wavelength, flux)
                .map_err(|err| anyhow::anyhow!(err.to_string()))
        })
        .collect()
}

fn split_series(row: &StringRecord, idx: usize, name: &str) -> Result<Vec<f64>> {
    let raw = row
        .get(idx)
        .ok_or_else(|| anyhow::anyhow!("missing field {name:?}"))?
        .trim();
    if raw.is_empty() {
        return Ok(Vec::new());
    }
    raw.split(';')
        .map(|value| {
            value
                .trim()
                .parse::<f64>()
                .with_context(|| format!("invalid numeric value in {name:?}"))
        })
        .collect()
}

fn required_header(headers: &StringRecord, name: &str) -> Result<usize> {
    optional_header(headers, name)
        .ok_or_else(|| anyhow::anyhow!("missing required column {name:?}"))
}

fn optional_header(headers: &StringRecord, name: &str) -> Option<usize> {
    headers.iter().position(|h| h.trim() == name)
}

fn parse_u64(row: &StringRecord, idx: usize, name: &str) -> Result<u64> {
    row.get(idx)
        .ok_or_else(|| anyhow::anyhow!("missing field {name:?}"))?
        .trim()
        .parse::<u64>()
        .with_context(|| format!("invalid integer field {name:?}"))
}

fn parse_required_f64(row: &StringRecord, idx: usize, name: &str) -> Result<f64> {
    row.get(idx)
        .ok_or_else(|| anyhow::anyhow!("missing field {name:?}"))?
        .trim()
        .parse::<f64>()
        .with_context(|| format!("invalid numeric field {name:?}"))
}

fn parse_optional_f64(row: &StringRecord, idx: Option<usize>, name: &str) -> Result<Option<f64>> {
    let Some(idx) = idx else {
        return Ok(None);
    };
    let Some(raw) = row
        .get(idx)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    raw.parse::<f64>()
        .map(Some)
        .with_context(|| format!("invalid numeric field {name:?}"))
}

fn parse_optional_bool(row: &StringRecord, idx: Option<usize>) -> Option<bool> {
    idx.and_then(|idx| row.get(idx))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "t" | "yes" | "ok"
            )
        })
}

fn output_writer(path: &PathBuf) -> Result<Box<dyn Write>> {
    if path.as_os_str() == OsStr::new("-") {
        Ok(Box::new(BufWriter::new(io::stdout())))
    } else {
        Ok(Box::new(BufWriter::new(File::create(path).with_context(
            || format!("failed to create output catalogue {}", path.display()),
        )?)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(input: PathBuf, output: PathBuf, diagnostics: PathBuf) -> Args {
        Args {
            input,
            output,
            diagnostics_output: Some(diagnostics),
            catalog_name: "Gaia".to_string(),
            catalog_release: "DR3".to_string(),
            catalog_license: "CC-BY-4.0-derived-policy-reviewed".to_string(),
            source_checksum: None,
            photometry_model: GAIA_XP_MODEL.to_string(),
            band_min_nm: 330.0,
            band_max_nm: 650.0,
            require_passband_photometry: true,
        }
    }

    #[test]
    fn tiny_gaia_fixture_prepares_canonical_sources() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("gaia.csv");
        let output = dir.path().join("canonical.csv");
        let diagnostics = dir.path().join("diagnostics.json");
        std::fs::write(
            &input,
            concat!(
                "source_id,ra,dec,ref_epoch,pmra,pmdec,parallax,phot_g_mean_mag,xp_wavelength_nm,xp_flux_w_m2_nm,quality_ok\n",
                "42,120.0,-30.0,2016.0,1.0,2.0,3.0,12.0,330;650,1e-12;1e-12,true\n"
            ),
        )?;

        run(args(input, output.clone(), diagnostics.clone()))?;

        let canonical = std::fs::read_to_string(output)?;
        assert!(canonical.starts_with(
            "source_id,ra_deg,dec_deg,epoch_jyr,photon_flux_ph_m2_s,photometry_model,weight"
        ));
        assert!(canonical.contains(GAIA_XP_MODEL));
        let report: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(diagnostics)?)?;
        assert_eq!(report["rows_read"], 1);
        assert_eq!(report["rows_used"], 1);
        assert_eq!(report["photometry_model"], GAIA_XP_MODEL);
        Ok(())
    }

    #[test]
    fn invalid_coordinates_are_rejected_as_empty_output() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("gaia.csv");
        let output = dir.path().join("canonical.csv");
        let diagnostics = dir.path().join("diagnostics.json");
        std::fs::write(
            &input,
            "source_id,ra,dec,ref_epoch,xp_wavelength_nm,xp_flux_w_m2_nm\n42,360.0,0.0,2016.0,330;650,1e-12;1e-12\n",
        )?;

        let err = run(args(input, output, diagnostics)).expect_err("all rows rejected");
        assert!(err.to_string().contains("no accepted starlight sources"));
        Ok(())
    }

    #[test]
    fn malformed_spectra_are_rejected() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("gaia.csv");
        let output = dir.path().join("canonical.csv");
        let diagnostics = dir.path().join("diagnostics.json");
        std::fs::write(
            &input,
            "source_id,ra,dec,ref_epoch,xp_wavelength_nm,xp_flux_w_m2_nm\n42,10.0,0.0,2016.0,650;330,1e-12;1e-12\n",
        )?;

        let err = run(args(input, output, diagnostics)).expect_err("all rows rejected");
        assert!(err.to_string().contains("no accepted starlight sources"));
        Ok(())
    }

    #[test]
    fn checksum_mismatch_is_rejected() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("gaia.csv");
        let output = dir.path().join("canonical.csv");
        let diagnostics = dir.path().join("diagnostics.json");
        std::fs::write(
            &input,
            "source_id,ra,dec,ref_epoch,xp_wavelength_nm,xp_flux_w_m2_nm\n42,10.0,0.0,2016.0,330;650,1e-12;1e-12\n",
        )?;
        let mut args = args(input, output, diagnostics);
        args.source_checksum = Some(format!("sha256:{}", "0".repeat(64)));

        let err = run(args).expect_err("checksum mismatch");
        assert!(err.to_string().contains("source checksum mismatch"));
        Ok(())
    }
}
