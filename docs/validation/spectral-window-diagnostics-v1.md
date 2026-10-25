# Spectral window residual diagnostics (v1)

Status: Outcome B — restricted admissible comparison.
Audience: Scientific reviewers and maintainers.
Issue: [#296](https://github.com/VPRamon/NSB/issues/296).
Artifact: [artifacts/spectral-window-diagnostics-v1.json](artifacts/spectral-window-diagnostics-v1.json).
Runner: [`scripts/spectral_window_diagnostics.py`](../../scripts/spectral_window_diagnostics.py).

## Scientific question

Where, and by how much, does NSB disagree with measured night-sky spectra under
genuinely comparable wavelength windows and observational operators?

This work does **not** claim independent absolute Oxygen admission ([#280](https://github.com/VPRamon/NSB/issues/280)),
does not retune model coefficients, and does not publish a full-model spectral MAE
over inequivalent total-sky versus Airglow-only quantities.

## Phase A — Comparability matrix

| Comparison | Units | Frame | Operator | Observation components | Model components | Status |
| --- | --- | --- | --- | --- | --- | --- |
| Continuum-subtracted O I excess vs NSB Oxygen-only | photons cm⁻² ns⁻¹ sr⁻¹ | Obs air (Morton 2000); model vacuum | Sideband continuum 3–5 nm; integrate excess ±2.5 nm (combined Or: multiplet) | Local excess (O I + blends + continuum-fit residual) | PALACE Oxygen delta lines only | **Conditional** |
| Total-sky continuum-control bands vs NSB continuum Airglow | same | Obs air; model vacuum via Morton | Registered λ·I_E Simpson integral | Total sky | Continuum Airglow only | **Unsupported as Airglow residual** (descriptive means only) |
| Full spectral MAE: total sky vs Airglow density | — | mixed | — | Total sky | Airglow only | **Unsupported** |
| nsb2 isolated Oxygen groups | — | SkyCalc | — | — | Combined Airglow | **Unsupported** (no separable Oxygen channel) |

### Instrumental evidence (Patat / FORS1)

Registered cohort spectra use grism **300V**, filter **FREE**, dark flag clear.
Sample FITS headers show:

- `CTYPE1 = LINEAR`, air-wavelength sampling.
- `CDELT1 ≈ 2.601 Å/pixel` for 300V (≈0.260 nm/pixel).
- `BUNIT = erg/s/cm2/A/arcsec2`.
- **No** `RESOLUT` / FWHM keyword in the archive headers.

Literature FORS1+300V resolving power is approximately **R ≈ 440**
(FWHM ≈ λ/R ≈ 1.3 nm near 558 nm). That value is **not** taken from the FITS
files and is used only to justify finite-window leakage estimates. Model delta
lines are **not** convolved for the reported integrals: the comparison is
integrated excess versus integrated Oxygen contribution, not peak height.

### NSB spectral contract

PALACE Oxygen lines are vacuum integrated delta fluxes at 557.8887, 630.2046,
636.5536 and 639.35 nm (`airglow_palace_oxygen_v1.json`).
`integrate_band` includes a line fully when its vacuum wavelength lies in
`[lower, upper)`. Continuum and Automatic configurations are unchanged.

### nsb2

Public nsb2 Airglow is combined SkyCalc emission. Continuum and Oxygen cannot be
separated. Window-level nsb2 Oxygen residuals are therefore unsupported here.

## Phase B — Frozen windows

Windows and operators were fixed before inspecting final residuals.
Changing them after seeing errors would require a labelled exploratory analysis.

| Window ID | Kind | Vacuum bounds (nm) | Air bounds (nm) | Notes |
| --- | --- | --- | --- | --- |
| `oi_green` | Oxygen local excess | 555.3887–560.3887 | Morton of vacuum | Centre 557.8887 vac |
| `oi_630` | Oxygen local excess | 627.7046–632.7046 | Morton of vacuum | Localization only; may blend with 636 at FORS resolution |
| `oi_636` | Oxygen local excess | 634.0536–639.0536 | Morton of vacuum | Weak 639.35 falls just outside +2.5 nm |
| `oi_red_combined` | Oxygen local excess | 627.7046–641.85 | Morton of vacuum | Preferred red diagnostic; includes 630/636/639 |
| `cont_blue_400_430` | Continuum control | Morton of air 400–430 | 400–430 | Descriptive only |
| `cont_mid_450_500` | Continuum control | Morton of air 450–500 | 450–500 | Descriptive only |

Sidebands for single features: `[c−5, c−3] ∪ [c+3, c+5]` nm (air).
Combined Or uses outer sidebands of the 630 and 636 air centres.

Wavelength conversion: **Morton (2000) / IAU** optical vac↔air for dry air at
15 °C, 760 Torr; domain 2000–10000 Å.

## Phase C–E — Operators, comparisons, statistics

- **Observed Oxygen windows:** continuum-subtracted excess in air.
- **NSB Oxygen windows:** Automatic Airglow − continuum Airglow over vacuum bounds
  (Oxygen-only contribution). Native operational solar inputs and retrospective
  centered-27-day F10.7 diagnostics are labelled separately ([#295](https://github.com/VPRamon/NSB/issues/295)).
- **Signed residual:** predicted − observed.
- **Uncertainty:** night-clustered bootstrap, 2000 draws, seed 182 (same
  convention as the observational notebook / PR #293).
- **Relative MAE:** reported only when |observed| is stably nonzero.
- Calibration/evaluation membership, source coefficients, Oxygen strengths,
  F10.7 resolver defaults and registered broadband predictions are **unchanged**.

## Numerical findings

Registered broadband observation mean 365–650 nm remains **0.32425**
(unchanged cohort). Observed Oxygen excess means over all 103 spectra recover the
oxygen-and-calibration-v1 references: green **0.03055**, 630 **0.02644**,
636 **0.00873**.

Evaluation partition (94 spectra / 17 nights). Units: photons cm⁻² ns⁻¹ sr⁻¹.
Signed residual = predicted − observed. CIs are night-clustered 95% intervals.

| Window | Scenario | Mean obs | Mean NSB O-only | MAE | Bias [95% CI] |
| --- | --- | ---: | ---: | ---: | --- |
| oi_green | Native operational | 0.03092 | 0.01845 | 0.01360 | −0.01248 |
| oi_green | Matched F10.7 (diagnostic) | 0.03092 | 0.02545 | 0.00845 | −0.00548 |
| oi_630 | Native operational | 0.02694 | 0.01610 | 0.01368 | −0.01085 |
| oi_630 | Matched F10.7 (diagnostic) | 0.02694 | 0.02752 | 0.01137 | +0.00058 |
| oi_636 | Native operational | 0.00890 | 0.00539 | 0.00451 | −0.00351 |
| oi_636 | Matched F10.7 (diagnostic) | 0.00890 | 0.00921 | 0.00381 | +0.00031 |
| oi_red_combined | Native operational | 0.03651 | 0.02149 | 0.01853 | −0.01503 |
| oi_red_combined | Matched F10.7 (diagnostic) | 0.03651 | 0.03674 | 0.01510 | +0.00023 |

Exact intervals and within-training-domain splits are in the JSON artifact.

**Interpretation (hypotheses, not demonstrated causes):**

1. Under native operational solar inputs, NSB Oxygen-only underpredicts local
   excess in every Oxygen window (negative bias).
2. With retrospective centered F10.7, red windows move to near-zero mean bias
   while green remains low. This is consistent with solar-extrapolation sensitivity
   documented in [#295](https://github.com/VPRamon/NSB/issues/295), not operational skill.
3. Continuum-control means (total sky vs continuum Airglow only): observed
   0.01982 / 0.03997 versus matched NSB continuum 0.00596 / 0.01059 in
   400–430 / 450–500 nm. **Not** Airglow residuals — other sky components remain
   in the observations.
4. Relative MAE can remain large even when mean bias is near zero because
   night-to-night scatter is large relative to per-spectrum excesses.

Interpretation rules:

1. Conditional Oxygen-window metrics measure agreement of **local excess scale**
   with NSB Oxygen-only integrals. They are not total-model prediction error.
2. Continuum-control means compare inequivalent quantities and must not be read
   as Airglow MAE.
3. A wavelength-localized residual is a **hypothesis generator**, not proof that
   a named component is causally responsible.
4. Most Patat nights lie above the PALACE 67–166 sfu training range; matched
   F10.7 remains a retrospective diagnostic ([#295](https://github.com/VPRamon/NSB/issues/295)).

## Reproduction

Requires the checksum-pinned Patat archive and NOAA-mirrored observed daily F10.7
listing (same identities as the observational notebook). No new downloads are
introduced by the runner when local paths are supplied:

```bash
python scripts/spectral_window_diagnostics.py \
  --patat /path/to/skyspectra.tar \
  --solar /path/to/solar-observed.txt \
  --output docs/validation/artifacts/spectral-window-diagnostics-v1.json

python -m pytest scripts/tests/test_spectral_window_diagnostics.py -q
```

Synthetic tests cover Morton vac↔air conversion, band integration, LSF flux
conservation and finite-window leakage, radiometric conversion, continuum
subtraction, and statistical integrity — without requiring the observational
archive.

## Follow-up priorities justified by this work

1. Independent absolute Oxygen metrology ([#280](https://github.com/VPRamon/NSB/issues/280)) — required before any admission claim.
2. Instrument-documented LSF for FORS1 300V if peak-shape comparisons are ever needed.
3. Do not pursue full spectral MAE until Starlight exposes an admitted arbitrary-band spectrum and total-sky operators can be matched.
4. Keep 630/636 as a combined red diagnostic unless higher-resolution observations justify separation.

## What this is not

- Not a recalibration of Oxygen or continuum coefficients.
- Not evidence that NSB is superior to nsb2.
- Not CTAO site calibration.
- Not a replacement for the registered 365–650 nm broadband evaluation.
