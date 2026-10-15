# `nsb` public API lifecycle

The first-release public API lifecycle is protected by [`scripts/check-public-api.sh`](../../../scripts/check-public-api.sh)
using pinned `cargo-public-api` directly (#176).

The reviewed `0.1.0` surface is recorded in `public-api.txt`, and `API_FROZEN`
activates snapshot equality plus historical SemVer checks. Before the first
release, the ignored `ThresholdQuery::sample_step`, `DEFAULT_SAMPLE_STEP` and
`with_sample_step` were deliberately removed, together with their Python/CLI
counterparts. The gate admits that transition only for the exact SHA-256 pair
of old and new snapshots, pinned in the script. It does not permit other
removals or signature changes. Subsequent snapshot pairs use the normal
`cargo-public-api diff --deny=removed --deny=changed` enforcement; the marker
stays present throughout. After the first public release this historical
transition can be retired when no supported comparison base needs it.

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
- `cargo public-api diff BASE..HEAD --deny=removed --deny=changed` must pass,
  except for the exact reviewed pre-release snapshot pair above;
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
