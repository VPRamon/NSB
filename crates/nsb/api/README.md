# `nsb` public API lifecycle

The first-release public API lifecycle is protected by [`scripts/check-public-api.sh`](../../../scripts/check-public-api.sh)
using pinned `cargo-public-api` directly (#176).

Issue #185 previously used the documented pre-freeze mode for the typed
site-profile redesign. Issue #214 now temporarily returns the unreleased
`0.1.0` tree to that same pre-freeze mode because the truthful Airglow model
identity changes from the historical Noll/SkyCalc/FORS1 label to PALACE v1.0.
No public NSB release exists yet, so preserving the obsolete model name as a
compatibility alias would be scientifically misleading. `public-api.txt`
records the current candidate surface, but snapshot equality and historical
SemVer rejection remain disabled until maintainers re-add `API_FROZEN` and
bootstrap the final reviewed `0.1.0` baseline.

The redesigned site-profile surface is typed: `SiteProfileTag` markers supply
compile-time identity; `SiteProfile<P>` is opaque and erases into
`NsbModelConfig`. Public constructors (`generic_clear_sky`, `planning`) produce
only `GenericFallback` or `PlanningPreset` maturity, so external markers cannot
claim `CalibrationStatus::Calibrated` or promote evaluator metadata to
`Production` without a future evidence-backed admission path. Profile resolution,
Airglow template selection, and asset details remain internal; `planning`
rejects nonphysical atmospheric inputs via `Result`.

## Modes

### Pre-freeze

When `crates/nsb/api/API_FROZEN` is absent:

- intentional signature changes are allowed;
- snapshot equality is not required;
- historical SemVer rejection is disabled;
- forbidden-API debt guards still run against the generated public API.

### Freeze bootstrap

The commit that introduces:

- `crates/nsb/api/API_FROZEN`
- `crates/nsb/api/public-api.txt`

bootstraps the reviewed baseline. When the selected historical base lacks the
freeze marker, snapshot equality is required and historical `BASE..HEAD`
comparison is skipped. Once HEAD is frozen, omitting a historical base fails
closed (it is not treated as successful snapshot-only mode).

### Post-freeze

Once the historical base is frozen:

- HEAD must match `public-api.txt`;
- `cargo public-api diff BASE..HEAD --deny=removed --deny=changed` must pass;
- `BASE == HEAD`, empty bases, and unresolvable bases fail closed.

## Maintainer commands

```bash
# regenerate snapshot (after reviewing the surface)
scripts/check-public-api.sh --write

# local check with an explicit historical base
scripts/check-public-api.sh --base origin/main

# lifecycle assertions (missing base, BASE==HEAD, bootstrap)
scripts/test-check-public-api.sh
```

Policy: [`docs/developer-guide/public-api.md`](../../../docs/developer-guide/public-api.md).
