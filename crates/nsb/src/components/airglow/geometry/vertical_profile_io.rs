//! TOML persistence for validated vertical-emission profiles.
//!
//! Owns schema parsing, serialization/deserialization, and checksum verification
//! against the canonical identity computed by the domain module. Conversion
//! always yields a [`VerticalEmissionProfile`] constructed through the
//! validated domain boundary.

use super::vertical_profile::{
    AirglowWavelengthApplicability, ValidatedZenithDomain, VerticalEmissionProfile,
    VerticalEmissionProfileDefinition, VerticalEmissionProfileError, VerticalProfileNormalization,
};
use serde::{Deserialize, Serialize};
use siderust::qtty::{Degrees, Kilometers, Nanometers};

impl VerticalEmissionProfile {
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
        let wavelength = profile.wavelength_applicability();
        let zenith = profile.validated_zenith_domain();
        Self {
            schema_version: profile.schema_version(),
            profile_id: profile.profile_id().to_string(),
            altitude_km: profile
                .altitude_km()
                .iter()
                .map(|value| value.value())
                .collect(),
            relative_emissivity: profile.relative_emissivity().to_vec(),
            normalization: profile.normalization(),
            wavelength_min_nm: wavelength.min.value(),
            wavelength_max_nm: wavelength.max.value(),
            wavelength_band: wavelength.band.clone(),
            assumptions: profile.assumptions().to_string(),
            provenance: profile.provenance().to_string(),
            license: profile.license().to_string(),
            validated_zenith_min_deg: zenith.min.value(),
            validated_zenith_max_deg: zenith.max.value(),
            checksum_sha256: profile.checksum_sha256().to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::vertical_profile::{
        AirglowWavelengthApplicability, ValidatedZenithDomain, VerticalEmissionProfile,
        VerticalEmissionProfileDefinition, VerticalEmissionProfileError,
        VerticalProfileNormalization, VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION,
    };
    use siderust::qtty::{Degrees, Kilometers, Nanometers};

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
}
