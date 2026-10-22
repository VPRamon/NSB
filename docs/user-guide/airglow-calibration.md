# Registered observatory continuum calibration

NSB can fit, export and reuse **one dimensionless continuum normalization** with
fixed PALACE spectral shape, month/local-time climatology, solar response,
atmosphere and default 88 km continuum geometry. It never treats an overall
normalization as a correction for missing lines. The optional oxygen model adds
its line flux separately, without multiplying it by the fitted continuum scale.

This workflow is **experimental**. A reproducible fit to registered observations
is different from trusted scientific admission. Public artifacts always carry
`registered-fit-not-independently-validated`; runtime component status is
`Experimental`, site status remains `PlanningPreset`, and
`is_airglow_site_calibrated()` stays false. Changing a name or TOML maturity label
cannot grant independent validation. Existing typed planning profiles remain the
user-configured path; the internal trusted admission mechanism is unchanged.

## Prepare independent measurements

Use `crates/nsb/examples/airglow-calibration/request.toml` as the schema example, replacing
all synthetic measurements, identities and provenance with your own. Supply:

- Flux-calibrated **isolated Airglow continuum**, in photons cm^-2 ns^-1 sr^-1,
  integrated over a declared top-hat **vacuum** wavelength interval within
  300–650 nm. An arbitrary filter response is unsupported. Convert energy
  spectra using photon energy at each wavelength before integrating.
- Documented removal/masking of emission lines and independently estimated
  zodiacal, stellar, lunar, artificial and other foregrounds. Their uncertainties
  must be propagated into each positive one-sigma measurement uncertainty.
  Broadband total-sky residuals alone cannot distinguish continuum normalization
  from missing emission lines or a wrong foreground estimate.
- UTC timestamps, J2000 RA/Dec, F10.7 quantity and provenance, observatory
  identity, WGS84 geodetic east longitude/latitude and ellipsoidal altitude in m.
  The wrapper's explicit ellipsoid convention governs its altitude field;
  an orthometric/MSL measurement needs an independently documented conversion.
- Fixed pressure, Rayleigh scale height, aerosol optical depth at 550 nm and
  Angstrom exponent, with uncertainty and assumptions. These are inputs, not
  jointly fitted parameters. Atmospheric uncertainties are recorded but are not
  automatically marginalized into the fitted covariance.
- Exact base model/asset checksum, observation/reference file checksums, source
  identities, licenses, measurement procedure and software/source version.
- Applicability dates, wavelength interval, F10.7 range and zenith limit at most
  60 degrees. Declare only domains supported by the observations and fixed
  spectral-shape assumption. A narrow-band fit does not validate other bands.
- Reserved evaluation night IDs. Night IDs are the local mean solar date after subtracting
  12 hours (`UTC + longitude/15 hours - 12 hours`, with longitude offset
  rounded to the nearest second). This keeps ordinary nights together across
  UTC midnight and noon at arbitrary longitudes. Continuous polar nights need
  a separately designed independence protocol. All supplied observations
  are training data; reserved-night overlap is rejected.

The fitter requires at least three positive observations on at least two
training nights and a finite, positive physical predictor. Reserve more nights
for evaluation; two clusters are only an engineering minimum, not a scientific
sample-size recommendation. Night reservation records an intended split; it
cannot establish that a dataset was never previously inspected.

## Fit and export

```bash
cargo run --locked -p nsb-data-tools --bin nsb-airglow-calibrate -- \
  --input /data/my-observatory/request.toml \
  --output /data/my-observatory/calibration.toml
```

Reference paths are normalized, relative to the request file. The utility checks
actual reference bytes against their SHA-256 values before fitting and refuses
to overwrite an existing output. Artifact loading can replay the registered
numerical fit without the original files; the artifact contains training rows
and their source checksums. Replay is not authentication of the observer's
scientific claims. Archive the original calibrated measurements separately.

