# Atmospheric transport

Status: Foundation for issue #187 / PR #193.
Audience: Library developers, scientific reviewers, and integrators.
Scope: Typed atmospheric propagation of **wavelength-resolved** radiance;
ownership boundary with Siderust; current component integration status.
Non-goals: Telescope/camera response (#189), Diffuse Galactic Light (#188),
full multiple-scattering RT, or silent replacement of validated component
paths.

## Pipeline

```text
source spectral radiance
    ↓
atmospheric transport (identity | direct extinction)
    ↓
ground spectral radiance
    ↓
future integration / instrument response
```

Source-model identity remains independent of transport selection. Changing
transport must never rewrite a component's declared scientific source model,
and must never silently upgrade `CalibrationStatus`.

## Supported public capabilities

| Capability | Contract |
| --- | --- |
| `TransportModel::Identity` | `output == input` exactly; atmosphere ignored; all radiance origins accepted |
| `TransportModel::Direct` | `I(λ) = I₀(λ) exp(−τ(λ) X(z))` with selectable Rayleigh / Mie / ozone and a Siderust airmass formula |
| Origin-checked spectral apply | `apply_energy_spectral` / `apply_photon_spectral` require [`RadianceOrigin`] |
| Inspectable atmosphere properties | `transmission(...)`, `optical_depth(...)` |
| Model metadata | `TransportModelMetadata` describes the configuration only (no atmosphere-profile string) |

Direct geometry is validated at construction: zenith ∈ `[0, 90]` degrees, finite.
Wavelengths must be finite and strictly positive. Values are never silently
clamped.

## Not exposed in the public API yet

These remain architectural goals, not frozen Rust symbols:

- single in-scattering request/result types;
- HEALPix scattering sampling;
- LUT / accelerated solvers;
- public wrappers around Siderust phase functions.

Direct extinction and future single in-scattering will remain **separate
concepts** when a solver lands. Placeholder scattering APIs are intentionally
absent so the eventual contract is not locked prematurely.

## Ownership boundary

| Layer | Owns |
| --- | --- |
| **Siderust** | Airmass formulas, Bodhaine Rayleigh, Patat Mie, Beer–Lambert `transmission`, ozone table, Rayleigh / tabulated phase functions, `AtmosphereProfile` |
| **NSB `site`** | Site-profile selection, `AtmosphericConditions`, maturity / calibration metadata |
| **NSB `transport`** | Transport model selection, origin policy, model metadata, spectral radiance application |
| **NSB components** | Source emission models and component-specific propagation not yet represented by generic transport |

Missing *generic* optical primitives belong in Siderust before an NSB-local
substitute is added. The public module is `nsb::transport` (not
`nsb::atmosphere`) so it does not collide with the forbidden public
`atmosphere` module name.

## Radiance origins

| Origin | Meaning | Celestial direct path? |
| --- | --- | --- |
| `TopOfAtmosphere` | Zodiacal, starlight, future DGL | Yes |
| `AtmosphericEmission` | Airglow | No (direct path rejected) |
| `PreScatteredAtmosphere` | Jones / KS91 moonlight | No (direct path rejected) |

Identity transport accepts every origin (no atmosphere is applied).

## Component integration status

| Component | Runtime behaviour change? | Notes |
| --- | --- | --- |
| Zodiacal | **No** | Still uses `ZodiacalExtinction::{None,Noll2012Approx}` |
| Starlight | **No** | Admitted HEALPix product unchanged; band-integrated radiance is **not** a public monochromatic transport input |
| Moonlight | **No** | Jones/KS91 retain validated embedded scattering |
| Airglow | **No** | Retains Noll in-atmosphere scattering and emitting-volume geometry |

## Metadata

`TransportModelMetadata` records immutable model facts: model identity,
extinction ingredients, airmass formula, approximation class identity,
validated domain, provenance, and uncertainty policy identity (`absent`).

It does **not** accept a caller-supplied atmosphere-profile identifier. Atmosphere
identity belongs with the evaluation context that owns the actual
`AtmosphericConditions` (for example a `SiteProfile`). Inventing provenance by
passing an unrelated string is therefore impossible through this API.

## Validation

See `crates/nsb/tests/atmospheric_transport.rs` for identity, Beer–Lambert,
monotonicity, origin rejection, geometry/wavelength validation, airmass and
ingredient coverage, and a labelled **cross-implementation validation** against
the nsb2 plane-parallel Beer–Lambert extinction contract under matched
assumptions.

## References

- Bodhaine et al. (1999), Rayleigh optical depth.
- Patat et al. (2011), Paranal aerosol extinction.
- Noll et al. (2012), Cerro Paranal ASM (component reference).
- Roellinghoff et al. (2025), nsb2 architectural comparison (not a dependency).
