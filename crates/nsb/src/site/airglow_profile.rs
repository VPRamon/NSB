// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Portable, registered one-parameter continuum fits. Evidence is not admission.
//!
//! Only independently isolated continuum measurements are accepted. A total-sky
//! residual cannot identify continuum normalization separately from omitted lines.
use super::calibration::SiteCalibrationAsset;
use super::AtmosphericConditions;
use crate::{NsbError, Result, SolarFluxUnits, Target};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use siderust::checksum::{sha256, to_hex};
use siderust::coordinates::{centers::Geodetic, frames::ECEF};
use siderust::qtty::{Degrees, Hectopascals, Kilometers, Meters, Nanometers, OpticalDepths};
use std::collections::{BTreeMap, BTreeSet};
use tempoch::{Time, UTC};

/// Declared physical assumptions, using NSB's existing site-evidence contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AirglowCalibrationDefinition {
    /// Version of this portable fit schema, currently 1.
    pub schema_version: u32,
    /// Must be `paranal-palace-v1`; fitted scale applies only to continuum.
    pub base_model: String,
    /// Exact bundled continuum checksum.
    pub base_asset_sha256: String,
    /// Site identity, validity, atmosphere, references, fitted scale and sigma.
    pub(crate) evidence: SiteCalibrationAsset,
    /// East longitude in degrees.
    pub longitude_deg: f64,
    /// Geodetic latitude in degrees.
    pub latitude_deg: f64,
    /// Must be `WGS84-geodetic-ellipsoid-m`; altitude is evidence.atmosphere altitude.
    pub coordinate_convention: String,
    /// Explicit F10.7 applicability in sfu (not a fitted response).
    pub solar_flux_range_sfu: [f64; 2],
    /// Maximum zenith distance for applying this fit.
    pub max_zenith_deg: f64,
    /// Methods and provenance for isolating continuum and propagating subtraction errors.
    pub measurement_provenance: String,
    /// Exact calibration software identity, including source commit when applicable.
    pub software_provenance: String,
}

/// An independent continuum measurement, with its physical query and uncertainty.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AirglowCalibrationObservation {
    /// Unique observation identity matching the checksum-pinned references.
    pub id: String,
    /// RFC3339 UTC timestamp, with explicit zero offset.
    pub utc: String,
    /// Right ascension, degrees J2000.
    pub ra_deg: f64,
    /// Declination, degrees J2000.
    pub dec_deg: f64,
    /// Top-hat vacuum wavelength interval in nm; no instrumental response assumed.
    pub wavelength_nm: [f64; 2],
    /// F10.7, sfu; interpretation and provenance must be registered with measurements.
    pub solar_flux_sfu: f64,
    /// Independently isolated continuum, photons cm^-2 ns^-1 sr^-1.
    pub continuum_radiance: f64,
    /// Positive one-sigma uncertainty including background/line subtraction.
    pub uncertainty: f64,
}

/// A complete fit request; every observation is training data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AirglowCalibrationRequest {
    /// Fixed physical and provenance assumptions.
    pub definition: AirglowCalibrationDefinition,
    /// Registered training observations, in deterministic input order.
    pub observations: Vec<AirglowCalibrationObservation>,
    /// Reserved evaluation night IDs (local mean solar date after subtracting 12 h).
    /// Reservation documents isolation, not independent scientific validation.
    pub reserved_nights: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Artifact {
    request: AirglowCalibrationRequest,
    method: String,
    parameter_unit: String,
    parameter_bounds: [f64; 2],
    lower_bound_exclusive: bool,
    fitter_software: String,
    maturity: String,
    scale: f64,
    variance: f64,
    chi_square: f64,
    degrees_of_freedom: usize,
}

/// Validated immutable artifact. Loading verifies the registered fit numerically;
/// neither a file field nor a profile name can confer trusted calibrated status.
#[derive(Debug, Clone, PartialEq)]
pub struct AirglowCalibrationProfile {
    artifact: Artifact,
    checksum: String,
}

fn invalid(message: impl Into<String>) -> NsbError {
    NsbError::OutOfRange(message.into())
}
fn finite_range(x: f64, lo: f64, hi: f64) -> bool {
    x.is_finite() && (lo..=hi).contains(&x)
}

