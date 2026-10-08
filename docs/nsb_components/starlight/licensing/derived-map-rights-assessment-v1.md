# Starlight derived-map rights: XHIP and SVO — preliminary assessment v1

**Date:** 2026-10-08  
**Release candidate:** `7e903ff289e76d07c018933b8f97fcf264cead73999912ff63f34b9d1e01b37d` (SHA-256)  
**Human release decision:** `pending` in issue [#103](https://github.com/VPRamon/NSB/issues/103).  
**Purpose:** assess the **specific derived numerical output**, not claim ownership or
new licensing rights over the upstream source files.

## 1. Facts established from NSB source and pinned provenance

The frozen 300–650 nm Galactic HEALPix `nside=128` candidate is assembled
from Gaia DR3 and a separate bright-star supplement. The original input
catalogues/response curves are retrieved for **offline** generation, not
installed as stand-alone upstream files in the intended Rust/Python package.

- **XHIP (CDS V/137D, Anderson & Francis 2012):**
  `crates/nsb-data-tools/src/starlight/bright_stars/catalogue.rs` ingests the
  table; `reconstruction.rs` selects temperature/luminosity codes to choose a
  CK04 spectral template for a Hipparcos star. The source is not a pixel-by-pixel
  output catalogue. The builder does *not* merely copy rows into the map.
- **SVO `Hipparcos/Hipparcos.Hp_bes` (Bessell 2000):**
  `scripts/build_starlight_bright_star_spectral_model.py` parses wavelength/
  throughput samples and constructs a locally pinned spectral calibration.
  `reconstruction.rs` uses that response and CALSPEC Vega to scale CK04
  templates from the Hp magnitude and integrates photon flux over 300–336 and
  336–650 nm. It does not serialize the SVO wavelength/throughput samples into
  the final per-pixel radiance field as a separate published table.
- **Final-map form:** the candidate stores per-pixel, band-integrated photon
  flux/radiance and uncertainty/diagnostic fields. The merge report and runtime
  sidecar *do* retain provenance strings (source IDs, URLs and SHA-256s), so
  those are not source-anonymous. No redistribution of the original XHIP
  `main.dat.gz` or the SVO XML is intended.

The merge report records **55 admitted supplement records**, zero replacement
records and ~1.52 billion admitted Gaia processing records. **55 is not a proven
count of distinct XHIP rows parsed or the volume of the source catalogue used
during offline ingestion**; do not use it alone to rule out database rights.

The public Git repository already includes the candidate and stage-runtime
artifacts; the crates.io/PyPI release scope is distinct. A source audit of the
aggregation pipeline is **not** a binary-by-binary proof that no protected
expression remains in every published file. Before new map distribution,
validate the complete package archive and its sidecars.

## 2. Different rights questions, not one generic license

| Act | Status/evidence | Interim consequence |
| --- | --- | --- |
| Download and use XHIP for scientific calculations | CDS/VizieR offers catalogue access with attribution/terms; no catalogue-specific CC redistribution grant verified | Preserve original source identity and source terms; check lawful use |
| Publish original XHIP table or a substantial protected extraction | No express right verified | Do not include the source table in the crate/wheel/release |
| Publish the derived HEALPix integrated map calculated using XHIP | Numerical integration is a new output, not a verbatim table copy | Potentially defensible but requires copyright/database-right and contractual review |
| Download and compute using SVO Hp response | SVO provides the filter response and scientific citation guidance; no specific broad redistribution grant established here | Preserve filter ID, Bessell 2000 and SVO references |
| Publish the original SVO XML or a derivative reproducing the full curve | Original curve redistribution rights not verified | Do not include the response XML or external spectral-model JSON unless separately reviewed |
| Publish only per-pixel flux resulting from SVO response integration | Derived scalar fields do not expose a wavelength/throughput table in the described schema | Lower apparent replication risk; still assess contract and underlying response rights |
| Publish Starlight map containing Gaia-derived values | ESA Gaia DR3 CC BY-NC 3.0 IGO; distribution may be subject to a noncommercial purpose restriction | Include ESA/DPAC notices, review downstream distribution scope |

**Important:** a public URL, publication citation, and a checksum are not
a license grant. The map cannot be assigned CC BY 4.0, AGPL or CC BY-NC on
behalf of any third-party contributor merely because its inputs were
scientifically transformed.

## 3. Legal framework to apply — EU/Spain (not a legal sign-off)

Directive **96/9/EC**, Articles **7(1)**, **7(5)** and **8(1)**, differentiates
re-utilization/extraction of a qualitatively or quantitatively **substantial**
part of a protected database from **insubstantial** portions used lawfully.
Repeated systematic extraction of insubstantial parts can also infringe.
A mere number of stars does not determine qualitative substantiality.

The CJEU decision **C-203/02 (British Horseracing Board v William Hill)**
requires examining substantiality in relation to the investment in obtaining,
verifying or presenting the part of the database reused. Access to a publicly
available catalogue does not automatically extinguish database rights.

Using a spectral class to select an independently licensed scientific model,
or using a filter response as a weighting function, does **not necessarily**
mean that the numerical integrated map republishes a protected part of the
original work. Conversely, computation does **not automatically erase**
database rights, contractual restrictions on access, or copyright in any
protected expression that survives in derived deliverables.

For Gaia-derived content that is copyright-protected and covered by the
ESA license, Creative Commons **BY-NC 3.0 IGO** grants specified
noncommercial uses, not general unrestricted downstream commercialization.
Free downloads/sponsorship do not replace a purpose-and-use review.

## 4. Source-specific rights verification update (2026-10-08)

This update uses source-specific records instead of inferring licenses from the
older ESA Hipparcos/Tycho catalogue terms:

| Upstream source | Primary source evidence | Current rights classification |
| --- | --- | --- |
| Tycho-2, CDS `I/259` | [Official CDS catalogue record](https://cdsarc.cds.unistra.fr/viz-bin/cat/I/259) explicitly declares CC BY-NC 3.0 IGO | **Verified source licence**, including the 20 pinned Tycho-2 segments; attribution and noncommercial restrictions apply where relevant |
| Hipparcos-2, CDS `I/311` | [Official van Leeuwen catalogue ReadMe](https://cdsarc.cds.unistra.fr/viz-bin/ReadMe/I/311?format=html) gives provenance and references but no explicit reuse/redistribution licence | **Not yet source-licensed for the exact reduction** |
| XHIP, CDS `V/137D` | [Official Anderson–Francis catalogue ReadMe](https://cdsarc.cds.unistra.fr/viz-bin/ReadMe/V/137D?format=html) identifies the authors and multiple upstream catalogues but no express redistribution licence | **No explicit source grant verified**; cite source/CDS, review derived-map rights or request written permission |
| SVO `Hipparcos/Hipparcos.Hp_bes` | [Official SVO VO service documentation](https://svo2.cab.inta-csic.es/theory/fps/index.php?mode=voservice) describes retrieving transmission curves and requests citation, without an explicit reuse licence | **No explicit source grant verified**; review rights to underlying Bessell 2000 curve separately |
| Gaia DR3 | [ESA Gaia data licensing](https://www.cosmos.esa.int/web/gaia-users/license) | CC BY-NC 3.0 IGO, subject to downstream scope and restrictions |

A source-specific license is not the same as authorization for the exact
aggregate map under all distribution channels. Conversely, not finding
a catalog-level license does not prove that mathematical integrated flux
requires permission: the issue #103 reviewer must document that determination.

**Unaltered frozen input identity:** the 20 Tycho-2 SHA-256 values in
`release-candidate-v1.toml` identify the same data shards. Their original
`license_or_terms_url` fields are historical provenance, not the exhaustive
license review; this document and `artifact-inventory-v1.toml` provide the
new source-specific citation without changing the signed science output.

## 5. Evidence needed before approving the precise map

1. **Pin actual output scope** separately for Git repository, GitHub Release,
   `.crate`, Python wheel/sdist, Zenodo, and optional remote downloads.
   Do not treat unapproved Git-tracked map bytes as 'not distributed'.
2. **Source-to-output audit:** independently compare a sample of original
   XHIP entries and SVO response samples with every file published; verify no
   source table/curve is reproducible from the published schema, including
   external spectral-model artifacts or developer bundles.
3. **XHIP substantiality assessment:** record size and distinct fields used
   during offline ingestion, count and nature of selections/transforms
   reaching the output, qualitative investment, and any applicable CDS/VizieR
   contract terms. Absence of copied raw rows is helpful but not dispositive.
4. **SVO source assessment:** document who controls the Bessell (2000) passband
   and applicable use terms, and distinguish curve redistribution from flux
   integration. Review whether intermediate spectral-model JSON remains
   outside every publication channel.
5. **ESA/other licences:** check Gaia BY-NC 3.0 IGO, the later Hipparcos-2
   and Tycho-2 compilations, and the specific CALSPEC/CK04 and Cantat-Gaudin
   versions. Provide accurate credits in every affected channel.
6. **Human review:** if any protected redistribution is required and no
   applicable grant can be established, obtain written, channel-specific
   permission from the relevant copyright/database-right holder(s).
   Record the decision, reviewer/date and checksums in #103.

## 6. Current conclusion and distribution boundary

**Preliminary technical conclusion:** the spectral reconstruction described
above uses XHIP classifications and SVO filter transmission as computational
inputs to *new* spatially integrated values, rather than publishing their raw
tabular bytes in the intended package. That favors an argument that permission
to redistribute whole XHIP/SVO sources is not necessary for a map **if** no
copyrighted expression, qualitatively substantial database extraction, or
contract-restricted content is being redistributed.

**Limit:** no evidence here proves an unconditional right to distribute the
frozen HEALPix map under all commercial and noncommercial channels. In
particular, the 55 published supplement *records* do not quantify all offline
use of XHIP, and the ESA Gaia noncommercial clause is a distinct review item.

**Release action now:** package NSB 0.1.0 **without** the frozen Starlight
candidate, report, sidecar and staged map; retain the exact scientific bytes
and their verification pins in the Git repository. Do **not** change
`redistribution-review-decision-v1.json` from `pending`, grant upstream
licenses, activate `runtime_embedded=true`, or tag the release before CI
and public API freeze pass. The already-Git-distributed bytes warrant their
own assessment and notices.

### Primary references

- [EU database Directive 96/9/EC](https://eur-lex.europa.eu/legal-content/EN/TXT/?uri=CELEX:31996L0009)
- [CJEU C-203/02, British Horseracing Board](https://eur-lex.europa.eu/legal-content/EN/TXT/?uri=CELEX:62002CJ0203)
- [ESA Gaia data licence](https://www.cosmos.esa.int/web/gaia-users/license)
- [Creative Commons BY-NC 3.0 IGO](https://creativecommons.org/licenses/by-nc/3.0/igo/)
- [XHIP catalogue, CDS V/137D](https://cdsarc.cds.unistra.fr/viz-bin/cat/V/137D)
- [CDS/VizieR licence information](https://cds.unistra.fr/vizier-org/licences_vizier.html)
- [SVO Filter Profile Service](https://svo2.cab.inta-csic.es/theory/fps/)
- [MAST Reference Atlases](https://archive.stsci.edu/hlsp/reference-atlases)
