# Starlight redistribution and licensing

Status: Current redistribution policy for Starlight data products.

The maintainer has approved the exact compact Starlight product with conditions
and accepted the XHIP calculated-output and independent embedded-data findings.
Production promotion enables the offline four-component configuration. Raw
Gaia, Hipparcos, Tycho, CALSPEC, CK04, and other upstream catalogue/reference
bytes are not bundled by the runtime package.

The attribution requirements and source families relevant to a future
Starlight product are documented in [ATTRIBUTION.md](ATTRIBUTION.md) and the
project-wide [THIRD_PARTY_NOTICES.md](../../../../THIRD_PARTY_NOTICES.md).

## Promotion contract

`nsb-data` implements a fail-closed redistribution review contract. A future
candidate promotion must provide:

- an inventory of every distributed and referenced artifact;
- exact SHA-256 identities for every distributed artifact, including documentation;
- source/release/licence information for upstream inputs;
- the intended distribution channels;
- an authorized human redistribution decision; and
- any machine-verifiable conditions attached to that decision.

The Rust implementation validates those records but cannot grant
redistribution approval itself. Approval belongs to an authorized human
reviewer.

The [lifecycle procedure](../release-candidate/README.md#approval-and-publication-lifecycle)
freezes `approval-inventory-v1.toml` before review and derives the separate current
`artifact-inventory-v1.toml` afterward. Planned channels are not actual uploads.
A schema-compatible, deliberately non-authorizing decision template includes
all pins/channels and null reviewer fields. Neither promotion nor publication
metadata silently changes the frozen evidence or grants a second permission.

## Admission rule

Until a Gaia-derived product has completed that process, it remains external to
the NSB release. Generating or validating a candidate does not make it a bundled
runtime asset.

## Verified licences and output-rights review (updated 2026-10-14)

- **Gaia DR3 GaiaSource/XP and Gaia EDR3 crossmatch:** ESA CC BY-NC 3.0 IGO.
- **MAST REFERENCE-ATLASES CK04 and CALSPEC:** HLSP CC BY 4.0
  (<https://archive.stsci.edu/hlsp/reference-atlases>).
- **Cantat-Gaudin 2023 selection-function dataset:** Zenodo CC BY 4.0
  (<https://zenodo.org/records/8063930>).
- **ESA original Hipparcos/Tycho:** CC BY-NC 3.0 IGO.
- **Tycho-2 (CDS I/259):** CDS's record for Tycho-2 explicitly declares
  CC BY-NC 3.0 IGO; source family verified for the 20 frozen shards
  (<https://cdsarc.cds.unistra.fr/viz-bin/cat/I/259>).
- **Hipparcos-2 (CDS I/311):** **author-confirmed publicly available**
  (Floor van Leeuwen correspondence supplied by the maintainer, reported
  2026-10-14). Cite both van Leeuwen (2007) book and A&A paper.
  The exact catalogue licence and worldwide commercial/downstream
  derivative-map redistribution rights were not expressly granted.
  Treat source availability/attribution as clarified, under the accepted #103
  calculated-output finding; no new unrestricted commercial grant is implied.
- **XHIP V/137D:** CDS/VizieR research access/citation conditions;
  no catalogue-specific redistributable CC or SPDX grant verified.
- **SVO Hp_bes:** the maintainer supplied an SVO/FPS team reply confirming
  that acknowledging SVO FPS and Bessell (2000), and citing their four
  requested references, is sufficient **from the service's perspective**
  for calculations using the profile. Exact text: [ATTRIBUTION.md](ATTRIBUTION.md#svofps-exact-acknowledgement-and-requested-references).
  No explicit right to redistribute the **original response curve** was
  granted.

Never fill unknown upstream licences with AGPL, CC BY, or CC BY-NC by
analogy. `license` strings that begin with `NO_EXPLICIT_` are disclosure of a
unverified source grant, **not a licence grant or proof that calculated output needs permission**. All original scientific source
files and checksum pins are preserved.

The exact derived candidate and report are already tracked in the Git
repository; their `git_repository` distribution status must be considered
separately from the non-distributed crates.io/PyPI production assets.
The authoritative issue #103 decision is `approved_with_conditions`; the actual
maintainer acceptance and account verification are recorded in the assessment.

## Derived-map rights analysis

The channel-specific preliminary analysis, factual byte/provenance flow, and
remaining legal questions are recorded in
[`derived-map-rights-assessment-v1.md`](derived-map-rights-assessment-v1.md).

The analysis distinguishes (1) access and offline computation from (2)
redistribution of the XHIP table or SVO Hp transmission curve and (3)
redistribution of the aggregate numerical HEALPix map. It is an evidence
record, **not** an automatic human approval under #103.

## Practical data rights for users

NSB code is AGPL-3.0-only, including commercial use subject to that licence.
Scientific inputs/products retain their own rights. Gaia's public BY-NC 3.0 IGO
grant permits compliant noncommercial adaptations and redistribution; it does
not require an individual permission for every scientific reuse. CC BY 4.0
inputs permit commercial reuse with their attribution/notice conditions.
Neither licence grants rights that its licensor does not hold. The approved compact Starlight pair is included through production promotion
and retains this data-rights boundary.

The maintainer has explicitly authorized the noncommercial publication scope;
[the authorization record](derived-map-rights-assessment-v1.md#13-explicit-project-authorization-and-accepted-findings)
identifies the direct instruction and authenticated account. That scope requires
no renewed confirmation. The maintainer also explicitly accepted the specific
XHIP output/database-right and AGPL/independent embedded-data interpretations.
This is a project decision, not a third-party permission or qualified opinion.
See [the measured audit and proposed decision](derived-map-rights-assessment-v1.md#8-actual-noncommercial-publication-scope-and-public-grants).

## Verified rights-enquiry contact directory

Verified **2026-10-14** against the linked primary pages. An author, archive
custodian and legal rights holder are different roles; a support address is
not presumed authorized to waive every upstream right. These are public
contacts only; private emails/signatures and correspondence headers are not
reproduced. Existing correspondence status is reported from project evidence,
not newly sent communication.

| Source | Responsible party / ownership status | Public contact and primary evidence | Enquiry / existing status / further contact |
| --- | --- | --- | --- |
| Gaia DR3 and archive data | ESA is the named public licensor; DPAC credited producer | `data.licences@esa.int`, expressly designated by [ESA archive terms](https://www.cosmos.esa.int/web/esdc/terms-and-conditions) | Commercial or exceptional licensing; **no further permission needed for compliant public-grant NC use** |
| Gaia Archive / DPAC | ESA archive/helpdesk; not a separate blanket rights grantor | `gaia-helpdesk@esa.int`, [official updated helpdesk announcement](https://www.cosmos.esa.int/web/gaia/newsletter/contents); [user support hub](https://www.cosmos.esa.int/web/gaia-users/archive) | Archive/data/provenance questions and routing; technical route, not automatic legal authority |
| CDS/VizieR and XHIP V/137D | CDS is archive custodian; Anderson & Francis are scientific authors; final copyright/database-maker allocation **not established** | `cds-question@unistra.fr`, [CDS contact](https://cds.unistra.fr/about/contact/), also linked from [XHIP ReadMe](https://cdsarc.cds.unistra.fr/viz-bin/ReadMe/V/137D?format=html) | Ask about source terms or route a narrow derived-output question to the actual holder; clarification requested 2026-10-08, no reply recorded; further email only if the qualified review identifies a necessary unresolved act |
| Hipparcos-2 I/311 | van Leeuwen is the named reduction author; complete allocation/assignment of rights not established | [Cambridge institutional directory](https://www.ast.cam.ac.uk/people/floor.van-leeuwen), listing the author's public university contact; CDS can route catalogue questions | Availability and both citations confirmed in maintainer-provided correspondence, reported 2026-10-14; no blanket commercial grant inferred; no new universal-permission enquiry required |
| Tycho-2 I/259 | Høg et al. catalogue authors; CDS publishes the source-specific licence; sole ownership not independently established | [CDS contact](https://cds.unistra.fr/about/contact/) / [catalogue record](https://cdsarc.cds.unistra.fr/viz-bin/cat/I/259) | Route source-specific holder questions to CDS; source BY-NC grant and actual diagnostic-only use do not justify a new commercial-permission gate |
| SVO Hp_bes | SVO/CAB service provider; original Bessell passband authorship does not prove sole service ownership | `svo-support@cab.inta-csic.es`, linked on the [FPS home page](https://svo2.cab.inta-csic.es/theory/fps/) | Service/passband provenance; calculation acknowledgement reply reported 2026-10-08 is retained privately; no new permission sought for results already covered by that reply; original-curve publication is outside scope |
| CK04 / CALSPEC reference products | STScI/MAST publisher/custodian; Castelli, Kurucz and CALSPEC contributors credited; no sole-owner inference | `archive@stsci.edu` and [MAST helpdesk](https://stsci.service-now.com/mast), both linked from [REFERENCE-ATLASES](https://archive.stsci.edu/hlsp/reference-atlases) | Exact release/provenance questions; verified CC BY 4.0 grants need no additional permission for covered reuse |
| PALACE v1.0 | Noll, Schmidt, Hannawald, Kausch and Kimeswenger are creators; institutional rights allocation not independently verified | [Published paper's correspondence contact](https://gmd.copernicus.org/articles/18/4353/2025/) and [Zenodo release](https://zenodo.org/records/14064023) | Stefan Noll is the publicly designated corresponding author; data CC BY 4.0, no extra permission needed for the table derivative; code GPL is a separate work |
| Cantat-Gaudin M10 v1 | Tristan Cantat-Gaudin is record creator, affiliated with MPIA; complete institutional assignment not established | [Creator/ORCID on the source record](https://zenodo.org/records/8063930), [corresponding paper](https://doi.org/10.1051/0004-6361/202244784) | Dataset identity/scientific questions; CC BY 4.0 covers the intended attributed use; no fresh licence request needed |
| NOAA/NWS SWPC F10.7 | NOAA/NWS publishes the products; DRAO/Penticton observational lineage credited; federal-work status does not assign all third-party rights to NOAA | [SWPC official contact form](https://www.spaceweather.gov/content/contact-us) | Product provenance/third-party markings; no routine permission needed for NOAA federal works; ask only about an actually identified excluded third-party right |
| NSB solar/Moonlight calculations and atmospheric/zodiacal implementation | NSB contributors own their code/output expression where protected; published equations/facts are cited, not copied datasets | [NSB issue tracker](https://github.com/VPRamon/NSB/issues) | Implementation, attribution and corrections; no new source dataset licence introduced |
| AGPL interpretation | FSF Compliance Lab offers guidance; it is **not** ESA/CDS's rights holder | `licensing@fsf.org`, linked in the [GNU licensing FAQ](https://www.gnu.org/licenses/gpl-faq.en.html) | Optional bounded independent-data/compiled-container question; professional review may instead resolve it without waiting for FSF |

## Targeted enquiry drafts (not sent)

**For CDS, only if a specific XHIP right remains material after review:**

> NSB is a freely available, noncommercial astronomical research project. We
> calculate a 300–650 nm NSIDE=128 HEALPix radiance map from Gaia DR3 with 55
> admitted bright-star supplements. We read XHIP V/137D main.dat for HIP-linked
> spectral classification and a perspective-motion sensitivity check. The
> published map contains integrated scalar radiances/uncertainties, not HIP
> identifiers, classification/RV columns, source row arrangements or original
> catalogue tables. We intend GitHub, crates.io and PyPI distribution with full
> attribution and applicable Gaia NC conditions. Does the published scientific-use
> permission cover these calculated outputs, or does a specific copyright,
> database or contractual right require additional permission? If so, please
> identify the relevant holder and the narrow permission required for these
> channels. We are not requesting unrestricted commercial use of your catalogue.

**For FSF/qualified licensing review, if needed:**

> NSB is AGPL-3.0-only code using a generic, independently readable scientific
> HEALPix input. Its versioned bytes and provenance are separately identified,
> but currently compiled through include_bytes! into the library/wheel. The
> map is calculated from ESA data under BY-NC 3.0 IGO; it contains no NSB code
> expression. We preserve those data terms and impose no NC restriction on NSB
> code. For this actual architecture, can the code and independently generated
> data be conveyed as an aggregate under AGPL §5, or should the data instead be
> shipped as a separately installed local resource? We seek a classification,
> not a waiver of ESA's rights or a change to AGPL.

No email has been sent on the maintainer's behalf. An unanswered optional
enquiry is not itself a reason to prevent a legally supported release.

## Licensing concerns, corrections and removal requests

Submit a [licensing concern to NSB](https://github.com/VPRamon/NSB/issues/new).
Identify the affected release/file/hash, the source and claimed right, the
claimant's authority or connection, the proposed correction and a public
contact route. Do not publish private correspondence or personal data in an
issue; request a private communication route if sensitive evidence is needed.

The maintainer should acknowledge and investigate the claim, preserve the
relevant evidence, compare the exact shipped bytes/terms, and seek the relevant
holder or professional advice when necessary. A substantiated or credibly
unresolved distribution-right problem may require suspending affected new
uploads, disabling promotion, requesting registry withdrawal/yanking, or
replacing/removing affected data in a corrected release. Publish the resolution
and affected versions/hashes in the issue and release notes so users can update;
retain scientific change/provenance records. No blanket deadline, indemnity or
promise that notices/takedown eliminate legal responsibility is made.
