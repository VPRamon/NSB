# Data-product workflow

Status: Current workflow for NSB scientific assets.
Audience: Release maintainers and scientific-data maintainers.
Scope: Selecting and sequencing durable `nsb-data` actions.

`nsb-data` is the only supported data-product executable. Start with its
hierarchical help and the generated [tool reference](tools.md):

```bash
cargo run --locked -p nsb-data-tools --bin nsb-data -- starlight --help
```

## Starlight workflow

```text
acquire -> catalogue or XP-continuous reconstruction -> sampling (when needed)
        -> map build/sweep -> map and accounting validation
        -> integrated product -> runtime-asset packaging -> asset verification
```

Use one action from each relevant group; do not recreate a phase-specific
workflow or call sibling commands from a tool. Each action consumes explicit,
versioned inputs and emits reusable, checksum-verified outputs.

- `starlight acquire`: Gaia TAP jobs, release inputs, and official XP bulk.
- `starlight catalogue` and `starlight xp-continuous`: canonical scientific
  inputs and reconstruction evidence.
- `starlight sampling`: deterministic model-development datasets.
- `starlight map`, `starlight quality`, and `starlight product`: candidate
  construction, scientific gates, and source accounting.
- `starlight release`: fail-closed runtime packaging.
- `assets verify`: final manifest, checksum, and metadata verification.

Candidate generation is not production admission. Follow the
[science requirements](../nsb_components/starlight/science-requirements.md),
[map validation contract](../nsb_components/starlight/map-validation.md), and
[external manifest contract](../nsb_components/starlight/external-manifest.md)
before production packaging.

## Maintaining actions

The registry is the sole action inventory. Add a reusable capability only when
it has a stable owner, typed input/output contract, validation path, and clear
audience. Update the registry, service, tests, and generated reference together.
Remove obsolete capabilities completely; aliases, pilot implementations, legacy
wrappers, and ad-hoc scripts are prohibited.
