use super::map::StarlightMap;
use super::output::StarlightOutputs;
use super::photometry::scale_outputs;
use crate::error::{NsbError, Result};
use crate::evaluator::Target;
use siderust::coordinates::spherical::direction;
use siderust::coordinates::transform::TransformFrame;

const CATALOGUE_MAP_FILE: &str = "data/starlight_galactic_map_v1.csv";

#[derive(Debug, Clone)]
pub struct Starlight {
    map: StarlightMap,
    scale: f64,
}

impl Starlight {
    pub fn catalogue_galactic_model() -> Result<Self> {
        Err(data_missing())
    }

    pub fn with_map(map: StarlightMap) -> Self {
        Self { map, scale: 1.0 }
    }

    pub fn with_scale(mut self, scale: f64) -> Self {
        self.scale = scale;
        self
    }

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

    pub fn map(&self) -> &StarlightMap {
        &self.map
    }
}

fn data_missing() -> NsbError {
    NsbError::DataMissing {
        file: CATALOGUE_MAP_FILE,
        message: "catalogue-derived Galactic starlight map is not bundled yet; generate a real provenance-backed map before enabling BundledCatalogueMap".to_string(),
    }
}
