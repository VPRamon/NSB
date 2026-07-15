# Updating scientific data

Status: Current maintainer runbook.
Audience: Scientific-data maintainers and release maintainers.
Scope: Reproducible acquisition, transformation, validation, admission, and release of NSB data products.

This runbook describes the supported process for updating scientific data. It is
not a single command: each product must preserve provenance, checksums, versioned
schemas, scientific validation, and explicit admission evidence.

## Non-negotiable rules

- Runtime evaluation never downloads or regenerates scientific data.
- Work in a caller-selected output directory outside the repository.
- Pin source identity, release, query/inventory, license policy, and checksums.
- Candidate generation and production admission are separate decisions.
- A successful command does not imply scientific approval.
- `NotRun`, missing evidence, and checksum mismatches fail production admission.
- Never replace a reviewed runtime asset in place without updating its manifest,
  validation evidence, and release history.

## Choose the update path

| Update | Start with | Typical final tool |
| --- | --- | --- |
| Verify existing bundled assets | `verify_assets` | `verify_assets` |
| Refresh Gaia metadata or TAP-derived inputs | `query_gaia_tap` or `generate_gaia_starlight_release_inputs` | Product-specific validator |
| Refresh official XP continuous bulk inputs | `download_gaia_xp_continuous_bulk` and `index_gaia_xp_continuous_bulk` | `run_starlight_xp_continuous_bulk_pipeline` while experimental |
| Rebuild a canonical source catalogue | `prepare_gaia_starlight_catalogue` | `audit_gaia_starlight_exclusions` |
| Rebuild a starlight HEALPix candidate | `build_starlight_map` | `validate_starlight_map` |
| Build an integrated starlight candidate | Contribution producers and `build_integrated_starlight_product` | `pack_starlight_asset` after all gates pass |
| Reassess map resolution | `sweep_starlight_nside` | `validate_starlight_map` |

The complete command contracts and maturity levels are in the
[data-tool reference](tools.md). The machine-readable authority is
[`tool-registry.toml`](../../crates/nsb-data-tools/tool-registry.toml).

## 1. Create an isolated update workspace

Use immutable input and output locations. Do not write generated files into the
source tree while exploring or processing.

```bash
export NSB_DATA_RUN=/data/nsb-runs/$(date -u +%Y%m%dT%H%M%SZ)
mkdir -p "$NSB_DATA_RUN"/{inputs,work,outputs,manifests,reports}

git rev-parse HEAD > "$NSB_DATA_RUN/manifests/software_commit.txt"
rustc --version --verbose > "$NSB_DATA_RUN/manifests/rustc.txt"
cargo metadata --locked --format-version 1 \
  > "$NSB_DATA_RUN/manifests/cargo-metadata.json"
```

Record any external service endpoint, query text, inventory URL, source release,
license statement, and local storage policy alongside the run.

## 2. Verify the current repository state

Before changing data, establish that the checked-out assets are internally
consistent:

```bash
cargo run --locked -p nsb-data-tools --bin verify_assets -- \
  --manifest crates/nsb/data/manifest.toml

cargo test --locked -p nsb-data-tools --test tool_registry
cargo test --locked -p nsb-data-tools --test data_product_architecture_contract
```

Resolve failures before generating a replacement. Otherwise the update cannot
show which defects were pre-existing.

## 3. Acquire or refresh source data

### Gaia TAP and metadata

Use `query_gaia_tap` for a pinned, reviewable ADQL request. Use
`generate_gaia_starlight_release_inputs` when preparing the complete metadata and
normalized-input bundle expected by the starlight workflow.

Persist:

- exact ADQL;
- service endpoint and job identifier;
- request and completion timestamps;
- result schema and row count;
- source/product release;
- checksums and license policy.

Resume only through the tool's persisted manifest. Do not treat an unverified
partial result as complete.

### Official Gaia XP continuous bulk

Use the official inventory and checksum set:

```bash
cargo run --locked --release -p nsb-data-tools \
  --bin download_gaia_xp_continuous_bulk -- --help

cargo run --locked --release -p nsb-data-tools \
  --bin index_gaia_xp_continuous_bulk -- --help
```

The download step may reuse only complete checksum-verified partitions. The index
must reconcile file and source counts with the pinned inventory.

## 4. Normalize and transform inputs

Choose the transformation that matches the source product:

- `prepare_gaia_starlight_catalogue` converts Gaia XP sampled inputs into the
  canonical starlight source schema;
