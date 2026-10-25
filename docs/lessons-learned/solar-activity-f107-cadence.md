# Solar activity — operational F10.7 cadence vs retrospective measurements

**Mistake to avoid:** A retrospective fit gain, even with a nominal one-day-lag proxy, is not proof of improved prediction-time accuracy. Keep historical publication availability, the PALACE training domain, and independent validation separate.

**Decision:** Retain the existing resolver default. Require genuinely time-available inputs and independent, in-domain paired validation before reconsidering.


**Outcome:** B — no demonstrated, defensible operational improvement  
**Issue:** [#295](https://github.com/VPRamon/NSB/issues/295)  
**Related:** [#280](https://github.com/VPRamon/NSB/issues/280), [#294](https://github.com/VPRamon/NSB/issues/294), [#296](https://github.com/VPRamon/NSB/issues/296); oxygen metrics in [`docs/nsb_components/airglow/validation/oxygen-and-calibration-v1.md`](../nsb_components/airglow/validation/oxygen-and-calibration-v1.md); resolver contract in [`docs/nsb_components/airglow/f107-resolver.md`](../nsb_components/airglow/f107-resolver.md)  
**Evidence artifact:** [`artifacts/solar-activity-f107-cadence-evidence.json`](artifacts/solar-activity-f107-cadence-evidence.json)  
**Baseline commit for the experiment:** `bcbeaf73c423b5ef0fc8c34ad08a47d0afb0c1d3` (`main` at investigation time)

## 1. Original hypothesis

The Patat/FORS1 development benchmark shows a large gap between NSB Automatic with
native solar resolution (~23.9% relative MAE for Oxygen) and the same physical
model with a retrospective centred 27-day observed F10.7 (~16.6%). The
hypothesis under test was:

> A scientifically consistent, **causally available** solar-activity input can
> improve NSB predictions without changing or recalibrating the Airglow model.

A negative or inconclusive result is preferred to an unjustified default change.

## 2. Why the investigation was necessary

Centred 27-day means use future daily observations and must never be implied to
be available at prediction time. Interpreting the matched-scenario improvement
as an operational gain would overstate NSB's real-time skill and could motivate
an unsafe resolver change. Issue #295 requires separating prospective inputs
from retrospective diagnostics and quantifying whether any causal alternative
helps under controlled conditions.

## 3. Current NSB behaviour (baseline audit)

| Property | Finding |
|---|---|
| Default source | `SolarActivitySource::Automatic` → bundled offline `f107_store.json` |
| Quantity | Monthly-averaged planning F10.7 (Noll/SkyCalc `msolflux` lineage), **not** raw daily |
| Units / convention | sfu; `penticton-f107-sfu-as-reported-by-noaa-swpc` |
| Precedence | Explicit → finalized monthly observed → provisional observed+forecast (full month only) → complete 45-day forecast month mean → official monthly prediction → climatology ≈ **129.207 sfu** |
| Time-validity rule | Forecasts issued after the requested instant are excluded. Historical finalized monthly observations can nevertheless be selected from a later snapshot, even if unavailable at that instant. |
| Daily rows in store | Present for diagnostics only; **never** selected as the Airglow input |
| Bundled coverage (snapshot `bundled-2026-08-27`) | Monthly observed **2023-08 … 2026-07**; daily observed **2026-07-28 … 2026-08-26**; no Patat-era (1999–2003) monthly indices |
| Patat-era Automatic | Resolves to **climatology** for all registered cohort dates (verified) |
| PALACE physics | Continuum/Oxygen scales are linear about 100 sfu: `1 + 0.01·SCE·(F−100)`; training evidence uses **centred 27-day** averages spanning **67–166 sfu** |
| Historical replay | Automatic has no separate retrospective mode, but it may select a later-published finalized monthly observed record for an earlier requested instant. Historical evaluations need explicit provenance labels. |
| Siderust / Tempoch | No F10.7 solar-index helpers; time/geometry remain there, solar selection stays in NSB |

Native versus matched notebook scenarios therefore compare **climatology** against
**centred 27-day observed F10.7** on this cohort, not “monthly planning mean”
against that training statistic. That coverage fact is essential when reading
published relative MAE figures (23.88% vs 16.57%).

## 4. Candidates investigated

Definitions were frozen **before** interpreting final metrics.

| ID | Definition | Available at observation UTC date \(T\)? | Role |
|---|---|---|---|
| `operational_automatic` | NSB Automatic resolver (no override) | Patat cohort: climatology available in advance; not generally causal for historical monthly observations from later snapshots | Operational baseline on this cohort |
| `trailing27_latency1` | Mean of observed daily F10.7 on \([T-27,T-1]\) | Conditional on every included daily record having been published by prediction time; publication timestamps not checked | Simulated lag-one-day research candidate |
| `previous_month_mean` | Mean of all observed dailies in the previous calendar month | Conditional on final daily publication, which may lag the beginning of the current month | Research candidate |
| `latest_daily_latency1` | Observed daily F10.7 on \(T-1\) | Conditional on the prior-day observation actually having been published | Simulated lag-one-day research candidate |
| `same_month_finalized_diagnostic` | Mean of all dailies in the calendar month of \(T\) | **No** mid-month; would match designed Automatic **after** month finalization if the store held historical monthly indices | Coverage diagnostic |
| `centered27_diagnostic` | Mean on \([T-13,T+13]\) | **No** — uses future days | Retrospective reference only |

**Publication caveat:** The one-day calendar lag is an *assumption*, not a reconstruction from issuance timestamps. Candidate inputs may have arrived after the observation instant. Hence these are simulated causal candidates, not verified real-time forecasts.

**Rejected as an automatic substitute for PALACE coefficients without evidence:**
treating a trailing mean as interchangeable with the centred training statistic
merely because both average 27 days. Compatibility with the linear SCE response
was tested empirically rather than assumed.

Data source for daily series: NOAA-mirrored NRC/Penticton observed noontime flux,
SHA-256 `7240df111ea45f26057b89d3e9880a58856d7b6d040ad16247db41f5546dac96`
(coverage through 2018-04-30 in the pinned file; sufficient for the Patat era).

## 5. Experiments performed

Controlled Patat development cohort (103 spectra / 22 nights; 94 / 17 evaluation):

- Identical coordinates, times, 365–650 nm band, Zodiacal + Airglow + Starlight
  (registered Rieke 365–650 / 300–650 fraction) + explicit Moon=0 check.
- NSB continuum (`ParanalPalaceV1`) and NSB Automatic Oxygen
  (`ParanalPalaceOxygenV1` via current Automatic policy).
- **Only** the solar scalar changed; no Oxygen/continuum refit.
- Statistics: absolute MAE, relative MAE, signed bias, night-weighted MAE,
  night-clustered bootstrap CIs (2000 draws, seed 182) — same conventions as
  the observational notebook / PR #293 helpers.
- Stratification by centred-F27 training membership and airmass.
- Decision rule frozen in the evidence JSON: a candidate is adoptable only if
  causal, and Oxygen paired relative-MAE versus `operational_automatic` has a
  night-clustered 95% CI entirely below zero **without** relying solely on
  undisclosed out-of-domain extrapolation.

Cohort status: **development / conditional evidence**, not an independent
post-development holdout (see #280).

## 6. Numerical results (Oxygen, evaluation partition)

Radiance units: photons cm⁻² ns⁻¹ sr⁻¹.

| Solar scenario | Rel. MAE [95% night CI] | MAE | Night-weighted MAE | Signed bias |
|---|---:|---:|---:|---:|
| Operational Automatic (climatology) | **23.88%** [20.70, 27.89] | 0.08146 | 0.08227 | −0.08146 |
| Trailing 27-day (\(T-27…T-1\)) | 14.80% [11.28, 21.01] | 0.05173 | 0.06195 | −0.04797 |
| Previous-month mean | 14.55% [11.08, 20.62] | 0.05103 | 0.05961 | −0.04721 |
| Latest daily (\(T-1\)) | 17.38% [13.52, 23.79] | 0.06007 | 0.07082 | −0.05857 |
| Same-month finalized (diagnostic) | 16.55% [13.09, 22.53] | 0.05720 | — | −0.05549 |
| Centred 27-day (diagnostic) | **16.57%** [12.97, 22.61] | 0.05750 | 0.06683 | −0.05588 |

Operational and centred figures reproduce the published Oxygen native/matched
benchmark. Continuum-only relative MAE stays ~34.6–36.1% across scenarios
(solar sensitivity is weak for continuum totals on this band).

**Solar inputs on the 94 evaluation spectra:**

| Scenario | Mean sfu | Outside 67–166 sfu |
|---|---:|---:|
| Operational (climatology) | 129.21 | 0 / 94 |
| Centred 27-day | 184.74 | 73 / 94 |
| Trailing 27-day | 202.24 | 73 / 94 |
| Previous month | 203.86 | 77 / 94 |

Trailing / previous-month proxies sit **~17–19 sfu above** centred F27 on average
(97% / 83% of spectra higher than centred). They track solar-max activity, but
they are not centred-equivalent.

### Domain split (Oxygen relative MAE)

| Scenario | F27 ∈ [67, 166] (21 / 8 nights) | F27 > 166 (73 / 9 nights) |
|---|---:|---:|
| Operational | 23.95% | 23.86% |
| Trailing 27-day | 23.61% | 12.27% |
| Previous month | 21.85% | 12.44% |
| Centred 27-day | 24.01% | 14.43% |

**Independently regenerated within-domain paired relative-MAE differences:**
negative means lower error than Automatic; differences and endpoints below are
percentage points, calculated from paired whole-night draws (2,000; seed 182).

| Candidate minus Automatic | Difference [95% night CI], pp | Spectra / nights |
|---|---:|---:|
| Trailing 27-day | −0.337 [−2.616, +1.637] | 21 / 8 |
| Previous-month mean | −2.097 [−6.685, +0.761] | 21 / 8 |
| Latest daily | −0.473 [−1.688, +1.041] | 21 / 8 |
| Centred 27-day (diagnostic) | +0.060 [−2.055, +1.946] | 21 / 8 |
| Same-month finalized (diagnostic) | −0.258 [−2.465, +1.775] | 21 / 8 |

Every interval contains zero. The missing intervals have now been computed from
the checksum-verified spectra, rather than inferred from aggregate metrics.
This does not demonstrate equivalence or exclude a useful effect: eight nights
provide limited power. The much larger full-cohort gains are concentrated in
the extrapolated high-F27 majority of this development sample.

## 7. Why candidates did not establish a useful improvement

1. **Wrong physical reading of the headline gap.** Native Automatic on Patat is
   climatology, not a monthly observed index. Matched centred F27 raises Oxygen
   emission under solar-max conditions and reduces a systematic faint bias.
2. **Improvement ≠ better training-statistic fidelity.** Trailing/previous-month
   means beat centred F27 on the full cohort while being systematically *higher*
   than centred F27, i.e. they amplify experimental positive-scale extrapolation
   rather than recovering the PALACE fit statistic.
3. **No demonstrated within-domain improvement.** Inside 67–166 sfu,
   independently regenerated paired intervals all contain zero. The full bounds
   are archived, including the modest previous-month point improvement.
4. **Continuum barely moves.** Broadband continuum errors are almost insensitive
   to the solar proxy; Oxygen drives the scenario differences.
5. **Development-cohort dependence.** The Patat sample informed Oxygen
   architecture; 79/103 spectra (73/94 evaluation) lie above the training F27
   range. Full-cohort MAE reductions are not independent validation and must not
   drive a silent default change (#280, #294).

This is a **measured failure to justify adopting a new operational resolver**,
not a claim that no future proxy could ever help. Within-domain statistical
power is modest (8 nights), but the absence of a clear within-domain effect —
together with the out-of-domain compensation mechanism — is sufficient to reject
Outcome A for this evidence set.

## 8. Scientific and operational limitations

- Daily publication was approximated using a one-calendar-day lag, without historical release timestamps; real delays can be longer or product-dependent. Causality is conditional, not independently verified.
- Observed versus 1-AU-adjusted F10.7 convention equivalence with PALACE’s
  original fit is **not** asserted (existing docs).
- Bundled store staleness / missing historical monthly indices affect
  historical replay; they do not by themselves justify substituting trailing
  averages into SCE coefficients.
- No independent absolute Oxygen admission cohort (#280).
- nsb2 was not re-run; the question is internal to NSB solar inputs.

## 9. Lessons for future development

1. Always distinguish **genuinely prospective**, **historical with verified
   availability**, **hypothetically lagged**, and **retrospective centred** solar
   scenarios separately in tables and prose.
2. Never promote a matched-F10.7 notebook number as an operational forecast skill.
3. Stratify solar-input experiments by the **model’s training domain** before
   claiming resolver improvements (#294).
4. A trailing window is not automatically the PALACE training statistic.
5. Store coverage gaps (climatology fallback) must be diagnosed before cadence
   redesign.
6. Prefer explicit opt-in sources over silent default changes; default changes
   need migration notes and before/after evidence.

## 10. Conditions that would justify reopening

Reopen only with **new evidence**, for example:

- A preregistered comparison on nights with centred F27 **inside** 67–166 sfu
  and with trustworthy causal inputs, showing a night-clustered CI for paired
  relative MAE entirely below zero versus the true operational monthly path; or
- An audited causal forecast product whose definition matches the PALACE solar
  covariate closely enough to re-fit or formally transfer SCE coefficients; or
- Independent absolute radiometry (#280) that can separate solar-input error
  from Oxygen absolute scale.

Filling historical monthly SWPC indices into the offline store is a separate
data-engineering task. Even on this cohort, a same-month finalized diagnostic
(~16.6% rel. MAE overall) still shows **no within-domain gain** versus
climatology, so coverage extension alone would not demonstrate in-domain
scientific improvement.

## 11. Reproduction and independent audit

Inputs (checksum-pinned):

- Patat archive SHA-256 `1f615fccb6dd464ab95b91f34adb4ba8ba3f4beebe97559204483c1da2c9965c`
- Solar daily listing SHA-256 `7240df111ea45f26057b89d3e9880a58856d7b6d040ad16247db41f5546dac96`
- Cohort identity SHA-256 `3efdd3a0f3af19bfbc7615864f928b33fe22bcdbd6c67568fe317c059d44a1d6`

**Verified reproduction (2026-10-25):** All original primary metrics and
conditional relative MAEs reproduced with absolute tolerance `1e-6` in fractional
relative MAE / stated radiance units (the present run agrees exactly). The
original archive omitted within-domain paired intervals; these are now archived
under `verified_reproduction`, including all conditional model metrics and
paired bounds. No bootstrap-method correction was necessary.

Registered selection: `DARKFLG=0`, `300V`, `FREE`, Sun altitude <−18°,
Moon altitude <−5°, header airmass in [1, 1.5], |Galactic latitude| ≥20°,
exposure ≥60 s, and complete 365–650 nm coverage. The five calibration nights
(integer UTC MJD 51643, 51725, 51761, 51785, 52847) remain excluded from evaluation.
The inventory is 1,189 FITS files; selection retains 103 spectra / 22 nights,
with the unchanged evaluation partition of 94 spectra / 17 nights. Domain
membership uses centred F27 for every scenario, never the candidate's value.

Inputs and acquisition:

- [Patat/FORS1 original ESO archive](https://www.eso.org/~fpatat/science/skybright/DATA/skyspectra.tar)
  returned HTTP 404 during this audit. The notebook's
  [registered archive mirror](https://web.archive.org/web/20160329190127id_/http://www.eso.org/~fpatat/science/skybright/DATA/skyspectra.tar)
  returned the exact pinned bytes.
- [NOAA observed Penticton listing](https://www.ngdc.noaa.gov/stp/space-weather/solar-data/solar-features/solar-radio/noontime-flux/penticton/penticton_observed/listings/listing_drao_noontime-flux-observed_daily.txt)
  was retrieved and verified against the existing pin. Date-only rows denote
  missing observations; incomplete required windows fail instead of averaging
  available samples. Values are observed noontime sfu, not 1-AU-adjusted flux.
- [STScI Rieke solar spectrum](https://archive.stsci.edu/hlsps/reference-atlases/cdbs/grid/solsys/solar_spec.fits)
  was retrieved independently and pinned to SHA-256
  `2213b2c8a61d9aa147ecb88a19c0c64c134955494b07fa8ba7e3cb91660e3301`.
  The registered source-grid trapezoid photon integral reproduces the
  365–650 / 300–650 fraction `0.9296804289344205`.

Procedure (download inputs explicitly first; the runner disables Astropy network access):

1. Check out this PR branch. The scientific Rust/Python implementation is
   unchanged from baseline `bcbeaf73c423b5ef0fc8c34ad08a47d0afb0c1d3`.
2. Build and install the bindings from this checkout:
   `maturin build --release --locked --out /tmp/nsb-wheels`, then
   `python -m pip install /tmp/nsb-wheels/*.whl` in an isolated environment.
   Install NumPy, SciPy and Astropy. The verified run used Python 3.14.4,
   nsb-py 0.2.0, NumPy 2.5.4, SciPy 1.18.1, Astropy 8.0.2, siderust 0.5.1,
   and tempoch 0.5.0; Rust dependencies are fixed by `Cargo.lock`.
3. Run:

   ```sh
   python scripts/reproduce_f107_cadence.py \
     --patat /path/to/skyspectra.tar \
     --solar /path/to/solar-observed.txt \
     --rieke /path/to/solar_spec.fits \
     --output /tmp/f107-reproduced.json
   ```

4. Compare with `verified_reproduction` in the evidence artifact. The runner
   checks input hashes, cohort identity, counts, original primary and
   conditional metrics, actual Automatic climatology provenance, equivalence
   to explicit climatology, and invariant non-airglow component budgets. It
   records source hashes, code revision, extension hash and package versions.
   Identical inputs/environment produce identical JSON; no timestamps or
   input filesystem paths enter the result.

Bootstrap: sample 17 evaluation nights (8 for the training subset) with
replacement, retain every spectrum in each sampled night, and divide error
sums by the resampled spectrum count. All scenarios share draws. Subset
intervals resample within the subset using seed 182. Percentile bounds use
NumPy's linear 2.5th/97.5th percentiles. Relative MAE is the mean of per-spectrum
absolute relative errors, not the ratio of aggregate MAE to mean observed flux.
Intervals capture night-sampling variability conditional on the fixed model;
they exclude calibration systematics and have no multiplicity adjustment.

Normal CI runs synthetic statistics, window/integrity, evidence-consistency and
relative-link checks without downloading scientific inputs. The command above
is the separate full scientific reproduction. Neither NSB's runtime defaults
nor PALACE coefficients changed.
