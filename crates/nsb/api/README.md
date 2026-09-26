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

bootstraps the reviewed baseline. Historical `BASE..HEAD` comparison is skipped
until the selected historical base also contains the freeze marker.

### Post-freeze

Once the historical base is frozen:

- HEAD must match `public-api.txt`;
- `cargo public-api diff BASE..HEAD --deny=removed --deny=changed` must pass;
- `BASE == HEAD` and empty historical comparisons fail closed.

## Maintainer commands

```bash
# regenerate snapshot (after reviewing the surface)
scripts/check-public-api.sh --write

# local check with an explicit historical base
scripts/check-public-api.sh --base origin/main
```

Policy: [`docs/developer-guide/public-api.md`](../../../docs/developer-guide/public-api.md).
