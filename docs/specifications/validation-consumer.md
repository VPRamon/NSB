# Independent validation consumer contract

New campaigns must identify the exact NSB source commit, package version, dirty
state, build features, lockfile and scientific asset checksums. A package
version alone cannot identify distinct source revisions or locally built wheels.
Retain the installed wheel/sdist hashes, toolchain and dependency provenance;
`SIDERUST_VERSION` and `SIDERUST_SOURCE` identify the upstream implementation.
Never relabel a frozen campaign after rebuilding a different revision.

Use public Rust/Python evaluator APIs and the CLI serialization contract rather
than private component internals. The canonical contracts are:

- [Restricted-band radiance](restricted-band-radiance.md): component-separated
  results, wavelength support, bounds, unavailable spectra and uncertainty.
- [Scientific metadata](scientific-metadata.md): model maturity and provenance.
- [CLI schemas](cli-schemas.md) and [Python interface](../python.md): exported
  representations, errors and cross-interface behavior.
- [Atmospheric transport](atmospheric-transport.md): physical origins and
  propagation assumptions; scattering remains explicitly opt-in.

Radiance is ground-level photon surface radiance in ph cm^-2 ns^-1 sr^-1;
full-band evaluation spans 300–650 nm. Restricted bands must remain within each
selected component's declared support. Preserve selected components, site,
UTC/TT/UT1 conventions, direction, atmosphere and calibration choices in receipts.
Do not infer a Starlight spectrum from its integrated production map. Unsupported
spectral evaluation is an error, not zero. Below-horizon Moon can be a physical
zero; missing contributions or uncertainties are unavailable, not measurements
of zero. Never invent cross-bin covariance or interpret numerical tolerance as
observational uncertainty. B/V central-wavelength diagnostics are not synthetic
passband photometry.

Keep raw component outputs, metadata and failures alongside any research-only
observable conversion. Document its response, wavelength integration, extinction
and calibration assumptions. Compare Rust, installed Python and CLI on equivalent
inputs and supported observables before scoring accuracy. Internal interface
agreement verifies software consistency; independent, admitted observations are
required for predictive-accuracy claims. Historical and current evidence is
summarized in [the validation matrix](validation.md).
