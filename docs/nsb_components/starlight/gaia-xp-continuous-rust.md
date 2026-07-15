# Gaia DR3 XP continuous reconstruction in Rust

The retained Gaia XP continuous workflow is implemented entirely in Rust. Python programs, Python package manifests, and shell orchestration wrappers were removed under issue #61.

## Durable commands

Build the maintainer tools once:

```text
cargo build --locked --release -p nsb-data-tools
```

Then invoke the compiled capabilities directly:

```text
nsb-data starlight xp-continuous normalize ...
nsb-data starlight xp-continuous reconstruct ...
nsb-data starlight xp-continuous validate ...
```

No command launches Python, GaiaXPy, `cargo run`, or another maintainer executable as a subprocess.

## Calibration contract

`gaia_xp_continuous_calibrate` reconstructs the inclusive 336–650 nm grid at 2 nm spacing from canonical BP/RP coefficients. It uses the checked-in GaiaXPy 2.1.4 design matrices for BP model `v375wi` and RP model `v142r`, with `truncation=false`.

The design fixture is:

```text
crates/nsb-data-tools/tests/fixtures/gaiaxpy_continuous_design_v375wi_v142r.json
```

Loading fails closed when the version, model identifiers, band, sampling grid, matrix dimensions, merge weights, or numeric values differ from the pinned contract.

## Independent parity evidence

The frozen oracle under `crates/nsb-data-tools/tests/fixtures/gaiaxpy_oracle/` was generated independently with GaiaXPy 2.1.4 and records:

- the GaiaXPy distribution and package-content digest;
- development and holdout coefficient records;
- full calibrated spectral flux and uncertainty arrays;
- integrated 336–650 nm photon flux and statistical uncertainty;
- a signed-spectrum case containing negative samples;
- records whose relevant-basis metadata demonstrates the pinned `truncation=false` policy.

`gaia_xp_continuous_calibrate_parity` compares every wavelength, flux sample, uncertainty sample, integrated flux, and integrated uncertainty. The test cannot skip when evidence is missing or empty.

## Provenance

`nsb-data starlight xp-continuous reconstruct` writes a versioned manifest containing the Rust implementation identifier, pinned GaiaXPy reference version, BP/RP model identifiers, design-fixture path and SHA-256, band/grid contract, integration implementation, input checksums, output checksums, and signed-sample diagnostics.

The authoritative photon integration and uncertainty propagation are owned by `gaia_xp::integrate_photon_flux`; reconstruction does not carry a second integration algorithm.
