# Gaia DR3 starlight derived-product policy template

Status: Template only; not approved for production use.
Audience: NSB maintainers preparing a Gaia DR3 XP-derived starlight release artifact.
Scope: Redistribution, attribution, provenance, and production-approval evidence for the derived runtime starlight CSV/TOML pair.

This file is intentionally **not** the production policy file consumed by the release pipeline. The production command must receive a separately reviewed file, expected by the documented maintainer workflow as:

```text
docs/policies/gaia_dr3_starlight_derived_product_policy.txt
```

Do not copy this template to that path until every section below has been completed and reviewed.

## Approval record

```toml
approved_for_production = false
reviewer = ""
review_date_utc = ""
review_issue_or_pr = ""
source_terms_checked = false
derived_product_redistribution_allowed = false
raw_gaia_data_redistribution_allowed = false
runtime_artifact_only = true
```

The final production policy must set `approved_for_production = true` only after review. The final production policy must also make clear that NSB ships only the derived runtime starlight asset, not raw Gaia XP spectra or raw Gaia source-row dumps.

## Source data covered

Record the exact Gaia products and access paths used for the release extraction.

- Source catalogue: Gaia DR3
- Source table: `gaiadr3.gaia_source`
- XP product type: XP sampled spectra
- Retrieval mode: official checksummed XP sampled bulk inventory; concurrent Gaia DataLink only for controlled validation or repair
- Selection summary: `has_xp_sampled = true`, G <= 15, non-null source ID/coordinates, one row per published source ID; retain `duplicated_source` as a quality flag
- Expected selected population: 34,468,373 unique source IDs, subject to exact release-time TAP and bulk reconciliation
- Band used for the derived product: 336-650 nm passband-integrated photon radiance
- Completeness limitation: sampled XP is a magnitude-limited subset of continuous XP; attach the reviewed missing-flux report

Add the final ADQL file checksum and extraction diagnostics checksum here before approval.

## Redistribution decision

State the reviewed conclusion for each item.

- Gaia source terms reviewed: no
- Derived CSV/TOML runtime artifact may be redistributed with NSB: no
- Raw Gaia metadata rows may be committed to the repository: no
- Raw Gaia XP spectra may be committed to the repository: no
- Additional attribution text required in documentation or release notes: TBD
- Additional license text required in binary/source distribution: TBD

The final production policy must include the specific attribution text and distribution obligations that apply to the derived starlight map.

## Files allowed after approval

Only the following derived runtime artifacts may be considered for repository inclusion after approval and validation:

```text
The final filenames must encode `336_650nm` and the nside selected by the reviewed 64/128/256 sweep. Record the exact paths only after that review.
```

The following are not allowed in the repository unless a separate review explicitly approves them:

```text
raw Gaia XP spectra
raw Gaia source-row dumps
Gaia DataLink response bodies
intermediate normalized XP chunks
full Gaia release-input extracts
```

## Required production evidence

Before creating the final `.txt` policy file, attach or reference:

1. Gaia source terms reviewed and dated.
2. Attribution text for NSB documentation and release notes.
3. Confirmation that the derived runtime map is redistributable.
4. Confirmation that raw Gaia XP spectra and raw Gaia row dumps are not committed.
5. Exact production extraction command.
6. Exact map-generation, validation, and packing commands.
7. SHA-256 checksums for the Gaia extract, canonical source CSV, release map CSV, runtime manifest, diagnostics, and validation report.
8. Independent validation reference used for production comparison.
9. Final decision on whether the runtime asset is bundled or distributed externally.
10. Reviewed 336-650 nm missing-flux estimate, uncertainty, and global/plane/pole convergence evidence.
11. Reviewed nside=64/128/256 sweep recommendation.

## Final-policy checklist

The final policy file may be created only when all boxes are checked:

- [ ] `approved_for_production = true` is justified by review evidence.
- [ ] Reviewer and review date are recorded.
- [ ] Gaia source terms and attribution requirements are documented.
- [ ] Derived runtime artifact redistribution is explicitly allowed.
- [ ] Raw Gaia data redistribution is either explicitly disallowed or separately approved.
- [ ] The policy references the exact source selection and production commands.
- [ ] The policy references the independent validation report.
- [ ] The policy references all required checksums.
- [ ] The release checklist has been updated with the final decision.

## Notes for maintainers

The production pipeline is intentionally fail-closed. Missing or placeholder policy evidence must block `generate_gaia_starlight_release_inputs --production` and must not be bypassed by creating an unreviewed file at the production policy path.
