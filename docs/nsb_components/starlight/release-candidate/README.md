# Starlight release-candidate bundle and promotion mechanism (#102)

Status: Current fail-closed bundle for the frozen UV-v2 candidate.
Audience: Maintainers running `nsb-data dataset starlight promote`,
GitHub Actions `starlight-final-promotion.yml`, and human reviewers on
issue #103.
Scope: Checksum-pinned candidate, packed runtime contract, and post-approval
automation. Human scientific and redistribution approval stay on #103.
Non-goals: This directory does not approve, regenerate, or rewrite the
candidate map. Promotion after valid #103 decisions is automated by
`.github/workflows/starlight-final-promotion.yml`.

## Files

| File | Role |
|---|---|
| `release-candidate-v1.toml` | Frozen candidate identity (checksum, schema, band, units, resolution, Gaia release, model versions) plus the fail-closed gate table (`gates.validation_status`, `gates.scientific_review_status`, `gates.redistribution_review_status`, `gates.promotion_eligible`). |
| `scientific-review-decision-v1.json` | Template for the human scientific decision owned by #103. Currently `"decision": "pending"`. |
| `redistribution-review-decision-v1.json` | Template for the human redistribution decision owned by #103. Currently `"decision": "pending"`. |

## The `nsb-starlight-release-candidate-v1` schema

```toml
schema_version = 1
schema = "nsb-starlight-release-candidate-v1"

[candidate]
status = "pinned" # or "awaiting_regeneration"
candidate_sha256 = "<64-hex sha256 of the exact candidate map bytes>"
map_path = "<repository-relative path to the candidate map>"
map_schema = "<map schema id, e.g. nsb-healpix-starlight-candidate-v5>"
band = "<passband definition>"
units = "<flux unit>"
nside = 128
ordering = "nested" # or "ring"
gaia_release = "<Gaia data release>"

[candidate.model_versions]
# free-form key/value pairs identifying every model/artifact version that
# produced the candidate (UV correction, photometry model, etc.)

[gates]
validation_status = "technical_pass" # or "pending_regeneration"
scientific_review_status = "pending" # | "approved" | "approved_with_conditions" | "rejected"
redistribution_review_status = "pending" # | "approved" | "approved_with_conditions" | "rejected"
promotion_eligible = false

[review_artifacts]
inventory_path = "docs/nsb_components/starlight/licensing/artifact-inventory-v1.toml"
inventory_sha256 = "<sha256 of that inventory file>"
gates_report_path = "docs/nsb_components/starlight/production-runs/release-candidate-gates-v1.json"
gates_report_sha256 = "<sha256 of that gates report>"
licensing_decision_path = "docs/nsb_components/starlight/licensing/redistribution-review-decision-v1.json"

notes = "<free text; must document any invalidation or regeneration dependency>"
```

`deny_unknown_fields` applies to every table (see
`crates/nsb-data-tools/src/starlight/promotion.rs`). `gates.promotion_eligible`
is report-only. Eligibility is derived from frozen CI gates, packed runtime
verification, and the two signed human decisions owned by issue #103.

## The `dataset starlight promote` command

```bash
nsb-data dataset starlight promote \
  --release-candidate docs/nsb_components/starlight/release-candidate/release-candidate-v1.toml \
  --scientific-decision docs/nsb_components/starlight/release-candidate/scientific-review-decision-v1.json \
  --redistribution-decision docs/nsb_components/starlight/release-candidate/redistribution-review-decision-v1.json \
  --repository-root . \
  --output target/starlight-promotion/production-manifest-draft.toml
```

The command:

1. Parses and structurally validates the release-candidate manifest and both
   decision files (schema versions, required non-placeholder fields).
2. Recomputes the SHA-256 of the map file at `candidate.map_path` (resolved
   under `--repository-root`) and requires it to match `candidate_sha256`
   byte-for-byte.
3. Cross-checks the repository's `crates/nsb/data/manifest.toml` registry
   entry for that path against the release candidate's pinned schema and
   checksum, to catch registry/candidate drift or tampering.
4. Requires `candidate.status == "pinned"` and
   `gates.validation_status == "technical_pass"`; otherwise it fails closed.
   It also checksum-verifies the frozen `release-candidate-gates-v1.json`
   (`passed = true`, `commit_sha` set, required jobs including `cargo deny`
   executed). It packs a runtime RING HEALPix map from the candidate-v5
   file without rewriting candidate bytes, and runs
   `RedistributionReview::require_approved` on the pinned inventory +
   licensing decision. Human `pending` decisions fail closed.
5. Requires both decisions to be `approved` (or `approved_with_conditions`
   with at least one recorded condition), each with a non-placeholder
   reviewer name, reviewer role, RFC 3339 review timestamp, and a
   `candidate_sha256` pin that matches the release candidate exactly.
6. TOML `scientific_review_status` / `redistribution_review_status` /
   `promotion_eligible` fields are not a second kill switch; signed
   decision files are authoritative.
7. Only if every check above passes does it render a **draft** production
   `manifest.toml` fragment (new packed `nsb-healpix-starlight-v2` map entry
   plus runtime sidecar, both `calibration_status = "production"` and
   `runtime_embedded = true`) to `--output` (or stdout). Pass `--apply` to
   write packed assets and registry entries. Candidate map bytes are never
   rewritten. `.github/workflows/starlight-final-promotion.yml` opens the
   promotion PR after those steps and a re-run of the required gate matrix.

Any failure — pending or rejected decision, wrong or tampered checksum,
missing reviewer identity, or mismatched candidate pin — exits non-zero
with a specific message and writes nothing. See
`crates/nsb-data-tools/src/starlight/promotion.rs` for the fail-closed test
matrix, exercised only against clearly synthetic fixtures.

## Runtime gate (already enforced on `main`)

`crates/nsb::StarlightModel::BundledProductionGaiaDr3` and
`ComponentMask::ALL` already implement the fail-closed production gate:

- `Starlight::bundled_production_model()` only succeeds when
  `crates/nsb/build.rs` finds a registered `nsb-healpix-starlight-v2` +
  `nsb-starlight-runtime-manifest-v1` production pair in
  `crates/nsb/data/manifest.toml` (see
  `crates/nsb/src/components/starlight/model.rs`); otherwise it returns
  `NsbError::DataMissing`, never a silent fallback to the experimental seed.
- `ComponentMask::ALL` (and its `DEFAULT` alias) includes `STARLIGHT` only
  under `cfg(nsb_bundled_production_starlight)`, which is not set while no
  production pair is registered.
- `crates/nsb-cli`'s `--components starlight` production selection always
  calls `StarlightModel::bundled_production_gaia_dr3()` explicitly; there is
  no code path that substitutes the experimental seed when production is
  unavailable.

The final-promotion workflow, after valid #103 signatures, registers that
production pair. The runtime gate then opens with no further runtime code
changes.

## Related issues

- #103 — final human scientific and redistribution approval (owns both
  decision files)
- #102 — technical packing, eligibility derivation, and promotion
  automation (this bundle)
- #94 — historical uncertainty-scale invalidation of the #93 candidate;
  the UV-v2 candidate is already pinned
