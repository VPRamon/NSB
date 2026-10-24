# Experimental oxygen addition and registered continuum fits

## Scientific scope and immutable references

Baseline NSB main: `1b2556727205694c7765ef0d254c5d1b301582ec`.
Frozen nsb2 main: `bc9320db03fbff69997ce366c6e76a37339c5d00`.
Historical benchmark: [nsb-validation PR #16](https://github.com/VPRamon/nsb-validation/pull/16).
That result, its cohort, predictions and admission are preserved unchanged.
Numerical implementation: `168e73dae89060423eebc344871b0084d217c305`.
Diagnostic source: `c602006e83ec646bc10c705e46e9ae18c7c61a08` in nsb-validation;
NSB extension SHA-256 `e6f19103a9287d2efe0ef86d97d0a8173a50d9c786271ec50b91930949a945f6`.
The clean source checkout, frozen inputs and complete output inventory are in
[the machine-readable manifest](https://github.com/VPRamon/nsb-validation/blob/science/four-component-empirical-benchmark/reports/airglow-diagnostic-v1/manifest.json).
New machine-readable reports live under `reports/airglow-diagnostic-v1` in
that validation branch. Numerical results here describe development evidence,
not independent observational validation.

PALACE data: Noll, Schmidt, Hannawald, Kausch and Kimeswenger, v1.0,
[Zenodo 14064023](https://doi.org/10.5281/zenodo.14064023), October 2024.
Scientific equations: [Noll et al. 2025, GMD 18, 4353](https://gmd.copernicus.org/articles/18/4353/2025/),
Eqs. (1)–(3), oxygen discussion in Sect. 4.4 and climatologies in Sect. 6.

| Input | SHA-256 |
|---|---|
| PALACE.zip | `a42577ffee8f76d9ac3765a9a2811d766c30158cf74c0a364ebf7a726dd9a0dc` |
| palace_lines.fits | `280d45262d6fc14aac5c9e01ea79b96e001eb9d4702bf8272a0e47b9446f96a6` |
| palace_var.fits | `e9c754802d7bd1c6fb1b2c1577b1d6c2b29b86b314e65acfbe482a7711c14234` |
| Derived oxygen JSON | `7db1b18102eea3abbbef21ed602e681fbed454deb65cc7d0743b5c465a703fc5` |
| Patat TAR | `1f615fccb6dd464ab95b91f34adb4ba8ba3f4beebe97559204483c1da2c9965c` |
| nsb2 combined SkyCalc spectrum | `f3ac8d8260d3eafcf79ed80dc9cf2a04ecd852d30176ec44888764b491c4841e` |
| NOAA-mirrored NRC observed daily F10.7 | `7240df111ea45f26057b89d3e9880a58856d7b6d040ad16247db41f5546dac96` |

The PALACE README distinguishes **CC BY 4.0 FITS model data** from its GPLv3
program. Only data tables are read; no GPL program is copied, imported or run.
`AIRGLOW_DATA_NOTICES.md` accompanies wheels/source distributions. The original
Starlight notices, runtime map, immutable approval inventory and original
registry are untouched. A narrowly constrained `airglow-oxygen-manifest.toml`
supplement passes through the same build checksum/schema pipeline; it can add
only the experimental oxygen asset, never replace an admitted asset or add a
Starlight admission path. This preserves the byte-pinned historical registry.

Reproduce the derived data with Python, astropy and numpy:

```bash
curl -L https://zenodo.org/api/records/14064023/files/PALACE.zip/content -o PALACE.zip
python3 crates/nsb-data-tools/scripts/project_oxygen.py PALACE.zip \
  crates/nsb/data/airglow_palace_oxygen_v1.json
```

The generator verifies archive and individual input checksums before projection.
Only `isX=1`, `varID in {Og,Or}`, vacuum wavelength <650 nm is selected: four
lines with reference intensities 162.668167, 122.669319, 41.051975 and 0.018835 R.
Their sum is 326.408296797 R, or 0.025974 photons cm^-2 ns^-1 sr^-1 at unit
climatology/geometry/transmission. A rayleigh is an integrated line radiance;
`1 R = 10^-3/(4 pi)` in the runtime band units, without a further nm factor.

## Implemented physical addition

`AirglowModel::ParanalPalaceOxygenV1` explicitly adds oxygen to the unchanged
continuum model. As of NSB v0.2.0, `Automatic` resolves to
`ParanalPalaceOxygenV1` as the temporary Paranal-derived planning fallback
(release-policy default; not an independent absolute admission). Explicit
`ParanalPalaceV1` remains available for continuum-only historical
reproducibility. Python uses `PARANAL_PALACE_OXYGEN_V1`; CLI selection is
`--airglow-model paranal-palace-oxygen-v1` or `paranal-palace-v1`.

Vacuum wavelengths are 557.8887, 630.2046, 636.5536 and 639.35 nm. Og uses
97 km and Or uses 250 km thin-shell geometry, source-specific solar/month/time
factors and conservative residual sigma. Existing Noll wavelength-dependent
Rayleigh/Mie scattering is applied once, with the selected atmospheric column.
This Noll approximation is not a species-specific scattering solver, especially
for the 250 km layer. There is no line/continuum overlap in the selected source
products, and the continuum fit is never applied to lines. Adjacent band
intervals are half-open for delta lines to conserve flux at shared boundaries.
The API does not invent an instrumental line-spread function or continuous
resolved line spectrum. Monochromatic B/V diagnostics remain continuum proxies.

The extension is experimental, limited to zenith <=60 degrees and the standard
continuum geometry. It omits molecular absorption (the source reference optical
line transmissions are about 0.97–0.98) and other atomic/molecular families:
Na, N, OH and O2. The full PALACE list has 26,541 lines and 23 variability
classes; reproducing every family requires absorption, line shapes, distinct
heights and an audited independent reference comparison. A global offset cannot
replace that work. Positive-scale F10.7 extrapolation remains explicit
experimental behavior; **79/103** diagnostic spectra have centered 27-day flux
above PALACE's reported 67–166 sfu training range. These comparisons do not
validate that extrapolation.

## Controlled discrepancy diagnostic

All means below use 103 spectra from 22 nights. Radiance units are photons
cm^-2 ns^-1 sr^-1. The historical error partition has 94 spectra from 17 nights.

| Quantity | Mean |
|---|---:|
| Observed 365–650 nm total | 0.32425 |
| Original NSB continuum Airglow | 0.08055 |
| Oxygen + continuum, original solar resolver | 0.11992 |
| Continuum, matched Patat SUNFLUX input | 0.08316 |
| Oxygen + continuum, matched SUNFLUX | 0.14237 |
| Continuum, centered 27-day measured F10.7 | 0.08446 |
| Oxygen + continuum, centered 27-day F10.7 | 0.14507 |
| NSB / nsb2 Zodiacal | 0.08386 / 0.08250 |
| NSB / nsb2 Starlight | 0.03775 / 0.00340 |

Independent local spectral diagnostics integrate the observed excess over a
linear continuum fitted in sidebands 3–5 nm from each feature, with a central
+/-2.5 nm integration window. Fixed air-wavelength centers account for FORS's
convention; PALACE uses vacuum wavelengths. The observed mean oxygen excess is
0.03055 (green), 0.02644 (630 nm) and 0.00873 (636 nm): **0.06572 combined**.
Na-window excess is 0.00395. These are measured spectral features, not a
subtraction of NSB's broadband prediction. Blends, continuum-fit choice,
calibration and unresolved source contributions limit their interpretation.

The oxygen-window excess is about 54% of the original mean total deficit
(0.12210). This is a comparison of scales, **not a precisely identified causal
fraction**. The modeled oxygen addition is 0.03937 with original solar inputs,
or 0.06061 with matched measured F10.7, leaving a mean total deficit around
0.05758 under the latter assumptions. Missing oxygen contributes substantially;
the entire deficit cannot be assigned to lines. Missing other species and
continuum normalization remain distinct questions.

The nsb2 asset contains combined SkyCalc Airglow with no separately recoverable
continuum channel; subtracting NSB would not recover one. Its source-level
130-sfu oxygen-window excesses are 0.01713/0.01272/0.00345. These are different
geometry/solar conditions from the observations; the report separately scales
nsb2 using its frozen scalar solar law for matched comparisons.

Continuum-sensitive **total observed** radiance in 400–430 and 450–500 nm is
0.01982 and 0.03997; NSB matched-F10.7 continuum contributions are 0.00595 and
0.01058, while complete nsb2 Airglow contributions are 0.01685 and 0.02497.
These are different component quantities; other components and weak lines are
still present. They do not uniquely identify an erroneous continuum scale.
The oxygen addition leaves these bands bit-identical.

The measured solar reference is noontime **observed** NRC/Penticton flux,
mirrored by NOAA. Every centered 27-day window is complete; no interpolation or
Patat residual fit is used. This uses the terrestrial observed convention;
identity with PALACE's original observed/adjusted training convention is not
asserted. Centered 27-day versus NSB monthly climatology is an input treatment
comparison, not site calibration. Matched F10.7 changes continuum mean by only
0.00391, but increases oxygen's contribution by 0.02123 relative to native.

Season/month/time dependence is retained from PALACE's independent data, never
fitted to Patat. Its causal contribution cannot be separated from observational
conditions with this cohort. Geometry changes are limited to the independently
specified oxygen heights; atmospheric treatment stays fixed in the controlled
before/after comparison. The benchmark's nsb2 source and NSB propagated
radiances still differ in their propagation conventions. No causal error
percentage is assigned to those unresolved effects.

Starlight's mean source-population difference is 0.03435. Removing it from NSB
would **increase** the observed deficit. The admitted map combines source
populations; its integrated pixels cannot uniquely recover a G>=15 map or a
slit masking operator. Generation inputs preserve bright-star supplement
provenance, but excluding that supplement alone is not the same selection as
nsb2's magnitude cut. A separate magnitude-selected product must be generated
from catalog sources with its own admission and wavelength/transport validation.
The existing production map and research-only solar band-fraction proxy are
unchanged. No solar SED redistribution is admitted as a correction.

## Error metrics and interpretation

| Configuration | Relative MAE | MAE | RMSE | Median AE |
|---|---:|---:|---:|---:|
| Original NSB, native inputs | 0.36103 | 0.12139 | 0.13146 | 0.11488 |
| Oxygen NSB, native inputs | 0.23882 | 0.08146 | 0.09291 | 0.07423 |
| Original NSB, measured F10.7 | 0.35027 | 0.11807 | 0.12827 | 0.10952 |
| Oxygen NSB, measured F10.7 | 0.16573 | 0.05750 | 0.07229 | 0.04252 |
| Frozen nsb2, native SUNFLUX | 0.17087 | 0.05395 | 0.06423 | 0.04652 |
| Frozen nsb2, measured F10.7 | 0.17425 | 0.05500 | 0.06363 | 0.04968 |
| Historical training-night median baseline | 0.15245 | 0.05201 | 0.07084 | 0.03642 |

Night-cluster bootstrap, 2000 draws, seed 182: native relative MAE 95% intervals
are [0.3322,0.3953] before and [0.2070,0.2789] after oxygen. With measured F10.7,
oxygen NSB's interval is [0.1297,0.2261]. Paired oxygen NSB minus matched nsb2
relative MAE is -0.00852, 95% interval [-0.05709,+0.05576]; absolute MAE changes
in the opposite direction (+0.00250). **There is no demonstrated superiority.**
The simple baseline remains better in both primary error means. Native oxygen
worsens none of the historical 94 evaluation cases; all per-case results and
condition splits remain available rather than selecting favorable conditions.
Signed errors, bootstrap intervals and all paired comparisons are in metrics.json.

No real continuum calibration is fitted here: these spectra do not independently
isolate continuum from the uncertain stellar/foreground populations over the
full band, and their residuals have already informed architecture selection.
The runnable synthetic fit demonstrates parameter recovery, uncertainty,
serialization, reuse, domain checks and reserved-night isolation. It cannot
establish calibrated predictive accuracy on independent observational input.
New independent continuum observations, a preregistered fit and truly untouched
nights remain necessary. The extension is justified for explicit experimental
use; retaining the original production default is the recommended release policy.

## Performance and tests

`cargo bench -p nsb --bench airglow_calibration --locked -- --warm-up-time 0.1
--measurement-time 0.2` compares point, 365–650 nm band, initialization, artifact
replay and one-hour window paths on deterministic Paranal fixtures. Ten-sample
local Criterion estimates (source `37dbf93c83887bac33c4719b613329d128c9fcb2`):
continuum/oxygen point 11.59/11.54 ms, band 11.59/11.66 ms, initialization
121/123 microseconds, one-hour planning 13.63/12.91 ms. Registered+oxygen
point/band/window are 13.12/13.14/13.45 ms; artifact validation/replay takes 36.04 ms for three measurements. Timing noise
and overlapping intervals preclude claiming a speedup. These short local
measurements bound practical overhead, not general hardware performance.
The oxygen asset is about 32 KiB static JSON plus about 7 KiB parsed tables,
lazily initialized once; no per-query file/network access or callback occurs.

Tests cover fit recovery/covariance, duplicate and reserved nights, replay and
maturity tampering, schema/provenance/nonfinite/bounds rejection, site/date/
solar/band applicability, immutable trusted maturity, same Rust/Python/CLI
calculation, unchanged continuum-only regions, line flux conservation, component
aggregation and atmosphere/geometry compatibility. Canonical run outcomes are
recorded in the implementation PR and validation report. Scientific source
reproduction is opt-in and requires the small checksum-pinned PALACE archive;
ordinary Rust tests need no public downloads.


Local verification at the numerical implementation revision: fmt and all-feature,
all-target clippy pass; workspace tests 1064 passed/7 ignored; explicit end-to-end
5 passed and science-metadata 7 passed; Python 139 passed; the optional source
projection 1 passed using the verified PALACE archive. The public API snapshot and
historical SemVer gate pass while keeping the legacy evidence schema private.
GitHub CI status is recorded on PR #272; local results do not substitute for it.
