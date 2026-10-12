# Adaptive threshold review measurements

Historical certificate-feasibility record. Its timing-only harness has been
replaced by the [complete adaptive acceptance harness](adaptive-acceptance/README.md).
The original experiment remains reproducible from its recorded historical commit;
its incomplete active-model timings are not production adaptive performance evidence.


Measured source: `99fcb62`, unchanged uniform model/scanner baseline from main `a3f6752`.
[Environment, revision, lock hash and memory record](adaptive-threshold-review-environment.json).
Subsequent `f032535` adds only Python civil-year limits to UTC export and its
regression test; the measured core engine is unchanged. The final
[end-to-end measurements](adaptive-threshold-cli-benchmarks.md) use `f032535`.
The historical prototype below is `7d98e64` (artifacts committed at `fbd7955`),
measured earlier on the same machine/toolchain/locked dependencies;
those historical timings are not newly paired trials against this revision.
The new paired trials compare the unchanged scanner and updated certified-only
discovery with identical inputs. The legacy JSON `prototype_*` names refer to
the updated discovery engine in this file. Target preparation is included;
shared site preparation and UTC output are excluded and reported separately.

## Three-variant comparison

| Core scenario | Fresh uniform median | Historical prototype median | Updated discovery median | Updated outcome |
| --- | ---: | ---: | ---: | --- |
| south/all/365d | 1.946234 s | 0.202430 s | 0.209285 s | incomplete; not an equivalent schedule |
| south/all/year/100targets | 207.434474 s | 4.326275 s | 3.960325 s | incomplete; not an equivalent schedule |
| north/all/year/100targets | 185.648936 s | 4.070083 s | 3.709168 s | incomplete; not an equivalent schedule |
| south/starlight/365d | 0.220507 s | 0.190569 s | 0.223107 s | complete |
| synthetic/close-pair | 1.614580 µs | 29.570750 µs | 16.278750 µs | incomplete; not an equivalent schedule |
| synthetic/tangent | 1.573610 µs | 18.132220 µs | 14.342650 µs | incomplete; not an equivalent schedule |

The southern 100-target ALL case reduces bound calls from 21,060,404 to 153,687;
the northern case reduces them from 20,813,506 to 149,390. Entire eligible ALL
target-time remains unresolved, so these are work-control improvements, not
complete-schedule speedups. All four annual 100-target rows remain incomplete.
The `all` label uses this build's frozen default mask (bits 13); separate
Starlight rows use an explicit experimental fixture. All-four mask bits 15 is
covered by correctness tests, not by these default-mask performance labels.

The [bound-only synthetic experiment](adaptive-threshold-bounds-only-benchmarks.md)
uses the same 59/41 bounds for close-pair/tangent, with no fits or exact probes.
It measures 8.33/6.95 µs versus 16.28/14.34 µs with fitting in the main matrix;
uncertainty remains identical (two/one regions). It is still slower than the
cheap point scan, whose discovery coverage differs. These are three-repetition
microbenchmarks, not tail-latency claims.

The full matrix completed with 9 complete and 74 incomplete discovery cases. Observed test-process peak RSS was 13,056 KiB; both algorithms share that process, so this is not a per-algorithm memory comparison. See the environment record for command-wide timing/RSS and sampling scope.

## Full paired core matrix

Measured 2026-10-12 against main `a3f6752`. See [methodology and limitations](adaptive-threshold-production-status.md). Raw repetitions/counters are in [JSONL](adaptive-threshold-review-benchmarks.jsonl).

The previous scanner has a discrete 600 s discovery contract. The measured discovery engine has certified/incomplete semantics. **A lower runtime for an incomplete answer is not a speedup for an equivalent observing-window result.** `ratio` is previous/prototype wall time only. Values below 1 mean discovery is slower.

Both paths use the same release binary, evaluator, input, reused context and two Rayon threads. Site preparation and final UTC conversion/output are excluded. Each has one warmup and three timed repetitions (synthetic cases batch 100 calls per repetition). Medians below are milliseconds. Three repetitions do not support a meaningful p95 estimate.

