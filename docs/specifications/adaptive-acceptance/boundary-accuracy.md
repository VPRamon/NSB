# Observed native boundary accuracy

Seven-day window starting 2026-01-03 UTC, both CTAO sites, both lunar models and all 16 masks: **64 cases**. The maximum observed matched boundary difference from a 30-second authoritative reference is **0.415292310 seconds**, below the unchanged 0.533-second allowance. This is reference agreement, not absolute astronomical time error; the scanner itself returns midpoint boundaries from brackets up to 0.864 seconds wide.

All **716 exported native UTC boundaries** satisfy the exact integrated maximum constraint after conversion back to TT; zero rejected output endpoints were found. There are 712 matched fine-reference boundaries, 0 unpaired reference boundaries, 0 singleton artifacts and 2 independently confirmed rejected excursions missed by scan30. The latter are the PALACE/Jones events in masks 12 and 14; new adaptive edges require independent dense local validation rather than being silently dropped from comparison.

Physical eligibility boundaries are often exactly shared, so a zero median over all pairs is not a root-refinement accuracy claim. Every paired error is retained in [JSONL](boundary-accuracy.jsonl); counts are in [summary JSON](boundary-accuracy-summary.json). Starlight composition thresholds add the prepared scalar to the nominal 0.25, except pure Starlight, matching the complete benchmark protocol.

