# Phase 5 XP continuous uncertainty contract

## Problem (policy v0)

Overlap validation compared `|sampled − reconstructed|` against **absolute**
GaiaXPy reconstruction uncertainty (`sigma_recon`). Because XP sampled and XP
continuous share Gaia calibration, absolute uncertainties (~10² ph m⁻² s⁻¹) dwarf
typical differences (~10⁻¹ ph m⁻² s⁻¹), yielding **coverage_68 = coverage_95 = 1.0**.

Flux reconstruction gates passed; only the uncertainty contract was wrong.

## Two contracts (policy v1)

| Contract | Use | Formula (summary) |
|----------|-----|-------------------|
| **overlap_difference_uncertainty** | XP sampled vs XP continuous overlap gates | `sigma_diff = inflation * hypot(sqrt(2)*sigma_recon*sqrt(1-rho), max(floor, q68_rel*|sampled|))` |
| **absolute_physical_uncertainty** | Integrated starlight product runtime | `sigma_abs = hypot(sigma_recon_stat, max(floor, fraction*|flux|))` |

Coverage gates apply **only** to `overlap_difference_uncertainty`.

## Fitting (train + validation only)

1. **Train:** `relative_residual_scale = p68(|Δ/sampled|)`.
2. **Validation:** grid-fit `inflation_factor` to target 68% difference coverage.
3. **Holdout v1:** single evaluation with frozen `phase5_frozen_validation_policy_v1.json`.

Exploratory v0 archived under:
`phase5-policy-v0-exploratory-no-explicit-uncertainty-model/`.

## Implementation

- `crates/nsb-data-tools/src/starlight_phase5_uncertainty.rs`
- `freeze_phase5_validation_policy_v1`
- `run_phase5_holdout_v1_validation`
