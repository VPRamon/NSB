use super::metadata::{BandDiagnostic, NsbComponentMetadata};
use crate::components::zodiacal;
use crate::components::{airglow, moonlight, starlight};
use crate::site::{CalibrationStatus, SiteProfileId};
use qtty::photometry::SurfaceBrightness;
use qtty::radiometry::{
    PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance, S10s as S10,
};
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::{EquatorialMeanJ2000, ECEF};
use siderust::coordinates::spherical::Direction as SphericalDirection;
use tempoch::{Time, UTC};

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
///
/// Construct with [`Self::new`] and [`Self::with_components`]. Fields remain
/// readable for diagnostics; the struct is `#[non_exhaustive]` so new inputs can
/// be added without breaking external struct literals.
#[non_exhaustive]
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

impl PointQuery {
    /// Point evaluation of the production-safe default component set.
    pub fn new(observer: Observer, time: Time<UTC>, target: Target) -> Self {
        Self {
            observer,
            time,
            target,
            components: ComponentMask::ALL,
        }
    }

    /// Restrict or expand the composed contributors.
    pub fn with_components(mut self, components: ComponentMask) -> Self {
        self.components = components;
        self
    }
}

#[derive(Debug, Clone)]
/// One reported component contribution.
#[non_exhaustive]
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
#[non_exhaustive]
pub struct NsbComponentDescriptor {
    /// Stable component name.
    pub name: &'static str,
    /// Scientific maturity and provenance.
    pub metadata: NsbComponentMetadata,
}

#[derive(Debug, Clone)]
/// Complete result of one point evaluation.
#[non_exhaustive]
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
/// Immutable model choices used to construct an evaluator.
///
/// Prefer [`Self::generic_clear_sky`] and the `with_*` builders. The struct is
/// `#[non_exhaustive]` so new model choices can be added without breaking
/// external struct literals. Public fields remain assignable for existing
/// builder-style configuration.
#[non_exhaustive]
pub struct NsbModelConfig {
    /// Scattered-moonlight implementation.
    pub moonlight_model: moonlight::MoonlightModel,
    /// Airglow scientific model/parameterization.
    pub airglow_model: airglow::AirglowModel,
    /// Zodiacal-light scientific source model.
    pub zodiacal_model: zodiacal::ZodiacalModel,
    /// Atmospheric and airglow site profile.
    pub site_profile: SiteProfileId,
    /// Optional explicit starlight product.
    pub starlight_product: Option<starlight::StarlightProduct>,
    /// How F10.7 is obtained for airglow (explicit, dataset, or automatic offline).
    pub solar_activity: crate::solar_activity::SolarActivitySource,
    /// Airglow emitting-volume line-of-sight geometry (separate from extinction).
    pub airglow_geometry: airglow::AirglowGeometryModel,
    /// Zodiacal atmospheric propagation choice.
    pub zodiacal_extinction: zodiacal::ZodiacalExtinction,
}

