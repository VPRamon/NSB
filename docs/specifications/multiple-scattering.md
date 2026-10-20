# Experimental scalar atmospheric multiple scattering

Status: opt-in research reference. Numerical validation is synthetic, under the
restricted conditions below. There is no observational admission, Gaia XP
promotion, uncertainty calibration, or change to evaluator defaults.

## Physical problem and conventions

The medium is finite, plane parallel, horizontally homogeneous, and piecewise
homogeneous in height. `LayeredAtmosphere` supplies every optical coefficient
and partial-column extinction, including molecular absorption and aerosol
absorption through SSA. NSB does not implement another atmospheric prescription.
Optica supplies normalized Rayleigh, HG and double-HG phases; Siderust supplies
the strict normalized phase-table adapter. Constituents retain separate
scattering coefficients and share total extinction exactly once per path.

Horizontal vectors point **toward the apparent sky** (north-clockwise azimuth).
Photon propagation is their negative. Their mutual phase cosine is unchanged
when both vectors are negated. Positive sky-vector zenith cosine consequently
labels *downward* photons. Negative cosine labels internally upward photons.
The upper inflow is exclusively supplied TOA spectral illumination. The lower
inflow is zero: black, nonreflecting, nonemitting ground. The observer is at the
first atmospheric edge. Both internal propagation hemispheres are retained.

Supported wavelengths are 360–570 nm. Output geometric altitude is 10–90°,
as in the authoritative first-order reference. No polarization, refraction,
spherical geometry, ground reflection, terrestrial emission, airglow or
instrument response is included. Site ellipsoidal height and the atmospheric
column's declared vertical datum remain distinct caller responsibilities.

## Discrete transfer and component semantics

For scattering order n+1, with I0 the attenuated upper-boundary field,

    S_n+1(z,u) = beta_R(z) integral P_R(u,v) I_n(z,v) dOmega_v
               + beta_A(z) integral P_A(u,v) I_n(z,v) dOmega_v.

The internal field uses Gaussian zenith cosines in each hemisphere and periodic
azimuths on the full sphere. Each homogeneous upstream layer is subdivided
uniformly. Source values at cell midpoints are represented as constants inside
cells. The formal solution for each order is swept in both directions with zero
diffuse inflow; the requested output view is integrated separately and need not
coincide with an ordinate. Stable `expm1` evaluation handles thin cells.

This is a finite-cell approximation, **not an exact first-order layer integral**.
The unscattered field at midpoints is exact for the supplied boundary quadrature;
its subsequent finite-cell integration has vertical discretization error.
Independent angular/vertical studies and comparisons with the authoritative
single-scattering solver are mandatory evidence, not an assertion that midpoint
bias is zero. Equivalent layer subdivisions agree when numerical cell edges match.

Outputs distinguish unscattered `direct`, exactly once-scattered `single`,
`higher_orders` (orders 2 and above), `total_scattered`, and `total`. Higher orders
come from actual repeated scattering and attenuated transport. They never arise
from a fitted enhancement. Uncertainty is `None`; numerical changes are not
physical uncertainties.

## Order-tail guarantee and stopping

Let q bound the infinity norm of the positive *discrete* internal scattering
operator. With maximum effective SSA omega, maximum active discrete phase-row
sum p, total vertical depth T, and smallest internal |mu|,

    q = omega * p * (1 - exp(-T / min|mu|)).

Every midpoint formal response is bounded by the full-column response, so this
bounds the actual discrete operator, including nonuniform layers. Raw active
phase rows must resolve unity to `min(angular_tolerance, 1e-3)`; the ground-view
row of each active constituent is audited too. No phase row is renormalized.
A finite q strictly below one is required. Near-conservative/thick unresolved
columns may therefore be rejected even though the continuous problem has a
solution.

For the actual maximum internal radiance M_n after the nth order and ground
response bound g = omega * p_view * (1-exp(-T/mu_view)), the omitted ground tail is

    tail_n <= g * M_n / (1-q).

The ground source of order n+1 depends on I_n; hence no extra q is inserted ahead
of g. The stopping threshold is the requested absolute tolerance plus relative
order tolerance times **ground single + higher radiance**. Peak interior
radiance is not an allowed relative scale. An explicit regression compares this
bound with a much longer Neumann sum. It bounds omitted orders of the discrete
problem only, not angular/vertical error or parameter uncertainty.

Four runs compare base, angle-doubled, cell-doubled and jointly doubled grids.
Both single and higher orders must pass all independent comparisons. These
successive-grid changes are diagnostics, not certified discretization bounds.
Arbitrary unresolved oscillations in a caller's nominally smooth sky cannot be
certified by two grids. Strong phase peaks, grazing output, nonconvergence and
excessive plans produce errors; no partial result is returned.

The signed hemispheric `flux_balance_relative` diagnostic evaluates outgoing
plus absorbed minus incident photon flux, divided by incident flux (zero for
a dark sky). Absorption is integrated analytically along the formal cell
fields, independently of the midpoint source used for the next order. It
therefore exposes finite-cell/angular/order photon-closure error rather than
assuming exact discrete conservation. Pure absorption and vacuum close to
rounding error. Conservative Rayleigh closure converges with vertical cells;
see the executed evidence. This diagnostic is not an observational uncertainty.

## Conservative HEALPix boundary and native bins

