use super::metadata::{BandDiagnostic, NsbComponentMetadata};
use crate::components::{airglow, starlight};
use crate::components::zodiacal::ZodiacalExtinction;
use crate::site::SiteProfileId;
use qtty::angular::Degrees;
use qtty::photometry::SurfaceBrightness;
use qtty::radiometry::{
    PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance, S10s as S10,
};
use qtty::Second;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::{EquatorialMeanJ2000, ECEF};
use siderust::coordinates::spherical::Direction as SphericalDirection;
use tempoch::{Period, Time, UTC};

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct ComponentMask: u8 {
        const ZODIACAL  = 0b0001;
        const STARLIGHT = 0b0010;
        const AIRGLOW   = 0b0100;
        const MOON      = 0b1000;

        const DEFAULT   = Self::ZODIACAL.bits()
                        | Self::AIRGLOW.bits()
                        | Self::MOON.bits();
        const ALL       = Self::DEFAULT.bits();
        const ALL_SUPPORTED = Self::DEFAULT.bits()
                            | Self::STARLIGHT.bits();
    }
}

pub type Observer = Geodetic<ECEF>;
pub type Target = SphericalDirection<EquatorialMeanJ2000>;

#[derive(Debug, Clone)]
pub struct PointQuery {
    pub observer: Observer,
    pub time: Time<UTC>,
    pub target: Target,
    pub components: ComponentMask,
}

#[derive(Debug, Clone)]
pub struct ThresholdQuery {
    pub observer: Observer,
    pub target: Target,
    pub window: Period<UTC>,
    pub threshold: BandPhotonRadiance,
    pub components: ComponentMask,
    pub sample_step: Second,
    pub sun_altitude_ceiling: Option<Degrees>,
    pub target_altitude_floor: Option<Degrees>,
}

impl ThresholdQuery {
    pub const DEFAULT_SAMPLE_STEP: Second = Second::new(600.0);
    pub const DEFAULT_SUN_ALTITUDE_CEILING: Degrees = Degrees::new(-18.0);
    pub const DEFAULT_TARGET_ALTITUDE_FLOOR: Degrees = Degrees::new(0.0);
}

#[derive(Debug, Clone)]
pub struct NsbComponent {
    pub name: &'static str,
    pub integrated: BandPhotonRadiance,
    pub b_flux_s10: S10,
    pub v_flux_s10: S10,
    pub relative_uncertainty: Option<f64>,
    pub metadata: NsbComponentMetadata,
}

#[derive(Debug, Clone)]
pub struct NsbResult {
    pub integrated: BandPhotonRadiance,
    pub b_mag: SurfaceBrightness,
    pub v_mag: SurfaceBrightness,
    pub components: Vec<NsbComponent>,
    pub band_diagnostic: BandDiagnostic,
}

#[derive(Debug, Clone)]
pub struct ThresholdQueryResult {
    pub threshold: BandPhotonRadiance,
    pub periods: Vec<Period<UTC>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoonlightModel {
    KrisciunasSchaefer1991,
    Jones2013Spectral,
}

#[derive(Debug, Clone)]
pub enum StarlightModel {
    Disabled,
    BundledCatalogueMap,
    CustomMap(Box<starlight::StarlightMap>),
}

impl StarlightModel {
    pub fn disabled() -> Self {
        Self::Disabled
    }

    pub fn bundled_catalogue_map() -> Self {
        Self::BundledCatalogueMap
    }

    pub fn with_map(map: starlight::StarlightMap) -> Self {
        Self::CustomMap(Box::new(map))
    }
}

#[derive(Debug, Clone)]
pub struct NsbModelConfig {
    pub moonlight_model: MoonlightModel,
    pub site_profile: SiteProfileId,
    pub starlight_model: StarlightModel,
    pub solar_radio_flux: airglow::SolarFluxUnits,
    pub zodiacal_extinction: ZodiacalExtinction,
}

impl NsbModelConfig {
    pub fn generic_clear_sky() -> Self {
        Self {
            moonlight_model: MoonlightModel::Jones2013Spectral,
            site_profile: SiteProfileId::GenericClearSky,
            starlight_model: StarlightModel::Disabled,
            solar_radio_flux: airglow::DEFAULT_SOLAR_RADIO_FLUX,
            zodiacal_extinction: ZodiacalExtinction::Noll2012Approx,
        }
    }

    pub fn cta_n_planning() -> Self {
        Self::generic_clear_sky().with_site_profile(SiteProfileId::CtaNorth)
    }

    pub fn cta_s_planning() -> Self {
        Self::generic_clear_sky().with_site_profile(SiteProfileId::CtaSouth)
    }

    pub fn with_site_profile(mut self, site_profile: SiteProfileId) -> Self {
        self.site_profile = site_profile;
        self
    }

    #[doc(hidden)]
    pub fn python_parity() -> Self {
        Self {
            moonlight_model: MoonlightModel::KrisciunasSchaefer1991,
            site_profile: SiteProfileId::GenericClearSky,
            starlight_model: StarlightModel::Disabled,
            solar_radio_flux: airglow::DEFAULT_SOLAR_RADIO_FLUX,
            zodiacal_extinction: ZodiacalExtinction::Noll2012Approx,
        }
    }
}

impl Default for NsbModelConfig {
    fn default() -> Self {
        Self::generic_clear_sky()
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PreparedPointQuery {
    pub(super) observer: Observer,
    pub(super) target: Target,
    pub(super) components: ComponentMask,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PreparedThresholdQuery {
    pub(super) observer: Observer,
    pub(super) target: Target,
    pub(super) components: ComponentMask,
    pub(super) starlight_integrated: BandPhotonRadiance,
}
