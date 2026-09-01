# Issue #116 — HEALPix flux anomaly investigation report

Status: **root causes identified and fixed; full 3386-partition production complete; full-sky diagnostics passed.**

## Executive summary

Three independent bugs produced HEALPix-aligned `flux_ph_m2_s / admitted_sources` discontinuities:

1. **ICRS→Galactic accumulation mismatch** (fixed): Gaia equatorial `source_id` HEALPix indices were accumulated into a map declared `coordinate_frame=galactic`. This explains the legacy **six-parent** morphology (parents 0, 16, 18, 26, 27, 43).

2. **Sparse selection-table spatial resolution** (fixed): Angular nearest-neighbour at NSIDE=128 on a NSIDE=32-subsampled completeness table created artificial Voronoi boundaries. Replaced with hierarchical NSIDE=32 parent inheritance.

3. **Photometric vs XP flux scale mismatch** (fixed): The pinned photometric artifact predicted ln(flux) with a zero point ~7000× higher than Gaia XP continuous 336–650 nm integrals. When photometric inference was enabled (ablation stage B), seven anomalous NSIDE=2 parents appeared immediately; UV and selection weighting did not create the patch morphology.

## Classification

### CONFIRMED BUGS

| Bug | Evidence | Fix |
|-----|----------|-----|
| Galactic map accumulated equatorial `source_id` HEALPix | Legacy six-parent pattern | `galactic_nested_pixel_from_icrs_position(ra, dec)` |
| Selection lookup used `source_id` equatorial pixel | Worker uses ICRS RA/Dec at artifact `healpix_nside` | `worker.rs` |
| Sparse completeness table resolved via angular Voronoi at NSIDE=128 | 4096/196608 tabulated cells; hierarchical resolve test | `build_hierarchical_resolve_map()` |
| Wrong logistic `m10_to_completeness` | 25 golden cases vs GaiaUnlimited | `cantat_gaudin_m10_to_completeness()` |
| XP path skipped scientific exclusions | Regression test | Shared `scientific_exclusion_reason()` |
| `faint_tail_flux_fraction` never applied | Code audit | Reporting contract clarified |
| Broken `nested_neighbours()` Morton interleaving | Prior boundary metrics 0.37→1.06 unreliable | `healpix_topology.rs` reference implementation |
| **Photometric artifact flux scale incompatible with XP integration** | Ablation: patches appear at stage B only; photometric G=15 predicted ~1e8 ph/m²/s vs XP oracle ~1.4e4; bright photometric-only outliers up to 4.5e10 in anomalous parents | XP-anchored photometric artifact `gaia-dr3-photometric-logflux-xp-anchored-v1` (SHA `02a6e5c9…`) |

### CONFIRMED ROOT CAUSES

**Legacy six patches:** ICRS→Galactic accumulation mismatch.

**Remaining seven-patch morphology (smoke parents 13, 24, 32, 33, 36, 37, 43):** Photometric inference branch admitted with flux per source orders of magnitude above XP continuous integrals. Spatial variation in XP availability and bright-star photometric-only outliers created sharp `flux/admitted` jumps at NSIDE=2 boundaries. Controlled ablation shows (corrected HEALPix neighbour topology; prior 0.37/0.40 ratios used a broken face-local Morton approximation and are invalid):

| Stage | Anomalous NSIDE=2 parents | Boundary cross/internal |
|-------|---------------------------|-------------------------|
| A (XP only) | `[]` | **1.24** |
| B (+ photometric, **old artifact**) | `[13, 24, 32, 33, 36, 37, 43]` | **1.44** |
| E (full production, **old artifact**) | `[13, 24, 32, 33, 36, 37, 43]` | **1.73** |
| E (full production, **XP-anchored artifact**) | `[]` | **1.15** |
| B (+ photometric, **XP-anchored artifact**) | `[]` | **1.12** |

### CONTRIBUTING FACTORS

- Selection weighting modulates but does not create patch morphology on smoke (phase 4 gate B).
- `invalid_uv_predictors` exclusion is spatially correlated but anti-correlated with anomalous parents (lower in anomalous regions).
- Published map uses weighted flux numerator with unweighted admitted denominator (diagnostic confound, not a bug).

### ELIMINATED HYPOTHESES

