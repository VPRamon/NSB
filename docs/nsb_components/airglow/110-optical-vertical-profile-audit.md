# Optical 300-650 nm airglow vertical-profile audit (#110)

Status: Scientific source audit and implementation decision.
Reviewed: 2026-09-02.
Decision: Do not bundle or recommend a single production broadband profile;
retain the explicit 90 km Van Rhijn thin shell as the default and support
checksum-pinned caller profiles.

## Question and selection criteria

The geometry implementation needs a vertical volume-emission-rate (VER) profile,
but implementation availability is not evidence that one profile represents the
whole 300-650 nm band. A production candidate would need traceable altitude-
resolved optical VER, adequate spectral coverage, documented spatial/temporal
applicability and uncertainty, and terms permitting redistribution of the exact
derived asset. A convenient altitude grid or an infrared limb product alone is
not sufficient.

## Optical contributors and altitude structure

The band is a mixture of physically different sources rather than one layer:

| Contributor | Optical relevance | Representative altitude evidence | Consequence |
|---|---|---|---|
| O I green line, 557.7 nm | Strong discrete visible feature | Commonly integrated over roughly 90-100 km in ground-based imaging; ICON/MIGHTI night green products cover 90-109 km | Mesospheric/lower-thermospheric layer, not a broadband continuum proxy |
| O I red doublet, 630.0/636.4 nm | In band near the red boundary | Ground-based work commonly integrates 200-400 km; ICON red products span approximately 210-300 km | A distinct thermospheric layer with much stronger horizon geometry |
| Na D, about 589 nm | Visible doublet | Reviews place the sodium airglow layer near 90 km | Narrow species-specific contribution |
| O2 bands | Multiple optical bands | Reviews place important O2 nightglow layers roughly 91-95 km, depending on band/process | Spectral and process dependence matters |
| FeO/metal-oxide continuum | Visible continuum peaking near 595 nm | X-shooter analysis infers an effective peak around 85-89 km but reports missing/uncertain emitters | Relevant to the empirical continuum, with incomplete attribution |
| NO2 and other continua | Broad/structured continuum candidates | X-shooter climatology separates multiple components with different spectral and temporal behavior | No single fixed broadband shape/layer is established |

