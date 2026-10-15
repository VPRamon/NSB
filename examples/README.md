# Examples

The runnable Rust examples live in [`crates/nsb/examples/`](../crates/nsb/examples/).
This root directory is a **guide and index**, not a second copy of the source
code. The Rust examples are compiled by the repository CI.

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

## Command line

The [CLI quickstart](../README.md#cli) demonstrates point evaluation with
the `nsb` executable. The [getting-started guide](../docs/user-guide/getting-started.md)
covers observing-window searches, JSON/CSV output, named sites and custom
coordinates.

**Note:** `nsb-cli` is a workspace crate and is not published separately
to crates.io in the 0.1 release line.
