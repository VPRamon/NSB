//! Validated vertical-emission profile domain model.
//!
//! Owns profile invariants, accessors, normalization, and checksum identity.
//! TOML persistence lives in [`super::vertical_profile_io`]; spherical LOS
//! integration lives in [`super::integration`].

use super::integration::{integrate_profile_los, VERTICAL_PROFILE_REFERENCE_SUBSTEPS};
use crate::error::{NsbError, Result};
use crate::units::ScaleFactors;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::qtty::{
    unit::{Kilometer, Nanometer},
    Degrees, Kilometers, Nanometers,
};
use std::sync::Arc;
use thiserror::Error;

/// Current schema accepted for persisted vertical-emission profiles.
pub const VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION: u32 = 1;

const NSB_WAVELENGTH_MIN_NM: f64 = 300.0;
const NSB_WAVELENGTH_MAX_NM: f64 = 650.0;
const MIN_PROFILE_SAMPLES: usize = 3;

/// Validation failure for a caller-provided or persisted vertical profile.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum VerticalEmissionProfileError {
    /// A field or sample violates the profile contract.
    #[error("invalid vertical-emission profile: {0}")]
    Invalid(String),
    /// TOML could not be decoded or encoded deterministically.
    #[error("vertical-emission profile TOML error: {0}")]
    Toml(String),
    /// Persisted bytes claim a checksum other than their canonical identity.
    #[error(
        "vertical-emission profile checksum mismatch: expected {expected}, computed {computed}"
    )]
    ChecksumMismatch {
        /// Checksum stored in the persisted profile.
        expected: String,
        /// Checksum computed from canonical validated fields.
        computed: String,
    },
}

/// Supported normalization convention for relative vertical emissivity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "kebab-case")]
pub enum VerticalProfileNormalization {
    /// Emissivity is rescaled so its trapezoidal vertical integral is one.
    UnitVerticalIntegral,
}

impl VerticalProfileNormalization {
    /// Stable machine-readable identifier.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnitVerticalIntegral => "unit-vertical-integral",
        }
    }
}

/// Optical wavelength/band domain represented by one vertical profile.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct AirglowWavelengthApplicability {
    /// Inclusive lower wavelength bound.
    pub min: Nanometers,
    /// Inclusive upper wavelength bound.
    pub max: Nanometers,
    /// Stable human-readable band or process identifier.
    pub band: String,
}

impl AirglowWavelengthApplicability {
    /// Define the wavelength applicability declared by a vertical-emission profile.
    pub fn new(min: Nanometers, max: Nanometers, band: impl Into<String>) -> Self {
        Self {
            min,
            max,
            band: band.into(),
        }
    }
}

/// Zenith-angle interval for which a profile is declared usable.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct ValidatedZenithDomain {
    /// Inclusive minimum zenith angle.
    pub min: Degrees,
    /// Inclusive maximum zenith angle.
    pub max: Degrees,
}

impl ValidatedZenithDomain {
    /// Define the declared zenith-angle applicability of a vertical profile.
    pub const fn new(min: Degrees, max: Degrees) -> Self {
        Self { min, max }
    }
}

/// Complete programmatic definition of a vertical-emission profile.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct VerticalEmissionProfileDefinition {
    /// Profile schema version.
    pub schema_version: u32,
    /// Stable profile identifier.
    pub profile_id: String,
    /// Strictly increasing altitude grid above mean spherical sea level.
    pub altitude_km: Vec<Kilometers>,
    /// Non-negative relative volume emissivity at each altitude.
    pub relative_emissivity: Vec<f64>,
    /// Normalization convention.
    pub normalization: VerticalProfileNormalization,
    /// Wavelength/band applicability.
    pub wavelength: AirglowWavelengthApplicability,
    /// Reference state and scientific assumptions.
    pub assumptions: String,
    /// Dataset/model/source provenance.
    pub provenance: String,
    /// License or caller-owned-data statement.
    pub license: String,
    /// Declared zenith-angle domain.
    pub validated_zenith: ValidatedZenithDomain,
}

