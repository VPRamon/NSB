# Independent Starlight validation (issue #87)

Status: Technical scaffolding. No reference has been acquired yet; no
candidate checksum has been scientifically approved.
Audience: Maintainers preparing evidence for the human review in #47.
Scope: Acquiring checksum-pinned external references and comparing them
against the integrated 300-650 nm Starlight candidate map.

## Why this exists

The XP holdout (`crates/nsb-data-tools/src/starlight/uv`) validates spectral
reconstruction against a held-out slice of the same underlying Gaia data. It
cannot detect a bug or systematic that is shared between the production
pipeline and its own holdout. Issue #87 asks for tooling that is independent
end to end: its own candidate-map reader, its own HEALPix pixel geometry, and
comparisons against external literature references that never touched the
Gaia-based production pipeline (or, in one case, used Gaia data through a
completely different, independently published pipeline; see
`references-v1.toml`).

Nothing in this pipeline may ever set `scientifically_validated = true` or
move `scientific_review_status` away from `"pending"`. Those decisions belong
exclusively to a qualified human scientist recorded in issue #47.

## The frozen documents

- [`preregistration-v1.toml`](preregistration-v1.toml) — the exact tolerances
  from issue #87, the pinned candidate map path, and the metric vocabulary.
  Frozen before any reference is compared, so thresholds cannot be tuned
  after seeing results.
- [`references-v1.toml`](references-v1.toml) — the reference registry. Every
  entry starts `status = "pending-acquisition"` with no `sha256`; acquisition
  is required before any of them can be used, and no checksum in this
  repository is ever invented.
- [`regions-v1.json`](regions-v1.json) — reproducible sky-region formulas
  (latitude/longitude bands, cones, cone unions, and candidate-map-driven
  percentile selectors) evaluated fresh by `RegionEngine` against whichever
  candidate map and `nside` are actually supplied to `run`.
- [`scientific-review-decision-v1.json`](scientific-review-decision-v1.json)
  — the pending human-decision template. It is never filled in by this
  pipeline; only a human, working from issue #47, edits a copy of it.

## Workflow

### 1. Acquire references

```sh
nsb-data dataset starlight validation acquire \
  --references docs/nsb_components/starlight/validation/references-v1.toml \
  --workspace path/to/validation-workspace
```

For each reference, this either copies a local file or performs a resumable
HTTP download, verifies the file's SHA-256 (when the registry declares one),
and writes a receipt into `--workspace`. It fails closed: a checksum mismatch
aborts with an error and writes no receipt, and it never fabricates a
verified state.

Every reference currently in `references-v1.toml` requires acquisition
before it has a URL or local path; run with `--source <id>=<path-or-url>` to
supply one, e.g.:

```sh
nsb-data dataset starlight validation acquire \
  --references docs/nsb_components/starlight/validation/references-v1.toml \
  --workspace path/to/validation-workspace \
  --source leinert-1998-diffuse-night-sky-brightness=path/to/downloaded/table.csv
```

Acquiring a file only proves the *bytes* match a declared checksum. It does
not, by itself, produce a comparable grid: each reference's raw data still
needs a documented physical transformation (see `transformation_to_target`
in `references-v1.toml`) into the 300-650 nm integrated photon-radiance
convention, nside=128, NESTED HEALPix grid the candidate map uses. That
transformation step is out of scope for this first PR (see "What's still
missing" below); its output format is the `transformed-grid-v1.csv` file
`run` looks for under `<references-workspace>/<reference-id>/`.

### 2. Run the comparison

```sh
nsb-data dataset starlight validation run \
  --preregistration docs/nsb_components/starlight/validation/preregistration-v1.toml \
  --references docs/nsb_components/starlight/validation/references-v1.toml \
  --regions docs/nsb_components/starlight/validation/regions-v1.json \
  --candidate-map crates/nsb/data/starlight_nside128.csv \
  --candidate-map-sha256 <expected-sha256> \
  --references-workspace path/to/validation-workspace \
  --output path/to/validation-output
```

`run` independently re-reads and re-checksums the candidate map (its own
minimal reader, not the production writer's), resolves every region against
the candidate map's own `nside`, and, for each reference that has both an
acquisition receipt and a `transformed-grid-v1.csv`, computes the full metric
vocabulary from `preregistration-v1.toml` (signed/absolute/relative bias,
MAE, median absolute error, RMSE, relative-error percentiles, coverage, and
outlier fraction) for all-sky and every region.

It writes three artifacts under `--output`:

- `validation-results-v1.json` — machine-readable results, always including
  `"scientifically_validated": false` and `"scientific_review_status":
  "pending"`.
- `validation-report-v1.md` — the same results rendered as a short
  human-readable report.
- `validation-artifact-manifest-v1.toml` — a SHA-256 manifest of every input
  and output artifact for this invocation, recomputed independently rather
  than trusted from elsewhere.

If a reference is acquired but not admissible (Toller Pioneer poles; GAMBONS),
`run` records `not-admissible` and does not invent comparison numbers. The
Leinert 1998 ISL analytic model is the admissible comparison; preregistered
gates may still fail, and that failure is reported rather than retuned.

HTML and Markdown reports are both written (`validation-report-v1.html` and
`.md`).

## Independent validation status for the UV v2 candidate

Results against map `5946fa170b1be911b8996ac4a36200133743bac6ba39a1392358cd3007a91563`
are stored in [`results/`](results/). All three acquired references are
**not admissible** as starlight-only TOA 300–650 nm grids. `technical_gates_passed = false`.
`scientifically_validated` remains false. Human review stays in #47.

## `scientific_review_status` stays `"pending"` until #47

This pipeline produces *technical* evidence only: reproducible acquisition,
frozen regions, computed metrics, and automatic gate evaluation against
preregistered tolerances. Whether a specific candidate checksum is fit for
production use is a scientific judgment made by a qualified human, recorded
by hand in a copy of `scientific-review-decision-v1.json`, tracked in issue
#47. No command in this pipeline, and no future automation built on top of
it, should ever flip `scientific_review_status` to anything other than
`"pending"` or set `scientifically_validated = true`.

## What's still missing after the technical #87 package

- **Human scientific decision.** `scientific_review_status` stays `"pending"`
  in issue #47. Independent validation of the UV v2 candidate versus the
  Leinert 1998 ISL model failed the preregistered numerical gates; see
  [`results/`](results/). Do not retune those gates to force a pass.
- Toller Pioneer, Leinert 1998, and GAMBONS remain acquired-but-not-admissible
  (DGL/ZL/airglow inseparable, or unpublished Gaussian parameters).
