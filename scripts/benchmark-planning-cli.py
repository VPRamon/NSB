#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Paired complete CLI process timings: scan600, adaptive before, adaptive after."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess
import tempfile
import time


def invoke(binary, arguments, memory_file):
    started = time.perf_counter()
    result = subprocess.run(
        ["/usr/bin/time", "-f", "%M", "-o", str(memory_file), str(binary), *arguments],
        capture_output=True, check=True, env={**os.environ, "RAYON_NUM_THREADS": "2"},
    )
    return (time.perf_counter() - started, int(memory_file.read_text().strip()),
            hashlib.sha256(result.stdout).hexdigest())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ["scan600", "before", "after"]:
        parser.add_argument(name, type=Path, help="preserved release CLI executable")
    parser.add_argument("--repeats", type=int, default=10)
    args = parser.parse_args()
    if args.repeats < 3:
        parser.error("at least three paired repetitions are required")
    binaries = [args.scan600.resolve(), args.before.resolve(), args.after.resolve()]
    cases = []
    for site, profile in [("CTAO-S", "cta-south"), ("CTAO-N", "cta-north")]:
        for moon in ["jones2013", "ks1991"]:
            arguments = ["--format", "csv", "window", "--start", "2026-01-01T00:00:00Z",
                         "--end", "2027-01-01T00:00:00Z", "--site", site,
                         "--site-profile", profile, "--ra", "83.6331", "--dec", "22.0145",
                         "--max-nsb", "0.25", "--sun-altitude-max", "-18",
                         "--target-altitude-min", "20", "--components", "all",
                         "--moonlight-model", moon]
            cases.append((f"{site}/{moon}/all/year", arguments))
    for moon in ["jones2013", "ks1991"]:
        arguments = ["--format", "csv", "window", "--start", "2026-01-14T01:00:00Z",
                     "--end", "2026-01-14T02:00:00Z", "--site", "PARANAL", "--ra", "83.6331",
                     "--dec", "22.0145", "--max-nsb", "0.25", "--components", "moon",
                     "--moonlight-model", moon, "--no-pre-filter"]
        cases.append((f"PARANAL/{moon}/moon-down/hour", arguments))
    with tempfile.TemporaryDirectory(prefix="nsb-planning-cli-") as tmp:
        for label, arguments in cases:
            memory_file = Path(tmp) / "rss.txt"
            warmups = [invoke(binary, arguments, memory_file) for binary in binaries]
            assert warmups[1][2] == warmups[2][2], f"optimized adaptive output differs: {label}"
            times, memory = [[], [], []], [[], [], []]
            for repeat in range(args.repeats):
                for variant in ([0, 1, 2] if repeat % 2 == 0 else [2, 1, 0]):
                    seconds, rss, digest = invoke(binaries[variant], arguments, memory_file)
                    assert digest == warmups[variant][2], f"nondeterministic output: {label}"
                    times[variant].append(seconds)
                    memory[variant].append(rss)
            result = {"scenario": label, "arguments": arguments, "repeats": args.repeats,
                      "rayon_threads": 2, "adaptive_outputs_identical": True}
            for variant, name in enumerate(["scan600", "before", "after"]):
                result[name] = {"seconds": times[variant], "median_seconds": statistics.median(times[variant]),
                                "range_seconds": [min(times[variant]), max(times[variant])],
                                "rss_kib": memory[variant], "output_sha256": warmups[variant][2],
                                "binary_sha256": hashlib.sha256(binaries[variant].read_bytes()).hexdigest()}
            print(json.dumps(result, sort_keys=True), flush=True)


if __name__ == "__main__":
    main()
