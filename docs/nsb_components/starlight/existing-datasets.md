# Current Starlight candidate provenance

Status: Current provenance record.
Audience: Maintainers, scientific reviewers, and users auditing bundled data.

## Active Gaia DR3 candidate

The first public baseline records one combined Gaia DR3 plus
bright-star-supplement candidate map with the validated UV-correction lineage:

| Artifact | Role | SHA-256 |
| --- | --- | --- |
| `starlight_nside128.csv` | Canonical source-level Gaia accumulation, 300–650 nm with covariance-corrected bright-star supplement | `7e903ff289e76d07c018933b8f97fcf264cead73999912ff63f34b9d1e01b37d` |
| `merge_report.json` | Map, population, policy, checksum, and deterministic merge evidence | `015545ac8214509a5c1ec86d6c8393e05a0d811a1fec346a0cbd405c52840469` |

Schema `nsb-healpix-starlight-candidate-v1`, nside 128 NESTED sparse, UV model
`calspec-linear-log-ratio-v1`. Photometric-inference and selection-function
artifacts are pinned in `starlight-production-300-650.ladon.toml` and remain
off-git. The candidate stays `calibration_status = "candidate"` and
`runtime_embedded = false` until an authorized #103 redistribution decision and
a follow-up production-activation change register the packed runtime map.

Full-sky production diagnostics frozen for #103 review live in
`docs/nsb_components/starlight/release-candidate/fullsky-production-evidence-v1.json`.
That file is intentionally outside the checksum-pinned `review-bundle-v1.toml`.

## Supported regeneration procedure

Use `crates/nsb-data-tools/config/starlight-production.toml` and the documented
`update → build → validate → publish` lifecycle from a fresh workspace. A new
candidate must retain its run manifest, validation report, generator commit,
configuration checksum, normalized inventory checksums, acquisition receipt
root, exact commands, artifact checksums, independent comparison evidence, and
redistribution decision.
