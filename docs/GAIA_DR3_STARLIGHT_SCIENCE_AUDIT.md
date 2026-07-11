# Gaia DR3 starlight source-population audit

Status: Release-blocking scientific audit for the NSB Gaia DR3 starlight map.
Audit date: 2026-07-11.
Catalogue release: Gaia DR3.

## Conclusion

The NSB product is a magnitude-limited, resolved-source map derived from Gaia
DR3 externally calibrated XP sampled mean spectra. It is not a map of every
published Gaia XP continuous spectrum and must not be called a complete Gaia XP
map.

The previous local metadata file with 592,652 rows was incomplete. The same
selection evaluated through the official Gaia TAP service contains 33,985,787
rows when `duplicated_source=false`, while the complete published sampled
population contains 34,468,373 rows. A release run must reconcile its selected
population against the official count and the checksummed bulk-file inventory;
mere successful completion of a TAP CSV response is not evidence of
completeness.

## Reproducible population queries

The production source population intentionally retains `duplicated_source`.
Gaia documents this flag as a processing/quality indicator: one source
identifier was retained after cross-match duplication. Removing the retained
source does not prevent double counting; it removes its measured light.

```adql
SELECT COUNT(*) AS selected_sources,
       MIN(phot_g_mean_mag) AS min_g,
       MAX(phot_g_mean_mag) AS max_g
FROM gaiadr3.gaia_source
WHERE has_xp_sampled = 'True'
  AND phot_g_mean_mag IS NOT NULL
  AND phot_g_mean_mag <= 15.0
  AND source_id IS NOT NULL
  AND ra IS NOT NULL
  AND dec IS NOT NULL
  AND ref_epoch IS NOT NULL
```

Observed result on 2026-07-11:

```text
selected_sources = 34,468,373
min_g = 2.1975422
max_g = 15.0
```

Comparison queries:

```adql
SELECT COUNT(*) FROM gaiadr3.gaia_source
WHERE has_xp_continuous = 'True'
```

```adql
SELECT has_xp_sampled, has_xp_continuous, COUNT(*)
FROM gaiadr3.gaia_source
WHERE has_xp_sampled = 'True' OR has_xp_continuous = 'True'
GROUP BY has_xp_sampled, has_xp_continuous
```

| Population | Sources | Observed G range |
| --- | ---: | ---: |
| `has_xp_continuous=true` | 219,197,643 | 2.1975–21.4261 |
| `has_xp_sampled=true` | 34,468,373 | 2.1975–15.0000 |
| sampled and continuous | 34,468,373 | same sampled range |
| continuous without sampled | 184,729,270 | — |
| sampled with `duplicated_source=true` | 482,586 | — |

In Gaia DR3, the archive contents satisfy
`has_xp_sampled ⇔ has_xp_continuous AND phot_g_mean_mag <= 15`. The broad XP
continuous population was selected nominally near G < 17.65, subject to transit
and SSC criteria and documented special samples. These flags mean that the
corresponding data product is published; neither means that the source is a
single non-variable star or that all Gaia sources have XP spectra.

Official definitions:

