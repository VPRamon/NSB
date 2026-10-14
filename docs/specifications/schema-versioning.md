# Schema versions and the first public release

NSB `0.1.0` establishes `v1` for all NSB-owned schemas and artifact formats,
including the Starlight candidate, merge report, runtime CSV and runtime manifest
(`schema_version = 1`). Unpublished internal iterations have no compatibility
promise. Readers and writers use the current contract directly.

Package SemVer, scientific model identities, input releases and external
dependencies are independent. Preserve Gaia DR3, CK04 v2, PALACE v1.0,
`calspec-linear-log-ratio-v2`, all input hashes and dependency versions.
All active review inventories and bundles also start at v1. Original v1/v2
review snapshots are preserved under approval-history for audit only; historical
records retain the exact identifiers and hashes that were authorized.

Changing format/provenance identifiers changes bytes and SHA-256 even when
scientific values are identical. Regenerate deterministic runtime artifacts and
update dependent hashes. An old approval is never approval of replacement bytes.
See the [Starlight reset record](../nsb_components/starlight/release-candidate/schema-reset-v1.md)
for the original evidence, new identities and required renewed authorization.

Use the existing format, scientific-asset, runtime and release checks. No new
version-specific validation gate is introduced. After the first publication,
breaking schema changes require a new identifier and compatibility review.
