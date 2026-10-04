# Python bindings

NSB exposes a deliberately small Python API for NSB evaluation and planning. The Rust crate in `crates/nsb` remains the scientific implementation and the authoritative public model contract.

The Python distribution is named **`nsb-rust`**; the import remains:

```python
import nsb
```

## Ownership boundary

NSB owns the Python API for model configuration, component selection, NSB evaluation, result/component metadata, threshold planning, and reusable planning contexts.

Generic scientific primitives belong upstream:

- **Siderust** owns observers, coordinate systems, directions, and astronomy semantics.
- **qtty** owns physical quantities and units.
- **tempoch / tempoch-py** own time-scale, interval, and Python datetime conversion semantics.

Python observers and directions are the canonical `siderust.Observer` and
`siderust.Direction` classes. NSB consumes `siderust-py`'s versioned
cross-extension bridge and does not register duplicate `nsb.Observer` or
`nsb.Direction` classes. At the Rust boundary, the bridge's ICRS direction is
transformed through Siderust's frame-bias rotation to NSB's
`EquatorialMeanJ2000` target type; getters apply the inverse transform before
constructing the canonical Python object.

Siderust owns coordinate validity. As a temporary compatibility safeguard for
`siderust-py` 0.2, the NSB query boundary rejects non-finite observer and
direction components before scientific evaluation. This guard can be removed
once the canonical Siderust constructors enforce the same invariant upstream;
NSB does not otherwise duplicate Siderust's coordinate-range policy.

## Package layout

This is a pure-Rust maturin project with import name `nsb`. Maturin supplies its
minimal generated package initializer, and the root-level `nsb.pyi` ships beside
the extension for static typing. There is no maintained pass-through facade or
duplicated re-export list.

## Point evaluation

```python
from datetime import datetime, timezone

import nsb
import siderust

# CTAO South WGS84 coordinates from the bundled observatory catalog.
ctao_south = siderust.Observer(
    lon_deg=-70.31634444444444,
    lat_deg=-24.683427777777776,
    height_m=2184.6,
)
# Sagittarius A* in ICRS coordinates.
sgr_a_star = siderust.Direction(
    ra_deg=266.41683,
    dec_deg=-29.00781,
)

config = (
    nsb.NsbModelConfig.generic_clear_sky()
    .with_site_profile(nsb.SiteProfile.CTA_SOUTH)
)
evaluator = nsb.NsbEvaluator(config)

query = nsb.PointQuery(
    ctao_south,
    datetime(2026, 9, 27, 22, 0, tzinfo=timezone.utc),
    sgr_a_star,
)
result = evaluator.evaluate(query)

print(result.integrated_photons_cm2_ns_sr)
for component in result.components:
    print(component.name, component.integrated_photons_cm2_ns_sr)
```

`SiteProfile` is a Python application-layer selector enum (`GENERIC_CLEAR_SKY`,
`CTA_NORTH`, `CTA_SOUTH`). Each variant constructs a typed Rust
`SiteProfile<P>` with a binding-local `SiteProfileTag` marker before
`NsbModelConfig::with_site_profile` erases the type. CTAO North/South are
convenience presets only; the generic Rust public API exports
`SiteProfileTag`, `SiteProfile<P>`, and `GenericClearSky` — not CTAO markers.
Inspect the attached profile after configuration via `config.site_profile()`;
`NsbModelConfig.site_profile_name()` mirrors the marker's `NAME` metadata.

`ComponentMask` exposes named component flags and bitwise composition. Raw bit construction is not part of the public Python API.

## Planning

Planning windows use ordinary timezone-aware Python datetimes at the boundary:

```python
max_nsb_photons_cm2_ns_sr = 0.25
sample_step_s = 600.0

query = nsb.ThresholdQuery(
    ctao_south,
    sgr_a_star,
    datetime(2026, 9, 27, 20, 0, tzinfo=timezone.utc),
    datetime(2026, 9, 28, 4, 0, tzinfo=timezone.utc),
    max_nsb_photons_cm2_ns_sr,
    components=nsb.ComponentMask.ZODIACAL | nsb.ComponentMask.AIRGLOW,
    sample_step_s=sample_step_s,
)

context = evaluator.prepare_site_window_context(query)
result = evaluator.periods_below_threshold_with_context(context, query)

for start, end in result.periods:
    print(start, end)
```

Returned periods are `(start, end)` tuples of timezone-aware UTC `datetime.datetime` values. NSB does not expose a bespoke Python `UtcPeriod` type.

## Time boundary

Input datetimes must be timezone-aware. `tempoch-py` validates Python datetime
objects, normalizes explicit non-UTC offsets, and performs the canonical
conversion to `tempoch::Time<UTC>`. NSB only translates conversion failures to
its documented `nsb.OutOfRangeError` with argument context. Returned instants
and period endpoints are created through the same upstream bridge as aware UTC
`datetime.datetime` values.

## Units

Python scalar arguments and properties carry units in their names because Rust `qtty` quantities do not cross the Python boundary directly. Common suffixes are `_deg`, `_m`, `_s`, `_nm`, `_photons_cm2_ns_sr`, `_mag_per_arcsec2`, and `_s10`.

## Errors and concurrency

Rust `NsbError` categories map to subclasses of `nsb.NsbError`. Generic boundary validation uses `OutOfRangeError`; NSB does not define a separate Siderust/tempoch exception taxonomy.

Evaluator construction, point evaluation, and planning operations detach from the Python interpreter while Rust-only work runs. The binding does not reimplement scientific calculations in Python.

## Development

```bash
python -m venv .venv
. .venv/bin/activate
python -m pip install -U pip "maturin>=1.9,<2" pytest
maturin develop --release --manifest-path ../siderust-py/Cargo.toml
maturin develop --locked
python -m pytest python/tests
```

To validate an installed wheel:

```bash
maturin build --release --locked --out dist
# Install the canonical siderust wheel first when testing with --no-index.
python -m pip install --no-index --find-links siderust-dist siderust
python -m pip install --force-reinstall --no-index --find-links dist nsb-rust
python -m pytest python/tests
```

The NSB binding uses PyO3's CPython stable ABI with a Python 3.10 floor
(`abi3-py310`). Python support and the `siderust-py` / `tempoch-py` Rust bridge
dependencies remain feature-gated, so normal Rust builds do not enable PyO3.

The `nsb-rust` distribution declares `siderust>=0.2,<0.3` as a runtime
dependency. Until canonical `siderust` wheels are available from PyPI, wheel CI
builds that dependency from the checksum-pinned crates.io 0.2.0 source package
and its published lockfile before testing the freshly built NSB wheel with
`--no-index`.