- `normalize_xp_continuous_coefficients` converts raw XP continuous records into
  the canonical coefficient schema;
- `reconstruct_canonical_coefficients` performs the in-process Rust calibration,
  spectral reconstruction, photon integration, and uncertainty calculation;
- `export_starlight_healpix_to_contributions` converts an existing HEALPix map
  into contribution rows for controlled integrated-product work.

Every transformation must emit or be accompanied by:

- input checksums;
- schema and model versions;
- accepted, excluded, and failed counts;
- deterministic output checksums;
- software identity and calibration/reference identity;
- explicit limitations.

Do not use the phase-numbered holdout or mini-pilot commands for a new release
workflow. They remain transitional evidence and orchestration dependencies only.

## 5. Build the candidate product

### Canonical map candidate

```bash
cargo run --locked --release -p nsb-data-tools \
  --bin build_starlight_map -- --help
```

Supply the canonical catalogue, explicit HEALPix configuration, catalogue
identity, and checksums. Write maps and diagnostics to the run output directory.

Use `sweep_starlight_nside` when resolution itself is under evaluation. A sweep
recommendation selects a candidate; it does not admit a production asset.

### Integrated product candidate

Use `build_integrated_starlight_product` only with versioned contribution inputs
and frozen policies. Record every population branch, correction, uncertainty,
coverage limitation, and blocker.

`export_starlight_healpix_to_contributions` is an experimental bridge for
controlled candidate work. Its output does not by itself establish complete
300–650 nm coverage or production readiness.

## 6. Validate and reconcile

Run the validators that correspond to the changed product:

- `audit_gaia_starlight_exclusions` for canonical source/exclusion accounting;
- `validate_xp_continuous_reconstruction` for frozen reconstruction parity and
  uncertainty tolerances;
- `validate_starlight_map` for map structure, scientific checks, independent
  references, and production gates;
- pipeline reconciliation for partition, source, exclusion, and HEALPix totals.

A validation report must distinguish at least:

- passed gates;
- failed gates;
- gates not run;
- missing evidence;
- candidate-only limitations;
- production blockers.

Counts must reconcile exactly. Document intentional exclusions individually or by
a stable, reviewable policy with reproducible evidence.

## 7. Package a proposed runtime asset

Only after all required production gates pass:

```bash
cargo run --locked --release -p nsb-data-tools \
  --bin pack_starlight_asset -- --help
```

Packaging must produce the runtime artifact and manifest together. Review that
the manifest records:

- stable asset and schema versions;
- checksum algorithm and digest;
- source catalogue/product identity;
- generation software and configuration;
- license and redistribution constraints;
- maturity and calibration status;
- validated domain and uncertainty;
- validation report identity.

Never copy a candidate into `crates/nsb/data/` manually and infer its metadata
later.

## 8. Admit the asset into the repository

For a reviewed production proposal:

1. add or replace the runtime asset under `crates/nsb/data/`;
2. update `crates/nsb/data/manifest.toml` in the same change;
3. add immutable validation fixtures or compact evidence required for review;
4. update `MODEL_MATURITY.md`, `VALIDATION.md`, and relevant scientific docs;
5. update the changelog with scientific and operational impact;
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

Use the full [release checklist](../RELEASE_CHECKLIST.md) before tagging or
distributing the result.

## 9. Preserve the update record

Keep the complete run directory outside Git. Commit only material required for
reproducibility and review:

- source query or official inventory identity;
- compact policies and schemas;
- checksums and manifests;
- deterministic validation fixtures;
- reviewed reports or summaries whose provenance is explicit;
- the admitted runtime asset.

Large raw catalogues, caches, checkpoints, and ad hoc operational logs should
remain in managed external storage.

## Failure and rollback

If any production gate fails, retain the candidate and reports externally, record
the blocker, and do not alter the runtime manifest. If an admitted asset must be
rolled back, revert the asset and manifest as one unit and rerun `verify_assets`
and the workspace tests.

## Related references

- [Data-product workflow](data-products.md)
- [Data-tool reference](tools.md)
- [Data-product pipeline architecture](../DATA_PRODUCT_PIPELINE_ARCHITECTURE.md)
- [Starlight science requirements](../STELLAR_MAP_SCIENCE_REQUIREMENTS.md)
- [Starlight generation](../STELLAR_MAP_GENERATION.md)
- [Starlight validation](../STELLAR_MAP_VALIDATION.md)
- [Model maturity](../MODEL_MATURITY.md)
- [Validation matrix](../VALIDATION.md)