# Zodiacal source-table audit

The inherited 37 by 19 grid is a historical 5-degree S10 grid attributed to
Leinert/Levasseur-Regourd and Dumont. It should not be described as a literal
transcription of the coarser updated Table 16 of Leinert et al. (1998).
Its complete interpolation/data lineage has not been independently recovered.
The production data have therefore been preserved.

The published Table 16 gives 60 +/-3 S10 at the ecliptic pole. Eq. 18 converts
one S10 to 1.28e-8 W m^-2 sr^-1 micron^-1 at 500 nm; the pole is approximately
77 in the numerical energy units of Table 17. The current nsb2 reference file
uses Table 17 energy values but appends a polar column of 60. This mixes the
S10 and energy conventions at that added boundary. Its effect grows toward
the pole and is not evidence for changing NSB's solar spectrum or polar data.

At longitude offset 180 degrees, the inherited NSB grid contains 66 S10 at
latitude 60 degrees and 58 S10 at 75 degrees; Table 16 gives 65 and 56.
At the pole NSB uses 63 S10. The differences are small but distinguish the
datasets; they do not establish the historical dense grid as a scientific
transcription error. Low-longitude constant fill regions are extrapolations,
not published measured values. No new data product is admitted by this audit.

References:
- Leinert et al. (1998), Tables 16, 17 and Eq. 18:
  https://aas.aanda.org/articles/aas/pdf/1998/01/ds1449.pdf
- Noda et al. (1992), discussion of the older LD tabulation and its 63 S10 pole:
  https://adsabs.harvard.edu/pdf/1992ApJ...391..456N
- The pinned external implementation is nsb2 bc9320db03fbff69997ce366c6e76a37339c5d00.

The investigation bundle records a pole-unit-only counterfactual, keeping
spectral shape and interpolation fixed. That diagnostic corrects a reference
normalization; it does not modify or recalibrate either production dataset.
