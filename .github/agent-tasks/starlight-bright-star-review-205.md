# Expert-agent brief — finish bright-star supplement fixes for NSB #205

This branch starts from `33fe9047f5f57eae6717131b5d35edd65976d5b8` on `fix/starlight-flux-deficit-182`.

## Goal

Resolve the remaining engineering/scientific-review findings in the experimental NSB-native bright-star supplement before running or interpreting the final bright-star artifact. Preserve the current design principles:

- measured-only `336–650 nm`; do not silently make this a `300–650 nm` product;
- no double counting of Gaia sources;
- external/checksum-pinned upstream data; do not embed Hipparcos/Tycho/XHIP catalogue bytes;
- fail closed on unsupported science/provenance states;
- keep `scientifically_validated = false`;
- do not alter or auto-approve the human `#103` scientific/redistribution review bundle;
- do not weaken CI, coverage floors, or scientific gates to make the branch pass.

Related work:
- parent PR: VPRamon/NSB#205
- investigation: VPRamon/NSB#182
- human Starlight approval/licensing gate: VPRamon/NSB#103
- independent comparison harness: VPRamon/nsb-validation#3

## Required fixes

### 1. Make bright-star conservation exact and order-independent

Current `BrightStarArtifact::validate_conservation()` compares:

- one plain-`f64` reduction over admitted source fluxes, and
- a differently grouped plain-`f64` reduction over per-pixel fluxes,

then requires bitwise equality with `to_bits()`.

That is mathematically brittle because floating-point addition is not associative.

Use the existing exact/order-independent accumulation machinery (`StableSum`, or a shared equivalent) for:

- source → pixel construction;
- source total reconstruction;
- pixel total reconstruction;
- conservation validation.

Do not replace this with a loose tolerance if an exact accumulator can express the intended invariant.

Acceptance:
- conservation is invariant to source ordering and pixel grouping;
- tests demonstrate reduction-tree/order independence;
- no valid artifact can fail only because of `f64` summation order.

### 2. Make `build_commit` provenance an assertion, not caller-authored metadata

The build config currently carries a `build_commit` string that is only checked for non-emptiness and copied into the manifest/artifact.

A build executed from different code can therefore claim an older commit.

Implement a fail-closed software identity contract. The actual implementation may use an explicit CLI/build argument or another repository-standard mechanism, but it must ensure that:

- the software commit used for the run is explicitly supplied/derived;
- it is validated against the expected pinned identity;
- the manifest records the verified identity;
- a mismatch fails before producing a publishable artifact.

Update the Ladon config/pins as needed only after the implementation is final.

Tests:
- matching commit passes;
- mismatched commit fails closed;
- empty/invalid identity fails.

### 3. Replace arbitrary uncertainty constants with a calibration artifact/evidence contract

The spectral-model builder currently hardcodes values such as:

- `template_mismatch_fraction = 0.10`
- `spectral_type_mapping_fraction = 0.05`
- `hp_zero_point_fraction = 0.01`

while the design says template/SpT uncertainty should be empirically calibrated against XP/CALSPEC or other independent evidence.

Do not present hardcoded illustrative fractions as calibrated science.

Implement one of the following, preferring the first:

A. a checksum-pinned machine-readable calibration artifact/report, produced by a reproducible script, containing the validation population, method, residual statistics, applicability, and derived uncertainty parameters; the spectral model consumes those calibrated values; or

B. if a defensible calibration cannot be completed now, mark those terms explicitly provisional/experimental in the artifact contract and prevent the supplement from being promoted to a production-candidate state that implies calibrated uncertainties.

Do not tune parameters merely to match nsb2.

### 4. Handle unsupported XHIP luminosity classes fail-closed

The Python model builder currently maps luminosity classes only with:

`{1: 1.0, 2: 2.0, 3: 3.0, 4: 4.0, 5: 4.5}`

Unsupported classes must not crash the complete build with a `KeyError`, and must not be assigned an invented `log g`.

Refactor the mapping to return an explicit unsupported result and ensure affected stars become a documented spectral reconstruction failure / ambiguous exclusion unless a cited, calibrated mapping exists.

Tests must include at least one unsupported luminosity class.

### 5. Version the Gaia ADQL recipes used to produce quality/fallback extracts

The build checksum-pins the resulting Gaia CSVs and external `.adql` files, but the actual queries are not currently versioned in the repository.

Add the small ADQL recipes under a stable repository path, e.g.:

`docs/nsb_components/starlight/bright-stars/queries/`

or an equivalent source-controlled location.

Requirements:
- exact queries used to produce the pinned CSVs are committed;
- build provenance references/checksums those repository recipes;
- docs explain how to reproduce the Gaia TAP extracts;
- no raw Gaia catalogue dump is committed.

### 6. Prove replacement identity equality, not only equal counts

The current merge invariant checks that:

`replacement_gaia_id_count == exclusion_reasons["bright_star_replaced_by_supplement"]`

but Gaia shards do not retain the exact identities actually suppressed. The report then hardcodes `base_admitted_replacement_intersection_count = 0`.

Strengthen the contract so the merged evidence can prove identity-level invariants.

