# PR #314 scientific correctness and robustness validation

Date: 2026-10-27. Reviewed starting HEAD:
`7987e74f09ab92f24c8e65b9ea1f37e24d8cb30e`, against PR base
`b33df93f142cc3537f467be8e8a1d657c0737975`.

All five reported issues were confirmed. The fixes preserve target resolution,
physical formulas, coordinate/time conventions, and the certified-search refusal
for dynamic Starlight. There are no new public API changes in these fixes.

## Findings and corrections

| Issue | Root cause | Correction and regression evidence |
| --- | --- | --- |
| 1: Zodiacal regimes | Absolute longitude cells do not identify the model's folded relative-longitude cells. Elongation and extinction branches were absent. | Partition signed latitude and wrapped relative longitude on the 5-degree grid, including the absolute-value/folding cusps and all three rectangular corner boundaries. Also partition true elongation at 30/90 degrees and the two Noll brightness branches. A synthetic crossing changes relative longitude from 4.5 to 5.5 degrees while both absolute longitudes remain in their original cells. A 10 ms scalar reference covers folds, negative latitudes, corners, reddening, and both extinction switches. Daylight horizontal queries compare actual threshold schedules with independent instantaneous 5 s scans. |
| 2: HEALPix budget | The 25,000-sample limit applied to the full request, including ineligible times, and samples were collected before evaluation. | Return immediately for empty candidates; discover only inside eligible intervals. Stream probes without a sample vector. Independent total guards bound probes and output. NSIDE128 ten-day and disjoint-interval tests pass. An empty-candidate regression with a simulated 137-year request proves irrelevant physical partitions are skipped. The public altitude-floor regression also returns an empty schedule. |
| 3: Missed HEALPix changes | Equal endpoint pixels skipped return visits; a single binary search could report only one edge per coarse interval. Mean pixel width was insufficient near vertices. | Probe midpoints even when endpoints agree and recursively refine both halves of witnessed changes. Discovery spacing is approximately 1/64 of a nominal pixel transit. A uniform 0.25 s reference checks ORM and Paranal directions, near-vertex short visits, and multiple edges within an old coarse interval. Splitting the stream at a discovered transition preserves the results. The frozen six-hour reference has 241 transitions; the original implementation found 229. |
| 4: Suppressed test failures | Unconditional `let Ok(...) else { return/continue; }` accepted arbitrary errors; a positive constant could satisfy the dynamic-signal assertion. | Horizontal integration tests always supply the checked-in synthetic map, require successful evaluation, and require changing Starlight. Instantaneous samples verify actual accepted and rejected threshold states. Native and once-converted equatorial searches must disagree at a chosen mid-window threshold. Frame conversion comparisons are explicitly described as API consistency tests, not independent scientific validation of Siderust. |
| 5: API gate | Removing `--deny=changed` authorized unrelated future changes whenever the snapshot was updated. | Record only the four exact historical query substitutions. A historical line-preservation check rejects all unrelated removals/changes during migration; comparisons from an already migrated base restore both deny flags. Ordinary additions need no allowlist maintenance. Four policy regressions run through the CI lifecycle script. Real historical and post-migration comparisons pass. A real negative control removing `const` from `ZodiacalExtinction::as_str`, with its snapshot regenerated, fails as intended. |

The Noll branch variable is exactly
`log10(S10 * S10_TO_W_M2_SR_UM / 1e-8)`, with thresholds 2.244 and 2.255.
Brightness comes from the authoritative runtime grid, not a fitted proxy.
Wavelength regime boundaries are constant during each query and need no time
cuts. Solar longitude uses the existing exact shared astronomy cache.

Nearly coincident physical edges can create numerical fragments tens of
microseconds wide. Cuts within the 0.1 ms partition resolution coalesce, including
at eligible endpoints. The existing 1 ms open-interior inset covers that cut
uncertainty. The visible, coincident latitude/longitude regression fails without
this handling and passes with it.

## Performance

Single-run comparison on an Intel Core Ultra 9 185H with Rust 1.97.1,
`profile.test` optimization level 1, warm build, bundled NSIDE128 map, ORM,
altitude 55 degrees, azimuth 120 degrees. Start is TT MJD
60479.91746740741. Timings measure partition calls, excluding map loading and
compilation; these are observations, not statistical performance guarantees.
The original function was compiled unchanged from the reviewed starting HEAD
beside the new implementation in a temporary test harness.

