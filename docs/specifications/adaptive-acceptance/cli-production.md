# Complete CLI process measurements

The migrated normal CLI default is compared with the retained pre-migration Siderust 0.14 scanner binary. Both use the same registry Siderust 0.14 / Tempoch 0.10 graph and release compiler. Two Rayon threads are pinned to CPUs 14 and 15 (two efficiency cores), with one warmup per variant and ten alternating timed repetitions. Process startup, immutable evaluator construction, preparation, output formatting and I/O are included. The library matrix runs concurrently on separate physical cores; shared package power, LLC and memory are not isolated. Absolute times are machine/workload observations, not portable latency guarantees.

Every run returns successfully with a complete CSV schedule and a deterministic digest within its variant. Annual outputs differ between discovery contracts; scientific discrepancy checks are provided by the library corpus. Moon-down schedules are identical. The new normal default also reproduces all six previously recorded explicit-adaptive CSV digests exactly; see [dispatch checks](default-dispatch.jsonl).

| Scenario | Scan median s | Adaptive default median s | Adaptive/scan | Scan observed p90 s | Adaptive observed p90 s | Peak RSS scan/adaptive KiB |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| CTAO-S/jones2013/all/year | 6.985247 | 11.440821 | 1.638 | 7.131548 | 11.593353 | 11432/11544 |
| CTAO-S/ks1991/all/year | 6.826935 | 10.930903 | 1.601 | 6.913006 | 11.236408 | 11212/11236 |
| CTAO-N/jones2013/all/year | 7.891401 | 14.784094 | 1.873 | 8.293714 | 14.994583 | 11772/11516 |
| CTAO-N/ks1991/all/year | 7.591780 | 13.602265 | 1.792 | 8.013685 | 13.822993 | 11540/11184 |
| PARANAL/jones2013/moon-down/hour | 0.011172 | 0.011236 | 1.006 | 0.011532 | 0.011694 | 10716/10532 |
| PARANAL/ks1991/moon-down/hour | 0.010821 | 0.010724 | 0.991 | 0.011159 | 0.011168 | 10796/10320 |

Ten samples provide an observed p90, not a statistically established tail bound. RSS is whole-process high-water memory, not isolated search allocations. Raw samples, command arguments, binary/output digests and observed maxima are in [JSONL](cli-production.jsonl).

Reproduce with an explicit retained scanner baseline and the migrated current binary:

```sh
RAYON_NUM_THREADS=2 taskset -c 14,15 python3 scripts/benchmark-certified-planning-cli.py \
  /path/to/scan600-0.14 target/release/nsb --adaptive-default --repeats 10
```
