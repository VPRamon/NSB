//! Evaluator construction and the public point-evaluation facade.

use super::metadata::{
    airglow_metadata, moonlight_metadata, starlight_metadata, zodiacal_metadata,
};
use super::point;
use super::types::*;
use crate::components::airglow::AirglowContinuum;
use crate::components::moonlight::MoonlightModel;
use crate::components::zodiacal::{self, ZodiacalLight};
use crate::components::{airglow, moonlight, starlight};
use crate::error::{NsbError, Result};
use std::sync::Arc;
use tempoch::{Time, UTC};

/// Reusable evaluator with parsed immutable component data.
pub struct NsbEvaluator {
    identity: Arc<()>,
    zodiacal: ZodiacalLight,
    airglow_continuum: Arc<AirglowContinuum>,
    airglow_resolved: airglow::ResolvedAirglowSelection,
    starlight: Option<starlight::Starlight>,
    config: NsbModelConfig,
}

impl NsbEvaluator {
    /// Construct the generic production-safe planning configuration.
    pub fn new() -> Result<Self> {
        Self::with_config(NsbModelConfig::generic_clear_sky())
    }

    /// Construct from explicit immutable model choices.
    pub fn with_config(config: NsbModelConfig) -> Result<Self> {
        let zodiacal = match config.zodiacal_model() {
            zodiacal::ZodiacalModel::Leinert1998 => ZodiacalLight::leinert1998()?,
        }
        .with_extinction(config.zodiacal_extinction());
        let airglow_resolved = airglow::resolve_airglow_selection(config.airglow_selection())?;
        let airglow_continuum = airglow::load_continuum_for_model(airglow_resolved.model)?;
        let starlight = match config.starlight_product() {
            None => None,
            Some(starlight::StarlightProduct::BundledProductionGaiaDr3) => {
                Some(starlight::Starlight::bundled_production_model()?)
            }
            Some(starlight::StarlightProduct::ExperimentalMap(map)) => {
                Some(starlight::Starlight::with_shared_map(Arc::clone(map)))
            }
            Some(starlight::StarlightProduct::ValidatedExternalMap(map)) => {
                Some(starlight::Starlight::with_shared_map(map.shared_map()))
            }
        };
        Ok(Self {
            identity: Arc::new(()),
            zodiacal,
            airglow_continuum,
            airglow_resolved,
            starlight,
            config,
        })
    }

    /// Borrow the evaluator configuration.
    pub fn config(&self) -> &NsbModelConfig {
        &self.config
    }

    /// Describe selected components without performing a time-dependent
    /// radiance evaluation.
    pub fn describe_components(
        &self,
        observer: Observer,
        components: ComponentMask,
    ) -> Result<Vec<NsbComponentDescriptor>> {
        let mut descriptions = Vec::new();
        if components.contains(ComponentMask::ZODIACAL) {
            descriptions.push(NsbComponentDescriptor {
                name: "zodiacal",
                metadata: zodiacal_metadata(
                    self.config.zodiacal_model(),
                    self.config.zodiacal_extinction(),
                ),
            });
        }
        if components.contains(ComponentMask::STARLIGHT) {
            if self.starlight.is_none() {
                return Err(NsbError::Unsupported(
                    "starlight component requested but no starlight product is configured".into(),
                ));
            }
            descriptions.push(NsbComponentDescriptor {
                name: "starlight",
                metadata: starlight_metadata(
                    self.config.starlight_product(),
                    self.starlight
                        .as_ref()
                        .map(|model| model.map().provenance()),
                ),
            });
        }
        if components.contains(ComponentMask::AIRGLOW) {
            descriptions.push(NsbComponentDescriptor {
                name: "airglow",
                metadata: airglow_metadata(
                    self.airglow_resolved,
                    self.config.site_profile(),
                    observer,
                    None,
                    self.config.airglow_geometry(),
                    airglow::AirglowPhysicalOutcome::Evaluated,
                    None,
                ),
            });
        }
        if components.contains(ComponentMask::MOON) {
            descriptions.push(NsbComponentDescriptor {
                name: "moon",
                metadata: moonlight_metadata(
                    self.config.moonlight_model(),
                    self.config.site_profile(),
                    observer,
                ),
            });
        }
        Ok(descriptions)
    }

    /// Evaluate selected components for one point query.
    pub fn evaluate(&self, query: &PointQuery) -> Result<NsbResult> {
        let prepared = PreparedPointQuery {
            observer: query.observer,
            target: query.target,
            components: query.components,
        };
        point::evaluate(self, &prepared, query.time)
    }

