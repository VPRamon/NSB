use super::{StarlightMap, ValidatedStarlightMap};
use crate::assets::BUNDLED_PRODUCTION_STARLIGHT_AVAILABLE;

/// Explicit integrated-starlight data-product selection.
///
/// This selects the admitted map product that backs the Starlight component; it
/// is deliberately separate from scientific-model selectors such as
/// [`crate::AirglowModel`] and [`crate::MoonlightModel`]. Additional admission paths may be
/// added; downstream matches should include a wildcard arm.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum StarlightProduct {
    /// Use the validated bundled Gaia DR3 XP-derived production map.
    BundledProductionGaiaDr3,
    /// Use a caller-supplied map for experiments without a production claim.
    ExperimentalMap(Box<StarlightMap>),
    /// Use an external map admitted through the production manifest contract.
    ValidatedExternalMap(Box<ValidatedStarlightMap>),
}

impl StarlightProduct {
    /// Select the bundled production Gaia DR3 XP-derived map.
    pub fn bundled_production_gaia_dr3() -> Self {
        Self::BundledProductionGaiaDr3
    }

    /// Select a caller-provided map without a production validation claim.
    pub fn with_experimental_map(map: StarlightMap) -> Self {
        Self::ExperimentalMap(Box::new(map))
    }

    /// Select a manifest-validated external production map.
    pub fn validated_external(map: ValidatedStarlightMap) -> Self {
        Self::ValidatedExternalMap(Box::new(map))
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
