# Starlight runtime: compatibility-preserving implementation plan

Status: **implementation design / pending code**, tracked by
[#260](https://github.com/VPRamon/NSB/issues/260) and the data/schema
prerequisite [#259](https://github.com/VPRamon/NSB/issues/259).
This draft runtime branch is stacked on the current scientific PR #257.

## Existing behavior that must not change

- `StarlightMap::from_healpix`, `from_csv_str`, `from_csv_path`,
  `from_compact_bytes`, and `StarlightMap::pixel_at` currently model a
  validated **legacy integrated** Galactic HEALPix Starlight map.
- `StarlightPixel::integrated` has a documented 300–650 nm meaning. Returning
  a Gaia XP 360–570 integrated number through that same field would silently
  alter physical semantics; it is **not** an acceptable generic adapter.
- `StarlightProduct::{BundledProductionGaiaDr3, ExperimentalMap,
  ValidatedExternalMap}` and `NsbModelConfig::with_starlight_product` remain
  the caller's only product-selection API. Production admission requires the
  existing manifest contract. Default NSB radiance must not change.
- The 0.1 public API signatures/snapshot are frozen; extending a shared type
  must not remove, rename or change existing methods or enum variants.

## Runtime representation to prototype

One public `StarlightMap`; internally a compatible integrated representation
and optionally a checked spectral payload with **shared per-map bin edges**,
HEALPix pixel rows and compact contiguous per-pixel channels (prefer one
`Vec<f64>` per channel, not one `Vec` allocation per pixel). Physical
quantities must be typed at the public boundary (qtty/NSB radiance and
Nanometers), with byte-level source SHA/provenance exposed as read-only data.

Implementation should maintain a strict legacy internal representation
separate from the new spectral-only case, rather than filling fake legacy
300–650 values. Existing `pixel_at` must retain old semantics for all legacy
constructed maps; for spectral-only maps, design the safe compatibility route
without silent relabelling or panic. If no safe contract is possible for that
infallible method, document the API freeze conflict explicitly and seek the
maintainer-approved compatibility decision before making it public.

The new **fallible** spectral query path should accept Galactic direction and
an explicit top-hat band, rather than assuming native pixel output matches
the NSB 300–650 system band. Use the existing HEALPix grid and coordinate
frame conversions; no parallel geometry stack.

## Capability algorithm

Given `edges = [e0..en]` and a user interval `[a,b]`:

1. First validate `0 < a < b`, in coverage `e0 <= a < b <= en`.
2. For **exact** radiance, require `a` and `b` to match stored bin edges.
   Sum each fully contained integrated photon-radiance bin. No per-nm
   division, interpolation, or extrapolation.
3. If an exact interval matches the native **full coverage**, return its
   specifically packed integrated statistical sigma when recorded.
   For a single native bin, return that bin's measured sigma.
   A union of 2+ bins without their stored cross-bin covariance has unknown
   exact sigma; return `None`/typed unsupported instead of quadrature.
4. If partial-bin integration is requested, exact operation must fail.
   A **separate** bounds API may use nonnegative source spectra: fully
   covered bins contribute to both bounds; intersected edge bins contribute
   between zero and their full integrated radiance. These are bounds, not
   a nominal reconstructed spectrum.
5. For direct transmission, reuse the same `nsb::transport` Rayleigh/Mie
   implementation and its physical preconditions; envelope monotonicity
   assumptions must be checked. Missing scattering/instrument response
   must remain explicit.

## Format reader / identity

- Dispatch only explicitly recognized versions: legacy packed CSV/compact
  through existing validated paths, frozen XP-only interval JSON v1 through a
  **strict legacy adapter**, and the agreed generic v2 schema from #259.
- The frozen XP v1 input is identified by the external pinned SHA:
  `fe331343cb9797a1d871ae798c88d2ae8865ae39358e8d82b464a91b3cf87d5f`.
  Keep the original bytes and reject unsupported v1 provenance.
- For generic products, load checksum-pinned bytes and validate schema,
  positive ordered bin edges, exact HEALPix completeness, finite nonnegative
  radiance and uncertainty, missing channel policy, source accounting when
  recorded, frame, domain, wavelength convention, and units.
- A valid checksum **never** confers production admission. An experimental
  spectral-only product must not satisfy the legacy `NsbEvaluator` default
  band contract.

## Python boundary

Expose one `nsb.StarlightMap` mirror of the chosen Rust API; old Python
`StarlightIntervalMap` is not the target public design. Keep PyO3 thin and
translate incompatible inputs to the existing documented typed error classes.
Demonstrate identical coverage, exact integration, partial-bin rejection and
missing-uncertainty behavior with installed-wheel tests.

## First implementation targets

1. Add failing-focused Rust tests for the data-model and 3/7/10-bin synthetic
   maps, invalid edges, partial bands, covariance-limited sigma and corrupted
   checksum; keep existing integrated baseline unchanged.
2. Implement a versioned internal reader / spectral storage. Do not expose
   new public types until the `pixel_at` compatibility decision is solved.
3. Add generic fallible band-scoped methods and capability metadata. Preserve
   the `StarlightProduct` variants, and reject incompatible coverage in
   `NsbEvaluator`.
4. Add thin PyO3 bindings; remove map-specific public Python API only after
   generic parity exists and compatibility policy has been reviewed.
5. Refresh documented public API snapshot intentionally and run MSRV, Clippy,
   workspace tests, numerical and scientific integration tests, wheels and
   API SemVer checks.

This document records a target design, **not an implemented runtime API**.
