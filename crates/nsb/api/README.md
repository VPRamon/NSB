# `nsb` public API lifecycle

The first-release public API is protected by [`scripts/check-public-api.sh`](../../../scripts/check-public-api.sh)
using pinned `cargo-public-api` directly (#176).

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
