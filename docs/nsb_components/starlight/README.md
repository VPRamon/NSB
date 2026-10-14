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

## Pending, registered and published configurations

While the authoritative #103 decision is pending, bundled production Starlight
is absent and the default mask contains the other three components. Explicit
selection of missing bundled Starlight fails. The frozen scientific candidate
is tracked as nonembedded evidence, excluded from runtime packages. This is a
current authorization boundary, not a permanent version restriction.

After a valid human decision, the existing promotion command deterministically
reproduces and admits the compact map/sidecar, updates the production registry
and current inventory, and enables `ALL`, `DEFAULT` and `NsbEvaluator::new()`
with all four components. Python uses the same Rust assets and defaults.
Evaluation needs neither the source checkout nor a network connection.
`validated_external` and explicitly experimental map APIs remain available;
there is no synthetic bundled fallback.

Production registration does not claim an upload. The immutable approval
inventory records planned channels; the current inventory records registration
and, after publication, the channels actually used. See the
[activation and publication procedure](release-candidate/README.md#approval-and-publication-lifecycle).
Published release facts are established from the tagged registry, checksums,
notices and successful distribution checks.

## Producing and admitting a map

The frozen combined 300–650 nm scientific product uses candidate-v5,
196,608 Galactic HEALPix pixels and checksum-pinned validation/provenance.
Its packed CSV is a scientific comparison reference. Production uses
`nsb-healpix-starlight-binary-v1` and a
`nsb-starlight-runtime-manifest-v2` sidecar: all radiances and uncertainty f64
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
