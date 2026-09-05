# Testing and mutation policy

Status: Current developer and maintainer test-quality contract.
Audience: Contributors adding tests; release maintainers auditing the suite.
Scope: Test taxonomy, ownership by layer, mutation-testing workflow, CLI
contract suites, regression fixtures, and how coverage differs from test quality.
Non-goals: This page does not set coverage floors (see [Coverage policy](coverage.md))
and does not replace scientific validation evidence.

## Why test quality is gated separately from coverage

Coverage answers “was this line executed?”. Release test quality answers:

> What real bug, contract violation, or scientific/numerical regression would
> this test detect?

A suite can raise coverage while remaining weak: constant self-checks,
`Default::default()` smoke, assertion-free execution, or duplicated happy paths
that leave rejection and boundary behaviour unprotected. Issue #123 audited the
suite against that standard. Coverage floors are the post-audit release baseline
in [Coverage policy](coverage.md) (`baseline_kind = release-post-audit`).

## Test taxonomy

| Category | Typical location | Protects |
| --- | --- | --- |
| Unit behaviour | `crates/nsb/src/**` `#[cfg(test)]`, `**/tests.rs` | Local scientific or parsing behaviour |
| Invariant / property | evaluator, window search, component composition | Sums, ordering, monotonicity, fail-closed rules |
| Numerical / physical boundary | component unit tests | Zenith edges, domain cut-offs, FP clamps |
| Error / rejection | `query_api`, component constructors, CLI error suite | Invalid input, missing assets, inverted ranges |
| Known regression | named `regression_*` tests, fixtures under `tests/data/` | Previously fixed bugs and pinned radiance cases |
| Scientific validation / reference | Jones/KS91/airglow fixtures, scientific-validation workflow | Literature or independent reference agreement |
| Public API contract | `crates/nsb/tests/{api_contract,query_api}.rs` | Supported constructors, masks, defaults, `NsbError` |
| Serialization / schema / metadata | `science_metadata`, CLI JSON/CSV suites, asset manifests | Schema versions, provenance, maturity honesty |
| End-to-end composition | `end_to_end_validation.rs` | Multi-component totals and threshold windows |
| CLI contract | `crates/nsb-cli/tests/*_contract.rs` | Command behaviour and machine output |
| Performance / regression | benches, precision performance tests | Throughput or precision budgets |

## Which layer owns which contract

| Layer | Owns | Must not own |
| --- | --- | --- |
| `nsb` unit tests | Component physics, local rejection, numeric pins | CLI parsing or site-alias UX |
| `nsb` integration tests | Public evaluator/query API, metadata honesty, e2e composition | Private layout details of helpers |
| `nsb-cli` contract suites | Argument parsing, aliases, JSON/CSV/table schemas, exit messages | Re-deriving scientific totals already pinned in `nsb` |
| `nsb-data-tools` tests | Offline pipeline schemas, asset registry, HEALPix/build contracts | Runtime query semantics |
| Scientific-validation workflow | Explicit release evidence commands | Everyday PR smoke |
| Doctests / examples | Documented public usage | Exhaustive rejection matrices |

Prefer one strong test at the correct layer over the same assertion copied into
unit, integration, and CLI suites.

## When to add which kind of test

- **Unit test** when a local formula, parser, or fail-closed branch can change
  without crossing a crate boundary.
- **Integration / public API test** when callers of the frozen `nsb` API must see
  a behaviour or error.
- **CLI contract test** when flags, aliases, exit codes, or output schemas are
  user-visible.
- **Scientific validation** when a claim depends on a reference fixture,
  checksum-pinned asset, or literature tolerance.
- **Mutation-driven test** when a surviving mutant shows a meaningful behaviour
  change that no existing assertion detects.

Do not add tests solely to raise coverage percentage.

## CLI contract suites

`crates/nsb-cli/tests/` is split by contract, not by file size:

| Suite | Contract |
| --- | --- |
| `sites_contract.rs` | Site list/show aliases and JSON |
| `config_contract.rs` | `config init` / `validate` accept and reject paths |
| `point_contract.rs` | Default point JSON/CSV schema and component presence |
| `window_contract.rs` | Window JSON schema, empty-result table/JSON behaviour |
| `airglow_provenance_contract.rs` | Vertical-profile and van Rhijn provenance in point/window CSV/JSON |
| `starlight_contract.rs` | Bundled/external starlight labelling, uncertainties, rejection |
| `cli_smoke.rs` | Thin scientific-validation compatibility target for one pinned starlight JSON label test |
| `error_contract.rs` | Unknown site and invalid NSB range rejection |
| `common/` | Shared fixtures and CSV helpers |

