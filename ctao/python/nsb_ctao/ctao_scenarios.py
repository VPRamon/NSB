"""Illustrative CTAO spectral→detector rate scenarios (not camera validation)."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import UTC, datetime
from itertools import pairwise
from typing import Any

import nsb
import numpy as np
import siderust

from .physical_checks import CTA_SITES, near_zenith_direction


@dataclass(frozen=True)
class IllustrativeResponse:
    """Toy optical x PDE response for scenario illustration only."""

    name: str
    wavelength_nm: np.ndarray
    A_opt_pde_cm2: np.ndarray  # product A_opt(λ)·PDE(λ) in cm²
    omega_pixel_sr: float

    def __post_init__(self) -> None:
        object.__setattr__(self, "wavelength_nm", np.asarray(self.wavelength_nm, dtype=float))
        object.__setattr__(self, "A_opt_pde_cm2", np.asarray(self.A_opt_pde_cm2, dtype=float))


def illustrative_wideband_response() -> IllustrativeResponse:
    """Smooth illustrative Cherenkov-band response — NOT a CTAO camera curve."""
    wave = np.linspace(300.0, 650.0, 351)
    # Peak ~0.5 cm²·effective near 400 nm, falling toward red — purely illustrative.
    peak = 0.5
    shape = np.exp(-0.5 * ((wave - 400.0) / 90.0) ** 2)
    a_pde = peak * shape
    # ~0.1° square pixel → Ω ≈ (π/180·0.1)² sr
    omega = (np.pi / 180.0 * 0.1) ** 2
    return IllustrativeResponse(
        name="illustrative_wideband_v1",
        wavelength_nm=wave,
        A_opt_pde_cm2=a_pde,
        omega_pixel_sr=float(omega),
    )


def spectral_density_photons(
    evaluator: nsb.NsbEvaluator,
    query: nsb.PointQuery,
    edges_nm: np.ndarray,
) -> np.ndarray:
    """Per-bin mean spectral density from restricted-band integrals (ph cm^-2 ns^-1 sr^-1 nm^-1)."""
    dens = []
    for lo, hi in pairwise(edges_nm):
        integ = float(evaluator.integrate_band(query, float(lo), float(hi)))
        dens.append(integ / (hi - lo))
    return np.asarray(dens, dtype=float)


def photoelectron_rate(
    density: np.ndarray,
    wave_centers: np.ndarray,
    response: IllustrativeResponse,
) -> float:
    """R_pe = Omega * integral I(lambda) A_opt(lambda) PDE(lambda) d lambda [pe/ns].

    density and response are interpolated onto response.wavelength_nm.
    """
    i_of_l = np.interp(response.wavelength_nm, wave_centers, density, left=0.0, right=0.0)
    integrand = i_of_l * response.A_opt_pde_cm2
    integral = float(np.trapezoid(integrand, response.wavelength_nm))
    return response.omega_pixel_sr * integral


def run_ctao_scenarios(*, f107_values: tuple[float, ...] = (70.0, 130.0, 200.0)) -> dict[str, Any]:
    response = illustrative_wideband_response()
    night = datetime(2024, 1, 15, 2, 0, tzinfo=UTC)
    edges = np.arange(300.0, 651.0, 5.0)
    centers = 0.5 * (edges[:-1] + edges[1:])
    rows = []
    for site_name, coords in CTA_SITES.items():
        profile = getattr(nsb.SiteProfile, site_name)
        observer = siderust.Observer(**coords)
        target = near_zenith_direction(observer, night)
        query = nsb.PointQuery(
            observer, night, target, components=nsb.ComponentMask.AIRGLOW
        )
        for f107 in f107_values:
            for label, model in (
                ("continuum", nsb.AirglowModel.PARANAL_PALACE_V1),
                ("oxygen", nsb.AirglowModel.PARANAL_PALACE_OXYGEN_V1),
            ):
                cfg = (
                    nsb.NsbModelConfig.generic_clear_sky()
                    .with_site_profile(profile)
                    .with_airglow_model(model)
                    .with_solar_radio_flux_sfu(f107)
                )
                evaluator = nsb.NsbEvaluator(cfg)
                dens = spectral_density_photons(evaluator, query, edges)
                rate = photoelectron_rate(dens, centers, response)
                band = float(evaluator.integrate_band(query, 300.0, 650.0))
                rows.append(
                    {
                        "site": site_name,
                        "f107_sfu": f107,
                        "airglow_model": label,
                        "airglow_300_650": band,
                        "R_pe_ns": rate,
                        "unit_band": "photons cm^-2 ns^-1 sr^-1",
                        "unit_rate": "photoelectrons ns^-1 (illustrative)",
                    }
                )
    # Oxygen effect at F10.7=130
    effect = []
    for site_name in CTA_SITES:
        cont = next(
            r
            for r in rows
            if r["site"] == site_name and r["f107_sfu"] == 130.0 and r["airglow_model"] == "continuum"
        )
        oxy = next(
            r
            for r in rows
            if r["site"] == site_name and r["f107_sfu"] == 130.0 and r["airglow_model"] == "oxygen"
        )
        effect.append(
            {
                "site": site_name,
                "delta_band": oxy["airglow_300_650"] - cont["airglow_300_650"],
                "rel_band": (oxy["airglow_300_650"] - cont["airglow_300_650"])
                / cont["airglow_300_650"],
                "delta_R_pe": oxy["R_pe_ns"] - cont["R_pe_ns"],
                "rel_R_pe": (oxy["R_pe_ns"] - cont["R_pe_ns"]) / cont["R_pe_ns"],
            }
        )
    return {
        "schema": "nsb-ctao-scenarios-v1",
        "label": "ILLUSTRATIVE — not validated CTAO camera predictions",
        "response": {
            "name": response.name,
            "omega_pixel_sr": response.omega_pixel_sr,
            "note": (
                "A_opt·PDE is a synthetic smooth function. Do not treat rates as "
                "instrument commissioning numbers."
            ),
        },
        "formula": "R_pe = Omega_pixel * integral I_gamma(lambda) * A_opt(lambda) * PDE(lambda) d lambda",
        "rows": rows,
        "oxygen_effect_f107_130": effect,
        "executed_at_utc": datetime.now(UTC).strftime("%Y-%m-%dT%H:%M:%SZ"),
    }
