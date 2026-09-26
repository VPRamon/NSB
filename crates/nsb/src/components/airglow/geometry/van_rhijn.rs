//! Thin-shell / Van Rhijn emitting-volume geometry.
//!
//! Owns configuration, validation, and evaluation for the analytic Van Rhijn
//! shell factor. Persistence and vertical-profile semantics live elsewhere.

use super::vertical_profile::{ValidatedZenithDomain, VerticalEmissionProfileError};
use crate::error::{NsbError, Result};
use crate::units::ScaleFactors;
use siderust::atmosphere::van_rhijn_factor;
use siderust::qtty::{unit::Radian, Degrees, Kilometers};

/// Implementation identifier for the preserved Siderust Van Rhijn baseline.
pub(crate) const VAN_RHIJN_IMPLEMENTATION_VERSION: &str = "siderust-0.11.0-mean-earth-radius";
/// Historical NSB effective emitting-shell height.
///
/// This is the retained default for the thin-shell model. It is not a universal
/// Airglow emission height; other geometry models and scientific components may
/// supply different layer altitudes explicitly.
pub(crate) const DEFAULT_VAN_RHIJN_EMISSION_HEIGHT_KM: Kilometers = Kilometers::new(90.0);

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

    /// Evaluate the dimensionless thin-shell line-of-sight correction.
    pub(crate) fn geometry_factor(self, zenith: Degrees) -> Result<ScaleFactors> {
        if !zenith.is_finite() || zenith < Degrees::new(0.0) || zenith > Degrees::new(90.0) {
            return Err(NsbError::OutOfRange(format!(
                "Van Rhijn zenith angle must be in [0, 90] deg, got {}",
                zenith.value()
            )));
        }
        // Siderust returns a typed scattering factor; ScaleFactors is the local
        // dimensionless domain type and currently constructs from a scalar.
        let factor = van_rhijn_factor(zenith.to::<Radian>(), self.emission_height_km()).value();
        if !factor.is_finite() || factor <= 0.0 {
            return Err(NsbError::Unsupported(
                "Van Rhijn configuration produced a non-finite geometry factor".into(),
            ));
        }
        Ok(ScaleFactors::new(factor))
    }
}

impl Default for VanRhijnConfig {
    fn default() -> Self {
        Self {
            emission_height_km: DEFAULT_VAN_RHIJN_EMISSION_HEIGHT_KM,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::AirglowGeometryModel;
    use super::*;
    use siderust::coordinates::centers::Geodetic;
    use siderust::coordinates::frames::ECEF;
    use siderust::qtty::Meters;

    fn observer(height_m: f64) -> Geodetic<ECEF> {
        Geodetic::new_raw(
            Degrees::new(12.345),
            Degrees::new(-43.21),
            Meters::new(height_m),
        )
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
    fn invalid_emission_height_is_rejected() {
        assert!(VanRhijnConfig::new(Kilometers::new(0.0)).is_err());
        assert!(VanRhijnConfig::new(Kilometers::new(-1.0)).is_err());
        assert!(VanRhijnConfig::new(Kilometers::new(f64::NAN)).is_err());
    }

    #[test]
    fn zenith_outside_domain_is_rejected() {
        let config = VanRhijnConfig::default();
        assert!(config.geometry_factor(Degrees::new(-0.1)).is_err());
        assert!(config.geometry_factor(Degrees::new(90.1)).is_err());
    }
}