- [Gaia DR3 `gaia_source` data model](https://gea.esac.esa.int/archive/documentation/GDR3/Gaia_archive/chap_datamodel/sec_dm_main_source_catalogue/ssec_dm_gaia_source.html)
- [XP source selection and spectral content](https://gea.esac.esa.int/archive/documentation/GDR3/Data_processing/chap_cu5pho/cu5pho_sec_specProcessing/cu5pho_ssec_specContent.html)
- [XP sampled mean-spectrum schema](https://gea.esac.esa.int/archive/documentation/GDR3/Gaia_archive/chap_datamodel/sec_dm_spectroscopic_tables/ssec_dm_xp_sampled_mean_spectrum.html)
- [XP continuous mean-spectrum schema](https://gea.esac.esa.int/archive/documentation/GDR3/Gaia_archive/chap_datamodel/sec_dm_spectroscopic_tables/ssec_dm_xp_continuous_mean_spectrum.html)

## Product choice and access strategy

`XP_SAMPLED` is the release product used for the base map. It contains the
externally calibrated combined BP+RP mean spectrum on a common 343-point grid,
336–1020 nm at 2 nm spacing, with `flux` and `flux_error` in W m⁻² nm⁻¹.
The derived NSB integral uses the exact inclusive 336–650 nm samples.

`XP_CONTINUOUS` contains BP/RP basis coefficients, coefficient errors and
correlations for 219,197,643 sources. Reconstructing it requires the official
GaiaXPy calibration basis. It is scientifically useful for estimating omitted
faint-source flux, but the 3.283 TiB compressed bulk is not the base product for
this less-than-24-hour pipeline. [GaiaXPy documents the continuous-spectrum
calibration](https://gaia-dpci.github.io/GaiaXPy-website/tutorials/Calibrator%20tutorial.html).

Official bulk inventories:

- [XP sampled bulk directory](https://cdn.gea.esac.esa.int/Gaia/gdr3/Spectroscopy/xp_sampled_mean_spectrum/)
- [XP sampled MD5 inventory](https://cdn.gea.esac.esa.int/Gaia/gdr3/Spectroscopy/xp_sampled_mean_spectrum/_MD5SUM.txt)
- [XP continuous bulk directory](https://cdn.gea.esac.esa.int/Gaia/gdr3/Spectroscopy/xp_continuous_mean_spectrum/)

The sampled inventory contains 3,386 compressed files totalling approximately
106.57 GiB. Completing that transfer in 24 hours requires about 1.3 MiB/s,
before parsing. The continuous inventory is approximately 3.283 TiB and would
require roughly 42 MB/s for transfer alone. The release pipeline therefore uses
checksummed sampled bulk files as its primary input and uses concurrent
DataLink only to repair missing sources and for controlled benchmarks.

Official bulk rows are ECSV with columns `source_id`, `solution_id`, `ra`,
`dec`, `flux`, and `flux_error`. `flux` and `flux_error` are quoted CSV fields
holding bracketed arrays of exactly 343 samples on the implicit XP sampled grid
336–1020 nm (step 2 nm). NSB integrates the inclusive 336–650 nm band only;
bulk files do not expose a per-row `wavelength` column.

Gaia DataLink supports multiple IDs (up to 5,000) with
`DATA_STRUCTURE=INDIVIDUAL`, returning an archive of individual products.
`DATA_STRUCTURE=COMBINED` is deprecated and is not used. See [Gaia programmatic
access](https://www.cosmos.esa.int/web/gaia-users/archive/programmatic-access)
and [Gaia Archive release notes](https://www.cosmos.esa.int/web/gaia-users/archive/release-notes).

## Completeness and missing flux

Broad-band Gaia flux aggregates are cross-checks, not substitutes for the
336–650 nm XP integral. They show that the omitted population is scientifically
material:

| Ratio | G-flux proxy | BP-flux proxy |
| --- | ---: | ---: |
| sampled / continuous XP | 88.63% | 89.75% |
| sampled with `duplicated_source=false` / continuous XP | 79.32% | 78.38% |
| continuous XP / all `gaia_source` | 82.88% | 83.53% |

Within continuous XP sources with `duplicated_source=false`, proxy-flux
convergence is:

| G limit | G proxy | BP proxy |
| ---: | ---: | ---: |
| 10 | 42.49% | 43.18% |
| 12 | 62.19% | 63.48% |
| 15 | 87.49% | 88.47% |
| 16 | 93.37% | 93.99% |
| 17 | 97.82% | 98.06% |
| 17.65 | 99.998% | 99.996% |

Production packing is blocked until a reviewed report measures
`F336–650(G_limit)` from the real sampled spectra and estimates the continuous-
only contribution from a reproducible sample stratified by magnitude, colour,
HEALPix region and quality. The report must give a global estimate, uncertainty
or confidence interval, and plane/pole regional estimates. Until then:

```text
estimated_missing_flux_contribution = unknown
release_completeness_gate = failed
```

## Source quality and interpretation

- XP spectra are mission-interval means. Variables remain valid contributors to
  a mean starlight map; `phot_variable_flag=NOT_AVAILABLE` does not mean
  constant.
- Unresolved binaries and blends contain real combined light. Resolved
  components also contain real light. Quality sensitivity is preferable to a
  blanket exclusion.
- 58.85% of sampled sources have at least one blended XP transit, yet they
  account for 36.64% of the G-flux proxy. A binary blended/not-blended cut would
  be scientifically destructive. Metrics from [`xp_summary`](https://gea.esac.esa.int/archive/documentation/GDR3/Gaia_archive/chap_datamodel/sec_dm_spectroscopic_tables/ssec_dm_xp_summary.html)
  must be used for sensitivity maps.
- Integrated QSO/galaxy candidate tables prioritise completeness and have
  nontrivial stellar contamination. Probabilistic non-stellar classifications
  must not be used as a silent hard cut. See the [official candidate-table
  warning](https://gea.esac.esa.int/archive/documentation/GDR3/Data_analysis/chap_cu3qso/sec_cu3qso_exploitation/).
- Small negative sampled fluxes are expected from noise/background subtraction.
  The real 1,000-source fixture has 92 negative samples out of 343,000 and all
  1,000 signed 336–650 nm integrals are positive. The pipeline retains their
  sign and reports their contribution.

## Required provenance fields

Every generated candidate and manifest records:

```text
source_population = Gaia DR3 sources with published XP_SAMPLED mean spectra
selection_predicate = has_xp_sampled='True', valid source_id/coordinates, G<=15
completeness_limitations = magnitude-limited subset of XP continuous; mission-
                           mean spectra; blending/confusion and classification
                           limitations remain
magnitude_limit = effective G<=15
xp_product_type = XP_SAMPLED, 343 samples, 336–1020 nm, 2 nm grid
number_of_selected_sources = 34468373
number_of_successfully_represented_sources = measured unique post-parse count
estimated_missing_flux_contribution = reviewed estimate and uncertainty, or
                                      explicit release blocker
```

The identity invariant is:

```text
selected_sources
  = successfully_represented_sources
  + explicitly documented scientific exclusions
```

Parse errors, missing files, duplicate IDs, partial files and unexpected
rejections are never scientific exclusions and always block production.
