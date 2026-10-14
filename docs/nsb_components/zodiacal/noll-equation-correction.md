# Noll effective-extinction equation correction

Noll et al. (2012), A&A 543 A92, Eqs. 1, 4, 18-20 and Fig. 7:
https://doi.org/10.1051/0004-6361/201219040

The fitted intensity argument is the 500 nm source brightness in numerical
units of 1e-8 W m^-2 sr^-1 micron^-1. It is held fixed across the transported
spectrum. The previous code instead took log10 of dimensional brightness at
each transported wavelength, omitting the 1e-8 normalization. It also used
ln(transmission) as a positive optical depth, and applied both species factors
to the aerosol depth alone. These independent errors partially cancelled.

Correct composition:

    dex = log10(I_500 / (1e-8 W m^-2 sr^-1 micron^-1))
    tau_M = 0.4 ln(10) k_aerosol
    tau_eff = X (f_R(dex) tau_R + f_M(dex) tau_M)
    T_effective = exp(-tau_eff)

The published aerosol coefficient is 0.013 lambda_micron^-1.38 mag/airmass,
with 0.050 below 0.4 micron. Molecular depth reuses NSB's shared Siderust
Bodhaine/local-pressure primitive at the fixed 744 hPa reference column.
This retains a fixed reference atmosphere; the current Copy selector does not
carry site conditions. It does not duplicate generic atmospheric transport
(issue #187) or claim arbitrary-site calibration. Young 1994 airmass is retained
as an explicitly documented approximation to Noll's Rozenberg formulation.

Faint-field factors can be negative: atmospheric in-scattering can make
T_effective exceed one, so the result must not be clamped like direct transmission.
The published fit mainly covers astronomical night, target zenith <=50 deg,
extended to 70 deg at high brightness. Daytime/high-zenith cases remain
extrapolation diagnostics. Molecular absorption is not introduced by this fix.

The earlier Sgr A* regression tuple (band, B S10, V S10) was
(0.07324982482125132, 61.915204368020945, 84.82413644987089).
The corrected tuple is
(0.139361113489048, 132.4445350790107, 153.8433624221859).
Strict numerical tolerances are retained. The old TSIS-to-Planck impact bounds
are still tested with the historical propagation held fixed in a test-only
counterfactual; they cannot be applied to a simultaneous physics correction.
Independent published-equation tests cover both fit branches, molecular and
aerosol wavelength behavior, net in-scattering, airmass sensitivity and zero
source. Agreement with a direct-only nsb2 atmosphere is not the acceptance test.
