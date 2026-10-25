# NSB for Python

**Night-sky background evaluation and observation planning for astronomy.**

`nsb-py` provides Python bindings to NSB's Rust scientific engine. Evaluate
integrated optical night-sky background at a ground-based observer, UTC instant
and sky direction, or search for observing windows satisfying brightness and
visibility constraints.

## Install

Python 3.10 or newer is required:

```bash
python -m pip install nsb-py
```

The installed **distribution** is `nsb-py`; the **import** is `nsb`.
Compatible `siderust` and `tempoch` Python runtime packages are installed as
dependencies. Version 0.2.0 requires successful registry publication.

## Evaluate a point

This example uses geographic coordinates of CTAO South with **generic clear-sky**
assumptions, not a site-calibrated observatory profile. All times must be
timezone-aware.

```python
from datetime import datetime, timezone

import nsb
import siderust

observer = siderust.Observer(
    lon_deg=-70.31634, lat_deg=-24.683427, height_m=2184.6
)
target = siderust.Direction(ra_deg=266.41683, dec_deg=-29.00781)

evaluator = nsb.NsbEvaluator()
query = nsb.PointQuery(
    observer,
    datetime(2026, 6, 19, 3, tzinfo=timezone.utc),
    target,
)
result = evaluator.evaluate(query)
print(f"{result.integrated_photons_cm2_ns_sr:.3e} ph/(cm² ns sr)")
```

Point outputs integrate **300–650 nm** in `ph cm^-2 ns^-1 sr^-1`. See the
[Python API guide](https://github.com/VPRamon/NSB/blob/main/docs/python.md)
for component breakdowns, observing-window searches and model configuration.

## Scientific applicability

NSB combines zodiacal light, integrated starlight, airglow and scattered
moonlight. The default configuration is a **planning model**, not a prediction
of absolute night-sky brightness calibrated at an observatory.

In 0.2.0, Automatic Airglow selects experimental PALACE Oxygen, limited to
standard continuum geometry and zenith distance **≤60°**. Out-of-domain queries
raise errors; no alternative model is silently substituted.
The bundled Gaia-derived Starlight map has verified provenance and
cross-implementation evidence, **not** independently established absolute
observational accuracy. CTAO planning presets are not observatory calibrations.

Before scientific use, consult
[validation and limitations](https://github.com/VPRamon/NSB/blob/main/docs/scientific-validation.md)
and the [model-maturity specification](https://github.com/VPRamon/NSB/blob/main/docs/specifications/model-maturity.md).

## License, data and citation

NSB source code is **AGPL-3.0-only**. Scientific datasets retain separate
licenses and conditions; Gaia-derived Starlight is subject to applicable
**CC BY-NC 3.0 IGO** terms, potentially restricting commercial use. See
[third-party notices](https://github.com/VPRamon/NSB/blob/main/THIRD_PARTY_NOTICES.md)
and [source terms](https://github.com/VPRamon/NSB/blob/main/docs/nsb_components/starlight/licensing/README.md).
Cite the software using [CITATION.cff](https://github.com/VPRamon/NSB/blob/main/CITATION.cff).

The [NSB repository](https://github.com/VPRamon/NSB) contains Rust, Python and
CLI guides, runnable examples and reproducible scientific evidence.
