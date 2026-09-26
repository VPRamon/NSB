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
    /// # Compatibility policy (frozen first-release contract)
    ///
    /// [`Self::DEFAULT`] is the **frozen first-release default composition**.
    /// [`Self::ALL`] is an alias of that same frozen set — **not** “every
    /// component ever implemented by this crate”.
    ///
    /// After the public API is frozen:
    ///
    /// - newly introduced physical components (for example Twilight) are
    ///   **opt-in** and must not be added silently to `DEFAULT` / `ALL` within
    ///   the same compatibility line merely because the Rust bitflag type can
    ///   accept a new bit;
    /// - intentional default-composition changes require an explicit model-
    ///   contract / `MODEL_VERSION` change, release notes, and regression
    ///   updates;
    /// - build-dependent Starlight inclusion (when a validated production map
    ///   is bundled) is part of this frozen default policy, not a precedent for
    ///   arbitrary future default changes.
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

        /// Frozen first-release default component composition.
        #[cfg(nsb_bundled_production_starlight)]
        const DEFAULT   = Self::ZODIACAL.bits()
                        | Self::STARLIGHT.bits()
                        | Self::AIRGLOW.bits()
                        | Self::MOON.bits();
        /// Frozen first-release default component composition.
        #[cfg(not(nsb_bundled_production_starlight))]
        const DEFAULT   = Self::ZODIACAL.bits()
                        | Self::AIRGLOW.bits()
                        | Self::MOON.bits();
        /// Alias of [`Self::DEFAULT`] — the frozen production-default set, not
        /// “every component implemented by the crate”.
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
/// Prefer [`Self::generic_clear_sky`] and the `with_*` builders. Fields are
/// private so the configuration shape can evolve without freezing struct layout.
/// Inspect supported choices through getters.
#[non_exhaustive]
pub struct NsbModelConfig {
    moonlight_model: moonlight::MoonlightModel,
    airglow_selection: airglow::AirglowSelection,
    zodiacal_model: zodiacal::ZodiacalModel,
    site_profile: SiteProfileId,
    starlight_product: Option<starlight::StarlightProduct>,
    solar_activity: crate::solar_activity::SolarActivitySource,
    airglow_geometry: airglow::AirglowGeometryModel,
    zodiacal_extinction: zodiacal::ZodiacalExtinction,
}

impl NsbModelConfig {
    /// Generic clear-sky planning configuration.
    ///
    /// Airglow uses [`airglow::AirglowSelection::Automatic`]. Until the global
    /// climatological model is admitted (#157), automatic policy resolves to the
    /// temporary Paranal-derived planning fallback with that fallback visible in
    /// result metadata. This does **not** freeze Paranal as the intrinsic
    /// generic global scientific contract.
    pub fn generic_clear_sky() -> Self {
        Self {
            moonlight_model: moonlight::MoonlightModel::Jones2013Spectral,
            airglow_selection: airglow::AirglowSelection::Automatic,
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

    /// Select an explicit Airglow scientific model.
    ///
    /// Explicit selection wins over automatic policy and never silently falls
    /// back to another model. Unsupported explicit models fail at evaluator
    /// construction / evaluation rather than substituting a different model.
    pub fn with_airglow_model(mut self, model: airglow::AirglowModel) -> Self {
        self.airglow_selection = airglow::AirglowSelection::Explicit(model);
        self
    }

    /// Replace the full Airglow selection policy (automatic or explicit).
    pub fn with_airglow_selection(mut self, selection: airglow::AirglowSelection) -> Self {
        self.airglow_selection = selection;
        self
    }

    /// Return the configured Airglow selection policy.
    pub const fn airglow_selection(&self) -> airglow::AirglowSelection {
        self.airglow_selection
    }

    /// Return the explicitly requested Airglow model, if selection is explicit.
    pub const fn airglow_model(&self) -> Option<airglow::AirglowModel> {
        self.airglow_selection.requested_model()
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

    /// Return the selected site profile identifier.
    pub const fn site_profile(&self) -> SiteProfileId {
        self.site_profile
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

    /// Remove any configured Starlight product.
    ///
    /// Crate-internal helper for tests that need an evaluator without starlight.
    #[cfg(test)]
    pub(crate) fn without_starlight_product(mut self) -> Self {
        self.starlight_product = None;
        self
    }

    /// Set an explicit caller-owned F10.7 override (highest resolver precedence).
    pub fn with_solar_radio_flux(mut self, flux: crate::units::SolarFluxUnits) -> Self {
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

    /// Return the configured solar-activity / F10.7 source.
    pub fn solar_activity(&self) -> &crate::solar_activity::SolarActivitySource {
        &self.solar_activity
    }

    /// Select Airglow emitting-volume LOS geometry without changing extinction.
    pub fn with_airglow_geometry(mut self, geometry: airglow::AirglowGeometryModel) -> Self {
        self.airglow_geometry = geometry;
        self
    }

    /// Return the selected Airglow emitting-volume geometry model.
    pub fn airglow_geometry(&self) -> &airglow::AirglowGeometryModel {
        &self.airglow_geometry
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
