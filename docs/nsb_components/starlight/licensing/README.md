# Starlight redistribution and licensing

Status: Current redistribution policy for Starlight data products.

NSB 0.1.0 does not redistribute a Gaia-derived production Starlight map. Raw
Gaia, Hipparcos, Tycho, CALSPEC, CK04, and other upstream catalogue/reference
bytes are not bundled by the runtime package.

The attribution requirements and source families relevant to a future
Starlight product are documented in [ATTRIBUTION.md](ATTRIBUTION.md) and the
project-wide [THIRD_PARTY_NOTICES.md](../../../../THIRD_PARTY_NOTICES.md).

## Promotion contract

`nsb-data` implements a fail-closed redistribution review contract. A future
candidate promotion must provide:

- an inventory of every distributed and referenced artifact;
- exact SHA-256 identities for distributed derived outputs;
- source/release/licence information for upstream inputs;
- the intended distribution channels;
- an authorized human redistribution decision; and
- any machine-verifiable conditions attached to that decision.

The Rust implementation validates those records but cannot grant
redistribution approval itself. Approval belongs to an authorized human
reviewer.

## First-release rule

Until a Gaia-derived product has completed that process, it remains external to
the NSB release. Generating or validating a candidate does not make it a bundled
runtime asset.

## Verified licenses and unresolved permissions (candidate 2026-10-06)

- **Gaia DR3 GaiaSource/XP and Gaia EDR3 crossmatch:** ESA CC BY-NC 3.0 IGO.
- **MAST REFERENCE-ATLASES CK04 and CALSPEC:** HLSP CC BY 4.0
  (<https://archive.stsci.edu/hlsp/reference-atlases>).
- **Cantat-Gaudin 2023 selection-function dataset:** Zenodo CC BY 4.0
  (<https://zenodo.org/records/8063930>).
- **ESA original Hipparcos/Tycho:** CC BY-NC 3.0 IGO.
- **Tycho-2 (CDS I/259):** CDS's record for Tycho-2 explicitly declares
  CC BY-NC 3.0 IGO; source family verified for the 20 frozen shards
  (<https://cdsarc.cds.unistra.fr/viz-bin/cat/I/259>).
- **Hipparcos-2 (CDS I/311):** 2007 re-reduction has no verified
  source-specific reuse/redistribution grant in the ReadMe; still pending.
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
remaining blocker, **not a license grant**. All original scientific source
files and checksum pins are preserved.

The exact derived candidate and report are already tracked in the Git
repository; their `git_repository` distribution status must be considered
separately from the non-distributed crates.io/PyPI production assets.
The issue #103 human redistribution review remains `pending`.

## Derived-map rights analysis

The channel-specific preliminary analysis, factual byte/provenance flow, and
remaining legal questions are recorded in
[`derived-map-rights-assessment-v1.md`](derived-map-rights-assessment-v1.md).

The analysis distinguishes (1) access and offline computation from (2)
redistribution of the XHIP table or SVO Hp transmission curve and (3)
redistribution of the aggregate numerical HEALPix map. It is an evidence
record, **not** an automatic human approval under #103.