impl VerticalEmissionProfileDefinition {
    /// Build a definition using the current schema and normalization convention.
    ///
    /// Scientific context fields remain explicit builders so future schema
    /// additions do not force a breaking constructor signature.
    pub fn new(
        profile_id: impl Into<String>,
        altitude_km: Vec<Kilometers>,
        relative_emissivity: Vec<f64>,
        wavelength: AirglowWavelengthApplicability,
        validated_zenith: ValidatedZenithDomain,
    ) -> Self {
        Self {
            schema_version: VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION,
            profile_id: profile_id.into(),
            altitude_km,
            relative_emissivity,
            normalization: VerticalProfileNormalization::UnitVerticalIntegral,
            wavelength,
            assumptions: String::new(),
            provenance: String::new(),
            license: String::new(),
            validated_zenith,
        }
    }

    /// Attach the scientific assumptions/reference state.
    pub fn with_assumptions(mut self, assumptions: impl Into<String>) -> Self {
        self.assumptions = assumptions.into();
        self
    }

    /// Attach the source/dataset provenance.
    pub fn with_provenance(mut self, provenance: impl Into<String>) -> Self {
        self.provenance = provenance.into();
        self
    }

    /// Attach the profile licence or caller-owned-data statement.
    pub fn with_license(mut self, license: impl Into<String>) -> Self {
        self.license = license.into();
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
struct VerticalEmissionProfileData {
    definition: VerticalEmissionProfileDefinition,
    checksum_sha256: String,
}

/// Validated, checksum-identified vertical airglow emissivity profile.
///
/// Construction normalizes the supplied relative emissivities to a unit
/// vertical trapezoidal integral. Clones are inexpensive and share immutable
/// profile storage.
#[derive(Debug, Clone, PartialEq)]
pub struct VerticalEmissionProfile(Arc<VerticalEmissionProfileData>);

impl VerticalEmissionProfile {
    /// Validate and checksum a programmatic profile definition.
    pub fn new(
        mut definition: VerticalEmissionProfileDefinition,
    ) -> std::result::Result<Self, VerticalEmissionProfileError> {
        validate_definition(&definition)?;
        normalize_emissivity(&mut definition);
        let checksum_sha256 = canonical_checksum(&definition);
        Ok(Self(Arc::new(VerticalEmissionProfileData {
            definition,
            checksum_sha256,
        })))
    }

    /// Stable profile identifier.
    pub fn profile_id(&self) -> &str {
        &self.0.definition.profile_id
    }

    /// Schema version.
    pub fn schema_version(&self) -> u32 {
        self.0.definition.schema_version
    }

    /// Strictly increasing altitude grid.
    pub fn altitude_km(&self) -> &[Kilometers] {
        &self.0.definition.altitude_km
    }

    /// Unit-vertical-integral emissivity samples.
    pub fn relative_emissivity(&self) -> &[f64] {
        &self.0.definition.relative_emissivity
    }

    /// Normalization convention.
    pub fn normalization(&self) -> VerticalProfileNormalization {
        self.0.definition.normalization
    }

    /// Wavelength/band applicability.
    pub fn wavelength_applicability(&self) -> &AirglowWavelengthApplicability {
        &self.0.definition.wavelength
    }

    /// Reference-state assumptions.
    pub fn assumptions(&self) -> &str {
        &self.0.definition.assumptions
    }

    /// Source provenance.
    pub fn provenance(&self) -> &str {
        &self.0.definition.provenance
    }

    /// License or caller-owned-data statement.
    pub fn license(&self) -> &str {
        &self.0.definition.license
    }

    /// Validated zenith-angle domain.
    pub fn validated_zenith_domain(&self) -> ValidatedZenithDomain {
        self.0.definition.validated_zenith
    }

    /// Deterministic SHA-256 identity of the canonical normalized profile.
    pub fn checksum_sha256(&self) -> &str {
        &self.0.checksum_sha256
    }

    /// Evaluate the auditable reference spherical LOS integral.
    pub(crate) fn geometry_factor(
        &self,
        observer: Geodetic<ECEF>,
        zenith: Degrees,
    ) -> Result<ScaleFactors> {
        self.geometry_factor_with_substeps(observer, zenith, VERTICAL_PROFILE_REFERENCE_SUBSTEPS)
    }

    /// Evaluate the same reference integrator at an explicit even resolution.
    ///
    /// Internal convergence tests use this path; production evaluation uses
    /// `VERTICAL_PROFILE_REFERENCE_SUBSTEPS`.
    pub(crate) fn geometry_factor_with_substeps(
        &self,
        observer: Geodetic<ECEF>,
        zenith: Degrees,
        substeps_per_interval: usize,
    ) -> Result<ScaleFactors> {
        if substeps_per_interval < 2 || !substeps_per_interval.is_multiple_of(2) {
            return Err(NsbError::OutOfRange(
                "vertical-profile Simpson substeps must be an even integer >= 2".into(),
            ));
        }
        let z = zenith.value();
        let domain = self.validated_zenith_domain();
        if !z.is_finite() || z < domain.min.value() || z > domain.max.value() {
            return Err(NsbError::Unsupported(format!(
                "vertical profile {} supports zenith angles [{}, {}] deg, got {} deg",
                self.profile_id(),
                domain.min.value(),
                domain.max.value(),
                z
            )));
        }
        let observer_height_km = observer.height.to::<Kilometer>().value();
        if !observer_height_km.is_finite() {
            return Err(NsbError::OutOfRange(
                "observer altitude must be finite for vertical-profile geometry".into(),
            ));
        }
        let top = self
            .altitude_km()
            .last()
            .expect("validated profile has samples")
            .value();
        if observer_height_km >= top {
            return Err(NsbError::Unsupported(format!(
                "observer altitude {observer_height_km} km is at or above profile top {top} km"
            )));
        }
        let vertical = integrate_profile_los(
            self.altitude_km(),
            self.relative_emissivity(),
            observer_height_km,
            0.0,
            substeps_per_interval,
        );
        if !vertical.is_finite() || vertical <= 0.0 {
            return Err(NsbError::Unsupported(format!(
                "vertical profile {} contains no visible emission above observer altitude {observer_height_km} km; its vertical normalization is invalid",
                self.profile_id()
            )));
        }
        if z.abs() <= f64::EPSILON {
            return Ok(ScaleFactors::new(1.0));
        }

        let los = integrate_profile_los(
            self.altitude_km(),
            self.relative_emissivity(),
            observer_height_km,
            z.to_radians(),
            substeps_per_interval,
        );
        let factor = los / vertical;
        if !los.is_finite() || !factor.is_finite() || factor <= 0.0 {
            return Err(NsbError::Unsupported(format!(
                "vertical profile {} produced invalid LOS normalization",
                self.profile_id()
            )));
        }
        Ok(ScaleFactors::new(factor))
    }
}

fn validate_definition(
    definition: &VerticalEmissionProfileDefinition,
) -> std::result::Result<(), VerticalEmissionProfileError> {
    if definition.schema_version != VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION {
        return Err(VerticalEmissionProfileError::Invalid(format!(
            "unsupported schema_version {}; expected {}",
            definition.schema_version, VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION
        )));
    }
    require_text("profile_id", &definition.profile_id)?;
    require_text("assumptions", &definition.assumptions)?;
    require_text("provenance", &definition.provenance)?;
    require_text("license", &definition.license)?;
    require_text("wavelength.band", &definition.wavelength.band)?;
    if definition.altitude_km.len() < MIN_PROFILE_SAMPLES {
        return Err(VerticalEmissionProfileError::Invalid(format!(
            "at least {MIN_PROFILE_SAMPLES} altitude/emissivity samples are required"
        )));
    }
    if definition.altitude_km.len() != definition.relative_emissivity.len() {
        return Err(VerticalEmissionProfileError::Invalid(
            "altitude and emissivity arrays must have equal length".into(),
        ));
    }
    let mut previous = None;
    for (index, altitude) in definition.altitude_km.iter().enumerate() {
        let value = altitude.value();
        if !value.is_finite() || value < 0.0 {
            return Err(VerticalEmissionProfileError::Invalid(format!(
                "altitude_km[{index}] must be finite and non-negative"
            )));
        }
        if previous.is_some_and(|prior| value <= prior) {
            return Err(VerticalEmissionProfileError::Invalid(
                "altitude bins must be strictly increasing (no duplicates)".into(),
            ));
        }
        previous = Some(value);
    }
    for (index, emissivity) in definition.relative_emissivity.iter().enumerate() {
        if !emissivity.is_finite() || *emissivity < 0.0 {
            return Err(VerticalEmissionProfileError::Invalid(format!(
                "relative_emissivity[{index}] must be finite and non-negative"
            )));
        }
    }
    let total = trapezoidal_total(definition);
    if !total.is_finite() || total <= 0.0 {
        return Err(VerticalEmissionProfileError::Invalid(
            "profile total emission must be finite and positive".into(),
        ));
    }
    let wavelength_min = definition.wavelength.min.to::<Nanometer>().value();
    let wavelength_max = definition.wavelength.max.to::<Nanometer>().value();
    if !wavelength_min.is_finite()
        || !wavelength_max.is_finite()
        || wavelength_min <= 0.0
        || wavelength_max <= wavelength_min
    {
        return Err(VerticalEmissionProfileError::Invalid(
            "wavelength bounds must be finite, positive, and increasing".into(),
        ));
    }
    if wavelength_min > NSB_WAVELENGTH_MIN_NM || wavelength_max < NSB_WAVELENGTH_MAX_NM {
        return Err(VerticalEmissionProfileError::Invalid(format!(
            "current broadband Airglow evaluation requires applicability covering {NSB_WAVELENGTH_MIN_NM}-{NSB_WAVELENGTH_MAX_NM} nm"
        )));
    }
    let domain = definition.validated_zenith;
    if !domain.min.is_finite()
        || !domain.max.is_finite()
        || domain.min.value() != 0.0
        || domain.max.value() <= 0.0
        || domain.max.value() > 90.0
    {
        return Err(VerticalEmissionProfileError::Invalid(
            "validated zenith domain must start at 0 deg and end in (0, 90] deg".into(),
        ));
    }
    Ok(())
}

fn require_text(field: &str, value: &str) -> std::result::Result<(), VerticalEmissionProfileError> {
    if value.trim().is_empty() {
        Err(VerticalEmissionProfileError::Invalid(format!(
            "{field} must not be empty"
        )))
    } else {
        Ok(())
    }
}

fn trapezoidal_total(definition: &VerticalEmissionProfileDefinition) -> f64 {
    definition
        .altitude_km
        .windows(2)
        .zip(definition.relative_emissivity.windows(2))
        .map(|(altitude, emissivity)| {
            let width = altitude[1].value() - altitude[0].value();
            width * (emissivity[0] + emissivity[1]) * 0.5
        })
        .sum()
}

fn normalize_emissivity(definition: &mut VerticalEmissionProfileDefinition) {
    match definition.normalization {
        VerticalProfileNormalization::UnitVerticalIntegral => {
            let total = trapezoidal_total(definition);
            if (total - 1.0).abs() <= 1.0e-12 {
                return;
            }
            for emissivity in &mut definition.relative_emissivity {
                *emissivity /= total;
            }
        }
    }
}

fn canonical_checksum(definition: &VerticalEmissionProfileDefinition) -> String {
    let mut digest = Sha256::new();
    digest.update(b"nsb-vertical-emission-profile-canonical-v1\0");
    digest.update(definition.schema_version.to_be_bytes());
    update_text(&mut digest, &definition.profile_id);
    update_text(&mut digest, definition.normalization.as_str());
    update_text(&mut digest, &definition.wavelength.band);
    update_f64(&mut digest, definition.wavelength.min.value());
    update_f64(&mut digest, definition.wavelength.max.value());
    update_text(&mut digest, &definition.assumptions);
    update_text(&mut digest, &definition.provenance);
    update_text(&mut digest, &definition.license);
    update_f64(&mut digest, definition.validated_zenith.min.value());
    update_f64(&mut digest, definition.validated_zenith.max.value());
    digest.update((definition.altitude_km.len() as u64).to_be_bytes());
    for altitude in &definition.altitude_km {
        update_f64(&mut digest, altitude.value());
    }
    for emissivity in &definition.relative_emissivity {
        update_f64(&mut digest, *emissivity);
    }
    let bytes = digest.finalize();
    let hex = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("sha256:{hex}")
}

fn update_text(digest: &mut Sha256, value: &str) {
    digest.update((value.len() as u64).to_be_bytes());
    digest.update(value.as_bytes());
}

fn update_f64(digest: &mut Sha256, value: f64) {
    digest.update(value.to_bits().to_be_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_definition() -> VerticalEmissionProfileDefinition {
        VerticalEmissionProfileDefinition {
            schema_version: VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION,
            profile_id: "validation-profile".into(),
            altitude_km: vec![
                Kilometers::new(80.0),
                Kilometers::new(90.0),
                Kilometers::new(100.0),
            ],
            relative_emissivity: vec![0.0, 1.0, 0.0],
            normalization: VerticalProfileNormalization::UnitVerticalIntegral,
            wavelength: AirglowWavelengthApplicability {
                min: Nanometers::new(300.0),
                max: Nanometers::new(650.0),
                band: "synthetic-300-650-nm".into(),
            },
            assumptions: "synthetic mathematical validation profile".into(),
            provenance: "NSB unit test".into(),
            license: "CC0-1.0".into(),
            validated_zenith: ValidatedZenithDomain {
                min: Degrees::new(0.0),
                max: Degrees::new(90.0),
            },
        }
    }

    #[test]
    fn profile_validation_fails_closed_for_bad_samples_and_domains() {
        let mut cases = Vec::new();

        let mut too_short = valid_definition();
        too_short.altitude_km.truncate(2);
        too_short.relative_emissivity.truncate(2);
        cases.push(too_short);

        let mut unsorted = valid_definition();
        unsorted.altitude_km.swap(1, 2);
        cases.push(unsorted);

        let mut duplicate = valid_definition();
        duplicate.altitude_km[2] = duplicate.altitude_km[1];
        cases.push(duplicate);

        for bad in [-1.0, f64::NAN, f64::INFINITY] {
            let mut invalid_emissivity = valid_definition();
            invalid_emissivity.relative_emissivity[1] = bad;
            cases.push(invalid_emissivity);
        }

        let mut invalid_altitude = valid_definition();
        invalid_altitude.altitude_km[1] = Kilometers::new(f64::NAN);
        cases.push(invalid_altitude);

        let mut zero = valid_definition();
        zero.relative_emissivity.fill(0.0);
        cases.push(zero);

        let mut schema = valid_definition();
        schema.schema_version += 1;
        cases.push(schema);

        let mut wavelength = valid_definition();
        wavelength.wavelength.min = Nanometers::new(400.0);
        cases.push(wavelength);

        let mut zenith = valid_definition();
        zenith.validated_zenith.max = Degrees::new(91.0);
        cases.push(zenith);

        let mut provenance = valid_definition();
        provenance.provenance.clear();
        cases.push(provenance);

        for definition in cases {
            assert!(VerticalEmissionProfile::new(definition).is_err());
        }
    }

    #[test]
    fn declared_zenith_domain_fails_honestly() {
        use siderust::coordinates::centers::Geodetic;
        use siderust::qtty::Meters;

        let mut definition = valid_definition();
        definition.validated_zenith.max = Degrees::new(80.0);
        let profile = VerticalEmissionProfile::new(definition).unwrap();
        let observer = Geodetic::new_raw(Degrees::new(0.0), Degrees::new(0.0), Meters::new(0.0));
        assert!(profile
            .geometry_factor(observer, Degrees::new(81.0))
            .is_err());
    }
}
