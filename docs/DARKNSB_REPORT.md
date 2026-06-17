# darknsb inspection report

> **Historical note**
> This report documents the original Python `darknsb` codebase that informed the
> first Rust port. The current `nsb` crate no longer vendors that tree or ships
> Python bindings. For the current API and CLI, see `../README.md` and
> `../examples/`.

## Executive summary

`darknsb/` contains a Python implementation of a "dark" night-sky-background (NSB) model used in the Cherenkov Telescope Array Observatory context. The active executable path in `get_NSB.py` computes the broadband optical NSB for one site, time, and target direction as:

```text
dark NSB = zodiacal light + scattered/averaged starlight + airglow
```

The result is integrated over 300-650 nm and expressed as photon radiance:

```text
ph / (cm^2 ns sr)
```

The library also contains a detailed scattered-moonlight implementation and precomputed moonlight lookup tables, but `get_NSB.py` comments out the moon contribution. In its current form, "dark" therefore means a moon-excluded natural sky model, not a complete all-conditions NSB model.

## Directory contents

Inspected root: `darknsb/darknsb-main/`.

| Path | Role |
|---|---|
| `README` | Minimal runtime notes: Python 3.9, Astropy 5.1, NumPy 1.23.5, main entry point, and required data files. |
| `LICENSE` | BSD-3-Clause license, copyright 2023 Cherenkov Telescope Array Observatory. |
| `get_NSB.py` | Main script. Hardcodes `CTAO-S`, `2023-09-04 01:48:00`, and `SgrA*`; computes zodiacal light, starlight, and airglow; prints component and total dark NSB. |
| `NSB_Utils.py` | Main model library: observatory/target setup, lunar geometry, zodiacal light, starlight, airglow, atmospheric extinction, Rayleigh/Mie scattering, plotting helpers, and data readers. |
| `get_new_moon.py` | Helper script to compute daily Moon illuminated fractions for 2024 and write dates with phase `< 0.01`. |
| `new_moon_dates_2024.csv` | Output from `get_new_moon.py`. |
| `data/solar_spectrum.dat` | Extraterrestrial solar spectrum, wavelength in nm and flux in W m^-2 nm^-1. |
| legacy starlight radiance text file | SkyCalc-derived scattered starlight radiance spectrum used by the original Python code. |
| `data/airglow_cont.dat` | Airglow continuum tables and seasonal parameters. |
| `data/o3trans.dat` | Ozone transmission table. |
| `data/mie_m15s1.dat` | Mie phase/scattering grid, used by scattered moonlight. |
| `data/sscatcor_m15s1.dat` | Multiple Rayleigh/Mie scattering correction factors, used by scattered moonlight. |
| `airglow_files/*.fits` | 65 ESO SkyCalc FITS outputs named by altitude `ALT20.0` through `ALT84.0` at `AZ0.0`; headers document columns for total flux, transmission, scattered moonlight/starlight, zodiacal light, airglow lines, ozone, Rayleigh, Mie, etc. |
| `LUT_moon/Period_autumn/*.csv` | 14 phase-specific scattered-moonlight lookup tables for autumn. Each row contains moon azimuth, moon altitude, moon phase, target altitude, target azimuth, and integrated scattered moonlight. |
| `airglow.ipynb` | Notebook associated with airglow exploration/model fitting. |
| `__pycache__/` and `#NSB_Utils.py#` | Python cache and editor backup; not model inputs. |

The inspected directory is about 156 MB. The largest data footprint is the FITS and lookup-table material rather than the Python source.

## What NSB means here

NSB means night sky background: diffuse sky radiance seen by the telescope camera from natural sky emission and scattering processes. For Cherenkov telescopes, NSB is operationally important because it contributes photoelectron noise, affects trigger thresholds, and constrains observing conditions. In this codebase the working NSB unit is a band-integrated photon radiance:

```text
photons / (cm^2 ns sr)
```

