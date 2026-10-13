# Adaptive NSB search contract

Normal Rust, Python and CLI threshold searches use one adaptive engine. It
supports every component mask, actual `ALL` (bits 13), and explicit Starlight +
Zodiacal + Airglow + Moonlight (bits 15), with both Jones spectral and K&S lunar
models. Starlight requires a configured admitted or explicitly experimental map.
Model formulas, spectral integration, atmospheric treatment and Siderust
astronomical event calculations retain their scientific meaning.

## Responsibility boundaries

| Module | Responsibility |
| --- | --- |
| Scientific components | Authoritative geometry, radiance, spectra and atmosphere |
| `astronomy` | Exact reusable solar/lunar state bound to one observer |
| `prepare` | Query/context validation, model preparation and eligibility |
| `filters` | Physical regime and calendar partitions |
| `discovery` / `bounds` | Empirical numerical discovery / admitted strict enclosures |
| `intervals` | Split-time ordering, coalescing and inward native UTC conversion |
| Python results / CLI time adapter | Output representability and error mapping |

The numerical engine receives radiance and bounds callbacks; it does not select
models or implement ephemerides. Language limits belong to adapters, not native
Rust interval semantics.

## Upstream quantities and geometry

The lockfile resolves Siderust 0.14.0, Tempoch/core 0.10.0, Qtty/core 0.8.5 and
Affn 0.11.0. NSB consumes quantity/coordinate types through Siderust re-exports,
without adding another Qtty or Affn dependency. Wavelength densities and integrated
photon radiance have distinct types; Optica integration supplies the wavelength
factor. Scalars remain at interpolation, transcendental and empirical-calibration
boundaries with explicit units. SFU, S10 and nanolambert conventions are domain
calibrations; they must not be treated as generic SI conversions.

Rust targets are `Direction<EquatorialMeanJ2000>`. Planning transforms them through
`TransformFrame` before calling Siderust's ICRS altitude-event provider; copying
RA/Dec into a differently tagged direction would omit frame bias. Zodiacal
geometry rotates targets into mean J2000 ecliptic axes shared by solar longitude.
Lunar positions start geocentric/ecliptic, rotate to J2000 equatorial axes, and use
Siderust's apparent topocentric and horizontal transformations. Moon/source
separation uses Affn direction geometry at the same observer; lunar distance is
topocentric, while the published phase triangle is geocentric. No tag-only center
or frame substitution is used.

Siderust's public phase API recomputes the ephemeris and cannot consume the Moon
position already used for horizontal geometry. The small position-based phase
kernel therefore remains: Vincenty elongation, Sun–Moon–Earth cosine triangle,
eastward elongation, and waxing on `(0, pi)`, following pinned upstream equations.
Qtty supplies the AU scale while preserving division order. An hourly full
lunation checks all phase fields against the authoritative upstream API, including
near-new/full Moon and both waxing conventions. This preserves expensive state
reuse without introducing approximate astronomy.

## Discovery

1. Prepare site-wide Sun/night/Moon visibility and solar activity once in a
   `SiteWindowContext`. Intersect Sun and target eligibility. Partition at
   physical horizons, PALACE night/hour/month transitions, UTC dates, F10.7
   transitions, and Zodiacal table/folding/reddening branches.
2. Fit smooth open interiors, with a 1 ms inset at regime edges. Evaluate all
   nine degree-eight Lobatto nodes. Fit degree four to the five alternating
   nodes; validate against all four interlaced nodes and two independent
   interior nodes. This minimum effort prevents the known opposite-aliasing
   regression. Physical intervals longer than six hours subdivide first.
3. When normalized residual exceeds `1e-5`, upgrade to degree eight and check
   four additional independent nodes. Subdivide if that fit remains unresolved.
   Normalization uses the largest observed absolute radiance and threshold,
   including the lower band threshold, with a `1e-12` floor.
4. Evaluate polynomial roots and derivative roots with the authoritative model.
   Near-threshold extrema use local exact golden-section searches to 0.1 ms.
   Indistinguishable extrema fail explicitly. Retain every interpolation,
   validation and candidate probe when constructing sign brackets.
5. Refine crossings with safeguarded Illinois/secant steps and periodic
   bisection until the authoritative bracket is at most 0.1 s. Export its
   accepted side. Inclusive minimum/maximum bands share the fit and exact cache;
   both final endpoints must satisfy both thresholds.
6. Preserve split coordinates through physical partitions. Native UTC, Python
   microseconds and CLI milliseconds project boundaries inward. An output
   boundary that cannot be represented within the contract returns an error.

