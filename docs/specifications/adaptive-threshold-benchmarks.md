# Adaptive threshold feasibility measurements

Measured 2026-10-12 against main `a3f6752`. See [methodology and limitations](adaptive-threshold-search.md). Raw repetitions/counters are in [JSONL](adaptive-threshold-benchmarks.jsonl).

The previous scanner has a discrete 600 s discovery contract. The prototype has certified/incomplete semantics. **A lower runtime for an incomplete answer is not a speedup for an equivalent observing-window result.** `ratio` is previous/prototype wall time only. Values below 1 mean the prototype is slower.

Both paths use the same release binary, evaluator, input, reused context and two Rayon threads. Site preparation and final UTC conversion/output are excluded. Each has one warmup and three timed repetitions (synthetic cases batch 100 calls per repetition). Medians below are milliseconds. Three repetitions do not support a meaningful p95 estimate.

| Scenario | Previous median ms | Prototype median ms | Runtime ratio | Exact calls old/new | Bounds / fits / subdivisions | Unresolved regions | Outcome |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| synthetic/far | 0.0026 | 0.0003 | 7.692 | 145/0 | 1/0/0 | 0 | complete; equivalent classification |
| synthetic/close-pair | 0.0026 | 0.0296 | 0.087 | 145/11 | 59/1/29 | 2 | incomplete; coverage differs |
| synthetic/tangent | 0.0019 | 0.0181 | 0.103 | 145/11 | 41/1/20 | 1 | incomplete; coverage differs |
| south/all/1d | 17.6327 | 0.7728 | 22.816 | 48/0 | 1152/0/621 | 630 | incomplete; coverage differs |
| south/all/7d | 119.0385 | 3.9342 | 30.257 | 343/0 | 8320/0/4482 | 4547 | incomplete; coverage differs |
| south/all/30d | 313.9151 | 21.7361 | 14.442 | 1297/0 | 30848/0/16620 | 16861 | incomplete; coverage differs |
| south/all/365d | 1944.1006 | 202.4300 | 9.604 | 8297/0 | 188927/0/101713 | 103189 | incomplete; coverage differs |
| south/zodiacal/1d | 4.5111 | 0.5695 | 7.922 | 38/0 | 128/0/71 | 72 | incomplete; coverage differs |
| south/zodiacal/7d | 19.5578 | 3.8383 | 5.095 | 258/0 | 896/0/497 | 504 | incomplete; coverage differs |
| south/zodiacal/30d | 76.9989 | 16.2402 | 4.741 | 976/0 | 3840/0/2130 | 2160 | incomplete; coverage differs |
| south/zodiacal/365d | 556.4540 | 192.1216 | 2.896 | 5900/0 | 34304/0/18825 | 19093 | incomplete; coverage differs |
| south/airglow/1d | 1.0791 | 0.7551 | 1.429 | 48/0 | 1152/0/621 | 630 | incomplete; coverage differs |
| south/airglow/7d | 7.3833 | 5.1767 | 1.426 | 329/0 | 8064/0/4347 | 4410 | incomplete; coverage differs |
| south/airglow/30d | 30.3004 | 22.5589 | 1.343 | 1227/0 | 28928/0/15609 | 15835 | incomplete; coverage differs |
| south/airglow/365d | 245.4615 | 195.2719 | 1.257 | 7340/0 | 176895/0/95394 | 96776 | incomplete; coverage differs |
| south/moon/1d | 24.7517 | 0.8181 | 30.253 | 38/0 | 128/0/71 | 72 | incomplete; coverage differs |
| south/moon/7d | 72.2268 | 5.5744 | 12.957 | 268/0 | 960/0/528 | 506 | incomplete; coverage differs |
| south/moon/30d | 158.3821 | 19.9351 | 7.945 | 1008/0 | 3472/0/1869 | 1654 | incomplete; coverage differs |
| south/moon/365d | 1034.7805 | 195.7075 | 5.287 | 6126/0 | 26284/0/14113 | 12856 | incomplete; coverage differs |
| south/starlight/1d | 0.5956 | 0.6039 | 0.986 | 38/0 | 1/0/0 | 0 | complete; equivalent classification |
| south/starlight/7d | 3.8966 | 3.7358 | 1.043 | 258/0 | 7/0/0 | 0 | complete; equivalent classification |
| south/starlight/30d | 15.6869 | 15.9802 | 0.982 | 976/0 | 30/0/0 | 0 | complete; equivalent classification |
| south/starlight/365d | 189.1260 | 190.5691 | 0.992 | 5900/0 | 268/0/0 | 0 | complete; equivalent classification |
| south/all/year/10targets | 27811.0569 | 573.5199 | 48.492 | 125598/0 | 2845006/0/1534379 | 1556612 | incomplete; coverage differs |
| south/all/year/100targets | 204521.3741 | 4326.2745 | 47.274 | 918794/0 | 21060404/0/11354333 | 11518930 | incomplete; coverage differs |
| south/bright-moon | 95.5571 | 1.1468 | 83.321 | 751/0 | 3064/0/1646 | 1500 | incomplete; coverage differs |
| south/moon-down | 74.0232 | 1.1180 | 66.212 | 626/0 | 2582/0/1384 | 1281 | incomplete; coverage differs |
| south/near-threshold | 267.2192 | 16.6444 | 16.055 | 1322/0 | 30848/0/16620 | 16861 | incomplete; coverage differs |
| south/far-threshold | 255.1481 | 19.9786 | 12.771 | 1253/0 | 30848/0/16620 | 16861 | incomplete; coverage differs |
| north/all/1d | 23.5200 | 0.1381 | 170.254 | 75/0 | 1792/0/968 | 982 | incomplete; coverage differs |
| north/all/7d | 145.6939 | 0.5650 | 257.884 | 516/0 | 11520/0/6225 | 6315 | incomplete; coverage differs |
| north/all/30d | 379.3382 | 2.1321 | 177.915 | 2002/0 | 46720/0/25231 | 25596 | incomplete; coverage differs |
| north/all/365d | 2302.7296 | 24.6255 | 93.510 | 12415/0 | 279419/0/150776 | 152959 | incomplete; coverage differs |
| north/zodiacal/1d | 3.5100 | 0.2516 | 13.948 | 60/0 | 256/0/142 | 144 | incomplete; coverage differs |
| north/zodiacal/7d | 23.2668 | 0.5076 | 45.834 | 400/0 | 1024/0/574 | 582 | incomplete; coverage differs |
| north/zodiacal/30d | 88.4107 | 1.5208 | 58.134 | 1549/0 | 3968/0/2229 | 2260 | incomplete; coverage differs |
| north/zodiacal/365d | 537.8559 | 15.0278 | 35.791 | 9354/0 | 35840/0/19832 | 20112 | incomplete; coverage differs |
| north/airglow/1d | 0.6164 | 0.1896 | 3.251 | 75/0 | 1792/0/968 | 982 | incomplete; coverage differs |
| north/airglow/7d | 3.9346 | 0.6885 | 5.714 | 497/0 | 11136/0/6024 | 6111 | incomplete; coverage differs |
| north/airglow/30d | 14.2713 | 2.5336 | 5.633 | 1945/0 | 44800/0/24224 | 24574 | incomplete; coverage differs |
| north/airglow/365d | 98.6516 | 21.4559 | 4.598 | 11587/0 | 266747/0/144132 | 146216 | incomplete; coverage differs |
| north/moon/1d | 14.9254 | 0.2219 | 67.277 | 60/0 | 256/0/142 | 144 | incomplete; coverage differs |
| north/moon/7d | 93.9144 | 0.5077 | 184.972 | 410/0 | 1127/0/622 | 583 | incomplete; coverage differs |
| north/moon/30d | 199.3733 | 1.3295 | 149.963 | 1578/0 | 3622/0/1965 | 1740 | incomplete; coverage differs |
| north/moon/365d | 1213.5912 | 19.0184 | 63.812 | 9636/0 | 28248/0/15239 | 13852 | incomplete; coverage differs |
| north/starlight/1d | 0.0599 | 0.0828 | 0.723 | 60/0 | 2/0/0 | 0 | complete; equivalent classification |
| north/starlight/7d | 0.2722 | 0.2648 | 1.028 | 400/0 | 8/0/0 | 0 | complete; equivalent classification |
| north/starlight/30d | 1.1666 | 1.1675 | 0.999 | 1549/0 | 31/0/0 | 0 | complete; equivalent classification |
| north/starlight/365d | 14.2943 | 14.1900 | 1.007 | 9354/0 | 280/0/0 | 0 | complete; equivalent classification |
| north/all/year/10targets | 10169.4832 | 717.9805 | 14.164 | 50696/0 | 1175479/0/633255 | 642440 | incomplete; coverage differs |
| north/all/year/100targets | 177047.3356 | 4070.0833 | 43.500 | 906946/0 | 20813506/0/11221898 | 11384547 | incomplete; coverage differs |
| north/bright-moon | 148.6645 | 1.4027 | 105.986 | 1159/0 | 3208/0/1738 | 1585 | incomplete; coverage differs |
| north/moon-down | 121.6077 | 16.0771 | 7.564 | 935/0 | 2596/0/1406 | 1296 | incomplete; coverage differs |
| north/near-threshold | 374.7660 | 2.0891 | 179.388 | 2029/0 | 46720/0/25231 | 25596 | incomplete; coverage differs |
| north/far-threshold | 361.2070 | 2.2165 | 162.966 | 1974/0 | 46720/0/25231 | 25596 | incomplete; coverage differs |

Synthetic close-pair and tangent cases are more expensive than the cheap point scan when their measured ratio is below one; they establish coverage with analytic envelopes and retain uncertainty near contacts. The scan can miss the synthetic close pair entirely.

Real dynamic cases deliberately disable fits until piecewise branches are isolated. `ALL`, Zodiacal and Airglow certify no time; Moon-only certifies only prepared Moon-down interiors. Starlight uses a test-only experimental map and certifies its cached scalar. These results do not support replacing the production scanner.

Additional per-query counters/times, unresolved duration, and site preparation cost are retained in JSONL. Bounds/fit/refinement timers are summed worker elapsed times, not CPU clocks or total wall time. Batch counters aggregate targets/windows; unresolved days are summed target-time. Counters and stage durations come from the final repetition. Peak pending intervals track search-stack size, not all allocations. Whole-process RSS should be collected separately with `/usr/bin/time -v`; no per-algorithm memory-improvement claim is made.

Reproduce:

```bash
RAYON_NUM_THREADS=2 /usr/bin/time -v scripts/benchmark-adaptive-threshold.sh > /tmp/adaptive-benchmark.log
python3 scripts/summarize-adaptive-threshold-benchmark.py /tmp/adaptive-benchmark.log
```
