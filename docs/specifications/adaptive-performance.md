# Adaptive search production performance

The complete 204-case three-way matrix passes. Annual 100-target schedules are
59–69% faster than the reviewed adaptive implementation and 0.76–1.00 times the
original scan600 latency. Cold single-target searches remain substantially slower:
the four annual CLI cases cost 1.55–1.79 times scan600. PR #235 remains Draft
pending explicit maintainer acceptance of that remaining performance compromise.
Authorization to change the default within 0.1 does not accept this tradeoff.

## Measurement contract

Compare A: original scan600, B: adaptive at commit
`954817f71f404f21fc9ff9fdd37c870506532c89`, and C: optimized adaptive at `169d5c7b7c81cf8f0a0414e6a432853225dd2336`.
All use registry Siderust 0.14.0, Tempoch 0.10.0, Principia 0.6.0 and the same
locked dependency graph, release compiler/profile, physical models, targets and
windows. The complete matrix contains 204 scenarios, described in
[scientific validation](adaptive-validation.md). Incomplete or unexplained
outputs are never accepted as timing results.

Each scenario validates warmups against a separate fine reference, then executes
three alternating scan/adaptive repetitions. Each timed output must exactly
match its validated warmup. The optimized run creates an empty astronomical cache
for every complete schedule: it measures reuse among that schedule's targets,
not a previously cached year. Native UTC export is included. Core latency includes
target preparation, physical partitions, discovery and output conversion;
end-to-end latency also includes shared site preparation once. Evaluator/model
construction is excluded from library timings. Separate CLI process timings
include construction, startup, formatting and I/O and require identical before/
after adaptive CSV bytes.

The host is an Intel Core Ultra 9 185H with 22 logical CPUs. Workers and CPU affinity match A/B/C within each group: South Jones uses two
workers on 0,3; North Jones two on 1,6; North K&S two on 8,10; South K&S four on
12,13,16,17. The first three use performance cores; the last uses efficiency
cores. South K&S retains its complete accepted four-worker A/B baseline from
the identical preserved executable; the other A/B groups were retimed. Absolute cross-group
latencies are not directly comparable. Groups share package power, memory and
LLC; background validation uses other cores. Report medians and observed ranges,
not a statistically reliable p95 from three samples. Process RSS is cumulative
VmHWM including fixture/reference/output copies, not isolated engine memory.

## Profile and optimization

Temporary instrumentation of the unchanged production path on a 30-day,
100-target, explicit four-component North workload found:

| Accumulated worker time (Jones) | Seconds |
| --- | ---: |
| Authoritative integrated radiance | 64.62 |
| Lunar geometry | 39.85 |
| Zodiacal geometry | 15.20 |
| Zodiacal spectral construction / integration | 3.29 / 1.14 |
| Airglow spectral integration | 1.87 |
| Jones spectral evaluation | 2.55 |
| Fits / polynomial roots / derivative roots | 0.005 / 0.099 / 0.068 |
| Crossing refinement, inclusive of exact probes | 0.671 |

These categories overlap; accumulated worker times are not end-to-end latency.
Measured wall time was 35.50 s. Shared preparation took 0.103 s, target preparation
0.652 s over 200 preparations, physical partitioning 2.020 s, and discovery
33.158 s. There were 195,751 exact calls from 754,989 probe requests: the existing
local radiance cache already removed about 74% of probe requests. Interpolation
nodes accounted for 133,092 requests and 43.67 accumulated seconds; low/high fit
validation accounted for 29,576 / 336 requests and 9.67 / 0.18 seconds. This case
needed no exact golden-section extremum search. K&S showed the same geometry
bottleneck; its radiance formula itself took only 0.020 s.

A second instrumented run counted 36,337 distinct exact timestamps among 195,751
Jones evaluations, and 35,151 among 195,709 K&S evaluations. Thus roughly 82% of
calls revisit a site-wide state. The optimization reuses the exact solar and lunar
states through a bounded context cache, preserving every discovery node and
physical radiance calculation. This addresses the measured bottleneck without
reducing scientific validation effort or introducing approximate ephemerides.
Kernel `perf` events are unavailable on this host; timing instrumentation was
removed from production after profiling.


