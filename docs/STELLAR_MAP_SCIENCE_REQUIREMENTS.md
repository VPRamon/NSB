# Starlight science requirements

Status: normative production contract; no currently bundled Starlight asset
satisfies this contract.

Audience: scientific reviewers, release maintainers, and authors of generation
tools.

Related documents: [generation](STELLAR_MAP_GENERATION.md),
[validation](STELLAR_MAP_VALIDATION.md), [model maturity](MODEL_MATURITY.md),
and [external manifest](EXTERNAL_STARLIGHT_MANIFEST.md).

## Scientific definition

The NSB Starlight component is the **top-of-atmosphere, band-integrated photon
radiance of direct Galactic stellar light over 300--650 nm**. It comprises
catalogued Gaia stellar sources whose spectral energy distributions are
measured, sources whose spectra are inferred from calibrated photometry, and a
spatially and photometrically resolved correction for stellar sources missing
from Gaia. The runtime quantity is

```text
L_star(l,b) = dN_gamma / (dA dt dOmega), integrated from 300 to 650 nm
unit        = photons cm^-2 ns^-1 sr^-1
```

`L_star` is exo-atmospheric. Atmospheric transmission or scattering must be a
separate, explicitly selected operation and must not be baked into this map.
The map represents the epoch-averaged Gaia DR3 stellar population; it is not an
instantaneous variable-star sky.

Every contribution in the runtime sum must use this exact interval and
radiometric definition. Gaia DR3 XP sampled spectra begin at 336 nm, so their
direct 336--650 nm integrals are incomplete under the NSB contract until a
validated 300--336 nm estimate and its uncertainty have been added. Relabelling
a 336--650 nm integral as 300--650 nm, or adding Gaia electron-rate sums to a
photon integral, is forbidden.

## Component boundary

Included:

- direct light from Galactic stars resolved by Gaia and admitted by the source
  quality policy;
- inferred direct light from Gaia sources with continuous XP or only broad-band
  photometry;
- statistically inferred direct light from unresolved, saturated, crowded, or
  fainter Galactic stars, through a bounded selection-function model;
- unresolved multiplicity insofar as it contributes to the measured or
  statistically inferred system flux;
- epoch-mean variability in the Gaia mean photometry, with an additional
  variability uncertainty where applicable;
- line-of-sight extinction already imprinted on measured stellar fluxes. A
  model used to infer missing sources may use extinction as a predictor, but it
  must not de-redden observed light and then add the extincted light again.

Excluded and never repaired by the stellar incompleteness term:

- diffuse Galactic light, including starlight scattered by interstellar dust;
- unresolved nebular continuum and line emission;
- extragalactic background light and resolved galaxies or quasars;
- zodiacal light, airglow, scattered moonlight, and atmospheric extinction or
  scattering;
- Solar-System objects and transients not representing the epoch-mean Galactic
  stellar sky.

Gaia sources classified as galaxy, quasar, or Solar-System objects must be
excluded where a release field or a documented cross-match permits this. Where
classification is ambiguous, their retained contribution and an upper bound
must be reported. A selection correction must target the Galactic stellar
population only; it must not statistically reintroduce excluded diffuse or
extragalactic components.

Duplicated Gaia source records may not be counted twice. A source marked
`duplicated_source` is not silently discarded: the release pipeline must apply
a documented identity/deduplication rule, report the affected flux, and carry a
crowding or duplication uncertainty. Negative samples in an otherwise positive
XP integral remain part of the calibrated measurement. A non-positive signed
integral is never clipped into a positive measurement; it is audited and may be
replaced only by the explicitly less-informative photometric branch.

## Population branches

The release diagnostics must account for every Gaia DR3 row exactly once in one
of these branches:

1. `xp_sampled_measured`: externally calibrated XP sampled spectrum integrated
   over 336--650 nm, plus a calibrated 300--336 nm estimate;
2. `xp_continuous_reconstructed`: official BP/RP continuous coefficients
   reconstructed with the pinned GaiaXPy calibration bases, integrated in the
   same two sub-bands;
3. `photometric_g_bp_rp`: G, BP, RP, and colour model within its calibrated
   support;
4. `photometric_partial`: an explicit partial-colour branch such as G+RP, with
   wider uncertainty and an extrapolation flag;
5. `photometric_g_only`: G-only estimate with still wider uncertainty and a
   reported upper bound;
6. `no_usable_photometry`: no point estimate presented as measured flux; only a
   population correction or upper bound may represent it;
7. `scientific_exclusion`: individually traceable rejected measurement, with
   any alternative branch recorded rather than silently substituted.

The Gaia selection-function correction is a separate contribution. It must be
conditioned on sky position, magnitude, and at least the available colour or a
documented marginalisation over colour. It must respond to scanning-law depth,
crowding, proximity to bright sources, the Galactic plane and centre,
saturation, and the faint limit. A single all-sky multiplier is inadmissible.
Inverse-completeness weights must be positive, bounded by a preregistered cap,
and accompanied by the residual tail estimate beyond the effective Gaia limit.

## Spectral modelling requirements

The 300--336 nm correction must be calibrated against independently flux-
calibrated spectra that cover both sides of 336 nm. A linear extrapolation of
the first Gaia samples is not an admissible production model. Training must
cover blue and red stars, extinction, surface gravity or a documented proxy,
metallicity where material, and the quality range of the Gaia inputs. The
release reports 300--336 nm, 336--650 nm, and combined 300--650 nm performance
separately.

