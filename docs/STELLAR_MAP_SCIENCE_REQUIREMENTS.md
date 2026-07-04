# Starlight science requirements

Status: Production requirements for future bundled starlight products.
Audience: Scientific reviewers and maintainers.
Scope: Release-blocking evidence before starlight can be bundled or treated as
production by default.
Non-goals: This document is not a generation recipe or a validation report; see
[Starlight data-product pipeline](STELLAR_MAP_GENERATION.md) and
[Starlight map validation](STELLAR_MAP_VALIDATION.md).

Production starlight is a derived, validated map product, not a user-supplied
Gaia workflow. Maintainers retrieve official Gaia DR3 data at release time,
process it through Siderust Gaia/passband APIs, validate the result, and embed
only the derived map asset.

## Document Path

Use this document to decide whether a product is scientifically admissible. Use
[Starlight data-product pipeline](STELLAR_MAP_GENERATION.md) for maintainer
commands, [Starlight map validation](STELLAR_MAP_VALIDATION.md) for validation
report semantics, [Validated external starlight manifest](EXTERNAL_STARLIGHT_MANIFEST.md)
for caller-supplied maps, and [Model maturity](MODEL_MATURITY.md) for the
metadata exposed to users.

Release-blocking requirements:

1. Gaia DR3 source catalogue with recorded release, license/policy, row count,
   and SHA-256. Production source selection must require
   `has_xp_sampled = 'true'`, `duplicated_source = 'false'`, finite
   coordinates/ref epoch, and the reviewed `phot_g_mean_mag` cut.
2. Raw Gaia extracts are release artifacts only and are not embedded in `nsb`.
3. Photometry model is `gaia_dr3_xp_photon_radiance_330_650nm_v1`.
4. Wavelength band is 330-650 nm unless an explicitly validated extension is
   implemented.
5. Source rows pass Siderust Gaia raw-to-domain validation.
6. XP sampled spectra pass Siderust spectral validation and photon-flux
   integration.
7. Map generation records canonical input checksum, map checksum, command,
   timestamp, catalogue provenance, and limitations.
8. Validation covers finite/nonnegative values, full HEALPix coverage,
   seam/wrap behavior, Galactic plane/pole contrast, center/reference regions
   where feasible, and independent astrophysical comparison.
9. Production metadata rejects proxy, experimental, placeholder, manual-seed,
   or missing validation evidence.
10. `ComponentMask::ALL` may include starlight only after a real bundled
    production asset and validation report are present.

The existing manual seed remains an experimental fixture for plumbing tests.
