# Experimental Gaia XP 360–570 nm product

This development path creates a new, explicitly selected **XP-only** candidate.
It is not a complete stellar-population product and is not production-admitted.
Existing production product schemas and data remain unchanged.

The dataset configuration uses `product_band = "measured-360-570"`. The ordinary
Rust dataset lifecycle owns acquisition, content-addressed SHA-256 verification,
SLURM partition workers, checkpoint receipts and reconciliation. New interval
shards have band-neutral fields; legacy calibrations and shards are rejected.

The calibrated v375wi BP design matrix from the pinned GaiaXPy 2.1.4 fixture is
integrated as a linear functional of the source coefficients, with clipped
piecewise-linear photon-density trapezoids. This retains fractional wavelength
boundaries and the complete published within-BP coefficient covariance. The
360–570 nm interval lies below the BP/RP merging region, so no unknown BP/RP
cross covariance is introduced. Seven contiguous 30 nm integrals preserve
spectral information for subsequent direct atmospheric transmission bounds.
Photon source flux is in photons/(m² s); dividing by the Galactic nested pixel
solid angle and multiplying by 1e-13 gives photons/(cm² ns sr).

Every joined GaiaSource row is admitted once or assigned an exclusion reason.
Duplicate GaiaSource entries and nonstellar candidates use the existing policy.
Repeated XP identifiers fail the worker. Sources whose integrated sub-band
fluxes are not all positive are excluded rather than clipped. This positivity
selection can bias faint populations and is explicitly part of the candidate's
population identity. The map must not be described as population-complete.

Non-XP sources have no band-calibrated fallback at this stage. Existing
336–650 nm photometric coefficients are not reusable. Selection corrections
and bright-star supplements are disabled until their restricted-band contracts
have been established. Missing systematic calibration uncertainty is `null`,
not zero. The map describes static Gaia DR3 mean spectra and positions, not
instantaneous variable-star brightness or proper-motion-corrected apertures.

This map is above the atmosphere and averages discrete stellar flux over an
NSIDE=128 pixel. It is not directly equivalent to a DESI source-masked sky-fiber
measurement. Neither a ground-level total prediction nor an accuracy-improvement
claim is admitted by generating this candidate alone. Atmospheric scattering,
source masking, population completion, and the DESI sky-model angular operator
must be resolved separately.

No default asset, manifest, production admission, licensing authorization or
redistribution policy is changed. Raw inputs and generated scientific products
remain outside Git in the project-owned BeeGFS workspace.
