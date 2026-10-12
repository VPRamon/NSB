# End-to-end planning CLI measurements

Measured source: `f032535`, on the Intel Core Ultra 9 185H / rustc 1.97.1 environment recorded in the [core environment artifact](adaptive-threshold-review-environment.json), with two Rayon threads. Previous binary retains the unchanged production implementation from main `a3f6752`, before integration. Binary checksums, exact arguments, all timings, per-run maximum RSS and output digests are in [raw JSONL](adaptive-threshold-cli-benchmarks.jsonl).

Both binaries warm up once per scenario. Twenty timed repetitions alternate their order. Each measurement includes process startup, evaluator/site/target preparation, search, UTC conversion and CSV output. Output is compared byte-for-byte on every run. The ALL rows retain the existing discrete discovery contract; **they do not establish continuous-time completeness**. Only the Moon-down rows run the new strict certified method, with no scan fallback. Their certificates are conditional on the existing prepared lunar-event periods. In this build the frozen ALL mask is bits 13 (Zodiacal/Airglow/Moon); all-four bits 15 is covered separately by correctness tests with an explicit Starlight fixture.

Observed p90 is the 18th ordered value among twenty trials. It is a sample diagnostic, not a reliable population-tail guarantee. RSS is measured for each CLI process by `/usr/bin/time`, excluding compiler and parent Python memory.

| Scenario | Previous median ms | Current median ms | Previous / current observed p90 ms | Previous / current median peak RSS KiB | Contract |
| --- | ---: | ---: | ---: | ---: | --- |
| CTAO-S/jones2013/all/year | 3093.072 | 3104.238 | 3456.381 / 3603.667 | 10436 / 10812 | discrete compatibility |
| CTAO-S/ks1991/all/year | 3281.412 | 3150.965 | 3470.590 / 3524.532 | 10212 / 10676 | discrete compatibility |
| CTAO-N/jones2013/all/year | 3704.256 | 3702.793 | 4096.846 / 3918.428 | 10562 / 11034 | discrete compatibility |
| CTAO-N/ks1991/all/year | 3479.686 | 3475.980 | 3560.025 / 3588.163 | 10458 / 10742 | discrete compatibility |
| PARANAL/jones2013/moon-down/hour | 8.186 | 7.262 | 10.000 / 9.683 | 9522 / 10028 | complete certified Moon-down |
| PARANAL/ks1991/moon-down/hour | 8.409 | 7.531 | 9.918 / 9.440 | 9618 / 10062 | complete certified Moon-down |

Annual ALL median changes range from -4.0% to +0.4%; observed p90 changes reach +4.3%. No material production speedup is demonstrated. These results validate compatibility overhead, not the missing certified-ALL objective. All outputs are byte-identical across the paired runs, including the two strict Moon-down cases.

Reproduce after retaining a release baseline binary built from main:

```bash
CARGO_BUILD_JOBS=2 cargo build --release --locked -p nsb-cli
python3 scripts/benchmark-certified-planning-cli.py /path/to/baseline-nsb target/release/nsb > /tmp/cli-benchmark.jsonl
```
