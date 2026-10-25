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
| Geometry | Pointing-direction TOA; HEALPix details recorded per implementation |
| Units | photons cm⁻² ns⁻¹ sr⁻¹ |
| Forbidden | Solar-colour proxy on NSB’s 300–650 map; arbitrary renormalization; treating missing populations as zero; silently dropping selected sources with invalid Pickles templates |

### Comparability matrix

| Comparison | Status |
| --- | --- |
| Synthetic `StarlightMap` band integrals vs analytic fixture | **Equivalent** (NSB↔analytic; not by itself NSB↔nsb2) |
| Imposed constant spectrum through NSB `StarlightMap` and nsb2 `SpectralGrid`/`Bandpass`/`integrate_wavelength` | **Equivalent** (genuine NSB↔nsb2 paired operators + analytic) |
| NSB bundled 300–650 vs nsb2 faint G>15 (same directions) | **Conditional** |
| NSB bundled 300–650 vs nsb2 faint+XHIP (+ optional bright catalog) | **Conditional** |
| Source-ID matched XP vs Pickles (360–570 nm) | **Unsupported** |
| Solar-colour proxy narrowing of NSB 300–650 | **Unsupported** |

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
Missing populations are labelled unavailable — never assigned zero radiance.
Selected sources with no finite Pickles template rate reject the evaluation
rather than vanishing from a partial sum.

The runner verifies the **installed** nsb2 implementation against the pin via
local git metadata or package `direct_url` VCS info (no network). Matched runs
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
  A shared NumPy helper is not the sole reference.

### Level 2 — conditional real-sky directions

Six ICRS directions **frozen before residual inspection**:

`galactic_north_pole`, `galactic_south_pole`, `sparse_high_lat`, `dense_plane`,
`anticenter`, `galactic_center`.

For each direction:

- **NSB:** bundled Starlight component via `NsbEvaluator.evaluate` (TOA 300–650).
- **nsb2:** `query_direct` with zero pressure; flat Bandpass 300–650 nm;
  **median over finite Pickles colour-grid template alternatives** (nsb2
  instrument central of nanmin/nanmedian/nanmax). This is an instrument-facing
  estimator, not a physical measurement of stellar spectral radiance.
  Empty cone selections contribute 0; selected sources with all-nonfinite
  templates reject the direction.

Default nsb2 configurations: faint-only and faint+XHIP. Full G<15 BallTree is
opt-in (`--include-bright-catalog`). **No solar-colour fraction is applied to NSB.**

## 6. Results

Machine-readable numbers are in the JSON artifact (generated by the runner, not
transcribed by hand).

- Level-1 NSB↔analytic / synthetic max relative difference ≲ 2×10⁻¹⁶.
- Level-1 NSB↔nsb2 imposed-spectrum cross cases: relative difference **0**
  (within 10⁻⁶ absolute tolerance in tests).
- Level-2 conditional metrics are unchanged from the post-`nanmedian` artifact
  (fail-closed template accounting did not alter these six directions).

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

XHIP (88 stars) did not contribute inside the nside=128 resolution cones for
these six directions, so faint+XHIP equals faint-only here. The large deficit of
nsb2 faint-only versus NSB is **consistent with a missing bright population** in
the nsb2 map channel (G<15 excluded), not proof of an NSB radiometric bug.
Bright-catalog BallTree evaluation remains opt-in (`--include-bright-catalog`).

### Demonstrated causes (not automatic NSB defects)

1. **Population mismatch** — full NSB admission vs nsb2 G-split (+ optional XHIP).
2. **Spectral-model difference** — XP/photometric/CK04 vs Pickles templates.
3. **Geometry difference** — Galactic RING nside=128 vs ICRS NESTED nside=1024.
4. **Unsupported observables** — source-ID XP matching; solar proxy.

## 7. Issue #298 acceptance

Issue #298 requires a comparison contract, **at least one genuinely equivalent
numerical paired test**, explicit missing-population accounting, and distinction
from empirical validation. This report closes that requirement via the Level-1
imposed-spectrum NSB↔nsb2 operator pair. Level-2 remains conditional and does
**not** claim sky-wide physical accuracy or empirical superiority.

## 8. Reproduction

Pinned nsb2 commit: `bc9320db03fbff69997ce366c6e76a37339c5d00`.

```bash
# Editable checkout matching the pin (example used for the committed artifact):
git clone https://github.com/cta-observatory/nsb2 /tmp/nsb2-reference
cd /tmp/nsb2-reference && git checkout bc9320db03fbff69997ce366c6e76a37339c5d00
pip install -e /tmp/nsb2-reference

python -m pytest scripts/tests/test_matched_starlight_nsb2.py -q

# Level 1 + audit only
python scripts/matched_starlight_nsb2_benchmark.py --skip-level2 --offline-audit

# Full Level 1 + Level 2 (requires nsb, pinned nsb2, healpy, cached Zenodo Gaia)
python scripts/matched_starlight_nsb2_benchmark.py \
  --output docs/validation/artifacts/matched-starlight-nsb2-v1.json
```

Zenodo Gaia assets are checksum-fingerprinted in the artifact when the cache is
available (offline audit mode records metadata without re-fetching).
Environment fields such as `module_path` and `platform` vary by machine;
scientific metrics and the verified `resolved_commit` are the invariant outputs.

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
