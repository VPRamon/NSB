# Pre-0.1.0 schema reset and checksum migration

All active NSB-owned formats start at v1. Candidate-v5, merge-report-v5,
runtime CSV v2 and runtime manifest v2 (including numeric `schema_version`)
are reset together. The merge report and nested scientific-policy format
versions also start at 1. Scientific model identities, inputs, thresholds,
source releases and dependency versions are unchanged. Active review inventories and bundles are consolidated to v1; original
documentary revisions are preserved only in the approval archive. Unsupported-version
examples in negative tests remain deliberately unsupported.

The original approval snapshots, decision and relevant external/CI evidence
are preserved byte-for-byte in [approval-history/pre-v1-reset](approval-history/pre-v1-reset/).
Other original files can be recovered from PR #240's prior head
`3b3a59ca33e1396d6cf879226efc5cb232cd315e`. Historical records retain their
original identifiers and hashes; they are not current product references.

## Exact artifact identities

| Artifact in `crates/nsb/data/` | Original SHA-256 | Regenerated SHA-256 |
|---|---|---|
| `starlight_nside128.csv` | `7e903ff289e76d07c018933b8f97fcf264cead73999912ff63f34b9d1e01b37d` | `e8d31d0a04a18314e3273ffc9e2ad449014adca8710b11bda2f3b6cf2c811526` |
| `merge_report.json` | `37ca25a4c684fba4818c54bea24652631e6b1cbde3f11d478b3cc13f11a96720` | `60a18ca8b92b6587ce23ffff718b115a22e38e67c2c6c952bc568dbfb7b0063c` |
| `starlight_nside128.release.csv` | `70069d81b02c48a588cce35bbf4bef2a12546d2885994e3eb43c66a66d734f6b` | `d649def4ca3c36036177f35816f3defc57e365d3f341c22a4f0dbf79051367e8` |
| `starlight_nside128.manifest.toml` | `b8d362b166ef15fedf6fc0b6875710d859cb11facc1507d43f119310053b17cd` | `9cd78ea9c0729fdc5c694aef2fccbc7c2dd7c01c8fa0ed0a079c4dc49b97a67d` |
| `starlight_nside128.compact.release.bin` | `ffd4642ca95a4efdc0e4b512838d7130fca11952a4f023d66a1ca8553a702784` | `11479bd0b6838d06df884b76dd74575bbc42d424ec85defecf847257c0f64d7f` |
| `starlight_nside128.compact.manifest.toml` | `b1b528ed6afe3fbc9f515efe66e0c0120b0bb90398d7eb91e0b175cf5f43681b` | `1fdaa3536520f74dd88a1ade37419234667de1cbb978b4539b4b8988b28a0cb8` |

## Reproduction and equivalence

Use the existing command twice, once for CSV and once for compact binary:

```bash
cargo run --locked -p nsb-data-tools --bin nsb-data -- dataset starlight stage-runtime \
  --repository-root . \
  --release-candidate docs/nsb_components/starlight/release-candidate/release-candidate-v1.toml \
  --output-csv crates/nsb/data/starlight_nside128.release.csv \
  --output-sidecar crates/nsb/data/starlight_nside128.manifest.toml
cargo run --locked -p nsb-data-tools --bin nsb-data -- dataset starlight stage-runtime \
  --repository-root . \
  --release-candidate docs/nsb_components/starlight/release-candidate/release-candidate-v1.toml \
  --output-csv crates/nsb/data/starlight_nside128.compact.release.bin \
  --output-sidecar crates/nsb/data/starlight_nside128.compact.manifest.toml
```

Both transports and sidecars were regenerated a second time into temporary
files and compared byte-for-byte. The candidate and packed CSV retain all
196,608 scientific rows byte-for-byte. The compact binary retains all four
f64 fields for all 196,608 pixels byte-for-byte; only its provenance header,
header length and internal SHA-256 change. Its internal digest verifies.
The merge report changes only format identifiers/numeric schema versions
and the canonical-map checksum; scientific values, model and input metadata
are identical. Validation manifests include updated hashes and byte counts.

## Evidence and authorization requiring renewal

- `redistribution-review-decision-v1.json`: the previous
  `approved_with_conditions` decision authorized the original exact hashes,
  inventory and bundle. The active decision is **pending**, with no reviewer,
  acceptance time, approved artifact pins or conditions copied to the new
  product. The original identity, timestamp, restrictions and acceptance remain
  intact in the archive. Issue #103 needs explicit authorization of the new
  candidate/report, inventory/bundle, compact pair and publication channels.
- `approval-inventory-v1.toml`, the current artifact inventory and
  `review-bundle-v1.toml`: regenerated technical review
  inputs, not approvals. Renew their checksum-bound review. Original documentary revisions remain archived with their former hashes.
- `release-candidate-gates-v1.json`: its recorded hosted CI run evaluated the
  former revision. Updated metadata does not establish a new hosted run;
  execute the existing CI gates on this PR's new head before release.
- `issue-207-external-cross-validation-v1.json`, combined-band validation,
  full-sky evidence and literature-validation outputs: their numerical results
  are retained because every scientific value is unchanged. External run IDs,
  commits and external result hashes remain original. Updated product pins
  are a documented format-only carry-forward, not a claim of rerunning the
  external validator. Renew checksum-bound validation evidence before release.
- The rights assessment's recorded acceptance and the archived notices apply
  to the previous product. Current notices retain the source terms and flag
  the pending renewal; no new legal or maintainer acceptance is asserted.

The embedded registry and four-component runtime defaults remain configured
for scientific equivalence checks. That technical configuration does not grant
permission to release replacement bytes. Existing promotion/release admission
rejects the pending decision. The existing
`production_starlight_requires_verified_human_promotion` test must remain
blocked until the renewed decision is supplied; do not weaken or skip it.
No new version-specific tests, scripts or workflow gates are introduced.