| Scenario | Previous median ms | Discovery median ms | Runtime ratio | Exact calls old/new | Bounds / fits / subdivisions | Unresolved regions | Outcome |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| synthetic/far | 0.0028 | 0.0004 | 7.907 | 145/0 | 1/0/0 | 0 | complete; equivalent classification |
| synthetic/close-pair | 0.0016 | 0.0163 | 0.099 | 145/11 | 59/1/29 | 2 | incomplete; coverage differs |
| synthetic/tangent | 0.0016 | 0.0143 | 0.110 | 145/11 | 41/1/20 | 1 | incomplete; coverage differs |
| south/all/1d | 29.4084 | 1.8007 | 16.331 | 48/0 | 9/0/0 | 9 | incomplete; coverage differs |
| south/all/7d | 115.6620 | 4.1946 | 27.574 | 343/0 | 65/0/0 | 65 | incomplete; coverage differs |
| south/all/30d | 291.6347 | 17.2293 | 16.927 | 1297/0 | 241/0/0 | 241 | incomplete; coverage differs |
| south/all/365d | 1946.2338 | 209.2851 | 9.299 | 8297/0 | 1476/0/0 | 1476 | incomplete; coverage differs |
| south/zodiacal/1d | 5.1851 | 0.6503 | 7.974 | 38/0 | 1/0/0 | 1 | incomplete; coverage differs |
| south/zodiacal/7d | 21.7517 | 4.0798 | 5.332 | 258/0 | 7/0/0 | 7 | incomplete; coverage differs |
| south/zodiacal/30d | 76.0105 | 17.9228 | 4.241 | 976/0 | 30/0/0 | 30 | incomplete; coverage differs |
| south/zodiacal/365d | 581.5943 | 226.7567 | 2.565 | 5900/0 | 268/0/0 | 268 | incomplete; coverage differs |
| south/airglow/1d | 1.2792 | 0.5804 | 2.204 | 48/0 | 9/0/0 | 9 | incomplete; coverage differs |
| south/airglow/7d | 6.7664 | 3.9365 | 1.719 | 329/0 | 63/0/0 | 63 | incomplete; coverage differs |
| south/airglow/30d | 25.0717 | 15.9324 | 1.574 | 1227/0 | 226/0/0 | 226 | incomplete; coverage differs |
| south/airglow/365d | 279.3349 | 220.3133 | 1.268 | 7340/0 | 1382/0/0 | 1382 | incomplete; coverage differs |
| south/moon/1d | 20.4006 | 0.8294 | 24.597 | 38/0 | 1/0/0 | 1 | incomplete; coverage differs |
| south/moon/7d | 73.9309 | 3.9612 | 18.664 | 268/0 | 71/0/31 | 9 | incomplete; coverage differs |
| south/moon/30d | 162.9323 | 18.3649 | 8.872 | 1008/0 | 551/0/253 | 38 | incomplete; coverage differs |
| south/moon/365d | 1068.8859 | 225.5255 | 4.740 | 6126/0 | 2048/0/936 | 367 | incomplete; coverage differs |
| south/starlight/1d | 0.6824 | 0.6487 | 1.052 | 38/0 | 1/0/0 | 0 | complete; equivalent classification |
| south/starlight/7d | 4.1552 | 4.2410 | 0.980 | 258/0 | 7/0/0 | 0 | complete; equivalent classification |
| south/starlight/30d | 18.0333 | 18.2969 | 0.986 | 976/0 | 30/0/0 | 0 | complete; equivalent classification |
| south/starlight/365d | 220.5071 | 223.1071 | 0.988 | 5900/0 | 268/0/0 | 0 | complete; equivalent classification |
| south/all/year/10targets | 27436.0331 | 577.9872 | 47.468 | 125598/0 | 19908/0/0 | 22233 | incomplete; coverage differs |
| south/all/year/100targets | 207434.4739 | 3960.3250 | 52.378 | 918794/0 | 153687/0/0 | 164597 | incomplete; coverage differs |
| south/bright-moon | 109.3745 | 1.1268 | 97.064 | 751/0 | 397/0/178 | 32 | incomplete; coverage differs |
| south/moon-down | 79.7351 | 1.1751 | 67.853 | 626/0 | 296/0/129 | 26 | incomplete; coverage differs |
| south/near-threshold | 296.9017 | 19.0093 | 15.619 | 1322/0 | 241/0/0 | 241 | incomplete; coverage differs |
| south/far-threshold | 277.9981 | 18.9168 | 14.696 | 1253/0 | 241/0/0 | 241 | incomplete; coverage differs |
| south/ks1991/moon/1d | 16.5253 | 0.5636 | 29.318 | 38/0 | 1/0/0 | 1 | incomplete; coverage differs |
| south/ks1991/moon/7d | 66.7563 | 3.7999 | 17.568 | 271/0 | 71/0/31 | 9 | incomplete; coverage differs |
| south/ks1991/moon/30d | 164.7042 | 17.3283 | 9.505 | 1056/0 | 551/0/253 | 38 | incomplete; coverage differs |
| south/ks1991/moon/365d | 1099.1369 | 200.3509 | 5.486 | 6401/0 | 2048/0/936 | 367 | incomplete; coverage differs |
| south/ks1991/all/1d | 17.5804 | 0.7333 | 23.975 | 48/0 | 9/0/0 | 9 | incomplete; coverage differs |
| south/ks1991/all/7d | 110.9865 | 5.0474 | 21.989 | 344/0 | 65/0/0 | 65 | incomplete; coverage differs |
| south/ks1991/all/30d | 280.1509 | 21.1745 | 13.231 | 1329/0 | 241/0/0 | 241 | incomplete; coverage differs |
| south/ks1991/all/365d | 1794.0901 | 248.8478 | 7.210 | 8334/0 | 1476/0/0 | 1476 | incomplete; coverage differs |
| south/ks1991/all/year/10targets | 26985.5795 | 544.8138 | 49.532 | 126363/0 | 19908/0/0 | 22233 | incomplete; coverage differs |
| south/ks1991/all/year/100targets | 181990.1357 | 3879.6192 | 46.909 | 925713/0 | 153687/0/0 | 164597 | incomplete; coverage differs |
| south/ks1991/bright-moon | 86.4440 | 1.0876 | 79.480 | 752/0 | 397/0/178 | 32 | incomplete; coverage differs |
| south/ks1991/moon-down | 82.1767 | 1.3112 | 62.673 | 645/0 | 296/0/129 | 26 | incomplete; coverage differs |
| south/ks1991/near-threshold | 254.4416 | 14.8267 | 17.161 | 1363/0 | 241/0/0 | 241 | incomplete; coverage differs |
| south/ks1991/far-threshold | 253.6954 | 16.5855 | 15.296 | 1253/0 | 241/0/0 | 241 | incomplete; coverage differs |
| north/all/1d | 24.0582 | 0.0714 | 337.096 | 75/0 | 14/0/0 | 14 | incomplete; coverage differs |
| north/all/7d | 163.9171 | 0.3626 | 452.104 | 516/0 | 90/0/0 | 90 | incomplete; coverage differs |
| north/all/30d | 386.7876 | 1.1122 | 347.761 | 2002/0 | 365/0/0 | 365 | incomplete; coverage differs |
| north/all/365d | 2588.3413 | 16.6656 | 155.311 | 12415/0 | 2048/0/0 | 2183 | incomplete; coverage differs |
| north/zodiacal/1d | 3.8540 | 0.0440 | 87.675 | 60/0 | 2/0/0 | 2 | incomplete; coverage differs |
| north/zodiacal/7d | 25.0961 | 0.2348 | 106.868 | 400/0 | 8/0/0 | 8 | incomplete; coverage differs |
| north/zodiacal/30d | 95.8007 | 1.0554 | 90.776 | 1549/0 | 31/0/0 | 31 | incomplete; coverage differs |
| north/zodiacal/365d | 550.8904 | 12.6442 | 43.569 | 9354/0 | 280/0/0 | 280 | incomplete; coverage differs |
| north/airglow/1d | 1.0666 | 0.1069 | 9.980 | 75/0 | 14/0/0 | 14 | incomplete; coverage differs |
| north/airglow/7d | 4.6376 | 0.3566 | 13.005 | 497/0 | 87/0/0 | 87 | incomplete; coverage differs |
| north/airglow/30d | 15.1095 | 1.1345 | 13.318 | 1945/0 | 350/0/0 | 350 | incomplete; coverage differs |
| north/airglow/365d | 113.7529 | 14.3848 | 7.908 | 11587/0 | 2048/0/0 | 2084 | incomplete; coverage differs |
| north/moon/1d | 17.1450 | 0.0922 | 185.959 | 60/0 | 2/0/0 | 2 | incomplete; coverage differs |
| north/moon/7d | 101.8421 | 0.3947 | 258.015 | 410/0 | 111/0/50 | 11 | incomplete; coverage differs |
| north/moon/30d | 197.4327 | 1.1777 | 167.645 | 1578/0 | 574/0/264 | 39 | incomplete; coverage differs |
| north/moon/365d | 1207.7954 | 13.9243 | 86.740 | 9636/0 | 2048/0/933 | 390 | incomplete; coverage differs |
| north/starlight/1d | 0.3506 | 0.0391 | 8.969 | 60/0 | 2/0/0 | 0 | complete; equivalent classification |
| north/starlight/7d | 0.4839 | 0.2654 | 1.823 | 400/0 | 8/0/0 | 0 | complete; equivalent classification |
| north/starlight/30d | 1.4949 | 1.1415 | 1.310 | 1549/0 | 31/0/0 | 0 | complete; equivalent classification |
| north/starlight/365d | 12.8706 | 12.3663 | 1.041 | 9354/0 | 280/0/0 | 0 | complete; equivalent classification |
| north/all/year/10targets | 10484.7885 | 718.3724 | 14.595 | 50696/0 | 9050/0/0 | 9185 | incomplete; coverage differs |
| north/all/year/100targets | 185648.9361 | 3709.1678 | 50.051 | 906946/0 | 149390/0/0 | 162649 | incomplete; coverage differs |
| north/bright-moon | 149.2007 | 1.0424 | 143.135 | 1159/0 | 414/0/186 | 33 | incomplete; coverage differs |
| north/moon-down | 128.8587 | 16.1397 | 7.984 | 935/0 | 310/0/136 | 26 | incomplete; coverage differs |
| north/near-threshold | 395.7977 | 1.1242 | 352.057 | 2029/0 | 365/0/0 | 365 | incomplete; coverage differs |
| north/far-threshold | 383.1813 | 1.1654 | 328.798 | 1974/0 | 365/0/0 | 365 | incomplete; coverage differs |
| north/ks1991/moon/1d | 16.6460 | 0.0611 | 272.416 | 60/0 | 2/0/0 | 2 | incomplete; coverage differs |
| north/ks1991/moon/7d | 94.0008 | 0.2653 | 354.272 | 420/0 | 111/0/50 | 11 | incomplete; coverage differs |
| north/ks1991/moon/30d | 213.0596 | 1.1315 | 188.304 | 1622/0 | 574/0/264 | 39 | incomplete; coverage differs |
| north/ks1991/moon/365d | 1205.3110 | 14.0552 | 85.755 | 9885/0 | 2048/0/933 | 390 | incomplete; coverage differs |
| north/ks1991/all/1d | 21.9989 | 0.0554 | 397.122 | 75/0 | 14/0/0 | 14 | incomplete; coverage differs |
| north/ks1991/all/7d | 143.3151 | 0.2862 | 500.811 | 518/0 | 90/0/0 | 90 | incomplete; coverage differs |
| north/ks1991/all/30d | 385.4001 | 1.1602 | 332.181 | 2036/0 | 365/0/0 | 365 | incomplete; coverage differs |
| north/ks1991/all/365d | 2365.8675 | 15.1451 | 156.213 | 12567/0 | 2048/0/0 | 2183 | incomplete; coverage differs |
| north/ks1991/all/year/10targets | 10507.0294 | 693.4694 | 15.151 | 51026/0 | 9050/0/0 | 9185 | incomplete; coverage differs |
| north/ks1991/all/year/100targets | 167917.4835 | 4056.5048 | 41.395 | 912944/0 | 149390/0/0 | 162649 | incomplete; coverage differs |
| north/ks1991/bright-moon | 138.2476 | 1.0469 | 132.051 | 1186/0 | 414/0/186 | 33 | incomplete; coverage differs |
| north/ks1991/moon-down | 114.4161 | 15.9603 | 7.169 | 952/0 | 310/0/136 | 26 | incomplete; coverage differs |
| north/ks1991/near-threshold | 358.9549 | 1.0515 | 341.368 | 2051/0 | 365/0/0 | 365 | incomplete; coverage differs |
| north/ks1991/far-threshold | 351.9062 | 1.0476 | 335.919 | 1974/0 | 365/0/0 | 365 | incomplete; coverage differs |