impl AirglowCalibrationDefinition {
    /// Validate fixed assumptions and the private, existing evidence contract.
    pub fn validate(&self) -> Result<()> {
        self.evidence
            .validate()
            .map_err(|e| invalid(e.to_string()))?;
        let asset = crate::components::airglow::calibration::airglow_continuum_asset();
        if self.schema_version != 1
            || self.base_model != "paranal-palace-v1"
            || self.base_asset_sha256 != asset.sha256
            || self.coordinate_convention != "WGS84-geodetic-ellipsoid-m"
            || !finite_range(self.longitude_deg, -180., 180.)
            || !finite_range(self.latitude_deg, -90., 90.)
            || !finite_range(self.max_zenith_deg, 0., 60.)
            || !finite_range(self.solar_flux_range_sfu[0], 1., 1000.)
            || !finite_range(
                self.solar_flux_range_sfu[1],
                self.solar_flux_range_sfu[0],
                1000.,
            )
            || self.measurement_provenance.trim().is_empty()
            || self.software_provenance.trim().is_empty()
            || self.evidence.validity.wavelength_nm[0] < 300
            || self.evidence.validity.wavelength_nm[1] > 650
            || self.evidence.airglow.temporal_correction_applied
        {
            return Err(invalid(
                "incompatible continuum calibration schema, asset, domain or assumptions",
            ));
        }
        Ok(())
    }
    /// Registered calibration identity; a presentation name cannot confer admission.
    pub fn calibration_id(&self) -> &str {
        &self.evidence.calibration_id
    }
    /// Stable observatory identity from the evidence package.
    pub fn site_id(&self) -> &str {
        self.evidence.site.as_str()
    }
    /// Registered reference (identity, normalized relative path, SHA-256).
    /// Read-only projection keeps the legacy evidence schema private.
    pub fn reference_checksums(&self) -> impl Iterator<Item = (&str, &str, &str)> {
        self.evidence
            .references
            .iter()
            .map(|r| (r.id.as_str(), r.path.as_str(), r.sha256.as_str()))
    }
    fn observer(&self) -> Geodetic<ECEF> {
        Geodetic::new_raw(
            Degrees::new(self.longitude_deg),
            Degrees::new(self.latitude_deg),
            Meters::new(self.evidence.atmosphere.representative_altitude_m),
        )
    }
    pub(crate) fn atmosphere(&self) -> Result<AtmosphericConditions> {
        let a = &self.evidence.atmosphere;
        let mut atmosphere = AtmosphericConditions::clear_sky_with_pressure(Hectopascals::new(
            a.surface_pressure_hpa,
        ))?;
        atmosphere.rayleigh_scale_height = Kilometers::new(a.rayleigh_scale_height_km);
        atmosphere.mie_params.tau0 = OpticalDepths::new(a.aerosol_optical_depth_550_nm);
        atmosphere.mie_params.alpha = a.angstrom_exponent;
        atmosphere.mie_params.lambda_ref = Nanometers::new(550.);
        atmosphere.validate()?;
        Ok(atmosphere)
    }
}

