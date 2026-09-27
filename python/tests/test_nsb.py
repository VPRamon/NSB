from __future__ import annotations

import math
import sys
import threading
from datetime import datetime, timedelta, timezone

import pytest

import nsb


def observer() -> nsb.Observer:
    return nsb.Observer(
        latitude_deg=-24.683427777777776,
        longitude_deg=-70.31634444444444,
        elevation_m=2184.6,
    )


def target() -> nsb.Target:
    return nsb.Target(ra_deg=266.41683, dec_deg=-29.00781)


def components() -> nsb.ComponentMask:
    return nsb.ComponentMask.ZODIACAL | nsb.ComponentMask.AIRGLOW


def test_smoke_and_versions() -> None:
    assert nsb.__version__
    assert nsb.MODEL_VERSION.startswith("nsb-model-")
    assert nsb.SIDERUST_VERSION == "0.11.1"
    assert nsb.ComponentMask.ALL.bits == nsb.ComponentMask.DEFAULT.bits


def test_coordinates_are_explicit_and_validated() -> None:
    site = observer()
    source = target()
    assert site.latitude_deg == pytest.approx(-24.683427777777776)
    assert site.longitude_deg == pytest.approx(-70.31634444444444)
    assert site.elevation_m == pytest.approx(2184.6)
    assert source.ra_deg == pytest.approx(266.41683)
    assert source.dec_deg == pytest.approx(-29.00781)
    assert source.reference_frame == "equatorial-mean-j2000"

    with pytest.raises(nsb.OutOfRangeError):
        nsb.Observer(latitude_deg=91.0, longitude_deg=0.0, elevation_m=0.0)
    with pytest.raises(nsb.OutOfRangeError):
        nsb.Target(ra_deg=float("nan"), dec_deg=0.0)


def test_model_configuration_tracks_rust_policy() -> None:
    default = nsb.NsbModelConfig.generic_clear_sky()
    assert default.site_profile == nsb.SiteProfile.GENERIC_CLEAR_SKY
    assert default.moonlight_model == nsb.MoonlightModel.JONES2013_SPECTRAL
    assert default.airglow_selection == "automatic"
    assert default.airglow_model is None
    assert default.zodiacal_model == nsb.ZodiacalModel.LEINERT1998
    assert default.zodiacal_extinction == nsb.ZodiacalExtinction.NOLL2012_APPROX

    explicit = default.with_airglow_model(
        nsb.AirglowModel.PARANAL_NOLL_SKY_CALC_FORS1
    )
    assert explicit.airglow_selection == "explicit"
    assert explicit.airglow_model == nsb.AirglowModel.PARANAL_NOLL_SKY_CALC_FORS1


def test_point_evaluation_delegates_to_rust_result() -> None:
    evaluator = nsb.NsbEvaluator()
    query = nsb.PointQuery(
        observer(),
        datetime(2023, 9, 4, 1, 48, tzinfo=timezone.utc),
        target(),
        components=components(),
    )
    result = evaluator.evaluate(query)

    assert result.integrated_photons_cm2_ns_sr > 0.0
    assert math.isfinite(result.integrated_photons_cm2_ns_sr)
    assert [component.name for component in result.components] == ["zodiacal", "airglow"]
    component_sum = sum(
        component.integrated_photons_cm2_ns_sr for component in result.components
    )
    assert result.integrated_photons_cm2_ns_sr == pytest.approx(
        component_sum, rel=1e-12, abs=1e-12
    )
    assert result.band_diagnostic.b_reference_nm == pytest.approx(445.0)
    airglow = result.components[1]
    assert airglow.metadata.airglow_selection_kind == "automatic"
    assert airglow.metadata.airglow_used_automatic_fallback is True
    assert airglow.metadata.airglow_resolved_model == "paranal-noll-skycalc-fors1"


def test_datetime_requires_tzinfo_and_normalizes_offsets() -> None:
    with pytest.raises(nsb.OutOfRangeError):
        nsb.PointQuery(
            observer(),
            datetime(2023, 9, 4, 1, 48),
            target(),
            components=components(),
        )

    local = timezone(timedelta(hours=2))
    query = nsb.PointQuery(
        observer(),
        datetime(2023, 9, 4, 3, 48, tzinfo=local),
        target(),
        components=components(),
    )
    assert query.time == datetime(2023, 9, 4, 1, 48, tzinfo=timezone.utc)


def test_threshold_search_and_prepared_context_reuse() -> None:
    evaluator = nsb.NsbEvaluator()
    query = (
        nsb.ThresholdQuery(
            observer(),
            target(),
            datetime(2023, 9, 4, 1, 0, tzinfo=timezone.utc),
            datetime(2023, 9, 4, 2, 0, tzinfo=timezone.utc),
            1.0e6,
            components=components(),
            sample_step_s=600.0,
        )
        .with_sun_altitude_ceiling_deg(None)
        .with_target_altitude_floor_deg(None)
    )

    direct = evaluator.periods_below_threshold(query)
    context = evaluator.prepare_site_window_context(query)
    reused = evaluator.periods_below_threshold_with_context(context, query)

    assert direct.threshold_photons_cm2_ns_sr == pytest.approx(1.0e6)
    assert len(direct.periods) == 1
    assert len(reused.periods) == 1
    assert direct.periods[0].start == datetime(2023, 9, 4, 1, 0, tzinfo=timezone.utc)
    assert direct.periods[0].end == datetime(2023, 9, 4, 2, 0, tzinfo=timezone.utc)
    assert reused.periods[0].start == direct.periods[0].start
    assert reused.periods[0].end == direct.periods[0].end


def test_threshold_input_errors_are_typed() -> None:
    with pytest.raises(nsb.OutOfRangeError):
        nsb.ThresholdQuery(
            observer(),
            target(),
            datetime(2023, 9, 4, 2, 0, tzinfo=timezone.utc),
            datetime(2023, 9, 4, 1, 0, tzinfo=timezone.utc),
            1.0,
        )

    with pytest.raises(nsb.OutOfRangeError):
        nsb.ThresholdQuery(
            observer(),
            target(),
            datetime(2023, 9, 4, 1, 0, tzinfo=timezone.utc),
            datetime(2023, 9, 4, 2, 0, tzinfo=timezone.utc),
            -1.0,
        )


def test_planning_detaches_from_python_interpreter() -> None:
    evaluator = nsb.NsbEvaluator()
    query = (
        nsb.ThresholdQuery(
            observer(),
            target(),
            datetime(2023, 9, 4, 0, 0, tzinfo=timezone.utc),
            datetime(2023, 9, 5, 0, 0, tzinfo=timezone.utc),
            1.0e6,
            components=components(),
            sample_step_s=600.0,
        )
        .with_sun_altitude_ceiling_deg(None)
        .with_target_altitude_floor_deg(None)
    )

    ready = threading.Event()
    go = threading.Event()
    stop = threading.Event()
    counter = [0]

    def contender() -> None:
        ready.set()
        go.wait()
        while not stop.is_set():
            counter[0] += 1

    thread = threading.Thread(target=contender)
    thread.start()
    ready.wait()

    previous_interval = sys.getswitchinterval()
    try:
        # Disable ordinary bytecode scheduling across the native call. The
        # contender can then advance only while PyO3 has detached Python.
        sys.setswitchinterval(60.0)
        before = counter[0]
        go.set()
        evaluator.periods_below_threshold(query)
        after = counter[0]
    finally:
        stop.set()
        sys.setswitchinterval(previous_interval)
        thread.join(timeout=5.0)

    assert after > before
