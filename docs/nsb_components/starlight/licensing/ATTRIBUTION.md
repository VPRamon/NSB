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

- **Original Hipparcos/Tycho (ESA)**: the ESA 1997 catalogues carry
  CC BY-NC 3.0 IGO and require "Credit: ESA":
  <https://www.cosmos.esa.int/web/hipparcos/catalogues>.
- **Tycho-2 (Høg et al. 2000, CDS I/259)**: the **Tycho-2 catalogue itself**
  is explicitly listed by CDS/VizieR as **CC BY-NC 3.0 IGO**, covering
  the dataset represented by the 20 pinned `tyc2.dat.00..19` shards.
  Credit ESA, Høg et al. (2000), and CDS/VizieR as appropriate. Source-specific
  license evidence: <https://cdsarc.cds.unistra.fr/viz-bin/cat/I/259>.
- **Hipparcos-2 (van Leeuwen 2007, CDS I/311)**: this is an independent
  re-reduction of Hipparcos raw observations. No source-specific licence
  is recorded in the catalogue's public ReadMe:
  <https://cdsarc.cds.unistra.fr/viz-bin/ReadMe/I/311?format=html>.
  The ESA 1997 grant alone must not be treated as an express grant for
  every element of the 2007 reduction.
- **XHIP (Anderson and Francis 2012, CDS V/137D)**: cite the original catalogue
  and CDS/VizieR (DOI 10.26093/cds/vizier). Its catalogue-specific grant to
  redistribute XHIP bytes has **not** been verified. CDS's scientific-use
  access conditions must not be mislabeled as CC BY 4.0:
  <https://cdsarc.cds.unistra.fr/viz-bin/ReadMe/V/137D?format=html>.
- **Castelli–Kurucz 2004 atlas**: the MAST REFERENCE-ATLASES HLSP is CC BY
  4.0. Credit STScI/ReDCaT, Castelli and Kurucz, and the HLSP
  DOI 10.17909/t9-khb7-4049:
  <https://archive.stsci.edu/hlsp/reference-atlases>.
- **SVO Hipparcos Hp Bessell (2000) response**: a maintainer-provided
  reply from the SVO/FPS team (signed Enrique, reported 2026-10-08) explains
  that, from their perspective, NSB users need to be informed that the
  calculations used FPS and the Bessell (2000) curve, with the exact
  acknowledgement and references recorded below. This is **not** a licence
  for redistributing the original SVO response XML:
  <https://svo2.cab.inta-csic.es/theory/fps/>.

These qualifications are unresolved rights questions, not invitations to
remove the frozen scientific inputs, nor automatic authorization to change
`redistribution-review-decision-v1.json` from `pending`.

## SVO/FPS: exact acknowledgement and requested references

Maintainer-provided reply from the SVO/FPS team (signed Enrique; reported on 2026-10-08). The reply states (translated
scope, not a fabricated copyright licence): informing users of calculations
performed using the FPS and the Bessell (2000) response is sufficient **on
their part**, provided the requested credits are included. Keep the original
correspondence with the maintainer for authorized review rather than
publishing private email headers or contact details.

**Requested acknowledgement (verbatim):**

> This research has made use of the SVO Filter Profile Service "Carlos Rodrigo", funded by MCIN/AEI/10.13039/501100011033/ through grant PID2023-146210NB-I00

SVO/FPS references requested in the reply:

- Rodrigo, C., Cruz, P., Aguilar, J.F., et al. (2024):
  <https://ui.adsabs.harvard.edu/abs/2024A%26A...689A..93R/abstract>.
- Rodrigo, C., Solano, E., Bayo, A. (2012):
  <https://ui.adsabs.harvard.edu/abs/2012ivoa.rept.1015R/abstract>.
- Rodrigo, C., Solano, E. (2020):
  <https://ui.adsabs.harvard.edu/abs/2020sea..confE.182R/abstract>.
- Bessell (2000), Hipparcos Hp passband:
  <https://ui.adsabs.harvard.edu/abs/2000PASP..112..961B/abstract>.

The reference to Bessell (2000) is for the original filter passband. The SVO
reply concerns **use of the service in calculations and communication to
NSB users**; it does not explicitly grant redistribution of the original
filter response table or waive unrelated upstream restrictions. Record the
scope of any intended commercial/downstream uses in the #103 review.

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