The closed-form weighted fit is `a = sum(x*y/sigma²)/sum(x²/sigma²)`, with fixed
physical model predictions `x` and independent continuum measurements `y`.
The bound is `0 < a <= 100`; values outside it error rather than being clamped.
The scalar covariance is the larger of inverse weighted information and a
finite-cluster-corrected night sandwich variance. Artifact units, bounds,
method/version, fit chi-square, degrees of freedom and fitter version are
explicit. No optimizer dependency or Python callback enters runtime evaluation.
Only this one scale is identified; seasonal, local-time, solar, geometry and
species-strength parameters are deliberately not fitted. Correlated background
systematics and model discrepancy can dominate the reported covariance.

TOML serialization is deterministic. Loading denies unknown fields, validates
all domains/provenance, verifies the exact bundled base asset, replays the fit,
and rejects modified statistics, bounds, software or maturity labels. The
canonical artifact checksum is available from `checksum_sha256()`.

## Apply in Rust, Python or the CLI

```rust,no_run
use nsb::{NsbEvaluator, NsbModelConfig};
use nsb::site::AirglowCalibrationProfile;
# fn main() -> Result<(), Box<dyn std::error::Error>> {
let raw = std::fs::read_to_string("/data/my-observatory/calibration.toml")?;
let profile = AirglowCalibrationProfile::from_toml_str(&raw)?;
println!("{}: scale={} covariance={}", profile.checksum_sha256(),
         profile.scale().value(), profile.variance());
let config = NsbModelConfig::generic_clear_sky().with_airglow_calibration(profile)?;
let evaluator = NsbEvaluator::with_config(config)?;
# let _ = evaluator;
# Ok(()) }
```

```python
from pathlib import Path
import nsb
config = (nsb.NsbModelConfig.generic_clear_sky()
          .with_airglow_calibration_toml(
              Path("/data/my-observatory/calibration.toml").read_text())
          .with_solar_radio_flux_sfu(100.0))  # supply the actual supported quantity
engine = nsb.NsbEvaluator(config)
```

```bash
nsb --format json point \
  --time 2023-09-04T01:48:00Z \
  --lon -70.4044 --lat -24.6275 --height 2635 \
  --ra 266.41683 --dec -29.00781 --components airglow \
  --solar-radio-flux-sfu 100 \
  --airglow-calibration /data/my-observatory/calibration.toml
```

The coordinates/time above illustrate the synthetic fixture; use your registered
site and applicable query. Site coordinates must match within 10^-6 degrees and
height within 1 m. Date, band, F10.7 and zenith applicability are checked on point,
band and planning evaluations, including skipped/daytime samples. A profile
limited to a sub-band cannot be applied to the full-band point/planning API.
Incompatible continuum geometry is rejected at construction. Calling
`with_site_profile` afterwards explicitly replaces the profile; ordinary solar
or model selection does not promote maturity. The registered atmosphere is
shared with site-aware Moonlight propagation, preserving NSB's conventions.

Point uncertainty combines PALACE's residual variability with the fitted scale
sigma in quadrature, using only the continuum part for the scale contribution.
This assumes no covariance between fit error and future residual variability;
unknown model discrepancy is not included. Restricted-band uncertainty fields
remain `None` under the existing band API contract; the artifact covariance
remains inspectable, and unknown uncertainty is not represented as zero.

## Runnable synthetic example

```bash
cargo run --locked -p nsb --example airglow_calibration
cargo run --locked -p nsb-data-tools --bin nsb-airglow-calibrate -- \
  --input target/airglow-example/request.toml \
  --output target/airglow-example/replayed.toml
```

This generates a request, checksum-linked synthetic measurements and a portable
artifact. It recovers a planted scale of 1.2; it is **not observational evidence**.
For real accuracy claims, freeze the calibration method and training nights,
then evaluate independent, previously uninspected nights and report all errors,
cluster uncertainty and the calibration-only baseline. The inspected Patat
cohort is development evidence even when retaining its historical split.