impl AirglowCalibrationProfile {
    /// Fit exactly one scale with weighted least squares through the origin.
    /// Physics stays fixed. Requires >=3 measurements on >=2 distinct nights.
    /// Variance is the larger of known-error inverse information and the
    /// night-cluster sandwich variance; it does not include model discrepancy.
    pub fn fit(mut request: AirglowCalibrationRequest) -> Result<Self> {
        request.definition.validate()?;
        if request.observations.len() < 3 {
            return Err(invalid("fit requires >=3 observations"));
        }
        let d = &request.definition;
        let continuum =
            std::sync::Arc::new(crate::components::airglow::calibration::load_builtin_standard()?);
        let atmosphere = d.atmosphere()?;
        let reserved: BTreeSet<_> = request.reserved_nights.iter().collect();
        if reserved.len() != request.reserved_nights.len()
            || reserved
                .iter()
                .any(|n| chrono::NaiveDate::parse_from_str(n, "%Y-%m-%d").is_err())
        {
            return Err(invalid("invalid/duplicate reserved nights"));
        }
        let mut ids = BTreeSet::new();
        let mut pairs = Vec::new();
        let mut information = 0.;
        let mut numerator = 0.;
        for o in &request.observations {
            if o.id.trim().is_empty()
                || !ids.insert(&o.id)
                || !finite_range(o.ra_deg, 0., 360.)
                || !finite_range(o.dec_deg, -90., 90.)
                || !o.continuum_radiance.is_finite()
                || o.continuum_radiance <= 0.
                || !o.uncertainty.is_finite()
                || o.uncertainty <= 0.
            {
                return Err(invalid(
                    "invalid/duplicate continuum observation or uncertainty",
                ));
            }
            let dt = DateTime::parse_from_rfc3339(&o.utc).map_err(|e| invalid(e.to_string()))?;
            if dt.offset().local_minus_utc() != 0 {
                return Err(invalid("observation timestamp must be UTC"));
            }
            let night = (dt + Duration::seconds((d.longitude_deg / 15. * 3600.).round() as i64)
                - Duration::hours(12))
            .date_naive()
            .to_string();
            if reserved.iter().any(|n| n.as_str() == night) {
                return Err(invalid("training/evaluation night overlap"));
            }
            let time = Time::<UTC>::from_chrono(dt.with_timezone(&Utc));
            let target = Target::new(Degrees::new(o.ra_deg), Degrees::new(o.dec_deg));
            let altitude = crate::components::airglow::target_altitude(time, d.observer(), target);
            validate_query(
                d,
                d.observer(),
                time,
                altitude.value(),
                o.solar_flux_sfu,
                o.wavelength_nm,
            )?;
            let model = crate::components::airglow::Airglow::with_shared_continuum(
                d.observer(),
                std::sync::Arc::clone(&continuum),
            )
            .with_atmosphere(atmosphere)
            .with_solar_radio_flux(SolarFluxUnits::new(o.solar_flux_sfu));
            let x = model
                .compute_band(time, target, o.wavelength_nm[0], o.wavelength_nm[1])?
                .value();
            if !x.is_finite() || x <= 0. {
                return Err(invalid(
                    "continuum parameter is not identifiable: zero predictor",
                ));
            }
            let a = x / o.uncertainty;
            let b = o.continuum_radiance / o.uncertainty;
            information += a * a;
            numerator += a * b;
            pairs.push((night, a, b));
        }
        let scale = numerator / information;
        if !information.is_finite()
            || information <= 0.
            || !finite_range(scale, f64::MIN_POSITIVE, 100.)
        {
            return Err(invalid("nonidentifiable or out-of-bounds continuum fit"));
        }
        let mut scores = BTreeMap::<String, f64>::new();
        let mut chi_square = 0.;
        for (night, a, b) in pairs {
            let residual = b - scale * a;
            chi_square += residual * residual;
            *scores.entry(night).or_default() += a * residual;
        }
        let k = scores.len();
        if k < 2 {
            return Err(invalid("fit requires >=2 distinct training nights"));
        }
        let cluster = k as f64 / (k - 1) as f64
            * scores
                .values()
                .map(|s| (s / information).powi(2))
                .sum::<f64>();
        let variance = cluster.max(1. / information);
        if !variance.is_finite() || variance <= 0. || !chi_square.is_finite() {
            return Err(invalid("nonfinite fit covariance"));
        }
        request.definition.evidence.airglow.continuum_scale = scale;
        request
            .definition
            .evidence
            .airglow
            .continuum_scale_uncertainty = variance.sqrt();
        request.definition.validate()?;
        let degrees_of_freedom = request.observations.len() - 1;
        let artifact = Artifact {
            request,
            method: "weighted-origin-scale-night-sandwich-v1".into(),
            parameter_unit: "dimensionless".into(),
            parameter_bounds: [0., 100.],
            lower_bound_exclusive: true,
            fitter_software: concat!("nsb-", env!("CARGO_PKG_VERSION")).into(),
            maturity: "registered-fit-not-independently-validated".into(),
            scale,
            variance,
            chi_square,
            degrees_of_freedom,
        };
        let canonical = toml::to_string(&artifact).map_err(|e| invalid(e.to_string()))?;
        let checksum = to_hex(&sha256(canonical.as_bytes()));
        Ok(Self { artifact, checksum })
    }
    /// Strict portable TOML loader; replay the fit and reject edited fit statistics.
    pub fn from_toml_str(raw: &str) -> Result<Self> {
        let artifact: Artifact = toml::from_str(raw).map_err(|e| invalid(e.to_string()))?;
        if !finite_range(artifact.scale, f64::MIN_POSITIVE, 100.)
            || !artifact.variance.is_finite()
            || artifact.variance <= 0.
            || !artifact.chi_square.is_finite()
            || artifact.chi_square < 0.
        {
            return Err(invalid("nonphysical calibration artifact statistics"));
        }
        let mut replay = Self::fit(artifact.request.clone())?.artifact;
        // Platform libm can differ by a few ULPs. Check numerical replay at
        // explicit roundoff tolerance, but retain the portable artifact's
        // original numbers so its canonical bytes/hash do not change on load.
        let close = |a: f64, b: f64, absolute: f64| {
            a.is_finite()
                && b.is_finite()
                && (a - b).abs() <= absolute + 1.0e-12 * a.abs().max(b.abs())
        };
        let supplied = &artifact.request.definition.evidence.airglow;
        let calculated = &replay.request.definition.evidence.airglow;
        if !close(artifact.scale, replay.scale, 1.0e-24)
            || !close(artifact.variance, replay.variance, 1.0e-24)
            || !close(artifact.chi_square, replay.chi_square, 1.0e-20)
            || !close(
                supplied.continuum_scale,
                calculated.continuum_scale,
                1.0e-24,
            )
            || !close(
                supplied.continuum_scale_uncertainty,
                calculated.continuum_scale_uncertainty,
                1.0e-24,
            )
        {
            return Err(invalid(
                "calibration artifact does not match reproducible registered fit",
            ));
        }
        replay.scale = artifact.scale;
        replay.variance = artifact.variance;
        replay.chi_square = artifact.chi_square;
        replay.request.definition.evidence.airglow.continuum_scale = supplied.continuum_scale;
        replay
            .request
            .definition
            .evidence
            .airglow
            .continuum_scale_uncertainty = supplied.continuum_scale_uncertainty;
        if replay != artifact {
            return Err(invalid(
                "calibration artifact does not match reproducible registered fit",
            ));
        }
        let canonical = toml::to_string(&artifact).map_err(|e| invalid(e.to_string()))?;
        Ok(Self {
            artifact,
            checksum: to_hex(&sha256(canonical.as_bytes())),
        })
    }

