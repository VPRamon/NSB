# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
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

    # The zodiacal reference includes the ICRS -> EquatorialMeanJ2000
    # frame-bias transform and the reviewed analytic solar-reference asset.
    # The Airglow reference is the PALACE v1 runtime contract for this query.
    # Noll propagation now uses the independently tested 500 nm normalization,
    # positive species optical depths and the local-pressure molecular column.
    # The former (incorrect transport) zodiacal value was 0.07321785379305516.
    assert result.components[0].integrated_photons_cm2_ns_sr == pytest.approx(
        0.13935338075534795, abs=1.0e-12
    )
    assert result.components[1].integrated_photons_cm2_ns_sr == pytest.approx(
        0.1788348139323062, abs=1.0e-12
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
    assert metadata.airglow_resolved_model == "paranal-palace-oxygen-v1"


def test_default_components_and_embedded_starlight(observer, direction, point_time):
    result = nsb.NsbEvaluator().evaluate(nsb.PointQuery(observer, point_time, direction))
    names = {component.name for component in result.components}
    assert nsb.ComponentMask.ALL.bits == nsb.ComponentMask.DEFAULT.bits == 15
    assert names == {"starlight", "airglow", "moon", "zodiacal"}
    starlight = next(c for c in result.components if c.name == "starlight")
    assert starlight.integrated_photons_cm2_ns_sr > 0
    assert result.integrated_photons_cm2_ns_sr == pytest.approx(
        sum(c.integrated_photons_cm2_ns_sr for c in result.components), rel=1e-12
    )


def test_automatic_and_explicit_oxygen_identity_and_domain(observer, direction, point_time):
    import siderust

    config = nsb.NsbModelConfig.generic_clear_sky().with_solar_radio_flux_sfu(100)
    auto = nsb.NsbEvaluator(config)
    oxygen = nsb.NsbEvaluator(config.with_airglow_model(nsb.AirglowModel.PARANAL_PALACE_OXYGEN_V1))
    continuum = nsb.NsbEvaluator(config.with_airglow_model(nsb.AirglowModel.PARANAL_PALACE_V1))
    query = nsb.PointQuery(observer, point_time, direction, components=nsb.ComponentMask.AIRGLOW)
    assert auto.integrate_band(query, 300, 650) == oxygen.integrate_band(query, 300, 650)
    assert auto.evaluate(query).components[0].metadata.airglow_selection_kind == "automatic"
    assert oxygen.evaluate(query).components[0].metadata.airglow_selection_kind == "explicit"
    outside = siderust.Direction(ra_deg=90, dec_deg=-24.6275)
    query = nsb.PointQuery(observer, point_time, outside, components=nsb.ComponentMask.AIRGLOW)
    for evaluator in (auto, oxygen):
        with pytest.raises(nsb.OutOfRangeError, match="zenith distance <= 60"):
            evaluator.integrate_band(query, 300, 650)
        with pytest.raises(nsb.OutOfRangeError, match="zenith distance <= 60"):
            evaluator.periods_below_threshold(nsb.ThresholdQuery(
                observer, outside, point_time, point_time.replace(hour=2), 1e6,
                components=nsb.ComponentMask.AIRGLOW,
                sun_altitude_ceiling_deg=None, target_altitude_floor_deg=None))
    assert continuum.integrate_band(query, 300, 650) > 0


@pytest.mark.parametrize("sfu, classification", [
    (66.999, "below-training-interval"),
    (67., "within-training-interval"),
    (166., "within-training-interval"),
    (166.001, "above-training-interval"),
])
def test_oxygen_f107_numerical_metadata(observer, direction, point_time, sfu, classification):
    cfg = nsb.NsbModelConfig.generic_clear_sky().with_solar_radio_flux_sfu(sfu)
    query = nsb.PointQuery(observer, point_time, direction, components=nsb.ComponentMask.AIRGLOW)
    metadata = nsb.NsbEvaluator(cfg).evaluate(query).components[0].metadata
    assert metadata.oxygen_f107_applied_sfu == sfu
    assert metadata.oxygen_f107_training_range_low_sfu == 67.
    assert metadata.oxygen_f107_training_range_high_sfu == 166.
    assert metadata.oxygen_f107_numerical_range == classification
    assert "do not establish actual centred-27-day" in metadata.oxygen_f107_limitation
    assert "kind=explicit" in metadata.solar_activity_provenance
    assert metadata.status == "experimental"
    continuum = cfg.with_airglow_model(nsb.AirglowModel.PARANAL_PALACE_V1)
    omitted = nsb.NsbEvaluator(continuum).evaluate(query).components[0].metadata
    assert omitted.oxygen_f107_numerical_range is None
    assert omitted.oxygen_f107_limitation is None
