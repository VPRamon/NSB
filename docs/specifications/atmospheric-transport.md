# Atmospheric transport

Status: Foundation for issue #187.
Audience: Library developers, scientific reviewers, and integrators.
Scope: Typed atmospheric propagation of spectral radiance; ownership boundary
with Siderust; component migration status.
Non-goals: Telescope/camera response (#189), Diffuse Galactic Light (#188),
full multiple-scattering RT, or silent replacement of validated component
paths.

## Pipeline

```text
sky emission
    ↓
atmospheric transport
    ↓
instrument-independent ground radiance
    ↓
future instrument response
```

Source-model identity remains independent of transport selection. Changing
transport must never rewrite a component's declared scientific source model,
and must never silently upgrade `CalibrationStatus`.

## Ownership boundary

| Layer | Owns |
| --- | --- |
| **Siderust** | Airmass formulas, Bodhaine Rayleigh optical depth, Patat Mie optical depth, Beer–Lambert `transmission`, ozone transmittance table, Rayleigh / tabulated phase functions, `AtmosphereProfile` |
| **NSB `site`** | Site-profile selection, `AtmosphericConditions`, maturity / calibration metadata |
| **NSB `transport`** | Transport model selection, direct / identity composition, radiance-origin checks, transport metadata, future scattered-path orchestration |
| **NSB components** | Source emission models and any still-legacy component-specific propagation |

Missing *generic* optical primitives belong in Siderust before an NSB-local
substitute is added. NSB must not grow a second atmosphere-kernel framework.

The public module is `nsb::transport` (not `nsb::atmosphere`) so it does not
collide with the forbidden public `atmosphere` module name reserved against
exposing the internal site-atmosphere implementation module.

## Models in this foundation

| Model | Contract |
| --- | --- |
| `TransportModel::Identity` | `output == input` exactly; atmosphere ignored |
| `TransportModel::Direct` | `I = I₀ exp(−τ X)` with selectable Rayleigh / Mie / ozone ingredients and a selectable Siderust airmass formula |
| Scattered path | Explicitly `NotImplemented` (API scaffolding only) |

Direct extinction and single in-scattering are separate operations. The
scattered-path API exists so HEALPix sampling, phase-function mixtures, and
LUT solvers can land later without redesigning the direct path.

## Radiance origins

| Origin | Meaning | Celestial direct path? |
| --- | --- | --- |
| `TopOfAtmosphere` | Zodiacal, starlight, future DGL | Yes |
| `AtmosphericEmission` | Airglow (emitting volume inside atmosphere) | No |
| `PreScatteredAtmosphere` | Legacy Jones / KS91 moonlight | No |

## Component migration status

| Component | Runtime behaviour change in #187? | Notes |
| --- | --- | --- |
| Zodiacal | **No** | Still uses `ZodiacalExtinction::{None,Noll2012Approx}`; source ⊥ propagation preserved |
| Starlight | **No** by default | Admitted HEALPix product unchanged; callers may apply `transport` to TOA radiance copies |
| Moonlight | **No** | Jones/KS91 retain validated embedded scattering; generic transport would double-count |
| Airglow | **No** | Retains Noll in-atmosphere scattering and Van Rhijn / profile geometry |

## Metadata

`TransportMetadata` records model identity, path kind, atmosphere/profile id,
extinction and scattering ingredient flags, airmass model, approximation state,
validated domain, provenance, and uncertainty reporting (currently `Absent`).
Supplying a named atmosphere profile id does **not** imply site calibration.

## Validation

See `crates/nsb/tests/atmospheric_transport.rs` for identity, Beer–Lambert,
monotonicity, origin-rejection, starlight non-mutation, Rayleigh phase
symmetry, and a labelled **cross-implementation validation** against the nsb2
plane-parallel Beer–Lambert extinction contract under matched assumptions.

## References

- Bodhaine et al. (1999), Rayleigh optical depth.
- Patat et al. (2011), Paranal aerosol extinction.
- Noll et al. (2012), Cerro Paranal ASM (legacy component paths).
- Roellinghoff et al. (2025), nsb2 architectural comparison (not a dependency).
