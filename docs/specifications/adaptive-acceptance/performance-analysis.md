# Performance analysis and default decision

Adaptive is not a general speedup over scan600. It performs more authoritative evaluations to validate the physical fits, resolve extrema and refine accepted-side boundaries to 0.1 seconds. The earlier aliasing fix requires all interlaced and independent interior probes. High-fit-only probes remain lazy, and redundant PALACE night-third fit boundaries are omitted without changing its model.

The largest measured absolute overheads are below. Each ratio is paired on identical workers/cores within its row; cross-group absolute times have different core allocations. Evaluation counts aggregate all target queries.

| Scenario | Scan E2E s | Adaptive E2E s | Latency ratio | Physical call ratio | Fits | Subdivisions | Root refinements |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| north/jones-2013-spectral/batch/365d/100 | 220.780 | 551.445 | 2.498 | 2.281 | 163081 | 757 | 9543 |
| north/krisciunas-schaefer-1991/batch/365d/100 | 209.665 | 522.526 | 2.492 | 2.240 | 163033 | 0 | 9914 |
| south/jones-2013-spectral/batch/365d/100 | 215.075 | 513.663 | 2.388 | 2.214 | 160470 | 700 | 10484 |
| south/krisciunas-schaefer-1991/batch/365d/100 | 184.834 | 450.161 | 2.435 | 2.171 | 160717 | 0 | 10540 |
| south/jones-2013-spectral/batch/365d/10 | 30.502 | 66.063 | 2.166 | 2.187 | 21575 | 56 | 1490 |
| south/krisciunas-schaefer-1991/batch/365d/10 | 26.547 | 61.128 | 2.303 | 2.155 | 21782 | 0 | 1471 |
| north/jones-2013-spectral/batch/30d/100 | 19.835 | 48.515 | 2.446 | 2.312 | 15743 | 75 | 549 |
| north/krisciunas-schaefer-1991/batch/30d/100 | 19.866 | 47.596 | 2.396 | 2.251 | 15579 | 0 | 680 |
| north/krisciunas-schaefer-1991/batch/365d/10 | 13.589 | 31.784 | 2.339 | 2.273 | 9180 | 0 | 623 |
| south/jones-2013-spectral/batch/30d/100 | 14.123 | 32.114 | 2.274 | 2.179 | 10974 | 87 | 484 |
| south/krisciunas-schaefer-1991/batch/30d/100 | 11.558 | 28.161 | 2.437 | 2.084 | 10771 | 0 | 648 |
| north/jones-2013-spectral/batch/365d/10 | 12.324 | 27.949 | 2.268 | 2.305 | 9153 | 27 | 612 |

Annual 100-target adaptive medians are 450.161–551.445 seconds (7.50–9.19 minutes). Paired latency ratios are 2.388–2.498, alongside 2.171–2.281 times the physical evaluations. The increased calls explain most of the overhead; the remainder includes fitting, extremum/root work and bounded batch scheduling. The annual counters show no uncontrolled recursive expansion or budget exhaustion.

The maximum cumulative whole-process RSS observed in the matrix is 24684 KiB (24.11 MiB). It includes immutable model data and benchmark copies of warmup/reference/timed schedules; it is not isolated per-algorithm memory. Cache payload and pending/depth peaks are reported separately in the complete data.

The tradeoff is accepted for offline annual planning: all 204 usable scenarios complete with bounded resources, preserve validated coarse witnesses and resolve independently confirmed events/artifacts missed by the references. No interactive latency or statistical tail guarantee is inferred from three repetitions. The maintainer explicitly authorized this behavioral migration within the 0.1 line. Normal Rust/Python/CLI methods therefore use adaptive discovery, with ignored legacy step inputs retained and the scanner moved to test-only references.

This decision does not turn empirical model resolution into mathematical completeness. Strict active-model interval envelopes remain unsupported. See [scientific contracts](../adaptive-threshold-production-status.md), [full measurements](production.md) and [validation](validation.md).
