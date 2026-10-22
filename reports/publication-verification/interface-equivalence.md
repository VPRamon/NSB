# Cross-interface evidence scope

## Capability matrix

| Existing operation | Rust | Python | CLI | New executed comparison |
| --- | --- | --- | --- | --- |
| Point integrated radiance and components, 300–650 nm | yes | yes | yes, JSON/CSV/table | Nine shared fixtures; each component and total, all three pairs |
| Restricted-band total/components | yes | yes | no command | Four band totals, Rust/Python; component API covered in existing tests |
| Threshold observing windows | yes | yes | yes | Existing tests only; not included in new equivalence claim |
| Experimental transport reference solvers | advanced Rust | no equivalent binding | no command | No cross-interface claim |
| Source-map preprocessing | separate data-tool crate | no matching API | nsb-data, separate app | No cross-interface claim |

The new common subset is deliberately restricted: explicit PALACE continuum,
explicit F10.7, generic-clear-sky atmosphere, Zodiacal/Airglow components and a
physically zero Moon. No TOA Starlight/ground mixing or invented lunar spectra.
Each fixture records UTC, geodetic geometry and **ICRS** target. Native Rust
converts ICRS to its public Mean-J2000 Target, matching the binding and CLI.
The common quantity is photon radiance, photons cm^-2 ns^-1 sr^-1; no numeric
comparison between S10 proxies and filter magnitudes is made.

The harness compares each interface pair, requires identical component sets,
compares Rust/Python full and sub-band integrals, repeats computations within
processes (Rust probe also restarted; CLI restarted) and requires deterministic
numeric representations. Explicit zero F10.7 must be rejected by each interface;
error classes/messages may differ across language boundaries. Existing suites
cover further invalid bounds/unsupported components. Siderust direction
constructors normalize angles, so a declination outside ±90 degrees is not used
as an invalid-input oracle after direction construction.

Tolerance is `abs(a-b) <= 1e-14 + 2e-12*max(abs(a),abs(b))`. This allows arithmetic
and text round-trip rounding across builds, is small compared with model
uncertainty, and applies to the **same** numerical observable. The absolute term
handles exact/near-zero contributions. No interface-specific scaling is used.
`interface-equivalence.json` contains each comparison and actual observed errors;
`manuscript-evidence.md` summarizes maxima. Registered output comparisons are
regressions, while analytic receipts provide independent oracles.

The Python package is a built wheel, never an editable import. The CLI is run
from a temporary directory outside the checkout. The clean-source reproduction
and build receipts establish which environment was actually executed. The
compiled Rust CLI and probe use the locked library; the CLI crate is marked
`publish=false`, so source/binary installation is the supported route.

An extended invocation is documented in
[the example](../../examples/publication/README.md); the offline invocation is
run in the existing Linux installed-wheel CI job, with evidence uploaded even
on failure. This does not assert every NSB operation or platform is equivalent.
