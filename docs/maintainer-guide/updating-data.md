# Updating scientific data

Status: Current maintainer runbook.
Audience: Scientific-data maintainers and release maintainers.
Scope: Reproducible acquisition, transformation, validation, admission, and release of NSB data products.

Scientific-data updates are controlled workflows, not a single command. Every
product must preserve provenance, checksums, versioned schemas, validation, and
an explicit admission decision.

## Non-negotiable rules

- Runtime evaluation never downloads or regenerates scientific data.
- Work in a caller-selected directory outside the repository.
- Pin source identity, release, query or inventory, license policy, and checksums.
- Candidate generation and production admission are separate.
- A successful command does not imply scientific approval.
- `NotRun`, missing evidence, and checksum mismatches block production admission.
- Replace a reviewed runtime asset only together with its manifest, validation
  evidence, and release record.

## Choose the update path

| Update | Start with | Validate or finish with |
| --- | --- | --- |
| Verify existing bundled assets | `verify_assets` | `verify_assets` |
| Refresh Gaia TAP-derived inputs | `query_gaia_tap` or `generate_gaia_starlight_release_inputs` | Product-specific validator |
| Refresh official XP continuous inputs | `download_gaia_xp_continuous_bulk`, then `index_gaia_xp_continuous_bulk` | `normalize_xp_continuous_coefficients`, `reconstruct_canonical_coefficients`, and `validate_xp_continuous_reconstruction` |
| Rebuild a canonical Gaia source catalogue | `prepare_gaia_starlight_catalogue` | `audit_gaia_starlight_exclusions` |
| Rebuild a starlight HEALPix candidate | `build_starlight_map` | `validate_starlight_map` |
| Build an integrated starlight candidate | Versioned contribution inputs and `build_integrated_starlight_product` | `pack_starlight_asset` after all gates pass |
| Reassess map resolution | `sweep_starlight_nside` | `validate_starlight_map` |

The complete contracts are in the [data-tool reference](tools.md). The
machine-readable authority is
[`tool-registry.toml`](../../crates/nsb-data-tools/tool-registry.toml).

## 1. Create an isolated update workspace

```bash
export NSB_DATA_RUN=/data/nsb-runs/$(date -u +%Y%m%dT%H%M%SZ)
mkdir -p "$NSB_DATA_RUN"/{inputs,work,outputs,manifests,reports}

git rev-parse HEAD > "$NSB_DATA_RUN/manifests/software_commit.txt"
rustc --version --verbose > "$NSB_DATA_RUN/manifests/rustc.txt"
cargo metadata --locked --format-version 1 \
  > "$NSB_DATA_RUN/manifests/cargo-metadata.json"
```

Record external endpoints, exact queries, official inventories, source releases,
license statements, and storage policy alongside the run.

## 2. Verify the current repository state

```bash
cargo run --locked -p nsb-data-tools --bin verify_assets -- \
  --manifest crates/nsb/data/manifest.toml

cargo test --locked -p nsb-data-tools --test tool_registry
cargo test --locked -p nsb-data-tools --test data_product_architecture_contract
```

Resolve failures before generating a replacement so the update can distinguish
pre-existing defects from newly introduced ones.

## 3. Acquire or refresh source data

### Gaia TAP and metadata

Use `query_gaia_tap` for a pinned ADQL request. Use
`generate_gaia_starlight_release_inputs` for the complete metadata and normalized
input bundle expected by starlight release processing.

Persist:

- exact ADQL and endpoint;
- service job identifier and timestamps;
- result schema and row count;
- source/product release;
- checksums and license policy.

Resume only through persisted, verified state.

### Official Gaia XP continuous bulk

```bash
cargo run --locked --release -p nsb-data-tools \
  --bin download_gaia_xp_continuous_bulk -- --help

cargo run --locked --release -p nsb-data-tools \
  --bin index_gaia_xp_continuous_bulk -- --help
```

The downloader may reuse only checksum-verified partitions. The index must
reconcile partition and source counts with the pinned official inventory.

## 4. Normalize and reconstruct inputs

Choose the transformation that matches the source product:

- `prepare_gaia_starlight_catalogue` converts Gaia XP sampled inputs into the
  canonical starlight source schema;
- `prepare_tycho_starlight_catalogue` provides the controlled experimental Tycho
  path;
