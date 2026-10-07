# PALACE v1 continuum runtime product

Status: release evidence for issue #214.  
Product: `crates/nsb/data/airglow_palace_v1.dat`.  
Schema: `nsb-airglow-palace-continuum-v1`.

## Source and redistribution

The exact source is PALACE v1.0, Zenodo record
[14064023](https://zenodo.org/records/14064023), DOI
`10.5281/zenodo.14064023`. The downloaded `PALACE.zip` has SHA-256
`a42577ffee8f76d9ac3765a9a2811d766c30158cf74c0a364ebf7a726dd9a0dc`.
The release README distinguishes the GPL-3.0-or-later Python/Cython program from
the model data (`data/palace_*.fits`), which are CC BY 4.0. The Zenodo record is
also marked CC BY 4.0 and the model paper's code-and-data availability statement
confirms CC BY 4.0 for data and GPLv3 for code.

NSB reads only these CC-BY-4.0 members:

| Input | Scientific role |
|---|---|
| `PALACE/src/palace/data/palace_cont.fits` | Three unresolved-continuum templates, reference radiance in R/nm, variability-class identity, and layer heights |
| `PALACE/src/palace/data/palace_var.fits` | 12 month × 12 local-mean-solar-time climatology: relative mean, F10.7 slope, residual variability, and nighttime weight |

`palace_lines.fits` is deliberately excluded: this product replaces NSB's
continuum component, not PALACE's 26,541-line emission model. NSB neither copies
nor executes PALACE's GPL program.

## Scientific mapping

PALACE covers 0.3–2.5 µm and separates resolved lines from three unresolved
continuum components: HO2, FeO-like emission (including other molecules), and
unresolved O2. For the NSB 300–650 nm band the runtime product stores each
continuum template on an exact 1 nm grid, together with its PALACE variability
class and source layer height (81, 88, and 94 km).

For component `c`, calendar month `m`, PALACE local-time bin `t`, and F10.7
value `F` in sfu, NSB preserves PALACE equation (1):

```text
scale(c,m,t,F) = rI(c,m,t) × [1 + 0.01 × SCE(c,m,t) × (F - 100)]
continuum(λ)   = Σ template(c,λ) × scale(c,m,t,F)
sigma(λ)       = Σ template(c,λ) × rI(c,m,t) × rdI(c,m,t)
```

PALACE equation (2) therefore scales the residual standard deviation by the
bin-specific mean factor `rI` but applies no additional solar-activity term.
PALACE sums overlapping component deviations linearly, so this runtime
uncertainty is a conservative maximum rather than a quadrature combination.

PALACE fitted bins 1–12 to Cerro Paranal local mean solar time over
18:00–06:00 in one-hour steps. For NSB's arbitrary-location `planning-proxy`
use, the observer's local mean solar month/hour is mapped onto the equivalent
PALACE bin. This preserves time-of-night phase under longitude transfer, but it
is an explicit spatial extrapolation rather than a claim that PALACE was fitted
at the observer longitude. Astronomical-night samples outside 18:00–06:00 use
the nearest endpoint and are an additional temporal extrapolation. The model paper reports that its X-shooter training
sample's centred 27-day F10.7 averages span 67–166 sfu. NSB records that evidence
range but does not silently clamp user input.

PALACE atmospheric absorption/scattering is not baked into the product. NSB
continues to apply its documented Noll-2012 effective Rayleigh/Mie stage and
selected emitting-volume geometry once at runtime. The current geometry API is
scalar and uses 88 km by default; the asset retains all three component heights
for future component-resolved geometry. This approximation is visible in
runtime metadata.

## Reproducible generation

The four lifecycle commands are recorded verbatim in `manifest.toml`. `update`
downloads and verifies the pinned archive. `build` uses NSB's Rust FITS binary-
table reader, validates the exact PALACE v1 table shapes and canonical row
ordering, samples the 0.02 nm templates at exact 1 nm nodes, and writes fixed
scientific-notation text in deterministic order. No timestamp, hash-map order,
locale, PALACE executable, Python, or live runtime request affects the bytes.

Two independent in-process generations from the pinned archive were
byte-identical and matched the committed product. The regression is
`generator_is_byte_deterministic_for_pinned_archive_when_available`; normal PR
CI validates the committed product offline, while the Scientific validation
workflow downloads the pinned archive, runs `update → build → validate`, and
byte-compares the regenerated product with the committed asset. Output:

```text
bytes   34,959
sha256  f03b48cce44764c05e7208e59a0c9dbbb773ffc4b32444f8157dc5ac36fd119e
```

## Structural and reference validation

The generator and runtime parser independently require:

- schema and unit identities;
- exactly three components, 351 wavelengths, and 144 climatology rows;
- a strictly canonical 300–650 nm, 1 nm grid;
- canonical month/time ordering;
- finite, non-negative continuum, mean, residual-sigma, and nighttime-weight
  values; finite solar slopes; and
- manifest SHA-256 equality before compilation.

As a cross-implementation check, the official PALACE v1.0 functions
`readdata → calcscalfac → scalecont → calccontspec` were run for September,
time bin 3, 100 sfu, zenith, no atmosphere, vacuum wavelengths, and a 1 nm
grid. Its 300–650 nm continuum integral was `1221.712448 R`; the NSB product
gave `1221.711975 R` (relative difference `3.88e-7`, caused by printed float
precision). Values at 300, 445, 551, and 650 nm were also finite and positive.

For the residual-variability path, PALACE equations (1) and (2) were evaluated
directly from the committed January / local-time-bin 2 coefficients at 550 nm.
The expected continuum is `2.757462436877382 R/nm` at 100 sfu and
`3.162935499374538 R/nm` at 160 sfu, while the residual standard deviation is
`1.1181690988460096 R/nm` at both solar-flux values. The runtime regression
pins those values so omitting the required `f0` factor, or incorrectly applying
the solar term to the residual deviation, fails.

Runtime tests exercise month, time, and solar changes, parser rejection,
atmospheric sensitivity, geometry scaling, 300–650 nm integration, B/V
diagnostics, uncertainty, and full/integrated-path parity.

## Diagnostic comparison with the removed historical asset

The historical `airglow_cont.dat` was read only from Git history for this
diagnostic. It was not a generator input. The table compares vertical,
unattenuated 300–650 nm photon radiance in `ph cm^-2 ns^-1 sr^-1`; old
three-night-phase bins were paired with representative PALACE local-time bins.

| Case | F10.7 | PALACE/NSB | Historical | Ratio |
|---|---:|---:|---:|---:|
| September early night | 100 | 0.09722 | 0.08900 | 1.09 |
| September middle night | 100 | 0.08281 | 0.07828 | 1.06 |
| September late night | 100 | 0.08981 | 0.12311 | 0.73 |
| January middle night | 100 | 0.07656 | 0.07204 | 1.06 |
| July middle night | 100 | 0.07100 | 0.08350 | 0.85 |
| September middle night | 67 | 0.08228 | 0.05896 | 1.40 |
| September middle night | 166 | 0.08387 | 0.11693 | 0.72 |

At 100 sfu several representative cases agree to roughly 6–27%, but the solar
response and late-night behaviour differ substantially. This is expected: the
historical file imposed one wavelength-independent solar law and coarse
6-season × 3-phase factors on a fixed FORS1-era shape, whereas PALACE provides
component-, month-, time-, and solar-dependent climatologies derived from ten
years of X-shooter data. No PALACE coefficient was tuned to reproduce the old
snapshot.

## Limitations and maturity

This is a Paranal-trained, continuum-only planning model. It is not a global
climatology, a line-complete airglow spectrum, a CTAO calibration, or a claim of
short-timescale predictability. Residual variability is reported, but site
transfer and the scalar 88 km geometry approximation are not fully represented
by that uncertainty. Molecular absorption and PALACE's own atmospheric
propagation are omitted. These limitations keep the asset at `planning-proxy`.