Synthetic close-pair and tangent cases are more expensive than the cheap point scan when their measured ratio is below one; they establish coverage with analytic envelopes and retain uncertainty near contacts. The scan can miss the synthetic close pair entirely.

Real dynamic cases deliberately disable fits until piecewise branches are isolated. `ALL`, Zodiacal and Airglow certify no time; Moon-only certifies only prepared Moon-down interiors. Starlight uses a test-only experimental map and certifies its cached scalar. These results do not support replacing the production scanner.

Additional per-query counters/times, unresolved duration, and site preparation cost are retained in JSONL. Bounds/fit/refinement timers are summed worker elapsed times, not CPU clocks or total wall time. Batch counters aggregate targets/windows; unresolved days are summed target-time. Counters and stage durations come from the final repetition. Peak pending intervals track work-queue size, not all allocations. Whole-process RSS should be collected separately with `/usr/bin/time -v`; no per-algorithm memory-improvement claim is made.

Reproduce:

```bash
RAYON_NUM_THREADS=2 /usr/bin/time -v scripts/benchmark-adaptive-threshold.sh > /tmp/adaptive-benchmark.log
python3 scripts/summarize-adaptive-threshold-benchmark.py /tmp/adaptive-benchmark.log adaptive-threshold-review-benchmarks
```