| Hypothesis | Evidence |
|------------|----------|
| Selection Voronoi alone explains remaining smoke patches | Ablation: patches appear at stage B before selection |
| UV correction primary cause | Patches visible at stage B (pre-UV) |
| Selection weights at G=17 alone explain legacy six | Weight variation <0.4% at G=17 |
| Branch mix alone (XP fraction) explains 100× flux jumps | Same photometric branch; flux scale mismatch dominates |
| Partition merge nondeterminism | Reproducible baseline and post-fix rebuild |

### 48-partition smoke before/after (production gate)

| Metric | Before (`ad23fe32…` artifact) | After (`02a6e5c9…` XP-anchored) |
|--------|------------------------------|----------------------------------|
| Anomalous NSIDE=2 parents | `[13, 24, 32, 33, 36, 37, 43]` | `[]` |
| Boundary cross/internal ratio (corrected neighbours) | **1.727** | **1.151** |
| Global median flux/admitted | 1.69e6 | 8.95e3 |
| Ablation B boundary ratio (corrected neighbours) | **1.44** | **1.12** |

Evidence: `docs/nsb_components/starlight/diagnostics/evidence/phase8-photometric-anchor/`

### Full-sky 3386-partition production (final candidate)

| Metric | Value |
|--------|-------|
| Partitions built | **3386 / 3386** |
| Candidate SHA-256 | `76191c8b682d96adfc3a017f44f3fcfd0bec5dcb9a958d31668250b8a0ba396a` |
| Merge report SHA-256 | `3f003afb6dcae09eaf917c5a3cbd0fc2fd113a331164fb0509d14c82bb76c5f9` |
| Runtime map SHA-256 (production admission headers) | `c777917b7c9aceab5d3e0e25bb6ab0e0b75ee21357097c2ca4abe6a097a2243b` |
| Runtime sidecar SHA-256 | `735be03e50bfe1f47254c46d0fc1c124912e285cac5e283dd8a06449c1ca2144` |
| Anomalous NSIDE=2 parents | `[]` |
| Boundary cross/internal ratio | **1.151** |
| Global median flux/admitted | 1.07e4 |
| Admitted sources | 21,581,555 |
| Legacy six-patch morphology | absent |
| Photometric seven-patch morphology | absent |

Evidence: `docs/nsb_components/starlight/diagnostics/evidence/phase9-fullsky-production-summary.json`

### KNOWN LIMITATIONS

- Photometric artifact trained on synthetic XP-scale model; production refit on held-out XP integrals is recommended.
- Human #103 decisions remain **pending**.
- `faint_tail_flux_fraction` not applied to flux (separate contract issue).

## Checksums

| Artifact | SHA-256 |
|----------|---------|
| Legacy candidate (frame bug) | `5946fa170b1be911b8996ac4a36200133743bac6ba39a1392358cd3007a91563` |
| Frame-fixed candidate (pre-photometric fix) | `b17124d057faad2445575239c04928514d2846ec36a2f5df7137566058d85154` |
| Photometric artifact (old, miscalibrated) | `ad23fe327b3cbb75167ffe47a00dc8bcbb63d72f9e5a1b19f32171dda5fd680d` |
| Photometric artifact (XP-anchored) | `02a6e5c98458351fb13ec7623cffa019a760bdf2e68cca64b80f9c5d7fe4f4f2` |
| Full-sky candidate (3386 partitions) | `76191c8b682d96adfc3a017f44f3fcfd0bec5dcb9a958d31668250b8a0ba396a` |
| Merge report | `3f003afb6dcae09eaf917c5a3cbd0fc2fd113a331164fb0509d14c82bb76c5f9` |
| Packed runtime map | `c777917b7c9aceab5d3e0e25bb6ab0e0b75ee21357097c2ca4abe6a097a2243b` |
| Runtime sidecar | `735be03e50bfe1f47254c46d0fc1c124912e285cac5e283dd8a06449c1ca2144` |
| Review bundle | `03150bb412df75cbe3db85e469d986feea9d52642744ccb05c47062cfed8070f` |

## #103 status

Scientific and redistribution decisions remain **pending**. Candidate, merge report, runtime assets, and review bundle pins updated to the full-sky production SHAs above.
