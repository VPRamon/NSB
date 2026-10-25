# Scientific validation and limitations — NSB 0.2.0

**Status:** Current scientific interpretation of the v0.2.0 runtime.
**Audience:** Astronomers, observers, scientific reviewers and integrators.
**Scope:** Model assumptions, numerical and observational evidence, uncertainty,
applicability and permitted interpretations of model output.

## What NSB predicts

NSB estimates optical night-sky **photon radiance** for an observer, UTC instant
and sky direction; it can also search for observation windows constrained by
a modelled radiance threshold. The default integrated band is **300–650 nm**
and the result unit is **photons cm⁻² ns⁻¹ sr⁻¹**. This is not a detector count,
a measured photometric passband magnitude, an exposure-time recommendation or
an absolute site-calibrated sky spectrum.

The runtime composes four separately identified sources: zodiacal light,
integrated stellar light, airglow and atmospherically scattered moonlight.
Model configuration, site conditions, component mask and time/frame conventions
are part of the experimental setup, not incidental implementation details.
For formulas see [scientific model](specifications/scientific-model.md); for
machine-readable metadata see
[scientific metadata](specifications/scientific-metadata.md).

## Evidence and fitness for use

| Scope | Current evidence | Allowed interpretation |
| --- | --- | --- |
| Evaluator composition and units | Numerical and end-to-end regression tests | Software behaviour within the tested input domain, not empirical sky accuracy |
| Zodiacal light | Leinert table anchors, Noll attenuation regressions | Generic/planning reference under documented assumptions |
| Airglow PALACE continuum | Pinned upstream continuum, regeneration and numerical comparisons | Empirical planning model; not globally or CTAO site-calibrated |
| Automatic PALACE Oxygen | Continuum plus Og/Or lines, deterministic equivalence to explicit Oxygen, development comparisons | **Experimental** planning estimate, standard continuum geometry and zenith distance **≤60°** |
| Moonlight | Jones 2013 spectral regressions; KS91 published V-band reference case | Model comparisons and planning; no full instrument calibration |
| Bundled Starlight | Checksum-pinned Gaia-derived 300–650 nm HEALPix map, numerical and cross-implementation checks, scoped data redistribution decision | Source-specific validated data product; independent astrophysical truth is **not** established by cross-implementation agreement |
| CTAO planning profiles | Explicit geographic/site assumptions | Hypothetical planning configuration, **not** CTAO-N or CTAO-S observational calibration |
| Spectral scattering and transport | Analytic, SciPy and synthetic independent numerical references within bounded configurations | Experimental numerical method, not complete multiple scattering or empirical sky admission |

The exact supported models and their maturity identifiers are defined in
[model maturity](specifications/model-maturity.md). An experimental component
is not promoted by summing it with other components or by selecting an
observatory's coordinates.

## Observational comparisons and important non-results

**Patat/FORS1 benchmark.** A previously evaluated NSB revision
(`1b2556727205694c7765ef0d254c5d1b301582ec`, **not v0.2.0**) had
a registered observational cohort of 103 spectra on 22 nights. A held-out
comparison included 94 spectra on 17 nights over 365–650 nm. Reported relative
mean absolute errors were about **0.361** for the evaluated NSB model,
**0.171** for nsb2 and **0.152** for the calibration-night baseline. The
comparison requires explicit spectral and Starlight compatibility assumptions.
This result must not be represented as the accuracy of today's default Oxygen
configuration.

**Oxygen development.** The same inspected observational cohort informed
Oxygen model development. Any improvement measured on those observations is
development evidence, not a statistically independent out-of-sample admission
of v0.2.0.

**DESI pilot.** Seven passbands across three nights do **not** constitute 21
independent nights. Instrument response, diffuse calibration, spectral
coverage and equivalent-observable mapping limit interpretation. An absolute
sky accuracy claim is not admitted.

**LVM.** Available reconstruction and acquisition work does not yet establish
complete diffuse absolute calibration, an uncertainty budget or observational
equivalence for model admission. No independent campaign here establishes
CTAO site calibration or overall absolute night-sky predictive accuracy.

**Independent Starlight literature checks.** Published diffuse-sky reference
surfaces may include diffuse Galactic light, zodiacal, airglow or extragalactic
background. They are not automatically admissible comparisons to *stellar
top-of-atmosphere* radiance alone. The
[Starlight validation report](nsb_components/starlight/validation/results/validation-report-v1.md)
records that its literature-grid route found no admissible independent
reference; separate
[cross-implementation metrics](nsb_components/starlight/validation/results/issue-207-external-cross-validation-v1.json)
support a narrower claim. Some original reports are tied to older source bytes
and schema versions: their checksums are provenance, not proof of a rerun against
the v0.2.0 release.

## Domains, fallbacks and uncertainty

- Automatic Airglow resolves to experimental `ParanalPalaceOxygenV1`.
  Standard continuum geometry and zenith distance **≤60°** are required.
  Out-of-domain queries fail explicitly rather than silently switching models.
- `ParanalPalaceV1` is a supported continuum-only model with broader geometric
  applicability. Selecting it changes the scientific assumptions; record the
  choice whenever comparing observations.
- CTAO site presets are planning assumptions, not measured atmospheric
  calibration. Source coordinates do not calibrate a model.
- B/V and S10 results use central-wavelength diagnostic conventions; they
  are **not** validated Johnson bandpass integrations.
- Reported uncertainty fields depend on the component and data product.
  Unknown/missing error terms must not be interpreted as zero or as a complete
  covariance budget. See [scientific metadata](specifications/scientific-metadata.md).
- Band-integrated Starlight cannot be treated as a native wavelength-resolved
  source for arbitrary-band transport without explicit spectral data and domain
  validation.

## Reproducibility and scientific citation

For a result intended for publication, record the NSB release and commit,
Siderust/Tempoch versions, query time and scale, observer coordinates and height
datum, target/frame, component selection, site profile, solar-activity source
or override, model identifiers and maturity, spectral band, data/manifest
checksums, uncertainty/status fields and any custom calibrations.

Run the minimal public-interface examples from
[the examples index](../examples/README.md). The
[observational notebook](../examples/observational-validation.ipynb) contains a
registered Patat/FORS1 acquisition and comparison protocol; raw observations
are not redistributed inside this repository. Internal and independent
numerical tests are specified in the [validation matrix](specifications/validation.md).
For a new independent observational claim, use the
[validation consumer contract](specifications/validation-consumer.md), with
held-out observations and a matching instrument/observable transfer function.

Cite the NSB software using [CITATION.cff](../CITATION.cff), the exact version
and any upstream datasets or methods necessary for the claim. Software tests,
cross-implementation agreement, signed distribution decisions and empirically
calibrated validation each answer different questions and should never be
conflated.

## Data rights

The software is AGPL-3.0-only. Some scientific assets carry distinct terms,
including Gaia-derived data under CC BY-NC 3.0 IGO. Source attribution and
noncommercial restrictions are not waived by NSB's code licence. See
[third-party notices](../THIRD_PARTY_NOTICES.md) and
[Starlight source terms](nsb_components/starlight/licensing/README.md).
