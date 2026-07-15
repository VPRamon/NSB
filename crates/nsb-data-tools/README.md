# nsb-data-tools

Offline Rust commands for acquiring, transforming, validating, and releasing NSB
scientific data products. Runtime evaluation never invokes this crate and never
downloads catalogues.

The normative command inventory is
[`tool-registry.toml`](tool-registry.toml). The complete human-readable reference
is [Data-tool reference](../../docs/maintainer-guide/tools.md), and the end-to-end
operational procedure is [Updating scientific data](../../docs/maintainer-guide/updating-data.md).

## Design policy

Every retained command represents a durable capability for an external verifier,
researcher, or release maintainer. All compiled data-product commands are Rust.
Historical phase executables, shell wrappers, and Python reconstruction programs
are not supported command surfaces.

Command adapters under `src/bin/` must stay thin:

1. parse command-line arguments;
2. initialize logging;
3. construct typed configuration;
4. call reusable library or `tool_services` code;
5. return a stable success or failure status.

Generated catalogues, maps, checkpoints, and reports belong in a caller-selected
output directory outside the repository.

## Maturity levels

- **supported** — durable maintainer capability with a fail-closed contract;
- **experimental** — useful research capability whose output is not production
  approved.

Candidate generation and production admission are separate. A successful command
does not imply that an artifact is approved for runtime use.

## Asset verification and release

- `verify_assets` verifies the runtime asset manifest, file coverage, schemas,
  provenance fields, and checksums.
- `pack_starlight_asset` packages an already validated starlight map and runtime
  manifest. Production packaging fails closed when evidence is incomplete.

```bash
cargo run --locked -p nsb-data-tools --bin verify_assets -- \
  --manifest crates/nsb/data/manifest.toml
```

## Catalogue preparation

- `prepare_gaia_starlight_catalogue` converts official Gaia XP sampled data into
  canonical passband-integrated starlight source rows with exact exclusions and
  diagnostics.
- `prepare_tycho_starlight_catalogue` performs the corresponding controlled
  experimental conversion for Tycho inputs.

## Gaia acquisition and indexing

- `query_gaia_tap` executes reproducible TAP jobs with persisted evidence and
  explicit resume.
- `generate_gaia_starlight_release_inputs` prepares the Gaia metadata and input
  bundle required by downstream release processing.
- `download_gaia_xp_continuous_bulk` downloads checksum-verified official XP
  continuous partitions.
- `index_gaia_xp_continuous_bulk` builds deterministic partition/source lookup
  data from verified partitions.

## Gaia XP continuous contract

- `normalize_xp_continuous_coefficients` converts official coefficient records
  into the canonical Rust schema.
- `reconstruct_canonical_coefficients` performs calibrated spectral
  reconstruction, photon integration, and uncertainty propagation entirely
  in-process in Rust.
- `validate_xp_continuous_reconstruction` checks reconstruction, flux,
  uncertainty, and provenance against the frozen contract.

See [Pure-Rust Gaia XP continuous reconstruction](../../docs/GAIA_XP_CONTINUOUS_RUST.md).

## Sampling and model development

- `generate_starlight_sample_queries` generates deterministic stratified Gaia
  queries.
- `consolidate_gaia_starlight_samples` validates, deduplicates, and spatially
  splits completed results.
- `train_starlight_photometry_models` trains experimental candidate models from
  frozen datasets.

## Starlight generation and validation

- `build_starlight_map` builds one deterministic HEALPix candidate.
- `sweep_starlight_nside` evaluates candidate resolutions.
- `validate_starlight_map` produces structural and scientific validation
  evidence.
- `audit_gaia_starlight_exclusions` reconciles source exclusions exactly.
- `build_integrated_starlight_product` combines approved contribution inputs and
  records unresolved admission blockers.

The machine-readable purpose, ownership, input/output contract, resume semantics,
and exit-code contract for every command remain authoritative in
[`tool-registry.toml`](tool-registry.toml).
