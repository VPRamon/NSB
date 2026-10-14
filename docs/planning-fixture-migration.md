# Planning fixtures after the lunar and zodiacal equation corrections

The planning regressions originally used the radiance curves produced before
correcting the Jones scattering column and Noll zodiacal transport. A threshold
is a test input, not an observational calibration. The physical correction can
remove an old crossing or change a peak into a trough. Regenerating these inputs
must preserve the discovery challenge and independently witness the new event.
NSB now resolves the complete lunar correction through released Siderust 0.14.1.
No production calibration, scientific acceptance tolerance or search tolerance
is changed by this migration.

All thresholds below are integrated photon radiance in
photons cm^-2 ns^-1 sr^-1. Times used to reconstruct fixtures are TT MJD.

## Independent reconstruction

- **Accepted islands:** keep the original site, target and annual scan120 grid
  phase. For each physical partition containing the old island, evaluate the
  authoritative radiance eight seconds before its right edge. Freeze that
  radiance as the new threshold. The test independently scans both boundaries
  at 1 ms, checks that scan120 misses the island, and verifies accepted-side
  samples throughout it. The right edges are 61166.04351066056 (south/KS91),
  61177.13383314815 (north/KS91), and 61183.05049981482 (north/Jones).
- **Moon/airglow narrow event:** locate the smooth minimum near MJD 61046.922
  using independent scalar samples and a bracketed extremum search, not the
  adaptive discovery output. Average its radiances at +/-1.2 seconds to form
  the threshold. This replaces the old rejected peak with an accepted trough.
  Its width must still be 2.3–2.5 seconds, scan30 must miss it, and a separate
  1 ms scan must recover both boundaries within the unchanged 0.101 s allowance.
- **Twilight false positive:** evaluate radiance 87.6 seconds after the physical
  night boundary at 61054.04395649246. The historical scan120 endpoint convention
  misses the intervening rejection; an independent 10 ms scan must witness it.
  The 87.5–87.8 s excess and original boundary checks remain required.
- **Zodiacal corner:** for beta=24 degrees and delta=20 degrees, independently
  locate the maximum near 61165.73402466 and average its radiances at +/-0.25 s.
  Freeze 1.472712454699566. The 10 s scan must miss exactly one sub-second
  rejection, and the local 1 ms scan must witness exactly one rejected interval
  with both boundaries agreeing within the original 0.1 s tolerance. Other
  corner/reddening cases retain their original thresholds. An earlier candidate
  at the first day's flatter maximum produced several millisecond edge
  crossings; it was not admitted as a single-event reference.
- **October frame regression:** raise the non-starlight threshold from 0.25 to
  0.35. Both Moon models recover the same approximately 7744 s eligibility
  window. Existing independent, uncached radiance and astronomical altitude
  checks at every integer second remain required.

## Reference endpoint correction

The legacy scanner stops root refinement at 0.864 s. At the MJD
61043.04432263801 twilight boundary, the endpoint radiance is below threshold,
but an independent sample 1 ms later is above it. Legacy midpoint roots expand
that isolated accepted endpoint into a false approximately 75 ms interval.

The all-mask comparison now uses a separate precise reference mode with 1 ms
root brackets and one-sided endpoint samples 1 microsecond inside physical
partitions. It retains the scan grid phase. The legacy mode is unchanged for
historical benchmark comparisons and the explicit twilight false-positive test.
An isolated-endpoint regression and a known linear-crossing regression protect
this distinction. Neither scanner proves completeness below its sample spacing.

## Reproduction

```sh
cargo test --locked -p nsb --lib planning -- --nocapture
```

This reruns the frozen inputs against the independent references. Original CI
failures, intermediate scalar probes and completed check logs are retained in
`nsb-cross-validation/investigation/ci-units-followup/` outside the source tree.
The planning migration applies to test inputs and the test reference only;
production discovery is unchanged.
