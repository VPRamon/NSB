use super::metadata::{BandDiagnostic, NsbComponentMetadata};
use crate::components::zodiacal::ZodiacalExtinction;
use crate::components::{airglow, starlight};
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
use siderust::time::{Interval as TimePeriod, ModifiedJulianDate};
use tempoch::{Period, Time, UTC};

bitflags::bitflags! {
    /// Components that can be composed by [`NsbEvaluator`](super::NsbEvaluator).
    ///
    /// [`Self::ALL`] is the complete production-safe default set. It includes
    /// starlight only when a validated production map is bundled at build time.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ComponentMask: u8 {
        /// Zodiacal-light component.
        const ZODIACAL  = 0b0001;
        /// Explicitly configured integrated-starlight component.
        const STARLIGHT = 0b0010;
        /// Atmospheric airglow component.
        const AIRGLOW   = 0b0100;
        /// Atmospherically scattered moonlight component.
        const MOON      = 0b1000;

        /// Production-safe default component composition.
        #[cfg(nsb_bundled_production_starlight)]
        const DEFAULT   = Self::ZODIACAL.bits()
                        | Self::STARLIGHT.bits()
                        | Self::AIRGLOW.bits()
                        | Self::MOON.bits();
        /// Production-safe default component composition.
        #[cfg(not(nsb_bundled_production_starlight))]
        const DEFAULT   = Self::ZODIACAL.bits()
                        | Self::AIRGLOW.bits()
                        | Self::MOON.bits();
        /// Alias for the complete production-safe default composition.
        const ALL       = Self::DEFAULT.bits();
    }
}

/// Ground observer in the ECEF geodetic frame.
pub type Observer = Geodetic<ECEF>;
/// ICRS/J2000 equatorial target direction.
pub type Target = SphericalDirection<EquatorialMeanJ2000>;

#[derive(Debug, Clone)]
/// Inputs for one point evaluation.
pub struct PointQuery {
    /// Ground observer.
    pub observer: Observer,
    /// Observation instant in UTC.
    pub time: Time<UTC>,
    /// Equatorial target direction.
    pub target: Target,
    /// Components to compose.
    pub components: ComponentMask,
}

#[derive(Debug, Clone)]
/// Inputs for a below-threshold observing-window search.
pub struct ThresholdQuery {
    /// Ground observer.
    pub observer: Observer,
    /// Equatorial target direction.
    pub target: Target,
    /// Inclusive search extent in UTC.
    pub window: Period<UTC>,
    /// Maximum accepted integrated radiance.
    pub threshold: BandPhotonRadiance,
    /// Components to compose.
    pub components: ComponentMask,
    /// Coarse radiance scan step.
    pub sample_step: Second,
    /// Optional maximum Sun altitude pre-filter.
    pub sun_altitude_ceiling: Option<Degrees>,
    /// Optional minimum target altitude pre-filter.
    pub target_altitude_floor: Option<Degrees>,
}

impl ThresholdQuery {
    /// Default ten-minute coarse scan step.
    pub const DEFAULT_SAMPLE_STEP: Second = Second::new(600.0);
    /// Default astronomical-night Sun-altitude ceiling.
    pub const DEFAULT_SUN_ALTITUDE_CEILING: Degrees = Degrees::new(-18.0);
    /// Default target-above-horizon altitude floor.
    pub const DEFAULT_TARGET_ALTITUDE_FLOOR: Degrees = Degrees::new(0.0);
}

#[derive(Debug, Clone)]
/// One reported component contribution.
pub struct NsbComponent {
    /// Stable component name.
    pub name: &'static str,
    /// Integrated 300–650 nm photon radiance.
    pub integrated: BandPhotonRadiance,
    /// B-reference S10 diagnostic.
    pub b_flux_s10: S10,
    /// V-reference S10 diagnostic.
    pub v_flux_s10: S10,
    /// Relative one-sigma uncertainty when defined.
    pub relative_uncertainty: Option<f64>,
    /// Statistical one-sigma uncertainty of the integrated photon radiance.
    pub statistical_uncertainty: Option<BandPhotonRadiance>,
    /// Systematic one-sigma uncertainty of the integrated photon radiance.
    pub systematic_uncertainty: Option<BandPhotonRadiance>,
    /// Total one-sigma uncertainty of the integrated photon radiance.
    pub total_uncertainty: Option<BandPhotonRadiance>,
    /// Scientific maturity and provenance.
    pub metadata: NsbComponentMetadata,
}

