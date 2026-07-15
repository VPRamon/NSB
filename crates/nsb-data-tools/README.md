# nsb-data-tools

Offline Rust tools for building, validating and releasing NSB scientific data
products. Runtime NSB never invokes this crate.

The normative command inventory is
[`tool-registry.toml`](tool-registry.toml). Every compiled binary must be listed
there with its owner, audience, maturity, purpose, input/output contract, resume
semantics, exit-code contract and documentation anchor. CI rejects undocumented
commands, phase-numbered binaries, unregistered Python/shell programs,
developer-specific absolute paths and generated machine reports committed as
source.

## Design policy

A retained command must provide a durable capability useful to an external user,
researcher or release maintainer. Historical development steps, one-off policy
freezes, pilot runners and phase-finalization executables are not part of the
supported command surface. Their reusable algorithms remain library code and
their scientific evidence remains as frozen fixtures or documentation.

All compiled commands are Rust. The remaining Python files are explicitly
migration-only or test-only reference implementations for GaiaXPy parity and are
tracked for removal by issue #61. Shell orchestration is not supported.

The command boundary should stay thin:

1. parse arguments;
2. initialize logging;
3. construct typed configuration;
4. call reusable library code;
5. return a stable success or failure status.

Generated products and operational reports belong under a caller-selected output
directory, never at the repository root.

## Maturity levels

- **supported**: durable maintainer capability with a fail-closed contract;
- **experimental**: useful research capability whose scientific output is not
  production approved;
- **migration-only**: temporary non-Rust reference required only while #61 is
  open;
- **test-only**: helper used exclusively to verify temporary migration evidence.

Candidate generation and production admission are separate. A successful
candidate command does not imply that a product is approved for runtime use.

## Asset verification and release

### `verify_assets`

Verifies the runtime asset registry, file coverage, schemas, required metadata
and checksums.

The normative command inventory is
[`tool-registry.toml`](tool-registry.toml). The complete human-readable reference
is [Data-tool reference](../../docs/maintainer-guide/tools.md), and the end-to-end
operational procedure is [Updating scientific data](../../docs/maintainer-guide/updating-data.md).

This command is suitable for CI and external release verification. It exits zero
only when every registered asset passes.

### `pack_starlight_asset`

Packages a validated Starlight map and its runtime manifest. Production mode is
fail-closed: incomplete provenance, missing validation evidence or inconsistent
checksums must prevent packaging as a production asset.

The command is deterministic from immutable inputs and has no implicit resume
state.

## Catalogue preparation

### `prepare_gaia_starlight_catalogue`

Converts official Gaia DR3 XP sampled bulk files, or a normalized controlled
fallback, into canonical passband-integrated Starlight source rows. It streams
gzip inputs, verifies source provenance and writes deterministic exclusions and
diagnostics.

```bash
cargo run --locked --release -p nsb-data-tools \
  --bin prepare_gaia_starlight_catalogue -- \
  --bulk-dir /data/gaia-dr3-xp-sampled \
  --output /data/starlight/gaia_dr3_starlight_sources.csv \
  --diagnostics-output /data/starlight/gaia_dr3_starlight_sources.diagnostics.json \
  --exclusions-output /data/starlight/gaia_dr3_starlight_exclusions.csv \
  --catalog-name Gaia \
  --catalog-release DR3 \
  --catalog-license "$GAIA_DERIVED_PRODUCT_LICENSE_POLICY" \
  --photometry-model gaia_dr3_xp_photon_radiance_336_650nm_v1 \
  --band-min-nm 336 \
  --band-max-nm 650
```

Production conversion rejects malformed spectra, incomplete provenance,
inconsistent source accounting and invalid photometry. `--exclusions-only` may
regenerate the deterministic exclusions evidence without rewriting an already
validated canonical catalogue.

### `prepare_tycho_starlight_catalogue`

Experimental converter for controlled Tycho BT/VT studies. Its photometric
transform is not a production Starlight calibration. The command verifies the
input checksum and writes canonical rows plus diagnostics.

## Gaia acquisition and indexing

### `query_gaia_tap`

Executes reproducible Gaia TAP jobs with persisted manifests, retries, result
validation and explicit resume behavior. A resumed job may reuse only a
persisted valid result or continue the same asynchronous service job.

### `generate_gaia_starlight_release_inputs`

Generates the Gaia metadata query, optionally retrieves metadata, merges
normalized XP inputs, computes checksums and emits the downstream release-input
configuration. The official bulk path is preferred; Gaia DataLink is a
controlled fallback.

