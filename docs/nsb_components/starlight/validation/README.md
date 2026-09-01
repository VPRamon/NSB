# Independent Starlight validation (issue #102)

Status: Acquired literature targets are checksum-pinned and not admissible
as starlight-only TOA 300–650 nm grids (`no_admissible_independent_reference`).
Audience: Maintainers preparing evidence for the human review in #103.
Scope: Checksum-pinned external references and independent comparison tooling
against the integrated 300-650 nm Starlight candidate map when a
scientifically admissible transform exists.

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
exclusively to a qualified human scientist recorded in issue #103.

## The frozen documents

- [`preregistration-v1.toml`](preregistration-v1.toml) — the exact tolerances
  from issue #87, the pinned candidate map path, and the metric vocabulary.
  Frozen before any reference is compared, so thresholds cannot be tuned
  after seeing results.
- [`references-v1.toml`](references-v1.toml) — the reference registry. Toller,
  Leinert, and GAMBONS are already `status = "acquired"` with pinned
  SHA-256 digests (`acquisition_required = false`). No checksum in this
  repository is ever invented.
- [`regions-v1.json`](regions-v1.json) — reproducible sky-region formulas
  (latitude/longitude bands, cones, cone unions, and candidate-map-driven
  percentile selectors) evaluated fresh by `RegionEngine` against whichever
  candidate map and `nside` are actually supplied to `run`.

This validation pipeline produces **technical evidence only**. The ONLY human
scientific decision used for final promotion is:

[`../release-candidate/scientific-review-decision-v1.json`](../release-candidate/scientific-review-decision-v1.json)

No scientist should edit a second decision template under `validation/`.

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

The three current registry entries are already acquired and checksum-pinned.
Re-running `acquire` only re-verifies those pinned bytes (for example after
refreshing a local workspace). Supply `--source <id>=<path-or-url>` only when
re-acquiring an existing entry or registering a future reference.

Acquiring a file only proves the *bytes* match a declared checksum. It does
not, by itself, produce a comparable grid. A documented physical
transformation into the candidate's 300-650 nm integrated photon-radiance
convention (nside=128, NESTED HEALPix) is required only when a reference is
scientifically admissible as a starlight-only TOA comparison surface. No such
transformation is pending for Toller, Leinert, or GAMBONS: each is already
recorded `not-admissible`. A future scientifically admissible reference would
need that transform before numeric metrics can be computed; its output format
is the `transformed-grid-v1.csv` file `run` looks for under
`<references-workspace>/<reference-id>/`.

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

If a reference is acquired but not admissible (Toller Pioneer poles; Leinert
1998; GAMBONS), `run` records `not-admissible` and does not invent comparison
numbers. Leinert et al. 1998 discusses a Gaussian representation / S10 anchor
data, but the published material does not expose the parameters required to
reconstruct the registered comparison surface without inventing an
interpolation/model. It is therefore acquired for provenance only and is
**not** an admissible independent numeric comparison grid. No reference in
the current frozen evidence is a numeric PASS.

HTML and Markdown reports are both written (`validation-report-v1.html` and
`.md`).

## Independent validation status for the UV v2 candidate

Results against map `76191c8b682d96adfc3a017f44f3fcfd0bec5dcb9a958d31668250b8a0ba396a`
are stored in [`results/`](results/). All three acquired references are
checksum-pinned and **not admissible** as starlight-only TOA 300–650 nm grids:

- Toller: not-admissible
- Leinert: not-admissible
- GAMBONS: not-admissible

`independent_reference_status = no_admissible_independent_reference`.
`reference_results = []`. `technical_gates_passed = false`.
`scientifically_validated` remains false. No transformation is pending for
these three references. That encoding is human-review evidence for #103, not
a software defect and not a scientific PASS.

## `scientific_review_status` stays `"pending"` until #103

This pipeline produces *technical* evidence only: reproducible acquisition,
frozen regions, computed metrics, and automatic gate evaluation against
preregistered tolerances. Whether a specific candidate checksum is fit for
production use is a scientific judgment made by a qualified human and
recorded only in:

`docs/nsb_components/starlight/release-candidate/scientific-review-decision-v1.json`

tracked by issue #103. No command in this pipeline, and no future automation
built on top of it, should ever flip `scientific_review_status` to anything
other than `"pending"` or set `scientifically_validated = true`.

## What's still missing after the technical #87 package

- **Human scientific decision.** `scientific_review_status` stays `"pending"`
  in issue #103. Independent validation remains
  `no_admissible_independent_reference`; see [`results/`](results/). Do not
  invent unpublished Leinert parameters or retune preregistered gates to force
  a pass.
- Toller Pioneer, Leinert 1998, and GAMBONS remain acquired-but-not-admissible
  (DGL/ZL/airglow inseparable, or unpublished Gaussian parameters). A numeric
  transform would be needed only if a future scientifically admissible
  reference appears.
