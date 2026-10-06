# Starlight very-bright-star supplement — design v1

Status: **design / experimental**. This document defines the scientific
policy for an optional NSB-native very-bright-star population supplement.
It does **not** grant redistribution approval (#103). Scientific production
readiness is established separately by checksum-pinned validation evidence.

Cross-implementation evidence (#182 Experiments A/B) shows that nsb2’s
88-star XHIP component is ≈ **7.4%** of full nsb2 stellar flux and removes
≈ **6 pp** of the integrated NSB deficit, with most of the bright-tail
correlation collapse. That motivates a supplement. It does **not** justify
copying nsb2’s XHIP+Pickles implementation.

## 1. Scientific purpose

Restore very-bright stars that are missing from or radiometrically unreliable
in the primary Gaia DR3 Starlight pipeline, **without double-counting** Gaia
sources.

The supplement is:

- independently versioned (`starlight-bright-stars-v1`);
- optional / opt-in until technical validation passes and redistribution is authorized;
- provenance-visible in runtime and merge reports;
- never a silent modifier of the Gaia base map.

## 2. Catalogue assessment

| Catalogue | Suitability | Astrometry | Photometry / spectra | Bright completeness | Gaia relationship | License / redistribution |
|---|---|---|---|---|---|---|
| **Hipparcos-2 (van Leeuwen 2007; ESA HIP2)** | **Preferred primary** | Milliarcsec; epoch J1991.25; PMs | Hp; spectral types not native | Designed for bright stars; complete for V≲7.3 | Cross-IDs via Hipparcos–Gaia crossmatch / CDS | ESA **CC BY-NC 3.0 IGO** (“Credit: ESA”). **Do not embed** catalogue bytes in the NSB repository until #103 redistribution review. |
| **Tycho-2** | Supporting photometry | Good; denser than HIP | \(B_T,V_T\) | Completeness to \(V_T\sim11\) | Overlaps Gaia; useful colours | Same ESA CC BY-NC 3.0 IGO. External only. |
| **XHIP (Anderson & Francis 2012; CDS V/137D)** | Convenient compilation (HIP2 + SpT + RV + …) | From HIP2 | SpT, photometry amalgam | Same bright core as HIP | Used by nsb2 (88-star cut) | CDS/VizieR scientific use with citation; commercial depends on origin components. **Do not choose solely because nsb2 uses it.** Prefer citing primary HIP2 + SpT sources when building NSB assets. External / user-generated until legal gate. |
| **Gaia DR3 bright end** | Primary population for \(G\gtrsim3\)–6 where reliable | DR3 | XP / photometry | Incomplete / saturated for \(G\lesssim3\) (~20% missing brighter than \(G=3\)); gating/saturation residuals to \(G\sim8\) | Identity | Already the NSB base; not a supplement source for missing stars. |
| **CALSpec / spectrophotometric standards** | Independent flux validation sample | — | Absolute SEDs | Sparse | Occasional Gaia match | Prefer for **validation**, not population completeness. |

### Decision (v1)

- **Primary catalogue for supplement membership:** Hipparcos-2 (ESA), selected
  by a completeness-motivated magnitude threshold (below), not by residual
  matching to nsb2.
- **Photometry / colour:** Tycho-2 \(B_T,V_T\) and/or Hipparcos Hp where
  available; Gaia photometry only when quality criteria say Gaia is reliable.
- **Spectral typing:** prefer a documented SpT source with explicit citation
  (e.g. XHIP SpT column with Anderson+Francis 2012 citation, or an
  alternative primary SpT compilation). XHIP may be used as a **build input**
  under external-asset mode; it is not the scientific product identity.
- **Redistribution:** **external opt-in / reproducible user-generated asset**.
  Do **not** ship Hipparcos, Tycho-2, or XHIP source bytes in the NSB git
  tree. Same posture as Gaia bulk products in
  `licensing/artifact-inventory-v1.toml`.

## 3. Population boundary

Gaia EDR3/DR3 validation: detection efficiency drops for \(G\lesssim 3\);
≈20% of stars brighter than \(G=3\) lack a Gaia entry; residual saturation
corrections are published for roughly \(2<G<8\).

**v1 admission threshold (completeness-motivated):**

```text
supplement candidate if:
  Hipparcos Hp ≤ 4.0
  OR (no Hp and Tycho V_T ≤ 4.0)
  OR (Gaia match with G ≤ 3.0 and Gaia photometry/XP fails quality gate)
```

Rationale: target the regime where Gaia incompleteness/saturation is
documented, not a threshold fitted to erase the −16% residual. Exact Hp/VT
cut may be refined after source-level audits; changes require a new policy
id (`bright-stars-population-v1` → `v2`).

Non-stellar / problematic flags (doubles without resolved flux rule,
variables without mean-flux policy) are excluded or deferred to
`ambiguous/manual-review`.

## 4. Epoch and proper motion

- Catalogue epoch: Hipparcos **J1991.25**.
- The build propagates the two-dimensional Hipparcos proper motion to
  **J2016.0** for matching. When XHIP supplies a finite radial velocity and
  HIP2 supplies a positive parallax, it also performs Cartesian constant-space-
  velocity propagation and records the 2D-versus-3D angular difference. The
  2D approximation is accepted only if the emitted diagnostic demonstrates
  that the omitted perspective term is negligible relative to the declared
  match radius; it is not assumed negligible by construction.
- Target epoch for HEALPix binning / Gaia crossmatch: **Gaia DR3**
  (J2016.0).
- Propagate \(\alpha,\delta\) with HIP2 proper motions to J2016.0.
- At NSIDE=128 (pixel ≈ 27.5′), even extreme nearby PMs over 25 yr are
  ≪ pixel scale for almost all stars; quantify max displacement in the
  build report. Full 3D (parallax+RV) propagation is optional diagnostic,
  not required for v1 binning if max 2D displacement ≪ pixel radius.

## 5. Crossmatch and duplicate policy

Every supplement source is classified as exactly one of:

| Class | Meaning |
|---|---|
| `supplement-only` | No secure Gaia DR3 match; admit supplement flux |
| `matched-and-replaces-primary` | Gaia match exists but fails bright-end quality gate; use supplement flux and **suppress** Gaia contribution for that source |
| `matched-and-rejected-as-duplicate` | Gaia match is reliable; discard supplement (Gaia wins) |
| `ambiguous/manual-review` | Multiple matches or inconsistent IDs; **exclude** from v1 maps |

Matching:

1. Prefer stable identifiers (HIP ↔ Gaia `source_id` via published
   crossmatch / DR3 `hipparcos2_best_neighbour` where available).
2. Fallback: positional match after PM propagation, radius
   \(\max(1'', 5\sigma_{\mathrm{pos}})\), with uniqueness enforcement.
3. High-PM stars: enlarge search using PM uncertainty; never silently
   double-count.

**No silent double counting:** runtime merge of base Gaia map + supplement
must apply the replacement/suppression table atomically.

## 6. Replace vs augment Gaia (policy)

Default for a matched source: **D — use supplement only when Gaia is
unreliable**.

Gaia is treated as unreliable for the source when any of:

- no Gaia DR3 entry;
- missing XP and photometric-fallback features fail;
- published saturation / bright-gate quality indicators fail the v1 cut;
- G brighter than the population threshold **and** XP calibration fails.

Otherwise Gaia is retained (`matched-and-rejected-as-duplicate`).

Do **not** automatically replace all HIP–Gaia matches. Statistical
combination (option C) is out of scope for v1.

Policy id: `bright-stars-gaia-precedence-v1`.

## 7. Spectral reconstruction

Two versioned artifact products exist:

| Product band | Model id | Schema | Use |
|---|---|---|---|
| `measured-336-650` | `starlight-bright-stars-v1` | 1 | Diagnostics / measured-only maps |
| `combined-300-650` | `starlight-bright-stars-combined-v1` | 2 | Production Combined300To650 (#207) |

The measured-only artifact contains no 300–336 nm field and **cannot** be
loaded into a `Combined300To650` build (fail closed). Missing UV coverage is
unavailable, never encoded as zero flux and never relabelled from 336–650 nm.

The combined artifact obtains the 300–336 nm term from the **same** Hp-scaled
CK04 template used for 336–650 nm:

1. Scale the pinned Castelli–Kurucz template to the Hipparcos Hp magnitude
   through the Bessell (2000) Hp response and CALSPEC Vega zero point.
2. Integrate the scaled template over 336–650 nm (measured contribution).
3. Integrate the **same** scaled template over 300–336 nm (UV completion).
4. Record `uv_completion_model_id = ck04-hp-scaled-uv-300-336-v1` with
   independent statistical/systematic terms on each sub-band and a single
   Hipparcos Hp zero-point correlated term on the full 300–650 integral.

Admitted combined sources without a justified positive 300–336 nm term fail
closed. Template coverage of both intervals is validated at model load.

Route priority:

1. If a matched Gaia source has **valid XP** and passes the quality gate →
   **prefer existing NSB XP machinery** (do not invent a parallel spectrum).
2. Else: spectral-type template integration with documented library (above).
3. Else: colour–temperature approximation from \(B_T-V_T\) / Hp with
   larger systematic.

**Template library:** evaluate candidates (Pickles 1998, and alternatives
with clearer redistribution terms and coverage) in a separate note. Do
**not** adopt Pickles merely because nsb2 uses it. Until a library clears
licensing + science review, the build pipeline may depend on an
**external** template artifact pin.

Zero points: every band used in conversion must declare system, response
reference, zero point, flux convention, and units in the build config.
Unit tests cover representative magnitudes → expected band integrals.

## 8. Uncertainty model

Separate components (no single arbitrary %):

| Component | Treatment |
|---|---|
| Photometric | From catalogue magnitude errors → flux |
| Template / SpT | Empirical scatter vs XP or CALSpec on validation sample |
| Crossmatch ambiguity | Sources in `ambiguous` excluded; matched replacements carry a discrete systematic floor |
| Catalogue calibration / ZP | Catalogue-wide correlated systematic (declared fraction or empirical) |
| Band integration | Negligible vs above if wavelength grid is fine; document |

Correlation: per-source photometric/template-mismatch terms are accumulated
in quadrature. Catalogue zero-point terms add linearly only inside their
explicit `correlation_group_id`; different groups (for example Hipparcos,
Tycho, and a template-library calibration) combine in quadrature. Do **not**
mark all bright-star errors globally correlated unless justified.

## 9. Artifact and pipeline

Pipeline (deterministic):

```text
ingest HIP2 (+ optional Tycho-2 / SpT table)
  → verify upstream checksums (fail-closed)
  → normalize astrometry to J2016.0
  → crossmatch Gaia DR3
  → classify supplement/duplicate/replace/ambiguous
  → derive spectrum / integrate band
  → assign uncertainty
  → Galactic HEALPix NSIDE=128
  → emit starlight-bright-stars-v1 artifact + source manifest
```

Outputs:

- compact HEALPix map for runtime (optional merge);
- **source-level manifest** (identity, class, fluxes, Gaia id, route);
- provenance JSON (input SHAs, commit, counts, output SHA).

Runtime config (conceptual):

```toml
[starlight.bright_star_supplement]
# Absent => Gaia-only (current default)
artifact_path = "/path/to/starlight-bright-stars-v1.json"
sha256 = "..."
policy_id = "bright-stars-gaia-precedence-v1"
population_policy_id = "bright-stars-population-v1"
```

No silent load of a local unmarked file.

## 10. Validation plan

- Before/after vs nsb2 full and no-XHIP (comparison, **not** truth).
- Top residual pixels → dominating sources table.
- Independent checks on a small CALSpec / published-SED sample.
- Selection-function and XP vs photometric audits remain separate (#182
  residual decomposition).

## 11. Activation states

| State | Meaning |
|---|---|
| `experimental` | **Current target** after implementation |
| `validated-for-cross-comparison` | Before/after metrics + source audits recorded |
| `production-candidate` | Policy + uncertainty + provenance complete; still opt-in |
| `production-approved` | Checksum-pinned technical and external scientific validation passes |
| Redistribution | Separate human gate (#103); external asset until cleared |

The `nsb2` comparison is cross-implementation evidence rather than independent
observational ground truth; its limitations must remain explicit in the
production validation record.

## 12. Relationship to #182

The supplement is intended to address the **very-bright population hole**
(~6 pp / bright-tail correlation). It is **not** expected to remove the
~16% Gaia-only measured-band residual.
