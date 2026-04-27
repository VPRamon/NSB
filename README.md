# nsb — Night Sky Background (Rust)

Rust reimplementation of the CTAO [`darknsb`](./darknsb/) Python model for the
ground-based night-sky background (NSB), with optional Python bindings.

The crate computes the photon flux in `ph/(cm² · ns · sr)` reaching a
ground-based observatory from the dark sky, plus the integrated B and V band
surface brightness in `mag/arcsec²`. Contributions:

- **Zodiacal light** — Leinert (1998) brightness map, Noll (2012) reddening &
  extinction, scaled solar spectrum.
- **Integrated starlight** — SkyCalc Cerro Paranal radiance.
- **Airglow continuum** — empirical cubic in source altitude (Noll 2012).
- **Scattered moonlight** — currently a stub returning zero (TODO: port the
  Jones 2013 model + per-season LUTs).

## Build & test

Pure-Rust build:

```bash
cargo build
cargo test                                  # unit tests
cargo test --test cross_validation          # against Python golden fixtures
cargo run --example ctaos_sgr_a             # mirrors get_NSB.py
```

Cross-validation tolerances and the per-metric Δ are aggregated into
`target/nsb_discrepancy_report.md` whenever the cross-validation test runs.

### Python bindings

```bash
python3 -m venv .venv
.venv/bin/pip install maturin pytest 'numpy<2' 'scipy<1.14' 'astropy<6'
.venv/bin/maturin develop --features python
.venv/bin/pytest python/tests
```

The Python module exposes:

```python
import nsb
r = nsb.calculate("CTAO-S", "2023-09-04 01:48:00", "SgrA*")
print(r.integrated, r.b_mag, r.v_mag)
for c in r.components:
    print(c.name, c.integrated, c.b_s10, c.v_s10)
```

### Regenerating golden fixtures

```bash
.venv/bin/python tools/capture_golden.py
```

The script pins to `numpy<2`, `scipy<1.14`, `astropy<6` because the original
Python code relies on `scipy.interpolate.interp2d` and
`astropy.coordinates.get_moon`, both removed in newer releases.

## Layout

```
src/
├── components/    # ZL, SL, AG, Moon
├── spectra/       # Solar / starlight / airglow / ozone loaders + B/V filters
├── atmosphere/    # Rayleigh / Mie / single-scatter
├── ephemeris/     # Sun, Moon, source resolver
├── geometry/      # Site + time + airmass + alt-az/ecliptic transforms
├── data/          # Embedded Leinert table + bundled .dat files
├── units/         # Local newtypes (S10, BandPhotonRadiance, ...)
└── pybind/        # PyO3 module behind `python` feature
```

Anything in `units/` or implemented locally that should eventually live in
`siderust`/`qtty` is annotated `TODO: implement in siderust` and tracked in
`docs/TODO_SIDERUST.md`.

## Sibling crates

This crate depends, via `path = ".."`, on:

- [`qtty`](../qtty) — quantity / unit types
- [`tempoch`](../tempoch) — astronomical time scales
- [`siderust`](../siderust) — coordinates, observatories, ephemerides
- [`affn`](../affn) — affine geometry primitives
- [`cheby`](../cheby) — Chebyshev interpolation

## Status & known gaps

| Component | Parity vs Python |
|---|---|
| Zodiacal | ~2 % (driven by simplified ecliptic transform) |
| Starlight | exact (<1e-4) |
| Airglow | <0.5 % (cubic-in-altitude approximation; full spectral model TODO) |
| Moonlight | not implemented |

See `target/nsb_discrepancy_report.md` after `cargo test` for the live numbers.

## Documentation

- `docs/README.md` — documentation index.
- `docs/DARKNSB_REPORT.md` — what NSB is and how the Python code computes it.
- `docs/SIDERUST_REIMPLEMENTATION_REPORT.md` — feasibility map onto siderust.
- `docs/NSB_STAGED_IMPLEMENTATION_PLAN.md` — staged porting roadmap.
- `docs/NSB_CONCEPT_PROVENANCE_AND_SIDERUST_REUSE_REPORT.md` — source-of-knowledge and upstream-reuse assessment for NSB concepts.
