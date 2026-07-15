# Updating scientific data

Status: Current maintainer runbook.
Audience: Scientific-data and release maintainers.
Scope: Safe replacement or addition of NSB runtime scientific assets.

## Before starting

Create a caller-owned run directory outside the repository and record the source
release, license, exact request, checksums, software commit, and tool versions.
Verify the checked-in asset state first:

```bash
cargo run --locked -p nsb-data-tools --bin nsb-data -- assets verify \
  --manifest crates/nsb/data/manifest.toml
```

Use the generated [tool reference](tools.md) to select the relevant action.
`nsb-data starlight --help` presents the supported acquisition, reconstruction,
map, quality, product, and release groups.

## Required workflow

1. Acquire source data with the appropriate `starlight acquire` action; reuse
   only verified persisted inputs.
2. Produce canonical sources with `starlight catalogue prepare-gaia` or the
   XP-continuous normalize/reconstruct actions.
3. Build and, when required, sweep a map through `starlight map`.
4. Run the relevant XP-continuous, map, and exclusion validation actions.
5. Build the integrated candidate only from versioned contributions and policies.
6. Package only after every required production gate passes, using
   `starlight release pack-asset`.
7. Add reviewed assets and their manifest entries in one change, then rerun
   `assets verify` and the workspace tests.

Never copy candidate output into `crates/nsb/data/` before provenance,
validation, maturity, and release evidence are complete. Candidate, partial, or
pilot data cannot satisfy production admission.

## Documentation and review

Update the relevant component generation and validation pages, the model-maturity
and validation specifications, the runtime manifest, and the changelog. Verify
the generated tool reference is current:

```bash
cargo run --locked -p nsb-data-tools --bin nsb-data -- \
  maintenance render-tool-reference --check
```

Use the full [release checklist](../operations/release-checklist.md) before
tagging or distributing a release.
