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
- **tempoch** owns time-scale and interval semantics.

The current `nsb.Observer`, `nsb.Direction`, and Python `datetime` bridge are temporary compatibility adapters. They exist because the reusable upstream Python packages are not yet on the Siderust/qtty/tempoch dependency stack used by NSB. They convert Python values immediately into canonical Rust Siderust/tempoch types; NSB does not implement a parallel coordinate or time policy.

`Observer` and `Direction` intentionally follow the naming used by `siderust-py`. There is no Python `nsb.Target`: NSB's Rust `Target` is a fixed equatorial direction, while Siderust's Python ecosystem already uses `Target` for a different concept.

## Package layout

The public package is a small Python facade over a private native extension:

```text
python/nsb/__init__.py
python/nsb/_nsb.*
```

Maturin builds the extension as `nsb._nsb`. Users import only `nsb`.

## Point evaluation

```python
from datetime import datetime, timezone

import nsb

observer = nsb.Observer(-70.4, -24.6, 2600.0)
direction = nsb.Direction(266.4, -29.0)

config = (
    nsb.NsbModelConfig.generic_clear_sky()
    .with_site_profile(nsb.SiteProfile.CTA_SOUTH)
)
evaluator = nsb.NsbEvaluator(config)

query = nsb.PointQuery(
    observer,
    datetime(2026, 9, 27, 22, 0, tzinfo=timezone.utc),
    direction,
)
result = evaluator.evaluate(query)

print(result.integrated_photons_cm2_ns_sr)
for component in result.components:
    print(component.name, component.integrated_photons_cm2_ns_sr)
```

Site-specific planning constructors are intentionally absent. For example, CTA South is expressed by composing the generic configuration with `SiteProfile.CTA_SOUTH` rather than through a `cta_s_planning()` alias.

`ComponentMask` exposes named component flags and bitwise composition. Raw bit construction is not part of the public Python API.

## Planning

Planning windows use ordinary timezone-aware Python datetimes at the boundary:

```python
query = nsb.ThresholdQuery(
    observer,
    direction,
    datetime(2026, 9, 27, 20, 0, tzinfo=timezone.utc),
    datetime(2026, 9, 28, 4, 0, tzinfo=timezone.utc),
    0.25,
    components=nsb.ComponentMask.ZODIACAL | nsb.ComponentMask.AIRGLOW,
    sample_step_s=600.0,
)

context = evaluator.prepare_site_window_context(query)
result = evaluator.periods_below_threshold_with_context(context, query)

for start, end in result.periods:
    print(start, end)
```

Returned periods are `(start, end)` tuples of timezone-aware UTC `datetime.datetime` values. NSB does not expose a bespoke Python `UtcPeriod` type.

## Time boundary

Input datetimes must be timezone-aware. Explicit non-UTC offsets are normalized to UTC before conversion into `tempoch::Time<UTC>`. Naive datetimes are rejected with `nsb.OutOfRangeError`. The conversion code is isolated under `python/compat/tempoch.rs`.

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
maturin develop --locked
python -m pytest python/tests
```

To validate an installed wheel:

```bash
maturin build --release --locked --out dist
python -m pip install --force-reinstall --no-index --find-links dist nsb-rust
python -m pytest python/tests
```

The binding uses PyO3's CPython stable ABI with a Python 3.10 floor (`abi3-py310`). Python support remains feature-gated, so normal Rust builds do not enable PyO3.

## Upstream migration note

The compatibility directory is deletion-oriented. When reusable Siderust/tempoch Python bindings match NSB's dependency stack, migration should be limited to replacing the facade exports/conversion entry points and deleting `python/compat/siderust.rs` and/or `python/compat/tempoch.rs`. NSB-owned files under `python/api/` should not need a scientific redesign.
