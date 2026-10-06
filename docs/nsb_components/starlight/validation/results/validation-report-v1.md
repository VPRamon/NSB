# Starlight independent validation report

- Issue: #102
- Generated (unix seconds): 1791227050
- Band: 300-650 nm (ph_m-2_s-1)
- Candidate map: `crates/nsb/data/starlight_nside128.csv`
- Candidate map SHA-256: `7e903ff289e76d07c018933b8f97fcf264cead73999912ff63f34b9d1e01b37d`
- Pinned checksum verified against: `7e903ff289e76d07c018933b8f97fcf264cead73999912ff63f34b9d1e01b37d`

## Scientific production gate

`scientific_gate = "external_validation_required"`. This literature-audit pipeline supplies evidence; checksum-pinned external cross-validation is the authoritative machine-verifiable scientific gate.

## Technical gates

`technical_gates_passed = false`

- no_admissible_independent_reference: every acquired literature target is not admissible as a starlight-only TOA 300-650 nm comparison grid; use the checksum-pinned cross-implementation validation route

`independent_reference_status = no_admissible_independent_reference`

## Reference status

| Reference | Status | Detail |
|---|---|---|
| toller-1981-pioneer-background-starlight | not-admissible | Pioneer 10 Galactic-pole photometry measures ISL+DGL+EBL; diffuse galactic light is inseparable from discrete starlight in the 2.3 deg FOV. Acquired for provenance only. |
| leinert-1998-diffuse-night-sky-brightness | not-admissible | Leinert et al. 1998 describe a two-dimensional Gaussian fitted to Elsässer & Haug (1960) isophotes and quote five S10 anchors. The published paper does not give the Gaussian amplitudes and widths needed to reconstruct that surface. Matching those anchors with an invented interpolation is not the registered model, so this reference is acquired for provenance only and is not an admissible comparison grid. |
| masana-2021-gambons-gaia-hipparcos-starlight | not-admissible | GAMBONS all-sky products mix Gaia/Hipparcos ISL with DGL, EBL, zodiacal light and airglow. Not an admissible TOA Galactic starlight-only 300-650 nm grid. |

No reference produced computed metrics in this run. Acquired literature targets may be not-admissible rather than unacquired; see `independent_reference_status`. No metrics were invented to fill this gap.

## Notes

Independent-validation technical audit for issue #102. Scientific production readiness is established separately by checksum-pinned external cross-validation.
