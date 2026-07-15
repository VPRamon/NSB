# Scientific metadata and uncertainty

Status: Current metadata contract for API and CLI outputs.
Audience: Users interpreting NSB results and maintainers changing output
metadata.
Scope: Component maturity, provenance, uncertainty, B/V diagnostics, and asset
audit fields.
Non-goals: This document does not promote any component beyond the evidence
listed in [Validation matrix](validation.md).

Every `NsbComponent` includes a maturity status, provenance, validated domain,
band diagnostic, and optional relative uncertainty. CLI JSON preserves those
fields; CSV v1 provides equivalent columns.

## Status vocabulary

| Status | Meaning |
|---|---|
| `Production` | Validated for a stated release domain |
| `GenericClearSky` | Generic atmosphere/sky assumptions, not site-calibrated |
| `PlanningPreset` | Named assumptions suitable for planning only |
| `Proxy` | Approximate conversion or diagnostic |
| `PublishedReference` | Supported published comparison model, not the default |
| `Experimental` | Capability without a production validation contract |

The default components report generic or planning status. No CTAO profile is
currently calibrated.

## Physical integrated band and B/V diagnostics

`integrated` is photon radiance over 300–650 nm. `b_flux_s10`, `v_flux_s10`,
`b_mag`, and `v_mag` use `BandDiagnostic::MONOCHROMATIC_S10_PROXY`:

- B reference: 445 nm;
- V reference: 551 nm;
- surface-brightness zero point: 27.78;
- convention: `monochromatic-central-wavelength-s10-proxy`.

These are diagnostics, not Johnson passband integrations. Starlight's
V-S10-to-integrated factor is additionally a proxy and remains outside defaults.
Zodiacal, airglow, and Jones moonlight integrated values are derived from
wavelength-resolved spectra, while their B/V fields still use central samples.

## Component uncertainty

Airglow exposes relative one-sigma uncertainty when the empirical temporal and
spectral tables support it. Zodiacal, moonlight, and starlight currently expose
`None`; consumers must propagate maturity and provenance instead of interpreting
missing uncertainty as zero.

Validated external starlight metadata includes dataset/release/license, source
selection and checksum, map checksum/resolution, photometry model, generation
command, validation report, independent comparison, and calibration status.
The library and CLI emit `Production` only after the strict external manifest
contract passes. A photometry model containing `proxy` or `experimental` is
rejected from that path and can only use experimental APIs.

Principal unquantified terms are catalogue completeness, aerosol/atmosphere
mismatch, intrinsic airglow variability, solar-spectrum provenance, scattering
approximations, and target geometry near model boundaries.

## Asset provenance

`crates/nsb/data/manifest.toml` is authoritative for file schema, SHA-256,
source, license, generator, generation command, validation report, calibration
status, and runtime inclusion. Runtime JSON exposes checksums for every embedded
asset. Incomplete inherited provenance is an explicit scientific limitation.
External starlight uses the equivalent sidecar contract because its bytes are
not part of the bundled registry.
