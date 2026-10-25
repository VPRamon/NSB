# Observational validation receipts, schema 1

Open [`examples/observational-validation.ipynb`](../examples/observational-validation.ipynb)
in a fresh supported CPython 3.12/3.13 Jupyter kernel and **Restart and Run All**.
The appendix exports to `RECEIPTS`, default `observational-validation-receipts/`
relative to the kernel's working directory. Change that path in the configuration
cell if needed. A checkout is unnecessary. Export uses the already computed
predictions and statistics and adds no dependencies or downloads.

The generated directory is ignored by Git. Keep the three files together:

| File | Contract |
| --- | --- |
| `manifest.json` | `schema_version: 1`, `status: complete`, provenance, ordered sample/model/scenario identities, partition counts, units, CSV columns and SHA-256 of the other two files |
| `metrics.json` | `schema_version: 1`; `aggregate[scenario]` is the notebook's existing `metrics[scenario]`; `conditional[scenario][condition]` is its existing conditional result |
| `predictions.csv` | One row for every selected spectrum × four configurations × two solar scenarios, including calibration observations (824 rows for the registered cohort) |

Both scenarios and all models must be available and aligned to the entire cohort.
Failed, duplicate, extra, missing, negative or nonfinite predictions prevent
export; zero/nonpositive observed denominators are rejected. No intersection or
favorable subset is reported. Before publication, the exporter recomputes the
ordered cohort SHA-256 with the same helper used during selection and requires
it to match `provenance.cohort.identity_sha256`; missing or invalid identity
metadata is rejected. It rebuilds condition masks from the exported evaluation
rows and reruns the existing `scores()` and `condition_scores()` helpers on the
aligned vectors and fixed calibration median. Every aggregate, per-night,
paired and conditional field (including intervals and any reported interpretations)
is checked without changing the supplied results. Floating-point comparisons
allow only `8 * numpy.finfo(float).eps` relative roundoff with zero absolute
tolerance; counts, field sets and text must match exactly. The extra computation
replays existing statistics and does not introduce a new estimator or alter the
serialized results. Schema 1 and successful receipt bytes remain unchanged.

An export attempt first removes any old manifest,
then validates and serializes all results in memory, writes CSV/metrics and
publishes the manifest last. If it fails, remaining files are **not a valid
receipt set**. The final campaign guard also removes an old manifest before reporting failure.
Earlier notebook failures that stop execution before this appendix
cannot invalidate an earlier run's files; use a fresh directory for each audit.
Concurrent writers to one directory are unsupported.

Schema 1 fixes the following fields and meanings. Consumers must reject unknown
schema versions. A change in field meaning, units, or required fields requires a
new schema version; additional provenance annotations may be added compatibly.

| CSV column | Meaning |
| --- | --- |
| `schema_version` | Integer `1` on every row |
| `observation_order` | Zero-based index in the registered cohort, retained for numerical replay |
| `observation_id`, `night_id` | Public FITS member identity and integer MJD UTC night |
| `subset` | `calibration` or `evaluation`; nights are disjoint |
| `scenario` | `native` or `matched`, separately evaluated |
| `model` | `NSB continuum`, `NSB Automatic` (experimental Oxygen), `nsb2`, or `baseline` |
| `predicted`, `observed` | Integrated photon radiances; baseline is the fixed median of calibration spectra |
| `residual`, `absolute_residual` | Prediction minus observation, and its magnitude, in radiance units |
| `relative_residual` | Signed residual divided by observed radiance, a dimensionless fraction |
| `airmass`, `f27_sfu`, `header_sfu` | Condition metadata: header airmass, observed centered F27, original header SUNFLUX; native nsb2 rounds the latter to 0.1 sfu |
| `wavelength_lower_nm`, `wavelength_upper_nm`, `unit` | 365.0, 650.0, `photons cm^-2 ns^-1 sr^-1` |

Order is native then matched, model order from the manifest, then registered
observation order. CSV uses UTF-8, LF, fixed columns, Python's shortest
round-trippable finite float representation, and standard CSV quoting. Required
CSV values cannot be missing. JSON uses UTF-8/ASCII escapes, sorted string keys,
two-space indentation, final LF, and `null` for explicitly unavailable metadata;
NaN and infinities are forbidden. Per-night JSON object keys are decimal strings.
No timestamps, local paths, install URLs, arbitrary environment variables or
exception messages are included. Identical inputs, notebook settings and package
versions produce identical bytes on the same numerical software stack. Different
NumPy/SciPy/NSB builds or architectures may change floating-point results; compare
numerically with a justified tolerance and retain the recorded versions in that
case. This is not a claim of bitwise reproducibility across software stacks.

## Reconstructing metrics

Use only `subset == "evaluation"` for reported performance. Calibration rows are
included for membership inspection and independent reconstruction of the fixed
median baseline; never pool them into evaluation errors. The following standard
library/NumPy example checks aggregate point estimates and night-cluster intervals
for every model and paired comparison directly from the receipts:

