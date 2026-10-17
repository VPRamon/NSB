# Restricted source-level XP interval evaluation

`xp_source_intervals` evaluates a small source list without generating an all-sky
map. It uses the existing canonical DataLink parser and BP interval integrator;
no spectral reconstruction or covariance formula is duplicated. It changes no
runtime API, production asset, selection policy or default model.

Keep downloaded catalogue records, acquisition manifests and output fluxes outside
Git. The source manifest has a `files` array whose entries contain `source_id` and
`sha256`; additional acquisition fields are permitted. Each source must have a
single-row official DataLink CSV named `<source_id>.csv` in the input directory.
Empty manifests, duplicate IDs, malformed records, input checksum mismatches and
existing output paths fail. Errors do not create a partial result.

The manifest, design fixture and each source CSV are read once into byte buffers.
SHA-256 verification and parsing/calibration consume the same bytes, so replacing
an input path after verification cannot change the reported calculation. The
report retains the verified manifest, design and source checksum identities.

```sh
cargo run --locked -p nsb-data-tools --example xp_source_intervals -- \
  "$PRIVATE_SOURCE_DIRECTORY" "$PRIVATE_MANIFEST" "$MANIFEST_SHA256" \
  crates/nsb-data-tools/tests/fixtures/gaiaxpy_continuous_design_v375wi_v142r.json \
  e8dc899916ed29d4a92ee6ff7ec3d88a85f9c26d9c7e375ef6a6b3dd76666539 \
  "$NEW_PRIVATE_OUTPUT"
```

Use the actual design SHA-256 printed by `sha256sum`; the pinned design identity
for this experiment is
`e8dc899916ed29d4a92ee6ff7ec3d88a85f9c26d9c7e375ef6a6b3dd76666539`.

The output contains the 360-570 nm photon flux, seven adjacent 30 nm photon fluxes,
and their full within-source covariance, in photons/(m² s) and its squared unit.
It flags positivity but applies no stellar population selection. Callers must
separately enforce duplicate/nonstellar exclusions and any scientific admission
policy. Unknown systematics remain null; mean-spectrum variability, proper motion,
source avoidance, telescope PSF and atmospheric transport are separate operations.

The DESI observational pilot in the validation repository used 54 restricted XP
records through this example. A successful interval evaluation does not establish
a complete stellar population or admit a Starlight accuracy score.

Reports are serialized before staging, then written and synced in the destination
directory and published without replacement. Concurrent invocations cannot
truncate each other's output; only one can publish a given path. Failed
publication removes the temporary file. Source identifiers must be canonical
positive decimal u64 values, and report rows are sorted by numeric source ID.

Run the synthetic source/covariance, negative-input and concurrent-publication
regressions with:

```sh
cargo test --locked -p nsb-data-tools --example xp_source_intervals
```