    pub(crate) fn identity(&self) -> &Arc<()> {
        &self.identity
    }

    pub(crate) fn zodiacal(&self) -> &ZodiacalLight {
        &self.zodiacal
    }

    pub(crate) fn airglow_continuum(&self) -> &Arc<AirglowContinuum> {
        &self.airglow_continuum
    }

    pub(crate) fn airglow_resolved(&self) -> airglow::ResolvedAirglowSelection {
        self.airglow_resolved
    }

    pub(crate) fn starlight(&self) -> Option<&starlight::Starlight> {
        self.starlight.as_ref()
    }

    pub(crate) fn model_config(&self) -> &NsbModelConfig {
        &self.config
    }

    pub(crate) fn evaluate_airglow_resolved(
        &self,
        observer: Observer,
        time: Time<UTC>,
        target: Target,
    ) -> Result<(
        airglow::AirglowOutputs,
        crate::solar_activity::ResolvedSolarActivity,
    )> {
        let solar = crate::solar_activity::resolve_f107(time, self.config.solar_activity())?;
        let profile = self.config.site_profile().profile(observer);
        let outputs =
            airglow::Airglow::with_shared_continuum(observer, Arc::clone(&self.airglow_continuum))
                .with_atmosphere(profile.atmosphere)
                .with_geometry(self.config.airglow_geometry().clone())
                .with_solar_radio_flux(solar.value)
                .with_scale(profile.airglow.scale)
                .compute(time, target)?;
        Ok((outputs, solar))
    }

    pub(crate) fn evaluate_starlight(&self, target: Target) -> Result<starlight::StarlightOutputs> {
        let model = self.starlight.as_ref().ok_or_else(|| {
            NsbError::Unsupported(
                concat!(
                    "starlight component requested but no starlight product is configured; ",
                    "provide a validated map with StarlightProduct::validated_external(...), ",
                    "use StarlightProduct::bundled_production_gaia_dr3(), or ",
                    "explicitly opt into StarlightProduct::with_experimental_map(...)"
                )
                .to_string(),
            )
        })?;
        model.compute(target)
    }

    pub(crate) fn evaluate_moonlight(
        &self,
        observer: Observer,
        time: Time<UTC>,
        target: Target,
    ) -> Result<moonlight::MoonOutputs> {
        match self.config.moonlight_model() {
            MoonlightModel::KrisciunasSchaefer1991 => {
                moonlight::KrisciunasSchaefer1991::published_reference(observer)
                    .compute(time, target)
            }
            MoonlightModel::Jones2013Spectral => {
                moonlight::Jones2013Spectral::for_site_profile(observer, self.config.site_profile())
                    .compute(time, target)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::NsbError;
    use siderust::catalogs::observatories;
    use siderust::coordinates::centers::Geodetic;
    use siderust::coordinates::frames::ECEF;

    fn paranal() -> Geodetic<ECEF> {
        observatories::EL_PARANAL.geodetic()
    }

    #[test]
    fn describe_components_rejects_starlight_without_configured_product() {
        let evaluator = NsbEvaluator::with_config(
            NsbModelConfig::generic_clear_sky().without_starlight_product(),
        )
        .unwrap();

        assert!(matches!(
            evaluator.describe_components(paranal(), ComponentMask::STARLIGHT),
            Err(NsbError::Unsupported(message))
                if message == "starlight component requested but no starlight product is configured"
        ));
    }

    #[test]
    fn evaluate_rejects_starlight_without_configured_product() {
        use chrono::{DateTime, Utc};
        use tempoch::{Time, UTC};

        let evaluator = NsbEvaluator::with_config(
            NsbModelConfig::generic_clear_sky().without_starlight_product(),
        )
        .unwrap();
        let time = Time::<UTC>::from_chrono(
            DateTime::parse_from_rfc3339("2023-09-04T01:48:00Z")
                .unwrap()
                .with_timezone(&Utc),
        );
        let target = Target::new(266.41683 * crate::DEG, -29.00781 * crate::DEG);
        let err = evaluator
            .evaluate(
                &PointQuery::new(paranal(), time, target).with_components(ComponentMask::STARLIGHT),
            )
            .unwrap_err();
        assert!(matches!(err, NsbError::Unsupported(_)));
    }

    #[test]
    fn config_returns_borrow_not_owned_clone() {
        let evaluator = NsbEvaluator::new().unwrap();
        let borrowed: &NsbModelConfig = evaluator.config();
        assert_eq!(
            borrowed.airglow_selection(),
            airglow::AirglowSelection::Automatic
        );
    }
}
