# Integrated starlight

Status: Current runtime and data-product guide.
Audience: Users selecting Starlight and maintainers producing maps.
Scope: Map-backed calculation, offline generation, production admission, and limits.

## What it is

Integrated starlight is the unresolved flux of catalogue stars along a line of
sight. NSB evaluates it from an immutable Galactic HEALPix map prepared offline;
runtime evaluation never downloads Gaia, Hipparcos, Tycho, or other catalogues.

A Starlight map is a scientific data product, not merely a cache. It carries
provenance, checksum, calibration, validation-domain, uncertainty, and licence
metadata.

## Runtime behaviour

The runtime path is:

```text
ICRS/J2000 target direction
  -> transform to Galactic coordinates
  -> HEALPix pixel lookup in the admitted map
  -> apply optional non-negative scale
  -> return diagnostics and 300–650 nm photon radiance
```

Atmospheric propagation is not applied inside the Starlight component. The
admitted HEALPix product is treated as top-of-atmosphere sky radiance.
Wavelength-resolved transport belongs with spectral products; a band-integrated
map must not be treated as monochromatic radiance.

## First-release boundary

NSB 0.1.0 does **not** ship a Gaia-derived production Starlight map. No
candidate map, merge report, staged runtime map, or candidate review bundle is
kept in the first-release data registry.

This is deliberate. A production Starlight product must first satisfy the
current generation and validation contracts, complete redistribution/licensing
review, and then be admitted by an explicit release change.

Consequently:

- `ComponentMask::ALL` does not silently substitute an unapproved Starlight map;
- `StarlightProduct::validated_external` can consume a caller-supplied product
  only when its map and sidecar pass the fail-closed admission contract;
- `StarlightProduct::with_experimental_map` remains available for explicit
  experimental use without promoting that map to production.

## Producing a map

The `nsb-data` tooling supports reproducible acquisition, source joining,
photometric/UV correction, bright-star supplementation, sparse Galactic HEALPix
accumulation, deterministic merging, validation, packing, and promotion.

Generated candidates remain outside the release registry until all admission
requirements pass. The first public candidate format is
`nsb-healpix-starlight-candidate-v1`; the first production runtime formats are
`nsb-healpix-starlight-v1` and `nsb-starlight-runtime-manifest-v1`.

## Related documentation

- [Starlight data-product pipeline](map-generation.md)
- [Starlight science requirements](science-requirements.md)
- [Starlight map validation](map-validation.md)
- [External Starlight manifest](external-manifest.md)
- [Redistribution and attribution](licensing/README.md)
- [Atmospheric transport](../../specifications/atmospheric-transport.md)
