# Starlight independent validation report

- Issue: #87
- Generated (unix seconds): 1787571593
- Band: 300-650 nm (ph_m-2_s-1)
- Candidate map: `crates/nsb/data/starlight_nside128.csv`
- Candidate map SHA-256: `5946fa170b1be911b8996ac4a36200133743bac6ba39a1392358cd3007a91563`
- Pinned checksum verified against: `5946fa170b1be911b8996ac4a36200133743bac6ba39a1392358cd3007a91563`

## Scientific review status

`scientific_review_status = "pending"`, `scientifically_validated = false`. This pipeline never marks a candidate as scientifically validated on its own; that decision is recorded only by a qualified human scientist in issue #47.

## Technical gates

`technical_gates_passed = false`

- leinert-1998-diffuse-night-sky-brightness/all-sky: absolute_all_sky_bias 4.066272e10 exceeds preregistered maximum 3.000000e-2
- leinert-1998-diffuse-night-sky-brightness/all-sky: median relative error 203.737948 exceeds preregistered maximum 0.050000
- leinert-1998-diffuse-night-sky-brightness/all-sky: p95 relative error 4014.214266 exceeds preregistered maximum 0.100000
- leinert-1998-diffuse-night-sky-brightness/all-sky: coverage_68 0.000000 is outside preregistered range [0.630000, 0.730000]
- leinert-1998-diffuse-night-sky-brightness/all-sky: coverage_95 0.000432 is outside preregistered range [0.900000, 0.980000]
- leinert-1998-diffuse-night-sky-brightness/galactic-plane: median relative error 149.209626 exceeds preregistered maximum 0.050000
- leinert-1998-diffuse-night-sky-brightness/galactic-plane: p95 relative error 3500.871093 exceeds preregistered maximum 0.100000
- leinert-1998-diffuse-night-sky-brightness/galactic-plane: coverage_68 0.000000 is outside preregistered range [0.630000, 0.730000]
- leinert-1998-diffuse-night-sky-brightness/galactic-plane: coverage_95 0.000466 is outside preregistered range [0.900000, 0.980000]
- leinert-1998-diffuse-night-sky-brightness/galactic-center: median relative error 112.724249 exceeds preregistered maximum 0.050000
- leinert-1998-diffuse-night-sky-brightness/galactic-center: p95 relative error 1329.719931 exceeds preregistered maximum 0.100000
- leinert-1998-diffuse-night-sky-brightness/galactic-center: coverage_68 0.000000 is outside preregistered range [0.630000, 0.730000]
- leinert-1998-diffuse-night-sky-brightness/galactic-center: coverage_95 0.000000 is outside preregistered range [0.900000, 0.980000]
- leinert-1998-diffuse-night-sky-brightness/anticenter: median relative error 221.167379 exceeds preregistered maximum 0.050000
- leinert-1998-diffuse-night-sky-brightness/anticenter: p95 relative error 2422.949320 exceeds preregistered maximum 0.100000
- leinert-1998-diffuse-night-sky-brightness/anticenter: coverage_68 0.000000 is outside preregistered range [0.630000, 0.730000]
- leinert-1998-diffuse-night-sky-brightness/anticenter: coverage_95 0.000000 is outside preregistered range [0.900000, 0.980000]
- leinert-1998-diffuse-night-sky-brightness/poles: median relative error 322.666614 exceeds preregistered maximum 0.050000
- leinert-1998-diffuse-night-sky-brightness/poles: p95 relative error 4655.396653 exceeds preregistered maximum 0.100000
- leinert-1998-diffuse-night-sky-brightness/poles: coverage_68 0.000000 is outside preregistered range [0.630000, 0.730000]
- leinert-1998-diffuse-night-sky-brightness/poles: coverage_95 0.000376 is outside preregistered range [0.900000, 0.980000]
- leinert-1998-diffuse-night-sky-brightness/dark-fields: median relative error 38.871682 exceeds preregistered maximum 0.050000
- leinert-1998-diffuse-night-sky-brightness/dark-fields: p95 relative error 5051.629767 exceeds preregistered maximum 0.100000
- leinert-1998-diffuse-night-sky-brightness/dark-fields: coverage_68 0.000000 is outside preregistered range [0.630000, 0.730000]
- leinert-1998-diffuse-night-sky-brightness/dark-fields: coverage_95 0.000000 is outside preregistered range [0.900000, 0.980000]
- leinert-1998-diffuse-night-sky-brightness/seam-0-360: median relative error 121.733751 exceeds preregistered maximum 0.050000
- leinert-1998-diffuse-night-sky-brightness/seam-0-360: p95 relative error 1505.260016 exceeds preregistered maximum 0.100000
- leinert-1998-diffuse-night-sky-brightness/seam-0-360: coverage_68 0.000000 is outside preregistered range [0.630000, 0.730000]
- leinert-1998-diffuse-night-sky-brightness/seam-0-360: coverage_95 0.000180 is outside preregistered range [0.900000, 0.980000]
- leinert-1998-diffuse-night-sky-brightness/dense: median relative error 2566.322839 exceeds preregistered maximum 0.050000
- leinert-1998-diffuse-night-sky-brightness/dense: p95 relative error 9431.978399 exceeds preregistered maximum 0.100000
- leinert-1998-diffuse-night-sky-brightness/dense: coverage_68 0.000000 is outside preregistered range [0.630000, 0.730000]
- leinert-1998-diffuse-night-sky-brightness/dense: coverage_95 0.001017 is outside preregistered range [0.900000, 0.980000]
- leinert-1998-diffuse-night-sky-brightness/high-extinction: median relative error 110.734590 exceeds preregistered maximum 0.050000
- leinert-1998-diffuse-night-sky-brightness/high-extinction: p95 relative error 1199.174215 exceeds preregistered maximum 0.100000
- leinert-1998-diffuse-night-sky-brightness/high-extinction: coverage_68 0.000000 is outside preregistered range [0.630000, 0.730000]
- leinert-1998-diffuse-night-sky-brightness/high-extinction: coverage_95 0.000000 is outside preregistered range [0.900000, 0.980000]
- leinert-1998-diffuse-night-sky-brightness/bright-star: median relative error 486.115361 exceeds preregistered maximum 0.050000
- leinert-1998-diffuse-night-sky-brightness/bright-star: p95 relative error 4996.083893 exceeds preregistered maximum 0.100000
- leinert-1998-diffuse-night-sky-brightness/bright-star: coverage_68 0.000000 is outside preregistered range [0.630000, 0.730000]
- leinert-1998-diffuse-night-sky-brightness/bright-star: coverage_95 0.000000 is outside preregistered range [0.900000, 0.980000]
- leinert-1998-diffuse-night-sky-brightness/high-crowding: median relative error 7942.363472 exceeds preregistered maximum 0.050000
- leinert-1998-diffuse-night-sky-brightness/high-crowding: p95 relative error 16421.971650 exceeds preregistered maximum 0.100000
- leinert-1998-diffuse-night-sky-brightness/high-crowding: coverage_68 0.000000 is outside preregistered range [0.630000, 0.730000]
- leinert-1998-diffuse-night-sky-brightness/high-crowding: coverage_95 0.001525 is outside preregistered range [0.900000, 0.980000]

