# Provenance of existing starlight datasets

Status: Historical provenance record.
Audience: Maintainers, scientific reviewers, and users auditing bundled data.

## Active Gaia DR3 candidate

The current dataset version publishes exactly one Gaia-derived candidate map
(UV-v2, Ladon production run):

| Artifact | Role | SHA-256 |
| --- | --- | --- |
| `starlight_nside128.csv` | Canonical source-level Gaia accumulation, 300–650 nm | `b17124d057faad2445575239c04928514d2846ec36a2f5df7137566058d85154` |
| `merge_report.json` | Map, population, policy, checksum, and deterministic merge evidence | `52ca4a9d30c82f5d76532bbeccb9c829f6cf60ae1364ee9b9982683c54820c43` |

Schema `nsb-healpix-starlight-candidate-v5`, nside 128 NESTED sparse, UV model
`calspec-linear-log-ratio-v2`. Photometric-inference and selection-function
artifacts are pinned in `starlight-production-300-650.ladon.toml` and remain
off-git. The candidate stays `calibration_status = "candidate"` and
`runtime_embedded = false` until issue #103 signs and the promotion workflow
registers the packed runtime map.

### Historical nside-128 candidate (superseded)

The previous published candidate used SHA-256
`4080ad95a057dda68ca89e48cdd32583829fc0ee2d58ba1398a6bd875fa70657` (merge
`333ec450a9f38bb59e7cd832a622a66082962de51e90e65eaf9699529b2044e0`). That
map integrated 336–650 nm without the UV-v2 correction.

The nside-128 scientific rows are identical to the artifact first published by
commit `6e515a6e7dc01b37594a765021d415fd5f7e768a`. PR #77 added the v2 physical
metadata. Issue #74 adds only the v3 sparse-representation headers and report
cardinality fields; the Gaia production pipeline was not rerun for either
metadata-only migration. The sparse file contains 196,604 strictly ordered
rows in a 196,608-pixel domain. Its four omitted pixels have zero integrated
flux and zero admitted/excluded source counts by contract.

The retained report records 219,197,642 observed sources, 219,109,593 admitted
sources, and 88,049 `invalid_flux` exclusions. Flux was integrated directly
over 336–650 nm. No validated selection-function correction, faint-tail
correction, or independently calibrated 300–336 nm correction was applied.
The candidate remains `calibration_status = "candidate"` and
`runtime_embedded = false`.

The original lifecycle `run.json`, `validation.json`, normalized inventories,
receipts, exact command, shard set, and site-local workspace were not retained.
The checked-in v4 report therefore has integrity and explicit science-policy
evidence but only the historical single-pixel deterministic reference. It is
not retroactively presented as complete-map deterministic evidence. New clean
runs use shard schema v3 and report schema v6, compare every pixel accumulator
and exclusion counter, and retain equal dataset-wide digests before publication.

## Retired derived artifacts

The original publication also included three maps derived from nside 128:

| Retired artifact | Historical role | Historical SHA-256 |
| --- | --- | --- |
| `starlight_nside64.csv` | NESTED downsample | `1cba5f154a801605d93f35501426c86e40bc120b620dc96f7f4372ff1ded3003` |
| `starlight_nside256.csv` | Diagnostic nearest-neighbour upsample | `4c7d437994b7105415973b0e99ebb09812798323b13b7ae8952f6674685c8fad` |
| `starlight_nside512.csv` | Diagnostic higher-grid upsample | `7da040ea844969f44062eb76c172e2df7f75645d3d98006931968be7bbbb53e8` |

These files are no longer active assets. Nside 256 and 512 contained no
independent source localization or scientific resolution, and the historical
implementation copied integrated parent-pixel flux into every child. Issue #76
superseded the repair direction in #72: derived maps are retired instead of
being repaired and republished. The nside-512 Git LFS object remains only in
repository history; normal checkouts and scientific CI no longer require it.

Future nside 256 or 512 candidates must start from Gaia source-level
contributions in a clean run. A resolution-selection study must compare such
independently generated candidates and publish only the selected map.

## Supported regeneration procedure

Use `crates/nsb-data-tools/config/starlight-production.toml` and the documented
`update → build → validate → publish` lifecycle from a fresh workspace. A new
candidate must retain its run manifest, validation report, generator commit,
configuration checksum, normalized inventory checksums, acquisition receipt
root, exact commands, artifact checksums, independent comparison evidence, and
redistribution decision.

The historical manual experimental seed CSV has been removed from the runtime
data tree; it is not a Gaia-derived candidate and cannot be restored by
repackaging candidate bytes.
