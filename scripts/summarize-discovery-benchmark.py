#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Summarize usable adaptive schedules; reject incomplete acceptance matrices."""
import json
from pathlib import Path
import statistics
import sys

source = Path(sys.argv[1]).read_text()
rows = [json.loads(line.split("DISCOVERY_BENCH ", 1)[1])
        for line in source.splitlines() if "DISCOVERY_BENCH " in line]
if len(rows) != 204 or len({r["scenario"] for r in rows}) != 204:
    raise SystemExit(f"expected 204 unique complete cases, found {len(rows)}")
if not all(r["complete_usable"] and not r["budget_exhaustions"]
           and not r["fine_count_disagreements"] for r in rows):
    raise SystemExit("acceptance matrix includes incomplete/disagreeing outcomes")
output = Path(__file__).resolve().parents[1] / "docs" / "specifications" / "adaptive-acceptance"
output.mkdir(exist_ok=True)
(output / "production.jsonl").write_text("".join(json.dumps(r, sort_keys=True) + "\n" for r in rows))
lines = [
    "# Complete adaptive schedule measurements\n\n",
    "Both algorithms are compiled into one release benchmark binary: the recorded production adaptive call graph and the unchanged test-only scanner, using the registry Siderust 0.14 / Tempoch 0.10 graph, paired fixed worker/core assignments per group (two Rayon threads on two performance cores for three groups; four threads on four efficiency cores for South K&S), "
    "three alternating timed repetitions after warmup. Both methods return usable native UTC schedules. "
    "The recorded core time includes target preparation, physical partitioning and UTC conversion; "
    "end-to-end time adds the measured shared site preparation once. It excludes process startup, "
    "JSON formatting and I/O. An existing evaluator is reused; immutable model and fixture construction are outside library timers. CLI process measurements include construction and are reported separately.\n\n",
    "CTAO-N/S, both lunar models, 1/7/30/365 days, 1/10/100 targets, every mask, actual ALL "
    "and explicit four-component composition are included. Starlight is a synthetic "
    "configured benchmark fixture, never an admitted production asset. For compositions including "
    "Starlight, each target's cached Starlight contribution is added to the nominal threshold "
    "so dynamic components still cross; pure Starlight keeps the nominal threshold.\n\n",
    "Compared against 600 s and 30 s (single target) / 120 s (batches) authoritative scanners. "
    "Those references are discrete evidence, not mathematical completeness proofs. Every "
    "validated coarse boundary witness is retained within 0.533 s; specifically verified "
    "singleton jump artifacts are counted separately. Fine-reference topology differences require independent dense local validation "
    "of every additional rejected excursion (including physical-window edges); unexplained differences fail before any row is emitted. The separate adversarial tests "
    "resolve the approximately 53 ms Zodiacal event and 2.4 s Airglow/Moonlight excursion with local 1 ms references.\n\n",
    "Three repetitions support medians and observed ranges, not statistical p95 estimates. "
    "Raw values, duration sums, exact evaluations, fits, subdivisions, root refinements, "
    "bracket widths, resource limits, peak pending work and process RSS high-water marks are "
    "in [JSONL](production.jsonl); the [environment](environment.json) records hardware and compiler. "
    "RSS is cumulative whole-process VmHWM, not a per-algorithm memory comparison. Cache byte counts are active sample payload, excluding allocator/capacity overhead. Exact counts are integrated search probes; target/site preparation is included in time but not in those counts. Batch counters and duration sums aggregate target queries; budgets apply to each ThresholdQuery, while peaks are maxima across targets.\n\n",
    "| Scenario | Scan core ms | Adaptive core ms | Scan E2E ms | Adaptive E2E ms | Adaptive/scan E2E | Exact scan/adaptive | Fits / splits / roots |\n",
    "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n",
]
for r in rows:
    med = lambda key: statistics.median(r[key])
    old, new = med("scan_end_to_end_seconds"), med("adaptive_end_to_end_seconds")
    lines.append(f'| {r["scenario"]} | {med("scan_seconds")*1000:.3f} | '
                 f'{med("adaptive_seconds")*1000:.3f} | {old*1000:.3f} | {new*1000:.3f} | '
                 f'{new/old:.3f} | {r["scan_exact"]}/{r["adaptive_exact"]} | '
                 f'{r["fits"]}/{r["subdivisions"]}/{r["root_refinements"]} |\n')
