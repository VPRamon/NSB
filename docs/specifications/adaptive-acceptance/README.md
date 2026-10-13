# Reproducing adaptive acceptance

`dependency-A.jsonl` is the frozen scanner on Siderust 0.13 / Tempoch 0.9;
`dependency-B.jsonl` is the same scanner and reference exporter on registry
Siderust 0.14 / Tempoch 0.10. The exporter is the ignored
`planning::discovery_bench::dependency_upgrade_reference` test. The recorded
64 physical cases cover both sites, both Moonlight models and all masks; three
additional UTC/TT records cover leap/time conversions. These finite cases are
exactly identical:

```sh
python3 scripts/compare-adaptive-dependencies.py
```

The practical schedule matrix compares the legacy scanner and adaptive engine
**on the same upgraded graph**. A sequential reproduction can use two Rayon threads and a release build:

```sh
RAYON_NUM_THREADS=2 NSB_DISCOVERY_REPEATS=3 \
  bash scripts/benchmark-adaptive-threshold.sh > target/acceptance-pr235/discovery-matrix.log 2>&1
python3 scripts/summarize-discovery-benchmark.py target/acceptance-pr235/discovery-matrix.log
```

There are 204 scenarios. `NSB_DISCOVERY_FILTER` selects a scenario substring; `NSB_DISCOVERY_EXCLUDE`
excludes one.
`NSB_DISCOVERY_COMPLETED` optionally names a log containing previously validated
`DISCOVERY_BENCH` rows, to skip expensive completed scenarios after interruption.
Combine retained and resumed rows into one unique row per scenario before
summarizing. The summarizer rejects missing/duplicate cases, exhaustion,
incomplete outputs and unexplained fine-reference discrepancies.

Each row includes one warmup for each engine, complete usable UTC schedules,
a 600-second reference and a separate 30-second (single target) or 120-second
(batch) reference. Scientific comparisons run before three alternating timed
repetitions; every timed output must reproduce its validated warmup exactly. The core timer includes target
preparation, partitioning, discovery and native UTC conversion. End-to-end time
adds shared site preparation once. File I/O and process startup are excluded;
An existing evaluator is reused; immutable model and fixture construction
are outside the library timers. CLI process timings include construction and
startup separately. CPU affinities, compiler and
registry lock hash are in `environment.json`. The recorded parallel run uses
two workers on two performance cores per group, except South K&S which uses
four workers on four efficiency cores. Each paired engine uses the same assignment. Simultaneous groups use disjoint
physical cores but share package power, memory and LLC; measurements are
observations on this machine, not portable latency or tail-confidence claims.

The coarse comparison requires every validated boundary witness within 0.533
seconds. A reference singleton at a physical jump is excluded only after exact
boundary and interior checks show that its positive duration is a scanner
artifact. Fine-reference topology differences require independent dense local
confirmation of every extra adaptive rejected gap, with steps at most 10 ms
and boundary disagreement at most 0.106 seconds. Additional accepted islands
require the symmetric independent local reference with both boundaries checked;
accepted-island and rejected-gap misses are counted separately. Other mismatches fail rather
than emitting an accepted timing row. Batch counters and duration sums aggregate target queries; operation limits
apply to each `ThresholdQuery`, while reported peaks are maxima across targets.
The JSON records raw count disagreements,
confirmed reference misses and singleton artifacts separately. A finer scan
remains discrete evidence, not a continuous-time completeness proof.

The permanent release scientific tests include an opposite aliasing regression
(a smooth excursion visible to scan600 but hidden from the original adaptive
nodes), an independently resolved approximately 53 ms Zodiacal excursion, and
a 2.4-second PALACE/Jones excursion. Run them with:

```sh
RAYON_NUM_THREADS=2 cargo test --release --locked -p nsb --lib planning::discovery_tests
```

The workspace's normal test profile enables optimization level 1 and debug
assertions. Every sample and assertion also runs under the required normal
workspace test command. Synthetic Starlight fixtures used by tests/benchmarks
are not admitted or packaged as production scientific assets.


The matrix starts with each site's/model's annual 100-target workload, then
executes the remaining 50 cases. Progress messages occur outside the timed
search calls. Keep logs on persistent storage (for example
`target/acceptance-pr235/`) rather than `/tmp` if runs may span a restart.
Observed native UTC boundary errors and exact endpoint checks for the 64-case
physical corpus are in [boundary accuracy](boundary-accuracy.md).

Completed results: [204-case table](production.md), [compact summary](performance-summary.md),
[performance decision](performance-analysis.md), and [independent discrepancy records](confirmed-discrepancies.jsonl).
