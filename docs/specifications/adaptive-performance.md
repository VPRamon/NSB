# Adaptive search production performance

Final acceptance is in progress. PR #235 remains Draft until the complete
three-way measurements and release checks are available. No substantial
remaining slowdown is accepted implicitly by the 0.1 default-change authorization.

## Measurement contract

Compare A: original scan600, B: adaptive at commit
`954817f71f404f21fc9ff9fdd37c870506532c89`, and C: optimized adaptive.
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

The host is an Intel Core Ultra 9 185H with 22 logical CPUs. All groups use two
Rayon workers, with matching CPU affinity for A/B/C within each group: South
Jones 0,3; North Jones 1,6; North K&S 8,10; South K&S 12,16. The first three pairs
use performance cores; the last uses efficiency cores. Absolute cross-group
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


Exact probe attribution for that Jones workload is 133,056 interpolation,
29,910 independent/interlaced validation, 29,504 endpoint, and 3,281
root/derivative/extremum/refinement evaluations. The local cache serves another
559,238 probe requests. These counts are preserved by the optimization. Shared
astronomy reduces 294,960 solar/lunar state requests to 54,951 computations;
K&S uses 52,578 computations for 294,876 state requests. Starlight remains a
once-per-target preparation value, with no per-node physical recalculation.

Instrumented cold-context execution on efficiency cores scales from 59.85 /
49.50 seconds with one worker to 32.63 / 29.57 with two and 21.23 / 17.65 with
four (Jones / K&S). Target preparation and physical partitioning remain serial
between targets, limiting scaling. These diagnostic runs locate costs; the
matched-worker release matrix supplies production comparisons.

A glibc allocation interposer measured a cold annual South/Jones CLI process:
scan600 made 584,088 allocation/reallocation calls requesting 319,308,636 bytes;
adaptive before made 1,150,997 calls requesting 685,792,377 bytes; optimized
adaptive made 1,151,025 calls requesting 690,085,129 bytes. Requested bytes are
cumulative allocation traffic, not peak live memory. The 28 extra calls and
4,292,752 extra requested bytes reflect context-cache storage. Before/after
adaptive CSV bytes are identical. Per-scenario RSS is reported separately.

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
