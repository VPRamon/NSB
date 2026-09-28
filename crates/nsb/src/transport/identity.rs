//! Identity / no-atmosphere transport.
//!
//! Contract: `output == input` within the representation's exact numerical
//! semantics. Used for top-of-atmosphere products, regression isolation, and
//! callers that apply propagation externally.

use qtty::dimensionless::Transmittances;
use qtty::radiometry::WattsPerSquareMeterSteradianNanometer;
use siderust::qtty::Nanometers;

use super::geometry::DirectPathGeometry;
use super::metadata::{
    AbsorptionTreatment, ApproximationState, ExtinctionIngredientFlags, ScatteringIngredientFlags,
    TransportMetadata, TransportPathKind, UncertaintyReporting,
};
use crate::site::AtmosphericConditions;

/// No-atmosphere transport: transmission is identically one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IdentityTransport;

impl IdentityTransport {
    /// Construct the identity transport.
    pub const fn new() -> Self {
        Self
    }

    /// Transmission is exactly `1` for every wavelength and geometry.
    pub fn transmission(
        &self,
        _wavelength: Nanometers,
        _geometry: DirectPathGeometry,
        _atmosphere: AtmosphericConditions,
    ) -> Transmittances {
        Transmittances::new(1.0)
    }

    /// Return the incident spectral radiance unchanged.
    pub fn apply_spectral(
        &self,
        incident: WattsPerSquareMeterSteradianNanometer,
        _wavelength: Nanometers,
        _geometry: DirectPathGeometry,
        _atmosphere: AtmosphericConditions,
    ) -> WattsPerSquareMeterSteradianNanometer {
        incident
    }

    /// Scientific metadata for the identity path.
    pub fn metadata(&self) -> TransportMetadata {
        TransportMetadata {
            model_id: "identity",
            path_kind: TransportPathKind::Identity,
            atmosphere_profile_id: "unused",
            extinction: ExtinctionIngredientFlags {
                rayleigh: false,
                mie: false,
                absorption: AbsorptionTreatment::None,
            },
            scattering: ScatteringIngredientFlags {
                rayleigh_phase: false,
                mie_phase: false,
            },
            airmass_model_id: "none",
            approximation: ApproximationState::ExactWithinRepresentation,
            validated_domain: "any finite typed spectral radiance; atmosphere ignored",
            provenance: concat!(
                "NSB identity atmospheric transport; no extinction or scattering; ",
                "output equals input within the quantity representation"
            ),
            uncertainty: UncertaintyReporting::Absent,
        }
    }
}
