import math

import nsb
import pytest


def test_point_fixture_matches_rust_public_contract(observer, direction, evaluator, point_time):
    mask = nsb.ComponentMask.ZODIACAL | nsb.ComponentMask.AIRGLOW
    result = evaluator.evaluate(
        nsb.PointQuery(observer, point_time, direction, components=mask)
    )

    assert [component.name for component in result.components] == ["zodiacal", "airglow"]
    assert math.isfinite(result.integrated_photons_cm2_ns_sr)
    assert result.integrated_photons_cm2_ns_sr > 0.0
    assert result.integrated_photons_cm2_ns_sr == pytest.approx(
        sum(component.integrated_photons_cm2_ns_sr for component in result.components),
        rel=1e-12,
    )


def test_config_component_selection_and_automatic_airglow(observer, direction, point_time):
    config = (
        nsb.NsbModelConfig.generic_clear_sky()
        .with_site_profile(nsb.SiteProfile.CTA_SOUTH)
        .with_moonlight_model(nsb.MoonlightModel.KRISCIUNAS_SCHAEFER1991)
        .with_zodiacal_extinction(nsb.ZodiacalExtinction.NONE)
    )
    assert config.site_profile == nsb.SiteProfile.CTA_SOUTH
    assert config.moonlight_model == nsb.MoonlightModel.KRISCIUNAS_SCHAEFER1991
    assert config.airglow_selection == "automatic"

    evaluator = nsb.NsbEvaluator(config)
    result = evaluator.evaluate(
        nsb.PointQuery(
            observer,
            point_time,
            direction,
            components=nsb.ComponentMask.AIRGLOW,
        )
    )
    assert len(result.components) == 1
    metadata = result.components[0].metadata
    assert metadata.airglow_selection_kind == "automatic"
    assert metadata.airglow_resolved_model is not None
