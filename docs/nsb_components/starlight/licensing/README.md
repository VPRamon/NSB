# Starlight redistribution and licensing package

Status: technical scaffolding for #88 (redistribution and attribution
review). Human approval is intentionally not recorded here.

## What lives in this folder

| File | Purpose |
| --- | --- |
| [`artifact-inventory-v1.toml`](artifact-inventory-v1.toml) | Every upstream input, generated artifact, sidecar, manifest, and report that participates in the Starlight candidate, with source, release, licence, checksum (when known), distribution class, and current distribution status. |
| [`ATTRIBUTION.md`](ATTRIBUTION.md) | Attribution wording for Gaia DR3, Cantat-Gaudin, CALSPEC, and GaiaXPy, plus how NSB-generated artifacts are licensed. |
| [`redistribution-review-decision-v1.json`](redistribution-review-decision-v1.json) | Decision schema template. Ships with `decision = "pending"` and no reviewer. |

The consolidated, project-wide notice this folder feeds into is
[`THIRD_PARTY_NOTICES.md`](../../../../THIRD_PARTY_NOTICES.md) at the
repository root.

The fail-closed Rust contract that parses and cross-validates the inventory
and decision files lives in
[`crates/nsb-data-tools/src/starlight/licensing.rs`](../../../../crates/nsb-data-tools/src/starlight/licensing.rs).
It is intended for a future promotion workflow (#89); it is not wired into
any CLI subcommand yet.

## What this package does *not* do

This package inventories facts and enforces internal consistency. It does
not, and cannot, authorize redistribution. Specifically, it does not:

- decide whether the Gaia data licence (CC BY-NC 3.0 IGO) non-commercial
  clause permits NSB's intended release channels for a Gaia-derived
  candidate map;
- select or approve independent validation evidence for #87;
- fill in `reviewer_name`, `reviewer_role`, `reviewed_at_utc`, or change
  `decision` away from `"pending"` in the decision template;
- claim that any artifact currently marked `distributed = true` in the
  inventory has completed redistribution review. Several already-embedded
  repository artifacts (the candidate map, its merge report, and the
  validation report) are flagged in the inventory as pending exactly this
  review.

## Where the human decision lives

**The human decision stays in #47.** Per that issue's structure, the
technical package (#88, this folder, closed by an authorized human process)
and the human sign-off (#47, the single final gate for Starlight production)
are deliberately separate. An authorized human reviewer — the project owner
or an authorized reviewer, not a software agent — must:

1. review the inventory, licences, attributions, distributed outputs,
   channels, notices, and restrictions recorded here;
2. copy `redistribution-review-decision-v1.json`, recompute
   `inventory_sha256` against the exact inventory bytes under review, and
   set `decision`, `reviewer_name`, `reviewer_role`, `reviewed_at_utc`, and
   the pinned per-artifact checksums/channels;
3. commit or attach the signed decision JSON on the promotion branch or
   workflow inputs referenced by #47.

No software agent, including the one that produced this package, may set
`decision` to anything other than `"pending"`.

## Known gaps blocking full #88 closure

- No independent validation evidence exists yet (#87); the inventory carries
  only a placeholder entry.
- No production selection-function or photometric-inference artifact has
  been trained or checked in; both remain unset in
  `starlight-production.toml`.
- The UV correction artifact embedded in the current candidate
  (`calspec-linear-log-ratio-v1`) is scientifically invalidated per #94; its
  replacement (`v2`) is not yet integrated into a published candidate, so the
  candidate map/report checksums in the inventory will need to be replaced
  after the Ladon rebuild.
- The Gaia CC BY-NC 3.0 IGO non-commercial clause's compatibility with NSB's
  intended distribution channels has not been legally or scientifically
  determined; this is the central open question for #47.
- `starlight_manual_seed_v1.csv` already carries a pre-existing
  "review required" licence flag in `crates/nsb/data/manifest.toml` that this
  package surfaces but does not resolve.
