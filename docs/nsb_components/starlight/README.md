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
- `Starlight::with_map` and `StarlightModel::with_experimental_map` allow
  explicit caller-supplied maps, but do not promote them to production.

There is no bundled experimental seed. Accordingly, `--components all` contains
starlight only if a production asset is available.

## Scientific boundaries

The Gaia DR3 XP pipeline produces candidates; promotion remains a separate
scientific and release decision. Missing-flux treatment, independent
validation, and redistribution-policy gates must be satisfied before a map is
represented as production quality.

## Release-candidate status

Technical packing and post-approval promotion automation are implemented
under issue #102. The frozen UV-v2 candidate remains scientifically
unapproved. Human scientific and redistribution review is issue #103, the
only remaining Starlight production blocker after #102 closes.

`nsb-data dataset starlight promote` packs a runtime-loadable RING HEALPix
map from the immutable candidate-v5 bytes. `gates.promotion_eligible` is
report-only; eligibility is the conjunction of frozen CI gates, the packed
runtime checksums, and the two signed #103 decisions. The final-promotion
workflow applies production registry entries and opens the promotion PR
after those decisions exist. Pipeline `validation_status = technical_pass`
is not independent scientific validation
(`no_admissible_independent_reference`; see
[independent-reference-audit-v1.md](validation/independent-reference-audit-v1.md)).

## Related documentation

- [Starlight data-product pipeline](map-generation.md)
- [Provenance of existing starlight datasets](existing-datasets.md)
- [Starlight science requirements](science-requirements.md)
- [Starlight map validation](map-validation.md)
- [External starlight manifest](external-manifest.md)
- [Redistribution and licensing package](licensing/README.md)
- [Release-candidate bundle and promotion mechanism](release-candidate/README.md)
