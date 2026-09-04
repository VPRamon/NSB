//! Airglow target and emitting-volume line-of-sight geometry.
//!
//! Two deliberately separate geometry models are supported:
//!
//! - [`VanRhijnConfig`] preserves the historical NSB/Siderust thin-shell
//!   correction exactly. It is the fast default and uses the 6,371 km mean
//!   Earth radius built into Siderust.
//! - [`VerticalEmissionProfile`] integrates a piecewise-linear emissivity
//!   profile through a spherical atmosphere and normalizes the result by the
//!   same profile viewed at zenith.
//!
//! Atmospheric extinction is not part of either model. The Noll Rayleigh/Mie
//! scattering stage is applied independently in `extinction.rs`.

use crate::error::{NsbError, Result};
use crate::units::ScaleFactors;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use siderust::atmosphere::van_rhijn_factor;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::{EquatorialMeanJ2000, ECEF};
use siderust::coordinates::spherical::Direction as SphericalDirection;
use siderust::event::horizontal::star_horizontal;
use siderust::qtty::{
    unit::{Kilometer, Nanometer, Radian},
    Degrees, Kilometers, Nanometers,
};
use std::sync::Arc;
use tempoch::{Time, JD, TT, UTC};
use thiserror::Error;

/// Current schema accepted for persisted vertical-emission profiles.
pub const VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION: u32 = 1;
/// Current implementation identifier for the reference spherical LOS integrator.
pub(crate) const VERTICAL_PROFILE_INTEGRATOR_VERSION: &str = "spherical-los-simpson-v1";
/// Implementation identifier for the preserved Siderust Van Rhijn baseline.
pub(crate) const VAN_RHIJN_IMPLEMENTATION_VERSION: &str = "siderust-0.11.0-mean-earth-radius";
/// Historical NSB effective emitting-shell height.
pub const DEFAULT_VAN_RHIJN_EMISSION_HEIGHT_KM: Kilometers = Kilometers::new(90.0);
/// Mean spherical Earth radius used by Siderust's Van Rhijn implementation.
pub(crate) const AIRGLOW_MEAN_EARTH_RADIUS_KM: Kilometers = Kilometers::new(6_371.0);
/// Production reference resolution per profile interval (must be even).
pub(crate) const VERTICAL_PROFILE_REFERENCE_SUBSTEPS: usize = 64;

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
pub struct AirglowWavelengthApplicability {
    /// Inclusive lower wavelength bound.
    pub min: Nanometers,
    /// Inclusive upper wavelength bound.
    pub max: Nanometers,
    /// Stable human-readable band or process identifier.
    pub band: String,
}

/// Zenith-angle interval for which a profile is declared usable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ValidatedZenithDomain {
    /// Inclusive minimum zenith angle.
    pub min: Degrees,
    /// Inclusive maximum zenith angle.
    pub max: Degrees,
}