## Exact reuse and resource bounds

Each initial physical interval owns a bounded cache of authoritative radiance
probes. A context also shares exact target-independent solar longitude and lunar
horizontal position/phase/distance across targets. Keys contain both parts of
Tempoch's TT instant; no rounded time, approximate ephemeris or interpolated
physical radiance is cached. The existing Siderust calculation computes each
miss. Target directions and component radiances still evaluate at the requested
instant. Caches belong to the context's validated evaluator/site/window identity.
The lunar cache owns its observer; callers cannot provide a different observer
for a cache lookup. A poisoned cache lock bypasses reuse and computes normally.

Each astronomical component cache admits at most 100,000 instants. Once full,
new instants compute normally. Ephemeris work occurs outside the lock; concurrent
misses can duplicate work without changing outputs. Context clones share the
cache. Context preparation and query entry/exit check Tempoch time-data identity;
a refreshed bundle requires a new context. Keep upstream time data fixed while
a query runs. These checks detect observed changes; they do not pin the bundle
inside all upstream calls and cannot establish isolation across an update followed
by restoration between checks. Concurrent searches require the same fixed-data
condition. Python result validation and its lazy datetime getter additionally
share one immutable upstream `TimeContext` for consistent civil representation.
Memory and cold-context costs are reported in [performance](adaptive-performance.md).

A query admits at most 25,000 physical windows. With
`C = sum(max(1, ceil(window_days / 0.25)))`, query limits are
`min(2,000,000, 64*C + 65,536)` exact probes and
`min(250,000, 8*C + 4,096)` fits. Each initial window additionally limits work to
4,096 probes, 256 fits and depth 32. Deterministic batches of at most 64 windows
allocate remaining budgets equally and reclaim unused work between batches.
Indexed collection selects the first error in physical order. Exhaustion,
non-finite radiance, unresolved fits or numerical uncertainty return an error;
no partial schedule is exposed as complete.

## Temporal and error contracts

TT search instants retain Tempoch split coordinates. Coalescing, physical-boundary
ordering and deduplication compare instants directly, including their low words.
Numerical discovery and solar-regime roots use local elapsed-day scalars, added
to the precise TT origin; day/second constants inside those kernels are explicit
unit boundaries, not timestamp identity. Airglow third boundaries and search
radii use typed durations.

Native accepted-side brackets avoid exporting rejected threshold values. Python
projects starts upward and ends downward to microseconds, stores the validated
projection, checks actual exported civil ordering, and rejects collapsed periods,
leap-second endpoint labels and years outside 1–9999 with `Unsupported`. A civil
round trip must remain inside the accepted native period on both UTC and TT axes;
the adapter never skips a leap second to satisfy its 0.1 s displacement limit.
CLI uses inward milliseconds and fallible UTC parsing. Neither adapter restricts
native Rust precision or imposes Python's year range on Rust.

Non-finite lunar geometry/radiance returns `OutOfRange`, rather than masquerading
as physical inactivity. Valid below-horizon zero contributions remain unchanged.
Stale contexts, unsupported certificates, unrepresentable output and unresolved
searches return explicit errors; no partial schedule is published as complete.

## Guarantees and limitations

Adaptive discovery has an empirical resolved-model contract. Agreement at fit
nodes is not a continuous-time enclosure. Arbitrarily hidden sub-node events
remain possible; local extremum searches assume a resolved unimodal basin.
Physical partition correctness is conditional on the existing astronomical event
calculations and represented branch boundaries. The validation corpus provides
observed scientific coverage, not universal mathematical completeness.

Strict certified methods reuse the same orchestration and accept only sound
whole-interval enclosures. They remain useful for the empty sum, the cached
Starlight scalar, and Moon-down interiors where the prepared evaluator skips
Moonlight. Inclusive Moon visibility edges cannot be certified as zero.
Active Zodiacal, Airglow and either Moon model lack validated temporal envelopes
and return `Unsupported`; sampling never substitutes for proof. No second
certificate search engine is maintained.

## Public compatibility

The maintainer authorized the default behavior change within NSB 0.1. Frozen
signatures remain intact: `sample_step`, its builder, CLI `--step`, and explicit
adaptive aliases are retained for source/command compatibility. Step values do
not control discovery. Python retains its legacy finite-positive constructor
validation. CLI `--adaptive` remains a compatibility alias for normal behavior.
Applications should use normal threshold methods. Removing these inputs requires
an explicit future API migration; API checks remain enabled. Strict certified
methods remain distinct because their guarantee differs from empirical search.

See [scientific validation](adaptive-validation.md) for evidence and reproduction.
