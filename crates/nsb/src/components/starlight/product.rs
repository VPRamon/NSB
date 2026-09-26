use super::{StarlightMap, ValidatedStarlightMap};
use crate::assets::BUNDLED_PRODUCTION_STARLIGHT_AVAILABLE;
use std::sync::Arc;

/// Explicit integrated-starlight data-product selection.
///
/// This selects the admitted map product that backs the Starlight component; it
/// is deliberately separate from scientific-model selectors such as
/// [`crate::AirglowModel`] and [`crate::MoonlightModel`]. Additional admission
/// paths may be added; downstream matches should include a wildcard arm.
///
/// Heavy HEALPix map payloads use shared ownership ([`Arc`]) so configuring an
/// evaluator from a caller-provided map does not require an unnecessary deep
/// copy of pixel data.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum StarlightProduct {
    /// Use the validated bundled Gaia DR3 XP-derived production map.
    BundledProductionGaiaDr3,
    /// Use a caller-supplied map for experiments without a production claim.
    ExperimentalMap(Arc<StarlightMap>),
    /// Use an external map admitted through the production manifest contract.
    ValidatedExternalMap(Arc<ValidatedStarlightMap>),
}

impl StarlightProduct {
    /// Select the bundled production Gaia DR3 XP-derived map.
    pub fn bundled_production_gaia_dr3() -> Self {
        Self::BundledProductionGaiaDr3
    }

    /// Select a caller-provided map without a production validation claim.
    ///
    /// The map is wrapped in [`Arc`] so subsequent evaluator construction can
    /// share the pixel data without cloning the HEALPix payload.
    pub fn with_experimental_map(map: StarlightMap) -> Self {
        Self::ExperimentalMap(Arc::new(map))
    }

    /// Select a shared caller-provided map without copying pixel data.
    pub fn with_shared_experimental_map(map: Arc<StarlightMap>) -> Self {
        Self::ExperimentalMap(map)
    }

    /// Select a manifest-validated external production map.
    pub fn validated_external(map: ValidatedStarlightMap) -> Self {
        Self::ValidatedExternalMap(Arc::new(map))
    }

    /// Select a shared validated external map without copying pixel data.
    pub fn with_shared_validated_external_map(map: Arc<ValidatedStarlightMap>) -> Self {
        Self::ValidatedExternalMap(map)
    }

    /// Return whether a validated production Gaia DR3 starlight map is bundled.
    pub const fn bundled_production_available() -> bool {
        BUNDLED_PRODUCTION_STARLIGHT_AVAILABLE
    }

    /// Stable machine-readable product identity.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::BundledProductionGaiaDr3 => "bundled-production-gaia-dr3",
            Self::ExperimentalMap(_) => "experimental-map",
            Self::ValidatedExternalMap(_) => "validated-external-map",
        }
    }
}
