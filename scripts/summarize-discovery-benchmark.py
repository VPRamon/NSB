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
    "Same release binary, registry Siderust 0.14 / Tempoch 0.10 graph, two Rayon threads, "
    "three alternating timed repetitions after warmup. Both methods return usable native UTC schedules. "
    "The recorded core time includes target preparation, physical partitioning and UTC conversion; "
    "end-to-end time adds the measured shared site preparation once. It excludes process startup, "
    "JSON formatting and I/O. CLI process measurements are reported separately.\n\n",
    "CTAO-N/S, both lunar models, 1/7/30/365 days, 1/10/100 targets, every mask, actual ALL "
    "and explicit four-component composition are included. Starlight is a synthetic "
    "configured benchmark fixture, never an admitted production asset. For compositions including "
    "Starlight, each target's cached Starlight contribution is added to the nominal threshold "
    "so dynamic components still cross; pure Starlight keeps the nominal threshold.\n\n",
    "Compared against 600 s and 30 s (single target) / 120 s (batches) authoritative scanners. "
    "Those references are discrete evidence, not mathematical completeness proofs. Every "
    "validated coarse boundary witness is retained within 0.533 s; specifically verified "
    "singleton jump artifacts are counted separately. Fine-reference period topology "
    "and boundaries are checked before any row is emitted. The separate adversarial tests "
    "resolve the approximately 53 ms Zodiacal event with a local 1 ms reference.\n\n",
    "Three repetitions support medians and observed ranges, not statistical p95 estimates. "
    "Raw values, duration sums, exact evaluations, fits, subdivisions, root refinements, "
    "bracket widths, resource limits, peak pending work and process RSS high-water marks are "
    "in [JSONL](production.jsonl); the [environment](environment.json) records hardware and compiler. "
    "RSS is cumulative whole-process VmHWM, not a per-algorithm memory comparison.\n\n",
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