The active model integrates optical spectra from 300 nm to 650 nm. This is a Cherenkov-camera-relevant wavelength interval rather than a generic astronomical photometric band. The code also computes B- and V-band-like S10 brightness proxies for magnitude-per-arcsec^2 reporting:

```python
v_mag = (27.78 - 2.5 * log10(v_flux)) mag / arcsec^2
b_mag = (27.78 - 2.5 * log10(b_flux)) mag / arcsec^2
```

Those magnitudes are currently calculated in `get_NSB.py` but not printed.

## Scientific model lineage

The Python source states that the model adapts the ESO Advanced Cerro Paranal Sky Model and cites:

1. Noll et al. 2012, "An atmospheric radiation model for Cerro Paranal. I. The optical spectral range", A&A 543, A92.
2. Jones et al. 2013, "An advanced scattered moonlight model for Cerro Paranal", A&A 560, A91.

The included data and algorithms match that lineage:

| Component | Scientific basis in the code |
|---|---|
| Zodiacal light | Leinert et al. 1997 tabulated zodiacal brightness in S10 units, scaled by a solar spectrum, reddening correction, and atmospheric extinction. |
| Starlight | SkyCalc-derived scattered starlight spectrum from the original Python data bundle. |
| Airglow | Active path uses an empirical cubic altitude fit; additional functions contain a seasonal/solar-radio-flux airglow continuum model and SkyCalc FITS inputs. |
| Moonlight | Lunar albedo and scattered moonlight machinery based on Jones et al. 2013: lunar phase angle, solar spectrum reflected by the Moon, Rayleigh/Mie scattering, aerosol single-scattering albedo, ground reflection, and correction grids. |
| Atmosphere | Airmass formulas, Rayleigh optical depth, Mie optical depth, extinction corrections, ozone and molecular-transmission hooks. |

## Active computation path

`get_NSB.py` performs the following workflow:

1. Build an observer and epoch with `setup_observatory("CTAO-S", "2023-09-04 01:48:00")`.
2. Resolve the target with `setup_source("SgrA*", obstime, location)`.
3. Compute:
   - `CalculateZL(location, obstime, source)`
   - `CalculateSL()`
   - `CalculateAG(location, obstime, source)`
4. Sum the integrated components:

```python
nsb_tot = integrated_zl + integrated_sl + integrated_ag
```

5. Print component radiances and total dark NSB.

`CalculateMoon(...)` exists but is commented out in the main script.

## Coordinate and time handling

The code uses Astropy for astronomy-facing coordinate work:

| Task | Implementation |
|---|---|
| Time | `astropy.time.Time(..., scale="utc")`; internal use of MJD for some formulas. |
| Observatory | `EarthLocation.of_site("lapalma")` for CTAO-N and `EarthLocation.of_site("paranal")` for CTAO-S. |
| Named targets | `SkyCoord.from_name(...)`, which depends on name-resolution services/cache. |
| Fixed coordinates | `SkyCoord(ra=RA, dec=DEC, frame="icrs", obstime=..., location=...)`. |
| Horizontal direction | `source.altaz` and `SkyCoord(..., frame="altaz", location=..., obstime=...)`. |
| Ecliptic direction | `source.heliocentrictrueecliptic`, used for zodiacal-light beta/lambda geometry. |
| Sun | Two methods exist: Astropy `get_sun` and a manual approximate `GetSunposition(mjd)` Keplerian ecliptic-longitude routine. |
| Moon | Astropy `get_moon`, phase-angle geometry, distance, separation from target, and topocentric altitude/zenith. |

From an engineering standpoint, the model depends strongly on Astropy conventions and cached site/name data. Reproducible production use would need explicit site coordinates and explicit target coordinates rather than relying on external registries at runtime.

## Component model details

### Zodiacal light

Zodiacal light is computed from the target's heliocentric true ecliptic latitude and its longitude separation from the Sun:

```text
beta = |target ecliptic latitude|
Delta_lambda = |target ecliptic longitude - Sun ecliptic longitude| folded into [0, 180 deg]
```

