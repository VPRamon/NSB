# Starlight independent validation report

- Issue: #87
- Generated (unix seconds): 1787573939
- Band: 300-650 nm (ph_m-2_s-1)
- Candidate map: `crates/nsb/data/starlight_nside128.csv`
- Candidate map SHA-256: `5946fa170b1be911b8996ac4a36200133743bac6ba39a1392358cd3007a91563`
- Pinned checksum verified against: `5946fa170b1be911b8996ac4a36200133743bac6ba39a1392358cd3007a91563`

## Scientific review status

`scientific_review_status = "pending"`, `scientifically_validated = false`. This pipeline never marks a candidate as scientifically validated on its own; that decision is recorded only by a qualified human scientist in issue #47.

## Technical gates

`technical_gates_passed = false`

- no acquired-and-transformed reference data was available; validation is pending acquisition (see #87/#47)

## Reference status

| Reference | Status | Detail |
|---|---|---|
| toller-1981-pioneer-background-starlight | not-admissible | Pioneer 10 Galactic-pole photometry measures ISL+DGL+EBL; diffuse galactic light is inseparable from discrete starlight in the 2.3 deg FOV. Acquired for provenance only. |
| leinert-1998-diffuse-night-sky-brightness | not-admissible | Leinert et al. 1998 describe a two-dimensional Gaussian fitted to Elsässer & Haug (1960) isophotes and quote five S10 anchors. The published paper does not give the Gaussian amplitudes and widths needed to reconstruct that surface. Matching those anchors with an invented interpolation is not the registered model, so this reference is acquired for provenance only and is not an admissible comparison grid. |
| masana-2021-gambons-gaia-hipparcos-starlight | not-admissible | GAMBONS all-sky products mix Gaia/Hipparcos ISL with DGL, EBL, zodiacal light and airglow. Not an admissible TOA Galactic starlight-only 300-650 nm grid. |

No reference produced computed metrics in this run: all references are pending acquisition, or acquired but not yet transformed onto the candidate grid. No metrics were invented to fill this gap.

## Notes

Technical scaffolding for issue #87. Scientific approval is recorded only in issue #47 and is never inferred from this report.
