pub mod components;
pub mod error;
pub mod evaluator;
mod reference;
pub mod site;

pub use components::airglow::{
    Airglow, AirglowContinuum, AirglowOutputs, SolarFluxUnits, DEFAULT_SOLAR_RADIO_FLUX,
};
pub use components::moonlight::{AtmosphericConditions, Jones2013Spectral, KrisciunasSchaefer1991};
pub use components::starlight::{Starlight, StarlightMap, StarlightOutputs, StarlightProvenance};
pub use components::zodiacal::{
    ZodiacalBrightnessGrid, ZodiacalBrightnessModel, ZodiacalExtinction, ZodiacalLight,
    ZodiacalOutputs, ZodiacalSpectrum,
};
pub use error::{NsbError, Result};
pub use evaluator::{
    BandDiagnostic, CalibrationStatus, ComponentMask, MoonlightModel, NsbComponent,
    NsbComponentMetadata, NsbEvaluator, NsbModelConfig, NsbResult, Observer, PointQuery,
    StarlightModel, Target, ThresholdQuery, ThresholdQueryResult,
};
pub use site::{
    AirglowSiteCalibration, CalibrationStatus as SiteCalibrationStatus, SiteProfile,
    SiteProfileId,
};

pub type ComponentCalibrationStatus = CalibrationStatus;

pub use siderust::coordinates::frames::EquatorialMeanJ2000;
pub use siderust::coordinates::spherical::Direction as SphericalDirection;
pub use siderust::qtty::DEG;

pub(crate) const NSB_S10_ZP: f64 = 27.78;
