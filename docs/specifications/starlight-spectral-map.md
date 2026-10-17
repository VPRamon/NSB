# Starlight spectral maps: generic v2 contract

The Starlight map runtime is `nsb::components::starlight::StarlightMap`
(Rust) or `nsb.StarlightMap` (Python). Its exact full-band and
native-bin integration interface works with a legacy integrated map, the
frozen seven-bin Gaia XP v1 map, or a compatible variable-bin v2 map.
The public API does **not** depend on catalogue identity or bin count.

## Data producer

`nsb-data-tools` keeps its established official Gaia acquisition,
XP covariance calibration, source admission and exclusion, HEALPix
Galactic accumulation, per-partition receipts and deterministic finalization.

Existing `product_band = "measured-360-570"` produces the original
v1 schema and exactly seven 30 nm bins, unchanged.

A new v2 XP configuration instead declares:

```toml
[starlight]
product_band = "measured-xp-binned"
spectral_bin_edges_nm = [360, 390, 420, 480, 570]

[starlight.map]
canonical_nside = 128
```

Supply the existing official checksum-pinned `gaia_products` pair,
workspace, execution policy and runtime paths as with the v1 candidate.
The example config requests four **nonuniform** integrated bins and
produces `starlight_spectral_candidate.json` and the paired
`starlight_spectral_validation.json`. New spectral products remain
experimental and are never bundled or production-promoted automatically.

## Serialized format

The v2 `nsb-starlight-spectral-map-v2` JSON document contains the
following **required** fields:

- `schema`, nonempty `product_id`, `band_nm` and `bin_edges_nm`.
- HEALPix `nside`, `ordering` (`ring` or `nested`), `coordinate_frame`
  (`galactic`); `domain` (`top-of-atmosphere`).
- `units` (`photon/(cm2 ns sr)`), `wavelength_convention` (`vacuum`),
  `maturity` (`experimental`).
- `provenance` with required strings `source_catalogue`,
  `source_license`, `source_selection`, `generation_method`, and
  nonempty `limitations`. The optional, **typed** `gaia` subrecord
  captures calibration design SHA, input partition/shard checksum identities
  and XP population counts where present.
- `catalogue_rows`, `admitted_sources`, `excluded_sources`, and a complete,
  indexed array of `pixels`.

Each pixel records `pixel`, `sources`, and an array `radiance` with
exactly `len(bin_edges_nm) - 1` nonnegative finite **bin-integrated**
mean photon radiances. `statistical_sigma` and
`integrated_statistical_sigma` may be omitted where scientifically
unavailable, but must be consistently present or absent across the map.
An empty pixel cannot carry nonzero radiance or any present uncertainty.
Source counts must reconcile to `catalogue_rows`. Unknown fields,
unknown schema revisions, malformed bin edges, inconsistent units,
wrong HEALPix coverage/order and corrupt checksums fail closed.

A v2 product's **out-of-band SHA-256** pins exact bytes. Loading a
checksummed map is never sufficient for production admission; the existing
`ValidatedStarlightMap` provenance, scientific and licensing gates remain
independent.

### Frozen v1 candidate

The exact legacy Gaia XP 360–570 nm JSON bytes must not change:

`fe331343cb9797a1d871ae798c88d2ae8865ae39358e8d82b464a91b3cf87d5f`

The generic runtime dispatches strictly to a v1 adapter and preserves
the original four explicitly-null provenance declarations, seven bin
edges, source accounting and uncertainty validation. Neither this
adapter nor v2 generation regenerates the archived full Gaia product.

## Scientific semantics

`StarlightMap::bin_edges_nm` reports native bin edges and `band_nm`
returns the map's native total coverage. Integrals are exact **only**
when both endpoints coincide with native edges. A partial-bin integral
cannot be reconstructed without spectral assumptions, and requests outside
coverage fail. A sum over multiple stored bins provides exact **radiance**
but not exact statistical uncertainty without cross-bin covariance. A
supplied full-native-band sigma comes from covariance-aware calibration
and is preserved; absent uncertainty stays `None`.

`StarlightMap::direct_pixel_bounds` applies the existing shared
Rayleigh/Mie direct transmission model under its explicit monotonicity
conditions. The envelope does **not** include stellar in-scattering, DESI
source-mask/instrument response or missing bright-star/photometric
completion. No observational accuracy improvement is claimed.

A spectral-only map can be selected explicitly in `StarlightProduct`,
but its **TOA** native-band integrals are queried through
`StarlightMap::integrate_band`. `NsbEvaluator::integrate_band` continues
to reject Starlight, because adding a TOA pixel mean to a ground-level
component total without atmospheric/instrumental transport would be
physically misleading. The evaluator's legacy full-band `evaluate`
rejects every TOA spectral map, including one spanning 300–650 nm.
Band coincidence alone does not provide ground-level atmospheric transport.

## Examples

Rust (schematic imports omitted):

```rust
let map = StarlightMap::from_json_file(path, expected_sha256)?;
let edges = map.bin_edges_nm();
let (radiance, sigma) = map.integrate_band(
    galactic_direction,
    Nanometers::new(f64::from(edges[0])),
    Nanometers::new(f64::from(edges[1])),
)?;
```

Python:

```python
import nsb

starlight = nsb.StarlightMap.load("starlight_spectral_candidate.json", sha256)
print(starlight.bin_edges_nm)
radiance, sigma = starlight.integrate_band(120.0, 30.0, 360.0, 390.0)
```

Scientific source auditing and the historical 300–650 nm production asset
are unchanged.


## Compiled provenance

The data-tool build script watches workspace crates (including embedded data and
build scripts), Cargo manifests/lockfile and `.cargo` configuration, plus Git HEAD,
its symbolic ref, the index and packed refs when present. A change in a linked
workspace crate refreshes the embedded identity without `cargo clean`. Dirty or
unverifiable source is never labelled as a clean commit. Reports and `target/`
are not watched build inputs. Cargo's incremental tracking is verified using a
small isolated workspace, detached HEAD and linked worktree.