| Eligible coverage | Original | Updated |
| --- | --- | --- |
| Six continuous hours | 0.205 s; 229 cuts | 1.066 s; 241 cuts, confirmed by the dense reference |
| Ten continuous days | Rejected by sample budget | 36.862 s; 10,175 cuts |
| Empty eligibility | Previously sampled the request | 0.000000321 s; no cuts |
| Two disjoint two-hour intervals, nine days apart | Full-span sampling rejected | 0.600 s; 169 cuts |

Denser discovery deliberately costs about 5.2 times more on the short-window
example and finds twelve transitions the old finder missed. Streaming reuses
adjacent endpoint probes; sample memory is constant apart from bounded recursion.
Only discovered cuts are collected (16 bytes per TT instant on this host).
Per discovery call, guards allow 5,000,000 probes and 100,000 cuts; the existing
adaptive search still limits the resulting physical windows to 25,000. Searches
can still reject excessive *eligible* work rather than return partial results.
The new ten-day streaming test and dense-reference tests reproduce the relevant
accuracy/scaling behavior without requiring a production Starlight asset.

## Compatibility and validation

Existing equatorial constructor calls remain valid through `Into<TargetDirection>`.
The PR's public `target` field migration is intentionally source breaking for
field users: assignments need `.into()` and readers use `as_equatorial()`.
Non-exhaustive structs do not make changing an existing public field compatible.
The migration exception documents this fact rather than suppressing general
SemVer detection. Rust, Python, and CLI equatorial behavior remains covered by
the existing contracts and installed-interface comparison suite.

Validation commands and results are recorded below. The installed Python wheel
uses published siderust 0.6.0 / tempoch 0.5.0 in a fresh uv environment; no optional
Starlight download is needed for the new integration regressions.

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-features --all-targets -- -D warnings`: passed.
- `cargo test -p nsb --test horizontal_targets`: 12 passed.
- `cargo test -p nsb --lib planning::tests`: 20 passed.
- `cargo test -p nsb --test query_api`: 19 passed.
- `cargo test --workspace`: 1,132 passed, 7 pre-existing ignored tests; the
  subsequently added empty-candidate test also passed separately.
- `NSB_INTERFACE_TESTS=1 ... python -m pytest python/tests -q`: 159 passed,
  including all 11 installed Rust/Python/CLI equivalence cases.
- Release metadata validation and its eight script tests: passed.
- Public API lifecycle, four migration policy tests, real historical gate, strict
  post-migration gate, and intentional negative control: passed.
- `PYO3_PYTHON=/tmp/nsb-pr314-python/bin/python cargo test --workspace
  --all-features --locked`: 1,139 passed, 7 pre-existing ignored tests. This
  includes the final empty-candidate regression and Python adapter unit tests.
- All-feature doctests: passed (no doctests currently defined).
- Documentation with all features and warnings denied: passed.
- `cargo +1.89 check --workspace --lib --bins --locked`: passed.
- `cargo package -p nsb --locked`: passed including verification; 6.3 MiB
  compressed, below the CI 10 MiB ceiling.

All-feature Rust linking uses the uv-managed Python interpreter because the
system Python installation lacks the unversioned development-library symlink.
The final installed-wheel tests run against the final implementation, not an
older installed build.

## Remaining limits

This is empirical discovery, not certified continuous geometry. HEALPix grazing
visits can be arbitrarily short near vertices; excursions wholly between probes
can still be missed (nominal discovery spacing about 1.71 s at NSIDE128).
Zodiacal discovery spacing is 15 s and likewise cannot certify unwitnessed
return excursions. The 0.25 s/10 ms references and instantaneous radiance scans
are independent numerical evidence, not proofs of completeness or independent
validation of the upstream celestial transforms. Numerically unresolved searches
and exhausted resource guards continue to return explicit errors.

The full remote CI workflow, platform matrices, coverage floors, and external
scientific comparison jobs are separate from the local checks above. Check their
status on the final PR HEAD before merging. This task does not merge the PR.