/// Metadata-only description of a selected component.
#[derive(Debug, Clone)]
pub struct NsbComponentDescriptor {
    /// Stable component name.
    pub name: &'static str,
    /// Scientific maturity and provenance.
    pub metadata: NsbComponentMetadata,
}

#[derive(Debug, Clone)]
/// Complete result of one point evaluation.
pub struct NsbResult {
    /// Sum of selected integrated radiances.
    pub integrated: BandPhotonRadiance,
    /// B-reference surface-brightness diagnostic.
    pub b_mag: SurfaceBrightness,
    /// V-reference surface-brightness diagnostic.
    pub v_mag: SurfaceBrightness,
    /// Individual selected contributions.
    pub components: Vec<NsbComponent>,
    /// Interpretation of B/V fields.
    pub band_diagnostic: BandDiagnostic,
}

#[derive(Debug, Clone)]
/// Result of a below-threshold window search.
pub struct ThresholdQueryResult {
    /// Threshold used by the search.
    pub threshold: BandPhotonRadiance,
    /// UTC periods satisfying all filters and the threshold.
    pub periods: Vec<Period<UTC>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Supported scattered-moonlight implementations.
pub enum MoonlightModel {
    /// Published analytic V-band reference model.
    KrisciunasSchaefer1991,
    /// Wavelength-resolved Jones et al. (2013) model.
    Jones2013Spectral,
}

impl MoonlightModel {
    /// Stable operational model identifier.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::KrisciunasSchaefer1991 => "krisciunas-schaefer-1991",
            Self::Jones2013Spectral => "jones-2013-spectral",
        }
    }
}

#[derive(Debug, Clone)]
/// Explicit starlight data-product selection.
pub enum StarlightModel {
    /// Use the validated bundled Gaia DR3 XP-derived production map.
    BundledProductionGaiaDr3,
    /// Use a caller-supplied map for experiments without a production claim.
    ExperimentalMap(Box<starlight::StarlightMap>),
    /// Use an external map admitted through the production manifest contract.
    ValidatedExternalMap(Box<starlight::ValidatedStarlightMap>),
}

impl StarlightModel {
    /// Select the bundled production Gaia DR3 XP-derived map.
    pub fn bundled_production_gaia_dr3() -> Self {
        Self::BundledProductionGaiaDr3
    }

    /// Select a caller-provided map without a production validation claim.
    pub fn with_experimental_map(map: starlight::StarlightMap) -> Self {
        Self::ExperimentalMap(Box::new(map))
    }

    /// Select a manifest-validated external production map.
    pub fn validated_external(map: starlight::ValidatedStarlightMap) -> Self {
        Self::ValidatedExternalMap(Box::new(map))
    }
}

#[derive(Debug, Clone)]
/// Immutable model choices used to construct an evaluator.
pub struct NsbModelConfig {
    /// Scattered-moonlight implementation.
    pub moonlight_model: MoonlightModel,
    /// Atmospheric and airglow site profile.
    pub site_profile: SiteProfileId,
    /// Optional explicit starlight product.
    pub starlight_model: Option<StarlightModel>,
    /// How F10.7 is obtained for airglow (explicit, dataset, or automatic offline).
    pub solar_activity: crate::solar_activity::SolarActivitySource,
    /// Zodiacal atmospheric propagation choice.
    pub zodiacal_extinction: ZodiacalExtinction,
}

impl NsbModelConfig {
    /// Generic clear-sky planning configuration.
    pub fn generic_clear_sky() -> Self {
        Self {
            moonlight_model: MoonlightModel::Jones2013Spectral,
            site_profile: SiteProfileId::GenericClearSky,
            starlight_model: default_starlight_model(),
            solar_activity: crate::solar_activity::SolarActivitySource::Automatic,
            zodiacal_extinction: ZodiacalExtinction::Noll2012Approx,
        }
    }

