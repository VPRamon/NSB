use super::map::StarlightMap;
use super::output::StarlightOutputs;
use super::photometry::scale_outputs;
use super::provenance::StarlightProvenance;
#[cfg(nsb_bundled_production_starlight)]
use super::validated::ValidatedStarlightMap;
use crate::assets::BUNDLED_PRODUCTION_STARLIGHT_AVAILABLE;
#[cfg(nsb_bundled_production_starlight)]
use crate::assets::{BUNDLED_PRODUCTION_STARLIGHT_MANIFEST, BUNDLED_PRODUCTION_STARLIGHT_MAP};
use crate::error::{NsbError, Result};
use crate::evaluator::Target;
use crate::units::ScaleFactors;
use siderust::coordinates::spherical::direction;
use siderust::coordinates::transform::TransformFrame;

#[derive(Debug, Clone)]
/// Directional starlight evaluator backed by one immutable map.
pub struct Starlight {
    map: StarlightMap,
    scale: ScaleFactors,
}

impl Starlight {
    /// Return whether a validated production Gaia DR3 starlight map is bundled.
    pub const fn bundled_production_available() -> bool {
        BUNDLED_PRODUCTION_STARLIGHT_AVAILABLE
    }

    /// Load the bundled production Gaia DR3 XP-derived starlight map.
    ///
    /// This succeeds only when a release CSV and runtime manifest are both
    /// registered in `crates/nsb/data/manifest.toml`, checksum-verified by the
    /// build script, embedded as static bytes, and admitted by
    /// [`ValidatedStarlightMap`].
    #[cfg(nsb_bundled_production_starlight)]
    pub fn bundled_production_model() -> Result<Self> {
        let validated = ValidatedStarlightMap::from_bytes_and_manifest(
            BUNDLED_PRODUCTION_STARLIGHT_MAP.as_bytes(),
            BUNDLED_PRODUCTION_STARLIGHT_MANIFEST,
        )?;
        Ok(Self::with_map(validated.map().clone()))
    }

    /// Report a missing bundled production starlight asset.
    #[cfg(not(nsb_bundled_production_starlight))]
    pub fn bundled_production_model() -> Result<Self> {
        Err(missing_bundled_production_asset())
    }

    /// Return provenance from the checksum-verified bundled production map.
    pub fn bundled_production_provenance() -> Result<StarlightProvenance> {
        Ok(Self::bundled_production_model()?.map().provenance().clone())
    }

    /// Build from a caller-provided validated map.
    pub fn with_map(map: StarlightMap) -> Self {
        Self {
            map,
            scale: ScaleFactors::new(1.0),
        }
    }

    /// Apply a non-negative multiplicative radiance scale.
    pub fn with_scale(mut self, scale: ScaleFactors) -> Self {
        self.scale = scale;
        self
    }

    /// Transform a target to Galactic coordinates and evaluate the map.
    pub fn compute(&self, target: Target) -> Result<StarlightOutputs> {
        if !self.scale.is_finite() || self.scale < ScaleFactors::new(0.0) {
            return Err(NsbError::OutOfRange(
                "starlight scale must be finite and non-negative".to_string(),
            ));
        }
        let galactic: direction::Galactic = target.to_frame();
        Ok(scale_outputs(
            self.map.lookup(galactic.to_cartesian()),
            self.scale,
        ))
    }

    /// Return the backing map.
    pub fn map(&self) -> &StarlightMap {
        &self.map
    }
}

#[cfg(not(nsb_bundled_production_starlight))]
fn missing_bundled_production_asset() -> NsbError {
    NsbError::DataMissing {
        file: "data/manifest.toml",
        message: concat!(
            "bundled production starlight asset is not registered; generate and commit ",
            "the Gaia DR3 XP nside=128 release CSV and runtime manifest, then register both ",
            "as runtime_embedded production assets"
        )
        .to_string(),
    }
}
