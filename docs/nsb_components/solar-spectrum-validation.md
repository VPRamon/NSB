# TSIS-1 HSRS v2 solar-spectrum validation

Status: reproducible reference-data replacement for issue #173. The asset is
still classified `generic-fallback` because dataset-specific redistribution
terms and site/model-level external validation remain unresolved.

## Source and transformation

The runtime asset is derived from the official LASP/LISIRD
`tsis1_hsrs_p025nm` product for the 300–650 nm band. It is TSIS-1 Hybrid Solar
Reference Spectrum Version 2, DOI
[`10.25980/ta3f-7h90`](https://doi.org/10.25980/ta3f-7h90), with 0.025 nm
spectral resolution and 0.005 nm sampling. Coddington et al. (2023) describe
the spectrum as a solar-minimum reference formed from the 1–7 December 2019
TSIS-1 SIM average, extended by CSIM and normalized high-resolution solar-line
data. LISIRD declares wavelength in nm and irradiance in W m^-2 nm^-1.

The source query and its SHA-256 are pinned in
`crates/nsb-data-tools/config/solar-spectrum.toml`. The transformer accepts the
exact two-column LISIRD schema, rejects non-finite, negative, duplicate, or
unordered data, requires 300–650 nm coverage, and emits fixed decimal/scientific
formatting with LF newlines. The runtime bytes have SHA-256
`c790cf53e5af435ebf51a46134eb8a6620bced8872c1bdf74b8c712c7d579877`.

LISIRD and the paper's data-availability statement provide public access and a
citation DOI, but no dataset-specific redistribution license was located. The
asset metadata records that limitation rather than inferring a license. LISIRD
also labels the quantity as solar spectral irradiance but does not explicitly
state a 1 AU normalization in its exposed metadata; NSB therefore documents it
as Earth-orbit spectral irradiance without asserting an unsupported distance
normalization.

## Resolution decision

The issue's preferred 1 nm product was tested first. “1 nm” means 1 nm spectral
resolution with 0.1 nm sampling; the native product is sampled at 0.001 nm in
this band. The 1 nm product reproduced the native 300–650 nm integral closely,
but it did not preserve NSB's exact-wavelength B/V diagnostics, so it was
rejected. The intermediate 0.1 nm product was also insufficient. The selected
0.025 nm product is the least-resolved official variant meeting the 1% B/V
shape criterion.

| Product | Sampling | Integrated irradiance (W m^-2) | 445/551 ratio | Ratio difference from native |
| --- | ---: | ---: | ---: | ---: |
| `tsis1_hsrs_1nm` | 0.1 nm | 554.337486078 | 1.060941921 | -17.254% |
| `tsis1_hsrs_p1nm` | 0.025 nm | 554.288317673 | 1.134580948 | -11.509% |
| `tsis1_hsrs_p025nm` | 0.005 nm | 554.284824817 | 1.270342709 | -0.922% |
| `tsis1_hsrs` native | 0.001 nm | 554.287953605 | 1.282159882 | reference |

The corresponding band-query SHA-256 values were `8186b62a…` (1 nm),
`5a0bc95d…` (0.1 nm), `1548b152…` (selected 0.025 nm), and `3869a52d…`
(native). The selected and native hashes are enforced by the checked-in
configuration; the other two identify the rejected study inputs.

For a representative extincted Zodiacal geometry, p025nm versus native changed
integrated photon radiance by +0.2645%, B by -0.9405%, and V by +0.0268%.
Across the three Jones 2013 regression geometries, integrated Moonlight changed
by at most 0.00116%, B by -1.1867%, and V by -0.2675%. These bounds justify
p025nm while avoiding the native product's fivefold larger runtime grid.

The lifecycle validation machine-checks the irradiance integral and B/V shape.
The component-level native comparison is reproducible after `update` with:

```bash
NSB_TSIS_NATIVE="$PWD/target/nsb-data/solar-spectrum/sources/tsis1_hsrs_native_300_650.csv" \
  cargo test -p nsb native_hsrs_resolution_comparison -- --ignored --nocapture
```

## Historical asset comparison

The replaced file (`dbf6a620…`) had no recoverable source/release or license.
On the 340 historical sample locations in 300–650 nm, the new spectrum has an
8.25% median absolute pointwise difference, 13.10% mean absolute difference,
43.86% 95th-percentile difference, and 92.25% maximum difference. Large
pointwise changes are expected where a sparse historical grid intersects solar
lines; integrated and model outputs are more meaningful.

| Quantity | Historical | TSIS-1 p025nm | Change |
| --- | ---: | ---: | ---: |
| Integrated irradiance, 300–650 nm | 545.162600 W m^-2 | 554.284825 W m^-2 | +1.6733% |
| Irradiance at 445 nm | 1.897000 W m^-2 nm^-1 | 2.347596 W m^-2 nm^-1 | +23.7531% |
| Irradiance at 500 nm | 1.913500 W m^-2 nm^-1 | 2.179563 W m^-2 nm^-1 | +13.9045% |
| Irradiance at 551 nm | 1.866500 W m^-2 nm^-1 | 1.848002 W m^-2 nm^-1 | -0.9910% |

For the Sgr A*/Paranal Zodiacal regression, integrated photon radiance changed
from 0.0707489355 to 0.0628003832 ph cm^-2 ns^-1 sr^-1 (-11.2349%), B changed
from 59.60198 to 65.02895 S10 (+9.1054%), and V changed from 87.22636 to
75.42801 S10 (-13.5261%). The normalization at 500 nm makes these deltas depend
on spectral shape, not simply on the band-integrated irradiance.

Jones 2013 integrated Moonlight changed by +2.1039%, +1.9405%, and +2.0340%
for the three existing regression geometries. The refreshed exact regression
pins are therefore an intentional consequence of the source replacement, not
an unrelated scattering-physics change.

## Reproduction

Run the four commands recorded verbatim in `crates/nsb/data/manifest.toml`.
`update` verifies both official-response checksums, `build` performs the pure
transform, `validate` compares regenerated bytes and native resolution, and
`publish` accepts only the unchanged validated artifact. Network-independent
unit tests cover missing coverage, ordering, duplicates, NaN, negative values,
schema mismatch, checksum mismatch, and publish-after-tamper rejection.