| Site | Moonlight | Mask bits | Paired boundaries | Maximum paired error s | Native endpoints checked | Confirmed reference misses |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| CTAO-S | jones-2013-spectral | 0 | 14 | 0.000000000 | 14 | 0 |
| CTAO-S | jones-2013-spectral | 1 | 14 | 0.000000000 | 14 | 0 |
| CTAO-S | jones-2013-spectral | 2 | 0 | 0.000000000 | 0 | 0 |
| CTAO-S | jones-2013-spectral | 3 | 14 | 0.000000000 | 14 | 0 |
| CTAO-S | jones-2013-spectral | 4 | 14 | 0.000000000 | 14 | 0 |
| CTAO-S | jones-2013-spectral | 5 | 14 | 0.000000000 | 14 | 0 |
| CTAO-S | jones-2013-spectral | 6 | 14 | 0.000000000 | 14 | 0 |
| CTAO-S | jones-2013-spectral | 7 | 14 | 0.000000000 | 14 | 0 |
| CTAO-S | jones-2013-spectral | 8 | 8 | 0.031393416 | 8 | 0 |
| CTAO-S | jones-2013-spectral | 9 | 8 | 0.401950359 | 8 | 0 |
| CTAO-S | jones-2013-spectral | 10 | 8 | 0.009855278 | 8 | 0 |
| CTAO-S | jones-2013-spectral | 11 | 8 | 0.401950359 | 8 | 0 |
| CTAO-S | jones-2013-spectral | 12 | 8 | 0.342023896 | 8 | 0 |
| CTAO-S | jones-2013-spectral | 13 | 8 | 0.313330220 | 8 | 0 |
| CTAO-S | jones-2013-spectral | 14 | 8 | 0.342023896 | 8 | 0 |
| CTAO-S | jones-2013-spectral | 15 | 8 | 0.313330220 | 8 | 0 |
| CTAO-S | krisciunas-schaefer-1991 | 0 | 14 | 0.000000000 | 14 | 0 |
| CTAO-S | krisciunas-schaefer-1991 | 1 | 14 | 0.000000000 | 14 | 0 |
| CTAO-S | krisciunas-schaefer-1991 | 2 | 0 | 0.000000000 | 0 | 0 |
| CTAO-S | krisciunas-schaefer-1991 | 3 | 14 | 0.000000000 | 14 | 0 |
| CTAO-S | krisciunas-schaefer-1991 | 4 | 14 | 0.000000000 | 14 | 0 |
| CTAO-S | krisciunas-schaefer-1991 | 5 | 14 | 0.000000000 | 14 | 0 |
| CTAO-S | krisciunas-schaefer-1991 | 6 | 14 | 0.000000000 | 14 | 0 |
| CTAO-S | krisciunas-schaefer-1991 | 7 | 14 | 0.000000000 | 14 | 0 |
| CTAO-S | krisciunas-schaefer-1991 | 8 | 8 | 0.390631080 | 8 | 0 |
| CTAO-S | krisciunas-schaefer-1991 | 9 | 8 | 0.415292310 | 8 | 0 |
| CTAO-S | krisciunas-schaefer-1991 | 10 | 8 | 0.390631080 | 8 | 0 |
| CTAO-S | krisciunas-schaefer-1991 | 11 | 8 | 0.415292310 | 8 | 0 |
| CTAO-S | krisciunas-schaefer-1991 | 12 | 8 | 0.371086167 | 8 | 0 |
| CTAO-S | krisciunas-schaefer-1991 | 13 | 8 | 0.332509875 | 8 | 0 |
| CTAO-S | krisciunas-schaefer-1991 | 14 | 8 | 0.371086167 | 8 | 0 |
| CTAO-S | krisciunas-schaefer-1991 | 15 | 8 | 0.332509875 | 8 | 0 |
| CTAO-N | jones-2013-spectral | 0 | 16 | 0.000000000 | 16 | 0 |
| CTAO-N | jones-2013-spectral | 1 | 16 | 0.000000000 | 16 | 0 |
| CTAO-N | jones-2013-spectral | 2 | 0 | 0.000000000 | 0 | 0 |
| CTAO-N | jones-2013-spectral | 3 | 16 | 0.000000000 | 16 | 0 |
| CTAO-N | jones-2013-spectral | 4 | 16 | 0.000000000 | 16 | 0 |
| CTAO-N | jones-2013-spectral | 5 | 16 | 0.000000000 | 16 | 0 |
| CTAO-N | jones-2013-spectral | 6 | 16 | 0.000000000 | 16 | 0 |
| CTAO-N | jones-2013-spectral | 7 | 16 | 0.000000000 | 16 | 0 |
| CTAO-N | jones-2013-spectral | 8 | 10 | 0.056952394 | 10 | 0 |
| CTAO-N | jones-2013-spectral | 9 | 10 | 0.082906008 | 10 | 0 |
| CTAO-N | jones-2013-spectral | 10 | 10 | 0.057314495 | 10 | 0 |
| CTAO-N | jones-2013-spectral | 11 | 10 | 0.082906008 | 10 | 0 |
| CTAO-N | jones-2013-spectral | 12 | 12 | 0.343299731 | 14 | 1 |
| CTAO-N | jones-2013-spectral | 13 | 10 | 0.362661051 | 10 | 0 |
| CTAO-N | jones-2013-spectral | 14 | 12 | 0.343299731 | 14 | 1 |
| CTAO-N | jones-2013-spectral | 15 | 10 | 0.362661051 | 10 | 0 |
| CTAO-N | krisciunas-schaefer-1991 | 0 | 16 | 0.000000000 | 16 | 0 |
| CTAO-N | krisciunas-schaefer-1991 | 1 | 16 | 0.000000000 | 16 | 0 |
| CTAO-N | krisciunas-schaefer-1991 | 2 | 0 | 0.000000000 | 0 | 0 |
| CTAO-N | krisciunas-schaefer-1991 | 3 | 16 | 0.000000000 | 16 | 0 |
| CTAO-N | krisciunas-schaefer-1991 | 4 | 16 | 0.000000000 | 16 | 0 |
| CTAO-N | krisciunas-schaefer-1991 | 5 | 16 | 0.000000000 | 16 | 0 |
| CTAO-N | krisciunas-schaefer-1991 | 6 | 16 | 0.000000000 | 16 | 0 |
| CTAO-N | krisciunas-schaefer-1991 | 7 | 16 | 0.000000000 | 16 | 0 |
| CTAO-N | krisciunas-schaefer-1991 | 8 | 10 | 0.353798436 | 10 | 0 |
| CTAO-N | krisciunas-schaefer-1991 | 9 | 10 | 0.292794870 | 10 | 0 |
| CTAO-N | krisciunas-schaefer-1991 | 10 | 10 | 0.353798436 | 10 | 0 |
| CTAO-N | krisciunas-schaefer-1991 | 11 | 10 | 0.292794870 | 10 | 0 |
| CTAO-N | krisciunas-schaefer-1991 | 12 | 10 | 0.372187423 | 10 | 0 |
| CTAO-N | krisciunas-schaefer-1991 | 13 | 10 | 0.378106760 | 10 | 0 |
| CTAO-N | krisciunas-schaefer-1991 | 14 | 10 | 0.372187423 | 10 | 0 |
| CTAO-N | krisciunas-schaefer-1991 | 15 | 10 | 0.378106760 | 10 | 0 |

Reproduce with `RAYON_NUM_THREADS=2 cargo test --release --locked -p nsb --lib physical_boundary_accuracy_reference -- --ignored --nocapture`. This empirical export complements the analytic crossing, joint-band and narrow-event regressions; it cannot prove completeness between probes.
