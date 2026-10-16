# Generic Starlight spectral map contract (design draft)

Status: **design proposal, not an implemented/published schema**. Tracking issues:
[data-tools #259](https://github.com/VPRamon/NSB/issues/259) and
[runtime #260](https://github.com/VPRamon/NSB/issues/260). Scientific baseline:
[#257](https://github.com/VPRamon/NSB/pull/257); restricted-band API prerequisite
[#254](https://github.com/VPRamon/NSB/pull/254).

## Architecture

`nsb-data-tools` owns catalogue-specific acquisition, calibration, source selection,
HEALPix accumulation, deterministic product generation, scientific audit, and data
provenance. `nsb` owns a **single** `StarlightMap` API for compatible map
representations, scientific capability checks, and runtime atmospheric transport.

`StarlightProduct` stays the sole selector in `NsbModelConfig`: bundled
production, explicit experimental, or independently admitted external production.
Parsing a versioned schema or verifying a checksum is **not** production admission.
No dataset- or band-specific `StarlightMap` class is required.

## Serialized representation (proposed v2)

A new schema ID (`nsb-starlight-spectral-map-v2`) keeps **frozen existing
formats unchanged**. The following shows the semantic model, not a canonical
byte serialization or an API commitment:

```json
{
  "schema": "nsb-starlight-spectral-map-v2",
  "product_id": "illustrative-measured-xp-map",
  "bin_edges_nm": [360, 390, 420, 450],
  "nside": 1,
  "ordering": "ring",
  "coordinate_frame": "galactic",
  "domain": "top-of-atmosphere",
  "wavelength_convention": "vacuum",
  "units": "photon/(cm2 ns sr)",
  "maturity": "experimental",
  "uncertainties": {
    "per_bin_statistical_sigma": true,
    "integrated_statistical_sigma": true,
    "per_bin_systematic_sigma": false,
    "cross_bin_statistical_covariance": false
  },
  "provenance": {
    "dataset_name": "Illustrative catalogue",
    "source_catalogue": "Illustrative catalogue release",
    "source_selection": "Illustrative measured-only policy",
    "generation_method": "Versioned source integration method",
    "source_license": "Illustrative terms; verify before distribution",
    "upstream_sha256": {}
  },
  "pixels": [
    {
      "pixel": 0,
      "sources": 1,
      "radiance": [1.0, 2.0, 3.0],
      "statistical_sigma": [0.1, 0.2, 0.3],
      "integrated_statistical_sigma": 0.48
    }
  ]
}
```

This example is intentionally **not** a valid full nside=1 HEALPix product:
a real nside=1 map must contain 12 validated grid-ordered pixels. Final
admission schema must specify full required provenance, population accounting,
present/missing uncertainty encoding, field-level error conditions, and
size/resource limits before generation or runtime parsers are implemented.

- Each `radiance[i]` is the **band-integrated** mean photon radiance for
  `[bin_edges_nm[i], bin_edges_nm[i+1]]`, **not** spectral density.
- Edges must be finite, strictly increasing, contiguous as represented, and
  within the scientifically supported integration domain; bin widths need
  not be equal. `len(radiance) = len(edges) - 1`; no hard-coded seven-bin
  array. Spectral coverage is exactly the first and last edge.
- All pixels use the declared uncertainty-channel schema, with known
  nonnegative finite entries; **missing uncertainties are unknown, not zero**.
- A supplied `integrated_statistical_sigma` is independently computed
  with full relevant source covariance; it must not be reconstructed from
  per-bin sigmas unless the joint covariance is actually provided.
- For all-zero/empty pixels, all present radiance and uncertainty channels
  must be zero.
- Geometry must have exact full HEALPix coverage in declared RING/NESTED
  order, with Galactic frame and explicit pixel IDs.
- Canonical product bytes are verified against an **out-of-band SHA-256**
  (or an independently pinned manifest), not a circular in-document hash.
- Unknown schema revisions or unsafe unit/frame/domain claims fail closed.
- Gaia-specific source IDs, design identities, exclusions, and manifest
  details belong in **typed, versioned provenance records**, not hard-coded
  runtime class names.

## Scientific operations and capability checks

1. Exact integrated radiance is available only for the product's native
   full band or a union of **complete stored bins**, with exactly aligned
   requested endpoints. Never interpret per-bin integrals as samples of a
   continuous spectral density.
2. Partial-bin queries cannot return **exact** radiance. Under an explicitly
   nonnegative underlying source spectrum, lower/upper bounds may be available
   from fully included and intersected bins; these must be typed as bounds.
   Wavelength extrapolation is unsupported.
3. Exact statistical uncertainty for arbitrary multi-bin unions requires
   their covariance (or a specifically stored matching integrated variance).
   Missing covariance means `unknown`; a clearly labelled upper bound such
   as the sum of sigma contributions is different from exact sigma.
4. Direct atmospheric transmission envelopes may be calculated using the
   existing `nsb::transport` models only if their monotonicity/nonnegative
   assumptions hold. This is **not** stellar in-scattering or source-masked
   fiber radiance.
5. No default evaluator may treat a 360–570 nm map as the 300–650 nm
   Starlight component. Coverage/capability checks must precede selection
   for each evaluation contract.

## Compatibility and migration

Existing 300–650 nm production packed CSV/compact maps and caller-built
`StarlightMap`/`StarlightPixel` retain their interpretation and exact
runtime outputs. Their existing public constructors and `pixel_at()`
semantics must remain intact under the API freeze.

The existing Gaia XP **360–570 nm experimental v1** map remains a separately
verified input, with exact original digest:

`fe331343cb9797a1d871ae798c88d2ae8865ae39358e8d82b464a91b3cf87d5f`

A backwards-compatible **strict v1 reader/adapter** can map its payload into
a generic internal representation without altering bytes or bypassing its
original provenance and population checks. Writing it as v2 would produce
a *different* asset identity and is not a silent migration.

The existing infallible `StarlightMap::pixel_at()` returns an integrated
sample with the historical 300–650 nm meaning. Do **not** silently return a
360–570 nm value or fake missing coverage from this method. The runtime
implementation issue must address this signature constraint explicitly,
e.g. with a **new fallible band-scoped inspection method** for spectral maps
while leaving legacy behavior unchanged; no runtime panic is acceptable.

## Open design decisions before implementation

- Exact typed envelope for optional per-pixel uncertainties and covariance,
  including a compact channel-major or pixel-major layout suitable for large
  nside maps.
- Safe legacy `pixel_at()` handling for generic spectral-only maps under the
  frozen API, with regression tests on semantics and no hidden fallbacks.
- Limits on accepted bin counts, memory, payload bytes and nside, independent
  of metadata claims.
- Canonical serialized versioned provenance record and production admission
  sidecar policy, without turning a schema ID into a production certificate.
- Where generic top-hat integration is surfaced on `NsbEvaluator` (#254),
  explicit support/rejection should follow the selected map's coverage and
  completeness rather than relying on class identity.

Scientific reproducibility and safe errors take precedence over convenience.
No frozen Gaia measurements, source selection, calibration coefficients,
source accounting, or default NSB predictions change in this design step.