`GetZodiacalLight(beta, Delta_lambda)` stores a 5-degree grid from Leinert et al. in S10 units at 500 nm. It bilinearly interpolates in:

```text
beta: 0..90 deg
Delta_lambda: 0..180 deg
```

Special near-Sun, low-latitude zones outside the measured table are clamped to maximum values rather than rejected.

`GetZodiacalLightSpectrum(...)` then assumes the zodiacal spectrum follows the solar spectrum shape:

```text
zodiacal_spectrum(lambda) = solar_radiance(lambda) * [ZL_500 / solar_radiance(500 nm)]
```

`GetZodicalReddening(...)` applies a wavelength-dependent reddening correction based on elongation. `GetZodiacalLightExtinc(...)` applies atmospheric extinction using an airmass model, wavelength-dependent aerosol extinction, and Rayleigh/Mie correction factors from Noll-style parameterizations.

Finally, `CalculateZL(...)` converts the attenuated energy spectrum to photon radiance and integrates between 300 and 650 nm.

### Starlight

`CalculateSL()` is the simplest active component in the original Python code:

1. Read the legacy SkyCalc-derived starlight radiance text file.
2. Interpret the spectrum as `ph / (s m^2 um arcsec^2)`.
3. Convert to `ph / (ns cm^2 nm sr)`.
4. Integrate over 300-650 nm.
5. Return fixed B/V S10 constants:

```text
SL_s10_b = 17.22580320204227
SL_s10_v = 9.011178802900696
```

This means the original Python starlight path is not target-, date-, Galactic-coordinate-, or atmospheric-state-dependent. It is a fixed all-sky/average spectral contribution inherited from SkyCalc-derived data.

### Airglow

The active `CalculateAG(...)` path uses only target altitude:

```python
airglow = a * alt^3 + b * alt^2 + c * alt + d
```

with:

```text
a = -1.38267419e-07
b =  4.71757583e-05
c = -5.16178594e-03
d =  2.96338243e-01
```

and returns:

```text
ph / (sr ns cm^2)
```

It also returns fixed minimum B/V S10 values:

```text
AG_s10_b = 163.1898104690372
AG_s10_v = 228.73585615060816
```

The file contains a more physical `airglowcont(...)` function that reads `airglow_cont.dat`, applies season, solar radio flux, Van Rhijn geometry, and source zenith distance. However, that richer continuum path is incomplete: it builds intermediate spectra but does not return a final integrated airglow result used by `get_NSB.py`.

### Moonlight

The code has a detailed scattered moonlight implementation, but it is not part of the active dark-NSB sum.

The intended moonlight chain is:

1. `MoonObj` obtains the topocentric Moon position, distance, altitude/zenith angle, phase angle, illuminated fraction, and separation from the source.
2. `GetMoonAlbedo(g)` evaluates wavelength-dependent lunar albedo as a function of phase angle and selenographic terminator coordinate.
3. `MoonObj.spectrum(...)` computes lunar radiance outside the atmosphere:

```text
I_moon(lambda) =
    I_sun(lambda) * (Omega_moon / pi) * albedo(lambda, phase)
    * (mean_moon_distance / actual_moon_distance)^2
```

4. `scat_moon(...)` computes wavelength-dependent scattering into the line of sight:
   - Rayleigh density uses an exponential atmospheric profile.
   - Mie/aerosol density uses a steeper exponential profile.
   - Rayleigh phase function is `0.75 * (1 + cos(theta)^2)`.
   - Mie phase is interpolated from `data/mie_m15s1.dat`.
   - Effective column densities are integrated through a spherical atmosphere up to `HMAX = 200 km`.
   - Extinction is accumulated along Moon-to-scatterer and scatterer-to-observer paths.
   - A multiple-scattering correction is interpolated from `data/sscatcor_m15s1.dat`.
