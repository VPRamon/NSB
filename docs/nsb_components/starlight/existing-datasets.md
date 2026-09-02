# Provenance of existing starlight datasets

Status: Current provenance record.
Audience: Maintainers, scientific reviewers, and users auditing bundled data.

## Active Gaia DR3 candidate

The current dataset version publishes exactly one Gaia-derived candidate map
(UV-v2, Ladon production run):

| Artifact | Role | SHA-256 |
| --- | --- | --- |
| `starlight_nside128.csv` | Canonical source-level Gaia accumulation, 300–650 nm | `76191c8b682d96adfc3a017f44f3fcfd0bec5dcb9a958d31668250b8a0ba396a` |
| `merge_report.json` | Map, population, policy, checksum, and deterministic merge evidence | `3f003afb6dcae09eaf917c5a3cbd0fc2fd113a331164fb0509d14c82bb76c5f9` |

Schema `nsb-healpix-starlight-candidate-v5`, nside 128 NESTED sparse, UV model
`calspec-linear-log-ratio-v2`. Photometric-inference and selection-function
artifacts are pinned in `starlight-production-300-650.ladon.toml` and remain
off-git. The candidate stays `calibration_status = "candidate"` and
`runtime_embedded = false` until issue #103 signs and the promotion workflow
registers the packed runtime map.

Full-sky production diagnostics frozen for #103 review live in
`docs/nsb_components/starlight/release-candidate/fullsky-production-evidence-v1.json`.
That file is intentionally outside the checksum-pinned `review-bundle-v1.toml`.

### Historical note

Earlier nside-128 candidates without the UV-v2 correction, derived nside 64/256/512
maps, and pre-fix HEALPix frame bugs are superseded. Checksums, investigation
reports, and intermediate evidence are preserved in Git history and closed
issues/PRs (#72, #74, #76, #94, #116).

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
