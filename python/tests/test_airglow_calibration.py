"""Portable registered fit uses the same Rust calculation as every public API."""
from datetime import UTC, datetime
from pathlib import Path

import nsb
import pytest
import siderust


def test_registered_artifact_load_and_experimental_oxygen():
    root = Path(__file__).resolve().parents[2]
    raw = (root / "crates/nsb/examples/airglow-calibration/calibration.toml").read_text()
    config = nsb.NsbModelConfig.generic_clear_sky().with_solar_radio_flux_sfu(100)
    fitted = config.with_airglow_calibration_toml(raw)
    assert not fitted.is_airglow_site_calibrated
    query = nsb.PointQuery(
        siderust.Observer(lon_deg=-70.4044, lat_deg=-24.6275, height_m=2635),
        datetime(2023, 9, 4, 1, 48, tzinfo=UTC),
        siderust.Direction(ra_deg=266.41683, dec_deg=-29.00781),
        components=nsb.ComponentMask.AIRGLOW,
    )
    evaluator = nsb.NsbEvaluator(fitted)
    result = evaluator.evaluate(query)
    assert evaluator.integrate_band(query, 300, 650) == pytest.approx(
        result.integrated_photons_cm2_ns_sr, abs=1e-14
    )
    assert "registered-fit-not-independently-validated" in result.components[0].metadata.provenance
    improved = nsb.NsbEvaluator(
        fitted.with_airglow_model(nsb.AirglowModel.PARANAL_PALACE_OXYGEN_V1)
    )
    assert improved.integrate_band(query, 300, 650) > evaluator.integrate_band(query, 300, 650)
    assert improved.integrate_band(query, 400, 500) == evaluator.integrate_band(query, 400, 500)
    with pytest.raises(ValueError):
        config.with_airglow_calibration_toml(
            raw.replace("registered-fit-not-independently-validated", "independently-validated")
        )


@pytest.mark.parametrize("sfu", [float("nan"), float("inf"), 0, -1])
def test_explicit_solar_flux_rejects_invalid(sfu):
    with pytest.raises(ValueError):
        nsb.NsbModelConfig.generic_clear_sky().with_solar_radio_flux_sfu(sfu)
