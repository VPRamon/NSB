# Complete CLI process measurements

The same release CLI binary (`3c14f03` production engine, Siderust 0.14 / Tempoch 0.10) runs the sampled default and `--adaptive`. Two Rayon threads are pinned to CPUs 16 and 17 (two efficiency cores), with one warmup per variant and ten alternating timed repetitions. Process startup, preparation, output formatting and I/O are included. Shared package power and memory are not isolated. These absolute times should not be compared directly with the performance-core library matrix.

Every run returns successfully with a complete CSV schedule and a deterministic digest within its variant. Annual outputs differ between discovery contracts, as expected from different crossing refinement and event discovery; this CLI timing exercise alone does not certify those differences. Scientific discrepancy checks are reported by the library corpus. Moon-down schedules are identical.

| Scenario | Scan median s | Adaptive median s | Adaptive/scan | Scan observed p90 s | Adaptive observed p90 s | Peak RSS scan/adaptive KiB |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| CTAO-S/jones2013/all/year | 8.444086 | 14.079073 | 1.667 | 8.566501 | 14.470768 | 11420/11420 |
| CTAO-S/ks1991/all/year | 7.708393 | 12.534911 | 1.626 | 8.485462 | 13.744639 | 11124/11220 |
| CTAO-N/jones2013/all/year | 8.779213 | 15.795956 | 1.799 | 10.202624 | 17.970962 | 11544/11436 |
| CTAO-N/ks1991/all/year | 9.031566 | 16.362191 | 1.812 | 10.151328 | 17.290951 | 11316/11312 |
| PARANAL/jones2013/moon-down/hour | 0.012457 | 0.012547 | 1.007 | 0.014442 | 0.015505 | 10328/10300 |
| PARANAL/ks1991/moon-down/hour | 0.011280 | 0.011274 | 0.999 | 0.011798 | 0.011888 | 10312/10304 |

Ten samples provide an observed p90, not a statistically established tail bound. RSS is whole-process high-water memory, not isolated search allocations. Raw samples, command arguments, binary/output digests and observed maxima are in [JSONL](cli-production.jsonl).

Reproduce after building `target/release/nsb`:

```sh
RAYON_NUM_THREADS=2 taskset -c 16,17 python3 scripts/benchmark-certified-planning-cli.py \
  target/release/nsb target/release/nsb --adaptive --repeats 10
```
