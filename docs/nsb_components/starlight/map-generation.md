# Starlight dataset generation

Starlight uses the common dataset lifecycle with the Gaia production
configuration:

```bash
nsb-data dataset starlight update --config crates/nsb-data-tools/config/starlight-production.toml
nsb-data dataset starlight build --config crates/nsb-data-tools/config/starlight-production.toml
nsb-data dataset starlight validate --config crates/nsb-data-tools/config/starlight-production.toml
nsb-data dataset starlight publish --config crates/nsb-data-tools/config/starlight-production.toml
```

The production configuration imports the official GaiaSource and XP continuous
checksum inventories. Both products must expose the same source-range
partitions. Downloads enter the content-addressed cache only after checksum
verification. Local and Slurm workers use the same Rust implementation and
write isolated, strictly validated partition shards.

The combined candidate finalized in #211 also integrates a checksum-pinned
bright-star supplement built offline from externally supplied
Hipparcos-2/Tycho-2/XHIP inputs and pinned CK04 templates. Those catalogue
bytes remain external and are not shipped in the repository or release
packages. Their exact source identities/checksums and the supplement merge
accounting are retained in the candidate manifest and merge report; the
supplement is part of the one Starlight data product, not a second runtime
component.

## One canonical map

Each Starlight dataset version has exactly one `canonical_nside`:

```toml
[starlight.map]
canonical_nside = 128
```

Every Gaia source contribution is accumulated directly into that resolution.
The reconciled shards produce:

```text
starlight_nside{canonical_nside}.csv
merge_report.json
```

The current candidate is nside 128. Changing `canonical_nside` changes the
configuration checksum and run identity and requires a clean source-level
generation, fresh report, validation, provenance, and scientific review. A
higher-resolution release must never use a lower-resolution map as its input.

The canonical candidate uses a sparse, strictly pixel-sorted representation.
Omitted HEALPix pixels have zero integrated flux and zero source counts; the
report records both the occupied row count and the full `12 * nside^2` pixel
domain. `flux_ph_m2_s` is integrated photon flux per HEALPix pixel in
`ph m-2 s-1`.
Runtime queries may convert a pixel-integrated quantity into the runtime
radiance contract using pixel solid angle; that does not make the candidate CSV
a surface-radiance field.

## Galactic heatmap

A generated candidate can be inspected as a Galactic Mollweide heatmap with
the Rust example under `crates/nsb/examples`:

```bash
cargo run --release --locked -p nsb --example starlight_heatmap -- \
  --map crates/nsb/data/starlight_nside128.csv \
  --output starlight_heatmap.png
```

The example validates the Starlight candidate contract used by the plot
(`nside`, Galactic frame, NESTED ordering, sparse zero-flux omission semantics,
integrated-per-pixel quantity and flux unit), then uses Siderust HEALPix
geometry and Plotters to render the map. The default logarithmic normalization
exposes both the Galactic plane and fainter high-latitude structure; use
`--norm linear` for a linear colour scale.

The title includes the candidate SHA-256 prefix so screenshots remain tied to
exact map bytes. Plotting support is already a development-only dependency of
the `nsb` crate and does not add a runtime dependency to the library or Python
package.

Resolution selection, when needed, is a separate scientific study comparing
independent source-level runs. Only the selected candidate is published.
Diagnostic resampling is outside the scientific publication lifecycle.

A production Gaia-derived replacement must satisfy the
[science requirements](science-requirements.md), [validation
contract](map-validation.md), redistribution policy, and [runtime manifest
contract](external-manifest.md).

Operational recovery and publication are documented in the
[dataset maintainer guide](../../maintainer-guide/datasets.md). Reference
artifacts and limitations are recorded in
[Provenance of existing starlight datasets](existing-datasets.md).

## Production hardening note

The full Gaia DR3 run encountered an upstream XP row with
`bp_n_parameters=null`. Canonical parsing excludes records that cannot be
calibrated and retains exact partition/source accounting. If a Slurm partition
fails, rerun only that partition and then repeat validation before publication.
