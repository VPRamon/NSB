#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Extract #234 harness JSONL and render medians without implying equivalence."""
import json
from pathlib import Path
import statistics
import sys

source = Path(sys.argv[1]).read_text()
stem = sys.argv[2] if len(sys.argv) > 2 else "adaptive-threshold-benchmarks"
if Path(stem).name != stem:
    raise SystemExit("output stem must be a filename, not a path")
rows = [json.loads(line.split("ADAPTIVE_BENCH ", 1)[1])
        for line in source.splitlines() if "ADAPTIVE_BENCH " in line]
if not rows or len({r["scenario"] for r in rows}) != len(rows):
    raise SystemExit("expected non-empty, unique benchmark scenarios")
for row in rows:
    # The first experiment used a misleading JSON label for Instant durations.
    # Normalize naming only: these are summed worker elapsed times, not CPU clocks.
    for phase in ("bounds", "fits", "refinement"):
        legacy = f"{phase}_cpu_seconds"
        if legacy in row:
            row[f"{phase}_worker_elapsed_seconds"] = row.pop(legacy)
output = Path(__file__).resolve().parents[1] / "docs" / "specifications"
command_prefix = ""
if all(row.get("synthetic_fit_depth") == 0 for row in rows):
    command_prefix = "NSB_ADAPTIVE_FILTER=synthetic NSB_ADAPTIVE_SYNTHETIC_FIT_DEPTH=0 "
(output / f"{stem}.jsonl").write_text(
    "".join(json.dumps(row, sort_keys=True) + "\n" for row in rows))
lines = [
    "# Adaptive threshold feasibility measurements\n\n",
    "Measured 2026-10-12 against main `a3f6752`. See "
    "[methodology and limitations](adaptive-threshold-production-status.md). "
    "Raw repetitions/counters are in "
    "[JSONL](adaptive-threshold-benchmarks.jsonl).\n\n",
    "The previous scanner has a discrete 600 s discovery contract. "
    "The measured discovery engine has certified/incomplete semantics. "
    "**A lower runtime for an incomplete answer is not a speedup for an "
    "equivalent observing-window result.** `ratio` is previous/prototype "
    "wall time only. Values below 1 mean discovery is slower.\n\n",
    "Both paths use the same release binary, evaluator, input, reused context "
    "and two Rayon threads. Site preparation and final UTC conversion/output "
    "are excluded. Each has one warmup and three timed repetitions (synthetic "
    "cases batch 100 calls per repetition). Medians below are milliseconds. "
    "Three repetitions do not support a meaningful p95 estimate.\n\n",
    "| Scenario | Previous median ms | Discovery median ms | Runtime ratio | "
    "Exact calls old/new | Bounds / fits / subdivisions | Unresolved regions | Outcome |\n",
    "| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |\n",
]
for row in rows:
    old = statistics.median(row["previous_seconds"])
    new = statistics.median(row["prototype_seconds"])
    outcome = "complete" if row["prototype_complete"] else "incomplete"
    if row["comparable_certification"]:
        outcome += "; equivalent classification"
    else:
        outcome += "; coverage differs"
    lines.append(
        f'| {row["scenario"]} | {old * 1000:.4f} | {new * 1000:.4f} | '
        f'{old / new:.3f} | {row["previous_exact"]}/{row["prototype_exact"]} | '
        f'{row["bound_computations"]}/{row["fits"]}/{row["subdivisions"]} | '
        f'{row["unresolved_intervals"]} | {outcome} |\n')
lines += [
    "\nSynthetic close-pair and tangent cases are more expensive than the "
    "cheap point scan when their measured ratio is below one; they establish "
    "coverage with analytic envelopes and retain uncertainty near contacts. "
    "The scan can miss the synthetic close pair entirely.\n\n",
    "Real dynamic cases deliberately disable fits until piecewise branches "
    "are isolated. `ALL`, Zodiacal and Airglow certify no time; Moon-only "
    "certifies only prepared Moon-down interiors. Starlight uses a test-only "
    "experimental map and certifies its cached scalar. These results do not "
    "support replacing the production scanner.\n\n",
    "Additional per-query counters/times, unresolved duration, and site "
    "preparation cost are retained in JSONL. Bounds/fit/refinement timers "
    "are summed worker elapsed times, not CPU clocks or total wall time. "
    "Batch counters aggregate targets/windows; unresolved days are summed target-time. "
    "Counters and stage durations come from the final repetition. "
    "Peak pending intervals track work-queue size, not all allocations. Whole-process "
    "RSS should be collected separately with `/usr/bin/time -v`; no "
    "per-algorithm memory-improvement claim is made.\n\n",
    "Reproduce:\n\n```bash\n",
    command_prefix + "RAYON_NUM_THREADS=2 /usr/bin/time -v scripts/benchmark-adaptive-threshold.sh "
    "> /tmp/adaptive-benchmark.log\n",
    "python3 scripts/summarize-adaptive-threshold-benchmark.py "
    f"/tmp/adaptive-benchmark.log {stem}\n```\n",
]
(output / f"{stem}.md").write_text("".join(lines).replace("adaptive-threshold-benchmarks.jsonl", f"{stem}.jsonl"))
print(f"wrote {len(rows)} benchmark scenarios")
