# Starlight derived-map rights: XHIP and SVO — preliminary assessment v1

**Date:** 2026-10-14<br>
**Release candidate:** `7e903ff289e76d07c018933b8f97fcf264cead73999912ff63f34b9d1e01b37d` (SHA-256)  
**Project publication authorization:** explicitly given by the maintainer; see §13.<br>
**Product-rights decision:** `approved_with_conditions`; explicit maintainer acceptance of §§9–10 recorded in §13.<br>
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
| Hipparcos-2, CDS `I/311` | [Official van Leeuwen catalogue ReadMe](https://cdsarc.cds.unistra.fr/viz-bin/ReadMe/I/311?format=html) plus author reply reported by the maintainer on 2026-10-14: the 2007 reduction uses ESA Hipparcos data and is "publicly available"; author requests book + A&A 2007 citations | **Public availability and attribution confirmed directly by the author**; no explicit I/311 licence or general commercial/downstream derived-map grant established |
| XHIP, CDS `V/137D` | [Official Anderson–Francis catalogue ReadMe](https://cdsarc.cds.unistra.fr/viz-bin/ReadMe/V/137D?format=html) identifies the authors and multiple upstream catalogues but no express redistribution licence | **No explicit source grant verified**; cite source/CDS, review derived-map rights or request written permission |
| SVO `Hipparcos/Hipparcos.Hp_bes` | Maintainer-provided SVO/FPS team email (signed Enrique, reported 2026-10-08) states that informing NSB users of the SVO FPS and Bessell (2000) usage and including specific credits/references is sufficient **on SVO's part** | **Acknowledgement conditions for computed results documented**; no express grant for republishing the original curve. Preserve original reply privately as review evidence. |
| Gaia DR3 | [ESA Gaia data licensing](https://www.cosmos.esa.int/web/gaia-users/license) | CC BY-NC 3.0 IGO, subject to downstream scope and restrictions |

**Hipparcos-2 correspondence update (reported 2026-10-14).**
The maintainer's 2026-10-08 request asked whether derived HEALPix maps could
be publicly redistributed and whether commercial or noncommercial downstream
use was restricted. Floor van Leeuwen's reply points to the ESA origins,
confirms that the 2007 reduction is publicly available, and requests two
specific scientific citations. This is positive first-party evidence for
availability and attribution. It does not explicitly answer the commercial
and downstream-redistribution questions or establish that the original ESA
licence covers all rights in the 2007 reduction. Retain the full email in
private project records; avoid publishing signatures or contact headers.
Do not label Hipparcos-2 as wholly uncontacted, nor as an unrestricted
redistribution permission.

The exact acknowledgement and FPS/Bessell citations requested by SVO are
reproduced in [ATTRIBUTION.md](ATTRIBUTION.md#svofps-exact-acknowledgement-and-requested-references)
and both project and published-crate notices. This is **direct correspondence
provided by the maintainer**, not independently verified license terms; the
original email should be retained for the authorized #103 review. SVO did not
explicitly waive rights to the source XML/throughput table. Clarify the
commercial-use/channel scope if an unequivocal waiver or written permission
for that scope is required.

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

**Release action now:** the maintainer has explicitly accepted the two bounded
findings in §§9–10 for this noncommercial publication (§13). Record that actual
conditional decision, promote the exact compact pair, and verify the production
packages. This is a project interpretation, not a new third-party licence or a
qualified legal opinion. Original catalogues/curves remain outside package scope.

### Primary references

- [EU database Directive 96/9/EC](https://eur-lex.europa.eu/legal-content/EN/TXT/?uri=CELEX:31996L0009)
- [CJEU C-203/02, British Horseracing Board](https://eur-lex.europa.eu/legal-content/EN/TXT/?uri=CELEX:62002CJ0203)
- [ESA Gaia data licence](https://www.cosmos.esa.int/web/gaia-users/license)
- [Creative Commons BY-NC 3.0 IGO](https://creativecommons.org/licenses/by-nc/3.0/igo/)
- [XHIP catalogue, CDS V/137D](https://cdsarc.cds.unistra.fr/viz-bin/cat/V/137D)
- [CDS/VizieR licence information](https://cds.unistra.fr/vizier-org/licences_vizier.html)
- [SVO Filter Profile Service](https://svo2.cab.inta-csic.es/theory/fps/)
- [MAST Reference Atlases](https://archive.stsci.edu/hlsp/reference-atlases)

## 7. Compact release recommendation (2026-10-14)

The compact representation preserves the same radiances, uncertainties and
provenance. It creates no additional redistribution permission. The maintainer
has accepted the calculated-map and independent-data classifications (§13), so
conditional noncommercial distribution is authorized for the exact compact pair
on the pinned GitHub source/release, crates.io and PyPI channels. Retain all source
terms and notices; do not offer general commercial data reuse under AGPL.

| Channel | Accepted scope | Remaining operational action |
| --- | --- | --- |
| Git repository | Existing frozen evidence plus exact approved compact pair | Push production artifacts, then record actual Git publication |
| crates.io `.crate` | Compact pair and notices under source-specific terms | Verify exact production package; separate final publication authorization |
| PyPI wheels and sdist | Same approved product and notices | Test installed wheel and sdist-built wheel offline |
| GitHub Release | Exact compact product under the same conditions | Separate final tag/release authorization |
| Zenodo | Not a planned compact channel in this decision | Obtain a channel-specific project decision before a new upload |
| Downstream commercial data use | No unrestricted grant established | Assess applicable source rights/permission for that use |

Primary-source checks during this change: ESA's [Gaia licence page](https://www.cosmos.esa.int/web/gaia-users/license)
continues to designate BY-NC 3.0 IGO and refers commercial use to ESA archive
terms; [CC legal code](https://creativecommons.org/licenses/by-nc/3.0/igo/legalcode)
does not supply unrestricted commercial reuse. [MAST REFERENCE-ATLASES](https://archive.stsci.edu/hlsp/reference-atlases)
explicitly lists CK04 and CALSPEC products and CC BY 4.0, requesting source
papers and DOI 10.17909/t9-khb7-4049. The [XHIP ReadMe](https://cdsarc.cds.unistra.fr/viz-bin/ReadMe/V/137D?format=html)
identifies the compilation and scientific fields but did not establish a
specific redistribution grant. This is evidence of an unresolved grant, not
proof that mathematical outputs infringe.

### Source-to-package assessment

The source-specific versions, URLs, original author names and hashes remain in
`artifact-inventory-v1.toml`, `SCIENTIFIC_REFERENCES.md`, and the asset registry;
the following table states the actual intended redistribution scope.

| Source/product | Original bytes in runtime packages? | Derived bytes? | Terms, citations and remaining question |
| --- | --- | --- | --- |
| Gaia DR3 GaiaSource/XP, ESA/DPAC | No | Frozen integrated map, only after approval | BY-NC 3.0 IGO; ESA/DPAC acknowledgement and Gaia mission/DR3 references; compliant NC grant established, actual purpose and combined-work classification require review |
| Tycho-2 I/259, Høg et al. 2000 | No | Diagnostics only in this frozen pipeline | CDS source designation BY-NC 3.0 IGO; Høg citations retained; no Tycho photometry alters final radiances/uncertainties |
| Hipparcos-2 I/311, van Leeuwen 2007 | No | Bright-star contribution | Author-confirmed availability; cite both 2007 book and validation paper; commercial/derived-map grant not explicit |
| XHIP V/137D, Anderson & Francis 2012 | No | Spectral-class selections used to calculate flux | Cite Anderson & Francis and CDS; catalogue-specific grant and substantiality assessment remain unresolved |
| SVO Hp_bes / Bessell 2000 | No curve/XML | Calculated scalar integrals | Maintainer-reported FPS correspondence supports calculation results with exact acknowledgement and four requested references; original-curve redistribution not authorized here |
| CK04 HLSP v2, Castelli & Kurucz; CALSPEC alpha_lyr_stis_012, STScI | No spectra | Integrated model flux and calibration | MAST CC BY 4.0, retain attribution, licence link, source papers, DOI and indication of transformation; exact input hashes in sidecar |
| Cantat-Gaudin Gaia DR3 M10 selection product | No original table | Selection-weighted map | CC BY 4.0; existing dataset DOI/citation and change description retained |
| PALACE v1.0, Noll et al. 2024 | No original FITS or GPL program | Embedded continuum/climatology table | Zenodo 14064023 data CC BY 4.0; retain author credit, DOI, licence and transformation notice; model remains a climatology |
| NOAA/NWS SWPC F10.7 snapshot | Selected published product values | Normalized offline store | Federal-work public-domain basis applies to those products; retain NOAA/DRAO lineage, no blanket waiver for unrelated contributed works |
| Solar reference v1 | No third-party spectrum | NSB Planck calculation | NSB AGPL terms, physical constants/provenance in registry; do not call it an observed spectrum |
| Moonlight Mie and multiple-scatter LUT v1 | No historical ESO tables | NSB numerical calculations | NSB terms and Jones 2013/model citations; reconstruction/approximation maturity retained |
| Atmospheric/zodiacal reference equations | No copied catalogue | Evaluated published models | Existing Noll/Jones/Leinert references and Siderust dependency notices; no new upstream dataset introduced |

### nsb2 precedent

Reviewed [stars.py](https://github.com/GerritRo/nsb2/blob/main/nsb2/emitter/stars.py),
its packaged `anderson2012_xhip_suppl.dat`, BSD-3-Clause software licence, and
[Zenodo Gaia product 15396676](https://zenodo.org/records/15396676).
nsb2 separates bright-source and faint HEALPix paths at G=15, uses photometric
spectral templates and XHIP bright-star supplementation, and requests Gaia
acknowledgements. Its Gaia products and SVO responses use first-use network
retrieval. This supplies scientific precedent, not a grant from Gaia/XHIP
rights holders. NSB copies none of its code/data and retains its offline
embedded XP-derived map architecture. Neither BSD software terms nor another
project's Zenodo metadata settle NSB's exact upstream and derived-product rights.

The bright-star CALSPEC zero-point file is checksum-pinned, while the older
UV-training snapshot is not fully pinned (see the bibliography/inventory).
This limits full upstream regeneration; the frozen output, calibration model
and representation conversion remain checksum-locked. It must remain visible
in release review rather than being described as complete upstream provenance.

## 8. Actual noncommercial publication scope and public grants

**Research update: 2026-10-14.** The maintainer states that NSB is a scientific
research project, freely distributed without planned monetization. The intended
channels are public GitHub source/releases, crates.io, and PyPI wheels/sdists.
This is not a request for an unrestricted commercial licence.

[CC BY-NC 3.0 IGO legal code](https://creativecommons.org/licenses/by-nc/3.0/igo/legalcode.en)
§3(a–b) already grants reproduction, distribution, collections and adaptations;
§3 also permits technically necessary format changes. §4(b) limits exercise of
those rights by purpose (commercial advantage/private monetary compensation).
§§4(a,c) require the licence link, preserved notices, attribution and identification
of changes; §8(a–b) preserves the original licensor's direct offer downstream.
§2 leaves copyright-free uses unaffected. **Additional individual permission is
not required for acts already within this public grant.** The licence does not
turn uncopyrightable numerical facts into copyrighted works.

ESA's [Gaia licence](https://www.cosmos.esa.int/web/gaia-users/license) expressly
adopts this licence. Its [archive terms](https://www.cosmos.esa.int/web/esdc/terms-and-conditions)
require attribution and route commercial applications, including indirect
financial gain, to `data.licences@esa.int`; they specify French law and an ICC
arbitration arrangement. They do not say that every noncommercial adaptation
needs an individual approval. The designated credit is ESA/Gaia DPAC.

**Conclusion A (public licence), with a stated inference:** the described free
scientific publication has a defensible noncommercial purpose and may rely on
the Gaia grant for reproduction/adaptation/distribution, subject to its actual
facts and conditions. A commercial hosting provider's existence does not by
itself classify the maintainer's purpose, nor does zero price prove that purpose.
The maintainer has explicitly confirmed the noncommercial publication facts
and authorized proceeding under the source conditions (§13). Those purpose
facts need no further confirmation. Preserve NC
terms where the underlying protected rights apply; do not promise commercial
reuse. A future downstream commercial application must assess its own rights.
No ESA reply is a prerequisite to an otherwise compliant noncommercial release.

Tycho-2 [I/259](https://cdsarc.cds.unistra.fr/viz-bin/cat/I/259) has the same
source licence designation, independently of Tycho-1. More specifically, this
NSB pipeline requires finite Hipparcos Hp photometry for **every** input record.
`admits_hp_or_vt(Some(hp), vt)` uses Hp and never takes its missing-Hp Tycho
fallback. Tycho BT/VT, errors and IDs are retained in external diagnostics;
they do not enter `reconstruct_spectral_estimates`, the astrometric matching,
or the final radiance/uncertainty fields. **Conclusion B:** no Tycho photometric
records are reproduced in the current runtime map, and no extra Tycho commercial
permission is justified merely by its presence in the offline input inventory.
Retain Høg/ESA/CDS credits for its actual use; do not generalize this finding to
a future model using Tycho photometry. Gaia independently supplies the NC boundary.

## 9. XHIP source-to-output audit

The audit downloaded the public upstream files into temporary local research
storage, verified their **exact frozen** compressed SHA-256s, and counted fixed-width
records. No raw file or record subset is added to the repository/package.

| Evidence | Verified result |
| --- | --- |
| XHIP `V/137D/main.dat.gz` | 11,163,196 bytes; SHA-256 `8b6a7efb5b3ebf985de2072ace9f2d47849f369c345e2de9a1e3fe822594771b` |
| XHIP rows read by `ingest_xhip` | **117,955**, with 117,955 unique HIP IDs; all rows are parsed into the in-memory map |
| Hipparcos-2 `I/311/hip2.dat.gz` | 8,977,907 bytes; SHA-256 `8e624f843d4254a9b7c2e8dda8e3158dbe825bf98f6bf3dbbae7f0d0b73d6858`; 117,955 rows |
| HIP identity cross-check | The two complete ID sets are identical |
| Nonmissing XHIP Tc/Lc pairs | **73,840** rows; available for the reconstruction model's assignment lookup |
| Nonmissing XHIP radial velocities | **46,392** rows |
| Hp ≤ 4 population | **480** Hipparcos records, all with an XHIP match; **477** have Tc/Lc; **479** have positive parallax and a nonmissing RV |
| Final merged supplement | **55 distinct admitted supplement records**, zero primary replacements, from the frozen merge accounting; classification selection and modelling affect at most 55 spatial pixels |

### Fields and computation

`catalogue.rs::ingest_xhip` reads **only `main.dat`**, not `photo.dat` or
`biblio.dat`. Fixed-width fields (one-based inclusive byte columns): HIP 1–6,
SpType 237–262, Tc 264–266, Lc 268, RV 270–276 and its error 278–283.
The parser validates RV/error finiteness; the RV error is not subsequently
used in the current calculation. It does **not** use XHIP photometry, positions,
parallaxes, proper motions or a copied XHIP spectral-flux table.

- `build_run.rs` ingests the **whole** XHIP table. Reconstruction loops over all
  117,955 Hipparcos records and consults matching XHIP Tc/Lc, selecting a
  checksum-pinned CK04 template where the model has an assignment. Counts of
  available Tc/Lc are not a count of successfully assigned templates; the
  original external spectral-model/diagnostics artifact is not available in this
  checkout, so that intermediate count is not invented.
- `reconstruction.rs` scales the selected template using **Hipparcos Hp** and
  the SVO response/CALSPEC Vega scale, then integrates 300–336 and 336–650 nm.
  Statistical errors come from Hp error; systematic components include template
  mismatch, class mapping and the shared zero point. This is computed model
  radiance, not a transcription of XHIP measurements.
- `builder.rs` applies the Hp ≤ 4 cut. XHIP RV is used for a 2D/3D perspective
  sensitivity check on identity/membership (479 available bright cases), while
  stored directions use the Hipparcos 2D propagation. RV values are not emitted
  into the runtime map. SpType strings are copied to **external diagnostics**,
  not to the runtime product. Intermediate source records contain HIP IDs and
  propagated coordinates; those intermediate records are **not** approved for
  runtime redistribution by this assessment.
- The frozen merger admits 55 supplement records. HEALPix accumulation combines
  them with Gaia into integrated flux and complete uncertainties. The runtime
  product publishes four f64 scalars per pixel and provenance about input files,
  without HIP identifiers, Tc/Lc, RV, SpType, catalogue row order or original
  database arrangement. The exact number of distinct affected pixels and the
  supplement's total-flux share require the external per-star artifact; the
  public merge report establishes the bound and record count, not those values.

### Recoverability and significance

The map is not an original-record subset and cannot by itself reproduce the
117,955-row table or its selection/arrangement. The transformation is many-to-one.
However, **non-invertibility is not proved against arbitrary auxiliary information**:
a person with independent Hipparcos positions/magnitudes, Gaia-only subtraction
and the template library might infer classifications for an isolated dominant
bright star. The four output fields therefore must not be advertised as having
zero possible informational overlap. All 55 selected stars are scientifically
important bright-end complements; 55/117,955 alone does not settle qualitative
substantiality. No raw records or intermediate source diagnostics may be added
to runtime packages on the strength of this map-only reasoning.

### Applicable rights and conclusion

[CDS/VizieR usage rules](https://cds.unistra.fr/vizier-org/licences_vizier.html)
expressly allow scientific-context use with author/publication/publisher
citations, distinguish commercial uses, and refer source copyrights to the
catalogue ReadMe/origin. This supports the scientific ingestion/calculation;
an unanswered clarification does not revoke that published use permission.
It is not a newly invented CC licence for the XHIP database.

[Directive 96/9/EC](https://eur-lex.europa.eu/legal-content/EN-DE/ALL/?uri=CELEX:31996L0009)
Arts. 3(1–2) distinguish original selection/arrangement from contents; Arts.
7(1–2,5), 8 and 9 distinguish protected extraction/reutilization, lawful-user
insubstantial use and research exceptions. The research exception is not an
unlimited worldwide right to publish a source database. In particular:

- [Football Dataco, C-604/10](https://eur-lex.europa.eu/legal-content/en/ALL/?uri=CELEX:62010CJ0604), paras. 29–39: database copyright concerns original structure,
  not an automatic monopoly over factual contents or mere labour.
- [BHB, C-203/02](https://eur-lex.europa.eu/legal-content/EN/TXT/?uri=CELEX:62002CJ0203): obtaining/verifying/presenting investment and qualitative as well as
  quantitative substantiality matter; investment creating data is distinct.
- [Directmedia, C-304/07](https://eur-lex.europa.eu/legal-content/EN/ALL/?uri=CELEX:62007CJ0304_SUM): assessment/transformation is not by itself a defence if protected
  contents have actually been transferred; raw-byte copying is not required.
- [Ryanair, C-30/14](https://eur-lex.europa.eu/legal-content/EN/SUM/?uri=CELEX:62014CJ0030): absence of copyright/database protection does not automatically
  invalidate applicable contractual restrictions.

EU/France are relevant to the CDS/ESA sources; Spain's [TRLPI Arts. 133–135](https://www.boe.es/buscar/act.php?id=BOE-A-1996-8930)
illustrate national implementation. The publisher's jurisdiction is not inferred
from the maintainer's name or timezone. The authorized reviewer must identify
applicable jurisdiction and any contractual obligations. A 2012 release date
is not proof that a database right has expired: publication timing, qualifying
investment and substantial updates matter.

**Conclusion:** scientific calculation is supported by published CDS terms
(A); the map-only, factual-output rationale for not needing a new XHIP licence
is substantial technical evidence (B), rather than merely an unanswered email.
Whether this particular high-impact selection/model output is legally a protected
reutilization is **D: a specific legal interpretation**, not **E: an established
missing permission**. The maintainer has expressly adopted the map-only rationale
for the stated noncommercial channels (§13). That project finding does not
assert a binding judgment or waive an actually applicable protected right. Do not request universal
commercial rights or call the release prohibited without an identified right.
If the reviewer determines that protected content is reused beyond the existing
scientific grant, ask CDS to identify the relevant holder and obtain that narrow
map/channel permission; no such refusal or necessity has been established here.

## 10. AGPL code and separately identifiable scientific data

[AGPL-3.0](https://www.gnu.org/licenses/agpl-3.0.en.html) §§0–2 limit coverage
to the licensed program/copyright-based modifications; §2 conditions output
coverage on whether output is a covered work. §5 permits aggregates with
separate, independent works. §§5(c), 7 and 10 also mean that an actually combined
covered work cannot be placed under an additional general noncommercial rule.
The [GNU FAQ](https://www.gnu.org/licenses/gpl-faq.en.html#MereAggregation)
is interpretive guidance, not a court judgment or a licence from ESA/CDS.
Its same-executable guidance is material counterevidence: it classifies software
modules included in one executable as a combined program. That passage addresses
programs/modules, so applying it to an independently generated numerical input
requires a further classification; it does not expressly decide this map's case.
The alternative argument relies on the data's independent origin, generic format
and absence of program expression, not merely on separate licence notices.
Neither that argument nor `include_bytes!` alone establishes the §5 aggregate
exception. The maintainer expressly adopted this application of the exception (§13),
beyond the earlier general acceptance of separate licences.

| Architecture | Compatibility finding |
| --- | --- |
| AGPL evaluator plus independent local scientific file | Separately identifiable code/data can retain distinct terms as an aggregate, provided the data is not itself an adaptation of the program. Functional use alone does not automatically transfer the code licence to data. |
| Crate/wheel with identifiable data resource | Same substantive test; shipping two licence files is not sufficient by itself. Attribution, actual independence, data grants and recipient rights matter. |
| `include_bytes!` scientific blob in the compiled library | Container count alone does not decide copyright scope. This versioned, checksum-addressed map is generic scientific input readable independently of NSB, generated from catalogues rather than copied program expression. That supports separate identity even inside one binary, **the maintainer has accepted the aggregate classification after reviewing the same-executable counterargument (§13)**. This is a bounded project finding, not an upstream waiver. |
| Map legally held to be part of one AGPL-covered adaptation/combined work | NC constraints cannot simply be added to that covered work. Change the distribution arrangement or obtain compatible rights; do not redefine AGPL as a noncommercial software licence. |

**Interpretation accepted by the maintainer (§13):** the evaluator code is AGPL-3.0-only;
the independent map retains the upstream data-rights boundary. The compact
format, separate checksum/provenance contract and advanced external-map loader
support that interpretation. This is not an assertion that `include_bytes!`
automatically makes incompatible licences compatible. The maintainer explicitly accepted this classification at
2026-10-14T19:50:19Z. FSF's public compliance contact is an
optional route for this narrow question, not an automatic extra permission gate.

If that classification is rejected, a separate installed local resource is the
preferred fallback, preserving all bytes/checksums and offline evaluation. A
Rust library cannot safely assume that Cargo's source checkout is present at
application runtime; such a change needs an actual installation/resource-location
contract and wheel resource handling. Merely replacing `include_bytes!` with a
compile-time absolute path would fail the requested offline installed-package
contract and is not implemented as a cosmetic legal workaround.

Commercial use of **the code** is permitted under AGPL's obligations. Commercial
exercise of rights in the protected Gaia data needs the applicable data licence
or permission. Overriding a map at runtime does not remove restricted bytes from
a redistributed binary; a code-only commercial redistribution must actually
exclude those bytes or have permission. No data restriction is imposed on the
code licence itself.

## 11. Accepted channel-specific decision and conditions

Disposition: `approved_with_conditions` for noncommercial scientific publication
on **GitHub source/releases, crates.io, PyPI wheels and sdists**. The maintainer
expressly accepted the two interpretations in §§9–10 (§13); the authoritative
JSON records the actual identity, role, timestamp, pins and restrictions.

1. Scope exactly the frozen #207 product, compact map and sidecar hashes pinned
   by the current review bundle. Original catalogues, response curves, spectral
   files and per-star diagnostics are outside the approved package scope.
2. Preserve Gaia's licence URI, ESA/DPAC credits, change descriptions, the direct
   original-licensor offer and applicable NC restrictions; make no unrestricted
   commercial data grant or ESA/CDS endorsement claim.
3. Apply the already authorized noncommercial publication scope (§13).
   No individual Gaia/Tycho permission is needed for covered public-licence acts.
4. Apply the accepted map-only XHIP and separate code/data aggregate findings
   under the EU framework discussed above, without inferring national domicile.
   Retain the maintainer-provided SVO/Hipparcos correspondence privately; do not demand an unsolicited universal commercial
   grant. Obtain a new narrow permission only if a relevant protected act is
   actually identified outside existing grants/limitations.
5. Keep the complete source notices/contact routes in all distributions. Run
   promotion and release validation against the exact approved revision and
   assets before publication. Public licences still govern users' actual uses;
   this project decision cannot waive third-party rights.

The existing machine-verifiable review-bundle checksum condition pins these
facts and notices. Do not insert free-text conditions in the JSON and then bypass
its verifier: record scope in the supported restrictions/pinned-artifact fields
and use supported repository-file/asset checksum conditions. The human's actual
identity/role/date must be entered only by the authorized process.

**Readiness classification:** rights findings and project approval recorded;
release readiness now depends on actual production activation and validation. There is currently no
proven third-party refusal or established missing universal permission. Full
upstream regeneration still has the disclosed historical UV-training snapshot
limitation; exact frozen-output reproduction/equivalence is separately verified.

## 12. Primary-source retrieval ledger (2026-10-14)

The following SHA-256 values identify the HTTP response bodies examined, not
immutable upstream licences or permission letters. Dynamic pages may change.
Failed requests and empty anti-bot responses are excluded. The legal provisions
and case-law pinpoint references are cited in section 9; an empty HTTP response
is not evidence that those provisions were retrieved. Private correspondence
is represented only by the maintainer-reported scope above.

| Primary URL | Retrieved | Response SHA-256 |
|---|---|---|
| <https://www.cosmos.esa.int/web/gaia-users/license> | 2026-10-14 | `532d59346479c2ca1e174912f5f6bb46c3f1336bca039288dfaf199879682fb9` |
| <https://www.cosmos.esa.int/web/esdc/terms-and-conditions> | 2026-10-14 | `f1b843e9243c3e848eea92038cee10615988e172d33d48eeb849463a9c9f3031` |
| <https://cds.unistra.fr/vizier-org/licences_vizier.html> | 2026-10-14 | `f6bab16f7060f41c98958f0044fa3a526eeeeacdd23cb2fa7a529016d2b29e9f` |
| <https://cdsarc.cds.unistra.fr/viz-bin/ReadMe/V/137D?format=html> | 2026-10-14 | `198226adb40ea17950813d39929e46a440d3d03c4c36f313b1cec3d3785359cf` |
| <https://svo2.cab.inta-csic.es/theory/fps/> | 2026-10-14 | `eb5143b7370ef53775927f2e4e1fad7aab61a854dbdb035b38be563db9e4efbf` |
| <https://archive.stsci.edu/hlsp/reference-atlases> | 2026-10-14 | `dcdba17ad52a062099a13c517e59309b56e067948584124cce82ee60a06b3bc6` |
| <https://zenodo.org/records/14064023> | 2026-10-14 | `7bbee4bd5c3882b483b7376437647f425903c3227fe8be7f9370d74a931f99db` |
| <https://www.spaceweather.gov/content/contact-us> | 2026-10-14 | `32d3c0df19021addcf9ac210871ea5f84123e6c72e234fc70335c6c0f00c2571` |
| <https://www.gnu.org/licenses/agpl-3.0.en.html> | 2026-10-14 | `12217a17b18a1713adc27443b85d3e7bd66106a64c79c2b8d44159c91391e56b` |
| <https://www.gnu.org/licenses/gpl-faq.en.html> | 2026-10-14 | `5053fe23f1da3b76245b17c416640cde37ce21b8175effc0c6619d6673a25d2e` |
| <https://raw.githubusercontent.com/GerritRo/nsb2/main/pyproject.toml> | 2026-10-14 | `45363fb68dc1b6d059b33cba9ccd0bc86e8467f5c0c08e0a161b1cfb9a2269f9` |
| <https://zenodo.org/records/15396676> | 2026-10-14 | `f30bb4c359d2696691dad9bc4a85ad459be5f4304bc6b40a9acdf7bc6e34c418` |
| <https://cds.unistra.fr/about/contact/> | 2026-10-14 | `1947edd8c784adb159fba9aaece15309284d9d5c40cb1eed8b40ec38e08acadd` |
| <https://www.ast.cam.ac.uk/people/floor.van-leeuwen> | 2026-10-14 | `0167e02d9f8c9d315507c37c49d4dac25be1612049717e448305fb7faadc0153` |
| <https://www.cosmos.esa.int/web/gaia/newsletter/contents> | 2026-10-14 | `3ff7f27196d8ac6736dd082c3471321a336eb57d192d0970666cfedd62703324` |
| <https://gmd.copernicus.org/articles/18/4353/2025/> | 2026-10-14 | `36881da057e9ec8b10aa51bbd411aecd7826f8526b1d409b1c1b87b3e99d1504` |
| <https://zenodo.org/records/8063930> | 2026-10-14 | `58cf5f7e4ade365cceaeef2834f68521ed49ddb972b198bfc36905d6db9d6549` |
| <https://creativecommons.org/licenses/by-nc/3.0/igo/legalcode.en> | 2026-10-14 | `f0cce843f2c2496cfc17fcbd753c0022d7696395f4c5d4f7d28341f64c8bcde7` |
| <https://oceanservice.noaa.gov/about/faq.html> | 2026-10-14 | `fc463ed03a8393ca60d56aa269b1c5139eb946cc6e1aea30b38fb909f6c641cd` |
| <https://library.noaa.gov/blogs/news-research-highlights/question-of-the-quarter-noaacopyright> | 2026-10-14 | `990fb5c112becd045c8654f03f5c962acfb5c53d43bdc728ce0e733b295f07df` |

## 13. Explicit project authorization and accepted findings

**Recorded 2026-10-14T19:47:36Z.** The maintainer's direct instruction in this
Codex conversation explicitly authorizes preparing and proceeding with the
noncommercial scientific release, including production activation once the
rights gates are satisfied, committing/pushing to existing PR #238, and updating
#103/#239. It expressly withholds automatic merge, tag and registry publication.
The instruction confirms free scientific distribution without monetization or
company exploitation, authority over NSB publication policy, acceptance of
separate AGPL code/scientific-data terms and applicable NC limits, preservation
of notices, an embedded-data preference, and no known additional contracts.
These facts and this project authorization are established, not outstanding.

Traceable instruction evidence: the user-supplied `Pasted text.txt` beginning
“You are continuing your work on the NSB Starlight release”, SHA-256
`9a0df9c04a585bd425e02c2bb50bdf18269c355116dd2ed8b20451f0b88f5054`.
The original is retained in the conversation attachment; the private local path
is deliberately not a required public build input. GitHub's authenticated
`GET /user` returned `VPRamon` (ID `74069811`, profile name
`Vallés Puig, Ramon`); `GET /repos/VPRamon/NSB` returned owner `VPRamon` and
admin/maintain/push permissions. This confirms the available publication account
and role, not a third-party rights grant, legal qualification or signature.
Agent-posted GitHub reports of this instruction must be labelled as reports;
they are not independent human acceptance of a disputed interpretation.

**Additional direct human acceptance, recorded 2026-10-14T19:50:19Z:** the
maintainer answered the two-finding question in this conversation with
“Accept both findings for this noncommercial release”. The question explicitly
identified full-table XHIP ingestion, 55 influencing stars, the GNU FAQ
same-executable guidance and EU database rules, and distinguished a project
legal-risk finding from third-party permissions. The accepted findings are:

1. **XHIP output:** the integrated radiance/uncertainty map does not publicly
   reutilize a substantial protected part of XHIP's catalogue contents, despite
   full-table scientific ingestion and the 55 influencing bright stars. The
   relevant reasons are the new CK04/Hp integrations and changed informational
   product; absence of raw rows and small counts alone are insufficient.
   Directmedia C-304/07, including paragraph 47, prevents treating transformation
   or noncommercial purpose alone as a defence to protected transfer. The
   published CDS scientific-use permission supports computation; its application
   to this particular public output is the bounded finding now accepted.
2. **Embedded data:** this independently generated numerical map remains a
   separate scientific work/input under the source data terms when compiled
   into the AGPL library, rather than becoming part of an AGPL-covered combined
   work. Account for the GNU FAQ's software-module guidance above. If rejected,
   authorize the already requested separate local packaged resource design,
   preserving offline use and all scientific bytes; that alternative does not
   resolve finding 1.

The current evidence establishes **no specific third-party refusal or missing
permission**. These findings support conditional publication of the specified
new numerical product under the existing source terms and the accepted
map-only/independent-data rationale. No new individual XHIP or universal
commercial permission is required by that finding. The maintainer is recorded
as the actual project release decision-maker, not as qualified legal counsel.
This acceptance cannot waive an actual protected right or replace a grant if
one is later shown necessary. It does not authorize source-table/curve uploads,
commercial data reuse, automatic merge, tags or registry publication.

The authoritative decision is `approved_with_conditions`, not the synthetic
test template. Its notes identify this direct conversation acceptance; the
public #103/#239 reports are agent-posted records of it. The final review bundle
is frozen before activation; promotion and actual publication update only the
verified current inventory, without repinning this accepted evidence.