The scientific-validation workflow pins

`cargo test --locked -p nsb-cli --test cli_smoke validated_external_starlight_is_production_labelled_in_json`.

Rename that test or binary only together with `.github/workflows/scientific-validation.yml`.

## Regression fixtures

- Keep fixtures after the original bug is fixed; they document the protected
  behaviour.
- Do not weaken scientific tolerances to make a mutant or CI job easier.
- Prefer behaviour-level assertions over re-implementing the production formula
  inside the test.
- Historical reference CSV columns may remain as schema/tolerance manifests even
  when numeric protection lives in unit-test regression pins (see Jones 2013
  validation notes).

## Mutation testing

Tool: `cargo-mutants` **27.1.0** (pin this version when reproducing).

Configuration: [`.cargo/mutants.toml`](../../.cargo/mutants.toml).

Reproducible maintainer command from the repository root:

```bash
cargo install cargo-mutants --version 27.1.0 --locked
cargo mutants -p nsb --config .cargo/mutants.toml
```

Scope concentrates on a finishable release-critical `nsb` default pass:

- evaluator orchestration (`evaluator/core.rs`);
- solar-activity resolution policy (`solar_activity/resolve.rs`);
- site-calibration fail-closed validation (`site_calibration.rs`).

Threshold-window search (`window_search.rs`, `evaluator/search.rs`) and broader
component physics (Jones spectral radiance, airglow geometry/extinction) are
intentional follow-up examine targets using the same tool version; keep them out
of the default pass when wall-clock would make the audit impractical.

### Intentional exclusions

| Exclusion | Justification |
| --- | --- |
| `**/tests.rs` and solar-activity test modules | Test code is not a production mutant target |
| `reference/`, moonlight `scattering.rs`, zodiacal `leinert.rs`, starlight `map.rs` | Large static grids / loaders where mutants are dominated by table noise |
| `assets.rs`, `build.rs` | Manifest registration and build glue; checksum contracts live elsewhere |
| CLI / data-tools crates in the default pass | First pass targets silent scientific/runtime logic changes in `nsb` |

Do not broaden exclusions merely to improve a mutation score. For every
meaningful surviving mutant: protect the behaviour with a contract-level test,
or document why the mutant is equivalent/irrelevant.

Mutation testing is a maintainer audit. It is not currently a blocking per-PR CI
job because full runs are long and flaky under unconstrained scope.

## Audit outcome (release pass for #123)

- Removed assertion-free or misnamed smoke tests (airglow polynomial no-op,
  formula-mirroring Noll checks, duplicated geometry/default smokes).
- Consolidated overlapping site-pressure, default-config, and API construction
  tests; strengthened public `NsbError` diagnostics and component-mask naming.
- Replaced absurd e2e radiance envelopes with composition + moonlit scene
  contrast; fixed the bright-Moon case to a time with non-zero moonlight.
- Added Jones spectral regression pins for historical fixture geometries;
  retained the CSV as schema/tolerance manifest.
- Split the former monolithic `cli_smoke.rs` into focused CLI contract suites,
  keeping a thin `cli_smoke` binary only for the scientific-validation workflow
  pin.
- Introduced `.cargo/mutants.toml` and recorded the maintainer mutation workflow
  above.
- Mutation follow-up on the default examine set (`cargo-mutants` 27.1.0):
  - `site_calibration` validity / identifier / uncertainty mutants that previously
    survived were closed with fail-closed asset tests (25/25 caught on re-verify).
  - `solar_activity/resolve` policy mutants: explicit/observed maturity and
    `is_degraded_planning_input` kind-or-completeness contracts were strengthened;
    PartialEq / provenance-fragment mutants remain excluded as low-signal.
  - `evaluator/core` remains in the default examine set for maintainer runs; prefer
    shard/filter flags when iterating because full three-file passes are long.

Coverage floors were not lowered. The public API freeze was preserved. Scientific
tolerances were not relaxed.

## Related documents

- [Coverage policy](coverage.md)
- [Validation specification](../specifications/validation.md)
- [Performance contract](../specifications/performance.md)
- [Public API policy](public-api.md)
- [Jones 2013 validation](../nsb_components/moonlight/jones2013-validation.md)
