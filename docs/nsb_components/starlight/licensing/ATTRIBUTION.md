# Starlight attribution

Status: current attribution record for the Starlight component's inputs and
generated artifacts.
Audience: redistribution reviewers, maintainers, and downstream integrators.
Scope: attribution wording only. Licence review, distribution classification,
and checksums live in [`artifact-inventory-v1.toml`](artifact-inventory-v1.toml).
The human redistribution decision is recorded only in issue #103 (see
[`README.md`](README.md)).

This document does not itself authorize redistribution. It records the
attribution text that must accompany any Starlight artifact, on any channel,
once a channel is authorized.

## Gaia DR3

> This work has made use of data from the European Space Agency (ESA)
> mission Gaia (<https://www.cosmos.esa.int/gaia>), processed by the Gaia
> Data Processing and Analysis Consortium (DPAC,
> <https://www.cosmos.esa.int/web/gaia/dpac/consortium>). Funding for the
> DPAC has been provided by national institutions, in particular the
> institutions participating in the Gaia Multilateral Agreement.

Gaia DR3 GaiaSource and XP continuous mean spectrum bulk products are
licensed under the Gaia data licence (CC BY-NC 3.0 IGO):
<https://www.cosmos.esa.int/web/gaia-users/license>. This licence's
non-commercial clause is the specific open question tracked by #103 for
any output derived from Gaia bulk data, including
`starlight_nside128.csv` and `merge_report.json`.

Primary references:

- Gaia DR3 documentation, release 1.3:
  <https://gea.esac.esa.int/archive/documentation/GDR3/>.
- Gaia DR3 XP processing and validation, De Angeli et al. (2023),
  DOI `10.1051/0004-6361/202243680`.
- Gaia DR3 XP external calibration, Montegriffo et al. (2023),
  DOI `10.1051/0004-6361/202243880`.

## Cantat-Gaudin Gaia DR3 selection function

> Cantat-Gaudin, T., et al. 2023, "Considerations on the Gaia DR3
> selection function of the astrophysical parameters catalogue",
> Astronomy & Astrophysics, DOI `10.1051/0004-6361/202244784`.

The original dataset at <https://zenodo.org/records/8063930> is published
under CC BY 4.0. Attribute the dataset creator, dataset DOI
10.5281/zenodo.8063930 and accompanying scientific paper when reused.
The UV-v2 candidate pins a BeeGFS-only selection-function artifact (see
`gaia-selection-function-cantat-gaudin` in the artifact inventory;
`distributed = false`).

## STScI CALSPEC

> This work uses spectrophotometric standard-star data from CALSPEC,
> maintained by the Space Telescope Science Institute (STScI):
> <https://www.stsci.edu/hst/instrumentation/reference-data-for-calibration-and-tools/astronomical-catalogs/calspec>.

The MAST REFERENCE-ATLASES HLSP explicitly licenses its data products
CC BY 4.0: <https://archive.stsci.edu/hlsp/reference-atlases>.
Acknowledge STScI/ReDCaT, DOI 10.17909/t9-khb7-4049, and the applicable
CALSPEC publication. The precise historical training snapshot is not pinned. CALSPEC spectra are used only as an offline training reference
for the 300-336 nm UV correction (#83) and are never hosted or redistributed
by NSB directly. Any redistributed UV-correction artifact (`calspec-linear-log-ratio-v1`
or `v2`) must carry this attribution because it is trained against CALSPEC
data.

## Bright-star supplement: Hipparcos, Tycho-2, XHIP, CK04 and SVO

The 34 checksum-pinned inputs for the current candidate are recorded in
`../release-candidate/release-candidate-v1.toml`. They are acquired offline;
none of the original catalogue, SVO response, or CK04 spectral-file bytes
belongs in crates.io/PyPI wheels. This does **not** itself prove permission to
redistribute the resulting map.

- **Hipparcos/Tycho (ESA)**: the original ESA Hipparcos and Tycho catalogues
  carry CC BY-NC 3.0 IGO and require "Credit: ESA":
  <https://www.cosmos.esa.int/web/hipparcos/catalogues>.
  The candidate uses a later Hipparcos-2 reduction (CDS I/311) and Tycho-2
  (CDS I/259); the original ESA notice alone does not establish the full
  licence status of those later compilations.
- **XHIP (Anderson and Francis 2012, CDS V/137D)**: cite the original catalogue
  and CDS/VizieR (DOI 10.26093/cds/vizier). Its catalogue-specific grant to
  redistribute XHIP bytes has **not** been verified. CDS's scientific-use
  access conditions must not be mislabeled as CC BY 4.0:
  <https://cdsarc.cds.unistra.fr/viz-bin/ReadMe/V/137D?format=html>.
- **Castelli–Kurucz 2004 atlas**: the MAST REFERENCE-ATLASES HLSP is CC BY
  4.0. Credit STScI/ReDCaT, Castelli and Kurucz, and the HLSP
  DOI 10.17909/t9-khb7-4049:
  <https://archive.stsci.edu/hlsp/reference-atlases>.
- **SVO Hipparcos Hp Bessell (2000) response**: acknowledge the SVO Filter
  Profile Service (Rodrigo et al. 2012, 2020), cite the original bandpass
  source and identify `Hipparcos/Hipparcos.Hp_bes`. Its explicit license for
  redistributing the entire curve has **not** been verified:
  <https://svo2.cab.inta-csic.es/theory/fps/>.

These qualifications are unresolved rights questions, not invitations to
remove the frozen scientific inputs, nor automatic authorization to change
`redistribution-review-decision-v1.json` from `pending`.

## GaiaXPy (historical reference only)

GaiaXPy is cited only as historical independent reference evidence for
continuous-XP reconstruction accuracy. It is not an operational NSB
dependency and no GaiaXPy code or data is redistributed:
<https://gaia-dpci.github.io/GaiaXPy-website/>.

## NSB-generated artifacts

`starlight_nside128.csv`, `merge_report.json`,
`crates/nsb/data/manifest.toml`, and the validation reports under
`docs/nsb_components/starlight/production-runs/` are generated by NSB's own
pipeline (`crates/nsb-data-tools`). NSB source itself is licensed under
AGPL-3.0-only (see the repository [`LICENSE`](../../../../LICENSE) and
[`README.md`](../../../../README.md#licensing)). Third-party dependencies retain
their own licence obligations. Gaia's CC BY-NC obligations apply where the underlying protected rights
reach the derived artifact. Numerical transformations do not automatically
relicense any upstream catalogue, nor do they automatically inherit all its
restrictions. A reviewer must evaluate the exact final map and its inputs.

## How to attribute a redistributed Starlight artifact

Any release channel that ships a Starlight artifact must include, verbatim
or by direct link:

1. the Gaia DR3 acknowledgement above;
2. the Cantat-Gaudin citation, if a selection-function artifact is included;
3. the CALSPEC/CK04 attribution when their data support the artifact;
4. the Hipparcos/Tycho and XHIP/SVO acknowledgements when used;
5. a link to this file and to `artifact-inventory-v1.toml` for the exact
   licence and checksum of the specific bytes being redistributed.

See [`THIRD_PARTY_NOTICES.md`](../../../../THIRD_PARTY_NOTICES.md) at the
repository root for the consolidated, project-wide third-party notice this
wording feeds into.