```bash
cargo run --locked -p nsb-data-tools \
  --bin generate_gaia_starlight_release_inputs -- \
  --out-dir /data/starlight/release-inputs \
  --max-g-mag 20.0 \
  --band-min-nm 336 \
  --band-max-nm 650 \
  --license-policy-file docs/policies/gaia_dr3_starlight_derived_product_policy.txt \
  --validation-reference validation/starlight_independent_reference_v1.json \
  --production \
  --resume
```

Production mode fails on missing products, parse failures, missing provenance or
unreconciled source counts.

### `download_gaia_xp_continuous_bulk`

Downloads official Gaia DR3 XP continuous bulk partitions from a pinned
inventory. Resume reuses only checksum-verified partitions. Partial, truncated
or checksum-mismatched files are never promoted as complete.

### `index_gaia_xp_continuous_bulk`

Builds a deterministic partition/source index from checksum-verified official
bulk files. The index must reconcile with the inventory before the command exits
successfully.

## Gaia XP continuous contract

### `normalize_xp_continuous_coefficients`

Normalizes official bulk or DataLink coefficient records into the versioned
canonical Rust schema. It validates coefficient dimensions, errors, packed
correlations, source provenance and exact rejection accounting.

### `validate_xp_continuous_reconstruction`

Validates reconstructed spectra, integrated photon flux, uncertainty and
calibration provenance against frozen scientific tolerances. It is read-only and
deterministic.

GaiaXPy-based reconstruction and parity scripts remain temporary migration
evidence only. They are not a supported user interface and will be removed by
#61 after the pure-Rust reconstruction passes the frozen oracle corpus.

## Sampling and model development

### `generate_starlight_sample_queries`

Generates deterministic stratified Gaia ADQL queries from the versioned sampling
contract.

### `consolidate_gaia_starlight_samples`

Inventories completed jobs, validates results, deduplicates sources and applies
the frozen spatial split. Reruns reuse persisted TAP results but recompute the
canonical consolidated outputs deterministically.

### `train_starlight_photometry_models`

Experimental model-development command. It consumes frozen train, validation and
test splits and writes candidate coefficients plus holdout metrics. Its outputs
are not production approved without the independent validation and admission
steps tracked by issue #47.

## Starlight generation and validation

### `build_starlight_map`

Builds a full-sky deterministic HEALPix map from a canonical source catalogue.
Coordinate transforms, HEALPix primitives and generic validators are delegated
to Siderust.

### `sweep_starlight_nside`

Builds candidate maps at multiple HEALPix resolutions or reassesses persisted
artefacts without rereading the source catalogue.

```bash
cargo run --locked --release -p nsb-data-tools \
  --bin sweep_starlight_nside -- \
  --output-dir /data/starlight/sweep \
  --assess-existing \
  --catalog-checksum "sha256:$CATALOG_SHA"
```

Candidate recommendation and production admission remain separate. Use
`--require-production-ready` only when a failing production gate must produce a
non-zero status.

### `validate_starlight_map`

Produces the structural, scientific and independent-reference validation report
for a generated map. Missing evidence and failed requested gates are errors.

### `audit_gaia_starlight_exclusions`

Reconciles the scientific exclusions sidecar against the canonical Gaia source
inventory. Every exclusion must be unique, justified and accounted for.

### `build_integrated_starlight_product`

Combines approved population contributions into an integrated Starlight
candidate and records explicit production blockers. It must not present a
candidate as production-ready while any admission gate remains unresolved.

## Removed historical commands

Phase-numbered executables and shell wrappers were removed because they encoded
completed development steps rather than durable capabilities. This includes
Phase 5 preparation/finalization, holdout freezing/finalization, pilot runners,
chunk benchmarks, merge/resume probes and one-off reconciliation audits.

Reusable parsing, uncertainty, validation and modelling code remains in library
modules and automated tests. Frozen policy files, reports and scientific fixtures
remain available for reproducibility.

Do not reintroduce a historical command under a new name. Add a new command only
when it has a durable audience and outcome, then register it in
`tool-registry.toml` with complete contracts.

## Complete command contracts

The full machine-readable contract for every retained command is maintained in
[`tool-registry.toml`](tool-registry.toml). That file is the source of truth for:

- ownership and intended audience;
- maturity;
- purpose;
- input and output contracts;
- resume/idempotency behavior;
- exit-code semantics;
- documentation location.
