//! Point NSB evaluation and component composition.

use super::metadata::{
    airglow_metadata, moonlight_metadata, starlight_metadata, zodiacal_metadata,
};
use super::types::{ComponentMask, NsbComponent, NsbResult, PreparedPointQuery};
use super::NsbEvaluator;
use crate::error::Result;
use crate::NSB_S10_ZP;
use qtty::photometry::s10_to_surface_brightness;
use qtty::radiometry::{
    PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance, S10s as S10,
};
use tempoch::{Time, UTC};

pub(crate) fn evaluate(
    evaluator: &NsbEvaluator,
    query: &PreparedPointQuery,
    time: Time<UTC>,
) -> Result<NsbResult> {
    let mut components = Vec::new();
    let mut total = BandPhotonRadiance::new(0.0);
    let (mut b_total, mut v_total) = (S10::new(0.0), S10::new(0.0));

    if query.components.contains(ComponentMask::ZODIACAL) {
        let out = evaluator
            .zodiacal()
            .compute(time, query.observer, query.target)?;
        total += out.integrated;
        b_total += out.b_flux_s10;
        v_total += out.v_flux_s10;
        components.push(NsbComponent {
            name: "zodiacal",
            integrated: out.integrated,
            b_flux_s10: out.b_flux_s10,
            v_flux_s10: out.v_flux_s10,
            relative_uncertainty: None,
            statistical_uncertainty: None,
            systematic_uncertainty: None,
            total_uncertainty: None,
            metadata: zodiacal_metadata(),
        });
    }
    if query.components.contains(ComponentMask::STARLIGHT) {
        let out = evaluator.evaluate_starlight(query.target)?;
        total += out.integrated;
        b_total += out.b_flux_s10;
        v_total += out.v_flux_s10;
        components.push(NsbComponent {
            name: "starlight",
            integrated: out.integrated,
            b_flux_s10: out.b_flux_s10,
            v_flux_s10: out.v_flux_s10,
            relative_uncertainty: out.relative_uncertainty(),
            statistical_uncertainty: out.statistical_uncertainty,
            systematic_uncertainty: out.systematic_uncertainty,
            total_uncertainty: out.total_uncertainty,
            metadata: starlight_metadata(
                evaluator.model_config().starlight_product.as_ref(),
                evaluator.starlight().map(|model| model.map().provenance()),
            ),
        });
    }
    if query.components.contains(ComponentMask::AIRGLOW) {
        let (out, solar) =
            evaluator.evaluate_airglow_resolved(query.observer, time, query.target)?;
        total += out.integrated;
        b_total += out.b_flux_s10;
        v_total += out.v_flux_s10;
        components.push(NsbComponent {
            name: "airglow",
            integrated: out.integrated,
            b_flux_s10: out.b_flux_s10,
            v_flux_s10: out.v_flux_s10,
            relative_uncertainty: out.relative_uncertainty,
            statistical_uncertainty: None,
            systematic_uncertainty: None,
            total_uncertainty: None,
            metadata: airglow_metadata(
                evaluator.model_config().airglow_model,
                evaluator.model_config().site_profile,
                query.observer,
                Some(&solar),
                &evaluator.model_config().airglow_geometry,
            ),
        });
    }
    if query.components.contains(ComponentMask::MOON) {
        let out = evaluator.evaluate_moonlight(query.observer, time, query.target)?;
        total += out.integrated;
        b_total += out.b_flux_s10;
        v_total += out.v_flux_s10;
        components.push(NsbComponent {
            name: "moon",
            integrated: out.integrated,
            b_flux_s10: out.b_flux_s10,
            v_flux_s10: out.v_flux_s10,
            relative_uncertainty: None,
            statistical_uncertainty: None,
            systematic_uncertainty: None,
            total_uncertainty: None,
            metadata: moonlight_metadata(
                evaluator.model_config().moonlight_model,
                evaluator.model_config().site_profile,
                query.observer,
            ),
        });
    }

    Ok(NsbResult {
        integrated: total,
        b_mag: s10_to_surface_brightness(
            b_total.max(S10::new(f64::MIN_POSITIVE)),
            NSB_S10_ZP.value(),
        ),
        v_mag: s10_to_surface_brightness(
            v_total.max(S10::new(f64::MIN_POSITIVE)),
            NSB_S10_ZP.value(),
        ),
        components,
        band_diagnostic: super::BandDiagnostic::MONOCHROMATIC_S10_PROXY,
    })
}
