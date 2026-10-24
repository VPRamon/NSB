# Standalone NSB observational validation

Download [NSB_Validation.ipynb](NSB_Validation.ipynb), open it in Jupyter with
CPython 3.12 or 3.13, configure section 1, and Run All. **Only the notebook is
needed.** It installs public dependencies, verifies/downloads the registered
Patat/FORS1 data, executes public models, displays results and optionally saves
one checksummed JSON record plus figures. No companion package, private service,
repository checkout or configuration file is required for notebook execution.

Tested platform: Linux x86-64. Other platforms need compatible NSB, provider and
scientific dependency wheels. Colab is untested. Scientific data downloads and
nsb2 source assets require network access initially; verified caches can be reused
with `OFFLINE_DATA=True`. Git is needed for the pinned public nsb2 installation.
Use a fresh virtual environment/kernel for each NSB runtime.

## Published and developer runtimes

The default is a real observational run using `RUNTIME="published"` and explicit
`NSB_VERSION="0.1.1"`. PyPI was checked on 24 October 2026: 0.1.1 is the latest
release; 0.2.0 is unpublished. **0.1.1 lacks the required Oxygen/restricted-band
API**, so the notebook deliberately stops with instructions rather than invent
an Oxygen approximation or substitute a different observable.

To run the complete experiment now, follow the notebook's separate public source
build instructions, install the wheel, and select `RUNTIME="developer"`. Set the
actual `NSB_VERSION`, `NSB_WHEEL`, `NSB_WHEEL_SHA256` and full `NSB_COMMIT`.
Imported Python files and the loaded native binary must match the selected wheel.
The source commit is explicitly a user-supplied build assertion; keep the clean
source/build log and Cargo.lock with the wheel for source traceability. Source
compilation never happens implicitly during analysis. The tested source revision
is `f5e1d22689ed417c0dcf9df72da54365c30f5bd5` (locally built nsb-py 0.2.0).

For a new published release, set its exact `NSB_VERSION`, start a fresh kernel,
and Run All. Binary availability, PyPI artifact identity and required APIs are
checked. For a new local build, select its wheel/version/hash/commit instead.

## Comparing releases

Keep version A's exported `run-*.json` and matching `.sha256`. In a fresh kernel
select version B and set `PREVIOUS_RUN` to A's JSON path. The notebook verifies
protocol, cohort, actual observations, band, solar inputs, statistical definitions,
reference inputs and provider identities before computing paired changes.
Positive current-minus-previous relative MAE is a regression. Incompatible,
incomplete or synthetic records are rejected. Comparison of a record with itself
is only a zero-difference verification, not a two-release scientific result.

## Scientific scope

The notebook contains the complete registered protocol and source citations:
103 spectra/22 nights; nine calibration spectra/five nights; 94 evaluation
spectra/17 nights; 365–650 nm exact energy-to-photon integration; native and
measured centered 27-day F10.7 scenarios; 2,000 paired whole-night bootstrap draws,
seed 182; practical relative-MAE margin 0.10. Invalid/missing predictions invalidate
a whole scenario. Archive, extracted cache, solar and reference identities are
checked, and individual selection exclusions are retained.

Oxygen is experimental. Patat influenced its development, so this cohort is not
an independent holdout. The restricted-band Starlight redistribution is a
research-only assumption. Spectral figures label observed total and modeled
Airglow as different observables. Bootstrap intervals omit shared calibration
systematics. See the notebook for limitations and original data acknowledgements.
No private validation source is copied or required; no production model/default
or scientific admission is changed.

## Repository verification

The tests extract definitions from the notebook itself. They are repository-only
and are not needed by notebook recipients. Ordinary CI uses a clean environment
without NSB or nsb2, then executes a single-file copy with network/install calls
forbidden. Manual workflow dispatch explicitly builds a selected public source
revision and runs the observational notebook.

```bash
python -m pip install numpy==2.5.4 astropy==8.0.2 matplotlib==3.11.2 \
  nbformat==5.11.1 nbclient==0.11.0 ipykernel==7.4.0 pytest
python -m pytest examples/validation/tests -q
```

`MODE="synthetic"` exercises all notebook sections without installing NSB/nsb2 or
fetching inputs. Its tables, plots and record are conspicuously labeled synthetic.
It does not establish scientific accuracy.

## Executed validation

Generated observations, executed notebooks, reports, wheels and caches stay out
of Git. The committed notebook has cleared outputs.

Verified on 24 October 2026:

- **31 tests pass on Python 3.12.13 and 3.13.14**, including a single-file nbclient
  execution from a temporary directory, with HTTP and installation forbidden.
  The 3.12 environment has neither NSB nor nsb2 installed. Analytic/Astropy unit
  oracles, extraction/cache defenses, selection boundaries, centered solar windows,
  missing predictions, unequal-night bootstrap, runtime tampering, executed-code
  signatures and incompatible-record rejection are exercised.
- **Full observational single-file nbclient execution passes** in a fresh Python
  3.13.14 environment: actual public nsb2 installation, fresh verified downloads,
  all three models on all 103 spectra in both solar scenarios, zero failures.
  Re-execution using the verified cache also passes. Tables and all four inline
  figure groups (coverage, native, matched, spectral/solar) were inspected.
- Observations, all six model/scenario component and total vectors, original
  aggregate metrics and paired-bootstrap endpoints reproduce the pre-refactor
  verified campaign exactly (maximum absolute computational difference **0**).
  This is numerical agreement with the prior implementation, not an assertion
  that different historical executables are identical.
- The actual published **0.1.1** PyPI wheel was downloaded, checksum-verified,
  installed and rejected by the required-API check as expected. A complete
  published-release campaign remains unavailable until a compatible release exists.
- Compatible real rerun comparison, deliberately incompatible protocol/selection
  rejection and simulated improvements/regressions are checked. The compatible
  real comparison uses the same runtime twice; no two-release performance claim
  is made.
- Ruff lint/format, notebook schema validation, workflow actionlint, tracked-source
  license headers and `git diff --check` pass. The production Rust test suite is
  outside this notebook/documentation-only change and was not rerun.

| Model | Native relative MAE | Matched measured F10.7 relative MAE |
|---|---:|---:|
| NSB Standard | 0.36102822 | 0.35026902 |
| NSB Oxygen (experimental) | 0.23882020 | 0.16572597 |
| nsb2 | 0.17086551 | 0.17424942 |
| Calibration-only baseline | 0.15245092 | 0.15245092 |

Matched Oxygen minus nsb2: relative-MAE difference −0.00852345, paired 95% CI
[−0.05709181, 0.05575557]. Absolute MAE is higher for Oxygen by 0.00250069
(CI [−0.01370266, 0.02409097] in radiance units). **Superiority is not established.**
79/103 measured solar inputs extrapolate beyond PALACE's 67–166 sfu domain.

Runtime: source `f5e1d22689ed417c0dcf9df72da54365c30f5bd5`, nsb-py 0.2.0;
nsb2 `bc9320db03fbff69997ce366c6e76a37339c5d00`; Python providers siderust 0.5.1 /
tempoch 0.5.0; NumPy 2.5.4 / Astropy 8.0.2 / Matplotlib 3.11.2. The existing
clean-source build receipt was verified and retained in the real run, including
Cargo.lock, Rust/provider/Maturin identities. Its wheel SHA-256 is
`16ea1fc237d6e40c3f63e7e5ffb4357c0c52c4f1dce6cf605757f2ee5b7a74d2`.
