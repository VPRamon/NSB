use super::coordinates::equatorial_to_galactic;
use super::map::StarlightMap;
use super::output::StarlightOutputs;
use super::photometry::scale_outputs;
use super::provenance::StarlightProvenance;
use crate::error::{NsbError, Result};
use crate::evaluator::Target;
use std::io::ErrorKind;
use std::path::Path;

const STANDARD_MAP_FILE: &str = "data/starlight_galactic_map_v1.csv";

#[derive(Debug, Clone)]
pub struct Starlight {
    map: StarlightMap,
    scale: f64,
}

impl Starlight {
    pub fn standard_galactic_model() -> Result<Self> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(STANDARD_MAP_FILE);
        match StarlightMap::from_csv_path(path, StarlightProvenance::standard_galactic_model_v1()) {
            Ok(map) => Ok(Self::with_map(map)),
            Err(NsbError::Io(err)) if err.kind() == ErrorKind::NotFound => Err(data_missing()),
            Err(err) => Err(err),
        }
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
        let galactic = equatorial_to_galactic(target);
        Ok(scale_outputs(
            self.map.lookup(galactic.lon, galactic.lat),
            self.scale,
        ))
    }

    pub fn map(&self) -> &StarlightMap {
        &self.map
    }
}

fn data_missing() -> NsbError {
    NsbError::DataMissing {
        file: STANDARD_MAP_FILE,
        message: "standard Galactic starlight map is not bundled; generate a provenance-recorded map with `cargo run -p nsb-data-tools --bin build_starlight_map -- ...` or provide one with Starlight::with_map(...)".to_string(),
    }
}