The species/altitude overview is grounded in Hecht's nightglow review
([doi:10.1029/2003RG000131](https://doi.org/10.1029/2003RG000131)), the optical
line atlas of Cosby et al.
([doi:10.1029/2006JA012023](https://doi.org/10.1029/2006JA012023)), the MANGO
green/red-line analysis
([doi:10.1029/2023JA031589](https://doi.org/10.1029/2023JA031589)), and the
official [ICON data catalogue](https://icon.ssl.berkeley.edu/Data). These ranges
are descriptive, not encoded as a production NSB profile.

Noll et al.'s ten-year X-shooter continuum analysis covers 300-1800 nm and finds
multiple components, different seasonal/local-time responses, and solar-cycle
dependence. Its visible FeO-like component is consistent with an approximately
85-89 km peak, but the authors also identify unexplained intensity and missing
emitters. See
[Noll et al. 2024, ACP 24, 1143-1166](https://doi.org/10.5194/acp-24-1143-2024).
The empirical continuum used by NSB ultimately follows the Paranal/SkyCalc
lineage documented by
[Noll et al. 2012](https://www.aanda.org/articles/aa/pdf/2012/07/aa19040-12.pdf).

## Variability and applicability

A production optical profile is not invariant. Published measurements show
dependencies on latitude and geomagnetic conditions, season, local solar time,
and solar activity. Different features do not necessarily vary together. The
six-year Paranal study documents distinct seasonal and solar-cycle responses
among airglow features
([Patat 2008](https://doi.org/10.1051/0004-6361:20079279)); the X-shooter
climatology further separates continuum components. Consequently, collapsing
all 300-650 nm emission into one height distribution would embed an unquantified
wavelength, time, latitude, and activity assumption.

The mixed 90 km and 200-400 km contributors also show why wavelength dependence
can be geometrically material near the horizon. Multiple profiles/processes are
scientifically preferable when a caller has suitable band-resolved evidence.
The current `VerticalEmissionProfile` records one wavelength applicability per
model; callers can construct evaluations appropriate to their chosen band, but
NSB does not invent a spectral decomposition of its broadband continuum.

## Candidate data and licence review

| Candidate | What it supplies | Licence/redistribution finding | Decision |
|---|---|---|---|
| ICON/MIGHTI Level 2 green/red VER | Public, altitude-resolved 557.7 and 630.0 nm products; SPASE metadata exposes relative VER products | NASA public-data policy applies unless a dataset states otherwise; product-specific acknowledgement and metadata still must be retained | Valuable line-specific validation source, not a global 300-650 nm broadband profile |
| UARS/WINDII Level 3 | Limb-derived winds and selected optical airglow emissions in an official NASA archive | Public catalogue access; derived-asset redistribution terms and transformations would need explicit documentation | Historical line-specific candidate, not adopted here |
| X-shooter Paranal continuum climatology | Ground-based spectra and inferred continuum components, including visible FeO-like emission | ACP article is CC BY 4.0; article access does not itself create a globally applicable altitude-resolved VER asset | Supports model separation and uncertainty, not a bundled profile |
| TIMED/SABER | Well-documented limb profiles including OH and O2 channels | Publicly accessible mission data with required acknowledgement | Channels are infrared (mission lists 1.27-17 micrometres); explicitly rejected as optical ground truth |

References for catalogue and use terms include the
[ICON/MIGHTI green VER SPASE record](https://spase-metadata.org/NASA/NumericalData/ICON/MIGHTI/L2/Vector/Green/PT30S),
[NASA WINDII catalogue](https://catalog.data.gov/dataset/uars-wind-imaging-interferometer-windii-level-3at-v011-uarwi3at-at-ges-disc),
[NASA heliophysics data-use policy](https://plasmasphere.nasa.gov/data_use_policy.html),
and the official [SABER instrument/data description](https://saber.gats-inc.com/).
No external scientific asset is redistributed by this change, so no third-party
dataset checksum or transformation recipe is asserted.

## Implementation decision

The evidence supports a generic vertical-profile capability but not a universal
production profile. NSB therefore:

1. keeps the 90 km Van Rhijn thin shell as its unchanged default;
2. accepts validated, caller-supplied `VerticalEmissionProfile` values;
3. requires persisted profiles to carry provenance, licence, applicability, and
   a deterministic matching checksum;
4. uses synthetic profiles only for geometry/integration validation; and
5. makes no claim that selecting the advanced geometry improves accuracy.

This is a scientific limitation, not missing machine-actionable geometry work.
Producing calibrated CTAO profiles requires measurements and belongs to #38.

## Deterministic cross-model validation

Run:

```bash
cargo run -p nsb --example airglow_geometry_comparison
```

The example emits reviewable CSV for observer altitudes 0, 2.5, and 5 km;
zenith angles 0, 30, 60, 75, 85, 89, and 90 degrees; and a 20 m thin shell,
a broad layer, and a two-layer profile. Selected sea-level results are:

| Zenith angle | Van Rhijn | 20 m profile | Relative difference | Broad profile | Two-layer profile |
|---:|---:|---:|---:|---:|---:|
| 0 | 1.000000000 | 1.000000000 | 0 | 1.000000000 | 1.000000000 |
| 30 | 1.149412868 | 1.149412868 | 2.4e-11 | 1.149206682 | 1.145944539 |
| 60 | 1.921836835 | 1.921836835 | 1.7e-11 | 1.919824256 | 1.888723882 |
| 85 | 5.341304193 | 5.341304196 | 4.8e-10 | 5.317777326 | 5.185412324 |
| 90 | 6.012170812 | 6.012170817 | 7.8e-10 | 5.976464404 | 5.822914154 |

The thin-profile tolerances are justified by its 20 m full width and 64 Simpson
substeps per altitude interval; refinement tests compare 16, 32, 64, and 128
substeps. At the geometric horizon the same thin shell produces factors
6.01217, 6.09686, and 6.18527 at observer altitudes 0, 2.5, and 5 km,
respectively. That altitude dependence is expected from the spherical ray
geometry and deliberately differs from the legacy observer-altitude-independent
Van Rhijn formula.

## Benchmark method

`cargo bench -p nsb --bench airglow_geometry` measures the unchanged Van Rhijn
factor, reference profile integration at 64 and 128 substeps, and complete
Airglow evaluations on both paths. It uses Criterion with fixed input profiles
and no network access. Caching was not added: correctness and an auditable
reference integration path take priority until measurements show a real need.

The 2026-09-02 short review run measured about 9 ns for Van Rhijn, 1.65 us for
the 64-substep profile factor, 3.16 us at 128 substeps, and 13.5-13.9 ms for a
complete evaluation with either geometry. Hardware and exact intervals are
recorded in the [performance contract](../../specifications/performance.md).