lines.append("\nAll 204 rows passed their finite reference checks without budget exhaustion. "
             "No speedup is inferred from an incomplete search. See the production status "
             "document for the default-engine decision and remaining limits.\n")
(output / "production.md").write_text("".join(lines))
print("wrote 204 complete schedule cases")

# A compact decision record alongside the complete per-scenario table.
summary = [
    "# Production performance acceptance\n\n",
    "These are paired medians from three repetitions on the worker/core assignments "
    "in [environment](environment.json). Each row uses identical hardware and workers "
    "for both engines. Absolute times across groups with different core types are "
    "not directly comparable. Full samples and all 204 scenarios are in "
    "[the complete report](production.md).\n\n",
    "## Annual planning\n\n",
    "| Site / Moonlight | Targets | Scan E2E s | Adaptive E2E s | Adaptive/scan |\n",
    "| --- | ---: | ---: | ---: | ---: |\n",
]
annual = sorted((r for r in rows if r["days"] == 365 and r["components_bits"] == 13),
                key=lambda r: (r["site"], r["moonlight_model"], r["targets"]))
for r in annual:
    old = statistics.median(r["scan_end_to_end_seconds"])
    new = statistics.median(r["adaptive_end_to_end_seconds"])
    summary.append(f'| {r["site"]} / {r["moonlight_model"]} | {r["targets"]} | '
                   f'{old:.3f} | {new:.3f} | {new/old:.3f} |\n')
summary += ["\n## Component costs\n\n",
            "Ranges below are paired adaptive/scan E2E ratios over the four site/model "
            "annual single-target cases, rather than pooled cross-hardware timings.\n\n",
            "| Component | Minimum ratio | Maximum ratio |\n",
            "| --- | ---: | ---: |\n"]
for name, bits in [("Starlight", 2), ("Zodiacal", 1), ("Airglow", 4),
                   ("Moonlight", 8), ("ALL", 13), ("Explicit four", 15)]:
    matching = [r for r in rows if r["days"] == 365 and r["targets"] == 1
                and r["components_bits"] == bits]
    ratios = [statistics.median(r["adaptive_end_to_end_seconds"])
              / statistics.median(r["scan_end_to_end_seconds"]) for r in matching]
    summary.append(f'| {name} | {min(ratios):.3f} | {max(ratios):.3f} |\n')
summary += ["\n## Resources and scientific comparison\n\n",
            f'- Complete usable scenarios: {len(rows)}; budget exhaustions: '
            f'{sum(r["budget_exhaustions"] for r in rows)}; unexplained fine discrepancies: '
            f'{sum(r["fine_count_disagreements"] for r in rows)}.\n',
            f'- Independently confirmed rejected excursions missed by the fine scanner: '
            f'{sum(r["fine_missed_excursions"] for r in rows)}. Counts are scenario occurrences, '
            'not deduplicated astronomical events.\n',
            f'- Independently confirmed accepted islands missed by the fine scanner: '
            f'{sum(r["fine_missed_accepted_excursions"] for r in rows)} scenario occurrences.\n',
            f'- Fine/coarse singleton midpoint artifacts: '
            f'{sum(r["fine_singleton_jump_artifacts"] for r in rows)}/'
            f'{sum(r["coarse_singleton_jump_artifacts"] for r in rows)}.\n',
            f'- Maximum pending physical intervals: {max(r["peak_pending_intervals"] for r in rows)}; '
            f'maximum recursion depth: {max(r["peak_recursion_depth"] for r in rows)}; '
            f'maximum active sample payload per worker: '
            f'{max(r["peak_sample_cache_bytes_per_worker"] for r in rows)} bytes '
            '(excludes allocator/capacity overhead).\n',
            '\nWhole-process cumulative RSS and per-scenario physical calls, fits, subdivisions, '
            'root refinements and duration totals remain in the raw JSONL. These empirical '
            'comparisons do not establish continuous-time completeness.\n']
