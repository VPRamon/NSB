# NSB versus nsb2: Patat observational notebook

Open [nsb-vs-nsb2.ipynb](nsb-vs-nsb2.ipynb). It is the user interface for setup,
verified acquisition, explicit model execution, paired statistics and an exported
scientific report. All plots render in Jupyter; every run has its own output
identity. No NSB runtime model, default, asset or admission is changed.

## Prepare the isolated environment

Use Python 3.11–3.13 and [uv](https://docs.astral.sh/uv/). From this directory:

```bash
uv sync --locked --python 3.12
uv run --no-sync python -m ipykernel install --user --name nsb-observational \
  --display-name 'NSB observational validation'
```

Select that kernel in Jupyter or your notebook editor, then **Run All**. The
initial mode is a deterministic **synthetic offline demonstration**. It exercises
the report pipeline and cannot establish empirical accuracy. `mode='historical'`
instead displays the published development results and their distinct source
identities, without claiming to execute a historical binary.

For the real campaign, explicitly install the public pinned comparison model:

```bash
uv sync --locked --python 3.12 --extra models
```

The optional lock pins nsb2 to
`bc9320db03fbff69997ce366c6e76a37339c5d00` (BSD-3-Clause). Installation uses
public HTTPS Git; GitHub authentication and the private validation repository
are unnecessary. nsb2 needs public scientific assets on its first use: the Rieke
solar spectrum, Gaia faint map, Gaia passbands, Vega and Pickles templates.
These data downloads can be substantial. Its public factories use Astropy's
cache. The report records cache URL/checksum pairs, the entire installed nsb2
RECORD inventory, the SkyCalc input checksum and the Python dependency inventory.
Changed remote assets are new input identities, not an exact historical replay.
Use `offline_data=True` only after caching all inputs. IERS automatic network
updates are disabled, and the pinned Astropy/IERS data version is recorded.

### Source development mode (works without a new NSB release)

Select a **full 40-character clean commit**, independently of the validation
checkout. The example pins the main revision inspected when this notebook was
implemented; it is a *new* experiment, not the historical executable:

```bash
NSB_COMMIT=f5e1d22689ed417c0dcf9df72da54365c30f5bd5
git clone https://github.com/VPRamon/NSB.git "$HOME/nsb-validation-source"
git -C "$HOME/nsb-validation-source" checkout --detach "$NSB_COMMIT"
uv run --no-sync nsb-validation-runtime source \
  --source "$HOME/nsb-validation-source" --commit "$NSB_COMMIT" \
  --wheel-dir "$HOME/.cache/nsb-observational/wheels-current" \
  --receipt "$HOME/.cache/nsb-observational/current.receipt.json"
```

Rust (the repository's supported version), Cargo and uv must be on PATH. The
command invokes the repository's Maturin workflow with `--release --locked`,
installs only that wheel into this isolated environment and writes a receipt.
It checks clean Git HEAD and Cargo.lock before and after building; records rustc,
Maturin, wheel and installed file hashes; and verifies the actual imported native
extension against the wheel. Retain the wheel and receipt. A receipt is local
build evidence, not an externally signed scientific attestation. Existing
receipts/wheels are never overwritten: use a new destination for a rebuild.
Older historical revisions may require their original Python provider versions;
the current environment is not advertised as a universal historical build graph.

Set the notebook's `Config(mode='current', receipt=..., nsb_commit=...)` to those
values. No install, Git checkout, pull or remote code execution occurs during
analysis. Restart the kernel after changing the installed NSB wheel.

### Published-package mode

At implementation time (24 October 2026), the PyPI JSON registry reported
`nsb-py` **0.1.1** as latest; no 0.2.0 publication is assumed. When a specific
Oxygen-capable version becomes available, prepare it explicitly:

```bash
# pip is used only for platform-wheel selection in this explicit setup command.
uv pip install --python .venv/bin/python pip
uv run --no-sync nsb-validation-runtime published --version VERSION \
  --wheel-dir "$HOME/.cache/nsb-observational/wheels-published-VERSION" \
  --receipt "$HOME/.cache/nsb-observational/published-VERSION.receipt.json"
```

The command checks that exact version's PyPI JSON, verifies its wheel digest,
installs it and requires both Oxygen and restricted-band APIs. An incompatible
version fails with source-build instructions. Set `runtime_mode='published'`,
`nsb_version='VERSION'` and its receipt in the notebook. A PyPI identity never
becomes a claimed Git source identity. This environment pins siderust 0.5.1 and
tempoch 0.5.0; a future package requiring another graph needs a separately reviewed
lock update, not an implicit dependency upgrade.

## Scientific protocol

[Patat (2008), A&A 481, 575](https://www.eso.org/~fpatat/science/skybright/)
provides flux-calibrated FORS1 spectra, in `erg s^-1 cm^-2 A^-1 arcsec^-2`.
The on-demand archive is about 18.3 MB:

- [ESO original](https://www.eso.org/~fpatat/science/skybright/DATA/skyspectra.tar)
- [Registered historical fallback](https://web.archive.org/web/20160329190127id_/http://www.eso.org/~fpatat/science/skybright/DATA/skyspectra.tar)
- Full SHA-256: `1f615fccb6dd464ab95b91f34adb4ba8ba3f4beebe97559204483c1da2c9965c`

An invalid existing cache is refused; explicitly quarantine it before retrying.
If both hosts fail, the campaign stops with their errors. No replacement dataset
is selected. Archive members are validated before any write: links, directories,
paths, duplicates and oversized members are rejected. Only flat FITS products
are written, and every extracted byte and inventory entry is reverified on reuse.
Store data outside the repository (default `~/.cache/nsb-observational`).

The observable is **ground-level photon surface radiance, 365–650 nm**, in
`photons cm^-2 ns^-1 sr^-1`. The measurement uses `I_ph = I_E lambda/(h c)`;
1 A = 1e-8 cm, 1 ns = 1e-9 s, and one steradian contains
`(180*3600/pi)^2` square arcseconds. Integrate wavelength times linearly sampled
energy radiance exactly, with interpolated band edges; retain noisy negative
spectral samples. This implementation uses Simpson quadrature for the quadratic
product; it is independent of the private reference's basis-weight algorithm.
No magnitude, telescope efficiency or detector counts enter the observable.

Frozen cuts: 300V/FREE, DARKFLG=0, Sun <−18°, Moon <−5°, 1≤airmass≤1.5,
|Galactic latitude|≥20°, original exposure≥60 s, full band coverage. Preserve the
103 filename identities, 22 integer-MJD nights, and five calibration nights
51643, 51725, 51761, 51785, 52847. A registered hash over sorted identities,
nights and split rejects selection drift. The remaining 94 spectra/17 nights
are the **historical evaluation partition**, already examined during Oxygen
development. Per-ID exclusions are exported. No threshold is fitted to residuals.

Models are explicitly `PARANAL_PALACE_V1`, `PARANAL_PALACE_OXYGEN_V1`, and nsb2.
NSB uses public restricted-band Zodiacal/Airglow/Moon APIs; Moon must return
physical zero. NSB Starlight has no restricted spectral API: retain the original
research-only Rieke photon fraction (365–650)/(300–650) applied to the production
integrated map. This is a disclosed SED assumption, **not an admitted spectral
Starlight product**. No production asset is modified. nsb2 uses public source
factories and `query_direct`, with the frozen source-grid integration convention
or public photometric resolver where its source normalization requires it.
Its Zodiacal path has no additional NSB extinction; its faint stellar population
differs from NSB's. Identical component names do not imply physical equivalence.

Native solar inputs retain NSB's resolver and nsb2's rounded Patat SUNFLUX.
Matched measured inputs use the complete centered 27-day observed NRC/Penticton
noontime F10.7 windows from the [NOAA mirror](https://www.ngdc.noaa.gov/stp/space-weather/solar-data/solar-features/solar-radio/noontime-flux/penticton/penticton_observed/listings/listing_drao_noontime-flux-observed_daily.txt),
SHA-256 `7240df111ea45f26057b89d3e9880a58856d7b6d040ad16247db41f5546dac96`.
A growing mirror may change: changed bytes are refused. An optional externally
supplied frozen factual JSON slice is accepted only with SHA-256
`cd44cf6f2e162b763cfe3a216402f4a18d6b2ca88d05907496c2c2c738d225dc` and the
registered parent identity. Missing/unverified solar data disables the matched
scenario explicitly. Nothing is interpolated or fit to sky residuals. nsb2's
frozen scalar solar law is rescaled without rounding measured f27, matching the
original diagnostic. PALACE's 67–166 sfu training domain is shaded and extrapolation
counted (historically 79/103). Observed/adjusted training-convention equivalence
is not asserted.

The baseline is the **median of calibration spectra only**. Scores give each
spectrum equal weight; the reference calls this “exposure-weighted”, meaning
weight per exposure, **not exposure-duration weighting**. Resample whole nights,
with replacement, 2,000 times, NumPy default_rng seed 182. All models share each
draw. Percentile 95% CIs cover MAE/relative MAE and every paired difference,
including the baseline. Report signed bias, MAE, RMSE, medians and relative errors.
The frozen practical relative-MAE difference threshold is 0.10: the entire paired
CI must exceed that margin for a practical directional decision. This is still
development evidence and never grants independent admission. No model is scored
on an opportunistic intersection. Any missing, duplicate or
nonfinite prediction invalidates the entire scenario; rejected IDs are retained.
Intervals condition on the fitted baseline and omit shared calibration systematics.

## Reports and interpretation

Each run creates `validation-results/run-<UTC>-<uuid>/` exclusively, with
Markdown/HTML report, metrics, admission, observations, component predictions,
configuration, spectral diagnostic, PNG/SVG figures, and a complete manifest.
`manifest.sha256` covers the manifest; its inventory covers every other file.
Reruns never overwrite prior results. Verification rejects modified, missing,
extra or symlink artifacts. This detects mutation, not malicious replacement of
both a manifest and its detached digest. Preserve a trusted manifest digest
separately for archival use.

Eight figure families cover predictions, error intervals, signed distributions,
all paired Oxygen improvements/degradations, components, solar sensitivity,
oxygen-feature regions, and night means. The spectral example shows **observed
total on air wavelengths** beside **model Airglow integrated in 1 nm vacuum
bins**. These are not identical physical quantities or instrument-convolved line
profiles. PALACE has four vacuum transitions near 557.89, 630.20, 636.55, 639.35 nm;
the notebook uses the real NSB implementation rather than approximating them.

Oxygen remains experimental. Other emission species, molecular absorption,
solar extrapolation, Starlight population/SED assumptions, transport differences,
Paranal-only dark conditions and no independent post-development cohort remain
limitations. Matched historical relative MAE 0.16573 versus nsb2 0.17425 does
**not** establish superiority: its paired CI contains zero. The baseline was
0.15245. None is a newly validated score for NSB 0.2.0. See the
[scientific evidence and maturity statement](../../docs/nsb_components/airglow/validation/oxygen-and-calibration-v1.md).

## Independent provenance and historical checks

Inspected nsb-validation main `3191bfc7597b5a2add95456aeb2ea1b906979d22` and
development evidence `4c557bff800250ee48a690efd96b6e362b121fde`, including Patat
selection, photometry, statistics, spectral adapters and Oxygen diagnostic.
The repository is private with no declared license/distribution permission.
**No private source is copied, imported, packaged or required.** No separately
licensed distributable backend was available. This small implementation is
written independently from the registered scientific protocol and public model
APIs. Only factual registration hashes, counts, partition and already-public
rounded scientific results are included. NSB's AGPL-3.0-only covers this code.

`python -m nsb_observational.reference_check` optionally checks user-supplied
historical artifacts (never downloaded from private services). Exact SHA-256
pins are required. It checks every ID, split, measured radiance, component,
residual, aggregate score and paired bootstrap interval. Measurement tolerances
are absolute 1e-12 in radiance, appropriate to floating roundoff for the same
linear measurement; these are not physical accuracy tolerances. Fresh model
components from a newer source are compared and differences reported; they do
not pass an “identical executable” gate merely because numbers agree. Historical
binary SHA `e6f191…945f6` belongs to numerical revision `168e73…c305`, whereas
`57584e…e9a9a` is its later merge; baseline `1b2556…582ec` is a third identity.
The public historical mode displays evidence only. A byte-identical historical
replay is unavailable without its exact artifacts and original build environment.
See [VERIFICATION.md](VERIFICATION.md) for the checks actually performed here.

## Offline tests and manual real-data execution

```bash
uv run --no-sync pytest -q
uv run --no-sync nsb-observational --mode offline
# current.json contains the same Config fields as the notebook configuration cell
uv run --no-sync nsb-observational --config current.json
```

The offline test executes the complete output-cleared notebook with nbclient,
uses only synthetic fixtures and checks report integrity. Ordinary CI neither
fetches observations/nsb2 nor compiles NSB. A separate workflow-dispatch job
builds an explicitly selected exact NSB source and runs the real campaign; all
scientific/data downloads occur only on that manual path. No release workflows
are changed. Use `uv run --no-sync` after installing an NSB wheel so uv does not
remove this intentionally separately built runtime.

The nsb2 stellar path uses ESA Gaia data processed by DPAC, whose funding is
provided by national institutions participating in the Gaia Multilateral Agreement.
HEALPix/healpy are used by its stellar map. Publications should cite Patat (2008),
Roellinghoff, Spencer & Funk (2025, A&A 698, A212), the nsb2 repository and the
original emission/solar/template sources; see [nsb2 acknowledgements](https://gerritro.github.io/nsb2/acknowledgements.html).
