# Production performance acceptance

These are paired medians from three repetitions on the worker/core assignments in [environment](environment.json). Each row uses identical hardware and workers for both engines. Absolute times across groups with different core types are not directly comparable. Full samples and all 204 scenarios are in [the complete report](production.md).

## Annual planning

| Site / Moonlight | Targets | Scan E2E s | Adaptive E2E s | Adaptive/scan |
| --- | ---: | ---: | ---: | ---: |
| CTAO-N / jones-2013-spectral | 1 | 4.248 | 8.549 | 2.013 |
| CTAO-N / jones-2013-spectral | 10 | 12.324 | 27.949 | 2.268 |
| CTAO-N / jones-2013-spectral | 100 | 220.780 | 551.445 | 2.498 |
| CTAO-N / krisciunas-schaefer-1991 | 1 | 4.776 | 8.271 | 1.732 |
| CTAO-N / krisciunas-schaefer-1991 | 10 | 13.589 | 31.784 | 2.339 |
| CTAO-N / krisciunas-schaefer-1991 | 100 | 209.665 | 522.526 | 2.492 |
| CTAO-S / jones-2013-spectral | 1 | 3.599 | 6.456 | 1.794 |
| CTAO-S / jones-2013-spectral | 10 | 30.502 | 66.063 | 2.166 |
| CTAO-S / jones-2013-spectral | 100 | 215.075 | 513.663 | 2.388 |
| CTAO-S / krisciunas-schaefer-1991 | 1 | 4.787 | 7.169 | 1.497 |
| CTAO-S / krisciunas-schaefer-1991 | 10 | 26.547 | 61.128 | 2.303 |
| CTAO-S / krisciunas-schaefer-1991 | 100 | 184.834 | 450.161 | 2.435 |

## Component costs

Ranges below are paired adaptive/scan E2E ratios over the four site/model annual single-target cases, rather than pooled cross-hardware timings.

| Component | Minimum ratio | Maximum ratio |
| --- | ---: | ---: |
| Starlight | 0.999 | 1.013 |
| Zodiacal | 1.140 | 1.316 |
| Airglow | 1.051 | 1.250 |
| Moonlight | 1.100 | 1.431 |
| ALL | 1.497 | 2.013 |
| Explicit four | 1.418 | 1.867 |

## Resources and scientific comparison

- Complete usable scenarios: 204; budget exhaustions: 0; unexplained fine discrepancies: 0.
- Independently confirmed rejected excursions missed by the fine scanner: 105. Counts are scenario occurrences, not deduplicated astronomical events.
- Independently confirmed accepted islands missed by the fine scanner: 13 scenario occurrences.
- Fine/coarse singleton midpoint artifacts: 12317/12391.
- Maximum pending physical intervals: 64; maximum recursion depth: 6; maximum active sample payload per worker: 9024 bytes (excludes allocator/capacity overhead).

Whole-process cumulative RSS and per-scenario physical calls, fits, subdivisions, root refinements and duration totals remain in the raw JSONL. These empirical comparisons do not establish continuous-time completeness.