5. `spec_calc(...)` multiplies the top-of-atmosphere Moon spectrum by scattering intensity and converts to sky radiance.
6. `CalculateMoon(...)` integrates 300-650 nm and returns scattered moonlight in `ph / (cm^2 ns sr)`.

Important caveat: comments explicitly mark some double-scattering and molecular-absorption pieces as not implemented or TBD. Also, the active code uses `scipy.interpolate.interp2d`, which has been removed in recent SciPy versions; the README's old dependency pins matter.

## Spectral and unit conversions

The active ZL path converts energy radiance to photon radiance with:

```python
wl = wavelength.to(Angstrom)
erg = energy_radiance.to(erg / cm^2 / ns / sr / Angstrom)
photons = erg * wl * 5.03e7
```

This is the standard relation:

```text
N_photons = E_energy / (h c / lambda)
```

with unit constants folded into `5.03e7` for cgs/Angstrom units.

Integration is a discrete wavelength-bin sum:

```python
delta_lambda = diff(lambda.insert(0, lambda[0] - dl))
integrated = sum(spectrum[low:high] * delta_lambda[low:high])
```

For ZL and starlight, this produces band-integrated photon radiance. For airglow, the active cubic already returns a band-integrated photon radiance rather than a spectrum.

## Engineering assessment

### Strengths

- The scientific intent is clear and grounded in established Paranal/SkyCalc models.
- Core natural-sky components are separated into functions.
- Astropy handles most coordinate/time complexity.
- The code keeps physical units attached with `astropy.units`, reducing dimensional mistakes.
- It includes raw data tables needed for reproducibility.
- The scattered-moonlight machinery is substantially more advanced than the active main script suggests.

### Weaknesses and risks

| Area | Issue |
|---|---|
| Entrypoint | `get_NSB.py` is a hardcoded example, not a reusable CLI or library API. |
| Packaging | No `requirements.txt`, `pyproject.toml`, tests, or installation metadata. |
| Runtime data paths | Data files are loaded through relative paths such as `./data/...`; execution depends on current working directory. |
| Reproducibility | Observatory lookup and target-name lookup can depend on network/cache state. |
| Error handling | Many functions print and return sentinel values or call `exit()` rather than raising typed errors. |
| Compatibility | Code is pinned to old Astropy/SciPy behavior; `interp2d` is deprecated/removed in modern SciPy. |
| Scientific completeness | Active dark NSB excludes moonlight and uses a simple altitude-only airglow polynomial. |
| Maintainability | Several comments say "TBD", "not implemented", or "can't remember why"; one interpolation branch references `MoonSpectrumRed` out of scope. |
| Performance | Single-point use is fine, but map generation and moon scattering are Python-loop-heavy. |
| Validation | No automated golden tests compare output against SkyCalc, original CTAO model output, or independent references. |

## What is implemented versus merely present

| Feature | Present in source | Used by active `get_NSB.py` |
|---|---:|---:|
| Site/time/source setup | Yes | Yes |
| Zodiacal light | Yes | Yes |
| Zodiacal reddening | Yes | Yes |
| Zodiacal atmospheric extinction | Yes | Yes |
| Starlight spectrum integration | Yes | Yes |
| Airglow altitude polynomial | Yes | Yes |
| Airglow continuum table model | Partial | No |
| Ozone transmission reader | Partial | No |
| Scattered moonlight physics | Partial/advanced | No |
| Moon LUT CSVs | Yes | No direct use found |
| SkyCalc FITS airglow files | Yes | No direct use found in main path |
| Visibility plots | Yes | No |
| Zodiacal maps | Yes | No |

## Bottom line

`darknsb` is best understood as a research/prototype Python implementation of a CTAO dark-NSB model. Its active executable computes a natural, moonless NSB from zodiacal light, fixed starlight, and altitude-fitted airglow. The repository also contains enough partial infrastructure to support a more complete all-sky model with moonlight, seasonal airglow, and SkyCalc-derived calibration data, but that broader capability is not fully wired into the main script.
