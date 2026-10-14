# Typed quantities in the corrected radiometric paths

The Jones scattering kernel takes qtty optical depths, airmasses, a normalized
Rayleigh phase density and the dimensionless tabulated Mie factor. Its result
has the structural unit spectral-radiance / spectral-irradiance (per steradian).
The source uses the typed irradiance API from released Siderust 0.14.1.
Multiplying lunar irradiance by this kernel recovers spectral radiance through
qtty's unit algebra, before the energy-to-photon conversion.

The Noll brightness logarithm is taken only after division by the typed
1e-8 W m^-2 sr^-1 micrometre^-1 reference. The aerosol wavelength law and zodiacal
reddening logarithm likewise use dimensionless wavelength ratios. Extinction
coefficients, optical depths, transmittances, wavelengths and B/V fallback
radiances retain their quantity types across helper boundaries. Solar irradiance
is converted to mean radiance by a typed projected solid angle of pi steradians.

Remaining scalars are empirical coefficients, interpolation coordinates/raw
storage at the optica table boundary, or dimensionless multipliers. qtty 0.8's
named dimensionless markers do not implement general cross-unit multiplication;
the code extracts those multipliers locally while retaining the physical result's
unit. Numerical test adapters use scalars to keep independent quadrature and
published equation references transparent. There is no new placeholder unit,
production calibration or source table normalization.