- `normalize_xp_continuous_coefficients` converts official XP continuous records
  into the canonical coefficient schema;
- `reconstruct_canonical_coefficients` performs pure-Rust calibration, spectral
  reconstruction, photon integration, and uncertainty propagation;
- `consolidate_gaia_starlight_samples` creates canonical modelling datasets from
  completed sample-query results.

Every transformation must emit or be accompanied by:

- input checksums;
- schema and model versions;
- accepted, excluded, and failed counts;
- deterministic output checksums;
- software and calibration/reference identity;
- explicit limitations.

For XP continuous reconstruction, follow
[Pure-Rust Gaia XP continuous reconstruction](../nsb_components/starlight/gaia-xp-continuous-rust.md).

## 5. Build the candidate product

### Canonical map candidate

```bash
cargo run --locked --release -p nsb-data-tools \
  --bin build_starlight_map -- --help
```

Supply the canonical catalogue, explicit HEALPix configuration, catalogue
identity, and checksums. Write maps and diagnostics to the run output directory.

Use `sweep_starlight_nside` when the resolution is itself under evaluation. A
sweep recommendation selects a candidate; it does not admit a production asset.

### Integrated product candidate

Use `build_integrated_starlight_product` only with versioned contribution inputs
and frozen policies. Record every population branch, correction, uncertainty,
coverage limitation, and unresolved admission blocker.

## 6. Validate and reconcile

Run the validators corresponding to the changed product:

- `audit_gaia_starlight_exclusions` for source/exclusion accounting;
- `validate_xp_continuous_reconstruction` for reconstruction parity,
  uncertainty, and provenance;
- `validate_starlight_map` for map structure, scientific checks, independent
  references, and production gates;
- `verify_assets` for the final runtime registry.

A report must distinguish passed, failed, not-run, and not-applicable gates.
Counts must reconcile exactly. Missing or skipped required evidence blocks
production admission.

## 7. Package a proposed runtime asset

Only after all required production gates pass:

```bash
cargo run --locked --release -p nsb-data-tools \
  --bin pack_starlight_asset -- --help
```

The artifact and manifest are one release unit. Review that the manifest records:

- stable asset and schema versions;
- checksum algorithm and digest;
- source catalogue/product identity;
- generation software and configuration;
- license and redistribution constraints;
- maturity and calibration status;
- validated domain and uncertainty;
- validation-report identity.

Never copy a candidate into `crates/nsb/data/` and infer metadata later.

## 8. Admit the asset

For a reviewed production proposal:

1. add or replace the runtime asset under `crates/nsb/data/`;
2. update `crates/nsb/data/manifest.toml` in the same change;
3. add immutable validation fixtures or compact review evidence;
4. update the model-maturity and validation specifications and relevant scientific docs;
5. update the changelog;
6. verify no generated working directories or machine-specific paths are staged.

Then run:

```bash
cargo run --locked -p nsb-data-tools --bin verify_assets -- \
  --manifest crates/nsb/data/manifest.toml
cargo test --workspace --locked
cargo test --workspace --doc --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
cargo build --workspace --release --locked
```

Use the full [release checklist](../operations/release-checklist.md) before tagging or
distributing the result.

## 9. Preserve the update record

Keep large raw catalogues, caches, checkpoints, and operational logs in managed
external storage. Commit only reviewable material required for reproducibility:

- source query or official inventory identity;
- compact policies and schemas;
- checksums and manifests;
- deterministic validation fixtures;
- reviewed reports or summaries with explicit provenance;
- the admitted runtime asset.

## Failure and rollback

When a production gate fails, retain the candidate and reports externally, record
the blocker, and do not alter the runtime manifest. Roll back an admitted asset
and its manifest as one unit, then rerun `verify_assets` and the workspace tests.

## Related references

- [Data-product workflow](data-products.md)
- [Data-tool reference](tools.md)
- [Data-product pipeline architecture](../specifications/data-product-pipeline.md)
- [Pure-Rust Gaia XP continuous reconstruction](../nsb_components/starlight/gaia-xp-continuous-rust.md)
- [Starlight science requirements](../nsb_components/starlight/science-requirements.md)
- [Starlight generation](../nsb_components/starlight/map-generation.md)
- [Starlight validation](../nsb_components/starlight/map-validation.md)
- [Model maturity](../specifications/model-maturity.md)
- [Validation matrix](../specifications/validation.md)
