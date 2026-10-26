# Matched Starlight benchmark: NSB ↔ nsb2 (v1)

Status: **Outcome B** — restricted equivalent + conditional sky-direction benchmark.
Issue: [#298](https://github.com/VPRamon/NSB/issues/298).
Parent context: [#263](https://github.com/VPRamon/NSB/issues/263); transport follow-up [#304](https://github.com/VPRamon/NSB/issues/304) / [#187](https://github.com/VPRamon/NSB/issues/187).
Artifact: [artifacts/matched-starlight-nsb2-v1.json](artifacts/matched-starlight-nsb2-v1.json).
Runner: [`scripts/matched_starlight_nsb2_benchmark.py`](../../scripts/matched_starlight_nsb2_benchmark.py).

## 1. Scientific question

When NSB and nsb2 represent the same stellar sources over the same supported
wavelength interval with the same physical observation operator, how closely do
their predictions agree?

This work measures **cross-implementation agreement under an explicit contract**.
It is **not** independent empirical validation and does **not** retune Starlight.

## 2. Comparison contract

| Property | Contract |
| --- | --- |
| Primary observable | Top-of-atmosphere band-integrated photon radiance |
| Level-1 bands | Analytic operators; synthetic fixture 360–450 nm; imposed constant spectrum 360–450 nm |
| Level-2 band | **300–650 nm** (NSB integrated-map native band) |
| Atmosphere | **Excluded** from the primary benchmark |
| Geometry | NSB pixel mean; nsb2 faint-map point interpolation; catalog mean over the exact NSB pixel |
| Units | photons cm⁻² ns⁻¹ sr⁻¹ |
| Forbidden | Solar-colour proxy on NSB’s 300–650 map; arbitrary renormalization; treating missing populations as zero; silently dropping selected sources with invalid Pickles templates |

### Comparability matrix

| Comparison | Status |
| --- | --- |
| Synthetic `StarlightMap` band integrals vs analytic fixture | **Equivalent** (NSB↔analytic; not by itself NSB↔nsb2) |
| Imposed constant spectrum through NSB `StarlightMap` and nsb2 `SpectralGrid`/`Bandpass`/`integrate_wavelength` | **Equivalent** (genuine NSB↔nsb2 paired operators + analytic) |
| NSB bundled 300–650 vs nsb2 faint G>15 (same directions) | **Conditional** |
| NSB bundled 300–650 vs nsb2 faint+XHIP (optional faint+bright excludes XHIP) | **Conditional** |
| Source-ID matched XP vs Pickles (360–570 nm) | **Unsupported** |
| Solar-colour proxy narrowing of NSB 300–650 | **Unsupported** |
| Population-complete Gaia bright + XHIP union without common IDs/deduplication | **Unsupported** |

## 3. Source-population audit

### NSB production map (bundled)

- Band: integrated **300–650 nm** photon radiance (not a continuous spectrum).
- HEALPix: Galactic **RING**, NSIDE=128, TOA.
- Admission: **1 518 801 751** admitted + **292 908 075** excluded =
  **1 811 709 826** observed Gaia rows; **55** bright-star supplement records.
- Spectral route: Gaia XP / photometric inference + Hipparcos/XHIP-selected CK04.
- Runtime maps **do not expose Gaia `source_id`s**.

### NSB experimental XP 360–570 nm

- Frozen candidate SHA-256:
  `fe331343cb9797a1d871ae798c88d2ae8865ae39358e8d82b464a91b3cf87d5f`.
- Seven 30 nm bins; NSIDE=128; XP-only; experimental; **not bundled in Git**.
- Historical raw XP records 219 197 643 → 219 197 642 canonical rows.
- Unavailable in this checkout → XP spectral Level-2 **unsupported**.

### nsb2 (`bc9320db03fbff69997ce366c6e76a37339c5d00`)

| Channel | Selection | Geometry | Spectra |
| --- | --- | --- | --- |
| Bright catalog | G < 15 (~3.69×10⁷ sources with `source_id`) | ICRS points | Pickles + Gaia G/BP/RP |
| Faint map | G > 15 | ICRS NESTED NSIDE=1024 | Pickles from mean colour |
| XHIP supplement | 88 stars | ICRS points | Pickles from Johnson V/B |

Exact source-ID intersection against NSB runtime aggregates is **unsupported**.
The bright file contains 36,908,056 unique Gaia IDs, with measured G range
1.731607–14.999999. The faint map has no IDs or magnitude-boundary audit, so
its nominal G>15 selection does not demonstrate completeness at G=15.
XHIP has no shared Gaia ID column. Disjointness of XHIP and either Gaia asset
cannot be established here. The optional bright comparison therefore excludes
XHIP; faint+XHIP remains explicitly conditional on an unverified population union.
Missing populations are labelled unavailable — never assigned zero radiance.
Selected sources with no finite Pickles template rate reject the evaluation
rather than vanishing from a partial sum.

The runner verifies the **installed** nsb2 implementation against the pin via
nsb2-owned local git metadata or package `direct_url` VCS info (no network).
Dirty nsb2 checkouts refuse by default. Installed package inputs and assets are
also fingerprinted. Matched runs
record `evidence_status=pinned_reference`. Unverifiable installs refuse by
default (`--allow-unverified-nsb2` marks provenance-limited Option B).

## 4. TOA vs atmosphere

Primary benchmark: **TOA only**.

Kept separate (not mixed into an undocumented total):

1. TOA stellar radiance (this PR).
2. Ground-level direct transmission.
3. Single-scattered atmospheric Starlight.
4. Higher-order atmospheric Starlight.
5. Diffuse Galactic light.

A prior matched single-scattering numerical reference (~0.00275% relative
difference vs nsb2 under #263) is **not** reused as map-agreement evidence.

## 5. Methodology

### Level 1 — matched numerical operators

- Analytic constant/linear spectrum integrals and HEALPix Ω = 4π/N_pix.
- Checksum-pinned synthetic spectral map
  `crates/nsb/tests/data/starlight-transport/uniform.json` loaded via
  `nsb.StarlightMap`; native-bin and full-band integrals must match fixture
  radiances; partial bins must fail closed.
- **Cross-implementation fixture:** the same imposed constant photon spectral
  radiance density is evaluated through NSB `StarlightMap.integrate_band` and
  through nsb2 `SpectralGrid` + `Bandpass` + `ResolvedField.integrate` /
  `integrate_wavelength`, each checked against the analytic band integral.
  A shared NumPy helper is not the sole reference. NSB sums native bin
  radiances; nsb2 applies its cubic bandpass interpolation and Simpson wavelength
  integration. These are independent operators for this imposed constant
  spectrum, not evidence of equivalence for arbitrary spectra or populations.
  The NSB fixture declares 12 sources, one per nside=1 pixel; each bin has
  density × bin width as radiance. The nsb2 field is one sampled radiance
  element, not a second declaration of a one-star sky.
  Acceptance is absolute error ≤10⁻¹⁵ in each case’s unit OR relative error
  ≤10⁻¹². The observed errors are near double-precision rounding. Absolute
  errors in steradians and radiance are not averaged together.

### Level 2 — conditional real-sky directions

Six ICRS directions **frozen before residual inspection**:

`galactic_north_pole`, `galactic_south_pole`, `sparse_high_lat`, `dense_plane`,
`anticenter`, `galactic_center`.

For each direction:

- **NSB:** bundled Starlight component via `NsbEvaluator.evaluate` (TOA 300–650).
- **nsb2 diffuse:** `query_direct` in ICRS at the pointing, with a flat
  Bandpass over 300–650 nm. The pinned spectral grid contains both band endpoints;
  the runner refuses other endpoints instead of silently integrating a smaller
  wavelength interval. HEALPix weights already contain inverse pixel solid angle.
- **nsb2 catalogs:** select the Galactic RING nside=128 pixel containing the
  direction, exactly as NSB's nearest-pixel lookup. Cone-search about the actual
  pixel center with `healpy.max_pixrad` only to obtain candidates, then transform
  candidate coordinates to Galactic and retain exact `ang2pix` membership.
  The rotation is SOFA/ERFA `icrs2g`/`g2icrs`, matching NSB/Siderust's canonical
  Hipparcos realization rather than Astropy's slightly different Galactic frame.
  At pixel boundaries, especially the two frozen pole directions, floating-point
  tie-breaking can still assign neighboring pixels. Catalog means there remain
  conditional on the recorded ERFA pixel; exact agreement of NSB's boundary
  assignment is not claimed. No XHIP sources lie in the selected polar pixels.
  Sum selected source fluxes and divide by Ω = 4π/(12×128²) sr. This constructs
  the physically defined pixel-mean radiance, not an instrument count rate.
- Both routes use the **median over finite Pickles colour-grid alternatives**
  (`nanmedian`) per selected source, then sum sources, matching nsb2's discrete instrument
  central statistic before instrumental weights. No telescope area, throughput,
  atmospheric extinction or angular acceptance weights are applied. Alternatives
  are model variants, not statistical confidence intervals. An empty selected
  catalog field is zero; any selected source without a finite template fails
  the entire run. A population not loaded is unavailable, not a zero measurement.

The previous `dimensionless_angles()` conversion could relabel catalog flux as
radiance without supplying an aperture. It is removed. The previous
`nside2resol(128)` radius is √Ω, whose cone area is approximately πΩ, and its cone
is not the NSB pixel footprint. Dividing by that cone area would still not match
NSB's spatial operator. The exact pixel selection fixes both errors.

Default configurations are faint-only and faint+XHIP. Full bright Gaia BallTree
construction is opt-in (`--include-bright-catalog`), reporting faint+bright and
keeping XHIP out of that sum because overlap cannot be certified. The diffuse
map still uses point interpolation rather than a rotated NSB-pixel footprint
integral: every Level-2 score is **conditional**, including geometry as well as
population and spectral differences. No solar-colour fraction is applied to NSB.

## 6. Results

Machine-readable numbers are in the JSON artifact (generated by the runner, not
transcribed by hand).

- Level-1 NSB↔analytic / synthetic max relative difference ≲ 2×10⁻¹⁶.
- Level-1 NSB↔nsb2 imposed-spectrum cross cases: relative difference **0**
  (relative acceptance tolerance 10⁻¹²).
- Corrected Level-2 faint-only median relative difference ≈0.884767663441;
  maximum ≈0.971094791101. Values change by <10⁻¹¹ radiance versus the old run
  after removing the redundant apparent AltAz coordinate round trip.
- Catalog normalization is now correct, but has no effect on these particular
  totals: no XHIP stars are selected in the six exact NSB pixel footprints.
  Synthetic nonempty catalog tests, rather than these empty fields, establish
  flux normalization and source-selection correctness.

### Level-2 conditional TOA 300–650 nm (nsb2 faint G>15 only)

Signed difference = nsb2 − NSB. Relative difference = |nsb2 − NSB| / |NSB|.

| Direction | NSB | nsb2 faint | Relative \|diff\| |
| --- | ---: | ---: | ---: |
| galactic_north_pole | 0.01692 | 0.00082 | 0.952 |
| galactic_south_pole | 0.02442 | 0.00071 | 0.971 |
| sparse_high_lat | 0.03249 | 0.00179 | 0.945 |
| dense_plane | 0.19956 | 0.08703 | 0.564 |
| anticenter | 0.08321 | 0.01460 | 0.825 |
| galactic_center | 0.12363 | 0.02420 | 0.804 |

XHIP (88 stars globally) contributes no stars in these six exact NSB pixels,
so faint+XHIP equals faint-only. This does not establish general population
agreement. Bright Gaia BallTree evaluation is optional and is not part of the
committed default evidence.

### Documented differences, not isolated causes of residuals

Population selections, spectral models and spatial operators demonstrably
differ. The experiments do not isolate how much each difference contributes to
a residual. Missing bright stars are one plausible contributor to the faint-only
deficit; no causal attribution, NSB calibration error or empirical superiority
is demonstrated. Source-ID XP matching and solar-proxy narrowing are unsupported
observables, not explanations of the numerical residuals.

## 7. Issue #298 acceptance

Issue #298 requires a comparison contract, **at least one genuinely equivalent
numerical paired test**, explicit missing-population accounting, and distinction
from empirical validation. This report closes that requirement via the Level-1
imposed-spectrum NSB↔nsb2 operator pair. Level-2 remains conditional and does
**not** claim sky-wide physical accuracy or empirical superiority.

## 8. Reproduction and provenance

Use Python 3.12 (nsb2 requires ≥3.11), Rust/Cargo and a clean checkout of this
PR's implementation commit. The verified upstream repository is
[GerritRo/nsb2](https://github.com/GerritRo/nsb2), whose `pyproject.toml` declares
that Repository URL. Pin: `bc9320db03fbff69997ce366c6e76a37339c5d00`.

Fast tests need only NumPy ≥2 and pytest. Runtime-dependent tests skip explicitly
in this environment; their success is not cross-implementation validation:

```bash
python3.12 -m venv /tmp/nsb-fast-venv
. /tmp/nsb-fast-venv/bin/activate
python -m pip install 'numpy>=2,<3' 'pytest>=8,<10'
python -m pytest scripts/tests/test_matched_starlight_nsb2.py -q
```

Full dependencies are declared by NSB and pinned nsb2 (Astropy, healpy, SciPy,
scikit-learn, dust_extinction, siderust and tempoch). Install from the NSB root;
the clone commands do not change the working directory:

```bash
python3.12 -m venv /tmp/nsb-benchmark-venv
. /tmp/nsb-benchmark-venv/bin/activate
python -m pip install 'maturin==1.15.0' 'numpy>=2,<3' 'pytest>=8,<10'
# Inspection checkout; verified to contain the pinned revision:
git clone https://github.com/GerritRo/nsb2.git ../nsb2-reference
git -C ../nsb2-reference checkout bc9320db03fbff69997ce366c6e76a37339c5d00
# VCS installation preserves direct_url commit provenance:
python -m pip install 'nsb2 @ git+https://github.com/GerritRo/nsb2.git@bc9320db03fbff69997ce366c6e76a37339c5d00'
# Build and install THIS checkout's NSB extension, not a stale installed wheel.
maturin build --release --locked --out /tmp/nsb-matched-wheels
python -m pip install --force-reinstall /tmp/nsb-matched-wheels/*.whl
nsb_build_revision=$(git rev-parse HEAD)
NSB_REQUIRE_CROSS_IMPLEMENTATION=1 python -m pytest scripts/tests/test_matched_starlight_nsb2.py -q

# Level-1-only evidence: defaults to reports/matched-starlight-nsb2-level1.json.
python scripts/matched_starlight_nsb2_benchmark.py --skip-level2 --offline-audit

# Full default evidence: may download/caches Gaia, SVO passbands and CALSPEC Vega.
python scripts/matched_starlight_nsb2_benchmark.py --nsb-build-revision "$nsb_build_revision"
```

`--offline-audit` avoids audit downloads; it does not make Level-2 source loaders
offline. Full data include ~301,990,016-byte faint-map and ~2.36 GB bright Gaia
assets, bundled Pickles/XHIP tables, SVO Gaia G/BP/RP and Johnson V/B passbands,
and CALSPEC `alpha_lyr_stis_012.fits`. Cache URLs, sizes and checksums are recorded.
`--include-bright-catalog` additionally builds the large bright-star BallTree.
Ordinary CI uses deterministic synthetic catalogs and no Gaia downloads.

Unexpected Level-2 or requested population-audit failures raise and exit nonzero.
Case completeness, finite values, metrics, wavelength contracts and Level-1
acceptance are checked before atomic replacement of output. `--skip-level2`
records `skipped_explicitly`, has no Level-2 aggregates and does not claim complete
evidence or issue closure. Partial or provenance-limited runs cannot overwrite
the official artifact.

The provenance has two separate identities: `nsb_checkout` describes the runner's
HEAD, dirty status (including staged and untracked paths) and a SHA-256 manifest
of working-tree inputs; `nsb_build` identifies the imported extension by binary
checksum and the caller's declared compiled-source revision. The latter is an
explicit build attestation, not an inference from `git HEAD`; the runner checks
that current Rust/build inputs match that revision. Package names/versions,
executable, NSB data hashes, nsb2 source-input digest and packaged assets are
recorded separately. An editable dirty nsb2 install is provenance-limited.

For official evidence: commit implementation, tests and report first; build NSB
from that clean commit; generate the artifact; commit only the artifact in a
second commit. Its provenance points to the implementation commit, not the
later evidence commit. Only the generated JSON is excluded from its input digest
because it cannot hash itself; its pre-run dirty state is still disclosed. A dirty
runner records actual input hashes and never represents HEAD as its exact bytes.

The old artifact's `nsb_git=0a7a0c0` was an ancestor of `817dd58`, with subsequent
runner changes. `git_rev(ROOT)` recorded only HEAD; no dirty state or binary
fingerprint was retained. The exact historical generating environment cannot be
recovered from that field. The old six-direction metrics were independently
reproduced during this review, but that does not repair its historical provenance.

## 9. What this is not

- Not production Starlight recalibration.
- Not a claim that NSB is more accurate than nsb2.
- Not CTAO site calibration.
- Not completion of XP 360–570 observational admission.
- Not empirical observational admission of either Starlight model.
- Independent of the scientific content of PRs #309 / #310 (merged on `main`
  separately; this branch only syncs documentation indexes).

## 10. Follow-ups

1. Supply the checksum-pinned XP map bytes to enable a 360–570 nm spectral
   comparison under `StarlightMap::integrate_band` (#304 context).
2. If source-level matching is required, generate a **small** offline Gaia ID
   subset with admitted tooling — do not invent IDs from HEALPix pixels.
3. Keep atmospheric operator matching on #187 / #304.
