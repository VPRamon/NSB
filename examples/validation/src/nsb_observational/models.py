# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Public source APIs only; research Starlight redistribution is explicit."""

from datetime import UTC
from types import SimpleNamespace

import astropy.units as u
import numpy as np
from astropy.constants import c, h
from astropy.coordinates import AltAz, EarthLocation, SkyCoord
from astropy.time import Time
from astropy.utils import iers
from astropy.utils.data import cache_contents

from .data import photon_density, sha256

SITE = dict(longitude=-70.4045, latitude=-24.6272, height=2635.0)
COMPONENTS = ("zodiacal", "airglow", "starlight", "moonlight")
MODELS = ("NSB Standard", "NSB Oxygen", "nsb2")
DENSITY_UNIT = 1 / u.cm**2 / u.ns / u.sr / u.nm


def reference_integral(wave, density, lower, upper):
    """Frozen nsb2/Rieke protocol: trapezoid on in-band source-grid nodes.

    Retained to compare with original evidence (not a new integration rule).
    The exact measured-energy operator is separate and inserts band edges.
    """
    wave, density = np.asarray(wave, float), np.asarray(density, float)
    if (
        wave.shape != density.shape
        or wave.ndim != 1
        or not np.isfinite(density).all()
        or not np.isfinite(wave).all()
        or np.any(np.diff(wave) <= 0)
        or wave[0] > lower
        or wave[-1] < upper
    ):
        raise ValueError("Invalid source spectrum or incomplete coverage")
    mask = (wave >= lower) & (wave <= upper)
    if np.count_nonzero(mask) < 2:
        raise ValueError("Insufficient in-band source-grid nodes")
    return float(np.trapezoid(density[mask], wave[mask]))


def starlight_fraction():
    from nsb2.core.photometry import SolarSpectrumRieke2008

    wave, energy = SolarSpectrumRieke2008()
    photon = (energy * wave / (h * c)).to_value(1 / u.s / u.cm**2 / u.nm)
    wave = wave.to_value(u.nm)
    return reference_integral(wave, photon, 365, 650) / reference_integral(wave, photon, 300, 650)


def external_cache_inventory():
    """Record all Astropy data inputs, including nsb2's remote source assets."""
    return {url: sha256(path) for url, path in sorted(cache_contents().items())}