Preferred design:
- Gaia shards record the exact set of Gaia source IDs suppressed by the supplement;
- synthetic supplement shard records replacement IDs;
- merge requires exact set equality;
- duplicate suppressed IDs across partitions fail;
- any replacement ID that survives as a base-admitted Gaia source fails;
- the report computes, rather than hardcodes, the intersection metric.

Keep this deterministic and included in canonical merge evidence.

Tests:
- exact match passes;
- same count/different identity fails;
- duplicate suppression fails;
- replacement still admitted in primary fails.

### 7. Clarify physical-source counts vs input-record accounting

A replaced physical star currently contributes:
- one observed+excluded Gaia record, and
- one observed+admitted supplement record.

This is valid radiometrically, but global `observed_sources` is therefore record accounting, not unique physical stars.

Make this explicit and machine-readable. Add appropriate counters/metadata, e.g.:
- primary input records;
- supplement input records;
- replacement records;
- unique/admitted physical-source semantics where supportable.

At minimum, rename/document fields so downstream users cannot interpret `observed_sources` as a count of unique stars.

Preserve backward compatibility intentionally; if schema semantics change, version the schema.

### 8. Do not require supplement spectral reconstruction for Gaia-retained duplicates

Current builder logic can turn `MatchedAndRejectedAsDuplicate` into `AmbiguousManualReview` solely because no XHIP/CK04 spectral estimate exists, even though the supplement will not be admitted and Gaia remains authoritative.

Only classes that actually need supplement flux (`SupplementOnly` and `MatchedAndReplacesPrimary`) should require a valid supplement spectral estimate.

Preserve the secure Gaia identity/classification for reliable Gaia matches when supplement reconstruction is unavailable.

Tests:
- reliable Gaia duplicate + no supplement spectrum remains duplicate-rejected;
- supplement-only + no spectrum fails closed;
- replacement + no spectrum fails closed.

### 9. Turn the 2D-vs-3D perspective diagnostic into an enforceable policy

The design says 2D Hipparcos→J2016 propagation is accepted only when the 3D comparison demonstrates that omitted perspective motion is negligible relative to the matching policy.

Currently the builder reports `max_2d_3d_difference_arcsec` but does not enforce a source-level or run-level gate.

Implement an explicit policy:
- define a justified threshold relative to the source match radius/uncertainty, not an arbitrary global magic number;
- sources exceeding the supported approximation domain become ambiguous/manual-review or otherwise fail closed;
- diagnostics report counts and worst offenders;
- tests cover both accepted and rejected perspective cases.

### 10. Fix current CI without weakening policy

Current workflow run for the reviewed head: `36974332699`.

Known failures:

#### Clippy
- `bright_stars/build_run.rs:81`: `clippy::map_entry`
- `bright_stars/catalogue.rs:200`: `clippy::map_entry`

Use the `BTreeMap::entry` API while preserving the conservative duplicate/ambiguity semantics.

#### Diff coverage
Overall workspace coverage passes:
- 83.60% vs 80% floor

Diff production coverage fails:
- 63.52% (1590/2503)
- required: 90%
- `bright_stars/mod.rs` currently appears with no coverage data

Add meaningful tests for the new production paths. Do not lower the 90% floor, exclude these modules, or add superficial line-touch tests only to game coverage.

## Scientific/run completion after code fixes

Once the implementation and CI are clean:

1. Re-finalize the true `Measured336To650` candidate with corrected bookkeeping and verify whether the selected flux map remains identical to the previously recorded `32d4f6d2…` candidate as expected.
2. Build the experimental bright-star supplement on Ladon using the verified commit/config/input pins.
3. Commit machine-readable build diagnostics/provenance that are safe to redistribute.
4. Report at least:
   - input Hipparcos population;
   - passing population cut;
   - Gaia matches;
   - supplement-only;
   - replacements;
   - Gaia-retained duplicates;
   - ambiguous/manual-review;
   - spectral reconstruction failures;
   - admitted supplement sources;
   - integrated supplement flux;
   - high proper-motion/perspective diagnostics.
5. Run before/after comparison using `nsb-validation` without treating nsb2 as truth.
6. Include full and no-XHIP-style reference comparisons, brightness-tail metrics, top residual pixels/sources, and flux conservation checks.
7. Do not close #182 merely because correlation or bias improves. Attribute remaining residuals and keep independent-validation limitations explicit.
8. Do not set `scientifically_validated = true` or modify #103 human decisions.

## Required CI / verification

Before marking this PR ready:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --lib --bins --tests --examples --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
```

Also require the repository CI coverage gate to pass, including >=90% changed production line coverage.

Run any bright-star-specific reproducibility/build tests introduced by this work.

If `nsb-validation` must change because the emitted contract changes, make the corresponding changes in its PR #3 and run its full pytest suite.

## Scope discipline

Do not:
- copy nsb2's XHIP/Pickles implementation;
- fit thresholds or uncertainties to erase the nsb2 residual;
- add UV flux to this measured-only supplement;
- embed restricted catalogue bytes;
- weaken fail-closed contracts;
- alter human review decisions;
- lower CI/coverage thresholds.

## Cleanup

This file exists only to bootstrap the expert-agent follow-up PR. Remove it before marking the PR ready for final merge; the durable result should be code, tests, provenance, and scientific documentation rather than an agent task file.
