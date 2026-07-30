# Integrated starlight

Status: Current runtime and data-product guide.
Audience: Users selecting starlight and maintainers producing maps.
Scope: Map-backed calculation, offline generation, production admission, and limits.

## What it is

Integrated starlight is the unresolved flux of catalogue stars along a line of
sight. Unlike the other components, NSB does not calculate it from a compact
analytic formula or query a catalogue at runtime. It evaluates an immutable,
directional Galactic HEALPix map prepared offline.

## How NSB calculates it

```text
ICRS/J2000 target direction
  -> transform to Galactic coordinates
  -> HEALPix pixel lookup in the selected map
  -> apply optional non-negative scale
  -> return spectral/diagnostic values and 300–650 nm photon radiance
```

The map is the scientific input to the calculation. Runtime evaluation is local
and deterministic: it never downloads Gaia, Tycho, or any other catalogue.

## How the map is generated and admitted

The current production candidate path begins with official Gaia DR3 XP sampled
bulk data. Offline tools reconstruct the fixed photon-radiance contract,
prepare canonical sources, bin their flux into a Galactic HEALPix map, and
produce diagnostics. Candidate maps are then validated for coverage, finite and
non-negative values, longitude wrapping, plane/pole behaviour, and, when source
totals are available, flux conservation.

Production use additionally requires provenance, an exact checksum and header
contract, calibrated non-proxy photometry, a validation report, and independent
comparison evidence. A map and its manifest are admitted together; integrity
alone does not establish scientific validity.

## Runtime selections

- `starlight` uses a bundled production asset when one is registered and
  validated, or a caller-provided map plus manifest that passes the fail-closed
  admission contract.
- `experimental-starlight` selects the bundled 12-pixel manual seed only.
- `Starlight::with_map` and the experimental library path allow explicit maps,
  but do not promote them to production.

There is no fallback from production starlight to the experimental seed.
Accordingly, `--components all` contains starlight only if a production asset is
available.

## Scientific boundaries

The bundled manual seed is explicitly experimental and is not a catalogue
product. The Gaia DR3 XP pipeline produces candidates; promotion remains a
separate scientific and release decision. Missing-flux treatment, independent
validation, and redistribution-policy gates must be satisfied before a map is
represented as production quality.

## Release-candidate status

**Production-ready release candidate pending final human approval.**

The technical promotion mechanism (issue #89) is complete: a fail-closed
`nsb-data dataset starlight promote` command and the
`nsb-starlight-release-candidate-v1` schema exist and are tested, but the
current candidate checksum is scientifically invalidated pending #94/#95
regeneration, and both human decisions in issue #47 remain `pending`.
`ComponentMask::ALL` and the CLI's `--components starlight` selection do not
fall back to the experimental seed and do not admit an unregistered
production map; see
[`release-candidate/README.md`](release-candidate/README.md) for the full
gate contract.

## Related documentation

- [Starlight data-product pipeline](map-generation.md)
- [Provenance of existing starlight datasets](existing-datasets.md)
- [Starlight science requirements](science-requirements.md)
- [Starlight map validation](map-validation.md)
- [External starlight manifest](external-manifest.md)
- [Redistribution and licensing package](licensing/README.md)
- [Release-candidate bundle and promotion mechanism](release-candidate/README.md)