impl NsbModelConfig {
    /// Generic clear-sky planning configuration.
    pub fn generic_clear_sky() -> Self {
        Self {
            moonlight_model: moonlight::MoonlightModel::Jones2013Spectral,
            airglow_model: airglow::AirglowModel::ParanalNollSkyCalcFors1,
            zodiacal_model: zodiacal::ZodiacalModel::Leinert1998,
            site_profile: SiteProfileId::GenericClearSky,
            starlight_product: default_starlight_product(),
            solar_activity: crate::solar_activity::SolarActivitySource::Automatic,
            airglow_geometry: airglow::AirglowGeometryModel::default(),
            zodiacal_extinction: zodiacal::ZodiacalExtinction::Noll2012Approx,
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

    /// Select the Moonlight scientific model independently of site assumptions.
    pub fn with_moonlight_model(mut self, model: moonlight::MoonlightModel) -> Self {
        self.moonlight_model = model;
        self
    }

    /// Return the selected Moonlight scientific model.
    pub const fn moonlight_model(&self) -> moonlight::MoonlightModel {
        self.moonlight_model
    }

    /// Select the Airglow scientific model independently of geometry and site maturity.
    pub fn with_airglow_model(mut self, model: airglow::AirglowModel) -> Self {
        self.airglow_model = model;
        self
    }

    /// Return the selected Airglow scientific model.
    pub const fn airglow_model(&self) -> airglow::AirglowModel {
        self.airglow_model
    }

    /// Select the Zodiacal-light scientific source model independently of propagation.
    pub fn with_zodiacal_model(mut self, model: zodiacal::ZodiacalModel) -> Self {
        self.zodiacal_model = model;
        self
    }

    /// Return the selected Zodiacal-light scientific source model.
    pub const fn zodiacal_model(&self) -> zodiacal::ZodiacalModel {
        self.zodiacal_model
    }

    /// Select Zodiacal atmospheric propagation independently of the source model.
    pub fn with_zodiacal_extinction(mut self, extinction: zodiacal::ZodiacalExtinction) -> Self {
        self.zodiacal_extinction = extinction;
        self
    }

    /// Return the selected Zodiacal atmospheric propagation.
    pub const fn zodiacal_extinction(&self) -> zodiacal::ZodiacalExtinction {
        self.zodiacal_extinction
    }

    /// Replace the site profile.
    pub fn with_site_profile(mut self, site_profile: SiteProfileId) -> Self {
        self.site_profile = site_profile;
        self
    }

    /// Return the evidence-backed Airglow calibration maturity.
    ///
    /// Observer coordinates, geometry, and solar-activity inputs do not change
    /// the scientific maturity selected by the site profile.
    pub const fn airglow_calibration_status(&self) -> CalibrationStatus {
        self.site_profile.calibration_status()
    }

    /// Return true only when the selected Airglow site profile is calibrated.
    pub const fn is_airglow_site_calibrated(&self) -> bool {
        self.site_profile.is_site_calibrated()
    }

    /// Configure an explicit Starlight data product.
    pub fn with_starlight_product(
        mut self,
        starlight_product: starlight::StarlightProduct,
    ) -> Self {
        self.starlight_product = Some(starlight_product);
        self
    }

    /// Return the configured Starlight data product, if any.
    pub fn starlight_product(&self) -> Option<&starlight::StarlightProduct> {
        self.starlight_product.as_ref()
    }

    /// Set an explicit caller-owned F10.7 override (highest resolver precedence).
    pub fn with_solar_radio_flux(mut self, flux: crate::SolarFluxUnits) -> Self {
        self.solar_activity = crate::solar_activity::SolarActivitySource::Explicit(flux);
        self
    }

    /// Resolve against a pinned local F10.7 store.
    pub fn with_f107_store(
        mut self,
        store: std::sync::Arc<crate::solar_activity::F107Store>,
    ) -> Self {
        self.solar_activity = crate::solar_activity::SolarActivitySource::Dataset(store);
        self
    }

    /// Select Airglow emitting-volume LOS geometry without changing extinction.
    pub fn with_airglow_geometry(mut self, geometry: airglow::AirglowGeometryModel) -> Self {
        self.airglow_geometry = geometry;
        self
    }
}

impl Default for NsbModelConfig {
    fn default() -> Self {
        Self::generic_clear_sky()
    }
}

#[cfg(nsb_bundled_production_starlight)]
fn default_starlight_product() -> Option<starlight::StarlightProduct> {
    Some(starlight::StarlightProduct::BundledProductionGaiaDr3)
}

#[cfg(not(nsb_bundled_production_starlight))]
fn default_starlight_product() -> Option<starlight::StarlightProduct> {
    None
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PreparedPointQuery {
    pub(crate) observer: Observer,
    pub(crate) target: Target,
    pub(crate) components: ComponentMask,
}
