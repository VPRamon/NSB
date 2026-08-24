# Starlight documentation wording audit (v1)

Status: One-time audit for issue #90, retained as historical evidence.
The current remaining production blocker is issue #103 (human scientific and
redistribution approval). Issue #47 is not the current final Starlight gate.

Scope: `docs/nsb_components/starlight/**/*.md` and Starlight source under
`crates/nsb-data-tools/src/starlight/**/*.rs` and
`crates/nsb/src/components/starlight/**/*.rs`.

## Method

Every occurrence of the terms below was located and read in context, then
classified as one of:

- **correct** — accurately describes current, intentional behaviour or status.
- **historical** — accurately describes a past state and is clearly scoped as
  such (e.g. a retired artifact or a superseded schema version).
- **obsolete** — describes a past state as if it were current; drifted from
  the code and should be corrected.
- **blocking** — would mislead a reviewer approving production use in #47 if
  left uncorrected.

Reproduce the raw scan with:

```bash
rg -n -i "candidate|experimental|not production ready|runtime_embedded\s*=\s*false|manual seed|placeholder|TODO" \
  docs/nsb_components/starlight crates/nsb-data-tools/src/starlight crates/nsb/src/components/starlight \
  -g '*.md' -g '*.rs'
```

This produced 98 matching lines across 16 files at the time of this audit
(commit `bb70bb56c34d24309e2baf1af7e69e2aeea2f434`, before this branch).
`not production ready` and `runtime_embedded=false` (as a literal token) had
zero matches; `runtime_embedded = false` (with spaces, as Rust/TOML render it)
is covered below. Findings are grouped by file/topic rather than listed
per line, since a single classification usually applies to every occurrence in
a file.

## Findings

### `candidate` / `experimental` — correct, no action

These are the dominant matches (52 and 46 raw hits respectively) and almost
all are **correct**: the whole point of #47/#87/#89/#90 is that the Gaia DR3
map is a `candidate`, not `production`, and `experimental-starlight` /
`experimental_seed_model` are real, intentionally-named API surfaces for the
bundled 12-pixel manual seed. Reviewed and confirmed correct:

- `docs/nsb_components/starlight/README.md`
- `docs/nsb_components/starlight/map-generation.md`
- `docs/nsb_components/starlight/external-manifest.md`
- `crates/nsb/src/components/starlight/model.rs`,
  `provenance.rs`, `tests.rs`, `validated.rs`
- `crates/nsb-data-tools/src/starlight/config.rs`,
  `selection.rs`, `uv.rs`, `sources/inventory.rs`,
  `map/product.rs` (`Candidate` variant name, doc comments)

`in_qso_candidates` / `in_galaxy_candidates` in
`crates/nsb-data-tools/src/starlight/worker.rs` are unrelated Gaia catalogue
column names, not references to map maturity. **Correct**, false-positive
match on the word "candidate".

### `runtime_embedded = false` — correct

`docs/nsb_components/starlight/existing-datasets.md:28`,
`docs/nsb_components/starlight/map-validation.md:30`, and
`crates/nsb/data/manifest.toml` agree with each other and with the checked-in
manifest (`runtime_embedded = false` for both starlight assets). **Correct**
and consistent.

### `manual seed` / `placeholder` / `TODO` — correct, test-only, or intentional guard text

- `starlight_manual_seed_v1.csv` and its provenance strings
  (`docs/nsb_components/starlight/existing-datasets.md`,
  `docs/nsb_components/starlight/README.md`,
  `docs/nsb_components/starlight/map-generation.md`,
  `crates/nsb/src/components/starlight/model.rs`) are **correct**: it is a
  genuinely separate, intentionally experimental nside-1 asset.
- `reject_placeholder(...)` in `crates/nsb/src/components/starlight/validated.rs`
  and the `"placeholder", "todo", "tbd", ...` literal lists in
  `crates/nsb-data-tools/src/starlight/{photometric,selection,uv}.rs` are
  **correct**: fail-closed guards that reject placeholder text in provenance
  fields, not placeholder text left behind in the codebase itself.
