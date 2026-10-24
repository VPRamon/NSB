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

## Approved, registered and published configurations

The active [redistribution decision](release-candidate/redistribution-review-decision-v1.json)
is `approved_with_conditions` for the exact regenerated v1 artifact hashes.
The approved compact binary and provenance sidecar are registered for offline
four-component Rust/Python defaults. Approval remains scoped to the recorded
channels, Gaia noncommercial restrictions and source-specific attribution; it
is separate from the AGPL code license and does not establish publication.
`validated_external` and explicitly experimental map APIs remain available.
See the [reset record](release-candidate/schema-reset-v1.md) for the recorded
byte equivalence and the checksum-bound external evidence still required before
release.

Production registration does not claim an upload. The immutable approval
inventory records planned channels; the current inventory records registration
and, after publication, the channels actually used. See the
[activation and publication procedure](release-candidate/README.md#approval-and-publication-lifecycle).
Published release facts are established from the tagged registry, checksums,
notices and successful distribution checks.

## Producing and admitting a map

The frozen combined 300–650 nm scientific product uses candidate-v1,
196,608 Galactic HEALPix pixels and checksum-pinned validation/provenance.
Its packed CSV is a scientific comparison reference. Production uses
`nsb-healpix-starlight-binary-v1` and a
`nsb-starlight-runtime-manifest-v1` sidecar: all radiances and uncertainty f64
values are lossless, with declared RING/NESTED ordering, units and SHA-256.
The [format and evidence contract](release-candidate/README.md#compact-runtime-transport-binary-v1)
describes the exact encoding and admission checks. Older CSV/external contracts
remain supported but are not the compact release product.

Production requires technical/independent scientific evidence and the separate
human redistribution decision. Software is AGPL-3.0-only; protected Gaia data
retain BY-NC conditions and all source-specific notices. No software licence
waives those restrictions. Only the exact approved compact pair may enter the
Git/crates.io/PyPI channels named in the decision.

## Related documentation

- [Starlight data-product pipeline](map-generation.md)
- [Starlight science requirements](science-requirements.md)
- [Starlight map validation](map-validation.md)
- [External Starlight manifest](external-manifest.md)
- [Redistribution and attribution](licensing/README.md)
- [Atmospheric transport](../../specifications/atmospheric-transport.md)

## Archived validation provenance

The pinned candidate gates and validation reports are historical evidence.
Their references to `performance-v1.json`, `documentation-wording-audit-v1.md`,
`starlight-final-promotion.yml` and the retired CSV benchmark describe earlier
runs, not current tools. Retrieve those inputs from the
[pre-reset revision](https://github.com/VPRamon/NSB/tree/3b3a59ca33e1396d6cf879226efc5cb232cd315e).
Current publication checks live in `publish.yml`; numerical validation and
compact runtime performance remain covered by the maintained tests/benchmarks.
