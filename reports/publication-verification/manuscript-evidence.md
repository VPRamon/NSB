# Integration into publication-readiness PR #273

Engineering supplement for the manuscript draft inspected at
`8e0bb493bc8d6c8615032f5434f6362e9ba92f35`. Do not overwrite that draft or alter
its historical observational section. Apply the following text only after
reviewing the executed receipts. Candidate identity and current verification
status are in the accompanying JSON manifests; release #265 remains open.

## Insert into section 4 (implementation and numerical verification)

The publication candidate was tested using analytic constant and linear spectral
densities over irregular and clipped wavelength grids, with a maximum absolute
integration error of 5.684341886080802e-14 photons cm^-2 ns^-1 sr^-1 on the
synthetic test densities (absolute tolerance 1e-10). For a normalized quadratic
density over 300–650 nm, the composite trapezoid error agreed with the exact
350/(6 n²) expression for n=8,16,32,64; each doubling reduced the discretization
error by a factor of four. This establishes quadrature-kernel behavior, not a
physical uncertainty estimate or convergence of all fixed component grids.

The typed conversion from photons cm^-2 ns^-1 sr^-1 to photons m^-2 s^-1 sr^-1
reproduced the factor 1e13 exactly in the recorded environment. Direct transmission
at three wavelengths and four zenith distances matched exp(-tau/cos(z)) to the
reported floating-point precision, with absolute tolerance 2e-14; the optical
depth is an input to this algebraic check, not an independently validated
atmospheric model. A separate 16-case photon-energy/S10 regression uses exact SI
h and c and has maximum relative error 1.816422439628186e-16 (tolerance 3e-15). It exposed and corrected a ~1.98e-14 relative typo in a private hc
constant, affecting diagnostic S10 values only. Band-integrated photon outputs
and historical observational conclusions are unchanged.

## Insert into section 3 (interfaces)

Nine shared UTC/ICRS/geodetic scenarios were evaluated through native Rust,
an installed PyO3 wheel and CLI JSON. The selected common subset comprises
explicit Paranal PALACE continuum, Zodiacal and physically zero Moon point
radiances. All three interface pairs agreed exactly for the compared point
totals and component radiances in the recorded Linux environment; tolerances
were 1e-14 absolute plus 2e-12 relative in photons cm^-2 ns^-1 sr^-1. Rust and
Python also agreed for four restricted-band integrals. Full-band and partition
identities had a maximum absolute residual of 8.326672684688674e-17 in the final candidate
measurement; the evidence JSON records each comparison. Repeated evaluations
were deterministic and zero F10.7 was rejected. These results cover this subset,
not the complete API, every architecture, or absolute coordinate accuracy.

## Scientific example (section 4 or new short subsection)

At UTC 2023-09-15T05:00:00Z, longitude −70.4045°, latitude −24.6272°, ellipsoidal
height 2635 m and ICRS (10°, −20°), the existing PALACE continuum model yields
0.08791574137848508 to 0.09025928936024186 photons cm^-2 ns^-1 sr^-1 integrated
over 300–650 nm as explicit F10.7 increases from 67 to 166 sfu. Six fixed solar
inputs reproduce the model's monotonic response at this geometry. These are
computed planning-model outputs, not measured sky radiances; their printed
precision is numerical and is not an uncertainty claim. Oxygen lines, other
sky components, telescope response and site recalibration are excluded from
the plotted continuum sweep.

**Figure caption:** Existing PALACE v1 continuum-airglow model at fixed Paranal
geometry (UTC 2023-09-15T05:00:00Z; ICRS RA 10°, Dec −20°). Points show the
300–650 nm integrated photon radiance at six explicit F10.7 inputs, using the
candidate's generic-clear-sky atmosphere and 88-km van Rhijn geometry. The line
joins computed points; no physical uncertainty band or observational accuracy
claim is implied. Inputs, numerical values and regenerated vector/PNG figures
are in `examples/publication/`.

## Claim-to-evidence table (section 4)

| Publication claim | Executed verification / canonical evidence |
| --- | --- |
| Source reproducibility | Locked Rust/CLI and installed-wheel build; isolated sdist and clean-source receipts in verification-results.json; software-provenance.json |
| Numerical correctness within tested contracts | numerical-verification.json, photon-si-reference.json and workspace log; contracts/tolerances in numerical-verification.md |
| Consistency of selected interfaces | interface-equivalence.json, shared examples/publication/config/fixtures.json; capability matrix in interface-equivalence.md |
| Scientific usability | examples/publication/run.py, expected/results.json and generated figures; end-to-end numerical regression |
| Registry release readiness | **Not established: FAIL/BLOCKED** until real upstream Siderust release and ordinary archive/publish dry-run pass |

## Replace prospective availability text in section 7

The candidate is identifiable by its full source commit and Cargo.lock digest
recorded in software-provenance.json. The source contains fixed scientific
inputs, registered computed outputs, an offline verification command and plotting
code. Provenance records toolchains, platform, Python package identities,
binary and data hashes. The later evidence commit contains run receipts and
manuscript integration notes; it does not change scientific executable sources.
Repository builds and source distributions require the exact pinned Siderust Git
source. This candidate is not a new crates.io/PyPI release; issue #265 remains an
external release gate. No all-platform release certification is claimed.

## Remaining submission gates

A compatible upstream Siderust registry release, refreshed dependency requirements
and successful ordinary archive/publish dry-run are needed to claim registry
release readiness. Review remote CI results for the candidate, have scientific
authors review the scope/tolerances/figure and SI correction, integrate these
statements into #273, and complete references, authorship, licensing/availability
and required AI-assistance disclosures. Historical Patat/DESI/LVM findings remain
as written; protected LVM observations remain sealed. No superiority or
independent observational accuracy follows from this engineering evidence.

## Executed test summary for insertion

The final source candidate is `a2ae1274f401a56e73d87350b41d025a5fff0928`.
The workspace run passed 1,074 tests with seven existing optional diagnostics
ignored; the installed release wheel passed 140 Python tests. An independently
built isolated-sdist wheel passed 64 public-API checks outside the checkout.
Format, all-target/all-feature Clippy with warnings denied and a Rust 1.89
compatibility check passed. A fresh source-archive end-to-end build reproduced
the registered data and figure. Ordinary Cargo archive verification and publish
dry-run failed on the absent Siderust registry APIs; registry release readiness
remains explicitly unestablished.
