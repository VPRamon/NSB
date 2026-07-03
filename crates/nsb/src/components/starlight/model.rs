use super::map::StarlightMap;
use super::output::StarlightOutputs;
use super::photometry::scale_outputs;
use super::provenance::StarlightProvenance;
use crate::assets::asset_registry;
use crate::error::{NsbError, Result};
use crate::evaluator::Target;
use siderust::coordinates::spherical::direction;
use siderust::coordinates::transform::TransformFrame;

const BUNDLED_EXPERIMENTAL_SEED: &str = include_str!("../../../data/starlight_manual_seed_v1.csv");
const BUNDLED_EXPERIMENTAL_SEED_PATH: &str = "starlight_manual_seed_v1.csv";
const BUNDLED_EXPERIMENTAL_SEED_SHA256: &str =
    "a18c41ceeaaaf343e6991d6a718b6edf0b8cbfc46faf1cfaf7551c3d1c434668";

siderust::assert_data_checksum!(
    "NSB/data/starlight_manual_seed_v1.csv",
    BUNDLED_EXPERIMENTAL_SEED.as_bytes(),
    "a18c41ceeaaaf343e6991d6a718b6edf0b8cbfc46faf1cfaf7551c3d1c434668"
);

#[derive(Debug, Clone)]
/// Directional starlight evaluator backed by one immutable map.
pub struct Starlight {
    map: StarlightMap,
    scale: f64,
}

impl Starlight {
    /// Load the bundled low-resolution experimental seed.
    ///
    /// The seed exists to exercise the catalogue-map pipeline. Its provenance
    /// explicitly forbids production interpretation.
    pub fn experimental_seed_model() -> Result<Self> {
        verify_experimental_seed_registry()?;
        let map = StarlightMap::from_csv_str(
            BUNDLED_EXPERIMENTAL_SEED,
            StarlightProvenance::experimental_seed_v1(),
        )?;
        Ok(Self::with_map(map))
    }

    /// Return provenance from the checksum-verified bundled seed.
    pub fn bundled_experimental_provenance() -> Result<StarlightProvenance> {
        Ok(Self::experimental_seed_model()?.map().provenance().clone())
    }

    /// Build from a caller-provided validated map.
    pub fn with_map(map: StarlightMap) -> Self {
        Self { map, scale: 1.0 }
    }

    /// Apply a non-negative multiplicative radiance scale.
    pub fn with_scale(mut self, scale: f64) -> Self {
        self.scale = scale;
        self
    }

    /// Transform a target to Galactic coordinates and evaluate the map.
    pub fn compute(&self, target: Target) -> Result<StarlightOutputs> {
        if !self.scale.is_finite() || self.scale < 0.0 {
            return Err(NsbError::OutOfRange(
                "starlight scale must be finite and non-negative".to_string(),
            ));
        }
        let galactic: direction::Galactic = target.to_frame();
        Ok(scale_outputs(
            self.map.lookup(galactic.azimuth, galactic.polar),
            self.scale,
        ))
    }

    /// Return the backing map.
    pub fn map(&self) -> &StarlightMap {
        &self.map
    }
}

fn verify_experimental_seed_registry() -> Result<()> {
    let asset = asset_registry()
        .asset(BUNDLED_EXPERIMENTAL_SEED_PATH)
        .ok_or_else(|| NsbError::DataMissing {
            file: "data/manifest.toml",
            message: format!("missing registry entry for {BUNDLED_EXPERIMENTAL_SEED_PATH}"),
        })?;
    if asset.sha256 != BUNDLED_EXPERIMENTAL_SEED_SHA256
        || asset.calibration_status != "experimental"
    {
        return Err(NsbError::DataParse {
            file: "data/manifest.toml",
            message: "experimental starlight registry metadata does not match the embedded asset"
                .to_string(),
        });
    }
    for (key, expected) in &asset.header {
        let actual = BUNDLED_EXPERIMENTAL_SEED
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with('#'))
            .filter_map(|line| line.trim_start_matches('#').trim().split_once('='))
            .find_map(|(actual_key, value)| (actual_key.trim() == key).then(|| value.trim()));
        if actual != Some(expected.as_str()) {
            return Err(NsbError::DataParse {
                file: "starlight_manual_seed_v1.csv",
                message: format!(
                    "header key {key:?} does not match data/manifest.toml: expected {expected:?}, got {actual:?}"
                ),
            });
        }
    }
    Ok(())
}