/// Complete programmatic definition of a vertical-emission profile.
#[derive(Debug, Clone, PartialEq)]
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

    /// Parse a persisted TOML profile and require its checksum pin to match.
    pub fn from_toml_str(input: &str) -> std::result::Result<Self, VerticalEmissionProfileError> {
        let persisted: PersistedVerticalEmissionProfile = toml::from_str(input)
            .map_err(|error| VerticalEmissionProfileError::Toml(error.to_string()))?;
        if persisted.checksum_sha256.trim().is_empty() {
            return Err(VerticalEmissionProfileError::Invalid(
                "persisted profiles require checksum_sha256".into(),
            ));
        }
        let expected = persisted.checksum_sha256.clone();
        let profile = Self::new(persisted.into_definition())?;
        if expected != profile.checksum_sha256() {
            return Err(VerticalEmissionProfileError::ChecksumMismatch {
                expected,
                computed: profile.checksum_sha256().to_string(),
            });
        }
        Ok(profile)
    }

    /// Serialize the canonical normalized profile with its checksum pin.
    pub fn to_toml_string(&self) -> std::result::Result<String, VerticalEmissionProfileError> {
        toml::to_string_pretty(&PersistedVerticalEmissionProfile::from_profile(self))
            .map_err(|error| VerticalEmissionProfileError::Toml(error.to_string()))
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
    pub fn geometry_factor(
        &self,
        observer: Geodetic<ECEF>,
        zenith: Degrees,
    ) -> Result<ScaleFactors> {
        self.geometry_factor_with_substeps(observer, zenith, VERTICAL_PROFILE_REFERENCE_SUBSTEPS)
    }

    /// Evaluate the same reference integrator at an explicit even resolution.
    ///
    /// This is exposed for convergence validation and benchmarking. Production
    /// evaluation uses `VERTICAL_PROFILE_REFERENCE_SUBSTEPS`.
    pub fn geometry_factor_with_substeps(
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
        let vertical = integrate_profile_los(self, observer_height_km, 0.0, substeps_per_interval);
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
            self,
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

/// Explicit configuration for the fast thin-shell Van Rhijn baseline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VanRhijnConfig {
    emission_height_km: Kilometers,
}

impl VanRhijnConfig {
    /// Construct a validated thin-shell configuration.
    pub fn new(
        emission_height_km: Kilometers,
    ) -> std::result::Result<Self, VerticalEmissionProfileError> {
        if !emission_height_km.is_finite() || emission_height_km <= Kilometers::new(0.0) {
            return Err(VerticalEmissionProfileError::Invalid(
                "Van Rhijn emission height must be finite and positive".into(),
            ));
        }
        Ok(Self { emission_height_km })
    }

    pub(crate) const fn from_continuum_height(emission_height_km: Kilometers) -> Self {
        Self { emission_height_km }
    }

    /// Effective altitude of the geometrically thin emitting shell.
    pub const fn emission_height_km(self) -> Kilometers {
        self.emission_height_km
    }

    /// Full above-horizon domain of the analytic formula.
    pub const fn validated_zenith_domain(self) -> ValidatedZenithDomain {
        ValidatedZenithDomain {
            min: Degrees::new(0.0),
            max: Degrees::new(90.0),
        }
    }
}

impl Default for VanRhijnConfig {
    fn default() -> Self {
        Self {
            emission_height_km: DEFAULT_VAN_RHIJN_EMISSION_HEIGHT_KM,
        }
    }
}

/// Configurable Airglow emitting-volume line-of-sight geometry.
///
/// Additional validated geometries may be added; match with a wildcard.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum AirglowGeometryModel {
    /// Fast, geometrically thin emitting shell; the historical NSB default.
    VanRhijn(VanRhijnConfig),
    /// Spherical LOS integration of a validated vertical emissivity profile.
    VerticalProfile(VerticalEmissionProfile),
}

impl AirglowGeometryModel {
    /// Evaluate the selected dimensionless line-of-sight correction.
    pub fn geometry_factor(
        &self,
        observer: Geodetic<ECEF>,
        zenith: Degrees,
    ) -> Result<ScaleFactors> {
        match self {
            Self::VanRhijn(config) => {
                let z = zenith.value();
                if !z.is_finite() || !(0.0..=90.0).contains(&z) {
                    return Err(NsbError::OutOfRange(format!(
                        "Van Rhijn zenith angle must be in [0, 90] deg, got {z}"
                    )));
                }
                let factor =
                    van_rhijn_factor(zenith.to::<Radian>(), config.emission_height_km()).value();
                if !factor.is_finite() || factor <= 0.0 {
                    return Err(NsbError::Unsupported(
                        "Van Rhijn configuration produced a non-finite geometry factor".into(),
                    ));
                }
                Ok(ScaleFactors::new(factor))
            }
            Self::VerticalProfile(profile) => profile.geometry_factor(observer, zenith),
        }
    }

    /// Stable model identifier used in scientific metadata.
    pub const fn model_id(&self) -> &'static str {
        match self {
            Self::VanRhijn(_) => "van_rhijn",
            Self::VerticalProfile(_) => "vertical_profile",
        }
    }

    /// Structured provenance for saved scientific results.
    pub fn metadata(&self) -> AirglowGeometryMetadata {
        match self {
            Self::VanRhijn(config) => AirglowGeometryMetadata {
                model: self.model_id(),
                implementation_version: VAN_RHIJN_IMPLEMENTATION_VERSION,
                emission_height_km: Some(config.emission_height_km()),
                profile_id: None,
                profile_schema_version: None,
                checksum_sha256: None,
                normalization: None,
                altitude_min_km: None,
                altitude_max_km: None,
                wavelength_min_nm: None,
                wavelength_max_nm: None,
                wavelength_band: None,
                assumptions: "geometrically thin, horizontally uniform emitting shell; historical NSB baseline".into(),
                provenance: "Van Rhijn (1921) analytic shell factor implemented by Siderust 0.11.0".into(),
                license: None,
                validated_zenith: config.validated_zenith_domain(),
            },
            Self::VerticalProfile(profile) => {
                let wavelength = profile.wavelength_applicability();
                AirglowGeometryMetadata {
                    model: self.model_id(),
                    implementation_version: VERTICAL_PROFILE_INTEGRATOR_VERSION,
                    emission_height_km: None,
                    profile_id: Some(profile.profile_id().to_string()),
                    profile_schema_version: Some(profile.schema_version()),
                    checksum_sha256: Some(profile.checksum_sha256().to_string()),
                    normalization: Some(profile.normalization().as_str()),
                    altitude_min_km: profile.altitude_km().first().copied(),
                    altitude_max_km: profile.altitude_km().last().copied(),
                    wavelength_min_nm: Some(wavelength.min),
                    wavelength_max_nm: Some(wavelength.max),
                    wavelength_band: Some(wavelength.band.clone()),
                    assumptions: profile.assumptions().to_string(),
                    provenance: profile.provenance().to_string(),
                    license: Some(profile.license().to_string()),
                    validated_zenith: profile.validated_zenith_domain(),
                }
            }
        }
    }
}