- `"# Versioned no-op placeholder for the not-yet-calibrated Gaia selection
  model."` in `crates/nsb-data-tools/src/starlight/map/product.rs:112` is
  **correct**: it documents an intentional, versioned no-op policy value, not
  unfinished work.
- `bad.training_command = "TODO".to_string();` in
  `crates/nsb-data-tools/src/starlight/selection.rs:661` is **correct**: it is
  a unit-test fixture value used to assert that a literal `"TODO"` is rejected
  as a placeholder, not a real TODO comment.
- `"# schema=nsb-healpix-starlight-candidate-v3\n"` in
  `crates/nsb-data-tools/src/starlight/map/mod.rs:19` is **correct**: an
  arbitrary schema string used only to exercise rejection of an incompatible
  `representation=full-sky` header; the test does not assert anything about
  schema-version currency.

No literal `TODO`/`FIXME` markers describing unfinished production work were
found in Starlight source.

### `docs/nsb_components/starlight/map-validation.md` schema versions — **obsolete, fixed in this PR**

Three version numbers in this file had drifted from the code and were fixed
directly as part of this audit (all three are objective, code-verifiable
facts, not scientific judgement calls):

| Line (before) | Said | Actual (source of truth) | Fixed to |
|---|---|---|---|
| "Candidate schema `nsb-healpix-starlight-candidate-v3` requires:" | v3 | `MAP_SCHEMA` in `crates/nsb-data-tools/src/starlight/map/product.rs` = `"nsb-healpix-starlight-candidate-v5"` | v5 |
| "Report schema v5 declares one `canonical_map`..." | v5 | `REPORT_SCHEMA_VERSION` in the same file = `6` | v6 |
| "Starlight shard schema v2 stores flux and uncertainty..." | v2 | `SHARD_SCHEMA_VERSION` in `crates/nsb-data-tools/src/starlight/map/accumulator.rs` = `3` | v3 |

`docs/nsb_components/starlight/existing-datasets.md:35` ("New clean runs use
shard schema v2 and report schema v5...") had the same drift and was fixed to
v3/v6 for consistency.

### `docs/nsb_components/starlight/science-requirements.md:232` — **blocking, not fixed here**

> "Until then, the existing manual seed and the 336--650 nm XP-sampled map
> remain experimental or candidate products, and Starlight remains outside
> `ComponentMask::ALL`."

At the time this sentence was written the best available Gaia candidate only
covered 336–650 nm (Gaia XP spectra start at 336 nm; see the same file,
line 33). The current frozen candidate
(`crates/nsb/data/starlight_nside128.csv`, see
`docs/nsb_components/starlight/production-runs/combined-300-650-validation.json`,
`declared-science-policy` gate) already applies the UV 300–336 nm correction
and is a **300–650 nm** product — the "336–650 nm XP-sampled map" description
is stale. This is classified **blocking** rather than fixed in this PR because
correcting it requires a scientific-content judgement call (this document is
the normative science contract, and its "no currently bundled asset satisfies
this contract" framing may be intentionally general), not just a mechanical
version bump. **Recommendation:** whoever regenerates the candidate under #94
should re-read and, if still accurate, correct this paragraph as part of that
PR, since #94 already requires updating the freeze table in #47.

## Summary

| Classification | Count (approx., by topic not raw line) | Action |
|---|---:|---|
| correct | ~14 topics / ~90 lines | none |
| historical | 0 additional topics beyond the correct/retired-artifact cases above | none |
| obsolete | 2 files (4 version numbers) | fixed in this PR |
| blocking | 1 (science-requirements.md band description) | flagged for #94 follow-up, not fixed here |

No occurrences of "not production ready" (as a literal phrase) or
`runtime_embedded=false` (compact form) were found. No unresolved `TODO`/`FIXME`
markers were found in Starlight documentation or source.
