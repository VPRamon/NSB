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
Atmospheric propagation is **not** applied inside the Starlight component: the
admitted HEALPix product is treated as top-of-atmosphere sky radiance.
Band-integrated map values must not be treated as monochromatic radiance for
transport; wavelength-resolved transport belongs with spectral products. See
[Atmospheric transport](../../specifications/atmospheric-transport.md).

## How the map is generated and admitted

The current frozen candidate starts with official Gaia DR3 source/XP inputs and
adds the checksum-pinned bright-star supplement finalized in #211. That
supplement is built offline from externally supplied Hipparcos-2/Tycho-2/XHIP
inputs and pinned CK04 spectral templates; raw catalogue bytes are not committed
or shipped. The candidate manifest and merge report retain the exact input
provenance and checksums. Offline tools reconstruct the fixed photon-radiance
contract, merge the admitted source populations, bin their flux into a Galactic
HEALPix map, and produce diagnostics. Candidate maps are then validated for
coverage, finite and non-negative values, longitude wrapping, plane/pole
behaviour, and flux/accounting consistency.

Production use additionally requires provenance, an exact checksum and header
contract, calibrated non-proxy photometry, a validation report, and independent
comparison evidence. A map and its manifest are admitted together; integrity
alone does not establish scientific validity.

## Runtime selections

- `starlight` uses a bundled production asset when one is registered and
  validated, or a caller-provided map plus manifest that passes the fail-closed
  admission contract.
- `StarlightProduct::with_experimental_map` selects an explicit caller-supplied
  map without promoting it to production; validated external products enter
  through `ValidatedStarlightMap` plus `StarlightProduct::validated_external`.

There is no bundled experimental seed. Accordingly, `--components all` contains
starlight only if a production asset is available.

Normal applications configure the product through `NsbModelConfig` and evaluate
through `NsbEvaluator`. The concrete directional evaluator and its component-only
output are internal. `StarlightMap`, `StarlightPixel`, provenance, and admission
types remain available as the advanced product construction/inspection API.

## Scientific boundaries

Scientific/technical readiness and redistribution approval are separate gates.
The frozen combined 300–650 nm candidate is tied to checksum-pinned technical
and external cross-implementation validation in the release-candidate bundle.
That evidence does not grant permission to redistribute the candidate as
bundled production data.

## Release-candidate status

PR #211 froze the current nside-128 combined 300–650 nm candidate, including the
covariance-corrected bright-star supplement. The release-candidate bundle makes
its scientific/technical readiness machine-verifiable; a separate human
scientific signature is not required.

Issue #103 tracks the remaining human redistribution/licensing decision. It
explicitly does **not** block the `0.1.0` MVP: while the decision is pending,
the candidate and staged runtime bytes remain `calibration_status = "candidate"`,
`runtime_embedded = false`, excluded from crates.io/PyPI packages, and outside
`ComponentMask::ALL`.

`nsb-data dataset starlight stage-runtime` deterministically prepares the
runtime map/sidecar for review without granting redistribution.
`nsb-data dataset starlight promote` fails closed unless the frozen evidence and
an authorized redistribution decision agree. Production activation still
requires a follow-up change that registers the approved runtime asset.

## Related documentation

- [Starlight data-product pipeline](map-generation.md)
- [Provenance of existing starlight datasets](existing-datasets.md)
- [Starlight science requirements](science-requirements.md)
- [Starlight map validation](map-validation.md)
- [External starlight manifest](external-manifest.md)
- [Redistribution and licensing package](licensing/README.md)
- [Release-candidate bundle and promotion mechanism](release-candidate/README.md)
