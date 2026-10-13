# Adaptive NSB search contract

Normal Rust, Python and CLI threshold searches use one adaptive engine. It
supports every component mask, actual `ALL` (bits 13), and explicit Starlight +
Zodiacal + Airglow + Moonlight (bits 15), with both Jones spectral and K&S lunar
models. Starlight requires a configured admitted or explicitly experimental map.
Model formulas, spectral integration, atmospheric treatment and Siderust
astronomical event calculations retain their scientific meaning.

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

Each astronomical component cache admits at most 100,000 instants. Once full,
new instants compute normally. Ephemeris work occurs outside the lock; concurrent
misses can duplicate work without changing outputs. Context clones share the
cache. Context preparation and query entry/exit check Tempoch time-data identity;
a refreshed bundle requires a new context. Keep upstream time data fixed while
a query runs. Memory and cold-context costs are reported in [performance](adaptive-performance.md).

A query admits at most 25,000 physical windows. With
`C = sum(max(1, ceil(window_days / 0.25)))`, query limits are
`min(2,000,000, 64*C + 65,536)` exact probes and
`min(250,000, 8*C + 4,096)` fits. Each initial window additionally limits work to
4,096 probes, 256 fits and depth 32. Deterministic batches of at most 64 windows
allocate remaining budgets equally and reclaim unused work between batches.
Indexed collection selects the first error in physical order. Exhaustion,
non-finite radiance, unresolved fits or numerical uncertainty return an error;
no partial schedule is exposed as complete.

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
