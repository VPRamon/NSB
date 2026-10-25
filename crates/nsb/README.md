# nsb

Night-sky background evaluation and observation planning for ground-based
astronomy.

This is the scientific runtime library for NSB. It provides typed point
evaluation and observing-window search over the production-safe composition of
zodiacal light, airglow, scattered moonlight, and an admitted integrated
starlight product when one is available.

For scientific interpretation, consult the
[versioned validation and limitations](https://github.com/VPRamon/NSB/blob/main/docs/scientific-validation.md).
Correct software execution, checksum-verified assets and agreement with another
implementation do not imply absolute observational accuracy. Automatic Airglow
uses experimental PALACE Oxygen with a zenith-distance limit of 60°.

## Installation

```bash
cargo add nsb@0.2.0
```

The minimum supported Rust version is 1.89. The `0.2.0` dependency graph
uses released Siderust 0.15.1, Tempoch 0.11.1 and Python bridges 0.5.
The versioned install command requires `nsb@0.2.0` to be available on
crates.io; until then, use a path dependency from this checkout. The corrected
Siderust Python bridge/runtime 0.5.1 is required; affected 0.5.0 is excluded.

NSB deliberately uses domain types from Siderust and tempoch at public
boundaries instead of duplicating coordinate, atmosphere, or time semantics.
Applications that construct those upstream types directly should add the
corresponding dependencies as needed.

## Cargo features

`serde` is enabled by default for backwards compatibility. It forwards
serialization support to `siderust` (including its coordinate and quantity
dependencies) and `tempoch`, covering the upstream `Observer`, `Target`,
and UTC `Time` types used at NSB's public Rust boundary.

To build without upstream Serde support:

```toml
nsb = { version = "0.2.0", default-features = false }
```

NSB's own internal TOML/JSON data formats remain available independently
of this feature; `serde` does not imply that all NSB evaluator, configuration,
or result structs implement Serde.

QTTY defines `SurfaceBrightness` (mag/arcsec²), which NSB re-exports through
`nsb::units::photometry`. This photometric newtype does not currently implement
Serde's traits upstream. Under `nsb/serde`, the
`nsb::serde_support::surface_brightness` field adapter lets applications
deserialize or serialize it as a plain JSON number without a separate wire
`f64` field or a duplicate physical type:

```rust
use nsb::units::photometry::SurfaceBrightness;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
struct NsbConstraintRequest {
    #[serde(with = "nsb::serde_support::surface_brightness")]
    minimum_nsb_in_mag_arcsec2: SurfaceBrightness,
    #[serde(with = "nsb::serde_support::surface_brightness")]
    maximum_nsb_in_mag_arcsec2: SurfaceBrightness,
}
```

The JSON remains `{"minimum_nsb_in_mag_arcsec2":21.39,"maximum_nsb_in_mag_arcsec2":30.0}`.
Application code must still validate finite, positive, ordered bounds before
converting magnitudes into profile-dependent photon radiance. This adapter
does not add validation to QTTY's type.

## Minimal Rust workflow

```rust,no_run
use nsb::{ComponentMask, NsbEvaluator, PointQuery, Target, DEG};
use siderust::catalogs::observatories;

# fn evaluate(time: tempoch::Time<tempoch::UTC>) -> nsb::Result<()> {
let evaluator = NsbEvaluator::new()?;
let query = PointQuery::new(
    observatories::EL_PARANAL.geodetic(),
    time,
    Target::new(266.41683 * DEG, -29.00781 * DEG),
)
.with_components(ComponentMask::ALL);

let result = evaluator.evaluate(&query)?;
println!("{}", result.integrated);
for component in &result.components {
    println!("{}: {}", component.name, component.integrated);
}
# Ok(())
# }
```

Construct an evaluator once and reuse it. Immutable model data and runtime
assets are prepared during evaluator construction.

## Scientific interpretation

`ComponentMask::DEFAULT` and `ComponentMask::ALL` are the same
production-safe composition for the `0.2.0` model contract. Starlight enters
that composition only when a validated production map is embedded. The
frozen candidate remains repository evidence; packages contain the approved
compact binary and sidecar, with all scientific f64 values preserved.

Observatory coordinates and scientific site assumptions are separate. The core
crate exposes generic typed site profiles; CTAO North/South planning presets
belong to application layers and are not site-calibration claims.

B/V magnitude and S10 outputs are central-wavelength diagnostics, not validated
Johnson B/V passband integrations. Each Rust/Python NsbComponent exposes
`bv_diagnostics_available`; when any selected component lacks B/V, the
frozen non-optional NsbResult B/V magnitude fields return NaN and the Rust
band-diagnostic convention ends in `-incomplete`. Missing is never silently
added as physical zero. Consult component maturity, provenance,
validated-domain, uncertainty, and asset metadata before making scientific or
operational claims.

A checksum-pinned spectral StarlightMap is a top-of-atmosphere diagnostic.
Its exact stored-bin integrals and statistical-only sigma can be inspected
without pretending they are observer-level NSB; NsbEvaluator::evaluate rejects
spectral-only maps until scientifically validated atmospheric transport exists.
The original integrated-production map path is unchanged.

## Interfaces and documentation

- Rust API: <https://docs.rs/nsb>
- Repository documentation: <https://github.com/VPRamon/NSB/tree/main/docs>
- Scientific model: <https://github.com/VPRamon/NSB/blob/main/docs/specifications/scientific-model.md>
- Model maturity: <https://github.com/VPRamon/NSB/blob/main/docs/specifications/model-maturity.md>
- Validation matrix: <https://github.com/VPRamon/NSB/blob/main/docs/specifications/validation.md>
- Python distribution: `nsb-py` on PyPI, imported as `nsb`
- CLI: sibling workspace crate `nsb-cli` (binary name `nsb`; not separately
  published in `0.2.0`)

The CLI owns command-line parsing, named-site lookup, and stable JSON/CSV
rendering. Offline catalogue acquisition and scientific data-product generation
belong to `nsb-data-tools`; the runtime library performs no catalogue downloads.

## License

AGPL-3.0-only. Scientific assets and third-party dependencies retain their own
licensing and attribution requirements; see the [notices included in this
crate](THIRD_PARTY_NOTICES.md) and the repository asset manifests.

## Scientific data and licensing

NSB **code** is AGPL-3.0-only; that does not make all scientific data AGPL.
PALACE and selected reference products use CC BY 4.0, NOAA products have their
applicable federal-work/data conditions, and Gaia-derived Starlight retains
applicable **CC BY-NC 3.0 IGO** terms. The code may be used commercially under
AGPL's obligations; commercial use of protected bundled data may need separate
rights. Credits do not imply endorsement by ESA, DPAC, CDS or the authors.

The approved compact Starlight map is embedded for fully offline evaluation.
`ALL`, `DEFAULT` and the standard Rust/Python evaluator include Starlight,
Airglow, Moonlight and Zodiacal Light. The maintainer accepted the pinned
calculated-output and independent-data findings under #103; source-specific
licences and noncommercial data restrictions remain applicable.

See the [source terms, attribution, public contact directory and concern procedure](https://github.com/VPRamon/NSB/blob/main/docs/nsb_components/starlight/licensing/README.md)
and the distributed `THIRD_PARTY_NOTICES.md`. Cite the original scientific
sources, preserve their notices and assess the data terms for your actual use.

### Experimental single scattering

`nsb::transport::single::single_scattering` provides a separate opt-in scalar
reference for synthetic wavelength-resolved TOA illumination (360–570 nm,
output zenith 0–80°). Direct and first-order spectral photon radiance are separate;
higher orders are uncomputed. Evaluator/component defaults do not use this solver.
Run `cargo run -p nsb --example single_scattering` from the repository.

`transport::single::starlight` adapts checksum-validated native spectral bins
with explicit flat photon density, local site/TT/UT1 geometry and independently
refined full-bin wavelength integration. Run the offline `starlight_transport`
example. A conservative HEALPix subpixel path resolves the frozen Gaia angular
case; the labelled historical comparison retains explicit astrometric differences. See the [spectral contract](https://github.com/VPRamon/NSB/blob/main/docs/specifications/starlight-transport.md)
for the bounded numerical method and diagnostic limitations.

The transport APIs use published Siderust 0.15.1 with the coordinated Python
0.5 bridges. This remains an experimental physical reference, with no empirical
sky-accuracy or complete transport claim.