(output / "performance-summary.md").write_text("".join(summary))

# Explain the largest absolute costs, rather than hiding them behind averages.
analysis = [
    "# Performance analysis and default decision\n\n",
    "Adaptive is not a general speedup over scan600. It performs more authoritative "
    "evaluations to validate the physical fits, resolve extrema and refine accepted-side "
    "boundaries to 0.1 seconds. The earlier aliasing fix requires all interlaced and "
    "independent interior probes. High-fit-only probes remain lazy, and redundant "
    "PALACE night-third fit boundaries are omitted without changing its model.\n\n",
    "The largest measured absolute overheads are below. Each ratio is paired on "
    "identical workers/cores within its row; cross-group absolute times have different "
    "core allocations. Evaluation counts aggregate all target queries.\n\n",
    "| Scenario | Scan E2E s | Adaptive E2E s | Latency ratio | Physical call ratio | Fits | Subdivisions | Root refinements |\n",
    "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n",
]
ordered = sorted(rows, key=lambda r: statistics.median(r["adaptive_end_to_end_seconds"])
                 - statistics.median(r["scan_end_to_end_seconds"]), reverse=True)
for r in ordered[:12]:
    old = statistics.median(r["scan_end_to_end_seconds"])
    new = statistics.median(r["adaptive_end_to_end_seconds"])
    calls = r["adaptive_exact"] / r["scan_exact"] if r["scan_exact"] else 0.0
    analysis.append(f'| {r["scenario"]} | {old:.3f} | {new:.3f} | {new/old:.3f} | '
                    f'{calls:.3f} | {r["fits"]} | {r["subdivisions"]} | {r["root_refinements"]} |\n')
annual100 = [r for r in annual if r["targets"] == 100]
latencies = [statistics.median(r["adaptive_end_to_end_seconds"]) for r in annual100]
ratios = [statistics.median(r["adaptive_end_to_end_seconds"])
          / statistics.median(r["scan_end_to_end_seconds"]) for r in annual100]
call_ratios = [r["adaptive_exact"] / r["scan_exact"] for r in annual100]
peak_rss = max(int(r["process_peak_rss_kib"].split()[1]) for r in rows
               if r["process_peak_rss_kib"])
analysis += [
    f'\nAnnual 100-target adaptive medians are {min(latencies):.3f}–{max(latencies):.3f} seconds '
    f'({min(latencies)/60:.2f}–{max(latencies)/60:.2f} minutes). Paired latency ratios are '
    f'{min(ratios):.3f}–{max(ratios):.3f}, alongside '
    f'{min(call_ratios):.3f}–{max(call_ratios):.3f} times the physical evaluations. '
    'The increased calls explain most of the overhead; the remainder includes fitting, '
    'extremum/root work and bounded batch scheduling. The annual counters show no '
    'uncontrolled recursive expansion or budget exhaustion.\n\n',
    f'The maximum cumulative whole-process RSS observed in the matrix is {peak_rss} KiB '
    f'({peak_rss/1024:.2f} MiB). It includes immutable model data and benchmark copies of '
    'warmup/reference/timed schedules; it is not isolated per-algorithm memory. Cache '
    'payload and pending/depth peaks are reported separately in the complete data.\n\n',
    'The tradeoff is accepted for offline annual planning: all 204 usable scenarios '
    'complete with bounded resources, preserve validated coarse witnesses and resolve '
    'independently confirmed events/artifacts missed by the references. No interactive '
    'latency or statistical tail guarantee is inferred from three repetitions. The '
    'maintainer explicitly authorized this behavioral migration within the 0.1 line. '
    'Normal Rust/Python/CLI methods therefore use adaptive discovery, with ignored '
    'legacy step inputs retained and the scanner moved to test-only references.\n\n',
    'This decision does not turn empirical model resolution into mathematical '
    'completeness. Strict active-model interval envelopes remain unsupported. '
    'See [scientific contracts](../adaptive-threshold-production-status.md), '
    '[full measurements](production.md) and [validation](validation.md).\n',
]
(output / "performance-analysis.md").write_text("".join(analysis))