## Reference status

| Reference | Status | Detail |
|---|---|---|
| toller-1981-pioneer-background-starlight | not-admissible | Pioneer 10 Galactic-pole photometry measures ISL+DGL+EBL; diffuse galactic light is inseparable from discrete starlight in the 2.3 deg FOV. Acquired for provenance only. |
| leinert-1998-diffuse-night-sky-brightness | evaluated | grid sha256 965af999fcda7bb8c5a87b53d4e472b611af8b4d304d11b867a30a5a80a40afe intersected with candidate map over 11 of 11 declared regions |
| masana-2021-gambons-gaia-hipparcos-starlight | not-admissible | GAMBONS all-sky products mix Gaia/Hipparcos ISL with DGL, EBL, zodiacal light and airglow. Not an admissible TOA Galactic starlight-only 300-650 nm grid. |

## Metrics for reference `leinert-1998-diffuse-night-sky-brightness`

### Region `all-sky`

| Metric | Value |
|---|---|
| sample_count | 196608 |
| signed_bias | 4.066272e10 |
| absolute_bias | 4.066272e10 |
| relative_bias | 918.257469 |
| mae | 4.066272e10 |
| median_absolute_error | 7.087923e9 |
| rmse | 2.058650e11 |
| relative_error_p50 | 203.737948 |
| relative_error_p68 | 509.132586 |
| relative_error_p95 | 4014.214266 |
| coverage_68 | 0.000000 |
| coverage_95 | 0.000432 |
| outlier_fraction | 0.001414 |

Tolerance failures:
- absolute_all_sky_bias 4.066272e10 exceeds preregistered maximum 3.000000e-2
- median relative error 203.737948 exceeds preregistered maximum 0.050000
- p95 relative error 4014.214266 exceeds preregistered maximum 0.100000
- coverage_68 0.000000 is outside preregistered range [0.630000, 0.730000]
- coverage_95 0.000432 is outside preregistered range [0.900000, 0.980000]

### Region `galactic-plane`

