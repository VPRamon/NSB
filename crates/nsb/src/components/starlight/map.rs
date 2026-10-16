// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
use super::interval::{DirectStarlightBounds, SpectralMap};
use super::output::StarlightOutputs;
use super::provenance::StarlightProvenance;
use super::validated::StarlightValidationDiagnostics;
use crate::error::{NsbError, Result};
use crate::site::AtmosphericConditions;
use crate::transport::DirectPathGeometry;
use crate::units::angular::Degrees;
use crate::units::length::Nanometers;
use crate::units::radiometry::{
    PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance, S10s,
};
use crate::units::solid_angle::Steradians;
use crate::units::PixelIntegratedPhotonFlux;
use csv::{ReaderBuilder, StringRecord};
use siderust::coordinates::cartesian::Direction as CartesianDirection;
use siderust::coordinates::frames::Galactic;
use siderust::coordinates::spherical::Direction as SphericalDirection;
use siderust::healpix::{HealpixGrid, HealpixIndex, HealpixMap, HealpixOrdering, Nside};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
/// One directional starlight-map sample.
///
/// Pixel centre coordinates and solid angle are derived from the owning
/// [`HealpixGrid`] at the sample index; they are not stored here so they cannot
/// drift from the geometry used for lookup.
pub struct StarlightPixel {
    /// Integrated photon radiance over the map's native band (300–650 nm for legacy maps).
    pub integrated: BandPhotonRadiance,
    /// B-reference S10 diagnostic.
    pub b_flux_s10: S10s,
    /// V-reference S10 diagnostic.
    pub v_flux_s10: S10s,
    /// Statistical one-sigma uncertainty of the integrated photon radiance.
    pub statistical_uncertainty: Option<BandPhotonRadiance>,
    /// Systematic one-sigma uncertainty of the integrated photon radiance.
    pub systematic_uncertainty: Option<BandPhotonRadiance>,
    /// Total one-sigma uncertainty of the integrated photon radiance.
    pub total_uncertainty: Option<BandPhotonRadiance>,
    /// Whether the map published measured B/V S10 diagnostics.
    pub s10_diagnostics_provided: bool,
}

impl StarlightPixel {
    /// Construct a map sample.
    pub fn new(integrated: BandPhotonRadiance, b_flux_s10: S10s, v_flux_s10: S10s) -> Self {
        Self {
            integrated,
            b_flux_s10,
            v_flux_s10,
            statistical_uncertainty: None,
            systematic_uncertainty: None,
            total_uncertainty: None,
            s10_diagnostics_provided: true,
        }
    }

    /// Mark B/V S10 as not provided by the packed candidate contract.
    pub fn without_s10_diagnostics(mut self) -> Self {
        self.s10_diagnostics_provided = false;
        self.b_flux_s10 = S10s::new(0.0);
        self.v_flux_s10 = S10s::new(0.0);
        self
    }

    /// Attach a complete absolute-uncertainty triplet to this sample.
    pub fn with_uncertainties(
        mut self,
        statistical: BandPhotonRadiance,
        systematic: BandPhotonRadiance,
        total: BandPhotonRadiance,
    ) -> Self {
        self.statistical_uncertainty = Some(statistical);
        self.systematic_uncertainty = Some(systematic);
        self.total_uncertainty = Some(total);
        self
    }

    fn output(self) -> StarlightOutputs {
        let mut output = StarlightOutputs::new(self.integrated, self.b_flux_s10, self.v_flux_s10);
        output.s10_diagnostics_provided = self.s10_diagnostics_provided;
        match (
            self.statistical_uncertainty,
            self.systematic_uncertainty,
            self.total_uncertainty,
        ) {
            (Some(statistical), Some(systematic), Some(total)) => {
                output.with_uncertainties(statistical, systematic, total)
            }
            (Some(statistical), None, None) => {
                output.statistical_uncertainty = Some(statistical);
                output
            }
            _ => output,
        }
    }

