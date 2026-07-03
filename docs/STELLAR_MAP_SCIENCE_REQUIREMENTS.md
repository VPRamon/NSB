# Starlight science requirements

Production starlight is a derived, validated map product, not a user-supplied
Gaia workflow. Maintainers retrieve official Gaia DR3 data at release time,
process it through Siderust Gaia/passband APIs, validate the result, and embed
only the derived map asset.

Release-blocking requirements:

1. Gaia DR3 source catalogue with recorded release, license/policy, row count,
   and SHA-256.
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
10. `ComponentMask::ALL` remains conservative until a real bundled production
    asset and validation report are present.

The existing manual seed remains an experimental fixture for plumbing tests.