Continuous-XP reconstruction uses the official coefficient representation,
calibration bases, truncation information, standard deviations, correlations,
and GaiaXPy version pinned by checksum or package lock. A deterministic,
stratified overlap sample containing both sampled and continuous products must
demonstrate reconstruction accuracy before the continuous-only population is
admitted.

Photometric models are trained against held-out XP photon integrals, not Gaia
electron rates treated as physical targets. Their features, transformations,
coefficients, domain bounds, fallback order, and covariance are versioned data.
Training, validation, and test partitions are separated by source and by sky
cell to prevent leakage from crowded regions or duplicated objects.

## Uncertainty contract

Every production pixel provides non-negative, finite values for:

```text
statistical_uncertainty
systematic_uncertainty
total_uncertainty
```

The total is not formed by blindly adding all terms in quadrature. Source-level
random XP/photometric noise may be combined as independent only where the input
covariance permits it. Absolute calibration, 300--336 nm model error,
selection-function error, and other spatially correlated terms remain explicit
systematics and are propagated with their stated correlation scale. The budget
addresses XP covariance, continuous reconstruction, photometric inference,
selection and faint-tail incompleteness, crowding, calibration, variability,
spatial resolution, and extrapolation.

The runtime map may carry only the three aggregate uncertainty fields needed by
queries. Scientific sidecars retain component-level variances/covariances,
measured and inferred contributions, inferred fraction, completeness, upper
bounds, and flags. API, JSON, and CSV output expose a usable uncertainty for the
queried Starlight component.

## Pre-registered validation gates

Thresholds apply to a genuinely independent test set and to predefined sky
regions; passing training residuals is not evidence. A release may tighten
these gates, but it may not relax them after seeing the test results without a
new model version and rationale.

- absolute integrated all-sky bias: at most 3% of the independent reference;
- median absolute regional relative error: at most 5%;
- regional absolute relative-error 95th percentile: at most 10%;
- no predefined region may fail solely because its reference value is omitted;
- empirical coverage of nominal 68% intervals: 63--73%;
- empirical coverage of nominal 95% intervals: 90--98%;
- total-flux drift between admitted HEALPix resolutions: at most 0.1%;
- longitude-seam discontinuity: no statistically significant excess over
  adjacent longitude boundaries;
- no negative correction, unbounded weight, NaN, missing pixel, or unexplained
  branch loss;
- source accounting equals the pinned Gaia population plus documented
  exclusions, without duplicate source IDs.

Metrics are reported globally and by G magnitude, BP-RP colour, Galactic
latitude and longitude, crowding, extinction proxy, quality, inference branch,
and extrapolation status. Source-level reports include mean and median bias,
RMSE, MAE, robust relative error, percentiles 50/68/90/95/99, outlier analysis,
and interval coverage. Sky validation includes the plane, Galactic centre,
poles, high-latitude dark fields, 0/360-degree seam, high-extinction regions,
dense fields, and bright-star fields.

Independent references must be bandpass-compatible. If a reference also
contains diffuse Galactic, extragalactic, zodiacal, or atmospheric light, those
terms are separated with their own uncertainties before comparison. A
cross-band magnitude or radiance comparison without physical convolution is a
failed gate.

## Resolution and operational requirements

The final, fully corrected map is swept at `nside=64`, `128`, `256`, and `512`.
The selected resolution is the lowest resolution that meets the scientific
stability gates and the documented runtime budgets; it is not automatically the
largest nside. The sweep records size, load and lookup time, memory, empty
pixels, per-pixel support, flux conservation, regional stability, high-latitude
noise, bright regions, seam behaviour, and uncertainty-field stability.
Smoothing is off unless a separately specified kernel and angular scale improve
validated error while conserving flux.

## Reproducibility and admission

Generation is an offline release operation. Runtime operation must not contact
Gaia or require Python. All release inputs, calibration bases, model tables,
scripts, seeds, environment versions, commands, diagnostics, manifests, and
outputs are checksum-pinned. Large acceptance runs may remain outside Git, but
their immutable manifests and hashes are versioned. Small deterministic
fixtures exercise the same code path in CI.

A bundled production product requires positive, checksum-linked artifacts for
all of the following: missing-flux review, independent validation,
redistribution review, final nside review, and production manifest. The gate is
fail-closed on a missing file, incompatible schema, checksum mismatch, wrong
release or band, non-positive decision, provisional reference, or mismatched
map. A software agent may prepare evidence but may not claim a human or legal
approval.

Only after all gates pass may the registry use
`calibration_status = "production"` and `runtime_embedded = true`. Until then,
the existing manual seed and the 336--650 nm XP-sampled map remain experimental
or candidate products, and Starlight remains outside `ComponentMask::ALL`.

## Primary references

- Gaia DR3 documentation, release 1.3:
  <https://gea.esac.esa.int/archive/documentation/GDR3/>.
- Gaia DR3 XP processing and validation, De Angeli et al. (2023),
  DOI `10.1051/0004-6361/202243680`.
- Gaia DR3 XP external calibration, Montegriffo et al. (2023),
  DOI `10.1051/0004-6361/202243880`.
- GaiaXPy documentation and calibration software:
  <https://gaia-dpci.github.io/GaiaXPy-website/>.
- Empirical Gaia DR3 selection function, Cantat-Gaudin et al. (2023),
  DOI `10.1051/0004-6361/202244784`.
- Gaia data licence (CC BY-NC 3.0 IGO):
  <https://www.cosmos.esa.int/web/gaia-users/license>.
