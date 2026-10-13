#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Validate and combine complete before/after native benchmark logs into JSONL."""
import argparse
import json
from pathlib import Path
import statistics


def records(paths):
    rows = {}
    for path in paths:
        for line in path.read_text().splitlines():
            if line.startswith("DISCOVERY_BENCH "):
                row = json.loads(line.removeprefix("DISCOVERY_BENCH "))
                assert row["scenario"] not in rows, f"duplicate scenario: {row['scenario']}"
                assert (row["complete_usable"] and not row["budget_exhaustions"]
                        and not row["fine_count_disagreements"]), f"incomplete scenario: {row['scenario']}"
                rows[row["scenario"]] = row
    assert len(rows) == 204, f"expected 204 validated scenarios, found {len(rows)}"
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", nargs="+", type=Path, required=True)
    parser.add_argument("--after", nargs="+", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    before, after = records(args.before), records(args.after)
    assert before.keys() == after.keys()
    stable = ["scan_exact", "adaptive_exact", "fits", "subdivisions", "root_refinements",
              "extrema", "accepted_periods", "accepted_duration_seconds", "scan_duration_seconds",
              "reference_duration_seconds", "fine_missed_excursions", "fine_missed_accepted_excursions",
              "fine_singleton_jump_artifacts", "coarse_singleton_jump_artifacts"]
    output = []
    for label in sorted(after):
        a, b = before[label], after[label]
        assert b.get("cold_cache_each_schedule"), f"warm cache measured: {label}"
        for key in stable:
            assert a[key] == b[key], f"scientific/counter change {label}: {key}: {a[key]} != {b[key]}"
        row = dict(b)
        for key in ["adaptive_seconds", "adaptive_end_to_end_seconds", "scan_seconds",
                    "scan_end_to_end_seconds", "preparation_seconds", "process_peak_rss_kib"]:
            row["before_" + key] = a[key]
        output.append(json.dumps(row, sort_keys=True))
        if b["days"] == 365 and b["targets"] == 100:
            med = statistics.median
            print(f'{label}: scan600 {med(a["scan_end_to_end_seconds"]):.3f}s, '
                  f'before {med(a["adaptive_end_to_end_seconds"]):.3f}s, '
                  f'after {med(b["adaptive_end_to_end_seconds"]):.3f}s, '
                  f'exact {a["scan_exact"]}/{b["adaptive_exact"]}')
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text("\n".join(output) + "\n")
    print("204 complete three-way comparisons; scientific outputs/counters preserved")


if __name__ == "__main__":
    main()
