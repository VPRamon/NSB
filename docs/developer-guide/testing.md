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
| Unit behaviour | `crates/*/src/**` test modules (`#[cfg(test)] mod tests`, `tests.rs`) | Local scientific or parsing behaviour |
| Invariant / property | evaluator, planning/window search, component composition | Sums, ordering, monotonicity, fail-closed rules |
| Numerical / physical boundary | component unit tests | Zenith edges, domain cut-offs, FP clamps |
| Error / rejection | `query_api`, component constructors, CLI error suite | Invalid input, missing assets, inverted ranges |
| Known regression | named `regression_*` tests, fixtures under `tests/data/` | Pinned bug regressions and radiance cases |
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

### Production/test module boundary

Inside production `crates/*/src/**` trees, `#[cfg(test)]` is a module-boundary attribute only.
It may include inline or sibling test modules (for example `mod tests`, a `tests.rs` via
`#[path]`, or a narrowly scoped `test_support` module), but it must not change production
imports, fields, methods, functions, constructors, branches, statics, thread-locals, or type
variants. Unit-test child modules can access private implementation details, so prefer Rust
module privacy over adding test-only accessors to production types.

Independent reference algorithms and synthetic fixtures that exist only to validate the real
implementation belong in those test modules. If a helper is a genuine implementation concept,
keep it as normal private production code and exercise the same helper from production and tests.
CI enforces this rule with the small Bash check `scripts/check-cfg-test-boundaries.sh`.

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
- Reference CSV columns may remain as schema/tolerance manifests even
  when numeric protection lives in unit-test regression pins (see Jones 2013
  validation notes).

## Numerical and interface regression

`cargo test --workspace --all-features --locked` runs `numerical_contracts`
(clipped analytic spectra, quadratic trapezoid convergence, radiance units,
Beer–Lambert composition), `interface_equivalence` (registered public Rust
results and deterministic evaluation), and the exact-SI S10 unit regression.
These verify implementation contracts, not observational accuracy.

The Linux installed-wheel CI job also compares Rust, installed Python and CLI:

```bash
cargo build --locked -p nsb-cli --bin nsb
NSB_INTERFACE_TESTS=1 .wheel-venv/bin/python -m pytest python/tests/test_interface_equivalence.py -q
```

Use an isolated venv containing a release wheel built from the same checkout
(`maturin build --release --locked`) and pytest. The pytest fixture runs the
ordinary Rust integration test and reads its temporary JSON output; CLI commands
run outside the checkout. Default pytest runs explicitly skip this optional
native-build integration when `NSB_INTERFACE_TESTS` is unset. Enabled checks fail
if native builds, binaries or dependencies are missing. `NSB_CLI` may identify
a different CLI binary; Cargo's normal target directory is used otherwise.

The nine shared fixtures under `crates/nsb/tests/data/interfaces/` preserve the
original registered numerical values: six continuum F10.7 inputs, zodiacal,
combined zodiacal+continuum and physically zero Moon. Point totals/components
are compared across all three interfaces. Four restricted bands are compared
between Rust and Python; the CLI does not expose this operation. The tests retain
component sums, full-band/partition identities, repeated-run determinism,
positive increasing sweep and nonpositive-flux rejection. Starlight, nonzero
Moon restricted bands and the complete planning API are outside this fixture
intersection; their existing domain/contract tests remain authoritative.

Tolerances are unchanged: `1e-14 + 2e-12 * max(abs(a), abs(b))` in
photons cm^-2 ns^-1 sr^-1, allowing serialization/ABI rounding with an absolute
floor for physically zero components. Numerical analytic cases retain their
original absolute tolerances. Reference changes require scientific review;
there is no automatic reference-registration command or committed result log.

## Mutation testing

Tool: `cargo-mutants` **27.1.0** (pin this version when reproducing).

Configuration: [`.cargo/mutants.toml`](../../.cargo/mutants.toml).

Reproducible maintainer command from the repository root:

```bash
cargo install cargo-mutants --version 27.1.0 --locked
cargo mutants -p nsb --config .cargo/mutants.toml
```

Scope concentrates on a finishable release-critical `nsb` default pass:

- evaluator orchestration (`evaluator/core.rs`, `evaluator/point.rs`);
- solar-activity resolution policy (`solar_activity/resolve.rs`);
- site-calibration fail-closed validation (`site/calibration.rs`).

Threshold-window search (`planning/`) and broader
component physics (Jones spectral radiance, airglow geometry/extinction) are
intentional follow-up examine targets using the same tool version; keep them out
of the default pass when wall-clock would make the audit impractical.

### Intentional exclusions

| Exclusion | Justification |
| --- | --- |
| `**/tests.rs` and solar-activity test modules | Test code is not a production mutant target |
| `spectra/`, moonlight `scattering.rs`, zodiacal `leinert.rs`, starlight `map.rs` | Large static grids / loaders where mutants are dominated by table noise |
| `data/bundled.rs`, `build.rs` | Bundled-data registry and build glue; checksum contracts live elsewhere |
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
- Jones spectral regression pins cover the reference fixture geometries;
  retained the CSV as schema/tolerance manifest.
- CLI contract coverage is split into focused suites,
  keeping a thin `cli_smoke` binary only for the scientific-validation workflow
  pin.
- Introduced `.cargo/mutants.toml` and recorded the maintainer mutation workflow
  above.
- Mutation follow-up on the default examine set (`cargo-mutants` 27.1.0):
  - `site_calibration` validity / identifier / uncertainty mutants that
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
