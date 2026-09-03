# Airglow component completion audit (#112)

Status: Machine-actionable acceptance audit.
Scope: Current Airglow implementation after #108, #109, #110, and #114.
Out of scope: measurement-led CTAO site calibration (#38).

| #112 acceptance area | Repository evidence and result |
|---|---|
| Generic baseline and site semantics (#108) | The standard continuum is labelled `GenericClearSky`; CTAO planning profiles retain explicit uncalibrated provenance. Geometry selection cannot change maturity. |
| Time-dependent solar activity (#109) | Explicit or bundled, offline monthly F10.7 resolution remains in the evaluation path; value, source kind, effective month, provenance, and checksum are emitted. |
| Emitting-volume geometry (#110) | `AirglowGeometryModel` exposes explicit Van Rhijn and validated vertical-profile variants. Van Rhijn remains the default and preserves its effective 90 km height and previous output. |
| Atmospheric attenuation (#114) | Noll Rayleigh/Mie effective extinction remains a wavelength-dependent stage after the emitting geometry multiplier. Metadata identifies its assumptions and extrapolation boundary. |
| Arbitrary locations | Both models accept normal `Geodetic<ECEF>` values; vertical integration consumes the real ellipsoidal observer height. Tests use non-CTAO coordinates and multiple heights. |
| Offline, deterministic evaluation | Continuum, F10.7 data, and optional profile input are local. No runtime network path was added. Persisted profiles are checksum-pinned and fail closed on mismatch. |
| Scientific metadata | Component metadata independently records maturity/provenance, F10.7 resolution, extinction assumptions, and structured geometry identity/configuration. JSON and the versioned point/window CSV schemas preserve that geometry identity, including the exact vertical-profile checksum. |
| Numerical/scientific validation | Tests protect exact default/explicit Van Rhijn parity, zenith normalization, honest failure when no emission is visible above the observer, thin-shell convergence, near-horizon stability, integration refinement, altitude sensitivity, invalid-input rejection, and independent extinction scaling. |
| Performance | Criterion coverage measures Van Rhijn, direct profile integration at two resolutions, and complete Airglow evaluation for both paths. The exact/reference integration remains available and no unmeasured cache was introduced. |
| Documentation | Runtime, profile schema, mathematics, metadata, audit evidence, limits, and issue boundaries are documented under this directory and in user/specification guides. |
| Calibration boundary | No geometry choice upgrades a result to `Calibrated`. No hidden Paranal/CTAO height or site whitelist exists. Dedicated CTAO calibration remains #38 and does not block component completion. |

## Final scope finding

The remaining uncertainty is scientific representativeness: no single global
300-650 nm vertical VER profile is supported well enough to bundle as a new
default. The generic capability is complete and accepts provenance-rich caller
profiles. Establishing and validating site-, wavelength-, and condition-specific
profiles requires observational data and is the separate #38 calibration track.
No known machine-actionable Airglow blocker remains within #112.