impl Default for AirglowGeometryModel {
    fn default() -> Self {
        Self::VanRhijn(VanRhijnConfig::default())
    }
}

/// Geometry provenance attached to Airglow component metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct AirglowGeometryMetadata {
    /// `van_rhijn` or `vertical_profile`.
    pub model: &'static str,
    /// Versioned implementation identifier.
    pub implementation_version: &'static str,
    /// Thin-shell effective height, when applicable.
    pub emission_height_km: Option<Kilometers>,
    /// Vertical-profile identifier, when applicable.
    pub profile_id: Option<String>,
    /// Vertical-profile schema, when applicable.
    pub profile_schema_version: Option<u32>,
    /// Canonical vertical-profile checksum, when applicable.
    pub checksum_sha256: Option<String>,
    /// Vertical-profile normalization, when applicable.
    pub normalization: Option<&'static str>,
    /// Lowest profile altitude, when applicable.
    pub altitude_min_km: Option<Kilometers>,
    /// Highest profile altitude, when applicable.
    pub altitude_max_km: Option<Kilometers>,
    /// Lower wavelength applicability bound, when applicable.
    pub wavelength_min_nm: Option<Nanometers>,
    /// Upper wavelength applicability bound, when applicable.
    pub wavelength_max_nm: Option<Nanometers>,
    /// Wavelength band/process label, when applicable.
    pub wavelength_band: Option<String>,
    /// Scientific assumptions/reference state.
    pub assumptions: String,
    /// Geometry/profile provenance.
    pub provenance: String,
    /// Profile license, when applicable.
    pub license: Option<String>,
    /// Declared zenith-angle domain.
    pub validated_zenith: ValidatedZenithDomain,
}

pub(crate) fn target_altitude(
    time: Time<UTC>,
    location: Geodetic<ECEF>,
    target: SphericalDirection<EquatorialMeanJ2000>,
) -> Degrees {
    let jd = time.to::<TT>().to::<JD>();
    star_horizontal(target.ra(), target.dec(), &location, jd).alt()
}

/// Integrate `j(h(s)) ds` over every profile interval using composite Simpson.
///
/// With observer radius `r0 = R + h_obs` and above-horizon zenith angle `z`,
/// the spherical ray is
///
/// `h(s) = sqrt(r0² + s² + 2 r0 s cos(z)) - R`.
///
/// Interval endpoints are transformed exactly from altitude to path length,
/// avoiding a plane-parallel approximation and keeping the horizon finite.
fn integrate_profile_los(
    profile: &VerticalEmissionProfile,
    observer_height_km: f64,
    zenith_rad: f64,
    substeps: usize,
) -> f64 {
    let r0 = AIRGLOW_MEAN_EARTH_RADIUS_KM.value() + observer_height_km;
    let sin_z = zenith_rad.sin();
    let cos_z = zenith_rad.cos().max(0.0);
    let altitudes = profile.altitude_km();
    let emissivities = profile.relative_emissivity();
    let mut total = 0.0;

    for index in 0..altitudes.len() - 1 {
        let bin_low = altitudes[index].value();
        let bin_high = altitudes[index + 1].value();
        let low = bin_low.max(observer_height_km);
        if low >= bin_high {
            continue;
        }
        let s_low = distance_to_altitude(r0, low, sin_z, cos_z);
        let s_high = distance_to_altitude(r0, bin_high, sin_z, cos_z);
        let ds = (s_high - s_low) / substeps as f64;
        let mut weighted = 0.0;
        for step in 0..=substeps {
            let s = s_low + ds * step as f64;
            let radius = (r0 * r0 + s * s + 2.0 * r0 * s * cos_z).sqrt();
            let altitude = radius - AIRGLOW_MEAN_EARTH_RADIUS_KM.value();
            let fraction = ((altitude - bin_low) / (bin_high - bin_low)).clamp(0.0, 1.0);
            let emissivity =
                emissivities[index] + fraction * (emissivities[index + 1] - emissivities[index]);
            let weight = if step == 0 || step == substeps {
                1.0
            } else if step % 2 == 0 {
                2.0
            } else {
                4.0
            };
            weighted += weight * emissivity;
        }
        total += ds * weighted / 3.0;
    }
    total
}