| Metric | Value |
|---|---|
| sample_count | 34304 |
| signed_bias | 5.964319e10 |
| absolute_bias | 5.964319e10 |
| relative_bias | 708.096346 |
| mae | 5.964319e10 |
| median_absolute_error | 1.148315e10 |
| rmse | 2.343011e11 |
| relative_error_p50 | 149.209626 |
| relative_error_p68 | 356.705837 |
| relative_error_p95 | 3500.871093 |
| coverage_68 | 0.000000 |
| coverage_95 | 0.000466 |
| outlier_fraction | 0.000670 |

Tolerance failures:
- median relative error 149.209626 exceeds preregistered maximum 0.050000
- p95 relative error 3500.871093 exceeds preregistered maximum 0.100000
- coverage_68 0.000000 is outside preregistered range [0.630000, 0.730000]
- coverage_95 0.000466 is outside preregistered range [0.900000, 0.980000]

### Region `galactic-center`

| Metric | Value |
|---|---|
| sample_count | 3360 |
| signed_bias | 5.516142e10 |
| absolute_bias | 5.516142e10 |
| relative_bias | 383.803164 |
| mae | 5.516142e10 |
| median_absolute_error | 1.618391e10 |
| rmse | 2.262119e11 |
| relative_error_p50 | 112.724249 |
| relative_error_p68 | 224.734711 |
| relative_error_p95 | 1329.719931 |
| coverage_68 | 0.000000 |
| coverage_95 | 0.000000 |
| outlier_fraction | 0.000298 |

Tolerance failures:
- median relative error 112.724249 exceeds preregistered maximum 0.050000
- p95 relative error 1329.719931 exceeds preregistered maximum 0.100000
- coverage_68 0.000000 is outside preregistered range [0.630000, 0.730000]
- coverage_95 0.000000 is outside preregistered range [0.900000, 0.980000]

### Region `anticenter`

| Metric | Value |
|---|---|
| sample_count | 3360 |
| signed_bias | 4.693584e10 |
| absolute_bias | 4.693584e10 |
| relative_bias | 778.888305 |
| mae | 4.693584e10 |
| median_absolute_error | 1.327742e10 |
| rmse | 1.961001e11 |
| relative_error_p50 | 221.167379 |
| relative_error_p68 | 451.610595 |
| relative_error_p95 | 2422.949320 |
| coverage_68 | 0.000000 |
| coverage_95 | 0.000000 |
| outlier_fraction | 0.000595 |

Tolerance failures:
- median relative error 221.167379 exceeds preregistered maximum 0.050000
- p95 relative error 2422.949320 exceeds preregistered maximum 0.100000
- coverage_68 0.000000 is outside preregistered range [0.630000, 0.730000]
- coverage_95 0.000000 is outside preregistered range [0.900000, 0.980000]

### Region `poles`

| Metric | Value |
|---|---|
| sample_count | 26568 |
| signed_bias | 2.285847e10 |
| absolute_bias | 2.285847e10 |
| relative_bias | 1392.683221 |
| mae | 2.285847e10 |
| median_absolute_error | 4.998809e9 |
| rmse | 1.779359e11 |
| relative_error_p50 | 322.666614 |
| relative_error_p68 | 617.902145 |
| relative_error_p95 | 4655.396653 |
| coverage_68 | 0.000000 |
| coverage_95 | 0.000376 |
| outlier_fraction | 0.001016 |

Tolerance failures:
- median relative error 322.666614 exceeds preregistered maximum 0.050000
- p95 relative error 4655.396653 exceeds preregistered maximum 0.100000
- coverage_68 0.000000 is outside preregistered range [0.630000, 0.730000]
- coverage_95 0.000376 is outside preregistered range [0.900000, 0.980000]

### Region `dark-fields`

| Metric | Value |
|---|---|
| sample_count | 177 |
| signed_bias | 3.160245e10 |
| absolute_bias | 3.160245e10 |
| relative_bias | 1447.845495 |
| mae | 3.160245e10 |
| median_absolute_error | 8.654458e8 |
| rmse | 5.361263e10 |
| relative_error_p50 | 38.871682 |
| relative_error_p68 | 2912.703033 |
| relative_error_p95 | 5051.629767 |
| coverage_68 | 0.000000 |
| coverage_95 | 0.000000 |
| outlier_fraction | 0.005650 |

Tolerance failures:
- median relative error 38.871682 exceeds preregistered maximum 0.050000
- p95 relative error 5051.629767 exceeds preregistered maximum 0.100000
- coverage_68 0.000000 is outside preregistered range [0.630000, 0.730000]
- coverage_95 0.000000 is outside preregistered range [0.900000, 0.980000]

### Region `seam-0-360`