    /// Deterministic serialization, with no wall-clock fields or self checksum.
    pub fn to_toml_string(&self) -> Result<String> {
        toml::to_string(&self.artifact).map_err(|e| invalid(e.to_string()))
    }
    /// SHA-256 of canonical portable artifact bytes.
    pub fn checksum_sha256(&self) -> &str {
        &self.checksum
    }
    /// Inspect all fixed assumptions, provenance and training records.
    pub fn request(&self) -> &AirglowCalibrationRequest {
        &self.artifact.request
    }
    /// Dimensionless fitted continuum normalization.
    pub fn scale(&self) -> crate::units::ScaleFactors {
        crate::units::ScaleFactors::new(self.artifact.scale)
    }
    /// One-parameter covariance (dimensionless squared).
    pub fn variance(&self) -> f64 {
        self.artifact.variance
    }
    pub(crate) fn validate_query(
        &self,
        observer: Geodetic<ECEF>,
        time: Time<UTC>,
        altitude: f64,
        solar: f64,
        band: [f64; 2],
    ) -> Result<()> {
        validate_query(
            &self.artifact.request.definition,
            observer,
            time,
            altitude,
            solar,
            band,
        )
    }
}

fn validate_query(
    d: &AirglowCalibrationDefinition,
    observer: Geodetic<ECEF>,
    time: Time<UTC>,
    altitude: f64,
    solar: f64,
    band: [f64; 2],
) -> Result<()> {
    let expected = d.observer();
    let date = time
        .to_chrono()
        .ok_or_else(|| invalid("unrepresentable calibration date"))?
        .date_naive();
    let v = &d.evidence.validity;
    let from = chrono::NaiveDate::parse_from_str(&v.valid_from, "%Y-%m-%d")
        .map_err(|e| invalid(e.to_string()))?;
    let through = chrono::NaiveDate::parse_from_str(&v.valid_through, "%Y-%m-%d")
        .map_err(|e| invalid(e.to_string()))?;
    if (observer.lon.value() - expected.lon.value()).abs() > 1.0e-6
        || (observer.lat.value() - expected.lat.value()).abs() > 1.0e-6
        || (observer.height.value() - expected.height.value()).abs() > 1.
        || date < from
        || date > through
        || !finite_range(altitude, 90. - d.max_zenith_deg, 90.)
        || !finite_range(solar, d.solar_flux_range_sfu[0], d.solar_flux_range_sfu[1])
        || !finite_range(
            band[0],
            v.wavelength_nm[0] as f64,
            v.wavelength_nm[1] as f64,
        )
        || !finite_range(band[1], band[0], v.wavelength_nm[1] as f64)
        || band[0] >= band[1]
    {
        return Err(invalid(
            "query outside registered Airglow calibration site/time/wavelength/solar/zenith domain",
        ));
    }
    Ok(())
}
