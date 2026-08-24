# Starlight release-candidate bundle and promotion mechanism (#89)

Status: **Production-ready release candidate pending final human approval.**
Audience: Maintainers running or reviewing `nsb-data dataset starlight
promote`, and the human reviewers recording decisions for issue #47.
Scope: The `nsb-starlight-release-candidate-v1` schema, the paired review
decision templates, and the fail-closed promotion command that consumes
them.
Non-goals: This directory does not approve, regenerate, or redistribute the
candidate map. It only prepares the mechanism so that promotion can happen
automatically and fail-closed once #47 records valid human evidence.

## Files

| File | Role |
|---|---|
| `release-candidate-v1.toml` | Frozen candidate identity (checksum, schema, band, units, resolution, Gaia release, model versions) plus the fail-closed gate table (`gates.validation_status`, `gates.scientific_review_status`, `gates.redistribution_review_status`, `gates.promotion_eligible`). |
| `scientific-review-decision-v1.json` | Template for the human scientific decision owned by #47. Currently `"decision": "pending"`. |
| `redistribution-review-decision-v1.json` | Template for the human redistribution decision owned by #47. Currently `"decision": "pending"`. |

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

notes = "<free text; must document any invalidation or regeneration dependency>"
```

`deny_unknown_fields` applies to every table (see
`crates/nsb-data-tools/src/starlight/promotion.rs`). `gates.promotion_eligible`
is the authoritative kill switch: it is set to `true` only by a maintainer
after #47 records both decisions, never by the promote command itself.

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
   Human `pending` decisions and `promotion_eligible = false` also fail closed.
5. Requires both decisions to be `approved` (or `approved_with_conditions`
   with at least one recorded condition), each with a non-placeholder
   reviewer name, reviewer role, RFC 3339 review timestamp, and a
   `candidate_sha256` pin that matches the release candidate exactly.
6. Requires `gates.scientific_review_status` and
   `gates.redistribution_review_status` to agree with the decisions'
   `decision` fields, and requires `gates.promotion_eligible == true` as the
   final, independent kill switch.
7. Only if every check above passes does it render a **draft** production
   `manifest.toml` fragment (new `nsb-healpix-starlight-v2` map entry plus
   `nsb-starlight-runtime-manifest-v1` sidecar entry, both
   `calibration_status = "production"` and `runtime_embedded = true`) to
   `--output` (or stdout). It never writes to `crates/nsb/data/manifest.toml`
   or to the map bytes; a maintainer applies the draft by hand as part of the
   #47 promotion PR.

Any failure — pending or rejected decision, wrong or tampered checksum,
missing reviewer identity, mismatched candidate pin, or
`promotion_eligible = false` — exits non-zero with a specific message and
writes nothing. See
`crates/nsb-data-tools/src/starlight/promotion.rs` for the fail-closed test
matrix, exercised only against clearly synthetic fixtures.

## Runtime gate (already enforced on `main`)

`crates/nsb::StarlightModel::BundledProductionGaiaDr3` and
`ComponentMask::ALL` already implement the "implemented but disabled while
pending" contract required by #89:

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

This directory's promotion command is what will, after #47 approval, cause a
maintainer to register that production pair — at which point the existing
runtime gate opens automatically, with no further runtime code changes
required.

## Related issues

- #47 — final human validation and production approval (owns both decision
  files and `gates.promotion_eligible`)
- #89 — this mechanism (technical scope closed by this bundle + command)
- #94 / #95 — uncertainty-scale audit that currently blocks
  `candidate.status` from becoming `"pinned"`
