# TSIS-1 HSRS v2 solar-spectrum validation

Status: reproducible reference-data replacement for issue #173. The asset
remains `generic-fallback`: its source, transformation, and numerical behavior
are reproducible, but dataset-specific redistribution terms and site/model-level
external validation remain unresolved.

## Official source selection

The authoritative input is the official LASP/LISIRD
`tsis1_hsrs_p025nm` product over 300–650 nm: TSIS-1 Hybrid Solar Reference
Spectrum Version 2, DOI
[`10.25980/ta3f-7h90`](https://doi.org/10.25980/ta3f-7h90). Its spectral
resolution is 0.025 nm and its sampling interval is 0.005 nm. The pinned
70,001-row LISIRD response has SHA-256
`1548b152d162cf877e890fb0aaf8ec7b8dcc102ad839dbd02b250ec17614de92`.
It is a scientific build input, not the runtime grid.

The issue's suggested 1 nm product was evaluated rather than adopted blindly.
The native source is sampled at 0.001 nm in this band. The p025nm product is the
least-resolved official fixed-resolution product that meets the 1% 445/551
shape gate.

| Official product | Sampling | Integrated irradiance (W m^-2) | 445/551 ratio | Ratio difference from native |
| --- | ---: | ---: | ---: | ---: |
| `tsis1_hsrs_1nm` | 0.1 nm | 554.337486078 | 1.060941921 | -17.254% |
| `tsis1_hsrs_p1nm` | 0.025 nm | 554.288317673 | 1.134580948 | -11.509% |
| `tsis1_hsrs_p025nm` | 0.005 nm | 554.284824817 | 1.270342709 | -0.922% |
| `tsis1_hsrs` native | 0.001 nm | 554.287953605 | 1.282159882 | reference |

The study-response hashes were `8186b62a…` (1 nm), `5a0bc95d…` (0.1 nm),
`1548b152…` (selected p025nm), and `3869a52d…` (native). The selected and
native hashes are enforced by the checked-in configuration.

For representative components, p025nm versus native changes extincted
Zodiacal integrated output by +0.2645%, B by -0.9405%, and V by +0.0268%.
Across three Jones geometries, integrated Moonlight changes by at most
0.00116%, B by -1.1867%, and V by -0.2675%. The machine-enforced
source-selection gates (`native_hsrs_resolution_comparison`, ignored until
sources are present) are:

| Comparison | Measured | Gate | Rationale |
| --- | ---: | ---: | --- |
| Zodiacal integrated | 0.2645% | 0.30% | below HSRS radiometric floor; margin beyond measured |
| Zodiacal B | 0.9405% | 1.00% | shape gate used to reject 1 nm / 0.1 nm products |
| Zodiacal V | 0.0268% | 0.050% | V is smoother; keep clearly below B-band allowance |
| Jones integrated | ≤0.00116% | 0.0020% | broadband moonlight is insensitive to residual lines |
| Jones B | 1.1867% | 1.25% | same shape-sensitive band as Zodiacal B |
| Jones V | 0.2675% | 0.30% | intermediate between integral and B |

These tests compare p025nm directly with native HSRS; the compact runtime
representation is not involved.

## Compact runtime-grid derivation

Publishing all 70,001 upstream samples would make every Zodiacal and Jones
evaluation iterate roughly 200 times more points than necessary. The build
therefore derives a separate 351-sample runtime grid at integer nanometres:

1. At each integer-nanometre node, integrate the piecewise-linear p025nm source
   over its one-nanometre Voronoi cell and store the cell mean. The two band
   edges use half-width cells. This makes NSB's trapezoidal 300–650 nm integral
   equal to the upstream integral.
2. Replace the 445, 500, and 551 nm nodes with the exact p025nm irradiances.
3. Apply equal compensating corrections to the adjacent nodes, preserving the
   integral without adding samples.

This is flux-aware reduction, not `step_by(N)` point selection. It protects the
exact B diagnostic, Zodiacal normalization, and V diagnostic while preserving
the broad-band energy. Fixed iteration order, `f64::total_cmp`, fixed numeric
formatting, and LF newlines make the transform deterministic. The final runtime
SHA-256 is
`71da8c3c5e2204dea0fde06329ef89bcec63a22980ed640bad54402cac5fee02`.

The runtime error budget is enforced separately from source selection:

| Gate | Bound | Rationale |
| --- | ---: | --- |
| Solar integral vs p025nm | 1e-12 relative | flux-conserving reduction + anchor compensation |
| Exact 445 / 500 / 551 nm | 1e-14 relative | diagnostic anchors |
| Exact B/V shape | 1e-12 relative | protected by 445/551 anchors |
| Zodiacal integrated | 0.05% | one sixth of HSRS's best quoted 0.3% radiometric uncertainty |
| Zodiacal B/V | exact (1e-12) | anchors |
| Jones integrated | 0.005% | measured ≤0.0018%; keeps clear margin |
| Jones B/V | exact (1e-12) | anchors |
| Runtime sample count | ≤351 | complexity guard; ≥100× reduction vs p025nm |

A 0.5 nm convergence candidate reduced the Zodiacal error only from 0.04480%
to 0.04082% while doubling Jones work, so the 1 nm grid is the justified
minimum-cost representation.

| Quantity | Compact runtime | Full p025nm | Relative difference |
| --- | ---: | ---: | ---: |
| Samples | 351 | 70,001 | -99.499% |
| Solar integral, 300–650 nm | 554.284824816965 | 554.284824816961 | +8.6e-13% |
| Irradiance at 445 nm | 2.347596278003 | 2.347596278003 | exact |
| Irradiance at 500 nm | 2.179562731749 | 2.179562731749 | exact |
| Irradiance at 551 nm | 1.848002323436 | 1.848002323436 | exact |
| 445/551 ratio | 1.270342709114 | 1.270342709114 | exact |
| Representative Zodiacal integral | 0.0569689932571 | 0.0569945253353 | -0.044797% |
| Representative Zodiacal B | 58.7087899263 | 58.7087899263 | exact |
| Representative Zodiacal V | 68.5652856369 | 68.5652856369 | exact |

For the three Jones geometries, compact-vs-p025nm integrated differences are
-0.001811%, -0.000546%, and -0.000923%; Moonlight B and V are exact because
the required wavelengths are explicit anchors. A release-mode local benchmark
of ten representative Jones evaluations measured 0.463 ms on the compact grid
and 71.358 ms on p025nm (154.1x faster). Timing is evidence rather than a
machine-independent gate; the deterministic guard is at most 351 runtime
samples and at least a 100x sample-count reduction in the benchmark test.

## License and reference distance

Dataset-specific redistribution terms remain **unresolved**. A fresh
primary-source pass checked, in order:

1. DOI / DataCite machine-readable metadata for
   [`10.25980/ta3f-7h90`](https://doi.org/10.25980/ta3f-7h90): the Handle
   resolves only to the LISIRD landing URL. DataCite and Crossref return no
   DOI record and therefore no `rightsList` / license field.
2. The [LISIRD HSRS landing page](https://lasp.colorado.edu/lisird/data/tsis1_hsrs/)
   and `tsis1_hsrs_p025nm` DAP JSON: public download metadata expose
   wavelength/irradiance/uncertainty/bandwidth only; no license, rights, or
   terms-of-use field.
3. LASP TSIS / CEOS WGCV documentation: product description and citation
   guidance, no HSRS redistribution grant.
4. NASA SMD [SPD-41a FAQ Q29](https://science.nasa.gov/researchers/science-information-policy_faq/):
   SMD scientific data *should* use CC0 **if there are no other restrictions**,
   while warning about IP, contracts, and underlying licenses. That is a
   conditional policy, not an HSRS-specific grant.
5. Coddington et al. (2021/2023) data-availability statements: they identify
   the LISIRD URL and constituent sources. Article CC BY 4.0 licenses the
   papers, not the dataset bytes.
6. LASP's internal
   [developer licensing guide](https://lasp-developer-guide.readthedocs.io/en/latest/licensing.html)
   notes that some TSIS-1 products use CC BY 4.0 as a *future packaging
   guideline*. It is not a rights statement attached to HSRS v2 itself.
7. Hybrid construction (TSIS-1 SIM/CSIM plus AFGL, QASUME, KPNO, and SPT
   line atlases) means constituent-source rights would also have to be clear
   before NSB could claim a standard SPDX identifier.

Public accessibility and citation instructions are recorded. NSB does **not**
claim `CC0-1.0` or `CC-BY-4.0` for the redistributed runtime asset. Maturity
therefore stays `generic-fallback`.

The reference distance is recorded as **1 AU**. This is **not** a direct
LISIRD HSRS metadata field: the DAP payload does not expose a reference-
distance attribute. The provenance chain is inherited through the radiometric
scale:

1. Coddington et al. state that the HSRS radiometric scale is the averaged
   TSIS-1 SIM spectrum.
2. LASP's authoritative
   [TSIS-1 SSI product documentation](https://lasp.colorado.edu/tsis/data/ssi-data/ssi-data-file-summary/)
   states that SIM SSI irradiances are reported at mean solar distance 1 AU
   and distinguishes them from irradiance at actual Earth–Sun distance.

That inherited-scale derivation is stronger than inferring 1 AU from the
numeric magnitude alone, and weaker than a direct HSRS endpoint annotation.

## Historical asset comparison

The removed historical file (`dbf6a620…`, 340 in-band samples) had no
recoverable source, release, or license and is not retained as a fallback or
selectable model. On its sample locations, the HSRS replacement has an 8.25%
median absolute pointwise difference, 13.10% mean, 43.86% 95th percentile, and
92.25% maximum; sparse sampling across solar lines makes broad outputs more
meaningful than the largest pointwise differences.

| Quantity | Historical | HSRS-derived runtime | Change |
| --- | ---: | ---: | ---: |
| Integrated irradiance, 300–650 nm | 545.162600 W m^-2 | 554.284825 W m^-2 | +1.6733% |
| Irradiance at 445 nm | 1.897000 W m^-2 nm^-1 | 2.347596 W m^-2 nm^-1 | +23.7531% |
| Irradiance at 500 nm | 1.913500 W m^-2 nm^-1 | 2.179563 W m^-2 nm^-1 | +13.9045% |
| Irradiance at 551 nm | 1.866500 W m^-2 nm^-1 | 1.848002 W m^-2 nm^-1 | -0.9910% |

For the Sgr A*/Paranal regression, integrated Zodiacal output changes from
0.0707489355 to 0.0627722029 ph cm^-2 ns^-1 sr^-1 (-11.2747%), B from
59.60198 to 65.02895 S10 (+9.1054%), and V from 87.22636 to 75.42801 S10
(-13.5261%). Jones integrated Moonlight changes by +2.1020%, +1.9400%, and
+2.0331%. These regression updates result from the data replacement and
runtime reduction; no scattering, reddening, or extinction physics changed.

## Reproduction and executable gates

Run the four commands recorded verbatim in `crates/nsb/data/manifest.toml`.
`update` verifies both official-response checksums, `build` performs the pure
reduction, `validate` separately checks p025nm-vs-native and
runtime-vs-p025nm, and `publish` accepts only unchanged validated bytes. Two
clean lifecycle runs must produce the runtime hash above.

The component comparisons are reproducible after `update` with:

```bash
NSB_TSIS_P025="$PWD/target/nsb-data/solar-spectrum/sources/tsis1_hsrs_p025nm_300_650.csv" \
NSB_TSIS_NATIVE="$PWD/target/nsb-data/solar-spectrum/sources/tsis1_hsrs_native_300_650.csv" \
  cargo test -p nsb native_hsrs_resolution_comparison -- --ignored --nocapture

NSB_TSIS_P025="$PWD/target/nsb-data/solar-spectrum/sources/tsis1_hsrs_p025nm_300_650.csv" \
  cargo test -p nsb compact_runtime_hsrs_comparison -- --ignored --nocapture

NSB_TSIS_P025="$PWD/target/nsb-data/solar-spectrum/sources/tsis1_hsrs_p025nm_300_650.csv" \
  cargo test --release -p nsb compact_runtime_jones_performance_evidence -- --ignored --nocapture
```

Network-independent tests cover malformed schema, checksum mismatch,
non-finite/non-positive/unordered/duplicate wavelength, negative/non-finite
irradiance, incomplete coverage, deterministic generation, required anchors,
runtime complexity, scientific error bounds, and publish-after-tamper
rejection.

## Remaining limitations

- No HSRS-specific redistribution grant was located after checking DOI/DataCite
  rights, LISIRD/DAP metadata, LASP/CEOS product docs, Coddington data-
  availability statements, SPD-41a, and the LASP developer licensing guide;
  article CC BY is not treated as a dataset license.
- Reference distance `1 AU` is inherited through the TSIS-1 SIM radiometric
  scale rather than exposed as a LISIRD HSRS field.
- This is a solar-minimum reference spectrum, not a time-varying solar model.
- Reproducibility and numerical fidelity do not constitute site-specific
  calibration of the downstream sky models.
