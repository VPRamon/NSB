"""Executable physical/numerical checks against an installed NSB Python package."""

from __future__ import annotations

from datetime import UTC, datetime
from typing import Any

import nsb
import siderust

from .rayleigh import PHOTONS_CM2_NS_SR_PER_RAYLEIGH, photons_per_ns_sr_from_rayleigh

def near_zenith_direction(observer: siderust.Observer, instant: datetime) -> siderust.Direction:
    """Pick an ICRS Direction near local zenith so Oxygen (ZD≤60°) can evaluate.

    Dec matches the site latitude; RA is chosen by probing the Oxygen domain at
    the evaluation instant (no silent continuum fallback).
    """
    lat = float(observer.lat_deg)
    cfg = (
        nsb.NsbModelConfig.generic_clear_sky()
        .with_airglow_model(nsb.AirglowModel.PARANAL_PALACE_OXYGEN_V1)
        .with_solar_radio_flux_sfu(100.0)
    )
    evaluator = nsb.NsbEvaluator(cfg)
    for ra in range(0, 360, 5):
        target = siderust.Direction(ra_deg=float(ra), dec_deg=lat)
        query = nsb.PointQuery(observer, instant, target, components=nsb.ComponentMask.AIRGLOW)
        try:
            evaluator.integrate_band(query, 300.0, 650.0)
            return target
        except Exception:
            continue
    raise RuntimeError("no ICRS RA found within Oxygen zenith-distance domain for this site/time")


# Public CTAO geographic parameters (approximate published locations).
CTA_SITES = {
    "CTA_SOUTH": {"lon_deg": -70.404, "lat_deg": -24.627, "height_m": 2150.0},
    "CTA_NORTH": {"lon_deg": -17.890, "lat_deg": 28.762, "height_m": 2200.0},
}


def run_physical_checks(*, f107_sfu: float = 130.0) -> dict[str, Any]:
    """Run live NSB checks. Results are machine-readable; FAIL marks do not raise."""
    checks: list[dict[str, Any]] = []
    models = {
        "continuum": nsb.AirglowModel.PARANAL_PALACE_V1,
        "oxygen": nsb.AirglowModel.PARANAL_PALACE_OXYGEN_V1,
    }
    checks.append(
        {
            "id": "airglow_models_present",
            "status": "PASS",
            "detail": {k: v.as_str() for k, v in models.items()},
        }
    )

    r = photons_per_ns_sr_from_rayleigh(1.0)
    checks.append(
        {
            "id": "rayleigh_convention",
            "status": "PASS" if abs(r - PHOTONS_CM2_NS_SR_PER_RAYLEIGH) < 1e-18 else "FAIL",
            "photons_cm2_ns_sr_per_R": r,
            "formula": "1 R = 1e-3/(4π) photons cm^-2 ns^-1 sr^-1",
        }
    )

    night = datetime(2024, 1, 15, 2, 0, tzinfo=UTC)
    results: dict[str, Any] = {}
    for site_name, coords in CTA_SITES.items():
        profile = getattr(nsb.SiteProfile, site_name)
        observer = siderust.Observer(**coords)
        target = near_zenith_direction(observer, night)
        query = nsb.PointQuery(
            observer, night, target, components=nsb.ComponentMask.AIRGLOW
        )
        site_results: dict[str, Any] = {}
        for label, model in models.items():
            cfg = (
                nsb.NsbModelConfig.generic_clear_sky()
                .with_site_profile(profile)
                .with_airglow_model(model)
                .with_solar_radio_flux_sfu(f107_sfu)
            )
            evaluator = nsb.NsbEvaluator(cfg)
            band_300_650 = float(evaluator.integrate_band(query, 300.0, 650.0))
            band_green = float(evaluator.integrate_band(query, 552.0, 563.0))
            band_red = float(evaluator.integrate_band(query, 625.0, 642.0))
            site_results[label] = {
                "airglow_300_650_nm": band_300_650,
                "airglow_552_563_nm": band_green,
                "airglow_625_642_nm": band_red,
                "unit": "photons cm^-2 ns^-1 sr^-1",
            }
        c = site_results["continuum"]["airglow_300_650_nm"]
        o = site_results["oxygen"]["airglow_300_650_nm"]
        delta = o - c
        site_results["oxygen_minus_continuum_300_650"] = delta
        site_results["relative_oxygen_increment"] = delta / c if c else None
        results[site_name] = site_results
        checks.append(
            {
                "id": f"oxygen_increases_airglow_{site_name}",
                "status": "PASS" if delta > 0 else "FAIL",
                "continuum": c,
                "oxygen": o,
                "delta": delta,
            }
        )
        # Component summation check (airglow-only band equals integrate_band)
        cfg_o = (
            nsb.NsbModelConfig.generic_clear_sky()
            .with_site_profile(profile)
            .with_airglow_model(models["oxygen"])
            .with_solar_radio_flux_sfu(f107_sfu)
        )
        evaluator = nsb.NsbEvaluator(cfg_o)
        za = nsb.ComponentMask.ZODIACAL | nsb.ComponentMask.AIRGLOW
        band = evaluator.integrate_band_components(
            nsb.PointQuery(observer, night, target, components=za),
            300.0,
            650.0,
        )
        comps = {c.name: float(c.integrated_photons_cm2_ns_sr) for c in band.components}
        summed = sum(comps.values())
        total = float(evaluator.integrate_band(
            nsb.PointQuery(observer, night, target, components=za), 300.0, 650.0
        ))
        ok = abs(summed - total) <= 1e-12 * max(1.0, abs(total))
        checks.append(
            {
                "id": f"component_summation_{site_name}",
                "status": "PASS" if ok else "FAIL",
                "components": comps,
                "sum": summed,
                "integrate_band_total": total,
            }
        )

    # Spectral ordering: oxygen green/red windows should increase vs continuum
    for site_name in CTA_SITES:
        c_g = results[site_name]["continuum"]["airglow_552_563_nm"]
        o_g = results[site_name]["oxygen"]["airglow_552_563_nm"]
        c_r = results[site_name]["continuum"]["airglow_625_642_nm"]
        o_r = results[site_name]["oxygen"]["airglow_625_642_nm"]
        checks.append(
            {
                "id": f"oxygen_line_windows_{site_name}",
                "status": "PASS" if (o_g > c_g and o_r > c_r) else "FAIL",
                "green_delta": o_g - c_g,
                "red_delta": o_r - c_r,
            }
        )

    checks.append(
        {
            "id": "model_version_reported",
            "status": "PASS",
            "MODEL_VERSION": nsb.MODEL_VERSION,
            "nsb_py_version": getattr(nsb, "__version__", None),
            "SIDERUST_VERSION": nsb.SIDERUST_VERSION,
            "SIDERUST_SOURCE": nsb.SIDERUST_SOURCE,
        }
    )

    failed = [c for c in checks if c.get("status") == "FAIL"]
    return {
        "schema": "nsb-physical-checks-v1",
        "executed_at_utc": datetime.now(UTC).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "f107_sfu": f107_sfu,
        "checks": checks,
        "ctao_airglow_bands": results,
        "all_passed": not failed,
        "note": (
            "Numerical/physical consistency on the installed NSB package — "
            "not observational validation."
        ),
    }