| Metric | Value |
|---|---|
| sample_count | 5542 |
| signed_bias | 3.423322e10 |
| absolute_bias | 3.423322e10 |
| relative_bias | 525.775599 |
| mae | 3.423322e10 |
| median_absolute_error | 4.493927e9 |
| rmse | 1.929706e11 |
| relative_error_p50 | 121.733751 |
| relative_error_p68 | 282.064028 |
| relative_error_p95 | 1505.260016 |
| coverage_68 | 0.000000 |
| coverage_95 | 0.000180 |
| outlier_fraction | 0.000722 |

Tolerance failures:
- median relative error 121.733751 exceeds preregistered maximum 0.050000
- p95 relative error 1505.260016 exceeds preregistered maximum 0.100000
- coverage_68 0.000000 is outside preregistered range [0.630000, 0.730000]
- coverage_95 0.000180 is outside preregistered range [0.900000, 0.980000]

### Region `dense`

| Metric | Value |
|---|---|
| sample_count | 19661 |
| signed_bias | 1.297172e11 |
| absolute_bias | 1.297172e11 |
| relative_bias | 3688.396887 |
| mae | 1.297172e11 |
| median_absolute_error | 7.227668e10 |
| rmse | 3.220044e11 |
| relative_error_p50 | 2566.322839 |
| relative_error_p68 | 3776.080922 |
| relative_error_p95 | 9431.978399 |
| coverage_68 | 0.000000 |
| coverage_95 | 0.001017 |
| outlier_fraction | 0.001322 |

Tolerance failures:
- median relative error 2566.322839 exceeds preregistered maximum 0.050000
- p95 relative error 9431.978399 exceeds preregistered maximum 0.100000
- coverage_68 0.000000 is outside preregistered range [0.630000, 0.730000]
- coverage_95 0.001017 is outside preregistered range [0.900000, 0.980000]

### Region `high-extinction`

| Metric | Value |
|---|---|
| sample_count | 960 |
| signed_bias | 5.116941e10 |
| absolute_bias | 5.116941e10 |
| relative_bias | 513.042034 |
| mae | 5.116941e10 |
| median_absolute_error | 1.177977e10 |
| rmse | 3.053072e11 |
| relative_error_p50 | 110.734590 |
| relative_error_p68 | 171.199131 |
| relative_error_p95 | 1199.174215 |
| coverage_68 | 0.000000 |
| coverage_95 | 0.000000 |
| outlier_fraction | 0.000000 |

Tolerance failures:
- median relative error 110.734590 exceeds preregistered maximum 0.050000
- p95 relative error 1199.174215 exceeds preregistered maximum 0.100000
- coverage_68 0.000000 is outside preregistered range [0.630000, 0.730000]
- coverage_95 0.000000 is outside preregistered range [0.900000, 0.980000]

### Region `bright-star`

| Metric | Value |
|---|---|
| sample_count | 43 |
| signed_bias | 6.939290e10 |
| absolute_bias | 6.939290e10 |
| relative_bias | 1407.748449 |
| mae | 6.939290e10 |
| median_absolute_error | 2.835002e10 |
| rmse | 1.016068e11 |
| relative_error_p50 | 486.115361 |
| relative_error_p68 | 2913.477540 |
| relative_error_p95 | 4996.083893 |
| coverage_68 | 0.000000 |
| coverage_95 | 0.000000 |
| outlier_fraction | 0.023256 |

Tolerance failures:
- median relative error 486.115361 exceeds preregistered maximum 0.050000
- p95 relative error 4996.083893 exceeds preregistered maximum 0.100000
- coverage_68 0.000000 is outside preregistered range [0.630000, 0.730000]
- coverage_95 0.000000 is outside preregistered range [0.900000, 0.980000]

### Region `high-crowding`

| Metric | Value |
|---|---|
| sample_count | 1967 |
| signed_bias | 3.128681e11 |
| absolute_bias | 3.128681e11 |
| relative_bias | 8822.687352 |
| mae | 3.128681e11 |
| median_absolute_error | 2.219137e11 |
| rmse | 4.663504e11 |
| relative_error_p50 | 7942.363472 |
| relative_error_p68 | 9388.727555 |
| relative_error_p95 | 16421.971650 |
| coverage_68 | 0.000000 |
| coverage_95 | 0.001525 |
| outlier_fraction | 0.007626 |

Tolerance failures:
- median relative error 7942.363472 exceeds preregistered maximum 0.050000
- p95 relative error 16421.971650 exceeds preregistered maximum 0.100000
- coverage_68 0.000000 is outside preregistered range [0.630000, 0.730000]
- coverage_95 0.001525 is outside preregistered range [0.900000, 0.980000]

## Notes

Technical scaffolding for issue #87. Scientific approval is recorded only in issue #47 and is never inferred from this report.
