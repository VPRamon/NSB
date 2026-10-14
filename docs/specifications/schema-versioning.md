# Schema versions and the first public release

Status: Pre-0.1.0 public compatibility baseline and scientific evidence policy.
Audience: NSB maintainers, contributors, and downstream schema consumers.

NSB `0.1.0` establishes the first **public** compatibility baseline for
NSB-owned interfaces. Historical, unpublished development iterations do not
oblige users to handle deprecated schema formats.

## Distinct version identities

- **Crate/package version**: `nsb` `0.1.0` (and the corresponding Python
  distribution) follows the published package release lifecycle.
- **Public serialization contract**: the CLI point/window JSON and CSV
  identifiers start at `v1`. The uncertainty-bearing point CSV layout is
  separately named `nsb-cli-point-csv-uncertainty-v1`. See
  [CLI schemas](cli-schemas.md).
- **Model-composition contract**: `MODEL_VERSION = "nsb-model-v1"` is
  independent of package SemVer. It changes when the documented default model
  composition/meaning changes.
- **Scientific assets and immutable evidence**: data-product format versions,
  model/calibration identities, source release labels, checksums, and approval
  records describe exact products, not the age of the NSB package.

## Reset rule before the first release

A not-yet-published, NSB-owned public schema may be consolidated to `v1`
before tagging `0.1.0`, together with its writers, readers, tests and docs.
There is no migration promise for superseded, unpublished development
contracts. After publication, a breaking schema change requires a new
identifier, compatibility review, and release notes.

The CI and release workflows run `scripts/check-first-release-schemas.py`.
For exactly `0.1.0`, it verifies the five public CLI schema identifiers,
their documentation, and the model-composition identifier. This is intentionally
a first-release gate, not a prohibition on future schema evolution.

## Exception: checksum-pinned Starlight evidence

The frozen Starlight production chain contains **NSB-owned** historical
identifiers that must **not** be normalized by a first-release cleanup:

| Product or contract | Existing identity | Reason to retain |
| --- | --- | --- |
| Frozen combined-band scientific candidate | `nsb-healpix-starlight-candidate-v5` | Candidate bytes and validated source evidence are pinned |
| Frozen merge/provenance report | `nsb-starlight-merge-report-v5` | Cross-validation and review evidence refer to exact report bytes |
| Packed RING runtime CSV | `nsb-healpix-starlight-v2` | Deterministically derived and checksum-pinned |
| Runtime provenance sidecar | `nsb-starlight-runtime-manifest-v2`, `schema_version = 2` | Contains complete pinned provenance, including bright-star inputs |
| Embedded compact binary transport | `nsb-healpix-starlight-binary-v1` | Its own transport format begins at v1 |

The release candidate, runtime-assets inventory, approval decision, production
registry and checksum/validation artifacts are related by exact content hashes.
Renaming an identifier inside them would change bytes and invalidate evidence.
Such changes require a **separate scientific product regeneration and approval
review**, never a cosmetic version reset. See the
[Starlight release-candidate lifecycle](../nsb_components/starlight/release-candidate/README.md).

Keep unrelated third-party versions unchanged as well: examples include
PALACE v1.0, Gaia DR3, CK04 v2, and Siderust dependency versions.

The generic new-run Starlight candidate contract can start at
`nsb-healpix-starlight-candidate-v1` while the **separate frozen product**
retains `candidate-v5`; both are intentional. See
[Starlight map generation](../nsb_components/starlight/map-generation.md).

## Review checklist for new changes

When touching a schema or scientific product, determine whether its identifier
describes a public API contract, an internal non-frozen working format, an
immutable scientific artifact, or an external dependency. Change public
identifiers only as required by their compatibility contract. Never rewrite a
checksum-pinned artifact or its approval chain merely to make all version
numbers match.