    fn validate(self) -> Result<()> {
        if !self.output().is_finite_non_negative()
            // A statistical-only XP product must not invent systematic/total sigma.
            || self.systematic_uncertainty.is_some() != self.total_uncertainty.is_some()
        {
            return Err(invalid_map(
                "pixel radiance and any provided uncertainties must be finite and non-negative; systematic and total must occur together and total must dominate both",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
/// Validated Galactic HEALPix starlight map.
pub struct StarlightMap {
    provenance: StarlightProvenance,
    map: HealpixMap<Galactic, StarlightPixel>,
    spectral: Option<Arc<SpectralMap>>,
}

impl StarlightMap {
    /// Build a complete HEALPix map from grid-ordered pixel values.
    pub fn from_healpix(
        grid: HealpixGrid,
        pixels: Vec<StarlightPixel>,
        provenance: StarlightProvenance,
    ) -> Result<Self> {
        if pixels.is_empty() {
            return Err(invalid_map("starlight map must contain at least one pixel"));
        }

        let has_uncertainties = pixels[0].output().has_uncertainties();
        let mut validated = Vec::with_capacity(pixels.len());
        for pixel in pixels {
            pixel.validate()?;
            if pixel.output().has_uncertainties() != has_uncertainties {
                return Err(invalid_map(
                    "all starlight pixels must use the same uncertainty schema",
                ));
            }
            validated.push(pixel);
        }

        let map = HealpixMap::new(grid, validated).map_err(|err| invalid_map(err.to_string()))?;
        Ok(Self {
            provenance,
            map,
            spectral: None,
        })
    }

    /// Parse a SHA-256-pinned spectral map in a supported versioned JSON format.
    ///
    /// The frozen Gaia XP v1 product and generic spectral v2 products share
    /// this API. Checksum verification is not scientific production admission.
    pub fn from_json_bytes(bytes: &[u8], expected_sha256: &str) -> Result<Self> {
        let spectral = SpectralMap::from_json_bytes(bytes, expected_sha256)?;
        Self::from_spectral(spectral)
    }

    fn from_spectral(spectral: SpectralMap) -> Result<Self> {
        let provenance = StarlightProvenance::new(
            spectral.product_id.clone(),
            "spectral-map",
            "not-recorded",
            spectral.source_catalogue.clone(),
            spectral.source_license.clone(),
            "see source selection",
            format!(
                "{}-{} nm native spectral bins",
                spectral.band_nm[0], spectral.band_nm[1]
            ),
            format!(
                "HEALPix nside={} ordering={:?}",
                spectral.grid.nside().get(),
                spectral.grid.ordering()
            ),
            Some(spectral.sha256.clone()),
        );
        let mut provenance = provenance;
        provenance.source_selection = Some(spectral.source_selection.clone());
        provenance.map_checksum = Some(spectral.sha256.clone());
        provenance.calibration_status = Some(spectral.maturity.clone());
        let pixels = spectral
            .pixels
            .iter()
            .map(|p| {
                let mut pixel = StarlightPixel::new(
                    BandPhotonRadiance::new(p.radiance.iter().sum()),
                    S10s::new(0.0),
                    S10s::new(0.0),
                )
                .without_s10_diagnostics();
                // Do not discard Gaia XP's full-band measured statistical sigma.
                // Total/systematic sigma remain unknown, not zero.
                pixel.statistical_uncertainty =
                    p.integrated_statistical_sigma.map(BandPhotonRadiance::new);
                pixel
            })
            .collect();
        let mut map = Self::from_healpix(spectral.grid, pixels, provenance)?;
        map.spectral = Some(Arc::new(spectral));
        Ok(map)
    }

    /// Load a checksum-pinned external Starlight spectral map from disk.
    pub fn from_json_file(path: &Path, expected_sha256: &str) -> Result<Self> {
        Self::from_spectral(SpectralMap::from_json_file(path, expected_sha256)?)
    }

    /// Band of the map's native integrated radiance, in vacuum nm.
    ///
    /// Legacy integrated Starlight maps use the historic 300–650 nm contract.
    pub fn band_nm(&self) -> [u16; 2] {
        self.spectral.as_ref().map_or([300, 650], |s| s.band_nm)
    }

    /// Edges of native integrated spectral bins in vacuum nm.
    pub fn bin_edges_nm(&self) -> &[u16] {
        self.spectral
            .as_ref()
            .map_or(&[300, 650], |s| s.edges_nm.as_slice())
    }

    /// Exact external map SHA-256 where recorded.
    pub fn sha256(&self) -> Option<&str> {
        self.provenance.map_checksum.as_deref()
    }

    /// Scientific maturity recorded by the map, not a production admission decision.
    pub fn maturity(&self) -> Option<&str> {
        self.provenance.calibration_status.as_deref()
    }

    /// HEALPix resolution.
    pub fn nside(&self) -> u32 {
        self.map.grid().nside().get()
    }

    /// Number of admitted sources when supplied by the spectral schema.
    pub fn admitted_sources(&self) -> Option<u64> {
        self.spectral.as_ref().map(|s| s.admitted_sources)
    }

    /// Native full-band TOA radiance and its measured statistical sigma when known.
    ///
    /// Missing uncertainty is `None`, never an invented zero.
    pub fn top_of_atmosphere_pixel(
        &self,
        direction: CartesianDirection<Galactic>,
    ) -> Result<(BandPhotonRadiance, Option<BandPhotonRadiance>)> {
        if let Some(spectral) = &self.spectral {
            return spectral.top_of_atmosphere_pixel(direction);
        }
        let p = self.try_pixel_at(direction)?;
        Ok((p.integrated, p.statistical_uncertainty))
    }

    /// Exact integral over complete stored bins; rejects partial bins and gaps.
    ///
    /// For unions of multiple spectral bins, sigma is unavailable without
    /// packed covariance, except when the native full-band sigma is recorded.
    pub fn integrate_band(
        &self,
        direction: CartesianDirection<Galactic>,
        low: Nanometers,
        high: Nanometers,
    ) -> Result<(BandPhotonRadiance, Option<BandPhotonRadiance>)> {
        if let Some(spectral) = &self.spectral {
            return spectral.integrate_band(direction, low, high);
        }
        if low.value() != 300.0 || high.value() != 650.0 {
            return Err(NsbError::Unsupported(
                "integrated-only starlight maps cannot reconstruct a sub-band".into(),
            ));
        }
        self.top_of_atmosphere_pixel(direction)
    }

    /// Bounds on directly transmitted native-band Starlight radiance.
    ///
    /// Only maps containing suitable measured spectral bins can use this
    /// operation; Rayleigh/Mie attenuation is not stellar in-scattering.
    pub fn direct_pixel_bounds(
        &self,
        direction: CartesianDirection<Galactic>,
        geometry: DirectPathGeometry,
        atmosphere: AtmosphericConditions,
    ) -> Result<DirectStarlightBounds> {
        self.spectral
            .as_ref()
            .ok_or_else(|| {
                NsbError::Unsupported(
                    "integrated-only Starlight maps lack wavelength-resolved transmission data"
                        .into(),
                )
            })?
            .direct_pixel_bounds(direction, geometry, atmosphere)
    }

    /// Parse Packed HEALPix CSV text and merge header provenance.
    pub fn from_csv_str(raw: &str, provenance: StarlightProvenance) -> Result<Self> {
        let metadata = parse_header_metadata(raw);
        let provenance = StarlightProvenance::from_header_metadata(&metadata, provenance);
        let data_header = first_data_header(raw)?;

        if data_header.starts_with("healpix_index,") {
            Self::from_healpix_csv_str(raw, metadata, provenance)
        } else {
            Err(NsbError::DataParse {
                file: "starlight map csv",
                message: format!("unsupported starlight map header {data_header:?}"),
            })
        }
    }

    /// Read and parse a map from a filesystem path.
    pub fn from_csv_path(path: impl AsRef<Path>, provenance: StarlightProvenance) -> Result<Self> {
        let raw = std::fs::read_to_string(path)?;
        Self::from_csv_str(&raw, provenance)
    }

    /// Whether this map contains a spectral-only TOA product.
    pub(crate) fn is_spectral(&self) -> bool {
        self.spectral.is_some()
    }

    /// Return the stored sample nearest a Galactic direction.
    ///
    /// Spectral-map samples refer to the map's native band (see [`Self::band_nm`]),
    /// never implicitly to the historical NSB 300–650 nm default.
    /// This is an advanced map-inspection operation. Normal NSB evaluation
    /// transforms [`crate::Target`] values and returns the shared
    /// [`crate::NsbComponent`] result contract through [`crate::NsbEvaluator`].
    pub fn pixel_at(&self, direction: CartesianDirection<Galactic>) -> StarlightPixel {
        self.try_pixel_at(direction)
            .expect("pixel_at requires a finite Galactic direction")
    }

    /// Fallible nearest-neighbor lookup for arbitrary caller-supplied directions.
    ///
    /// Unlike the frozen legacy pixel_at method, this never panics on an
    /// invalid direction or an out-of-range HEALPix index.
    pub fn try_pixel_at(
        &self,
        direction: CartesianDirection<Galactic>,
    ) -> Result<StarlightPixel> {
        let index = self
            .map
            .grid()
            .direction_to_pixel(direction)
            .map_err(|err| invalid_map(err.to_string()))?;
        let slot = usize::try_from(index.get())
            .map_err(|_| invalid_map("HEALPix index exceeds address space"))?;
        self.map.values()
            .get(slot)
            .copied()
            .ok_or_else(|| invalid_map("HEALPix pixel index out of range"))
    }

    pub(crate) fn lookup(&self, direction: CartesianDirection<Galactic>) -> StarlightOutputs {
        self.pixel_at(direction).output()
    }

    /// Return map provenance.
    pub fn provenance(&self) -> &StarlightProvenance {
        &self.provenance
    }

    /// Return the underlying HEALPix grid.
    pub fn grid(&self) -> HealpixGrid {
        self.map.grid()
    }

    /// Return validated pixels in HEALPix storage order.
    pub fn pixels(&self) -> &[StarlightPixel] {
        self.map.values()
    }

    /// Galactic spherical direction of a stored pixel centre.
    pub fn pixel_direction(&self, index: HealpixIndex) -> Result<SphericalDirection<Galactic>> {
        self.grid()
            .pixel_center_spherical(index)
            .map_err(|err| invalid_map(err.to_string()))
    }

    /// Equal-area solid angle of every pixel on this map.
    pub fn pixel_solid_angle(&self) -> Steradians {
        Steradians::new(self.grid().pixel_area_sr())
    }

    pub(super) fn validate_production_diagnostics(
        &self,
        input_integrated_flux_sum: Option<f64>,
        integrated_flux_tolerance: Option<f64>,
    ) -> Result<StarlightValidationDiagnostics> {
        let pixels = self.pixels();
        validate_integrated_values(pixels)?;
        let flux_conservation_recomputed = if let (Some(expected), Some(tolerance)) =
            (input_integrated_flux_sum, integrated_flux_tolerance)
        {
            validate_integrated_flux_conservation(
                self.grid(),
                pixels,
                PixelIntegratedPhotonFlux::new(expected),
                tolerance,
            )?;
            true
        } else {
            return Err(invalid_map(
                "validated Gaia XP starlight requires integrated flux-conservation inputs",
            ));
        };

        let (plane_pole_ratio, longitude_wrap_relative_jump) =
            diagnostic_values(self.grid(), pixels)?;
        if plane_pole_ratio < 1.0 {
            return Err(invalid_map(
                "integrated plane/pole validation failed: plane radiance is below pole radiance",
            ));
        }
        if longitude_wrap_relative_jump > 1.0 {
            return Err(invalid_map(format!(
                "integrated longitude-wrap validation failed: relative jump {longitude_wrap_relative_jump} exceeds 1"
            )));
        }
        Ok(StarlightValidationDiagnostics {
            pixel_count: pixels.len(),
            radiance_field: "integrated_ph_cm2_ns_sr",
            plane_pole_ratio,
            longitude_wrap_relative_jump,
            flux_conservation_recomputed,
        })
    }

    fn from_healpix_csv_str(
        raw: &str,
        metadata: BTreeMap<String, String>,
        provenance: StarlightProvenance,
    ) -> Result<Self> {
        let nside = required_metadata(&metadata, "nside")?
            .parse::<u32>()
            .map_err(|err| NsbError::DataParse {
                file: "starlight map csv",
                message: format!("invalid HEALPix nside: {err}"),
            })?;
        let ordering = match required_metadata(&metadata, "ordering")?
            .to_ascii_lowercase()
            .as_str()
        {
            "ring" => HealpixOrdering::Ring,
            "nested" => HealpixOrdering::Nested,
            other => {
                return Err(NsbError::DataParse {
                    file: "starlight map csv",
                    message: format!("unsupported HEALPix ordering {other:?}"),
                })
            }
        };
        let frame = required_metadata(&metadata, "coordinate_frame")?.to_ascii_lowercase();
        if frame != "galactic" {
            return Err(NsbError::DataParse {
                file: "starlight map csv",
                message: format!("expected coordinate_frame=galactic, got {frame:?}"),
            });
        }

        let diagnostics = metadata
            .get("s10_diagnostics")
            .map(String::as_str)
            .unwrap_or("");
        if diagnostics != "not_provided" {
            return Err(NsbError::DataParse {
                file: "starlight map csv",
                message: "packed HEALPix maps must declare # s10_diagnostics=not_provided"
                    .to_string(),
            });
        }

        let grid = HealpixGrid::new(
            Nside::new(nside).map_err(|err| invalid_map(err.to_string()))?,
            ordering,
        )
        .map_err(|err| invalid_map(err.to_string()))?;
        let npix = usize::try_from(grid.npix()).expect("HEALPix npix fits usize");
        let mut pixels = vec![None; npix];
        let mut reader = ReaderBuilder::new()
            .comment(Some(b'#'))
            .trim(csv::Trim::All)
            .from_reader(raw.as_bytes());
        let headers = reader.headers().map_err(|err| NsbError::DataParse {
            file: "starlight map csv",
            message: format!("failed to read HEALPix CSV header: {err}"),
        })?;
        validate_packed_healpix_header(headers)?;

        for (row_idx, record) in reader.records().enumerate() {
            let record = record.map_err(|err| NsbError::DataParse {
                file: "starlight map csv",
                message: format!("failed to read HEALPix CSV row {}: {err}", row_idx + 1),
            })?;
            if record.len() != 5 {
                return Err(NsbError::DataParse {
                    file: "starlight map csv",
                    message: format!(
                        "HEALPix CSV row {} has {} fields, expected 5",
                        row_idx + 1,
                        record.len()
                    ),
                });
            }
            let index = parse_record_u64(&record, 0, row_idx + 1, "healpix_index")?;
            grid.validate_index(HealpixIndex::new(index))
                .map_err(|err| invalid_map(err.to_string()))?;
            let slot = usize::try_from(index).expect("pixel index fits usize");
            let pixel = StarlightPixel::new(
                BandPhotonRadiance::new(parse_record_f64(
                    &record,
                    1,
                    row_idx + 1,
                    "integrated_ph_cm2_ns_sr",
                )?),
                S10s::new(0.0),
                S10s::new(0.0),
            )
            .without_s10_diagnostics()
            .with_uncertainties(
                BandPhotonRadiance::new(parse_record_f64(
                    &record,
                    2,
                    row_idx + 1,
                    "statistical_uncertainty_ph_cm2_ns_sr",
                )?),
                BandPhotonRadiance::new(parse_record_f64(
                    &record,
                    3,
                    row_idx + 1,
                    "systematic_uncertainty_ph_cm2_ns_sr",
                )?),
                BandPhotonRadiance::new(parse_record_f64(
                    &record,
                    4,
                    row_idx + 1,
                    "total_uncertainty_ph_cm2_ns_sr",
                )?),
            );
            pixel.validate()?;
            if pixels[slot].replace(pixel).is_some() {
                return Err(invalid_map(format!(
                    "duplicate HEALPix pixel index {index}"
                )));
            }
        }

        let mut validated = Vec::with_capacity(npix);
        for (index, pixel) in pixels.into_iter().enumerate() {
            validated.push(
                pixel.ok_or_else(|| invalid_map(format!("missing HEALPix pixel index {index}")))?,
            );
        }

        Self::from_healpix(grid, validated, provenance)
    }
}

fn first_data_header(raw: &str) -> Result<&str> {
    raw.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .ok_or_else(|| invalid_map("starlight map csv has no data header"))
}

pub(super) fn parse_header_metadata(raw: &str) -> BTreeMap<String, String> {
    raw.lines()
        .map(str::trim)
        .filter(|line| line.starts_with('#'))
        .filter_map(|line| line.trim_start_matches('#').trim().split_once('='))
        .map(|(key, value)| (key.trim().to_string(), value.trim().to_string()))
        .collect()
}

fn diagnostic_values(grid: HealpixGrid, pixels: &[StarlightPixel]) -> Result<(f64, f64)> {
    let mean = |values: &[f64]| -> Result<f64> {
        if values.is_empty() {
            return Err(invalid_map("starlight diagnostic region is empty"));
        }
        Ok(values.iter().sum::<f64>() / values.len() as f64)
    };
    let plane_lat_limit = Degrees::new(10.0);
    let pole_lat_limit = Degrees::new(60.0);
    let wrap_lat_limit = Degrees::new(30.0);
    let wrap_low_lon_limit = Degrees::new(10.0);
    let wrap_high_lon_limit = Degrees::new(350.0);
    let mut plane = Vec::new();
    let mut pole = Vec::new();
    let mut low = Vec::new();
    let mut high = Vec::new();
    for (index, pixel) in pixels.iter().enumerate() {
        let direction = grid
            .pixel_center_spherical::<Galactic>(HealpixIndex::new(index as u64))
            .map_err(|err| invalid_map(err.to_string()))?;
        let lon = direction.l();
        let lat = direction.b();
        let value = pixel.integrated.value();
        if lat.abs() <= plane_lat_limit {
            plane.push(value);
        }
        if lat.abs() >= pole_lat_limit {
            pole.push(value);
        }
        if lat.abs() <= wrap_lat_limit && lon <= wrap_low_lon_limit {
            low.push(value);
        }
        if lat.abs() <= wrap_lat_limit && lon >= wrap_high_lon_limit {
            high.push(value);
        }
    }
    let plane_mean = mean(&plane)?;
    let pole_mean = mean(&pole)?;
    let ratio = if pole_mean == 0.0 {
        if plane_mean > 0.0 {
            f64::INFINITY
        } else {
            1.0
        }
    } else {
        plane_mean / pole_mean
    };
    let low_mean = mean(&low)?;
    let high_mean = mean(&high)?;
    let jump = (low_mean - high_mean).abs() / low_mean.abs().max(high_mean.abs()).max(1.0);
    Ok((ratio, jump))
}

fn validate_integrated_values(pixels: &[StarlightPixel]) -> Result<()> {
    for pixel in pixels {
        let value = pixel.integrated.value();
        if !value.is_finite() || value < 0.0 {
            return Err(invalid_map(
                "integrated radiance values must be finite and non-negative",
            ));
        }
    }
    Ok(())
}

fn validate_integrated_flux_conservation(
    grid: HealpixGrid,
    pixels: &[StarlightPixel],
    expected_flux: PixelIntegratedPhotonFlux,
    tolerance: f64,
) -> Result<()> {
    if !expected_flux.is_finite() || expected_flux < PixelIntegratedPhotonFlux::new(0.0) {
        return Err(invalid_map(
            "input_integrated_flux_sum must be finite and non-negative",
        ));
    }
    if !tolerance.is_finite() || tolerance < 0.0 {
        return Err(invalid_map(
            "integrated_flux_conservation_tolerance must be finite and non-negative",
        ));
    }
    let solid_angle = Steradians::new(grid.pixel_area_sr());
    let actual_flux: PixelIntegratedPhotonFlux = pixels
        .iter()
        .map(|pixel| pixel.integrated * solid_angle)
        .sum();
    let scale = expected_flux
        .abs()
        .max(actual_flux.abs())
        .max(PixelIntegratedPhotonFlux::new(1.0));
    let relative_error = (actual_flux - expected_flux).abs().value() / scale.value();
    if relative_error > tolerance {
        return Err(invalid_map(format!(
            "integrated flux-conservation validation failed: expected {}, actual {}, relative error {relative_error}, tolerance {tolerance}",
            expected_flux.value(),
            actual_flux.value()
        )));
    }
    Ok(())
}

fn required_metadata<'a>(metadata: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str> {
    metadata
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| invalid_map(format!("missing required HEALPix metadata key {key:?}")))
}

fn validate_packed_healpix_header(headers: &StringRecord) -> Result<()> {
    const PACKED: [&str; 5] = [
        "healpix_index",
        "integrated_ph_cm2_ns_sr",
        "statistical_uncertainty_ph_cm2_ns_sr",
        "systematic_uncertainty_ph_cm2_ns_sr",
        "total_uncertainty_ph_cm2_ns_sr",
    ];
    let matches = headers.len() == PACKED.len()
        && headers
            .iter()
            .zip(PACKED)
            .all(|(actual, expected)| actual.trim() == expected);
    if matches {
        Ok(())
    } else {
        Err(NsbError::DataParse {
            file: "starlight map csv",
            message: format!(
                "unsupported HEALPix starlight map header {:?}; expected packed {}",
                headers.iter().collect::<Vec<_>>(),
                PACKED.join(",")
            ),
        })
    }
}

fn record_field<'a>(
    record: &'a StringRecord,
    idx: usize,
    row: usize,
    name: &str,
) -> Result<&'a str> {
    record.get(idx).ok_or_else(|| NsbError::DataParse {
        file: "starlight map csv",
        message: format!("HEALPix CSV row {row} is missing field {name}"),
    })
}

fn parse_record_u64(record: &StringRecord, idx: usize, row: usize, name: &str) -> Result<u64> {
    record_field(record, idx, row, name)?
        .parse::<u64>()
        .map_err(|err| NsbError::DataParse {
            file: "starlight map csv",
            message: format!("HEALPix CSV row {row} invalid {name}: {err}"),
        })
}

fn parse_record_f64(record: &StringRecord, idx: usize, row: usize, name: &str) -> Result<f64> {
    record_field(record, idx, row, name)?
        .parse::<f64>()
        .map_err(|err| NsbError::DataParse {
            file: "starlight map csv",
            message: format!("HEALPix CSV row {row} invalid {name}: {err}"),
        })
}

fn invalid_map(message: impl Into<String>) -> NsbError {
    NsbError::InvalidMap {
        message: message.into(),
    }
}