`multiple::starlight::starlight_multiple_scattering` accepts the existing
checksum-validated `StarlightSky`, preserving Galactic coordinates, source grid,
product provenance, actual observer, explicit TT/UT1, exact direct-view lookup
and `WithinBinSpectrum::FlatPhotonDensity`.

The frozen pilot has seven native 30 nm bins from 360 to 570 nm. Each bin's stored
photon radiance is divided by its full width to obtain spectral photon radiance.
This is a declared within-bin model, not recovery of a measured SED. Complete-bin
Gaussian wavelength integration independently doubles its order and checks
**direct, single and higher** components. Endpoints must be covered by both the
atmosphere and phase law. No bin centers, extrapolation or cross-bin covariance
are substituted.

The incident boundary uses a separate, deterministically refined equal-area
HEALPix grid. Fine cells carry their containing parent's mean radiance and exact
solid angle. With power-of-two subdivision, each parent receives exactly the
square of the linear subdivision in children. RING enumerates those geometric
cells; their summed full-sphere integral preserves every parent. Tests cover
all parent cells through NSIDE8, isolated bright parents, poles and seams.

The upstream celestial rotation maps fine centers into the local frame. Center
clipping selects positive local cosine; antipodal pairs give at most half the
centers and exactly 2pi visible measure away from exact horizon centers. Horizon
intersection fractions are approximated by refinement, not known exactly.
Two consecutive changes in ground single/higher radiance must satisfy the
independent boundary threshold. Child samples integrate attenuation and phase
inside constant parent pixels; they are not invented subpixel stellar sources.
The internal diffuse field always uses its own full-sphere ordinate grid.
It is not a point-sampled copy of the input map. Direct view remains exact.

The pinned upstream pixel operations support **RING**, not NESTED. Native NSIDE
must be a power of two for this boundary path. Unresolved boundary/horizon
quadrature fails explicitly. Boundary changes, diffuse-angle changes, vertical
changes, wavelength changes and order tails have distinct meanings and fields.

## API, budgets and usage

The original `multiple_scattering` API remains the smooth-sky monochromatic
entry point. Settings bound angular resolution, subcells, order count and three
convergence criteria independently. `StarlightMultipleScattering` adds integrated
components, original native-bin context, independent wavelength/boundary
settings, accepted resolutions, changes and per-wavelength monochromatic results.
Existing Python and CLI interfaces are unchanged.

A monochromatic four-grid plan retains its 250 million work-unit cap. A complete
native-bin plan has a separate **four billion** cap because it includes at least
six wavelengths and three boundary levels, each with four diffuse-grid runs.
This does not raise the monochromatic cap. Both paths retain a 16 MiB temporary
field/boundary workspace ceiling. Entire worst-case plans, including levels and
wavelengths that might not run, are charged before source sampling or allocation.
Integer arithmetic is checked. Work units count bounded numerical operations,
not CPU instructions or wall-clock guarantees.

For D internal directions, K subcells, N orders and B boundary cells, diffuse
work is O(N K D^2), boundary preparation O(B K D), and temporary storage
O(D^2 + K D + B). Boundary phases are computed once per incident/output pair
and reused across vertical cells. No whole-map clone, global state, cache/LUT
system, service or runtime Python dependency is introduced.

Run the examples offline:

```sh
cargo run --release -p nsb --example multiple_scattering --locked
cargo run --release -p nsb --example starlight_multiple_scattering --locked
```

The synthetic native-bin example uses explicitly relaxed 5% successive diffuse
grid criteria, a strict 1e-4 boundary criterion and independent wavelength
criteria. Its matched independent error is below 1%; those tolerances do not
establish arbitrary-source accuracy. Defaults are not silently changed to make
an unsupported request pass.

## Validation and scientific limitations

See [executed evidence and frozen-pilot blocker](../validation/multiple-scattering-269.md)
and the checked-in DISORT/order JSON fixtures. Generators and receipts are pinned
in [nsb-validation 08b50e6](https://github.com/VPRamon/nsb-validation/tree/08b50e604589702405267f6b1ebae521dfaa1b24/scripts/multiple-scattering).

Independent stationary validation covers seven synthetic Rayleigh, HG g<=0.4,
absorbing mixed, azimuthally anisotropic, stratified and moderately thicker
columns. Separate second/third-order comparisons cover five homogeneous cases.
Tables with constant isotropic density and modest double-HG phases have upstream
contract/regression tests, not independent arbitrary-table accuracy admission.
Vacuum, absorption, zero SSA, dark boundary, equivalent splitting, residual
bounds, invalid inputs and resource rejection are tested.

This is a restricted reference, not a general DISORT replacement. Extremely
thin higher-order fields, conservative/thick media, strong forward peaks,
high-resolution maps and strict refinement requests may exceed its useful domain.
The frozen NSIDE128 Gaia multiple-scattering experiment is **not reproduced**:
its configured boundary exceeds memory before transfer, and its historical
optical prescription/center wavelengths/apparent-place astrometry differ from
the current upstream model. Exact reproduction commands and evidence are in the
validation report. Historical 28.1% enhancement remains a comparison datum only.

Registry publication remains blocked by #265: registry Siderust 0.14.1 does not
provide the required layered/phase APIs. Preserve Git revision
`83d1ebb29ca1ae0b327d5784d13eda231d52c120`, dependency identity and release guards.

Refs #187, #192, #263, #265.