Exact probe attribution for that Jones workload is 133,056 Lobatto-grid fit/interlaced-check,
29,910 independent validation, 29,504 endpoint, and 3,281
root/derivative/extremum/refinement evaluations. The local cache serves another
559,238 probe requests. These counts are preserved by the optimization. Shared
astronomy reduces 294,960 solar/lunar state requests to 54,951 computations;
K&S uses 52,578 computations for 294,876 state requests. Starlight remains a
once-per-target preparation value, with no per-node physical recalculation.

The final instrumented two-worker efficiency-core run takes 31.11 s Jones and
27.31 s K&S. Its 195,751 / 195,709 exact calls are unchanged. Solar/lunar cache
misses are 54,950 / 52,578; the one-miss Jones difference from the earlier profile
is allowed duplicate concurrent computation. Accumulated Jones component times
are Zodiacal 12.92 s (1.28 geometry, 8.36 spectral construction, 3.17 integration),
Airglow 6.19 s (4.94 integration), lunar state construction 3.93 s and Jones
spectral evaluation 6.59 s. Starlight preparation is 0.00056 s; context identity
validation is 0.00029 s over 200 checks. Polynomial roots and derivative roots
cost 0.29 / 0.22 s; inclusive crossing refinement costs 1.53 s. Nested scopes
overlap. Serial target preparation and physical partitioning limit scalability;
the matched-worker release matrix supplies production comparisons. Compact
[profiles](adaptive-acceptance/profiles.jsonl) retain both baseline core classes
and the final matched efficiency-core profile; temporary instrumentation is absent
from production.

Four sample-buffer reservations preserve spectral arithmetic while reducing
allocation traffic. A glibc allocation interposer measured a cold annual
South/Jones CLI process: scan600 made 584,088 allocation/reallocation calls
requesting 319,308,636 bytes; adaptive before made 1,150,997 calls requesting
685,792,377 bytes; final adaptive made 700,117 calls requesting 230,138,921 bytes.
That is 39% fewer calls and 66% less traffic than before. Requested bytes are
cumulative traffic, not peak live memory. Before/after adaptive CSV bytes are
identical. [Allocation records](adaptive-acceptance/allocations.jsonl) include
binary/output hashes. A controlled buffer-only comparison did not establish a
separate latency improvement; the change is retained for its allocation benefit.

## Final complete schedules

Seconds below are end-to-end medians of three repetitions. A is the original
scan600 sample from the baseline run, B the reviewed adaptive version, and C the
final version. The machine-readable [204 comparisons](adaptive-acceptance/benchmarks.jsonl)
retain both original A and the scan retimed alongside C, every repetition,
preparation/core times, counters, scientific checks and cumulative process RSS.
The [environment](adaptive-acceptance/environment.json) records toolchain,
affinity, dependency lock and executable hashes.

| Annual 100 targets | A | B | C | Reduction from B | C/A | Exact calls A / B=C |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| North Jones | 232.499 | 554.995 | 202.939 | 63.4% | 0.873 | 906,946 / 2,069,160 |
| North K&S | 218.687 | 533.697 | 166.467 | 68.8% | 0.761 | 912,944 / 2,044,726 |
| South Jones | 222.699 | 517.071 | 188.984 | 63.5% | 0.849 | 918,794 / 2,033,915 |
| South K&S | 184.834 | 450.161 | 184.974 | 58.9% | 1.001 | 925,713 / 2,009,329 |

C observed ranges in the same order are 197.292–211.253, 161.259–179.280,
188.781–195.830 and 179.640–189.294 s. Core A/B/C medians are respectively
231.183/553.679/201.482, 217.358/532.369/165.182,
221.249/515.621/187.685 and 182.179/447.505/182.433 s. Final annual fit /
subdivision / refinement counts are 163,081/757/9,543, 163,033/0/9,914,
160,470/700/10,484 and 160,717/0/10,540; every count equals B.
C cumulative process peak RSS is 82.2, 70.5, 76.3 and 76.6 MiB respectively.
This includes earlier reference allocations and is not cache-only live memory.
The 100,000-state admission cap per astronomy component bounds growth; saturated
misses still compute normally. All 204 schedules have zero unresolved seconds,
budget exhaustions, fallbacks or unexplained fine-reference disagreements.

Individual single-target components have mixed results: across sites, models
and 1/7/30/365-day windows, C/A ranges are 0.89–1.26 Starlight,
0.82–1.50 Zodiacal and 0.88–1.51 Airglow. Single-target actual ALL and explicit
four-component ranges are 1.25–2.17 and 1.24–2.28. Preparation dominates tiny
cases; cross-target astronomy reuse supplies the large batch improvement.
There is no general single-target parity claim.