class Models:
    def __init__(self):
        import nsb
        import siderust
        from nsb2.emitter.stars import from_gaia_dr3_map
        from nsb2.emitter.zodiacal import from_leinert1998

        iers.conf.auto_download = False
        self.nsb, self.siderust = nsb, siderust
        self.observer = siderust.Observer(
            lon_deg=SITE["longitude"], lat_deg=SITE["latitude"], height_m=SITE["height"]
        )
        self.site = EarthLocation.from_geodetic(
            SITE["longitude"] * u.deg, SITE["latitude"] * u.deg, SITE["height"] * u.m
        )
        print("Preparing nsb2 reference assets (Astropy cache; data downloads only)", flush=True)
        self.fraction = starlight_fraction()
        self.zodiacal = from_leinert1998()
        self.stars = from_gaia_dr3_map()
        self.air_sources = {}

    def query(self, row, mask):
        return self.nsb.PointQuery(
            self.observer,
            Time(row["mjd"], format="mjd", scale="utc").to_datetime(timezone=UTC),
            self.siderust.Direction(ra_deg=row["ra"], dec_deg=row["dec"]),
            components=mask,
        )

    def nsb_components(self, row, oxygen, sfu=None):
        nsb = self.nsb
        config = nsb.NsbModelConfig.generic_clear_sky().with_airglow_model(
            nsb.AirglowModel.PARANAL_PALACE_OXYGEN_V1
            if oxygen
            else nsb.AirglowModel.PARANAL_PALACE_V1
        )
        if sfu is not None:
            config = config.with_solar_radio_flux_sfu(float(sfu))
        evaluator = nsb.NsbEvaluator(config)
        # Includes Moon explicitly: API must establish physical zero, not a fabricated band.
        mask = nsb.ComponentMask.ZODIACAL | nsb.ComponentMask.AIRGLOW | nsb.ComponentMask.MOON
        band = evaluator.integrate_band_components(self.query(row, mask), 365.0, 650.0)
        result = {
            component.name: float(component.integrated_photons_cm2_ns_sr)
            for component in band.components
        }
        # Runtime uses "moon" rather than the report's "moonlight" label.
        result["moonlight"] = result.pop("moon")
        if result["moonlight"] != 0.0:
            raise ValueError("Registered moonless assumption violated")
        star = evaluator.evaluate(self.query(row, nsb.ComponentMask.STARLIGHT))
        result["starlight"] = float(star.components[0].integrated_photons_cm2_ns_sr) * self.fraction
        return {key: result[key] for key in COMPONENTS}

    def source_integral(self, source, row):
        frame = AltAz(
            obstime=Time(row["mjd"], format="mjd", scale="utc"),
            location=self.site,
            pressure=0 * u.hPa,
        )
        direction = SkyCoord(ra=[row["ra"]] * u.deg, dec=[row["dec"]] * u.deg)
        field, _ = source.query_direct(SimpleNamespace(origin=frame), direction, np.zeros(1))
        wave = field.spectral_grid.wvl.to_value(u.nm)
        raw = field.spectral_grid(field.spectral_data)
        try:
            density = (raw[:, 0] * field.weights[0, 0]).to_value(DENSITY_UNIT)
        except u.UnitConversionError:
            density = None
        if density is None or density.shape != wave.shape:
            # The public resolver handles the source's photometric normalization units.
            from nsb2.core.spectral import Bandpass

            band = Bandpass(np.array([365.0, 400.0, 550.0, 650.0]) * u.nm, np.ones(4))
            rate = field.resolve_spectra(band).integrate()
            return (
                float(
                    np.asarray(
                        rate.to_value(
                            1 / u.s / u.cm**2 / u.sr, equivalencies=u.dimensionless_angles()
                        )
                    ).reshape(-1)[0]
                )
                * 1e-9
            )
        return reference_integral(wave, density, 365, 650)

    def nsb2_components(self, row, sfu=None):
        from nsb2.emitter.airglow import from_eso_skycalc

        # Native nsb2 protocol uses SUNFLUX rounded to 0.1 sfu. Matched diagnostic
        # scales that exact source by its scalar law, with unrounded measured f27.
        native_flux = round(row["header_sfu"], 1)
        if native_flux not in self.air_sources:
            self.air_sources[native_flux] = from_eso_skycalc(90 * u.km, native_flux)
        air = self.source_integral(self.air_sources[native_flux], row)
        if sfu is not None:
            air *= (0.2 + 0.00614 * sfu) / (0.2 + 0.00614 * native_flux)
        return {
            "zodiacal": self.source_integral(self.zodiacal, row),
            "airglow": air,
            "starlight": self.source_integral(self.stars, row),
            "moonlight": 0.0,
        }

    def predict(self, row, model, sfu=None):
        parts = (
            self.nsb2_components(row, sfu)
            if model == "nsb2"
            else self.nsb_components(row, model == "NSB Oxygen", sfu)
        )
        if (
            set(parts) != set(COMPONENTS)
            or not np.isfinite(list(parts.values())).all()
            or min(parts.values()) < 0
        ):
            raise ValueError("Invalid/incomplete component prediction")
        return {"id": row["id"], "total": float(sum(parts.values())), "components": parts}

    def spectral_example(self, row, sfu=None):
        """Actual restricted API bin integrals, never an invented resolved line profile."""
        nsb = self.nsb
        bins = np.arange(550.0, 645.0, 1.0)
        result = {
            "id": row["id"],
            "air_wavelength_nm": (row["wave"] / 10).tolist(),
            "observed_total_density": photon_density(row["wave"], row["energy"]).tolist(),
            "vacuum_bin_edges_nm": bins.tolist(),
            "scenario": "native" if sfu is None else "matched",
        }
        for name, model in (
            ("standard", nsb.AirglowModel.PARANAL_PALACE_V1),
            ("oxygen", nsb.AirglowModel.PARANAL_PALACE_OXYGEN_V1),
        ):
            cfg = nsb.NsbModelConfig.generic_clear_sky().with_airglow_model(model)
            if sfu is not None:
                cfg = cfg.with_solar_radio_flux_sfu(sfu)
            ev = nsb.NsbEvaluator(cfg)
            result[name] = [
                float(
                    ev.integrate_band(
                        self.query(row, nsb.ComponentMask.AIRGLOW), float(a), float(b)
                    )
                )
                for a, b in zip(bins[:-1], bins[1:], strict=True)
            ]
        return result
