# Third-party notices

NSB source is licensed under AGPL-3.0-only (see [`LICENSE`](LICENSE) and
[`README.md`](README.md#licensing)). Third-party dependencies, scientific data,
and reference material retain their own licence and attribution obligations.
Distributors must review and comply with the complete terms that apply to the
artifacts and dependency versions they ship. This file consolidates attribution
for third-party data and reference material used by NSB components.
Component-specific detail, licence classification, distribution status, and
checksums live next to each component; see the links below.

This file records attribution and licence facts. It does not itself
authorize redistribution of any listed third-party or derived artifact.
Where a component's licensing folder records a pending human decision, that
decision remains the sole authorization gate.

## Starlight (integrated starlight component)

While the authoritative #103 decision is pending, NSB packages exclude the
production Starlight map. Authorized promotion admits the exact compact pair.
The frozen candidate and its merge report **are present in the Git repository**
and constitute a distinct distribution channel. They are not yet authorized
as production assets; issue #103 remains the human redistribution gate.

Provenance and exact source-family terms:
[artifact inventory](docs/nsb_components/starlight/licensing/artifact-inventory-v1.toml),
[attributions](docs/nsb_components/starlight/licensing/ATTRIBUTION.md),
and [scientific references](docs/nsb_components/starlight/licensing/SCIENTIFIC_REFERENCES.md).

- **Gaia DR3 GaiaSource/XP:** ESA CC BY-NC 3.0 IGO; cite Gaia/DPAC.
- **Original Hipparcos/Tycho (ESA):** CC BY-NC 3.0 IGO, Credit: ESA.
- **Tycho-2 (Høg et al. 2000, CDS I/259):** the CDS catalogue's own record
  declares CC BY-NC 3.0 IGO; see
  <https://cdsarc.cds.unistra.fr/viz-bin/cat/I/259>. All 20 Tycho-2 shards
  remain offline inputs.
- **Hipparcos-2 (van Leeuwen 2007, CDS I/311):** Floor van Leeuwen
  confirmed to the maintainer (reply reported 2026-10-14) that the 2007
  reduction is based on ESA Hipparcos data and publicly available; he
  requests citations to both his 2007 book and A&A article. An explicit
  I/311 licence or blanket commercial/downstream map redistribution grant
  was not supplied.
- **XHIP reference:** Anderson & Francis (2012), *XHIP: An Extended
  Hipparcos Compilation*, *Astronomy Letters* **38**, 331,
  [ADS 2012AstL...38..331A](https://ui.adsabs.harvard.edu/abs/2012AstL...38..331A/abstract).
- **Hipparcos-2 reference:** van Leeuwen (2007), *Validation of the new
  Hipparcos reduction*, A&A **474**, 653–664,
  [DOI 10.1051/0004-6361:20078357](https://doi.org/10.1051/0004-6361:20078357).
  Also cite van Leeuwen (2007), *Hipparcos, the New Reduction of the Raw
  Data*, Astrophysics and Space Science Library **350**,
  [DOI 10.1007/978-1-4020-6342-8](https://doi.org/10.1007/978-1-4020-6342-8).
- **CDS/VizieR acknowledgement:** cite the original authors and credit the
  catalogue access service, CDS, Strasbourg, France
  (DOI [10.26093/cds/vizier](https://doi.org/10.26093/cds/vizier)).
- **CALSPEC and Castelli-Kurucz CK04 REFERENCE-ATLASES HLSP:** CC BY 4.0;
  cite STScI/ReDCaT and DOI 10.17909/t9-khb7-4049.
- **Cantat-Gaudin Gaia DR3 M10 selection dataset:** CC BY 4.0;
  Zenodo DOI 10.5281/zenodo.8063930.
- **XHIP V/137D:** catalogue-specific redistribution licence not verified;
  CDS/VizieR scientific-use and citation conditions apply to access.
- **SVO Hipparcos Hp_bes passband:** according to the maintainer-provided
  reply from the SVO/FPS team, users should be informed of SVO's role in
  NSB's calculations and receive the acknowledgement and citations below.
  This does not grant redistribution of the original response curve.

No raw upstream catalogue, atlas, or filter curve is intended to be bundled in
release packages. This notice cannot supply a missing upstream permission.

Licensing clarifications for XHIP and Hipparcos-2 were requested on
2026-10-08. Hipparcos-2's author has since confirmed public availability
and requested citations to his two works (reply reported 2026-10-14);
XHIP is still awaiting source-specific clarification. Neither a request
nor a statement of availability is a blanket redistribution grant;
#103 remains pending.

### SVO/FPS acknowledgement for Starlight-derived calculations

The SVO team provided the following requested acknowledgement via
maintainer-reported correspondence (2026-10-08), for use of
`Hipparcos/Hipparcos.Hp_bes` (Bessell 2000):

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

The original SVO curve is not bundled by the `nsb` crate. These credits
also remain applicable when the scientific computation is redistributed
as a derived map; they do not grant a licence for the original table.

## Other bundled runtime assets

`airglow_palace_v1.dat` is an NSB-generated derivative of the PALACE v1.0 model
data `palace_cont.fits` and `palace_var.fits` by Stefan Noll, Carsten Schmidt,
Patrick Hannawald, Wolfgang Kausch, and Stefan Kimeswenger. PALACE model data are
licensed under Creative Commons Attribution 4.0. Source: Zenodo record 14064023,
DOI `10.5281/zenodo.14064023`. The PALACE program is GPL-3.0-or-later; NSB's
generator does not copy, execute, or redistribute that program.

`solar_spectrum.dat` is an NSB-authored analytic product generated from
physical constants and distributed under AGPL-3.0-only with the rest of NSB.
The bundled `moonlight_mie_nsb_v1.dat` is generated by NSB's AGPL-3.0-only
data tooling from aerosol parameters published in Jones et al. (2013), which
in turn cites Warneck & Williams (2012), using equations from Bohren & Huffman
(1983). It contains newly calculated numeric output and no third-party table or
solver bytes.

The bundled `moonlight_multiscatter_nsb_v1.dat` is newly calculated by NSB's
AGPL-3.0-only scalar radiative-transfer implementation from the Jones et al.
(2013) equations and the compatible NSB Mie product. It contains no historical
ESO correction-table bytes and no third-party solver or dataset bytes.

## Reporting a missing or incorrect notice

Open an issue identifying the affected bundled asset or dependency.

## Compact Starlight transport and approval boundary

The binary format preserves the frozen Gaia-derived radiances and all available
uncertainties; changing representation does not change data rights. While #103
is pending, no compact Starlight payload is shipped. After actual authorization,
production packages may embed only the checksum-approved compact map and
sidecar. The software remains AGPL-3.0-only; Gaia/Tycho noncommercial conditions
and all source-specific attribution remain separate. Package availability does
not promise unrestricted commercial reuse of the scientific data. See the
[source-specific channel assessment](https://github.com/VPRamon/NSB/blob/main/docs/nsb_components/starlight/licensing/derived-map-rights-assessment-v1.md).

## Gaia, catalogue and spectral-model credits

> This work has made use of data from the European Space Agency (ESA)
> mission Gaia (<https://www.cosmos.esa.int/gaia>), processed by the Gaia
> Data Processing and Analysis Consortium (DPAC,
> <https://www.cosmos.esa.int/web/gaia/dpac/consortium>). Funding for the
> DPAC has been provided by national institutions, in particular the
> institutions participating in the Gaia Multilateral Agreement.

Gaia DR3/XP source terms: [CC BY-NC 3.0 IGO](https://creativecommons.org/licenses/by-nc/3.0/igo/),
[ESA licence](https://www.cosmos.esa.int/web/gaia-users/license).
The map spatially aggregates integrated 300–650 nm photon flux, with calibrated
UV completion and bright-star supplementation; it is a transformed output,
not the original catalogue. Cite De Angeli et al. (2023),
[XP processing](https://doi.org/10.1051/0004-6361/202243680), and Montegriffo
et al. (2023), [XP calibration](https://doi.org/10.1051/0004-6361/202243880),
with the [Gaia DR3 documentation](https://gea.esac.esa.int/archive/documentation/GDR3/).

Tycho-2: Credit ESA; Høg et al. (2000), *The Tycho-2 catalogue of the 2.5 million
brightest stars*, A&A 355, L27–L30, [ADS](https://ui.adsabs.harvard.edu/abs/2000A%26A...355L..27H/abstract).
[CDS I/259](https://cdsarc.cds.unistra.fr/viz-bin/cat/I/259) designates
CC BY-NC 3.0 IGO. Original catalogue bytes are not included.

CK04: Castelli & Kurucz (2004). CALSPEC: STScI/ReDCaT spectrophotometric
standards; Bohlin, Gordon & Tremblay (2014), [PASP 126, 711](https://doi.org/10.1086/677655).
[MAST REFERENCE-ATLASES](https://archive.stsci.edu/hlsp/reference-atlases),
DOI [10.17909/t9-khb7-4049](https://doi.org/10.17909/t9-khb7-4049),
licenses its HLSP data under [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/).
NSB integrates selected CK04 models, scales bright stars using CALSPEC Vega
and Hp photometry, and calibrates UV completion using CALSPEC; original
spectra are not shipped. The bright-star zero point is pinned as
`alpha_lyr_stis_012`; the older UV-training snapshot remains incompletely pinned.

Selection weights: Cantat-Gaudin et al. (2023),
[dataset 10.5281/zenodo.8063930](https://doi.org/10.5281/zenodo.8063930),
CC BY 4.0; [associated paper](https://doi.org/10.1051/0004-6361/202244784).
The source selection table is not included; its use is recorded in the
frozen candidate provenance. None of these notices grants missing rights
in XHIP, Hipparcos-2 or the final derived map; #103 remains authoritative.

## Rights enquiries and concerns

The [scientific-data guide and verified contact directory](https://github.com/VPRamon/NSB/blob/main/docs/nsb_components/starlight/licensing/README.md)
identifies the providers, public contact channels, evidence and verification dates.
Report a concern through [NSB issues](https://github.com/VPRamon/NSB/issues),
identifying the asset, package/version, asserted right and relevant evidence.
The maintainer reviews the evidence with the provider, corrects notices or
artifacts, and suspends affected distribution when warranted; corrections are
communicated in the issue and release notes. This procedure does not itself
confer permission. Gaia noncommercial restrictions continue to apply to the
embedded data even when a user selects an external map at runtime. AGPL code
rights remain distinct. The combined binary's aggregate interpretation and
XHIP's map-only treatment require the specific human review recorded in #103.
Tycho-2 fields in the frozen pipeline are diagnostic only; no Tycho photometric
value changes its radiances or uncertainty values.

Public contact routes verified 2026-10-14 (custodians are not necessarily sole
rights holders): ESA licensing `data.licences@esa.int` ([archive terms](https://www.cosmos.esa.int/web/esdc/terms-and-conditions));
Gaia technical support `gaia-helpdesk@esa.int` ([official announcement](https://www.cosmos.esa.int/web/gaia/newsletter/contents));
CDS/XHIP/Tycho `cds-question@unistra.fr` ([CDS](https://cds.unistra.fr/about/contact/));
Hipparcos-2 [van Leeuwen's institutional directory](https://www.ast.cam.ac.uk/people/floor.van-leeuwen);
SVO `svo-support@cab.inta-csic.es` ([FPS](https://svo2.cab.inta-csic.es/theory/fps/));
STScI `archive@stsci.edu` ([MAST](https://archive.stsci.edu/hlsp/reference-atlases));
PALACE [paper correspondence](https://gmd.copernicus.org/articles/18/4353/2025/);
selection weights [Cantat-Gaudin's record/ORCID](https://zenodo.org/records/8063930);
F10.7 [NOAA SWPC contact](https://www.spaceweather.gov/content/contact-us).
No additional individual permission is required for acts already covered by an
applicable public grant. Names identify credited creators, not proof of sole
legal ownership. The guide records existing correspondence and the limited
questions still requiring interpretation.
