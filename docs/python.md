# Python bindings

NSB exposes its supported evaluator and planning API to Python through a thin
PyO3 adapter. The Rust implementation remains the scientific source of truth:
Siderust owns astronomy and coordinate semantics, `qtty` owns physical
quantities, `tempoch` owns time semantics, and NSB owns night-sky-background
model composition and planning.

The Python distribution is named **`nsb-rust`** to avoid colliding with the
unrelated `nsb` project already present on PyPI. The import name is deliberately
kept as:

```python
import nsb
```

The project does not publish to PyPI as part of this feature. The distribution
name is a packaging identifier, not a claim that the name has been reserved.

## Supported versions and platforms

The bindings use PyO3's CPython stable ABI with a Python 3.10 floor
(`abi3-py310`). CI builds and installs real wheels on Linux x86_64, macOS, and
Windows x86_64, and exercises both the minimum Python and a current Python on
Linux. The CI matrix is the authoritative support matrix.

## Development build

Create a Python virtual environment and install maturin:

```bash
python -m venv .venv
. .venv/bin/activate
python -m pip install -U pip "maturin>=1.9,<2" pytest
maturin develop --locked
python -m pytest python/tests
```

To build a wheel instead:

```bash
maturin build --release --locked --out dist
python -m pip install --force-reinstall --find-links dist nsb-rust
```

## Point evaluation

```python
from datetime import datetime, timezone

import nsb

observer = nsb.Observer(
    latitude_deg=-24.683427777777776,
    longitude_deg=-70.31634444444444,
    elevation_m=2184.6,
)
target = nsb.Target(ra_deg=266.41683, dec_deg=-29.00781)

config = (
    nsb.NsbModelConfig.generic_clear_sky()
    .with_site_profile(nsb.SiteProfile.CTA_SOUTH)
)
evaluator = nsb.NsbEvaluator(config)

query = nsb.PointQuery(
    observer,
    datetime(2023, 9, 4, 1, 48, tzinfo=timezone.utc),
    target,
    components=nsb.ComponentMask.ZODIACAL | nsb.ComponentMask.AIRGLOW,
)
result = evaluator.evaluate(query)

print(result.integrated_photons_cm2_ns_sr)
for component in result.components:
    print(component.name, component.integrated_photons_cm2_ns_sr)
```

`Target` uses the NSB/Siderust `EquatorialMeanJ2000` convention. Right
ascension and declination are expressed explicitly in degrees.

## Window planning

```python
from datetime import datetime, timezone

query = (
    nsb.ThresholdQuery(
        observer,
        target,
        datetime(2023, 9, 4, 1, 0, tzinfo=timezone.utc),
        datetime(2023, 9, 4, 2, 0, tzinfo=timezone.utc),
        1.0e6,
        components=nsb.ComponentMask.ZODIACAL | nsb.ComponentMask.AIRGLOW,
        sample_step_s=600.0,
    )
    .with_sun_altitude_ceiling_deg(None)
    .with_target_altitude_floor_deg(None)
)

context = evaluator.prepare_site_window_context(query)
result = evaluator.periods_below_threshold_with_context(context, query)

for period in result.periods:
    print(period.start, period.end)
```

Returned period endpoints are timezone-aware UTC `datetime.datetime` values.

## Units

Python floats do not carry Rust's `qtty` type information, so every
dimensionful Python property and argument encodes its unit in its name:

| Python name suffix | Unit |
| --- | --- |
| `_deg` | degrees |
| `_m` | metres above the WGS84 reference ellipsoid |
| `_s` | seconds |
| `_nm` | nanometres |
| `_photons_cm2_ns_sr` | photons cm⁻² ns⁻¹ sr⁻¹ |
| `_mag_per_arcsec2` | mag arcsec⁻² |
| `_s10` | S10 |

Longitude is geodetic longitude, positive eastward, in `[-180, 180)`.
Latitude and declination are in `[-90, 90]`. Right ascension is in
`[0, 360)`.

## Time

Inputs must be timezone-aware Python `datetime.datetime` objects. Any explicit
UTC offset is normalized to UTC before conversion through `tempoch::Time<UTC>`.
Naive datetimes are rejected rather than being interpreted as UTC or local
time.

## Errors

All Rust `NsbError` categories have Python exception subclasses rooted at
`nsb.NsbError`:

- `DataParseError`
- `DataMissingError`
- `InvalidMapError`
- `OutOfRangeError`
- `UnsupportedError`
- `InterpolationError`
- `IoError`

Invalid Python boundary values such as a non-finite coordinate or a naive
datetime are reported as `OutOfRangeError`. CPU-heavy evaluator and planning
operations detach from the Python interpreter while Rust executes, allowing
other Python threads to make progress while NSB and Rayon compute.