    /// CTAO-North planning configuration.
    pub fn cta_n_planning() -> Self {
        Self::generic_clear_sky().with_site_profile(SiteProfileId::CtaNorth)
    }

    /// CTAO-South planning configuration.
    pub fn cta_s_planning() -> Self {
        Self::generic_clear_sky().with_site_profile(SiteProfileId::CtaSouth)
    }

    /// Replace the site profile.
    pub fn with_site_profile(mut self, site_profile: SiteProfileId) -> Self {
        self.site_profile = site_profile;
        self
    }

    /// Configure an explicit starlight product.
    pub fn with_starlight_model(mut self, starlight_model: StarlightModel) -> Self {
        self.starlight_model = Some(starlight_model);
        self
    }

    /// Set an explicit caller-owned F10.7 override (highest resolver precedence).
    pub fn with_solar_radio_flux(mut self, flux: airglow::SolarFluxUnits) -> Self {
        self.solar_activity = crate::solar_activity::SolarActivitySource::Explicit(flux);
        self
    }

    /// Alias for [`Self::with_solar_radio_flux`].
    pub fn with_f10_7(self, flux: airglow::SolarFluxUnits) -> Self {
        self.with_solar_radio_flux(flux)
    }

    /// Resolve against a pinned local F10.7 store.
    pub fn with_f107_store(
        mut self,
        store: std::sync::Arc<crate::solar_activity::F107Store>,
    ) -> Self {
        self.solar_activity = crate::solar_activity::SolarActivitySource::Dataset(store);
        self
    }

    /// Explicit F10.7 when configured; otherwise the legacy neutralizing constant.
    ///
    /// Date-aware automatic / dataset resolution uses
    /// [`crate::solar_activity::resolve_f107`] at evaluation time and is **not**
    /// represented by this accessor (it returns the neutralizing constant for
    /// those sources). Prefer component metadata `solar_activity` for the value
    /// actually applied.
    pub fn solar_radio_flux(&self) -> airglow::SolarFluxUnits {
        match &self.solar_activity {
            crate::solar_activity::SolarActivitySource::Explicit(flux) => *flux,
            crate::solar_activity::SolarActivitySource::LegacyDefault => {
                airglow::DEFAULT_SOLAR_RADIO_FLUX
            }
            crate::solar_activity::SolarActivitySource::Dataset(_)
            | crate::solar_activity::SolarActivitySource::Automatic => {
                airglow::DEFAULT_SOLAR_RADIO_FLUX
            }
        }
    }
}

impl Default for NsbModelConfig {
    fn default() -> Self {
        Self::generic_clear_sky()
    }
}

#[cfg(nsb_bundled_production_starlight)]
fn default_starlight_model() -> Option<StarlightModel> {
    Some(StarlightModel::BundledProductionGaiaDr3)
}

#[cfg(not(nsb_bundled_production_starlight))]
fn default_starlight_model() -> Option<StarlightModel> {
    None
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PreparedPointQuery {
    pub(super) observer: Observer,
    pub(super) target: Target,
    pub(super) components: ComponentMask,
}

#[derive(Debug, Clone)]
pub(super) struct PreparedThresholdQuery {
    pub(super) observer: Observer,
    pub(super) target: Target,
    pub(super) components: ComponentMask,
    pub(super) starlight_integrated: BandPhotonRadiance,
    pub(super) tt_window: TimePeriod<ModifiedJulianDate>,
    pub(super) sun_filter_periods: Vec<TimePeriod<ModifiedJulianDate>>,
    pub(super) astronomical_night_periods: Vec<airglow::temporal::AstronomicalNightPeriod>,
    pub(super) target_visible_periods: Vec<TimePeriod<ModifiedJulianDate>>,
    pub(super) candidate_windows: Vec<TimePeriod<ModifiedJulianDate>>,
    pub(super) airglow_phase_periods: Vec<airglow::temporal::AirglowPhasePeriod>,
    pub(super) moon_visible_periods: Option<Vec<TimePeriod<ModifiedJulianDate>>>,
}
