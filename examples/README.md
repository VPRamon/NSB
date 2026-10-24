# Examples

The runnable Rust examples live in [`crates/nsb/examples/`](../crates/nsb/examples/).
This root directory indexes the examples and contains Python notebooks. The Rust examples are compiled by the repository CI.

## Rust

Run these from the repository root with Rust 1.89+:

| Example | Purpose | Source |
| --- | --- | --- |
| `point_query` | Evaluate NSB for one target and UTC instant | [`point_query.rs`](../crates/nsb/examples/point_query.rs) |
| `threshold_window` | Find periods below a radiance threshold | [`threshold_window.rs`](../crates/nsb/examples/threshold_window.rs) |
| `model_metadata` | Select components and inspect model provenance | [`model_metadata.rs`](../crates/nsb/examples/model_metadata.rs) |

```bash
cargo run --locked -p nsb --example point_query
cargo run --locked -p nsb --example threshold_window
cargo run --locked -p nsb --example model_metadata
```

The example programs use the local workspace dependencies and pinned lockfile.
For library installation in your own project, see the
[main README](../README.md#install) and [crates.io](https://crates.io/crates/nsb).

## Python

The [Python quickstart](../README.md#python) shows a complete point evaluation
using `nsb-py` (import `nsb`) and `siderust`. Continue with the
[Python guide](../docs/python.md) for threshold-window planning, model
selection, errors and units.

The optional [continuum notebook](scientific-reproducibility.ipynb) demonstrates
a fixed-geometry F10.7 sweep with an isolated, precisely pinned source build,
bundled assets and one figure. Its outputs are cleared in Git.

The [observational validation notebook](validation/NSB_Validation.ipynb) compares
NSB Standard, explicitly selected experimental Oxygen, and pinned nsb2 against
the same verified Patat FORS1 spectra. See its [setup and scientific protocol](validation/README.md)
for single-file execution, version selection, explicit developer wheels, solar
scenarios, paired night-cluster statistics and compatible release comparison.
Published 0.1.1 lacks the required APIs; the notebook explains developer mode.
Results remain development evidence.

## Command line

The [CLI quickstart](../README.md#cli) demonstrates point evaluation with
the `nsb` executable. The [getting-started guide](../docs/user-guide/getting-started.md)
covers observing-window searches, JSON/CSV output, named sites and custom
coordinates.

**Note:** `nsb-cli` is a workspace crate and is not published separately
to crates.io in the 0.2 release line.

### Registered Airglow calibration

`cargo run --locked -p nsb --example airglow_calibration` generates a runnable,
checksum-linked synthetic fit request and portable artifact. The example recovers
one continuum scale without promoting scientific maturity. See
[the complete observatory workflow](../docs/user-guide/airglow-calibration.md).

`point_query` supplies the minimal evaluation path; `model_metadata` demonstrates
explicit component/model selection and provenance. Custom observer coordinates
use the public Siderust observer type shown in the CLI/Python quickstarts; they
do not establish site calibration. `airglow_calibration` is an advanced synthetic
fit/export/replay example, not an admitted observatory calibration. The generic
global Airglow model remains unavailable: automatic selection reports its
explicit PALACE reference fallback and applicability in component metadata.
