# Data-product workflow

Status: Current maintainer workflow overview.
Audience: Scientific-data maintainers and release reviewers.
Scope: Generate, update, validate, package, and admit NSB runtime data.

Runtime NSB consumes immutable, checksum-pinned scientific assets. Catalogue
acquisition, transformation, model training, map construction, and validation
are offline maintainer activities performed by `nsb-data-tools`.

## When data must be regenerated

Regenerate or reassess a data product when one of its scientific inputs or
contracts changes, including:

- source catalogue release, inventory, query, or selection policy;
- passband, wavelength grid, unit, calibration, or photometry model;
- source exclusions, completeness policy, or population accounting;
- HEALPix resolution, ordering, smoothing, or map-building algorithm;
- validation reference, tolerance, uncertainty model, or production gate;
- provenance, license, redistribution policy, or release metadata;
- persisted schema or checksum contract.

Do not regenerate a large validated product merely because the software was
refactored. If scientific inputs and serialized output contracts are unchanged,
prefer deterministic reassessment of the existing immutable artifact.

## End-to-end lifecycle

```text
source policy and inventory
  -> acquisition
  -> normalization and canonical catalogue
  -> candidate generation
  -> scientific and structural validation
  -> production admission review
  -> packaging and runtime manifest
  -> asset registry verification and release
```

Candidate generation does not imply production approval. The artifact becomes a
runtime production asset only after all required evidence is complete and the
asset is registered in `crates/nsb/data/manifest.toml`.

## 1. Define the scientific contract

Before processing data, freeze the relevant contract:

- source catalogue and immutable release;
- selection and exclusion policy;
- coordinate frame;
- spectral band and units;
- expected input/output schemas;
- uncertainty semantics;
- completeness and reconciliation rules;
- validation references and tolerances;
- candidate and production admission gates;
- redistribution and attribution policy.

For integrated starlight, start with
[Starlight science requirements](../nsb_components/starlight/science-requirements.md).

## 2. Acquire and verify inputs

Use durable acquisition tools rather than ad hoc downloads. Persist inventories,
service-job manifests, checksums, retry evidence, and explicit resume state.

Relevant tools include:

- `query_gaia_tap`;
- `generate_gaia_starlight_release_inputs`;
- `download_gaia_xp_continuous_bulk`;
- `index_gaia_xp_continuous_bulk`.

Resume may reuse only files or service results that pass their documented
integrity checks.

## 3. Normalize to canonical scientific inputs

Transformation commands must produce deterministic, versioned schemas with exact
source accounting and explicit exclusions.

Relevant tools include:

- `prepare_gaia_starlight_catalogue`;
- `prepare_tycho_starlight_catalogue` for controlled experiments;
- `normalize_xp_continuous_coefficients`;
- `consolidate_gaia_starlight_samples`.

Record the exact command, source checksums, tool version, schema version, and
output checksums. Production conversion must fail on malformed data, missing
provenance, inconsistent dimensions, or unreconciled row counts.

## 4. Generate candidates

Use candidate commands to create scientific artifacts without overstating their
maturity:

- `build_starlight_map` builds one deterministic HEALPix map;
- `sweep_starlight_nside` compares or reassesses several resolutions;
- `train_starlight_photometry_models` creates experimental model candidates;
- `build_integrated_starlight_product` combines approved population
  contributions and records unresolved production blockers.

Write all outputs beneath an explicit external output directory. Do not generate
catalogues, maps, checkpoints, or reports in the repository root.

## 5. Validate and reconcile

Validation must be reproducible and independent of the generation command where
the scientific contract requires independent evidence.

Relevant tools include:

- `validate_starlight_map`;
- `validate_xp_continuous_reconstruction`;
- `audit_gaia_starlight_exclusions`;
- `verify_assets` for the final runtime registry.

Every required gate is one of `Passed`, `Failed`, `NotRun`, or `NotApplicable`.
Only `Passed` satisfies a required production gate. Missing evidence and skipped
validation must block production admission.

## 6. Package a runtime asset

`pack_starlight_asset` packages a validated map and its runtime manifest. The
manifest must contain immutable provenance, source selection, scientific model,
band definition, generation command, map checksum, validation report, independent
comparison, and exact header expectations.

External production maps use the same fail-closed sidecar contract documented in
[Validated external starlight manifest](../nsb_components/starlight/external-manifest.md).

## 7. Register and release

A reviewed runtime asset is added under `crates/nsb/data/` and registered in
`crates/nsb/data/manifest.toml`. Then run:

```bash
cargo run --locked -p nsb-data-tools --bin verify_assets -- \
  --manifest crates/nsb/data/manifest.toml
```

The registry must cover every file, checksum, required provenance field, schema,
license statement, and maturity declaration. Build-time checks and runtime
metadata must agree with the manifest.

Complete the [Release checklist](../operations/release-checklist.md) and update the
changelog, model maturity, validation matrix, and user-facing component
behaviour when the default composition changes.

## Updating data for a specific observatory

Observatory-specific work falls into three categories:

### Geometry only

No new data product is required. Use explicit longitude, latitude, and height, or
add a documented CLI alias. The generic clear-sky profile remains scientifically
unchanged.

### Planning profile

Add explicit atmospheric and airglow assumptions with `PlanningPreset` maturity,
provenance, regression tests, and documentation. Do not label the profile as
calibrated.

### Calibrated observatory profile

Prepare immutable site-reference inputs and validation evidence for pressure,
Rayleigh behaviour, aerosol/Mie parameters, airglow continuum and temporal
corrections, and their effect on NSB results. Register any runtime data, add a
stable site-profile identifier, expose the configuration path, and promote to
`Calibrated` only after the documented comparisons pass.

The user- and developer-facing steps are described in
[Observatory configuration and customisation](../user-guide/observatory-customization.md).

## Reproducibility record

For each released data product retain, at minimum:

- immutable input inventory and checksums;
- source catalogue identity and license policy;
- exact generation command and tool version;
- scientific contract and schema version;
- deterministic output checksums;
- exclusions and reconciliation evidence;
- validation report and independent reference;
- uncertainty and completeness assessment;
- production admission decision;
- runtime manifest and asset-registry entry.

Historical experiments may remain as frozen evidence, but only the current
capability-oriented command path belongs in operational documentation.