## Cold public CLI and remaining decision

Ten paired repetitions per case include process startup, evaluator construction,
search and CSV output on two efficiency cores (14,15). All B/C output hashes
match exactly, and each executable is deterministic across repetitions.
[CLI records](adaptive-acceptance/cli.jsonl) retain timings, arguments and RSS.

| Annual single target, ALL | A | B | C | C/A | C observed range |
| --- | ---: | ---: | ---: | ---: | ---: |
| South Jones | 6.704 | 11.510 | 10.699 | 1.596 | 10.174–11.147 |
| South K&S | 6.414 | 10.418 | 9.960 | 1.553 | 9.782–10.251 |
| North Jones | 7.884 | 14.607 | 14.133 | 1.793 | 13.499–14.511 |
| North K&S | 7.263 | 12.799 | 12.443 | 1.713 | 11.903–13.064 |

Moon-down one-hour cases take 0.0112 / 0.0107 s, near scan startup cost. Annual
final CLI peak RSS is 13.4–13.9 MiB versus roughly 10.5–11.1 MiB before.
Caching reduces repeated target-independent geometry; it cannot eliminate the
extra authoritative nodes of a cold single-target search. Annual batch adaptive
uses 2.17–2.28 times scan600's exact calls. The preserved fit-validation and
extremum probes detect narrow events and accepted islands missed by the coarse
reference, including the opposite-aliasing regression. Removing those probes
would invalidate demonstrated coverage. No cheaper equivalent-coverage algorithm
has been established in this pass; this is evidence for the cost, not a proof
that the overhead is unavoidable. Adaptive remains empirical rather than
universally complete. The maintainer must explicitly accept the substantial
single-target compromise before the PR becomes ready.

## Release validation

On final production source: workspace 875 passed, zero failed (six ignored
manual benchmarks); all-feature suites 647 passed; release planning 46 passed
(two ignored manual benchmarks). Installed abi3 wheels pass 40 Python tests each
on Python 3.11 and 3.14. All six CLI benchmark cases complete ten repetitions per
variant with identical B/C CSV bytes. Formatting, license headers, cfg boundaries,
all-feature Clippy with warnings denied, workspace/no-default checks, Rust 1.89,
Rustdoc with warnings denied, doctests, cargo-deny and clean package verification
pass. Frozen public API snapshot and historical SemVer checks pass without
bypassing policy. Siderust remains 0.14.0 with Tempoch 0.10.0 and Principia 0.6.0.

Fresh coverage is workspace 88.36%, NSB 92.84%, and changed production 94.78%
(926/977 executable lines), above unchanged 80/87/90% floors. Draft CI skips its
conditional coverage job; local coverage supplies the measured gate. GitHub
Actions also validates Linux/macOS/Windows abi3 wheels. See PR #235 for checks
on the final evidence commit; no source changes follow these measurements.

## Reproduction

Preserve the release test executable from commit `954817f` before building the
optimized tree. Use the same compiler and locked dependencies for both builds.
A detached archive or separate checkout can build the historical binary; no
historical engine must remain in production. The native runner accepts a
preserved executable via `NSB_DISCOVERY_BINARY`:

```bash
NSB_DISCOVERY_BINARY=/absolute/path/benchmark-before scripts/benchmark-adaptive-threshold.sh target/before.log
scripts/benchmark-adaptive-threshold.sh target/after.log
python3 scripts/summarize-discovery-benchmark.py --before target/before.log --after target/after.log --output target/three-way.jsonl
python3 scripts/benchmark-planning-cli.py /path/scan600 /path/before /path/after --repeats 10 > target/cli-three-way.jsonl
```

Set matching `RAYON_NUM_THREADS`, CPU affinity and `NSB_DISCOVERY_REPEATS` for both
native runs. `NSB_DISCOVERY_FILTER` selects a group or scenario; combine all four
group logs for the final summarizer. The summarizer requires all 204 unique usable
cases and unchanged exact counts, fit/refinement counts, accepted duration,
period count and scientific discrepancy counts. It retains raw repetitions and
resource metrics in one machine-readable file. Historical accepted evidence is
accessible at commit `954817f`; it is not treated as final optimized acceptance.
