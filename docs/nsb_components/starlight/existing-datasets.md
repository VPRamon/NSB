# Provenance of existing starlight datasets

Status: Historical provenance record.
Audience: Maintainers, scientific reviewers, and users auditing bundled data.

## Active Gaia DR3 candidate

The current dataset version publishes exactly one Gaia-derived candidate map:

| Artifact | Role | SHA-256 |
| --- | --- | --- |
| `starlight_nside128.csv` | Canonical source-level Gaia accumulation | `ab9ed8db9c81d35887642ae7453e3fea69a4f2ebfa475662edc758133d01ffda` |
| `merge_report.json` | Singular map, population, policy, checksum, and deterministic-merge evidence | `9a09a9be25b6fef472eb53bc36fd7567f76775504859c133c9278ea36f14b371` |

The nside-128 scientific rows are identical to the artifact first published by
commit `6e515a6e7dc01b37594a765021d415fd5f7e768a`. PR #77 adds explicit v2
metadata headers, so its byte checksum changes from
`09ca9bd57407beab49ff26cf1fe8ab305ccf9394e244563ee833b059a2287d35`.
The Gaia production pipeline was not rerun for this metadata-only migration.

The retained report records 219,197,642 observed sources, 219,109,593 admitted
sources, and 88,049 `invalid_flux` exclusions. Flux was integrated directly
over 336–650 nm. No validated selection-function correction, faint-tail
correction, or independently calibrated 300–336 nm correction was applied.
The candidate remains `calibration_status = "candidate"` and
`runtime_embedded = false`.

The original lifecycle `run.json`, `validation.json`, normalized inventories,
receipts, exact command, and site-local workspace were not retained. The
checked-in candidate therefore has integrity and explicit science-policy
evidence but not a self-contained byte-for-byte reproduction bundle.

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

## Experimental manual seed

`starlight_manual_seed_v1.csv` remains a separate nside-1 experimental asset
with SHA-256
`a18c41ceeaaaf343e6991d6a718b6edf0b8cbfc46faf1cfaf7551c3d1c434668`.
It is not the Gaia-derived candidate and cannot be promoted by repackaging the
same bytes.