```python
import csv
import hashlib
import json
from pathlib import Path
import numpy as np

receipt_dir = Path("observational-validation-receipts")
manifest = json.loads((receipt_dir / "manifest.json").read_text())
metrics = json.loads((receipt_dir / "metrics.json").read_text())
assert manifest["schema_version"] == metrics["schema_version"] == 1
assert manifest["status"] == "complete"
for name, identity in manifest["files"].items():
    assert hashlib.sha256((receipt_dir / name).read_bytes()).hexdigest() == identity["sha256"]
with (receipt_dir / "predictions.csv").open(newline="") as stream:
    records = list(csv.DictReader(stream))
assert len(records) == manifest["prediction_records"]
settings = manifest["provenance"]["bootstrap"]
for scenario in manifest["scenario_order"]:
    result = metrics["aggregate"][scenario]
    draws, points = {}, {}
    for model in manifest["model_order"]:
        sample = sorted((r for r in records if r["scenario"] == scenario
                         and r["model"] == model and r["subset"] == "evaluation"),
                        key=lambda r: int(r["observation_order"]))
        y = np.array([float(r["observed"]) for r in sample])
        p = np.array([float(r["predicted"]) for r in sample])
        nights = np.array([int(r["night_id"]) for r in sample])
        r = p - y
        points[model] = [np.mean(abs(r)), np.mean(abs(r)/y), np.mean(r)]
        measured = result["models"][model]
        np.testing.assert_allclose(points[model], [measured["mae"],
            measured["mean_absolute_relative_error"], measured["mean_signed_error"]])
        unique = np.unique(nights)
        np.testing.assert_allclose(measured["night_weighted_mae"],
            np.mean([np.mean(abs(r[nights == n])) for n in unique]))
        # Restart the same RNG for each model: identical draws are paired.
        rng = np.random.default_rng(settings["seed"])
        sampled = rng.integers(len(unique), size=(settings["repetitions"], len(unique)))
        values = []
        for draw in sampled:
            indices = np.concatenate([np.flatnonzero(nights == unique[j]) for j in draw])
            d = r[indices]
            values.append([np.mean(abs(d)), np.mean(abs(d)/y[indices]), np.mean(d)])
        draws[model] = np.array(values)
        for column, metric in enumerate(("mae", "relative_mae", "bias")):
            np.testing.assert_allclose(measured[f"{metric}_ci95"],
                np.percentile(draws[model][:, column], [2.5, 97.5], method="linear"))
    for pair, measured in result["paired"].items():
        left, right = pair.split("_minus_")
        for column, metric in enumerate(("mae", "relative_mae", "bias")):
            np.testing.assert_allclose(measured[f"{metric}_difference"],
                points[left][column] - points[right][column])
            np.testing.assert_allclose(measured[f"{metric}_ci95"],
                np.percentile((draws[left]-draws[right])[:, column],
                              [2.5, 97.5], method="linear"))
```

`mae` is absolute error in radiance units. `mean_absolute_relative_error` is a
fraction: multiply by 100 for displayed percent; paired relative differences
become percentage points. `mean_signed_error` is physical signed bias, allowing
cancellation, while `relative_bias` is mean signed relative residual. Their
intervals must not be interchanged. `per_night` contains the existing descriptive
metrics and counts; `night_weighted_mae` weights nights equally and has no interval.
Paired intervals subtract metrics **within shared draws**, never CI endpoints.

Conditional tables retain the notebook's three physical models; the calibration
baseline is included in aggregate and per-night results.
To reproduce `conditional` results, apply each condition to the evaluation rows
before the same calculation: airmass ≤1.25 / >1.25, and F27 <67 / 67–166 inclusive /
>166 sfu. Both scenarios use the observed `f27_sfu` for stratification. Restart the
RNG with the recorded seed for each group; group spectra by their original night
IDs. Empty groups have empty model/paired objects. Groups with fewer than three
nights have descriptive points only; groups with three/four nights are flagged
sparse. Nights appearing in multiple bins do not make the bins independent.

## Provenance and downstream use

The manifest records input source URLs, registered Patat/solar SHA-256 identities,
cohort identity, selection, calibration nights, configuration, units, solar
conventions, software versions, available source revision and bootstrap settings.
Additional nsb2 reference downloads are fingerprinted **from the cached files
already used**, and its packaged Airglow/zodiacal tables are fingerprinted locally.
These observed digests are not an upstream admission or pre-registered checksum.
Verify downloaded scientific bytes with SHA-256 against the manifest. For cohort
identity, reconstruct LF-joined `id:night:split` strings in `observation_order`
(no final LF) and compare with `provenance.cohort.identity_sha256`.

Unknown NSB wheel revision, notebook checkout identity, bundled NSB asset digests
and missing uncertainty information are explicitly recorded as unavailable.
A release version is not a verified source commit. Preserve the notebook/source
used alongside receipts when archiving an experiment. Derived public broadband
values and identifiers are exported; raw FITS spectra are not copied. Retain
Patat and upstream data credits/terms when sharing derived results.

Downstream tools, including `nsb-validation`, can read these files, check schema,
file hashes and the full observation/model/scenario Cartesian product, then join
on observation ID and night ID. They need no imports from this repository and no
runtime connection to it. Receipts are audit artifacts, not new scientific
admission evidence. Preserve the native/matched distinction, model identities,
fixed calibration split, development dependence, research-only Starlight proxy
and limitations in [scientific validation](scientific-validation.md).