fn distance_to_altitude(r0: f64, altitude_km: f64, sin_z: f64, cos_z: f64) -> f64 {
    let radius = AIRGLOW_MEAN_EARTH_RADIUS_KM.value() + altitude_km;
    let discriminant = (radius * radius - r0 * r0 * sin_z * sin_z).max(0.0);
    (-r0 * cos_z + discriminant.sqrt()).max(0.0)
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedVerticalEmissionProfile {
    schema_version: u32,
    profile_id: String,
    altitude_km: Vec<f64>,
    relative_emissivity: Vec<f64>,
    normalization: VerticalProfileNormalization,
    wavelength_min_nm: f64,
    wavelength_max_nm: f64,
    wavelength_band: String,
    assumptions: String,
    provenance: String,
    license: String,
    validated_zenith_min_deg: f64,
    validated_zenith_max_deg: f64,
    checksum_sha256: String,
}

impl PersistedVerticalEmissionProfile {
    fn into_definition(self) -> VerticalEmissionProfileDefinition {
        VerticalEmissionProfileDefinition {
            schema_version: self.schema_version,
            profile_id: self.profile_id,
            altitude_km: self.altitude_km.into_iter().map(Kilometers::new).collect(),
            relative_emissivity: self.relative_emissivity,
            normalization: self.normalization,
            wavelength: AirglowWavelengthApplicability {
                min: Nanometers::new(self.wavelength_min_nm),
                max: Nanometers::new(self.wavelength_max_nm),
                band: self.wavelength_band,
            },
            assumptions: self.assumptions,
            provenance: self.provenance,
            license: self.license,
            validated_zenith: ValidatedZenithDomain {
                min: Degrees::new(self.validated_zenith_min_deg),
                max: Degrees::new(self.validated_zenith_max_deg),
            },
        }
    }

    fn from_profile(profile: &VerticalEmissionProfile) -> Self {
        let definition = &profile.0.definition;
        Self {
            schema_version: definition.schema_version,
            profile_id: definition.profile_id.clone(),
            altitude_km: definition
                .altitude_km
                .iter()
                .map(|value| value.value())
                .collect(),
            relative_emissivity: definition.relative_emissivity.clone(),
            normalization: definition.normalization,
            wavelength_min_nm: definition.wavelength.min.value(),
            wavelength_max_nm: definition.wavelength.max.value(),
            wavelength_band: definition.wavelength.band.clone(),
            assumptions: definition.assumptions.clone(),
            provenance: definition.provenance.clone(),
            license: definition.license.clone(),
            validated_zenith_min_deg: definition.validated_zenith.min.value(),
            validated_zenith_max_deg: definition.validated_zenith.max.value(),
            checksum_sha256: profile.checksum_sha256().to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use siderust::qtty::Meters;

    fn observer(height_m: f64) -> Geodetic<ECEF> {
        Geodetic::new_raw(
            Degrees::new(12.345),
            Degrees::new(-43.21),
            Meters::new(height_m),
        )
    }

    fn profile(id: &str, altitudes: &[f64], emissivities: &[f64]) -> VerticalEmissionProfile {
        VerticalEmissionProfile::new(VerticalEmissionProfileDefinition {
            schema_version: VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION,
            profile_id: id.into(),
            altitude_km: altitudes.iter().copied().map(Kilometers::new).collect(),
            relative_emissivity: emissivities.to_vec(),
            normalization: VerticalProfileNormalization::UnitVerticalIntegral,
            wavelength: AirglowWavelengthApplicability {
                min: Nanometers::new(300.0),
                max: Nanometers::new(650.0),
                band: "synthetic-300-650-nm".into(),
            },
            assumptions: "synthetic mathematical validation profile; not physical data".into(),
            provenance: "generated in deterministic NSB unit test".into(),
            license: "CC0-1.0 synthetic fixture".into(),
            validated_zenith: ValidatedZenithDomain {
                min: Degrees::new(0.0),
                max: Degrees::new(90.0),
            },
        })
        .unwrap()
    }

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
    fn van_rhijn_default_is_the_historical_explicit_configuration() {
        let default = AirglowGeometryModel::default();
        let explicit =
            AirglowGeometryModel::VanRhijn(VanRhijnConfig::new(Kilometers::new(90.0)).unwrap());
        for zenith in [0.0, 30.0, 60.0, 80.0, 90.0] {
            let default_factor = default
                .geometry_factor(observer(2_635.0), Degrees::new(zenith))
                .unwrap();
            let explicit_factor = explicit
                .geometry_factor(observer(2_635.0), Degrees::new(zenith))
                .unwrap();
            assert_eq!(
                default_factor.value().to_bits(),
                explicit_factor.value().to_bits()
            );
        }
    }

    #[test]
    fn vertical_profile_is_exactly_normalized_at_zenith() {
        let profile = profile("broad-triangle", &[75.0, 90.0, 110.0], &[0.0, 1.0, 0.0]);
        for height_m in [-400.0, 0.0, 2_400.0, 5_000.0] {
            assert_eq!(
                profile
                    .geometry_factor(observer(height_m), Degrees::new(0.0))
                    .unwrap()
                    .value(),
                1.0
            );
        }
    }

    #[test]
    fn profile_without_visible_emission_above_observer_fails_at_all_angles() {
        let profile = profile(
            "emission-below-observer",
            &[0.0, 1.0, 2.0],
            &[1.0, 0.0, 0.0],
        );
        let location = observer(1_500.0);
        for zenith in [0.0, 30.0, 90.0] {
            let error = profile
                .geometry_factor(location, Degrees::new(zenith))
                .unwrap_err();
            assert!(error
                .to_string()
                .contains("contains no visible emission above observer altitude"));
        }
    }

    #[test]
    fn zero_padding_above_visible_emitting_layer_remains_valid() {
        let profile = profile(
            "zero-padded-visible-layer",
            &[80.0, 90.0, 100.0, 120.0],
            &[0.0, 1.0, 0.0, 0.0],
        );
        let location = observer(2_000.0);
        assert_eq!(
            profile
                .geometry_factor(location, Degrees::new(0.0))
                .unwrap()
                .value(),
            1.0
        );
        for zenith in [30.0, 90.0] {
            let factor = profile
                .geometry_factor(location, Degrees::new(zenith))
                .unwrap()
                .value();
            assert!(factor.is_finite() && factor > 0.0);
        }
    }

    #[test]
    fn thin_profile_converges_to_same_height_van_rhijn_shell() {
        let location = observer(0.0);
        let van_rhijn = AirglowGeometryModel::VanRhijn(VanRhijnConfig::default());
        let thin = profile(
            "thin-shell-90km-width-20m",
            &[89.99, 90.0, 90.01],
            &[0.0, 1.0, 0.0],
        );
        for (zenith, tolerance) in [
            (0.0, 0.0),
            (30.0, 2.0e-9),
            (60.0, 2.0e-8),
            (85.0, 2.0e-6),
            (90.0, 2.0e-5),
        ] {
            let expected = van_rhijn
                .geometry_factor(location, Degrees::new(zenith))
                .unwrap()
                .value();
            let actual = thin
                .geometry_factor_with_substeps(location, Degrees::new(zenith), 128)
                .unwrap()
                .value();
            let relative = ((actual - expected) / expected).abs();
            assert!(
                relative <= tolerance,
                "z={zenith}: vertical={actual}, van_rhijn={expected}, rel={relative}, tolerance={tolerance}"
            );
        }
    }

    #[test]
    fn representative_profiles_are_finite_positive_through_horizon() {
        let profiles = [
            profile("narrow", &[85.0, 90.0, 95.0], &[0.0, 1.0, 0.0]),
            profile("broad", &[75.0, 85.0, 100.0, 115.0], &[0.0, 0.8, 1.0, 0.0]),
            profile(
                "two-layer",
                &[75.0, 85.0, 90.0, 100.0, 110.0, 120.0],
                &[0.0, 1.0, 0.1, 0.2, 0.8, 0.0],
            ),
        ];
        for profile in profiles {
            let mut previous = 1.0;
            for zenith in [0.0, 30.0, 60.0, 75.0, 85.0, 89.0, 90.0] {
                let factor = profile
                    .geometry_factor(observer(2_000.0), Degrees::new(zenith))
                    .unwrap()
                    .value();
                assert!(factor.is_finite() && factor > 0.0);
                assert!(factor >= previous);
                previous = factor;
            }
        }
    }

    #[test]
    fn integration_converges_under_resolution_refinement() {
        let profile = profile(
            "asymmetric-broad",
            &[70.0, 78.0, 88.0, 97.0, 113.0],
            &[0.0, 0.25, 1.0, 0.35, 0.0],
        );
        let location = observer(1_700.0);
        let f16 = profile
            .geometry_factor_with_substeps(location, Degrees::new(88.0), 16)
            .unwrap()
            .value();
        let f32 = profile
            .geometry_factor_with_substeps(location, Degrees::new(88.0), 32)
            .unwrap()
            .value();
        let f64 = profile
            .geometry_factor_with_substeps(location, Degrees::new(88.0), 64)
            .unwrap()
            .value();
        let f128 = profile
            .geometry_factor_with_substeps(location, Degrees::new(88.0), 128)
            .unwrap()
            .value();
        assert!((f64 - f128).abs() < (f32 - f64).abs());
        assert!((f32 - f64).abs() < (f16 - f32).abs());
        assert!(((f64 - f128) / f128).abs() < 1.0e-10);
    }

    #[test]
    fn observer_altitude_changes_vertical_profile_factor() {
        let profile = profile("altitude-test", &[80.0, 90.0, 100.0], &[0.0, 1.0, 0.0]);
        let sea_level = profile
            .geometry_factor(observer(0.0), Degrees::new(80.0))
            .unwrap()
            .value();
        let mountain = profile
            .geometry_factor(observer(4_500.0), Degrees::new(80.0))
            .unwrap()
            .value();
        assert!(mountain > sea_level);
        assert!((mountain - sea_level) / sea_level > 1.0e-3);
    }

    #[test]
    fn arbitrary_longitude_and_latitude_are_supported() {
        let profile = profile("global-math", &[80.0, 90.0, 100.0], &[0.0, 1.0, 0.0]);
        for location in [
            Geodetic::new_raw(Degrees::new(151.2), Degrees::new(-33.9), Meters::new(58.0)),
            Geodetic::new_raw(Degrees::new(-149.9), Degrees::new(61.2), Meters::new(350.0)),
            Geodetic::new_raw(Degrees::new(0.0), Degrees::new(0.0), Meters::new(0.0)),
        ] {
            let factor = profile
                .geometry_factor(location, Degrees::new(70.0))
                .unwrap()
                .value();
            assert!(factor.is_finite() && factor > 1.0);
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
    fn persisted_profile_checksum_round_trip_is_deterministic_and_pinned() {
        let profile = profile("pinned", &[80.0, 90.0, 100.0], &[0.0, 1.0, 0.0]);
        assert_eq!(
            profile.checksum_sha256(),
            "sha256:aeab7c60b6c6d799bf4a342a49d1c51df2c336b2c428e08f919a29650545e90f"
        );
        let encoded = profile.to_toml_string().unwrap();
        let reparsed = VerticalEmissionProfile::from_toml_str(&encoded).unwrap();
        assert_eq!(profile.checksum_sha256(), reparsed.checksum_sha256());
        assert_eq!(profile.altitude_km(), reparsed.altitude_km());
        assert_eq!(
            profile.relative_emissivity(),
            reparsed.relative_emissivity()
        );

        let tampered = encoded.replace("profile_id = \"pinned\"", "profile_id = \"tampered\"");
        assert!(matches!(
            VerticalEmissionProfile::from_toml_str(&tampered),
            Err(VerticalEmissionProfileError::ChecksumMismatch { .. })
        ));

        let missing = encoded.replace(profile.checksum_sha256(), "");
        assert!(VerticalEmissionProfile::from_toml_str(&missing).is_err());

        let invalid_normalization =
            encoded.replace("unit-vertical-integral", "unsupported-normalization");
        assert!(VerticalEmissionProfile::from_toml_str(&invalid_normalization).is_err());
    }

    #[test]
    fn declared_zenith_domain_fails_honestly() {
        let mut definition = valid_definition();
        definition.validated_zenith.max = Degrees::new(80.0);
        let profile = VerticalEmissionProfile::new(definition).unwrap();
        assert!(profile
            .geometry_factor(observer(0.0), Degrees::new(81.0))
            .is_err());
    }
}
