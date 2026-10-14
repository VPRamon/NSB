# Scientific references for Starlight

Status: bibliographic and acknowledgement record for the frozen Starlight
candidate, not a redistribution licence or a release approval.
Scope: 300–650 nm HEALPix sky-radiance map built from Gaia DR3 and the
Hipparcos/Tycho/XHIP bright-star supplement. Original catalogue and filter
files are offline scientific inputs; the map is a numerical derived product.

The authoritative source-by-source rights classifications, file checksums, and
distribution channels are in [artifact-inventory-v1.toml](artifact-inventory-v1.toml).
Required acknowledgement wording is in [ATTRIBUTION.md](ATTRIBUTION.md).
The human decision is tracked by [#103](https://github.com/VPRamon/NSB/issues/103)
and remains `pending`. A citation is **not** a grant of copyright,
database rights, commercial use, or downstream redistribution rights.

## Gaia DR3 and the XP spectra

- ESA Gaia mission and Gaia Data Processing and Analysis Consortium (DPAC).
  [Gaia DR3 documentation](https://gea.esac.esa.int/archive/documentation/GDR3/)
  and [Gaia data licence](https://www.cosmos.esa.int/web/gaia-users/license)
  (CC BY-NC 3.0 IGO); use the official Gaia/DPAC acknowledgement in
  [ATTRIBUTION.md](ATTRIBUTION.md#gaia-dr3).
- De Angeli et al. (2023), Gaia DR3 XP spectra: processing and validation,
  *Astronomy & Astrophysics*, DOI
  [10.1051/0004-6361/202243680](https://doi.org/10.1051/0004-6361/202243680).
- Montegriffo et al. (2023), Gaia DR3 XP external calibration,
  *Astronomy & Astrophysics*, DOI
  [10.1051/0004-6361/202243880](https://doi.org/10.1051/0004-6361/202243880).

## Bright-star catalogue sources

### XHIP: Extended Hipparcos Compilation

- **Anderson, E. & Francis, C. (2012)**, *XHIP: An Extended Hipparcos
  Compilation*, *Astronomy Letters* **38**, 331.
  ADS bibcode [2012AstL...38..331A](https://ui.adsabs.harvard.edu/abs/2012AstL...38..331A/abstract);
  [preprint arXiv:1108.4971](https://arxiv.org/abs/1108.4971).
- Catalogue: [CDS/VizieR V/137D](https://cdsarc.cds.unistra.fr/viz-bin/cat/V/137D);
  [catalogue ReadMe](https://cdsarc.cds.unistra.fr/viz-bin/ReadMe/V/137D?format=html).
- NSB uses XHIP spectral-type and luminosity/temperature classifications to
  select CK04 templates in offline reconstruction, rather than distributing
  the XHIP source table in the planned Rust/Python packages.
- **Rights status:** no source-specific permission for redistribution of the
  protected XHIP database content or the final derived map has been verified.
  Clarification was requested from CDS/VizieR on **2026-10-08**. That request
  expressly covers worldwide downstream commercial and non-commercial reuse;
  submission does not constitute authorization.

### Hipparcos-2: the 2007 new reduction

- **van Leeuwen, F. (2007)**, *Validation of the new Hipparcos reduction*,
  *Astronomy & Astrophysics* **474**, 653–664, DOI
  [10.1051/0004-6361:20078357](https://doi.org/10.1051/0004-6361:20078357);
  ADS bibcode
  [2007A&A...474..653V](https://ui.adsabs.harvard.edu/abs/2007A%26A...474..653V/abstract).
- **van Leeuwen, F. (2007)**, *Hipparcos, the New Reduction of the Raw Data*,
  *Astrophysics and Space Science Library* **350**,
  [Springer DOI 10.1007/978-1-4020-6342-8](https://doi.org/10.1007/978-1-4020-6342-8).
- Catalogue: [CDS/VizieR I/311](https://cdsarc.cds.unistra.fr/viz-bin/cat/I/311);
  [catalogue ReadMe](https://cdsarc.cds.unistra.fr/viz-bin/ReadMe/I/311?format=html).
- **Rights status (author clarification received):** Floor van Leeuwen
  confirmed in correspondence supplied by the maintainer (reported
  **2026-10-14**) that his 2007 reduction builds on the ESA Hipparcos data
  and is **publicly available**. He specifically asks users to cite **both**
  2007 publications immediately above. The request was sent on
  **2026-10-08**; the original reply should be kept privately as evidence.
  This confirms source availability and scholarly attribution, **not**
  a source-specific licence identifier or an explicit grant for
  worldwide commercial/downstream redistribution of derived maps. The
  original ESA Hipparcos licence must not be silently assigned to I/311.

### Tycho-2

- **Høg, E. et al. (2000)**, *The Tycho-2 catalogue of the 2.5 million
  brightest stars*, *Astronomy & Astrophysics* **355**, L27–L30;
  ADS bibcode
  [2000A&A...355L..27H](https://ui.adsabs.harvard.edu/abs/2000A%26A...355L..27H/abstract).
- Catalogue: [CDS/VizieR I/259](https://cdsarc.cds.unistra.fr/viz-bin/cat/I/259),
  with its catalogue-specific **CC BY-NC 3.0 IGO** designation. Credit Høg
  et al. and ESA/CDS as appropriate. The original shards are offline inputs,
  not Rust/Python package payloads.

### ESA original Hipparcos/Tycho catalogues

- [ESA Hipparcos catalogues](https://www.cosmos.esa.int/web/hipparcos/catalogues)
  and the original ESA **CC BY-NC 3.0 IGO** terms. These should not be
  substituted for Hipparcos-2's unverified, later re-reduction terms.

## Spectral reference and photometric calibration

- **Castelli, F. & Kurucz, R. L. (2004)**, CK04 stellar atmosphere grids.
  Scientific model/data reference:
  [MAST REFERENCE-ATLASES HLSP](https://archive.stsci.edu/hlsp/reference-atlases),
  DOI [10.17909/t9-khb7-4049](https://doi.org/10.17909/t9-khb7-4049)
  (HLSP data marked CC BY 4.0).
- **CALSPEC**, spectrophotometric standards maintained by the Space Telescope
  Science Institute (STScI): [STScI CALSPEC](https://www.stsci.edu/hst/instrumentation/reference-data-for-calibration-and-tools/astronomical-catalogs/calspec);
  [MAST REFERENCE-ATLASES HLSP](https://archive.stsci.edu/hlsp/reference-atlases),
  DOI [10.17909/t9-khb7-4049](https://doi.org/10.17909/t9-khb7-4049).
  The precise historical offline-training snapshot is not pinned.
- **SVO Filter Profile Service**, filter identifier
  `Hipparcos/Hipparcos.Hp_bes`, Bessell (2000). The SVO team requested the
  exact service acknowledgement and the **four** references reproduced in
  [ATTRIBUTION.md](ATTRIBUTION.md#svofps-exact-acknowledgement-and-requested-references).
  The SVO response concerns calculations with the filter; it is **not**
  permission to redistribute the original filter-transmission table.
- **Cantat-Gaudin et al. (2023)**, Gaia DR3 selection-function work,
  *Astronomy & Astrophysics*, DOI
  [10.1051/0004-6361/202244784](https://doi.org/10.1051/0004-6361/202244784);
  associated [Zenodo dataset](https://zenodo.org/records/8063930),
  DOI [10.5281/zenodo.8063930](https://doi.org/10.5281/zenodo.8063930)
  (CC BY 4.0). The pinned selection-function artifact is offline-only.

## CDS/VizieR acknowledgement

CDS recommends acknowledgement when its VizieR catalogue service is used:

> This research has made use of the VizieR catalogue access tool, CDS,
> Strasbourg, France (DOI: 10.26093/cds/vizier).

See the official [CDS V/137D catalogue page](https://cdsarc.cds.unistra.fr/viz-bin/cat/V/137D).
Credit the original catalogue creators **as well as** CDS/VizieR; service
acknowledgement does not substitute for either the original research citations
or a source-specific rights grant.

## Publication and licensing boundary

NSB code is **AGPL-3.0-only**. The Gaia and Tycho-2 catalogue grants include
non-commercial restrictions that may apply to protected data incorporated in
derivatives; other source-specific rights remain unresolved. Publishing the
source code freely and distributing a numerical map without raw catalogue rows
do not, on their own, resolve EU database rights or downstream commercial use.

The Starlight candidate and staged runtime files are already tracked in Git;
Git-hosted distribution is therefore a distinct channel from crates.io/PyPI.
The planned package release must retain human review via #103 and its
[redistribution decision](../release-candidate/redistribution-review-decision-v1.json).
Do not infer automatic approval from bibliographic completeness.
