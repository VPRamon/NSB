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

If no reference has both an acquisition receipt and a transformed grid — the
current state, since nothing has been acquired yet — `run` still produces all
three artifacts, but `technical_gates_passed = false` with an explicit
pending-acquisition/pending-transform reason for every reference. It never
invents a passing (or failing) number in the absence of real data.

## `scientific_review_status` stays `"pending"` until #47

This pipeline produces *technical* evidence only: reproducible acquisition,
frozen regions, computed metrics, and automatic gate evaluation against
preregistered tolerances. Whether a specific candidate checksum is fit for
production use is a scientific judgment made by a qualified human, recorded
by hand in a copy of `scientific-review-decision-v1.json`, tracked in issue
#47. No command in this pipeline, and no future automation built on top of
it, should ever flip `scientific_review_status` to anything other than
`"pending"` or set `scientifically_validated = true`.

## What's still missing for #87 to close

- **Real acquired references.** All three entries in `references-v1.toml`
  are still `status = "pending-acquisition"`; nothing has been downloaded,
  hashed, or receipted yet.
- **Physical transformations.** Each reference needs a reviewed, documented
  implementation of its `transformation_to_target` (unit conversion,
  passband adjustment, and regridding onto the candidate map's nside=128
  NESTED pixelization) before `run` can compute anything beyond a
  pending-transform status for it.
- **A resolved dependency on #94.** The candidate map's checksum is expected
  to change once the uncertainty scale audit (#94) regenerates it;
  `preregistration-v1.toml` pins the map's *path*, not a specific checksum,
  for exactly this reason, but validation coverage from this pipeline should
  not be treated as trustworthy until #94 lands.
- **HTML report rendering.** Issue #87 lists Markdown and HTML reports; this
  PR ships only the Markdown report and the JSON results it is rendered
  from.
- **The actual human decision.** Once real references and transformations
  exist and `run` produces a `validation-results-v1.json` with
  `technical_gates_passed = true` (or a clearly diagnosed failure), a
  qualified human scientist still needs to review that evidence and record a
  real decision in issue #47 — this pipeline only prepares the template and
  the evidence for that decision, it never makes it.
